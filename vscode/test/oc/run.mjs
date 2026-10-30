#!/usr/bin/env node
// The oc layer's runner, started by xtask/src/layers/oc.rs. It runs each wanted session on the
// installed `opencode` in the sandbox of sandbox.mjs, with the plugin the build prints, then the
// sandbox check over them all, and writes their outcomes to WTPOOL_VERIFY_OC_RESULTS.
import { execFileSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { startProxy, startSampler } from './network.mjs';
import { AssumptionBroken, Inconclusive } from './opencode.mjs';
import { makeDirs, makePool, openCodeEnv, ownerChanges, ownerFolders, ownerSnapshot, sandboxCredentials, writeWrapper, wtpoolEnv } from './sandbox.mjs';
import { COORDINATOR_SESSIONS } from './sessions-coordinator.mjs';
import { RUN_SESSIONS } from './sessions-run.mjs';
import { SERVE_SESSIONS } from './sessions-serve.mjs';
import { startStubModel } from './stub-model.mjs';

const RESULTS = process.env.WTPOOL_VERIFY_OC_RESULTS;
const WANTED = (process.env.WTPOOL_VERIFY_OC_CHECKS ?? '').split(',').filter(Boolean);
const SANDBOX_CHECK = 'contract__should_run_opencode_in_the_sandbox';
const SESSIONS = [...RUN_SESSIONS, ...SERVE_SESSIONS, ...COORDINATOR_SESSIONS];
/** The longest one session may take, its retries apart. */
const SESSION_LIMIT_MS = 360_000;
const results = [];

function report(check, outcome, { kind = null, code = null, message = '' } = {}) {
  results.push({ check, outcome, kind, code, message: String(message).slice(0, 4000) });
  writeFileSync(RESULTS, JSON.stringify(results, null, 2));
}

function everyOther(outcome, details) {
  for (const check of WANTED) if (!results.some((r) => r.check === check)) report(check, outcome, details);
}

/** The text `wtpool hook install --harness opencode --policy` prints, or a failure. */
function printedPlugin(context) {
  const pool = makePool(context, 'install');
  try {
    const text = execFileSync(context.wtpool, ['hook', 'install', '--harness', 'opencode', '--policy'], {
      cwd: pool.fg,
      encoding: 'utf8',
      env: wtpoolEnv(context, pool),
      timeout: 30_000,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    if (!text.includes('const CONFIG')) return { error: `it printed no plugin: ${JSON.stringify(text.slice(0, 300))}` };
    return { text };
  } catch (error) {
    return { error: `it failed (exit ${error.status}): ${String(error.stderr ?? error.message).slice(0, 600)}` };
  }
}

/** Runs one session within its limit, its retries apart, and reports it. */
async function runSession(context, session) {
  const attempts = session.attempts ?? 1;
  for (let attempt = 1; ; attempt += 1) {
    context.proxy.session = session.name;
    context.ran.push(session.name);
    try {
      const limit = new Promise((_, reject) => setTimeout(() => reject(new Error(`the session ran past ${SESSION_LIMIT_MS / 1000} s`)), SESSION_LIMIT_MS).unref());
      const message = await Promise.race([session.run(context), limit]);
      report(session.name, 'pass', { message });
      return;
    } catch (error) {
      if (error instanceof Inconclusive && attempt < attempts) continue;
      if (error instanceof Inconclusive) report(session.name, 'needs_you', { code: 'inconclusive', message: error.message });
      else if (error instanceof AssumptionBroken) report(session.name, 'needs_you', { code: 'assumption_broken', message: error.message });
      else if (error?.code === 'ERR_ASSERTION') report(session.name, 'fail', { kind: 'assertion', message: error.message });
      else if (/ran past/.test(String(error?.message))) report(session.name, 'fail', { kind: 'timeout', message: error.message });
      else report(session.name, 'fail', { kind: 'error', message: error?.stack ?? String(error) });
      return;
    } finally {
      for (const open of context.open.splice(0)) await open.end().catch(() => {});
      context.proxy.session = null;
    }
  }
}

/** contract__should_run_opencode_in_the_sandbox over the sessions that ran. */
function sandboxResult(context, before, sampler) {
  if (!context.ran.length) {
    report(SANDBOX_CHECK, 'skipped', { message: 'no live OpenCode session ran, so none had to stay in the sandbox' });
    return;
  }
  const problems = [...ownerChanges(before, ownerSnapshot())];
  for (const file of sampler.owner) problems.push(`${file.command} ${file.pid} held ${file.name} open`);
  for (const remote of sampler.remote) problems.push(`${remote.command} ${remote.pid} reached ${remote.host}:${remote.port} around the proxy`);
  for (const asked of context.proxy.asked) {
    const allowed = asked.session === 'oc_real_model' && asked.allowed;
    if (!allowed) problems.push(`${asked.session ?? 'the setup'} asked the proxy for ${asked.host}:${asked.port}`);
  }
  problems.push(...sandboxCredentials(context.dirs));
  const hosts = [...new Set(context.proxy.asked.filter((a) => a.allowed).map((a) => a.host))];
  report(
    SANDBOX_CHECK,
    problems.length ? 'fail' : 'pass',
    problems.length
      ? { kind: 'assertion', message: [...new Set(problems)].join('\n') }
      : { message: `${context.ran.length} session runs, ${sampler.samples} samples; no host but loopback${hosts.length ? ` and ${hosts.join(', ')}` : ''}; no credential` },
  );
}

async function main() {
  mkdirSync(path.dirname(RESULTS), { recursive: true });
  writeFileSync(RESULTS, '[]');
  const before = ownerSnapshot();
  const dirs = makeDirs(process.env.WTPOOL_VERIFY_RUN_DIR);
  const stubLog = path.join(dirs.logs, 'stub-model.jsonl');
  const stub = startStubModel({ log: stubLog });
  const proxy = startProxy();
  const context = {
    dirs,
    stub,
    stubLog,
    proxy,
    stubPort: await stub.ready,
    opencode: process.env.WTPOOL_VERIFY_OC_OPENCODE,
    wtpool: process.env.WTPOOL_VERIFY_OC_WTPOOL,
    open: [],
    ran: [],
  };
  await proxy.ready;
  const sampler = startSampler(Object.values(ownerFolders()));
  try {
    const version = execFileSync(context.opencode, ['--version'], { encoding: 'utf8', env: openCodeEnv(context, null), timeout: 60_000, stdio: ['ignore', 'pipe', 'pipe'] }).trim();
    writeFileSync(path.join(dirs.logs, 'setup.json'), JSON.stringify({ opencode: context.opencode, version, wtpool: context.wtpool, sandbox: dirs.root }, null, 2));
    const printed = printedPlugin(context);
    const sessions = SESSIONS.filter((session) => WANTED.includes(session.name));
    if (printed.error) {
      for (const session of sessions) {
        report(session.name, 'fail', { kind: 'assertion', message: `wtpool hook install --harness opencode --policy: ${printed.error}` });
      }
    } else {
      context.printed = printed.text;
      writeFileSync(path.join(dirs.logs, 'printed-plugin.ts'), printed.text);
      context.wrapper = writeWrapper(dirs, context.wtpool);
      // macOS checks a new executable on its first run, which outlasts a short session.
      execFileSync(context.wrapper, ['--version'], { input: '', env: openCodeEnv(context, null), timeout: 60_000, stdio: ['pipe', 'ignore', 'ignore'] });
      for (const session of sessions) await runSession(context, session);
    }
  } catch (error) {
    everyOther('fail', { kind: 'error', message: `setup: ${error?.stack ?? error}` });
  } finally {
    await sampler.stop();
    if (WANTED.includes(SANDBOX_CHECK) && !results.some((r) => r.check === SANDBOX_CHECK)) sandboxResult(context, before, sampler);
    writeFileSync(path.join(dirs.logs, 'proxy.json'), JSON.stringify(proxy.asked, null, 1));
    writeFileSync(path.join(dirs.logs, 'sampler.json'), JSON.stringify({ samples: sampler.samples, remote: sampler.remote, owner: sampler.owner }, null, 1));
    await stub.close();
    await proxy.close();
    rmSync(dirs.root, { recursive: true, force: true });
    everyOther('fail', { kind: 'error', message: 'the oc runner reported no result for it' });
  }
}

main()
  .catch((error) => everyOther('fail', { kind: 'error', message: error?.stack ?? String(error) }))
  .finally(() => process.exit(0));
