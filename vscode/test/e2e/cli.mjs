// The release wtpool as a command line program, for the e2e checks of the status document and the
// activity records that need no VS Code window (S10 to S12, H25, H26): a sandbox without VS Code,
// hook payloads from verify/fixtures/hooks/, run by stub `claude` processes, records read and
// written by hand, and the oracle's schema, open as the product's or closed for the strict check.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { test } from '@playwright/test';
import Ajv2020 from 'ajv/dist/2020.js';

import { scenarioDocuments } from '../support/docs.mjs';
import { writeEarlierBuilds } from '../support/code.mjs';
import { REPO, SCHEMA, runDir } from '../support/paths.mjs';
import { startStub } from './agents.mjs';
import { env, releaseWtpool } from './vscode.mjs';

const FIXTURES = path.join(REPO, 'verify', 'fixtures', 'hooks');

/** A sandbox for wtpool alone, below <run>/e2e/cli/: {root, home, tmp, bin, wtpoolPath}. */
export function cliSandbox(key) {
  const repeat = test.info().repeatEachIndex;
  const root = path.join(runDir(), 'e2e', 'cli', `${key}${repeat ? `-repeat${repeat}` : ''}`);
  rmSync(root, { recursive: true, force: true });
  const dirs = { root, home: path.join(root, 'home'), tmp: path.join(root, 'tmp'), bin: path.join(root, 'bin'), wtpoolPath: releaseWtpool() };
  for (const dir of [dirs.home, dirs.tmp, dirs.bin]) mkdirSync(dir, { recursive: true });
  writeEarlierBuilds(dirs.home);
  return dirs;
}

/**
 * Runs the release wtpool with `args` in `cwd` and the sandbox environment, `extra` added and each
 * variable of `drop` removed: {code, stdout, stderr}.
 */
export function wtpool(dirs, poolInfo, args, { cwd = poolInfo?.fg, extra = {}, drop = [], input = '' } = {}) {
  const variables = { ...env(dirs, poolInfo), ...extra };
  for (const name of drop) delete variables[name];
  const ran = spawnSync(dirs.wtpoolPath, args, { cwd, env: variables, input, encoding: 'utf8', timeout: 30_000 });
  if (ran.error) throw new Error(`setup: wtpool ${args.join(' ')}: ${ran.error.message}`);
  return { code: ran.status, stdout: ran.stdout, stderr: ran.stderr };
}

/** The document `wtpool status --json` prints, or an assertion naming how it failed. */
export function statusDocument(dirs, poolInfo, options) {
  const ran = wtpool(dirs, poolInfo, ['status', '--json'], options);
  assert.equal(ran.code, 0, `wtpool status --json exited ${ran.code}: ${ran.stderr.trim()}`);
  try {
    return JSON.parse(ran.stdout);
  } catch {
    assert.fail(`wtpool status --json printed no JSON document: ${ran.stdout.slice(0, 500)}`);
  }
}

/** The worktree called `name` of a document's only pool. */
export function worktreeOf(status, name) {
  const found = status.pools[0].worktrees.find((w) => w.name === name);
  assert.ok(found, `the document has no worktree ${name}`);
  return found;
}

/** The hook fixture `name` for the pool, `{pool}` resolved, `overrides` merged in. */
export function payload(name, poolInfo, overrides = {}) {
  const text = readFileSync(path.join(FIXTURES, `${name}.json`), 'utf8').replaceAll('{pool}', poolInfo.dir);
  return { ...JSON.parse(text), ...overrides };
}

/**
 * A stub `claude` process that runs the release `wtpool hook` as its child, so records name a live
 * parent (H19): `run(payload, cwd)` asserts the hook exited 0 and printed nothing (H2, H3).
 */
export function claude(dirs, poolInfo) {
  const stub = startStub();
  return {
    async run(body, cwd = body.cwd) {
      const reply = await stub.send({ binary: dirs.wtpoolPath, payload: body, cwd, env: env(dirs, poolInfo) });
      assert.equal(reply.code, 0, `setup: wtpool hook for ${body.hook_event_name} ${body.tool_name ?? ''} exited ${reply.code ?? reply.signal}: ${reply.stderr}`);
      assert.equal(reply.stdout, '', `setup: wtpool hook for ${body.hook_event_name} printed on stdout`);
    },
    async stop() {
      stub.child.kill('SIGTERM');
      await stub.exited;
    },
  };
}

export const activityDir = (poolInfo) => path.join(poolInfo.common, 'wtpool-activity');

/** The record files of wtpool-activity/, without the sandbox marker and lock files. */
export function recordFiles(poolInfo) {
  return readdirSync(activityDir(poolInfo))
    .filter((name) => name.endsWith('.json'))
    .sort();
}

export function readRecord(poolInfo, stem) {
  return JSON.parse(readFileSync(path.join(activityDir(poolInfo), `${stem}.json`), 'utf8'));
}

/** Replaces a record as a hook writes it: a temp file renamed over it (H18). */
export function writeRecord(dirs, poolInfo, stem, record) {
  const temp = path.join(dirs.tmp, `${stem}.${Date.now()}.json`);
  writeFileSync(temp, JSON.stringify(record));
  renameSync(temp, path.join(activityDir(poolInfo), `${stem}.json`));
}

/** RFC 3339 UTC, `seconds` ago. */
export function secondsAgo(seconds) {
  return new Date(Date.now() - seconds * 1000).toISOString().replace(/\.\d{3}Z$/, 'Z');
}

/** The phase the harness runs (WTPOOL_VERIFY_PHASE), 1 by default. */
const PHASE = Number(process.env.WTPOOL_VERIFY_PHASE ?? 1);

/**
 * The oracle's schema of this run's phase, open as status-panel.md S12 has it: from phase 4
 * `status.phase-4.schema.json`, which adds `machine.jobs` (S14) and `build.queued` (S4).
 */
export function oracleSchema() {
  const file = PHASE >= 4 ? path.join(path.dirname(SCHEMA), 'status.phase-4.schema.json') : SCHEMA;
  return JSON.parse(readFileSync(file, 'utf8'));
}

/** `schema` with every object closed: wtpool itself emits only the listed properties (S12). */
export function closed(schema) {
  if (Array.isArray(schema)) return schema.map(closed);
  if (schema === null || typeof schema !== 'object') return schema;
  const out = Object.fromEntries(Object.entries(schema).map(([key, value]) => [key, closed(value)]));
  if (out.properties && typeof out.properties === 'object') out.additionalProperties = false;
  return out;
}

let strict;

/**
 * The closed validator, proved first to accept a scenario document and to refuse it with an
 * unlisted property at each level, so the strict check catches what it exists for.
 */
function strictValidator() {
  if (strict) return strict;
  const ajv = () => new Ajv2020({ allErrors: true, strict: false });
  const open = ajv().compile(oracleSchema());
  const validator = ajv().compile(closed(oracleSchema()));
  const sample = scenarioDocuments('several_agents_one_worktree').documents.at(-1);
  if (!open(sample) || !validator(sample)) throw new Error('setup: a scenario document breaks the oracle schema');
  const places = [
    (doc) => doc,
    (doc) => doc.machine,
    (doc) => doc.pools[0],
    (doc) => doc.pools[0].repository,
    (doc) => doc.pools[0].worktrees[0],
    (doc) => doc.pools[0].worktrees[0].git,
    (doc) => doc.pools[0].worktrees.find((w) => w.actors.length).actors[0],
  ];
  for (const place of places) {
    const probe = structuredClone(sample);
    place(probe).wtpool_verify_unlisted = true;
    if (!open(probe) || validator(probe)) throw new Error('setup: the closed schema accepts an unlisted property, or the open one refuses it');
  }
  strict = validator;
  return strict;
}

/**
 * The problems of `document` against the closed oracle schema: each missing, wrong or unlisted
 * property, with its place in the document.
 */
export function strictProblems(document) {
  const validator = strictValidator();
  if (validator(document)) return [];
  return validator.errors.map((error) => {
    const extra = error.params?.additionalProperty;
    return `${error.instancePath || '/'} ${error.message}${extra ? `: ${extra}` : ''}`;
  });
}

/** The first line `wtpool status --watch` prints, parsed; then it gets stdin EOF (W8). */
export function firstWatchDocument(dirs, poolInfo) {
  return new Promise((resolve, reject) => {
    const child = spawn(dirs.wtpoolPath, ['status', '--watch'], { cwd: poolInfo.fg, env: env(dirs, poolInfo), stdio: ['pipe', 'pipe', 'pipe'] });
    const err = [];
    child.stderr.on('data', (chunk) => err.push(chunk));
    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error(`wtpool status --watch printed no line in 15 s: ${Buffer.concat(err).toString('utf8')}`));
    }, 15_000);
    child.on('error', (error) => reject(new Error(`setup: wtpool status --watch: ${error.message}`)));
    child.on('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`wtpool status --watch exited ${code} before a line: ${Buffer.concat(err).toString('utf8').trim()}`));
    });
    createInterface({ input: child.stdout }).once('line', (line) => {
      clearTimeout(timer);
      child.removeAllListeners('exit');
      child.stdin.end();
      try {
        resolve(JSON.parse(line));
      } catch {
        reject(new Error(`wtpool status --watch printed no JSON line: ${line.slice(0, 300)}`));
      }
    });
  });
}
