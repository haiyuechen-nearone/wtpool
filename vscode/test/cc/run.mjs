#!/usr/bin/env node
// The cc layer's runner, started by xtask/src/layers/cc.rs. It runs the contract check and each
// wanted session on the installed `claude`, and writes their outcomes to WTPOOL_VERIFY_CC_RESULTS.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import {
  MODEL,
  installChanges,
  installSnapshot,
  makePool,
  makeRunDirs,
  ownerTraces,
  plantCanaries,
  removeOwnTranscripts,
  removeTempFolders,
  sessionEnv,
  transcriptFolders,
  writeSettings,
} from './sandbox.mjs';
import { AssumptionBroken, Inconclusive, SESSIONS } from './sessions.mjs';

const RESULTS = process.env.WTPOOL_VERIFY_CC_RESULTS;
const WANTED = (process.env.WTPOOL_VERIFY_CC_CHECKS ?? '').split(',').filter(Boolean);
const SDK = '@anthropic-ai/claude-agent-sdk';
const results = [];

function report(check, outcome, { kind = null, code = null, message = '' } = {}) {
  results.push({ check, outcome, kind, code, message: String(message).slice(0, 4000) });
  writeFileSync(RESULTS, JSON.stringify(results, null, 2));
}

/** Turns the reported `check` into a fail that names `problem` first. */
function failAfter(check, problem) {
  const result = results.find((r) => r.check === check);
  if (!result) return;
  Object.assign(result, { outcome: 'fail', kind: 'assertion', code: null });
  result.message = `${problem}\n${result.message}`.slice(0, 4000);
  writeFileSync(RESULTS, JSON.stringify(results, null, 2));
}

function everyOther(outcome, details) {
  for (const check of WANTED) {
    if (!results.some((r) => r.check === check)) report(check, outcome, details);
  }
}

function installedClaude() {
  return execFileSync('/usr/bin/which', ['claude'], { encoding: 'utf8' }).trim();
}

/** contract__should_run_in_the_sandbox: one auto mode turn loads only the run's settings. */
async function contract(sdk, context) {
  const pool = makePool(context.dirs, 'contract');
  const canaries = plantCanaries(context.dirs, pool);
  const recording = path.join(context.dirs.logs, 'contract.jsonl');
  const messages = [];
  const q = sdk.query({
    prompt: 'Reply with the single word ready. Use no tool.',
    options: {
      ...context.options,
      cwd: pool.fg,
      permissionMode: 'auto',
      // Auto mode runs the calls its classifier approves without canUseTool: the turn needs none.
      tools: [],
      env: sessionEnv(context.dirs, pool, { WTPOOL_VERIFY_CC_RECORDING: recording }),
    },
  });
  for await (const message of q) messages.push(message);
  writeFileSync(path.join(context.dirs.logs, 'contract-messages.json'), JSON.stringify(messages, null, 1));
  const result = messages.findLast((m) => m.type === 'result');
  if (result?.is_error && /log ?in|authenticat|credential|oauth/i.test(JSON.stringify(result))) {
    return { outcome: 'needs_you', code: 'logged_out', message: JSON.stringify(result).slice(0, 500) };
  }
  const init = messages.find((m) => m.type === 'system' && m.subtype === 'init');
  const recorded = existsSync(recording)
    ? readFileSync(recording, 'utf8').split('\n').filter(Boolean).map((line) => JSON.parse(line))
    : [];
  const events = recorded.map((p) => p.hook_event_name);
  const problems = [];
  if (!events.includes('UserPromptSubmit') || !events.includes('Stop')) {
    problems.push(`the run's recorder hook saw ${JSON.stringify(events)}: the run's settings did not load`);
  }
  const loaded = canaries.filter((file) => existsSync(file)).map((file) => path.basename(file));
  if (loaded.length) problems.push(`the session loaded project or local settings: ${loaded.join(', ')}`);
  const foreignServers = (init?.mcp_servers ?? []).filter((s) => ['user', 'project', 'local'].includes(s.source));
  if (foreignServers.length) problems.push(`MCP servers from user, project or local settings: ${JSON.stringify(foreignServers)}`);
  const started = messages.filter((m) => m.type === 'system' && m.subtype === 'hook_started');
  const perEvent = new Map();
  for (const m of started) perEvent.set(m.hook_event, (perEvent.get(m.hook_event) ?? 0) + 1);
  const crowded = [...perEvent].filter(([, count]) => count > 2);
  if (crowded.length) problems.push(`more hooks ran than the run's settings hold: ${JSON.stringify(crowded)}`);
  if (init?.permissionMode !== 'auto') problems.push(`the turn ran in ${init?.permissionMode}, not auto`);
  if (init?.claude_code_version !== context.version) {
    problems.push(`the session ran Claude Code ${init?.claude_code_version}, not the installed ${context.version}`);
  }
  const traces = init?.session_id ? ownerTraces(init.session_id, process.env.WTPOOL_VERIFY_RUN_DIR) : [];
  if (traces.length) problems.push(`the session wrote into the owner's Claude Code configuration: ${traces.join(', ')}`);
  return problems.length
    ? { outcome: 'fail', kind: 'assertion', message: problems.join('\n') }
    : { outcome: 'pass', message: `one auto mode turn on Claude Code ${init.claude_code_version}: ${events.join(', ')}` };
}

async function runSessions(sdk, context) {
  if (WANTED.includes('contract__should_run_in_the_sandbox')) {
    const outcome = await contract(sdk, context).catch((error) => ({ outcome: 'fail', kind: 'error', message: error?.stack ?? String(error) }));
    report('contract__should_run_in_the_sandbox', outcome.outcome, outcome);
    if (outcome.code === 'logged_out') {
      everyOther('needs_you', { code: 'logged_out', message: 'Claude Code is not logged in' });
      return;
    }
  }
  const shared = { parents: [], records: [], hookOutput: [], open: [] };
  let sessions = SESSIONS.filter((s) => WANTED.includes(s.name));
  // The checks over all sessions need a session to run, and `--only` with them alone selects none.
  const aggregate = ['cc_no_hook_output', 'cc_records_claude_pid'].some((check) => WANTED.includes(check));
  const silent = new Set();
  if (aggregate && !sessions.length) {
    sessions = SESSIONS.slice(0, 1);
    silent.add(sessions[0].name);
  }
  for (const session of sessions) {
    for (let attempt = 1; ; attempt += 1) {
      try {
        const message = await session.run(context, shared);
        if (!silent.has(session.name)) report(session.name, 'pass', { message });
        break;
      } catch (error) {
        if (silent.has(session.name)) break;
        if (error instanceof Inconclusive && attempt < 3) continue;
        if (error instanceof Inconclusive) report(session.name, 'needs_you', { code: 'inconclusive', message: error.message });
        else if (error instanceof AssumptionBroken && error.message.startsWith('logged_out')) {
          report(session.name, 'needs_you', { code: 'logged_out', message: error.message });
        } else if (error instanceof AssumptionBroken) report(session.name, 'needs_you', { code: 'assumption_broken', message: error.message });
        else if (error?.code === 'ERR_ASSERTION') report(session.name, 'fail', { kind: 'assertion', message: error.message });
        else report(session.name, 'fail', { kind: 'error', message: error?.stack ?? String(error) });
        break;
      } finally {
        for (const run of shared.open.splice(0)) await run.end().catch(() => {});
      }
    }
  }
  if (WANTED.includes('cc_no_hook_output')) {
    report(
      'cc_no_hook_output',
      shared.hookOutput.length ? 'fail' : 'pass',
      shared.hookOutput.length ? { kind: 'assertion', message: `hook output: ${JSON.stringify(shared.hookOutput.slice(0, 5))}` } : {},
    );
  }
  if (WANTED.includes('cc_records_claude_pid')) {
    const strange = shared.parents.filter((p) => !/claude/i.test(p.parent));
    const claudePids = new Set(shared.parents.map((p) => p.ppid));
    const unrecorded = shared.records.filter((r) => !claudePids.has(r.process?.pid));
    if (!shared.parents.length) {
      report('cc_records_claude_pid', 'fail', { kind: 'error', message: 'the recorder saw no hook' });
    } else if (strange.length) {
      report('cc_records_claude_pid', 'needs_you', { code: 'assumption_broken', message: `hook parents that are not claude: ${JSON.stringify(strange.slice(0, 5))}` });
    } else if (!shared.records.length || unrecorded.length) {
      report('cc_records_claude_pid', 'fail', {
        kind: 'assertion',
        message: shared.records.length ? `records without claude's pid: ${JSON.stringify(unrecorded.slice(0, 3))}` : 'wtpool wrote no record in any session',
      });
    } else {
      report('cc_records_claude_pid', 'pass', { message: `the hooks' parent is ${[...new Set(shared.parents.map((p) => p.parent))].join(', ')}` });
    }
  }
}

async function main() {
  mkdirSync(path.dirname(RESULTS), { recursive: true });
  let sdk;
  try {
    sdk = await import(SDK);
  } catch (error) {
    everyOther('fail', { kind: 'error', message: `the Agent SDK (${SDK}) is not installed in vscode/test: ${error.message}` });
    return;
  }
  // Before the run's first `claude`, so a change by any of them counts.
  const install = installSnapshot();
  const dirs = makeRunDirs(process.env.WTPOOL_VERIFY_RUN_DIR);
  const wtpool = process.env.WTPOOL_VERIFY_CC_WTPOOL;
  const claude = installedClaude();
  const version = execFileSync(claude, ['--version'], { encoding: 'utf8', timeout: 30_000 }).trim().split(/\s+/)[0];
  const settings = writeSettings(dirs, wtpool, makePool(dirs, 'settings'));
  writeFileSync(path.join(dirs.logs, 'setup.json'), JSON.stringify({ claude, version, settings }, null, 2));
  const context = {
    sdk,
    dirs,
    wtpool,
    version,
    recordings: process.env.WTPOOL_VERIFY_CC_RECORDINGS,
    options: {
      model: MODEL,
      pathToClaudeCodeExecutable: claude,
      settingSources: [],
      settings: settings.file,
      includeHookEvents: true,
      stderr: (text) => writeFileSync(path.join(dirs.logs, 'claude-stderr.log'), text, { flag: 'a' }),
    },
  };
  const before = transcriptFolders();
  try {
    await runSessions(sdk, context);
  } finally {
    const removed = removeOwnTranscripts(before, process.env.WTPOOL_VERIFY_RUN_DIR);
    writeFileSync(path.join(dirs.logs, 'transcripts-removed.json'), JSON.stringify(removed, null, 2));
    const temp = removeTempFolders(process.env.WTPOOL_VERIFY_RUN_DIR);
    writeFileSync(path.join(dirs.logs, 'temp-removed.json'), JSON.stringify(temp, null, 2));
    const changes = installChanges(install, installSnapshot());
    if (changes.length) {
      failAfter('contract__should_run_in_the_sandbox', `the owner's Claude Code install changed during the run: ${changes.join(', ')}`);
    }
  }
}

main()
  .catch((error) => everyOther('fail', { kind: 'error', message: error?.stack ?? String(error) }))
  .finally(() => process.exit(0));
