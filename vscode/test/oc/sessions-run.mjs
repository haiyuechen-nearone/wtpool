// The oc layer's sessions under `opencode run --standalone`: each runs a script of the stub model
// on a fresh pool, then checks OpenCode with expectOpenCode and wtpool with plain asserts.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { sleep } from '../support/processes.mjs';
import { Inconclusive, Session, expectOpenCode, openCodeActors, stemOf } from './opencode.mjs';
import { providerHost } from './network.mjs';
import { wtpoolEnv } from './sandbox.mjs';
import { call, say, slow } from './stub-model.mjs';

/** The main session's record: a stem with no subagent part. */
export function sessionRecords(session) {
  return session.records().filter((r) => !r.file.slice(0, -'.json'.length).includes('.'));
}

/**
 * True when the turn_end of session `id` reached wtpool: the wrapper kept its payload. Under
 * `run --standalone` the run's server may exit before that hook runs, and the record then misses
 * its turn_end.
 */
export function turnEndReached(session, id) {
  return session.recordCalls().some((c) => /^session\.execution\.(succeeded|failed|interrupted)$/.test(c.payload?.name ?? '') && c.payload?.data?.sessionID === id && !c.payload.parent);
}

/** The note of a result whose run lost its last turn_end. */
export const LOST_TURN_END = "the run's server exited before its last turn_end reached wtpool (opencode.md, Limits)";

/** The index of the first recorder line `test` passes, or -1. */
const indexOf = (lines, test) => lines.findIndex(test);

/** The recorder's server: the pid and argv of its first instance. */
function server(session) {
  const setup = session.recorded().find((line) => line.kind === 'setup');
  return setup ? { pid: setup.pid, argv: setup.payload?.argv ?? [] } : null;
}

/** The actor of record `stem` in each sample in which it shows. */
function shownAs(session, stem) {
  return session.samples.flatMap((sample) => sample.actors.filter((a) => a.key === stem).map((actor) => ({ at: sample.at, actor })));
}

async function readEditShell(context) {
  const s = new Session(context, 'oc_read_edit_shell');
  s.config();
  const wt1 = s.wt('wt1');
  context.stub.scripts.oc_read_edit_shell = [
    call('read', { path: '../wt1/f.txt' }),
    call('edit', { path: `${wt1}/f.txt`, oldString: 'base', newString: 'edited' }),
    call('shell', { command: 'ls' }),
    call('shell', { command: `cd ${wt1} && git status` }),
    slow(20, 100),
  ];
  const stop = s.sampleActors(250);
  const run = await s.run('[[scenario:oc_read_edit_shell]] Read ../wt1/f.txt, change base to edited there, run ls, then run cd ../wt1 && git status.');
  await stop();
  const after = openCodeActors(s.actors());
  const lines = s.recorded();
  expectOpenCode(run.code === 0, `opencode run exited ${run.code} ${run.signal ?? ''}`);
  const before = lines.filter((l) => l.kind === 'hook' && l.name === 'tool.execute.before');
  expectOpenCode(
    JSON.stringify(before.map((l) => l.payload.tool)) === JSON.stringify(['read', 'edit', 'shell', 'shell']),
    `the calls' tool.execute.before: ${JSON.stringify(before.map((l) => l.payload.tool))}`,
  );
  for (const hook of before) {
    const id = hook.payload.id;
    const at = lines.indexOf(hook);
    const success = indexOf(lines, (l) => l.kind === 'event' && l.name === 'session.tool.success' && l.payload.data?.id === id);
    expectOpenCode(success > at, `${hook.payload.tool} ${id}: session.tool.success at ${success}, after the hook at ${at}`);
    if (hook.payload.tool === 'shell') {
      const create = indexOf(lines, (l, i) => i > at && l.kind === 'hook' && l.name === 'shell.create.before');
      expectOpenCode(create > at && create < success, `${id}: shell.create.before at ${create}, not between ${at} and ${success}`);
    }
  }
  const records = sessionRecords(s);
  assert.equal(records.length, 1, `${records.length} session records: ${records.map((r) => r.file).join(', ')}`);
  const [record] = records;
  const sessionID = before[0].payload.sessionID;
  assert.equal(record.file, `${stemOf(sessionID)}.json`, 'the record file');
  assert.equal(record.harness, 'opencode', 'the record harness');
  const host = server(s);
  assert.equal(record.process?.pid, host?.pid, 'the record process is not the OpenCode server');
  expectOpenCode(host.argv.includes('serve') && host.argv.includes('--stdio'), `the server's argv ${JSON.stringify(host.argv)}`);
  assert.equal(record.process?.shared, true, 'the record process is not shared');
  assert.equal(path.resolve(record.worktree), wt1, 'the record is not on wt1');
  assert.equal(record.placed_by, 'path', 'wt1 did not place the record by path');
  assert.equal(record.label, 'Stub title oc_read_edit_shell', 'the label is not the generated title');
  const turnEnd = turnEndReached(s, sessionID);
  if (turnEnd) assert.equal(record.state, 'idle', 'the turn_end reached wtpool, yet the record is not idle');
  const shown = shownAs(s, stemOf(sessionID));
  assert.ok(shown.some((x) => x.actor.worktree === 'wt1' && x.actor.placed_by === 'path'), `the session never showed on wt1 by path: ${JSON.stringify(shown.map((x) => [x.actor.worktree, x.actor.placed_by]))}`);
  assert.deepEqual(after.map((a) => a.key), [], 'the session still shows after the run exited');
  return `records ${record.file}; process ${record.process.pid} ${host.argv.join(' ')}; ${turnEnd ? 'idle after the turn' : LOST_TURN_END}`;
}

async function subagent(context) {
  const s = new Session(context, 'oc_subagent');
  s.config();
  const wt2 = s.wt('wt2');
  context.stub.scripts.oc_subagent = [
    call('subagent', { agent: 'explore', description: 'Read slot two', prompt: '[[scenario:oc_subagent_child]] Read the file of slot two and report its first line.' }),
    say('The subagent reported the line.'),
  ];
  context.stub.scripts.oc_subagent_child = [call('read', { path: `${wt2}/f.txt` }), slow(10, 100)];
  const stop = s.sampleActors(250);
  const run = await s.run('[[scenario:oc_subagent]] Use your subagent tool to read the file of slot two.');
  await stop();
  expectOpenCode(run.code === 0, `opencode run exited ${run.code}`);
  const lines = s.recorded();
  const created = lines.find((l) => l.kind === 'event' && l.name === 'session.created' && l.payload.data?.parentID);
  expectOpenCode(created, 'no session.created with a parentID');
  const parent = created.payload.data.parentID;
  const child = created.payload.data.sessionID;
  expectOpenCode(
    lines.some((l) => l.kind === 'hook' && l.name === 'tool.execute.before' && l.payload.sessionID === child && l.payload.tool === 'read'),
    `the child's read hook does not carry the child's id ${child}`,
  );
  const stem = `${stemOf(parent)}.${child.replace('_', '-')}`;
  const record = s.records().find((r) => r.file === `${stem}.json`);
  assert.ok(record, `no subagent record ${stem}.json: ${s.records().map((r) => r.file).join(', ')}`);
  assert.equal(path.resolve(record.worktree), wt2, 'the subagent is not on wt2');
  assert.equal(record.label, 'Read slot two', "the subagent's label is not its description");
  assert.equal(record.state, 'done', 'the subagent is not done after its end');
  const shown = shownAs(s, stem);
  assert.ok(shown.some((x) => x.actor.worktree === 'wt2'), `the subagent never showed on wt2: ${JSON.stringify(shown.map((x) => x.actor.worktree))}`);
  return `subagent ${stem}`;
}

/** The time of an actor's `updated`, in milliseconds. */
function updatedMs(actor) {
  const value = actor?.updated;
  if (typeof value === 'string') return Date.parse(value);
  if (typeof value === 'number') return value < 1e12 ? value * 1000 : value;
  if (value && typeof value === 'object') return Date.parse(value.iso ?? value.utc ?? value.time ?? '') || Number(value.epoch ?? value.unix ?? 0) * 1000;
  return Number.NaN;
}

async function longAnswer(context) {
  const s = new Session(context, 'oc_long_answer');
  s.config();
  context.stub.scripts.oc_long_answer = [slow(800, 22, { essay: true })];
  const stop = s.sampleActors(500);
  const run = await s.run('[[scenario:oc_long_answer]] Write an 800 word essay about git worktrees. Use no tools.');
  await stop();
  expectOpenCode(run.code === 0, `opencode run exited ${run.code}`);
  const deltas = s.recorded().filter((l) => l.kind === 'event' && l.name === 'session.text.delta');
  expectOpenCode(deltas.length >= 100, `${deltas.length} session.text.delta events, not at least 100`);
  const first = deltas[0].t;
  const last = deltas.at(-1).t;
  const streaming = s.samples.filter((x) => x.at > first + 1000 && x.at < last);
  const actors = streaming.map((x) => openCodeActors(x.actors).find((a) => a.kind === 'session')).filter(Boolean);
  assert.ok(actors.length >= 5, `the session showed in ${actors.length} samples while it streamed`);
  assert.ok(actors.some((a) => a.action?.tool === 'generating'), `the action never read generating: ${JSON.stringify([...new Set(actors.map((a) => JSON.stringify(a.action)))])}`);
  const times = [...new Set(actors.map(updatedMs))].sort((a, b) => a - b);
  assert.ok(times.length >= 2, `updated never advanced while it streamed: ${JSON.stringify(actors.map((a) => a.updated).slice(0, 3))}`);
  for (let i = 1; i < times.length; i += 1) {
    assert.ok(times[i] - times[i - 1] >= 9000, `updated advanced twice within 10 s: ${new Date(times[i - 1]).toISOString()} then ${new Date(times[i]).toISOString()}`);
  }
  const progress = s.recordCalls().filter((c) => /\.delta$/.test(c.payload?.name ?? ''));
  assert.ok(progress.length >= 1 && progress.length <= 3, `${progress.length} progress spawns in the wrapper's log, not 1 to 3`);
  return `${deltas.length} deltas over ${((last - first) / 1000).toFixed(1)} s; ${progress.length} progress spawns; updated ${times.length} times`;
}

async function deny(context) {
  const s = new Session(context, 'oc_deny');
  s.config();
  const job = (...args) => execFileSync(context.wtpool, ['job', ...args], { cwd: s.pool.fg, env: wtpoolEnv(context, s.pool), encoding: 'utf8', timeout: 30_000 });
  job('enforce', 'require');
  job('heavy', 'add', '--', 'cargo test');
  context.stub.scripts.oc_deny = [call('shell', { command: 'cargo test' }), say('The tests were refused.')];
  let end;
  let id;
  try {
    await s.serve();
    id = await s.create(s.pool.fg);
    const from = s.mark();
    await s.prompt(id, '[[scenario:oc_deny]] Run cargo test in the shell.');
    end = await s.turnEnd(id, from);
    await sleep(1500);
  } finally {
    // The machine settings of the sandbox stay as the other sessions expect them.
    job('heavy', 'rm', '--', 'cargo test');
    job('enforce', 'default');
  }
  expectOpenCode(end?.type === 'session.execution.succeeded', `the turn ended ${end?.type}`);
  const lines = s.recorded();
  const called = lines.find((l) => l.kind === 'event' && l.name === 'session.tool.called' && l.payload.data?.input?.command === 'cargo test');
  expectOpenCode(called, 'no session.tool.called for cargo test');
  const failed = lines.find((l) => l.kind === 'event' && l.name === 'session.tool.failed' && l.payload.data?.id === called.payload.data.id);
  assert.ok(failed, `the denied call did not fail: ${lines.filter((l) => l.name?.startsWith('session.tool')).map((l) => l.name).join(', ')}`);
  const reason = failed.payload.data?.error?.message ?? '';
  expectOpenCode(failed.payload.data?.error?.type === 'unknown', `the thrown deny failed the call as ${failed.payload.data?.error?.type}`);
  assert.ok(reason.includes('wtpool job -- cargo test'), `the call failed with ${JSON.stringify(reason)}, not J21's reason naming the wrapped command`);
  const requests = readFileSync(context.stubLog, 'utf8').split('\n').filter(Boolean).map((line) => JSON.parse(line)).filter((r) => r.scenario === 'oc_deny' && r.step >= 1);
  expectOpenCode(requests.some((r) => r.toolResults.some((t) => t.content.includes(reason.slice(0, 60)))), "the model's next input does not hold the reason");
  assert.ok(!lines.some((l) => l.kind === 'hook' && l.name === 'shell.create.before' && l.payload.command === 'cargo test'), 'the denied command started a shell');
  assert.ok(!lines.some((l) => l.kind === 'event' && l.name === 'shell.created'), 'the denied command ran in a shell');
  const record = s.records().find((r) => r.file === `${stemOf(id)}.json`);
  assert.ok(record, 'no session record');
  assert.equal(record.state, 'idle', 'the turn did not end idle');
  assert.deepEqual(record.in_flight ?? [], [], 'a call is still in flight');
  return `denied: ${reason.slice(0, 200)}`;
}

async function wtpoolMissing(context) {
  const s = new Session(context, 'oc_wtpool_missing', { plugin: path.join(context.dirs.bin, 'no-such-wtpool') });
  s.config();
  writeFileSync(path.join(s.pool.fg, 'notes.txt'), 'notes of the foreground\n');
  context.stub.scripts.oc_wtpool_missing = [call('read', { path: 'notes.txt' }), call('shell', { command: 'ls' }), say('Done without wtpool.')];
  const run = await s.run('[[scenario:oc_wtpool_missing]] Read notes.txt and run ls.');
  const lines = s.recorded();
  expectOpenCode(run.code === 0, `opencode run exited ${run.code}`);
  expectOpenCode(lines.some((l) => l.kind === 'event' && l.name === 'session.execution.succeeded'), 'the session did not end its turn');
  const failed = lines.filter((l) => l.kind === 'event' && l.name === 'session.tool.failed');
  assert.deepEqual(failed.map((l) => l.payload.data?.error), [], 'a call failed: the plugin threw');
  assert.equal(lines.filter((l) => l.kind === 'event' && l.name === 'session.tool.success').length, 2, 'the calls did not both succeed');
  assert.deepEqual(s.records().map((r) => r.file), [], 'a record was written');
  return 'the session ran and ended with no record';
}

/**
 * A plugin named to load before wtpool's, whose setup takes 400 ms. OpenCode loads a folder's
 * plugins one after another, so the instances of the plugins after it start reading events only
 * once `run --title` has created and renamed its session.
 */
const SLOW_PLUGIN = '0-wtpool-verify-slow';
const SLOW_SOURCE = "export default { id: 'wtpool-verify-slow', setup: async () => { await new Promise((resolve) => setTimeout(resolve, 400)); return () => {}; } };\n";

async function runTitle(context) {
  const s = new Session(context, 'oc_run_title');
  s.config();
  const slow = path.join(context.dirs.config, 'opencode', 'plugins', SLOW_PLUGIN);
  mkdirSync(slow, { recursive: true });
  writeFileSync(path.join(slow, 'index.ts'), SLOW_SOURCE);
  const title = 'Given title oc_run_title';
  context.stub.scripts.oc_run_title = [say('The title check is done.')];
  let run;
  try {
    run = await s.run('[[scenario:oc_run_title]] Say that the title check is done.', { flags: ['--title', title] });
  } finally {
    rmSync(slow, { recursive: true, force: true });
  }
  expectOpenCode(run.code === 0, `opencode run exited ${run.code} ${run.signal ?? ''}`);
  const lines = s.recorded();
  const first = lines.find((l) => l.kind === 'event' && typeof l.payload?.data?.sessionID === 'string');
  expectOpenCode(first, 'no event named a session');
  const id = first.payload.data.sessionID;
  const named = lines.filter((l) => l.kind === 'event' && /^session\.(created|renamed)$/.test(l.name) && l.payload.data?.sessionID === id);
  expectOpenCode(!named.length, `the plugins after the slow one got ${named.map((l) => l.name).join(', ')} of the session`);
  const records = sessionRecords(s);
  assert.equal(records.length, 1, `${records.length} session records: ${records.map((r) => r.file).join(', ')}`);
  const [record] = records;
  assert.equal(record.file, `${stemOf(id)}.json`, 'the record file');
  assert.equal(record.label, title, 'the label is not the title given with run --title');
  assert.deepEqual(record.title, { text: title, by: 'harness' }, "the record's title");
  const lookups = s.recordCalls().filter((c) => c.payload?.kind === 'lookup' && c.payload?.data?.id === id);
  assert.equal(lookups.length, 1, `${lookups.length} lookups of the session reached wtpool`);
  return `the label ${JSON.stringify(record.label)} by the lookup; ${turnEndReached(s, id) ? 'its turn_end reached wtpool' : LOST_TURN_END}`;
}

/** The maker of `model` by the start of its last part, as the panel's mark table reads it. */
const makerOf = (model) => (/^(glm|z-ai)/i.test(model.split('/').at(-1)) ? 'zhipu' : 'other');

async function realModel(context) {
  const model = process.env.WTPOOL_VERIFY_OC_MODEL || 'opencode/glm-5.3-flash-free';
  const s = new Session(context, 'oc_real_model');
  s.config();
  const listed = await s.spawnOpenCode(['models', '--standalone'], { label: 'models', limitMs: 120_000 }).exited;
  const models = readFileSync(path.join(s.dir, 'models.stdout'), 'utf8');
  expectOpenCode(listed.code === 0 && models.split(/\s+/).includes(model), `opencode models --standalone does not list ${model} (exit ${listed.code})`);
  const wt1 = s.wt('wt1');
  context.proxy.allow = providerHost;
  let run;
  try {
    const stop = s.sampleActors(500);
    run = await s.run('List the files of the folder ../wt1 with your shell tool: run exactly the command `ls ../wt1` once, then reply with the word done.', {
      flags: ['--model', model],
      limitMs: 300_000,
    });
    await stop();
  } finally {
    context.proxy.allow = () => false;
  }
  const lines = s.recorded();
  expectOpenCode(run.code === 0, `opencode run exited ${run.code}: ${readFileSync(path.join(s.dir, 'run.stderr'), 'utf8').slice(0, 300)}`);
  const shell = lines.find((l) => l.kind === 'hook' && l.name === 'tool.execute.before' && l.payload.tool === 'shell');
  if (!shell) throw new Inconclusive(`the model called no shell tool: ${lines.filter((l) => l.name === 'tool.execute.before').map((l) => l.payload.tool).join(', ') || 'no tool'}`);
  const stem = stemOf(shell.payload.sessionID);
  const shown = shownAs(s, stem);
  const call = shell.t;
  assert.ok(shown.some((x) => x.actor.worktree === 'fg'), `the session never showed in fg: ${JSON.stringify(shown.map((x) => x.actor.worktree))}`);
  assert.ok(shown.some((x) => x.at > call && x.actor.worktree === 'wt1' && x.actor.placed_by === 'path'), `the shell call did not place the session in wt1 by path: ${JSON.stringify(shown.map((x) => [x.actor.worktree, x.actor.placed_by]))}`);
  assert.ok(shown.some((x) => x.actor.state === 'working'), 'the session never showed working during its turn');
  const [record] = sessionRecords(s);
  assert.ok(record, 'no session record');
  const turnEnd = turnEndReached(s, shell.payload.sessionID);
  if (turnEnd) assert.equal(record.state, 'idle', 'the turn_end reached wtpool, yet the record is not idle');
  assert.equal(record.model, model, "the record's model");
  assert.equal(record.harness, 'opencode', "the record's harness");
  assert.equal(makerOf(record.model), 'zhipu', `the model ${record.model} gives no Zhipu mark`);
  const asked = context.proxy.asked.filter((a) => a.session === 'oc_real_model').map((a) => a.host);
  assert.equal(path.resolve(record.worktree), wt1, 'the record is not on wt1');
  return `${model} through ${[...new Set(asked)].join(', ') || 'no proxied host'}; shell ${JSON.stringify(shell.payload.input?.command)}; ${turnEnd ? 'idle after the turn' : LOST_TURN_END}`;
}

export const RUN_SESSIONS = [
  { name: 'oc_read_edit_shell', run: readEditShell },
  { name: 'oc_subagent', run: subagent },
  { name: 'oc_long_answer', run: longAnswer },
  { name: 'oc_deny', run: deny },
  { name: 'oc_wtpool_missing', run: wtpoolMissing },
  { name: 'oc_run_title', run: runTitle },
  { name: 'oc_real_model', run: realModel, attempts: 3 },
];
