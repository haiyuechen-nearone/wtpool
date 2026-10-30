// The plugin's shape: one file that imports nothing, one CONFIG line, erasable TypeScript, and a
// setup that registers its two hooks and reads events without awaiting them.
import assert from 'node:assert/strict';
import { stripTypeScriptTypes } from 'node:module';
import test from 'node:test';

import { FakeClock, FakeContext, flush, realSleep } from './fakes.mjs';
import {
  BIN,
  bun,
  committedConfig,
  configLines,
  event,
  loadPlugin,
  pluginSource,
  recordSpawns,
  sessionId,
  toolCall,
  withoutStrayErrors,
} from './plugin.mjs';

const FG = '/sandbox/repo/fg';

/** The source with its comments blanked, strings kept. */
function withoutComments(source) {
  let out = '';
  let quote = null;
  for (let i = 0; i < source.length; i += 1) {
    const c = source[i];
    const next = source[i + 1];
    if (quote) {
      out += c;
      if (c === '\\') {
        out += next ?? '';
        i += 1;
      } else if (c === quote) quote = null;
      continue;
    }
    if (c === '"' || c === "'" || c === '`') {
      quote = c;
      out += c;
    } else if (c === '/' && next === '/') {
      while (i < source.length && source[i] !== '\n') i += 1;
      out += '\n';
    } else if (c === '/' && next === '*') {
      const end = source.indexOf('*/', i + 2);
      const comment = source.slice(i, end < 0 ? source.length : end + 2);
      out += comment.replace(/[^\n]/g, ' ');
      i += comment.length - 1;
    } else out += c;
  }
  return out;
}

/** Runs setup on a fresh plugin with `ctx`, at most 1 s of real time. */
async function setupWithin(plugin, context) {
  const done = Promise.resolve(plugin.setup(context.ctx));
  const result = await Promise.race([done, realSleep(1000).then(() => 'late')]);
  assert.notEqual(result, 'late', 'setup did not resolve within 1 s while the event stream yielded nothing');
  return result;
}

test('plugin__should_import_nothing', () => {
  // Given
  const source = pluginSource();
  const code = withoutComments(source);

  // When
  const found = [];
  for (const [pattern, what] of [
    [/\bimport\b/, 'an import'],
    [/\brequire\s*\(/, 'a require'],
    [/\bgetBuiltinModule\b/, 'a builtin module load'],
    [/['"`]node:[a-z_/]+['"`]/, 'a node: module name'],
    [/['"`]@[a-z0-9-]+\/[a-z0-9.-]+['"`]/, 'a package name'],
    [/\bfrom\s*['"`]/, 'a from clause'],
  ]) {
    const match = pattern.exec(code);
    if (match) found.push(`${what}: ${code.slice(Math.max(0, match.index - 20), match.index + 40).replace(/\s+/g, ' ')}`);
  }

  // Then
  assert.deepEqual(found, []);
  assert.equal(configLines(source).length, 1, 'the plugin declares CONFIG on more or fewer lines than one');
  const { value } = committedConfig(source);
  assert.equal(typeof value.bin, 'string', 'the CONFIG sample names no `bin`');
  assert.equal(typeof value.policy, 'boolean', 'the CONFIG sample has no boolean `policy`');
  assert.doesNotThrow(() => stripTypeScriptTypes(source, { mode: 'strip' }), 'the plugin uses TypeScript syntax Node cannot erase');
});

test('plugin__should_import_nothing as one default export', async () => {
  // Given
  const plugin = await loadPlugin();

  // Then
  assert.equal(plugin?.id, 'wtpool', 'the default export has no id `wtpool`');
  assert.equal(typeof plugin?.setup, 'function', 'the default export has no setup function');
});

test('plugin__should_register_only_its_hooks', async () => {
  // Given
  const plugin = await loadPlugin();
  const context = new FakeContext(FG);

  // When
  const cleanup = await setupWithin(plugin, context);
  await flush();

  // Then
  const registered = context.registered.map((r) => `${r.domain}.${r.name}`).sort();
  assert.deepEqual(registered, ['shell.create.before', 'tool.execute.before']);
  assert.deepEqual(context.transforms.map((t) => t.domain), [], 'the plugin registered a transform');
  assert.equal(context.subscriptions, 1, 'the plugin did not subscribe to events exactly once');
  assert.ok(context.pulls >= 1, 'the plugin does not read its event stream');
  assert.equal(typeof cleanup, 'function', 'setup returned no cleanup');
  context.end();
});

test('plugin__should_register_only_its_hooks while a registration fails', async () => {
  for (const [domain, how] of [
    ['tool', 'throw'],
    ['tool', 'reject'],
    ['shell', 'throw'],
    ['shell', 'reject'],
  ]) {
    // Given
    bun.reset();
    const plugin = await loadPlugin();
    const context = new FakeContext(FG, { refuse: { [domain]: how } });
    const session = sessionId(11);

    // When
    const stray = await withoutStrayErrors(async () => {
      await setupWithin(plugin, context);
      await context.feed(event('session.renamed', { sessionID: session, title: 'still here' }, FG));
      if (domain !== 'tool') await context.call('tool.execute.before', toolCall(session, 'call_1', 'read', { path: 'a' }));
    });

    // Then
    const others = context.registered.map((r) => `${r.domain}.${r.name}`);
    const expected = domain === 'tool' ? ['shell.create.before'] : ['tool.execute.before'];
    assert.deepEqual(others, expected, `${domain}.hook ${how}s: the other hook`);
    assert.deepEqual(stray, [], `${domain}.hook ${how}s`);
    const names = recordSpawns().map((spawn) => spawn.argv[0]);
    assert.ok(names.length >= 1 && names.every((bin) => bin === BIN), `${domain}.hook ${how}s: events stopped: ${JSON.stringify(names)}`);
    context.end();
  }
});

test('plugin__should_register_only_its_hooks and cleanup drops the maps', async () => {
  // Given
  bun.reset();
  const clock = new FakeClock().install();
  bun.clock = clock;
  try {
    const plugin = await loadPlugin();
    const context = new FakeContext(FG);
    const cleanup = await setupWithin(plugin, context);
    const session = sessionId(12);
    await context.feed(event('session.inbox.enqueued', { sessionID: session, inboxID: 'msg_1' }, FG));
    const learned = bun.spawns.length;
    await context.feed(event('session.execution.started', { sessionID: session }));
    assert.equal(recordSpawns(learned).length, 1, 'setup: a session learned from a located event is not forwarded');

    // When
    await cleanup();
    await flush();
    const from = bun.spawns.length;
    await context.feed(event('session.execution.succeeded', { sessionID: session }));

    // Then
    assert.equal(recordSpawns(from).length, 0, 'after cleanup an event without a location still found its session');
    context.end();
  } finally {
    clock.restore();
    bun.reset();
  }
});
