// The processes of the heavy job flows: stand in commands that hold their place until released,
// jobs, stub agents and the wtpool wrappers.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { availableParallelism } from 'node:os';
import path from 'node:path';

import { runDir } from '../support/paths.mjs';
import { startStub } from './agents.mjs';
import { payload } from './cli.mjs';
import { poll, worktreePath } from './flows.mjs';
import { readText } from './jobs-config.mjs';
import { env, releaseWtpool, sleep, statusJson } from './vscode.mjs';

/**
 * A change shows within 1 s. The wait adds the watch's 200 ms merge and polling, and fails at that
 * deadline.
 */
export const WITHIN_A_SECOND_MS = 3_000;

/** The default heavy job limit on this machine. */
export function defaultLimit() {
  return Math.min(8, Math.max(1, Math.floor(availableParallelism() / 4)));
}

/** The folder blocking commands mark their start in and wait for their release in. */
export function holdDir(dirs) {
  const dir = path.join(dirs.root, 'hold');
  mkdirSync(dir, { recursive: true });
  return dir;
}

/**
 * A stand in for a heavy command called `name` in `dirs.bin`: it marks `<pid>.started`, waits
 * until `<pid>.go` or `all.go` appears, or the folder goes, marks `<pid>.ended` and exits 0. A
 * shell script, so the process table reads `sh <bin>/<name> <args>`, and wtpool matches its words
 * as `<name> <args>`.
 */
export function heavyCommand(dirs, name) {
  const file = path.join(dirs.bin, name);
  const hold = holdDir(dirs);
  writeFileSync(
    file,
    [
      '#!/bin/sh',
      `hold='${hold}'`,
      'echo "$0 $*" > "$hold/$$.started"',
      // A run folder removed under it, as a retry does, ends it too.
      'while [ -d "$hold" ] && [ ! -e "$hold/$$.go" ] && [ ! -e "$hold/all.go" ]; do sleep 0.1; done',
      'touch "$hold/$$.ended" 2>/dev/null',
      'exit 0',
      '',
    ].join('\n'),
  );
  chmodSync(file, 0o755);
  return file;
}

/** The commands that started and have not ended, oldest first: [{pid, line}]. */
export function runningCommands(dirs) {
  const hold = holdDir(dirs);
  return readdirSync(hold)
    .filter((name) => name.endsWith('.started') && !existsSync(path.join(hold, name.replace('.started', '.ended'))))
    .map((name) => ({ pid: Number(name.split('.')[0]), line: readFileSync(path.join(hold, name), 'utf8').trim(), at: statSync(path.join(hold, name)).mtimeMs }))
    .sort((a, b) => a.at - b.at);
}

/** The number of commands started so far. */
export function startedCount(dirs) {
  return readdirSync(holdDir(dirs)).filter((name) => name.endsWith('.started')).length;
}

/** Ends the running command `pid`, and resolves once it marked its end. */
export async function endCommand(dirs, pid) {
  const hold = holdDir(dirs);
  writeFileSync(path.join(hold, `${pid}.go`), '');
  await poll(() => existsSync(path.join(hold, `${pid}.ended`)), 5_000, 50);
}

/** The environment of a job: the extension's, with the stand ins first on PATH. */
export function jobEnv(session, extra = {}) {
  const variables = { ...env(session.dirs, session.pool), ...extra };
  variables.PATH = `${session.dirs.bin}:${variables.PATH}`;
  return variables;
}

/**
 * Starts `wtpool job [--label <label>] -- <args>` in `cwd` with the release wtpool. Returns the
 * child, its stderr collected in `child.err`.
 */
export function startJob(session, args, { cwd = session.pool.fg, label } = {}) {
  const argv = ['job', ...(label ? ['--label', label] : []), '--', ...args];
  const child = spawn(releaseWtpool(), argv, { cwd, env: jobEnv(session), stdio: ['ignore', 'ignore', 'pipe'] });
  child.err = '';
  child.stderr.on('data', (chunk) => {
    child.err += String(chunk);
  });
  child.exitCode_ = null;
  child.on('exit', (code, signal) => {
    child.exitCode_ = code ?? signal;
  });
  child.on('error', () => {});
  session.jobs ??= [];
  session.jobs.push(child);
  return child;
}

/**
 * Waits until `count` commands run, at most `timeoutMs`. Fails, as the product's failure, when
 * they did not start: a job that exits at once names its stderr.
 */
export async function assertRunning(session, count, what, timeoutMs = 10_000) {
  const ok = await poll(() => runningCommands(session.dirs).length === count, timeoutMs, 100);
  const exited = (session.jobs ?? []).filter((job) => job.exitCode_ !== null).map((job) => `${job.spawnargs.slice(1).join(' ')} exited ${job.exitCode_}: ${job.err.trim()}`);
  assert.ok(ok, `${what}: ${runningCommands(session.dirs).length} of ${count} commands run under wtpool job (J1, J10)${exited.length ? `; ${exited.join('; ')}` : ''}`);
}

/** Lets every stand in end and stops every job and stub the flow started. */
export async function stopJobs(session) {
  writeFileSync(path.join(holdDir(session.dirs), 'all.go'), '');
  await sleep(300);
  for (const job of session.jobs ?? []) {
    try {
      job.kill('SIGTERM');
    } catch {
      // Already gone.
    }
  }
  for (const stub of session.stubs ?? []) await stub.stop();
}

/** `wtpool <args>` of the release wtpool in the pool's foreground, the extension's environment. */
export function wtpoolRun(session, args, { cwd = session.pool.fg } = {}) {
  const ran = spawnSync(releaseWtpool(), args, { cwd, env: jobEnv(session), encoding: 'utf8', timeout: 30_000 });
  if (ran.error) throw new Error(`setup: wtpool ${args.join(' ')}: ${ran.error.message}`);
  return { code: ran.status, stdout: ran.stdout, stderr: ran.stderr };
}

/** The repository the pool's heading names, and the machine, from `wtpool status --json`. */
export function names(session) {
  const status = statusJson(session.dirs, session.pool);
  const pool = status.pools[0];
  return { machine: status.machine.name, repository: pool.repository.github ?? pool.repository.name, status };
}

/**
 * A wrapper `wtpool` for `wtpool.path` that notes each argument array, tab separated, one line per
 * run, then runs the release wtpool with it: {file, log}. It lives outside the sandbox, as
 * `sandbox` makes that afresh, so the settings can name it before VS Code starts. Each attempt
 * starts with an empty log.
 */
export function loggingWtpool(key) {
  const dir = path.join(runDir(), 'e2e', 'wrappers', key);
  mkdirSync(dir, { recursive: true });
  const file = path.join(dir, 'wtpool');
  const log = path.join(dir, 'wtpool-argv.txt');
  rmSync(log, { force: true });
  writeFileSync(file, ['#!/bin/sh', `printf '%s\\t' "$@" >> '${log}'`, `printf '\\n' >> '${log}'`, `exec '${releaseWtpool()}' "$@"`, ''].join('\n'));
  chmodSync(file, 0o755);
  return { file, log };
}

/** The argument arrays the logging wtpool ran, but the watch, in order. */
export function loggedRuns(log) {
  const text = readText(log) ?? '';
  return text
    .split('\n')
    .filter(Boolean)
    .map((line) => line.replace(/\t$/, '').split('\t'))
    .filter((argv) => argv[0] !== 'status');
}

/** A stub agent: a live `claude` stand in whose hooks and jobs run as its children. */
export function agent(session, { id, title, cwd }) {
  const stub = startStub();
  const base = { session_id: id, cwd, session_title: title };
  let calls = 0;
  const runHook = async (args, body) => {
    const reply = await stub.send({ binary: releaseWtpool(), args, payload: body, cwd: body.cwd, env: jobEnv(session) });
    return reply;
  };
  const handle = {
    stub,
    /** Runs `wtpool hook` with fixture `name`, overrides merged in. A failing hook fails setup. */
    async hook(name, overrides = {}) {
      const reply = await runHook(['hook'], { ...payload(name, session.pool), ...base, ...overrides });
      if (reply.code !== 0 || reply.stdout) throw new Error(`setup: wtpool hook ${name} exited ${reply.code ?? reply.signal}, stdout ${reply.stdout}, stderr ${reply.stderr}`);
    },
    /** A Bash call through `wtpool hook policy`, then `wtpool hook`. Gives the policy's reply. */
    async bash(command) {
      calls += 1;
      const body = { ...payload('PreToolUse-Bash', session.pool), ...base, tool_input: { command, description: 'wtpool-verify' }, tool_use_id: `toolu_${id}_${calls}` };
      const policy = await runHook(['hook', 'policy'], body);
      const record = await runHook(['hook'], body);
      if (record.code !== 0 || record.stdout) throw new Error(`setup: wtpool hook for Bash exited ${record.code ?? record.signal}: ${record.stderr}`);
      return { policy, toolUseId: body.tool_use_id };
    },
    /** Starts `wtpool job -- <args>` as the agent's own child, as its Bash call would. */
    async job(args, { cwd: where = cwd, label } = {}) {
      const argv = ['job', ...(label ? ['--label', label] : []), '--', ...args];
      const reply = await stub.send({ binary: releaseWtpool(), args: argv, cwd: where, env: jobEnv(session), detach: true });
      if (!reply.pid) throw new Error(`setup: the stub could not start wtpool job: ${reply.stderr}`);
      session.pids ??= [];
      session.pids.push(reply.pid);
      return reply.pid;
    },
    async stop() {
      for (const pid of session.pids ?? []) {
        try {
          process.kill(pid, 'SIGTERM');
        } catch {
          // Already gone.
        }
      }
      stub.child.kill('SIGTERM');
      await stub.exited;
    },
  };
  session.stubs ??= [];
  session.stubs.push(handle);
  return handle;
}

/**
 * Four jobs running and two waiting under limit 4. Running: `cargo nextest` in wt1, `npm run build`
 * in wt3, `cargo xtask verify` labelled `verify` and `cargo build` outside the pool. Waiting:
 * `cargo test` in wt1, of the agent `API cleanup` when `withAgent`, else of no agent, then
 * `cargo clippy` in fg.
 */
export async function fourRunTwoWait(session, { withAgent = false } = {}) {
  const { dirs } = session;
  for (const name of ['cargo', 'npm']) heavyCommand(dirs, name);
  const wt1 = worktreePath(session, 'wt1');
  const outside = path.join(dirs.root, 'outside');
  mkdirSync(outside, { recursive: true });
  startJob(session, ['cargo', 'nextest', '--no-fail-fast'], { cwd: wt1 });
  startJob(session, ['npm', 'run', 'build'], { cwd: worktreePath(session, 'wt3') });
  startJob(session, ['cargo', 'xtask', 'verify'], { cwd: outside, label: 'verify' });
  startJob(session, ['cargo', 'build'], { cwd: outside });
  await assertRunning(session, 4, 'four jobs under limit 4');
  let designer = null;
  if (withAgent) {
    designer = agent(session, { id: 'design', title: 'API cleanup', cwd: wt1 });
    await designer.hook('UserPromptSubmit');
    await designer.bash(`cd ${wt1} && wtpool job -- cargo test`);
    await designer.job(['cargo', 'test'], { cwd: wt1 });
  } else {
    startJob(session, ['cargo', 'test'], { cwd: wt1 });
  }
  await sleep(500);
  startJob(session, ['cargo', 'clippy'], { cwd: session.pool.fg });
  await sleep(1500);
  await assertRunning(session, 4, 'limit 4 with six jobs: the fifth and sixth must wait (J10)', 1_000);
  return { designer, wt1 };
}
