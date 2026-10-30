// What the plugin forwards from the recorded OpenCode 2.2.0 streams: once per process, only the
// kept fields, and exactly the frozen forwarded/ fixtures.
import assert from 'node:assert/strict';
import test from 'node:test';

import { FakeClock } from './fakes.mjs';
import {
  DELTAS,
  KEPT,
  KINDS,
  PAYLOAD_KEYS,
  Server,
  bun,
  event,
  forwarded,
  instanceFolders,
  leaves,
  payloadOf,
  rawLines,
  recordSpawns,
  replay,
  sessionId,
  streams,
  toolCall,
} from './plugin.mjs';

const TWO_FOLDERS = '10-two-folders';

/** JSON text with sorted keys, so two values compare as JSON values. */
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
      .join(',')}}`;
  }
  return JSON.stringify(value) ?? 'undefined';
}

/** The session an event's data names, by its row's field. */
function sessionOf(payload) {
  const data = payload?.data ?? {};
  return data.sessionID ?? data.form?.sessionID;
}

test('plugin__should_forward_each_event_once_per_process', async () => {
  // Given
  const lines = rawLines(TWO_FOLDERS);
  const folders = instanceFolders();
  const instances = [...new Set(lines.map((line) => line.instance))];
  assert.equal(instances.length, 3, `${TWO_FOLDERS} records ${instances.length} instances, not 3`);

  // When
  const { spawns } = await replay(TWO_FOLDERS);

  // Then
  const byEvent = new Map();
  for (const spawn of spawns) {
    const line = lines[spawn.line - 1];
    if (line.kind !== 'event' || spawn.payload.kind !== 'event') continue;
    const id = line.payload.id;
    byEvent.set(id, [...(byEvent.get(id) ?? []), { instance: line.instance, payload: spawn.payload }]);
  }
  // The instances that may forward each event: the one of its folder, or for an event without a
  // location each instance that learned its session from a located event of its own folder or a
  // hook before it.
  const learned = new Map(instances.map((instance) => [instance, new Set()]));
  const owners = new Map();
  const problems = [];
  for (const line of lines) {
    const own = folders.get(line.instance);
    if (line.kind === 'hook' && line.name === 'tool.execute.before') learned.get(line.instance).add(line.payload.sessionID);
    if (line.kind !== 'event') continue;
    const located = line.payload.location?.directory;
    const session = sessionOf(line.payload);
    const may = located === undefined ? learned.get(line.instance).has(session) : located === own;
    if (may) owners.set(line.payload.id, [...(owners.get(line.payload.id) ?? []), line.instance]);
    if (located === own && session) learned.get(line.instance).add(session);
  }
  const seen = new Set();
  for (const line of lines) {
    if (line.kind !== 'event' || seen.has(line.payload.id)) continue;
    seen.add(line.payload.id);
    const type = line.payload.type;
    const sent = byEvent.get(line.payload.id) ?? [];
    if (!Object.hasOwn(KEPT, type)) {
      if (sent.length) problems.push(`${type} ${line.payload.id} is not in the event table, yet ${sent.length} instances forwarded it`);
      continue;
    }
    const may = owners.get(line.payload.id) ?? [];
    if (may.length > 1) problems.push(`setup: ${type} ${line.payload.id} has ${may.length} owning instances`);
    const optional = DELTAS.includes(type) || type === 'session.step.started';
    const expected = may.length ? 1 : 0;
    if (sent.length > expected || (!optional && sent.length !== expected)) {
      problems.push(`${type} ${line.payload.id} spawned ${sent.length} times across the instances, not ${expected}`);
      continue;
    }
    if (!sent.length) continue;
    if (!may.includes(sent[0].instance)) {
      problems.push(`${type} ${line.payload.id} was forwarded by the instance of ${folders.get(sent[0].instance)}, which does not own it`);
    }
    if (sent[0].payload.directory !== folders.get(sent[0].instance)) {
      problems.push(`${type} ${line.payload.id}: directory ${sent[0].payload.directory}, not the instance's ${folders.get(sent[0].instance)}`);
    }
  }
  const counted = [...byEvent.values()].filter((sent) => sent.length === 1).length;
  assert.ok(counted >= 5, `only ${counted} events of ${TWO_FOLDERS} were forwarded`);
  assert.deepEqual(problems, []);
});

test('plugin__should_forward_each_event_once_per_process for another folder and unknown sessions', async () => {
  // Given
  const clock = new FakeClock().install();
  bun.reset();
  bun.clock = clock;
  const server = new Server({ clock });
  try {
    const fg = await server.start('fg', '/sandbox/repo/fg');
    const wt1 = await server.start('wt1', '/sandbox/repo/fg-wt1');
    const mine = sessionId(1);
    const theirs = sessionId(2);
    const stranger = sessionId(3);

    // When
    const steps = [
      ['a located event of wt1', event('session.renamed', { sessionID: theirs, title: 'theirs' }, '/sandbox/repo/fg-wt1'), 1],
      ['an unlisted located event of fg', event('session.inbox.enqueued', { sessionID: mine, inboxID: 'msg_1' }, '/sandbox/repo/fg'), 0],
      ['an event without a location of a session fg learned', event('session.execution.started', { sessionID: mine }), 1],
      ['an event without a location of a session wt1 learned', event('session.execution.succeeded', { sessionID: theirs }), 1],
      ['an event without a location of an unknown session', event('session.execution.started', { sessionID: stranger }), 0],
      ['a located event of a folder no instance serves', event('session.renamed', { sessionID: stranger, title: 'x' }, '/sandbox/repo/fg-wt2'), 0],
      ['session.deleted of an unknown session', event('session.deleted', { sessionID: stranger }), 0],
    ];
    const problems = [];
    for (const [what, envelope, expected] of steps) {
      const from = bun.spawns.length;
      for (const instance of [fg, wt1]) await instance.context.feed(structuredClone(envelope));
      const count = recordSpawns(from).length;
      if (count !== expected) problems.push(`${what}: ${count} spawns across both instances, not ${expected}`);
    }

    // Then
    assert.deepEqual(problems, []);
  } finally {
    server.end();
    clock.restore();
    bun.reset();
  }
});

test('plugin__should_keep_only_the_listed_fields', async () => {
  // Given
  const names = streams();
  assert.ok(names.length, 'no recorded stream');
  const problems = [];

  // When
  for (const name of names) {
    const { spawns } = await replay(name);
    for (const { line, payload } of spawns) {
      const where = `${name}:${line}`;
      const extra = Object.keys(payload).filter((key) => !PAYLOAD_KEYS.includes(key));
      if (extra.length) problems.push(`${where}: keys outside the payload's ${extra.join(', ')}`);
      if (payload.v !== 1) problems.push(`${where}: v ${JSON.stringify(payload.v)}`);
      const kept = KEPT[payload.name];
      if (!kept) {
        problems.push(`${where}: forwarded ${payload.name}, which the event table does not list`);
        continue;
      }
      const kind = KINDS[payload.name] ?? 'event';
      if (payload.kind !== kind) problems.push(`${where}: ${payload.name} as kind ${payload.kind}, not ${kind}`);
      const outside = leaves(payload.data).filter((leaf) => !kept.includes(leaf));
      if (outside.length) problems.push(`${where}: ${payload.name} data holds ${outside.join(', ')}`);
    }
  }

  // Then
  assert.deepEqual(problems.slice(0, 40), []);
});

test('plugin__should_keep_only_the_listed_fields over 1 MiB', async () => {
  // Given
  const clock = new FakeClock().install();
  bun.reset();
  bun.clock = clock;
  const server = new Server({ clock });
  try {
    const fg = await server.start('fg', '/sandbox/repo/fg');
    const session = sessionId(7);
    const big = 'x'.repeat(1_100_000);
    const near = 'y'.repeat(1_000_000);

    // When
    const from = bun.spawns.length;
    await fg.context.call('tool.execute.before', toolCall(session, 'call_big', 'shell', { command: big, description: 'big' }));
    await fg.context.feed(event('session.renamed', { sessionID: session, title: big }, '/sandbox/repo/fg'));
    const over = recordSpawns(from).length;
    const before = bun.spawns.length;
    await fg.context.call('tool.execute.before', toolCall(session, 'call_near', 'read', { path: near }));
    await bun.settle();
    const under = recordSpawns(before).map(payloadOf);

    // Then
    assert.equal(over, 0, 'a payload over 1 MiB was sent');
    assert.equal(under.length, 1, 'a payload under 1 MiB was not sent');
    assert.equal(under[0].data.input.path.length, near.length, 'the path under 1 MiB was cut');
  } finally {
    server.end();
    clock.restore();
    bun.reset();
  }
});

test('plugin__should_keep_only_the_listed_fields never env, output, text, a prompt or an answer', async () => {
  // Given
  const clock = new FakeClock().install();
  bun.reset();
  bun.clock = clock;
  const server = new Server({ clock });
  const secret = 'wtpool-verify-secret-0b1f';
  try {
    const fg = await server.start('fg', '/sandbox/repo/fg');
    const session = sessionId(8);
    const at = '/sandbox/repo/fg';

    // When
    await fg.context.call('tool.execute.before', toolCall(session, 'call_1', 'write', { path: 'a.txt', content: secret }));
    await fg.context.call('shell.create.before', { command: 'ls', cwd: at, timeout: 1, shell: '/bin/zsh', env: { OPENCODE_SERVER_PASSWORD: secret } });
    await fg.context.feed(event('session.text.delta', { sessionID: session, delta: secret, ordinal: 0, assistantMessageID: 'msg_1' }, at));
    await fg.context.feed(event('session.tool.success', { sessionID: session, id: 'call_1', content: [{ type: 'text', text: secret }], metadata: { output: secret } }, at));
    await fg.context.feed(event('form.replied', { sessionID: session, id: 'frm_1', answer: { q0: secret } }, at));
    await fg.context.feed(event('session.execution.failed', { sessionID: session, error: { type: 'provider.auth', message: secret, response: { body: secret } } }));
    await fg.context.feed(event('session.created', { sessionID: sessionId(9), parentID: session, title: 'child', agent: 'explore', slug: secret, location: { directory: at } }, at));
    await bun.settle();

    // Then
    const leaked = bun.spawns.filter((spawn) => String(spawn.stdin).includes(secret)).map((spawn) => spawn.stdin.slice(0, 200));
    assert.deepEqual(leaked, []);
    assert.ok(recordSpawns().length >= 5, `only ${recordSpawns().length} payloads were sent`);
  } finally {
    server.end();
    clock.restore();
    bun.reset();
  }
});

test('plugin__should_forward_the_recorded_streams_as_frozen', async () => {
  // Given
  const names = streams();
  assert.ok(names.length, 'no recorded stream');
  const problems = [];

  // When
  for (const name of names) {
    const expected = forwarded(name);
    const { spawns } = await replay(name);
    const found = spawns.map(({ line, t, payload }) => ({ line, t, payload }));
    if (canonical(found) === canonical(expected)) continue;
    const at = found.findIndex((item, i) => canonical(item) !== canonical(expected[i]));
    const first = at < 0 ? Math.min(found.length, expected.length) : at;
    problems.push(
      `${name}: ${found.length} spawns, ${expected.length} frozen; first difference at spawn ${first + 1}:\n` +
        `  found  ${JSON.stringify(found[first] ?? null)}\n  frozen ${JSON.stringify(expected[first] ?? null)}`,
    );
  }

  // Then
  assert.equal(problems.length, 0, problems.join('\n'));
});

