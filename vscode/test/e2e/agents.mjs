// The `claude` processes of a scenario in the e2e layer are stubs that run the release
// `wtpool hook` (claude-stub.mjs), so hooks see a live parent and the real clock. The server of
// another harness is a stub of the same kind that a `claude` stub starts, so it descends from it.
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
 * does. A row that lists one actor at a time shows no order, so only a scenario that lists two
 * actors in one row pays the wait.
 */
const ACTOR_GAP_MS = 1100;

/** The documents a scenario asserts: each `expect` step, then the final one. */
function documentsOf(scenario) {
  return [...scenario.steps.filter((step) => step.expect).map((step) => step.expect), scenario.expect];
}

/** Tells a scenario whose documents ever show one row with two actors at once. */
function rowsShareActors(scenario) {
  return documentsOf(scenario).some((document) =>
    document.pools.some((pool) => pool.worktrees.some((worktree) => worktree.actors.length > 1)),
  );
}

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
  const stub = {
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
    /**
     * Starts a stub of the same kind as a child of this one, in `cwd`. Gives its `send`, which
     * has the stub run the request, and `child.kill` and `exited` for its process.
     */
    async spawnChild({ cwd, env: variables }) {
      const started = await stub.send({ spawn: true, cwd, env: variables });
      if (!started.pid) throw new Error(`setup: the stub could not start a child stub: ${started.stderr}`);
      const gone = new Promise((resolve) => {
        const timer = setInterval(() => {
          try {
            process.kill(started.pid, 0);
          } catch {
            clearInterval(timer);
            resolve();
          }
        }, 50);
        timer.unref();
      });
      return {
        pid: started.pid,
        child: {
          kill(signal) {
            try {
              process.kill(started.pid, signal);
            } catch {
              // Already gone, as it is once its parent stub stopped.
            }
          },
        },
        exited: gone,
        send: (request) => stub.send({ ...request, to: started.id }),
      };
    },
  };
  return stub;
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
    this.gapActors = rowsShareActors(scenario);
    this.problems = [];
    this.documentIndex = 0;
  }

  async run(from, to, observe) {
    const { dirs, pool: poolInfo } = this.session;
    for (let index = from; index < to; index += 1) {
      const step = this.scenario.steps[index];
      if (step.process?.start) {
        const { pid, name, parent } = step.process.start;
        assert.ok(!step.process.start.started, 'setup: e2e cannot reuse a pid');
        if (name === 'claude') this.stubs.set(pid, startStub());
        else if (name === 'opencode') {
          const launcher = this.stubs.get(parent);
          assert.ok(launcher, `setup: ${this.name} starts opencode ${pid} under ${parent}, which no step started`);
          this.stubs.set(pid, await launcher.spawnChild({ cwd: folder(step.process.start.in, poolInfo), env: env(dirs, poolInfo) }));
        } else throw new Error(`setup: e2e scenario ${this.name} starts ${name}`);
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
    const other = step.harness !== undefined && step.harness !== 'claude-code';
    const actor = other ? `${step.payload.data.sessionID}/` : `${step.session}/${step.agent ?? ''}`;
    if (this.gapActors && this.lastActor !== null && this.lastActor !== actor) await sleep(ACTOR_GAP_MS);
    this.lastActor = actor;
    const stub = this.stubs.get(step.process);
    assert.ok(stub, `setup: step ${index} runs from claude ${step.process}, which no step started`);
    const variables = env(dirs, poolInfo);
    if (step.home) variables.HOME = folder(step.home, poolInfo);
    const reply = await stub.send({
      binary: releaseWtpool(),
      ...(other ? { args: ['hook', '--harness', step.harness] } : {}),
      payload: payloadOf(this.name, index, poolInfo),
      cwd: folder(step.cwd, poolInfo),
      env: variables,
    });
    if (reply.code !== 0 || reply.stdout) {
      this.problems.push(
        `step ${index}: wtpool hook for ${step.fixture ?? step.payload?.name} exited ${reply.code ?? reply.signal}, stdout ${JSON.stringify(reply.stdout)}, stderr ${JSON.stringify(reply.stderr)}`,
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
