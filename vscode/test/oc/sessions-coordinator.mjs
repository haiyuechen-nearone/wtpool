// The oc layer's coordinator session: a stand-in `claude` with a Claude Code record parks a branch
// in a slot and starts `opencode run --standalone` there as its child; wtpool links the launched
// session to it, keeps the coordinator where it works, and guards the slot while the agent runs.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

import { sleep } from '../support/processes.mjs';
import { Session, expectOpenCode } from './opencode.mjs';
import { wtpoolEnv } from './sandbox.mjs';
import { call, slow } from './stub-model.mjs';

const STUB = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'e2e', 'claude-stub.mjs');

/** A stand-in `claude` that runs each request as its child (e2e/claude-stub.mjs). */
function startClaude() {
  const child = spawn(process.execPath, [STUB], { stdio: ['pipe', 'pipe', 'ignore'] });
  const waiting = new Map();
  let next = 0;
  createInterface({ input: child.stdout }).on('line', (line) => {
    try {
      const reply = JSON.parse(line);
      waiting.get(reply.id)?.(reply);
      waiting.delete(reply.id);
    } catch {
      // Not a reply.
    }
  });
  return {
    pid: child.pid,
    send(request) {
      next += 1;
      const id = next;
      const reply = new Promise((resolve) => {
        waiting.set(id, resolve);
        setTimeout(() => resolve({ id, code: null, signal: 'timeout', stdout: '', stderr: 'no reply within 30 s' }), 30_000);
      });
      child.stdin.write(`${JSON.stringify({ ...request, id })}\n`);
      return reply;
    },
    stop() {
      child.stdin.end();
      child.kill();
    },
  };
}

/** A Claude Code hook payload of session `sessionId` in `cwd`. */
function claudePayload(sessionId, cwd, event, extra = {}) {
  return {
    session_id: sessionId,
    transcript_path: `/verify/.claude/projects/-verify-pool/${sessionId}.jsonl`,
    cwd,
    permission_mode: 'default',
    hook_event_name: event,
    ...extra,
  };
}

async function coordinator(context) {
  const s = new Session(context, 'oc_coordinator');
  s.config();
  const env = wtpoolEnv(context, s.pool);
  const fg = s.pool.fg;
  const claude = startClaude();
  const other = startClaude();
  try {
    const hook = (stub, payload, args) => stub.send({ binary: context.wtpool, args, payload, cwd: payload.cwd, env });
    await hook(claude, claudePayload('s1', fg, 'UserPromptSubmit', { prompt: 'Start an OpenCode agent in a slot.' }));
    await hook(other, claudePayload('s2', fg, 'UserPromptSubmit', { prompt: 'Look around.' }));
    execFileSync('git', ['-C', fg, 'branch', 'oc-task'], { env, timeout: 30_000 });
    const parked = await claude.send({ binary: context.wtpool, args: ['bg', 'oc-task'], payload: {}, cwd: fg, env });
    assert.equal(parked.code, 0, `wtpool bg oc-task failed: ${parked.stderr}`);
    const slot = parked.stdout.trim().split('\n').at(-1);
    const slotName = path.basename(slot);
    context.stub.scripts.oc_coordinator = [call('read', { path: 'f.txt' }), slow(80, 100)];
    const command = `cd ${slot} && opencode run --standalone --format json "[[scenario:oc_coordinator]] Read f.txt and count slowly."`;
    await hook(claude, claudePayload('s1', fg, 'PreToolUse', { tool_name: 'Bash', tool_input: { command, description: 'Start the agent', run_in_background: true }, tool_use_id: 'toolu_01Launch' }));
    const launched = await claude.send({ binary: '/bin/sh', args: ['-c', `${command} > ${path.join(s.dir, 'run.stdout')} 2> ${path.join(s.dir, 'run.stderr')} < /dev/null`], cwd: fg, env: s.env(), detach: true });
    assert.ok(launched.pid, `the coordinator could not start the run: ${launched.stderr}`);
    const deadline = Date.now() + 60_000;
    let agent;
    while (Date.now() < deadline && !agent) {
      agent = s.actors().find((a) => a.harness === 'opencode' && a.kind === 'session' && a.worktree === slotName);
      if (!agent) await sleep(250);
    }
    assert.ok(agent, `the OpenCode session never showed in ${slotName}`);
    const actors = s.actors();
    const coordinatorActor = actors.find((a) => a.harness === 'claude-code' && a.key === 's1');
    assert.ok(coordinatorActor, `no coordinator actor: ${JSON.stringify(actors.map((a) => [a.key, a.worktree]))}`);
    assert.equal(agent.launched_by, coordinatorActor.key, "the agent's launched_by");
    assert.equal(coordinatorActor.worktree, 'fg', 'the coordinator moved');
    assert.equal(coordinatorActor.action?.target, 'opencode', "the coordinator's action");
    const refused = await other.send({ binary: context.wtpool, args: ['free', slotName], payload: {}, cwd: fg, env });
    assert.notEqual(refused.code, 0, `another session's wtpool free ${slotName} went on while the agent ran`);
    const words = `${refused.stdout}\n${refused.stderr}`;
    assert.ok(words.includes(agent.label) || words.includes(agent.key), `the refusal does not name the agent ${agent.key}: ${words.slice(0, 300)}`);
    const ended = Date.now() + 90_000;
    while (Date.now() < ended && s.actors().some((a) => a.key === agent.key)) await sleep(500);
    assert.ok(!s.actors().some((a) => a.key === agent.key), "the agent's line did not leave the slot once its run ended");
    const lines = s.recorded();
    expectOpenCode(lines.some((l) => l.kind === 'event' && l.name === 'session.execution.succeeded'), "the run's turn did not end");
    const freed = await other.send({ binary: context.wtpool, args: ['free', slotName], payload: {}, cwd: fg, env });
    assert.equal(freed.code, 0, `wtpool free ${slotName} still refused after the run: ${freed.stderr}`);
    return `the agent ${agent.key} in ${slotName}, launched by ${agent.launched_by}`;
  } finally {
    claude.stop();
    other.stop();
  }
}

export const COORDINATOR_SESSIONS = [{ name: 'oc_coordinator', run: coordinator }];
