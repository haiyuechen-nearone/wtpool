// Status documents and view messages for the render, shot and host layers. Every document is
// valid against verify/schema/status.phase-4.schema.json, and `mustValidate` proves it before a
// test uses it. That schema only adds properties to status.schema.json.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import Ajv2020 from 'ajv/dist/2020.js';

import { SCENARIOS, SCHEMA_PHASE_4 } from './paths.mjs';

export const GENERATED = '2026-10-20T12:00:00Z';
export const NOW = Date.parse(GENERATED);
export const POOL = '/Users/verify/pool';

/** `seconds` before {@link GENERATED}, as RFC 3339. */
export function ago(seconds) {
  return new Date(NOW - seconds * 1000).toISOString().replace('.000Z', 'Z');
}

export function git(overrides = {}) {
  return {
    dirty: 0,
    ahead: null,
    behind: null,
    unreferenced: 0,
    operation: null,
    unknown: false,
    ...overrides,
  };
}

function roleOf(name) {
  if (name === 'fg') return 'foreground';
  return /^wt\d+$/.test(name) ? 'slot' : 'unmanaged';
}

/** A worktree of the pool, with defaults that follow its role. */
export function worktree(name, overrides = {}) {
  const role = roleOf(name);
  const base = {
    path: `${POOL}/${name}`,
    name,
    role,
    missing: false,
    branch: null,
    upstream: null,
    git: git(),
    free: false,
    reasons: [],
    activity: 'unused',
    actors: [],
    builds: [],
    last_event: null,
    last_change: ago(600),
  };
  if (role === 'foreground') {
    Object.assign(base, {
      branch: 'main',
      upstream: 'origin/main',
      git: git({ ahead: 0, behind: 0 }),
      reasons: ['foreground'],
      activity: 'foreground',
    });
  }
  if (role === 'unmanaged') {
    Object.assign(base, { branch: 'side', reasons: ['unmanaged'] });
  }
  if (role === 'slot') {
    Object.assign(base, { free: true, activity: 'free' });
  }
  return { ...base, ...overrides };
}

let actorCount = 0;

/** An actor line: a session working on a Bash call unless `overrides` says otherwise. */
export function actor(overrides = {}) {
  actorCount += 1;
  return {
    key: `s${actorCount}`,
    kind: 'session',
    harness: 'claude-code',
    label: 'session 13:41',
    state: 'working',
    reason: null,
    action: { tool: 'Bash', target: 'cargo nextest' },
    placed_by: 'cwd',
    error: null,
    since: ago(120),
    updated: ago(30),
    ...overrides,
  };
}

/** A worktree on a branch that holds one actor, its activity following the actor's state. */
export function withActor(name, actorOverrides, worktreeOverrides = {}) {
  const one = actor(actorOverrides);
  const activity =
    one.state === 'waiting'
      ? 'waiting'
      : one.state === 'working'
        ? 'working'
        : one.state === 'quiet' || one.state === 'idle'
          ? 'idle'
          : 'unused';
  return worktree(name, {
    branch: `${name}-branch`,
    free: false,
    reasons: ['branch'],
    actors: [one],
    last_event: one.updated,
    activity,
    ...worktreeOverrides,
  });
}

/** The repository of the pool, on GitHub: its heading reads `acme/app · verify-mac`. */
export const REPOSITORY = { name: 'app', github: 'acme/app' };

export function document(worktrees, poolOverrides = {}, overrides = {}) {
  return {
    type: 'status',
    version: 1,
    generated: GENERATED,
    machine: { name: 'verify-mac' },
    pools: [
      {
        common_dir: `${POOL}/fg/.git`,
        repository: { ...REPOSITORY },
        agents: 'on',
        skipped_records: 0,
        worktrees,
        ...poolOverrides,
      },
    ],
    ...overrides,
  };
}

/**
 * A webview input message. `retryAt` is the next restart of a `paused` view. `status` is null only
 * before the first document.
 */
export function view(status, { connection = 'live', lastLine = status?.generated ?? GENERATED, retryAt } = {}) {
  const message = { type: 'view', version: 1, connection, last_line: lastLine, status };
  if (retryAt !== undefined) message.retry_at = retryAt;
  return message;
}

/** One worktree per activity value. */
export function everyActivity() {
  return document([
    worktree('fg', { last_change: ago(300) }),
    withActor('wt1', {
      state: 'waiting',
      reason: 'permission',
      action: { tool: 'Bash', target: 'git push' },
    }),
    withActor('wt2', { state: 'working', label: 'review' }),
    worktree('wt3', {
      branch: 'feature-x',
      reasons: ['branch'],
      free: false,
      activity: 'building',
      builds: [{ pid: 4242, name: 'cargo-nextest' }],
    }),
    withActor('wt4', { state: 'idle', label: 'session 14:02', action: null }),
    worktree('wt5', { last_change: ago(7200) }),
    worktree('wt6', {
      free: false,
      reasons: ['dirty'],
      git: git({ dirty: 2 }),
      activity: 'not_reusable',
    }),
    worktree('wt7', { branch: 'feature-y', free: false, reasons: ['branch'], activity: 'unused' }),
    worktree('wt8', {
      missing: true,
      free: false,
      reasons: ['missing'],
      git: git({ dirty: null, unreferenced: null, unknown: true }),
      activity: 'missing',
      last_change: null,
    }),
    worktree('x', {
      git: git({ dirty: null, unknown: true }),
      reasons: ['unmanaged', 'unknown'],
      activity: 'unknown',
    }),
  ]);
}

let validator;

/** Throws a setup error when `status` breaks the schema, so a test never feeds one. */
export function mustValidate(status) {
  if (!validator) {
    const ajv = new Ajv2020({ allErrors: true, strict: false });
    validator = ajv.compile(JSON.parse(readFileSync(SCHEMA_PHASE_4, 'utf8')));
  }
  if (!validator(status)) {
    throw new Error(`setup: a test document breaks the schema: ${JSON.stringify(validator.errors)}`);
  }
  return status;
}

/** Every document of a scenario file, `{pool}` and `{common}` replaced by fixed paths. */
export function scenarioDocuments(name) {
  const scenario = JSON.parse(readFileSync(path.join(SCENARIOS, `${name}.json`), 'utf8'));
  const resolve = (value) =>
    JSON.parse(
      JSON.stringify(value)
        .replaceAll('{common}', `${POOL}/fg/.git`)
        .replaceAll('{pool}', POOL),
    );
  const documents = scenario.steps
    .filter((step) => step.expect)
    .map((step) => resolve(step.expect));
  documents.push(resolve(scenario.expect));
  return { scenario, documents };
}
