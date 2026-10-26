import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from '@playwright/test';

import { VSCODE } from '../support/paths.mjs';
import { claude, cliSandbox, firstWatchDocument, oracleSchema, payload, statusDocument, strictProblems, wtpool } from './cli.mjs';
import { git, pool, startProcess } from './vscode.mjs';
import { repositoryPool } from './folders.mjs';

/** Annotations a schema words freely, as they never change the documents it accepts. */
const ANNOTATIONS = new Set(['title', 'description', '$id', '$comment']);
/** Arrays whose order means nothing to a schema. */
const SETS = new Set(['required', 'enum', 'type']);

/**
 * `schema` without annotations and with its unordered arrays sorted, the part that decides the
 * documents it accepts.
 */
function structure(schema, key = '') {
  if (Array.isArray(schema)) {
    const items = schema.map((item) => structure(item));
    return SETS.has(key) ? items.map((item) => JSON.stringify(item)).sort() : items;
  }
  if (schema === null || typeof schema !== 'object') return schema;
  return Object.fromEntries(
    Object.keys(schema)
      .filter((name) => !ANNOTATIONS.has(name))
      .sort()
      .map((name) => [name, structure(schema[name], name)]),
  );
}

/** Each property of `schema`, at any depth, whose `description` is no one-line text. */
function undescribed(schema, at = '#') {
  if (Array.isArray(schema)) return schema.flatMap((item, index) => undescribed(item, `${at}/${index}`));
  if (schema === null || typeof schema !== 'object') return [];
  const found = [];
  for (const [name, value] of Object.entries(schema)) {
    const here = `${at}/${name}`;
    if (name === 'properties' && value && typeof value === 'object') {
      for (const [property, sub] of Object.entries(value)) {
        const text = sub?.description;
        if (typeof text !== 'string' || !text.trim() || /[\r\n]/.test(text)) found.push(`${here}/${property}`);
      }
    }
    found.push(...undescribed(value, here));
  }
  return found;
}

/** Each place of `schema` that sets `additionalProperties: false`. */
function closedObjects(schema, at = '#') {
  if (Array.isArray(schema)) return schema.flatMap((item, index) => closedObjects(item, `${at}/${index}`));
  if (schema === null || typeof schema !== 'object') return [];
  const found = schema.additionalProperties === false ? [at] : [];
  for (const [name, value] of Object.entries(schema)) found.push(...closedObjects(value, `${at}/${name}`));
  return found;
}

/** The structural differences of two schemas, as JSON pointers, at most `limit`. */
function differences(a, b, at = '#', limit = 20) {
  if (JSON.stringify(a) === JSON.stringify(b)) return [];
  const objects = a && b && typeof a === 'object' && typeof b === 'object' && !Array.isArray(a) && !Array.isArray(b);
  if (!objects) return [`${at}: wtpool has ${JSON.stringify(a)}, the oracle ${JSON.stringify(b)}`];
  const found = [];
  for (const key of [...new Set([...Object.keys(a), ...Object.keys(b)])].sort()) {
    if (found.length >= limit) break;
    if (!(key in a)) found.push(`${at}/${key}: only the oracle has it`);
    else if (!(key in b)) found.push(`${at}/${key}: only wtpool has it`);
    else found.push(...differences(a[key], b[key], `${at}/${key}`, limit - found.length));
  }
  return found;
}

test('schema__should_be_published_by_wtpool_status', async () => {
  // Given
  const dirs = cliSandbox('schema');
  const poolInfo = pool('idle_pool', 'schema');
  const oracle = oracleSchema();
  const generated = JSON.parse(readFileSync(path.join(VSCODE, 'src', 'generated', 'status.schema.json'), 'utf8'));

  // When
  const ran = wtpool(dirs, poolInfo, ['status', '--schema']);

  // Then
  assert.equal(ran.code, 0, `wtpool status --schema exited ${ran.code}: ${ran.stderr.trim()}`);
  let schema;
  try {
    schema = JSON.parse(ran.stdout);
  } catch {
    assert.fail(`wtpool status --schema printed no JSON: ${ran.stdout.slice(0, 300)}`);
  }
  assert.deepEqual(undescribed(oracle), [], 'setup: the oracle schema has a property without a description');
  assert.deepEqual(differences(structure(schema), structure(oracle)), [], 'wtpool status --schema is not the oracle schema (descriptions and titles aside)');
  assert.deepEqual(undescribed(schema), [], 'properties without a one-line description');
  assert.deepEqual(closedObjects(schema), [], 'objects that set additionalProperties: false');
  // The example's text is in src/generated, kept current by schema__generated_files_current.
  assert.deepEqual(schema, generated, 'wtpool status --schema differs from the status_schema example (vscode/src/generated/status.schema.json)');
});

/** The scenario pools whose documents the strict check reads: every role, activity and git fact. */
const STRICT_POOLS = ['idle_pool', 'folder_missing', 'git_operations', 'unknown_git', 'unmanaged_worktree', 'agents_off'];

test('status__should_emit_only_listed_properties', async () => {
  // Given: the scenario pools, and a pool with a session, a subagent, a wait and a build.
  const dirs = cliSandbox('strict');
  const documents = [];
  for (const name of STRICT_POOLS) {
    const poolInfo = pool(name, `strict-${name}`);
    documents.push([`${name} --json`, statusDocument(dirs, poolInfo)]);
  }
  const busy = pool('idle_pool', 'strict-agents');
  const session = claude(dirs, busy);
  const build = startProcess(dirs, 'cargo', path.join(busy.dir, 'wt3'));
  try {
    await session.run(payload('UserPromptSubmit', busy));
    await session.run(payload('PreToolUse-Bash', busy));
    await session.run(payload('PermissionRequest-Bash', busy));
    await session.run(payload('SubagentStart', busy));
    await session.run(payload('PreToolUse-Read-subagent', busy));

    // When
    const status = statusDocument(dirs, busy);
    documents.push(['agents --json', status], ['agents --watch', await firstWatchDocument(dirs, busy)]);

    // Then
    const actors = status.pools[0].worktrees.flatMap((w) => w.actors);
    assert.ok(actors.length >= 2 && actors.some((a) => a.action) && actors.some((a) => a.reason), `setup: no session, subagent and wait show: ${JSON.stringify(actors)}`);
    assert.ok(status.pools[0].worktrees.some((w) => w.builds.length), 'setup: the build in wt3 does not show');
  } finally {
    build.kill();
    await session.stop();
  }
  const problems = documents.flatMap(([name, doc]) => strictProblems(doc).map((problem) => `${name}: ${problem}`));
  assert.deepEqual(problems, [], "wtpool's documents against the oracle schema with every object closed");
});

/** Each remote URL form, and the `repository.github` it gives. */
const REMOTES = [
  ['https://github.com/acme/app.git', 'acme/app'],
  ['https://github.com/acme/app', 'acme/app'],
  ['git@github.com:acme/app.git', 'acme/app'],
  ['git@github.com:acme/app', 'acme/app'],
  ['ssh://git@github.com/acme/app.git', 'acme/app'],
  ['ssh://git@github.com/acme/app', 'acme/app'],
  ['https://github.com/acme-co/my_app.v2.git', 'acme-co/my_app.v2'],
  ['https://gitlab.com/acme/app.git', null],
  ['https://github.acme.com/acme/app.git', null],
];

/** Puts the pool's remotes back, also after a check that timed out. */
const restore = [];
test.afterEach(() => {
  for (const undo of restore.splice(0)) undo();
});

test('status__should_name_the_repository', async () => {
  // Given
  const dirs = cliSandbox('repository');
  const poolInfo = pool('idle_pool', 'repository');
  const fg = poolInfo.fg;
  const local = git(fg, ['remote', 'get-url', 'origin']).trim();
  // isolation__left_no_trace fails on a remote outside the run folder.
  const putBack = () => {
    git(fg, ['remote', 'set-url', 'origin', local]);
    if (git(fg, ['remote']).split('\n').includes('upstream')) git(fg, ['remote', 'remove', 'upstream']);
  };
  restore.push(putBack);
  const seen = [];
  // Each case reads on, so a failure shows every case's repository, or the reason wtpool printed
  // none.
  const read = (label, options) => {
    try {
      seen.push([label, statusDocument(dirs, poolInfo, options).pools[0].repository ?? 'no repository']);
    } catch (error) {
      seen.push([label, error.message.split('\n')[0]]);
    }
  };

  // When
  try {
    read(`origin ${local}`);
    for (const [url] of REMOTES) {
      git(fg, ['remote', 'set-url', 'origin', url]);
      read(`origin ${url}`);
    }
    git(fg, ['remote', 'set-url', 'origin', local]);
    git(fg, ['remote', 'add', 'upstream', 'https://github.com/acme/app.git']);
    read('WTPOOL_REMOTE=upstream', { extra: { WTPOOL_REMOTE: 'upstream' } });
    read('WTPOOL_REMOTE=nothing', { extra: { WTPOOL_REMOTE: 'nothing' } });
    read('from wt2, no WTPOOL_FOREGROUND', { cwd: path.join(poolInfo.dir, 'wt2'), drop: ['WTPOOL_FOREGROUND', 'WTPOOL_SLOT_DIR'] });
  } finally {
    putBack();
  }
  const bare = repositoryPool('repository-app', { slots: 0 });
  read('app, no remote', { cwd: bare.fg, drop: ['WTPOOL_FOREGROUND', 'WTPOOL_SLOT_DIR'] });

  // Then
  const expected = [
    [`origin ${local}`, { name: 'fg', github: null }],
    ...REMOTES.map(([url, github]) => [`origin ${url}`, { name: 'fg', github }]),
    ['WTPOOL_REMOTE=upstream', { name: 'fg', github: 'acme/app' }],
    ['WTPOOL_REMOTE=nothing', { name: 'fg', github: null }],
    ['from wt2, no WTPOOL_FOREGROUND', { name: 'fg', github: null }],
    ['app, no remote', { name: 'app', github: null }],
  ];
  assert.deepEqual(seen, expected, 'pools[0].repository for each remote');
});
