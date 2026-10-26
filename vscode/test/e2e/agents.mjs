// The `claude` processes of a scenario in the e2e layer are stubs that run the release
// `wtpool hook` (claude-stub.mjs), so hooks see a live parent and the real clock.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline';

import { env, releaseWtpool, sleep } from './vscode.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const STUB = path.join(here, 'claude-stub.mjs');
/**
 * Hook steps of different actors run this far apart, so `updated` orders them as the int clock
 * does.
 */
const ACTOR_GAP_MS = 1100;

/** A running stub: send(request) resolves with the hook's reply. */
export function startStub() {
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
  const exited = new Promise((resolve) => child.on('exit', resolve));
  return {
    child,
    exited,
    send(request) {
      next += 1;
      const id = next;
      const reply = new Promise((resolve) => {
        waiting.set(id, resolve);
        setTimeout(() => resolve({ id, code: null, signal: 'timeout', stdout: '', stderr: 'no reply within 10 s' }), 10_000);
      });
      child.stdin.write(`${JSON.stringify({ ...request, id })}\n`);
      return reply;
    },
  };
}

/** The payload of hook step `index` of `scenario` for the pool the harness built. */
function payloadOf(scenario, index, poolInfo) {
  const xtask = process.env.WTPOOL_VERIFY_XTASK;
  if (!xtask) throw new Error('setup: WTPOOL_VERIFY_XTASK names no xtask binary');
  const text = execFileSync(xtask, ['payload', '--scenario', scenario, '--step', String(index), '--dir', poolInfo.dir], {
    encoding: 'utf8',
    timeout: 60_000,
  });
  return JSON.parse(text);
}

/** A folder a step names: a worktree of the pool, or a path with `{pool}`. */
function folder(name, poolInfo) {
  if (name.includes('{') || name.includes('/')) {
    return path.resolve(name.replaceAll('{pool}', poolInfo.dir).replaceAll('{common}', poolInfo.common));
  }
  return path.join(poolInfo.dir, name);
}

/** The files of the pool's wtpool-activity/, the sandbox marker left out. */
export function activityFiles(poolInfo) {
  try {
    return readdirSync(path.join(poolInfo.common, 'wtpool-activity'))
      .filter((name) => name !== '.verify-sandbox')
      .sort();
  } catch {
    return [];
  }
}

/**
 * Runs the steps of a scenario in a session whose VS Code shows the pool. {@link AgentSteps.run}
 * calls `observe(index)` at each expect step with the index of its document. `problems` collects
 * the failures of hook and files steps, as those are the product's.
 */
export class AgentSteps {
  constructor(session, scenarioName, scenario) {
    this.session = session;
    this.name = scenarioName;
    this.scenario = scenario;
    this.stubs = new Map();
    this.lastActor = null;
    this.problems = [];
    this.documentIndex = 0;
  }

  async run(from, to, observe) {
    const { dirs, pool: poolInfo } = this.session;
    for (let index = from; index < to; index += 1) {
      const step = this.scenario.steps[index];
      if (step.process?.start) {
        const { pid, name } = step.process.start;
        assert.ok(!step.process.start.started, 'setup: e2e cannot reuse a pid');
        if (name === 'claude') this.stubs.set(pid, startStub());
        else throw new Error(`setup: e2e scenario ${this.name} starts ${name}`);
      }
      if (step.process?.stop !== undefined) {
        const stub = this.stubs.get(step.process.stop);
        stub?.child.kill('SIGTERM');
        await stub?.exited;
        this.stubs.delete(step.process.stop);
      }
      if (step.clock) throw new Error(`setup: e2e scenario ${this.name} has a clock step`);
      if (step.gc) {
        execFileSync(releaseWtpool(), ['hook', 'gc'], { cwd: poolInfo.fg, env: env(dirs, poolInfo), timeout: 30_000 });
      }
      if (step.hook) await this.hook(step.hook, index);
      if (step.files) {
        const found = activityFiles(poolInfo);
        if (JSON.stringify(found) !== JSON.stringify(step.files)) {
          this.problems.push(`step ${index}: wtpool-activity holds ${JSON.stringify(found)}, not ${JSON.stringify(step.files)}`);
        }
      }
      if (step.expect) {
        await observe(this.documentIndex);
        this.documentIndex += 1;
      }
    }
  }

  async hook(step, index) {
    const { dirs, pool: poolInfo } = this.session;
    const actor = `${step.session}/${step.agent ?? ''}`;
    if (this.lastActor !== null && this.lastActor !== actor) await sleep(ACTOR_GAP_MS);
    this.lastActor = actor;
    const stub = this.stubs.get(step.process);
    assert.ok(stub, `setup: step ${index} runs from claude ${step.process}, which no step started`);
    const variables = env(dirs, poolInfo);
    if (step.home) variables.HOME = folder(step.home, poolInfo);
    const reply = await stub.send({
      binary: releaseWtpool(),
      payload: payloadOf(this.name, index, poolInfo),
      cwd: folder(step.cwd, poolInfo),
      env: variables,
    });
    if (reply.code !== 0 || reply.stdout) {
      this.problems.push(
        `step ${index}: wtpool hook for ${step.fixture} exited ${reply.code ?? reply.signal}, stdout ${JSON.stringify(reply.stdout)}, stderr ${JSON.stringify(reply.stderr)}`,
      );
    }
  }

  async stop() {
    for (const stub of this.stubs.values()) {
      stub.child.kill('SIGTERM');
      await stub.exited;
    }
    this.stubs.clear();
  }
}
