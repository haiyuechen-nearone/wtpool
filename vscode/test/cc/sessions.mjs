// The cc layer's sessions. Each runs a script for claude on a fresh pool of its own, then checks
// Claude Code with expectClaude and wtpool with plain asserts.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { chmodSync, closeSync, constants, existsSync, mkdirSync, openSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { makePool, sessionEnv, wtpoolEnv } from './sandbox.mjs';

export class Inconclusive extends Error {}
export class AssumptionBroken extends Error {}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** A Claude Code assert: its failure is assumption_broken. */
function expectClaude(condition, message) {
  if (!condition) throw new AssumptionBroken(message);
}

function events(recorded) {
  return recorded.map((p) => p.hook_event_name + (p.tool_name ? `:${p.tool_name}` : '') + (p.notification_type ? `:${p.notification_type}` : ''));
}

/** The script's tool calls happened, else the session reruns. */
function expectTools(recorded, tools) {
  const used = recorded.filter((p) => p.hook_event_name === 'PreToolUse').map((p) => p.tool_name);
  for (const tool of tools) {
    if (!used.includes(tool)) throw new Inconclusive(`the script's ${tool} call did not happen: ${JSON.stringify(used)}`);
  }
}

function readLines(file) {
  return existsSync(file) ? readFileSync(file, 'utf8').split('\n').filter(Boolean).map((line) => JSON.parse(line)) : [];
}

const allow = async (toolName, input) => ({ behavior: 'allow', updatedInput: input });

/** `text` as a RegExp source that matches it literally. */
const literal = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** A path as a command names it, bare or quoted. `slash` lets a folder end in one. */
function named(target, slash = '') {
  const bare = `${literal(target)}${slash}`;
  return `(?:${bare}|'${bare}'|"${bare}")`;
}

const namedDir = (dir) => named(dir, '/?');

/** A Bash call whose trimmed command matches one of `patterns`, RegExp sources. */
const bashIs = (...patterns) => (input) => patterns.some((p) => new RegExp(`^(?:${p})$`).test(String(input.command ?? '').trim()));
const fileIs = (target) => (input) => typeof input.file_path === 'string' && path.resolve(input.file_path) === target;
const agentIs = (type) => (input) => (input.subagent_type ?? 'general-purpose') === type;

/**
 * canUseTool for the script of session `name`: `script` maps each scripted tool to a test of
 * its input. A scripted call goes on to `decide`, any other is denied with a message naming it.
 * Calls Claude Code allows by itself, such as a Read inside the pool, never reach it.
 */
function scripted(name, script, decide = allow) {
  return async (toolName, input, options) => {
    if (Object.hasOwn(script, toolName) && script[toolName](input)) return decide(toolName, input, options);
    const call = JSON.stringify(input.command ?? input.file_path ?? input.subagent_type ?? input).slice(0, 300);
    return { behavior: 'deny', message: `wtpool-verify allows only the calls of the ${name} script, and ${toolName} ${call} is not one` };
  };
}

/** One run of one session: its pool, recording and helpers. */
class Run {
  constructor(context, shared, name) {
    this.context = context;
    this.shared = shared;
    this.name = name;
    this.pool = makePool(context.dirs, `${name}-${Date.now()}`);
  }

  wt(name) {
    return path.join(this.pool.dir, name);
  }

  status() {
    const text = execFileSync(this.context.wtpool, ['status', '--json'], {
      cwd: this.pool.fg,
      encoding: 'utf8',
      env: wtpoolEnv(this.context.dirs, this.pool),
      timeout: 30_000,
    });
    return JSON.parse(text);
  }

  /** The actors of the document, each with its worktree's name, or none when wtpool fails. */
  actors() {
    try {
      return this.status().pools[0].worktrees.flatMap((w) => w.actors.map((a) => ({ ...a, worktree: w.name })));
    } catch {
      return [];
    }
  }

  session() {
    return this.actors().find((a) => a.kind === 'session');
  }

  records() {
    const dir = path.join(this.pool.common, 'wtpool-activity');
    try {
      return readdirSync(dir)
        .filter((name) => name.endsWith('.json'))
        .map((name) => JSON.parse(readFileSync(path.join(dir, name), 'utf8')));
    } catch {
      return [];
    }
  }

  recorded() {
    return readLines(this.recording).slice(1);
  }

  /**
   * Starts `prompt` with streaming input, so the claude process lives on after the turn, and
   * reads messages until the turn's result. `onMessage(message, query)` sees each one.
   * {@link Run.end} closes the input and so ends the session.
   */
  async query(prompt, options = {}, onMessage) {
    const folder = path.join(this.context.recordings, `claude-${this.context.version}`);
    mkdirSync(folder, { recursive: true });
    this.recording = path.join(folder, `${this.name}.jsonl`);
    const header = { wt_verify_recording: 1, pool: this.pool.dir, home: process.env.HOME, config: this.context.dirs.config };
    writeFileSync(this.recording, `${JSON.stringify(header)}\n`);
    this.parents = path.join(this.context.dirs.logs, `${this.name}-parents.jsonl`);
    writeFileSync(this.parents, '');
    const env = sessionEnv(this.context.dirs, this.pool, {
      WTPOOL_VERIFY_CC_RECORDING: this.recording,
      WTPOOL_VERIFY_CC_PARENTS: this.parents,
    });
    let finish;
    const finished = new Promise((resolve) => {
      finish = resolve;
    });
    this.finishInput = finish;
    async function* input() {
      yield { type: 'user', message: { role: 'user', content: prompt }, parent_tool_use_id: null };
      await finished;
    }
    this.messages = [];
    this.q = this.context.sdk.query({
      prompt: input(),
      options: {
        ...this.context.options,
        cwd: this.pool.fg,
        additionalDirectories: ['wt1', 'wt2', 'wt3'].map((n) => this.wt(n)),
        env,
        canUseTool: scripted(this.name, {}),
        ...options,
      },
    });
    this.shared.open.push(this);
    let firstResult;
    const resulted = new Promise((resolve) => {
      firstResult = resolve;
    });
    // One pump reads every message for the whole session, so the claude process never stalls
    // on output while the checks wait on wtpool or on background subagents.
    this.pump = (async () => {
      try {
        for await (const message of this.q) {
          this.take(message);
          await onMessage?.(message, this.q);
          if (message.type === 'result') firstResult();
        }
      } finally {
        this.ended = true;
        firstResult();
      }
    })();
    await resulted;
    await this.settle(this.settleMs ?? 180_000);
    const result = this.messages.findLast((m) => m.type === 'result');
    if (result?.is_error && /log ?in|authenticat|credential|oauth/i.test(JSON.stringify(result))) {
      throw new AssumptionBroken(`logged_out: ${JSON.stringify(result).slice(0, 500)}`);
    }
    return this.messages;
  }

  /**
   * Waits until the session is at rest: every subagent that started has stopped (the Agent tool
   * may run them in the background, after the main turn's Stop), the last event ends a turn,
   * and nothing new came for 3 s. At most `limitMs`.
   */
  async settle(limitMs = 180_000) {
    const deadline = Date.now() + limitMs;
    let seen = -1;
    let stableSince = Date.now();
    while (Date.now() < deadline && !this.ended) {
      const recorded = this.recorded();
      if (recorded.length !== seen) {
        seen = recorded.length;
        stableSince = Date.now();
      }
      const started = recorded.filter((p) => p.hook_event_name === 'SubagentStart').length;
      const stopped = recorded.filter((p) => p.hook_event_name === 'SubagentStop').length;
      const last = recorded.at(-1)?.hook_event_name;
      const atRest = started === stopped && ['Stop', 'StopFailure', 'SessionEnd'].includes(last);
      if (atRest && Date.now() - stableSince >= 3_000) return;
      await sleep(500);
    }
  }

  take(message) {
    this.messages.push(message);
    if (message.type === 'system' && message.subtype === 'hook_response' && message.stdout) {
      this.shared.hookOutput.push({ session: this.name, hook: message.hook_name, event: message.hook_event, stdout: message.stdout });
    }
  }

  /** Ends the session: closes its input, reads its last messages, waits for SessionEnd. */
  async end() {
    if (this.closed) return;
    this.closed = true;
    this.finishInput?.();
    await Promise.race([this.pump, sleep(30_000)]);
    await sleep(2_000);
    writeFileSync(path.join(this.context.dirs.logs, `${this.name}-messages.json`), JSON.stringify(this.messages, null, 1));
    this.shared.parents.push(...readLines(this.parents));
    this.shared.records.push(...this.records().map((r) => ({ ...r, run: this.name })));
  }
}

/** canUseTool that holds a permission dialog `holdMs`, then allows or denies. */
function holding(holdMs, { deny = false, interrupt = false } = {}, during = async () => {}) {
  return async (toolName, input) => {
    await during(toolName);
    await sleep(holdMs);
    if (deny) return { behavior: 'deny', message: 'wtpool-verify denies this call', interrupt };
    return { behavior: 'allow', updatedInput: input };
  };
}

async function readEditBash(context, shared) {
  const run = new Run(context, shared, 'cc_read_edit_bash');
  const w = run.wt('wt1');
  const canUseTool = scripted(run.name, { Read: fileIs(`${w}/f.txt`), Edit: fileIs(`${w}/f.txt`), Bash: bashIs(`ls ${namedDir(w)}`) });
  await run.query(`Do exactly three tool calls and nothing else: Read ${w}/f.txt, then Edit ${w}/f.txt replacing "base" with "edited", then run \`ls ${w}\` with Bash.`, { canUseTool });
  const recorded = run.recorded();
  expectTools(recorded, ['Read', 'Edit', 'Bash']);
  for (const [tool, field] of [['Read', 'file_path'], ['Edit', 'file_path'], ['Bash', 'command']]) {
    for (const event of ['PreToolUse', 'PostToolUse']) {
      const payload = recorded.find((p) => p.hook_event_name === event && p.tool_name === tool);
      expectClaude(payload?.tool_input?.[field], `${tool}'s ${event} has no tool_input.${field}: ${JSON.stringify(payload?.tool_input)}`);
    }
  }
  const actor = run.session();
  assert.equal(actor?.worktree, 'wt1', `the session is not on wt1: ${JSON.stringify(actor)}`);
  assert.equal(actor?.state, 'idle', 'the session is not idle after Stop');
  return `events ${events(recorded).join(', ')}`;
}

async function subagentElsewhere(context, shared) {
  const run = new Run(context, shared, 'cc_subagent_elsewhere');
  let seen;
  const canUseTool = scripted(run.name, { Agent: agentIs('Explore'), Read: fileIs(`${run.wt('wt2')}/f.txt`) });
  const prompt = `Use the Agent tool once, subagent type Explore, description "read wt2", to read ${run.wt('wt2')}/f.txt and report its first line. Do nothing else.`;
  await run.query(prompt, { canUseTool }, async () => {
    seen ??= run.actors().find((a) => a.kind === 'subagent' && a.worktree === 'wt2');
  });
  const recorded = run.recorded();
  expectTools(recorded, ['Agent']);
  const sub = recorded.find((p) => p.agent_id && p.hook_event_name === 'PreToolUse');
  expectClaude(sub?.agent_id && sub?.agent_type, `no subagent PreToolUse with agent_id and agent_type: ${JSON.stringify(events(recorded))}`);
  const agent = recorded.find((p) => p.tool_name === 'Agent');
  assert.ok(seen, 'the subagent never showed on wt2');
  return `Agent description ${JSON.stringify(agent?.tool_input?.description)}; agent_type ${sub.agent_type}`;
}

async function subagentPermission(context, shared) {
  const run = new Run(context, shared, 'cc_subagent_permission');
  let waits = [];
  const script = { Agent: agentIs('general-purpose'), Bash: bashIs(`cd ${namedDir(run.wt('wt3'))} && git status`) };
  await run.query(
    `Use the Agent tool once, subagent type general-purpose, to run \`cd ${run.wt('wt3')} && git status\` with Bash and report the result. Do nothing else.`,
    {
      canUseTool: scripted(
        run.name,
        script,
        holding(5_000, {}, async (tool) => {
          if (tool !== 'Bash') return;
          await sleep(5_000);
          waits = run.actors().filter((a) => a.state === 'waiting');
        }),
      ),
    },
  );
  const recorded = run.recorded();
  expectTools(recorded, ['Agent']);
  const request = recorded.find((p) => p.hook_event_name === 'PermissionRequest');
  expectClaude(request, `no PermissionRequest: ${events(recorded).join(', ')}`);
  expectClaude(request.agent_id, `the PermissionRequest has no agent_id: ${JSON.stringify(Object.keys(request))}`);
  assert.deepEqual(waits.map((a) => a.kind), ['subagent'], `who waits during the hold: ${JSON.stringify(waits)}`);
  return `permission_prompt ${recorded.some((p) => p.notification_type === 'permission_prompt') ? 'recorded' : 'absent'}`;
}

async function cdIntoSlot(context, shared) {
  const run = new Run(context, shared, 'cc_cd_into_slot');
  const canUseTool = scripted(run.name, { Bash: bashIs(`cd ${namedDir(run.wt('wt2'))} && ls`, 'pwd') });
  await run.query(`Run \`cd ${run.wt('wt2')} && ls\` with Bash, then run \`pwd\` with Bash. Nothing else.`, { canUseTool });
  const recorded = run.recorded();
  expectTools(recorded, ['Bash']);
  const actor = run.session();
  assert.equal(actor?.worktree, 'wt2', `the session is not on wt2: ${JSON.stringify(actor)}`);
  const changed = recorded.find((p) => p.hook_event_name === 'CwdChanged');
  return `CwdChanged ${changed ? JSON.stringify(changed.new_cwd) : 'absent'}; later cwd ${recorded.at(-1)?.cwd}`;
}

function permissionSession(name) {
  return async (context, shared) => {
    const run = new Run(context, shared, name);
    // An interrupt sends no Stop, so the session never comes to rest by itself.
    if (name === 'cc_permission_deny_interrupt') run.settleMs = 5_000;
    const deny = name !== 'cc_permission';
    const interrupt = name === 'cc_permission_deny_interrupt';
    const hold = name === 'cc_permission' ? 70_000 : 5_000;
    const samples = [];
    let requestDuringHold = false;
    const held = holding(hold, { deny, interrupt }, async () => {
      const sampler = setInterval(() => samples.push(run.session()?.state ?? null), 5_000);
      setTimeout(() => {
        clearInterval(sampler);
        requestDuringHold = run.recorded().some((p) => p.hook_event_name === 'PermissionRequest');
      }, Math.max(hold - 500, 1_000));
    });
    const canUseTool = scripted(name, { Bash: bashIs(`cd ${namedDir(run.wt('wt3'))} && git status`) }, held);
    await run.query(`Run \`cd ${run.wt('wt3')} && git status\` with Bash. Nothing else.`, { canUseTool });
    const recorded = run.recorded();
    expectTools(recorded, ['Bash']);
    expectClaude(requestDuringHold, `no PermissionRequest during the hold: ${events(recorded).join(', ')}`);
    if (name === 'cc_permission') {
      assert.ok(samples.length && samples.every((s) => s === 'waiting'), `the session was not waiting all along: ${JSON.stringify(samples)}`);
      return `event order ${events(recorded).join(', ')}`;
    }
    if (name === 'cc_permission_deny') {
      const batch = recorded.find((p) => p.hook_event_name === 'PostToolBatch');
      expectClaude(batch?.tool_calls?.length, `no PostToolBatch lists the denied call: ${events(recorded).join(', ')}`);
      const actor = run.session();
      assert.ok(actor, 'the session shows in no worktree');
      assert.notEqual(actor.state, 'waiting', 'the wait did not end at the PostToolBatch');
      return `PostToolUseFailure ${recorded.some((p) => p.hook_event_name === 'PostToolUseFailure') ? 'recorded' : 'absent'}; ${events(recorded).join(', ')}`;
    }
    const stopAt = recorded.findIndex((p) => p.hook_event_name === 'Stop');
    expectClaude(stopAt < 0, `a Stop came after the interrupt: ${events(recorded).join(', ')}`);
    const waitingAfter = run.session()?.state;
    await sleep(70_000);
    const later = run.recorded();
    const idlePromptCame = later.some((p) => p.notification_type === 'idle_prompt');
    const state = run.session()?.state;
    assert.equal(waitingAfter, 'waiting', 'the session left waiting before idle_prompt');
    // A wait never decays: without an idle_prompt the session must still wait.
    assert.equal(state, idlePromptCame ? 'idle' : 'waiting', `70 s after the denial, idle_prompt ${idlePromptCame ? 'came' : 'did not come'}`);
    return `idle_prompt ${idlePromptCame ? 'recorded' : 'absent'}; ${events(later).join(', ')}`;
  };
}

async function interruptSession(context, shared) {
  const run = new Run(context, shared, 'cc_interrupt');
  run.settleMs = 5_000;
  const dir = path.join(run.wt('wt2'), '.verify');
  mkdirSync(dir, { recursive: true });
  const fifo = path.join(dir, 'hold.fifo');
  execFileSync('mkfifo', [fifo]);
  const script = path.join(dir, 'block.sh');
  writeFileSync(script, `#!/bin/sh\ncat '${fifo}'\n`);
  chmodSync(script, 0o755);
  let interrupted = false;
  const watcher = setInterval(async () => {
    if (interrupted || !run.recording) return;
    if (run.recorded().some((p) => p.hook_event_name === 'PreToolUse' && p.tool_name === 'Bash')) {
      interrupted = true;
      await sleep(2_000);
      await run.q?.interrupt();
    }
  }, 500);
  // A plain interrupt with no wait: the session's settings allow exactly this script, so Claude
  // Code sends no PermissionRequest, as one would leave a wait that never decays.
  const settings = JSON.parse(readFileSync(context.options.settings, 'utf8'));
  settings.permissions = { ...settings.permissions, allow: [`Bash(${script})`] };
  const sessionSettings = path.join(context.dirs.root, 'settings-cc_interrupt.json');
  writeFileSync(sessionSettings, JSON.stringify(settings, null, 2));
  try {
    const canUseTool = scripted(run.name, { Bash: bashIs(named(script)) });
    await run.query(`Run \`${script}\` with Bash and wait for it to finish. Nothing else.`, { settings: sessionSettings, canUseTool });
  } finally {
    clearInterval(watcher);
    unblock(fifo);
  }
  const recorded = run.recorded();
  expectTools(recorded, ['Bash']);
  expectClaude(
    !recorded.some((p) => p.hook_event_name === 'PermissionRequest'),
    `the allowed script still asked for permission: ${events(recorded).join(', ')}`,
  );
  await sleep(10_000);
  const after = run.recorded();
  expectClaude(!after.some((p) => p.hook_event_name === 'Stop'), `a Stop came within 10 s of the interrupt: ${events(after).join(', ')}`);
  await sleep(115_000);
  assert.equal(run.session()?.state, 'quiet', 'the session is not quiet 125 s after the interrupt');
  const failure = after.find((p) => p.hook_event_name === 'PostToolUseFailure');
  return `PostToolUseFailure ${failure ? `is_interrupt ${JSON.stringify(failure.is_interrupt)}` : 'absent'}; ${events(after).join(', ')}`;
}

/** Gives a reader of `fifo` its end of file, so no `cat` the session left blocks. */
function unblock(fifo) {
  try {
    closeSync(openSync(fifo, constants.O_WRONLY | constants.O_NONBLOCK));
  } catch {
    // No reader is left.
  }
}

async function idlePrompt(context, shared) {
  const run = new Run(context, shared, 'cc_idle_prompt');
  await run.query('Reply with the single word ok. Use no tool.');
  const states = [];
  for (let n = 0; n < 14; n += 1) {
    await sleep(5_000);
    states.push(run.session()?.state ?? null);
  }
  assert.ok(!states.includes('waiting'), `the session showed waiting: ${states}`);
  assert.equal(states.at(-1), 'idle', `the session is not idle: ${states}`);
  return `idle_prompt ${run.recorded().some((p) => p.notification_type === 'idle_prompt') ? 'recorded' : 'absent'}`;
}

async function stopFailure(context, shared) {
  const run = new Run(context, shared, 'cc_stop_failure');
  await run.query('Reply with ok.', { model: 'claude-wtpool-verify-unknown-model' });
  const recorded = run.recorded();
  const failure = recorded.find((p) => p.hook_event_name === 'StopFailure');
  const refused = run.messages.some((m) => m.type === 'result' && m.is_error);
  expectClaude(failure || refused, `neither a StopFailure nor a refusal: ${events(recorded).join(', ')}`);
  if (failure) {
    const actor = run.session();
    assert.equal(actor?.state, 'idle', 'the session is not idle after StopFailure');
    assert.ok(actor?.error, 'the idle session shows no error');
  }
  return failure ? `StopFailure ${JSON.stringify(failure.error ?? null)}` : 'a refusal';
}

async function sessionEnd(context, shared) {
  const run = new Run(context, shared, 'cc_session_end');
  const canUseTool = scripted(run.name, { Agent: agentIs('Explore'), Read: fileIs(`${run.wt('wt1')}/f.txt`) });
  await run.query(`Use the Agent tool once, subagent type Explore, to read ${run.wt('wt1')}/f.txt. Then stop.`, { canUseTool });
  await run.end();
  const recorded = run.recorded();
  expectTools(recorded, ['Agent']);
  expectClaude(recorded.some((p) => p.hook_event_name === 'SessionEnd'), `no SessionEnd when the session ended: ${events(recorded).join(', ')}`);
  const sessionId = recorded[0]?.session_id;
  const records = run.records().filter((r) => r.session_id === sessionId);
  assert.ok(records.some((r) => r.state === 'ended' && !r.agent_id), 'the session record is not ended');
  assert.ok(!records.some((r) => r.agent_id), 'a subagent record is left');
  return 'ended, subagent gone';
}

export const SESSIONS = [
  { name: 'cc_read_edit_bash', run: readEditBash },
  { name: 'cc_subagent_elsewhere', run: subagentElsewhere },
  { name: 'cc_subagent_permission', run: subagentPermission },
  { name: 'cc_cd_into_slot', run: cdIntoSlot },
  { name: 'cc_permission', run: permissionSession('cc_permission') },
  { name: 'cc_permission_deny', run: permissionSession('cc_permission_deny') },
  { name: 'cc_permission_deny_interrupt', run: permissionSession('cc_permission_deny_interrupt') },
  { name: 'cc_interrupt', run: interruptSession },
  { name: 'cc_idle_prompt', run: idlePrompt },
  { name: 'cc_stop_failure', run: stopFailure },
  { name: 'cc_session_end', run: sessionEnd },
];
