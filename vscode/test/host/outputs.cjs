// The plumbing the output checks share: the pool the fake wtpool reports, a fresh folder of task
// output files per check, the documents that list them, and the files the extension opens, lstats
// and watches.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const { openStatus, posted, resetFake, scenarioDocuments, verifyApi, views, waitFor, wrap } = require('./support.cjs');

const POOL = '/Users/verify/pool';
const WT2 = `${POOL}/wt2`;
const WT3 = `${POOL}/wt3`;
const HOST_DIR = process.env.WTPOOL_VERIFY_HOST_DIR ?? fs.mkdtempSync('/tmp/wtpool-verify-output-');
/** How long a check waits for something the extension must not do. */
const QUIET_MS = 2500;

let folder;
let generation = 0;

/**
 * A fresh folder for one check, `folder`, and its `tasks` folder for the output files: the
 * extension reads only a path that ends in `/tasks/<id>.output`.
 */
function outputFolder(name) {
  folder = path.join(HOST_DIR, 'outputs', `${name}-${Date.now()}`);
  const tasks = path.join(folder, 'tasks');
  fs.mkdirSync(tasks, { recursive: true });
  return tasks;
}

function output(file, extra = {}) {
  return {
    task: path.basename(file, '.output'),
    path: file,
    summary: 'cargo build',
    running: true,
    started: '2026-10-20T11:58:00Z',
    ended: null,
    ...extra,
  };
}

function actor(key, outputs, extra = {}) {
  return {
    key,
    kind: 'session',
    harness: 'claude-code',
    label: `session ${key}`,
    state: 'working',
    reason: null,
    action: { tool: 'Bash', target: 'cargo build' },
    placed_by: 'path',
    error: null,
    since: '2026-10-20T11:59:00Z',
    updated: '2026-10-20T11:59:30Z',
    ...(outputs.length ? { outputs } : {}),
    ...extra,
  };
}

/** idle_pool's last document with `wt2` and `wt3` holding these actors, at a new time. */
function poolWith({ wt2 = [], wt3 = [], wt2Facts = {} } = {}) {
  const doc = structuredClone(scenarioDocuments('idle_pool').at(-1));
  generation += 1;
  doc.generated = new Date(Date.parse(doc.generated) + generation * 1000).toISOString().replace('.000Z', 'Z');
  for (const [where, actors] of [
    [WT2, wt2],
    [WT3, wt3],
  ]) {
    const row = doc.pools[0].worktrees.find((w) => w.path === where);
    assert.ok(row, `setup: idle_pool has no ${where}`);
    row.actors = actors;
    if (actors.length) {
      row.activity = 'working';
      row.last_event = '2026-10-20T11:59:30Z';
    }
    if (where === WT2) Object.assign(row, wt2Facts);
  }
  return doc;
}

/** Records the paths below `folder` that the extension opens, lstats and watches. */
function watchFiles() {
  const seen = [];
  const inFolder = (target) => typeof target === 'string' && folder && target.startsWith(folder);
  const record = (kind) => (target) => {
    if (inFolder(target)) seen.push({ kind, path: target, at: Date.now() });
    return true;
  };
  const wraps = [
    wrap(fs.promises, 'open', record('open')),
    wrap(fs.promises, 'lstat', record('lstat')),
    wrap(fs, 'lstat', record('lstat')),
    wrap(fs, 'watch', record('watch')),
  ];
  return {
    seen,
    restore() {
      for (const one of wraps.reverse()) one.restore();
    },
  };
}

async function outputs() {
  return (await posted()).filter((m) => m && m.type === 'output');
}

/** The next output message after `count` for `row` whose tails pass `test`, within `ms`. */
async function nextOutput(count, row, test = () => true, ms = 5_000) {
  return waitFor(async () => (await outputs()).slice(count).find((m) => m.row === row && test(m)), ms);
}

async function start(doc) {
  await resetFake({ documents: [doc], aliveMs: 1000 });
  const before = (await views()).length;
  await openStatus();
  const shown = await waitFor(async () => (await views()).slice(before).find((v) => v.status?.generated === doc.generated), 10_000);
  assert.ok(shown, 'the extension posted no document');
  return verifyApi();
}

function lines(count, prefix = 'line') {
  return Array.from({ length: count }, (_, n) => `${prefix} ${String(n + 1).padStart(5, '0')}`).join('\n') + '\n';
}

module.exports = {
  HOST_DIR,
  POOL,
  QUIET_MS,
  WT2,
  WT3,
  actor,
  lines,
  nextOutput,
  output,
  outputFolder,
  outputs,
  poolWith,
  start,
  watchFiles,
};
