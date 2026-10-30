// The lookup of a session the plugin first learns without its title: OpenCode sends a folder's
// first events before that folder's instance reads them, `run --title`'s rename among them.
import assert from 'node:assert/strict';
import test from 'node:test';

import { FakeClock, flush, realSleep } from './fakes.mjs';
import { Server, bun, event, payloadOf, recordSpawns, sessionId, toolCall, withoutStrayErrors } from './plugin.mjs';

const FG = '/sandbox/repo/fg';
const TITLE = 'Given title probe';

/** OpenCode's answer for session `id`: the fields 2.2.0 gave, `title` and `parentID` as named. */
function answer(id, fields = {}) {
  return {
    id,
    projectID: '3d7fadac9684354168a1bd2b0daa4d54c10c0a11',
    cost: 0,
    tokens: { input: 0, output: 0, reasoning: 0, cache: { read: 0, write: 0 } },
    time: { created: 1793301973470, updated: 1793301973480 },
    location: { directory: FG },
    ...fields,
  };
}

/** Runs `fn` with one fg instance on a fake clock, and gives its payloads and its context. */
async function withInstance(fn, options = {}) {
  const clock = new FakeClock().install();
  bun.reset();
  bun.clock = clock;
  const server = new Server({ clock });
  try {
    const fg = await server.start('fg', FG, options);
    await fn(fg);
    await bun.settle();
    return { sent: recordSpawns().map(payloadOf), context: fg.context };
  } finally {
    server.end();
    clock.restore();
    bun.reset();
  }
}

const lookupsOf = (sent) => sent.filter((payload) => payload.kind === 'lookup');

test('plugin__should_look_up_a_session_first_seen_without_its_title', async () => {
  // Given
  const id = sessionId(31);
  const sessions = { [id]: answer(id, { title: TITLE, slug: 'wtpool-verify-slug' }) };

  // When
  const { sent, context } = await withInstance(async (fg) => {
    await fg.context.feed(event('session.inbox.enqueued', { sessionID: id, inboxID: 'msg_1' }, FG));
    await fg.context.feed(event('session.execution.started', { sessionID: id }));
  }, { sessions });

  // Then
  assert.deepEqual(context.lookups, [{ sessionID: id }], 'the lookups');
  assert.deepEqual(
    sent,
    [
      { v: 1, kind: 'lookup', name: 'session.get', directory: FG, data: { id, title: TITLE } },
      { v: 1, kind: 'event', name: 'session.execution.started', directory: FG, data: { sessionID: id } },
    ],
    'the payloads',
  );
});

test('plugin__should_look_up_a_session_first_seen_without_its_title from a hook, never awaited', async () => {
  // Given
  const id = sessionId(32);
  let release;
  const held = new Promise((resolve) => {
    release = () => resolve(answer(id, { title: TITLE }));
  });
  let returned;

  // When
  const { sent } = await withInstance(async (fg) => {
    const call = fg.context.call('tool.execute.before', toolCall(id, 'call_1', 'read', { path: 'a.txt' })).then(() => true);
    returned = await Promise.race([call, realSleep(1000).then(() => false)]);
    await bun.settle();
    assert.deepEqual(recordSpawns().map(payloadOf).map((p) => p.name), ['tool.execute.before'], 'the payloads before the answer');
    release();
    await held;
    await flush();
  }, { lookup: () => held });

  // Then
  assert.ok(returned, 'the hook waited for the lookup');
  assert.deepEqual(sent.map((p) => [p.kind, p.name]), [['hook', 'tool.execute.before'], ['lookup', 'session.get']]);
  assert.deepEqual(lookupsOf(sent)[0].data, { id, title: TITLE });
});

test('plugin__should_look_up_a_session_first_seen_without_its_title only once and not after its title', async () => {
  // Given
  const created = sessionId(33);
  const renamed = sessionId(34);
  const again = sessionId(35);
  const sessions = Object.fromEntries([created, renamed, again].map((id) => [id, answer(id, { title: TITLE })]));

  // When
  const { sent, context } = await withInstance(async (fg) => {
    await fg.context.feed(event('session.created', { sessionID: created, location: { directory: FG } }, FG));
    await fg.context.feed(event('session.renamed', { sessionID: renamed, title: 'Renamed' }, FG));
    await fg.context.feed(event('session.inbox.enqueued', { sessionID: again, inboxID: 'msg_1' }, FG));
    await fg.context.feed(event('session.inbox.delivered', { sessionID: again, inboxID: 'msg_1' }, FG));
    await fg.context.call('tool.execute.before', toolCall(again, 'call_1', 'read', { path: 'a.txt' }));
  }, { sessions });

  // Then
  assert.deepEqual(context.lookups, [{ sessionID: again }], 'the lookups');
  assert.equal(lookupsOf(sent).length, 1, 'the lookup payloads');
});

test('plugin__should_look_up_a_session_first_seen_without_its_title and send nothing without one', async () => {
  // Given
  const answers = [
    (id) => Promise.resolve(answer(id)),
    (id) => Promise.resolve(answer(id, { title: '' })),
    (id) => Promise.resolve(answer(id, { title: null })),
    () => Promise.resolve(answer(sessionId(99), { title: TITLE })),
    () => Promise.resolve(undefined),
    () => Promise.resolve('a title'),
    () => Promise.reject(new Error('wtpool-verify: no such session')),
    () => {
      throw new Error('wtpool-verify: get throws');
    },
    () => new Promise(() => {}),
  ];
  const problems = [];

  // When
  const stray = await withoutStrayErrors(async () => {
    for (const [index, give] of answers.entries()) {
      const id = sessionId(40 + index);
      const { sent } = await withInstance(async (fg) => {
        await fg.context.feed(event('session.inbox.enqueued', { sessionID: id, inboxID: 'msg_1' }, FG));
        await fg.context.feed(event('session.execution.started', { sessionID: id }));
      }, { lookup: (input) => give(input.sessionID) });
      const names = sent.map((p) => p.name);
      if (JSON.stringify(names) !== JSON.stringify(['session.execution.started'])) problems.push(`answer ${index}: sent ${JSON.stringify(names)}`);
    }
  });

  // Then
  assert.deepEqual(problems, []);
  assert.deepEqual(stray, []);
});

test('plugin__should_look_up_a_session_first_seen_without_its_title with the root of a child', async () => {
  // Given
  const root = sessionId(51);
  const child = sessionId(52);
  const sessions = { [child]: answer(child, { title: 'Read slot two', parentID: root }) };

  // When
  const { sent } = await withInstance(async (fg) => {
    await fg.context.feed(event('session.created', { sessionID: root, location: { directory: FG } }, FG));
    await fg.context.feed(event('session.inbox.enqueued', { sessionID: child, inboxID: 'msg_2' }, FG));
  }, { sessions });

  // Then
  assert.deepEqual(lookupsOf(sent), [
    { v: 1, kind: 'lookup', name: 'session.get', directory: FG, parent: root, data: { id: child, parentID: root, title: 'Read slot two' } },
  ]);
});

test('plugin__should_look_up_a_session_first_seen_without_its_title and send nothing after cleanup', async () => {
  // Given
  const id = sessionId(53);
  let release;
  const held = new Promise((resolve) => {
    release = () => resolve(answer(id, { title: TITLE }));
  });

  // When
  const { sent } = await withInstance(async (fg) => {
    await fg.context.feed(event('session.inbox.enqueued', { sessionID: id, inboxID: 'msg_1' }, FG));
    await fg.cleanup();
    release();
    await held;
    await flush();
  }, { lookup: () => held });

  // Then
  assert.deepEqual(lookupsOf(sent), []);
});

test('plugin__should_look_up_a_session_first_seen_without_its_title and drop the answer after its deletion', async () => {
  // Given
  const id = sessionId(54);
  let release;
  const held = new Promise((resolve) => {
    release = () => resolve(answer(id, { title: TITLE }));
  });

  // When
  const { sent } = await withInstance(async (fg) => {
    await fg.context.feed(event('session.inbox.enqueued', { sessionID: id, inboxID: 'msg_1' }, FG));
    await fg.context.feed(event('session.deleted', { sessionID: id }));
    release();
    await held;
    await flush();
  }, { lookup: () => held });

  // Then
  assert.deepEqual(sent.map((p) => [p.kind, p.name]), [['event', 'session.deleted']]);
});

test('plugin__should_look_up_a_session_first_seen_without_its_title but not one first seen in its deletion', async () => {
  // Given
  const id = sessionId(55);
  const sessions = { [id]: answer(id, { title: TITLE }) };

  // When
  const { sent, context } = await withInstance(async (fg) => {
    await fg.context.feed(event('session.deleted', { sessionID: id }, FG));
  }, { sessions });

  // Then
  assert.deepEqual(context.lookups, [], 'the lookups');
  assert.deepEqual(sent.map((p) => [p.kind, p.name]), [['event', 'session.deleted']]);
});
