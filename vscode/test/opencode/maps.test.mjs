// The plugin's per-instance state: the progress throttle, the model sent last, each child's root
// session, each reply's call, and the bounds of its maps.
import assert from 'node:assert/strict';
import test from 'node:test';

import { FakeClock } from './fakes.mjs';
import { DELTAS, Server, bun, event, payloadOf, rawLines, recordSpawns, replay, sessionId, toolCall } from './plugin.mjs';

const FG = '/sandbox/repo/fg';
const LONG_ANSWER = '07-long-answer';

/** Runs `fn` with one fg instance on a fake clock, and gives the payloads it sent. */
async function withInstance(fn, options = {}) {
  const clock = new FakeClock().install();
  bun.reset();
  bun.clock = clock;
  const server = new Server({ clock });
  try {
    const fg = await server.start('fg', FG, options);
    await fn(fg, clock);
    await bun.settle();
    return recordSpawns().map(payloadOf);
  } finally {
    server.end();
    clock.restore();
    bun.reset();
  }
}

/** The payloads sent while `fn` runs. */
async function sentBy(fn) {
  const from = bun.spawns.length;
  await fn();
  await bun.settle();
  return recordSpawns(from).map(payloadOf);
}

test('plugin__should_send_one_progress_per_session_per_10_s', async () => {
  // Given
  const lines = rawLines(LONG_ANSWER);
  const deltas = lines.filter((line) => line.kind === 'event' && DELTAS.includes(line.payload.type));
  assert.ok(deltas.length >= 100, `setup: ${LONG_ANSWER} records ${deltas.length} deltas`);
  const expected = [];
  const last = new Map();
  for (const line of deltas) {
    const session = line.payload.data.sessionID;
    if (last.has(session) && line.t - last.get(session) < 10_000) continue;
    last.set(session, line.t);
    expected.push(line.t);
  }

  // When
  const { spawns } = await replay(LONG_ANSWER);

  // Then
  const sent = spawns.filter((s) => DELTAS.includes(s.payload.name));
  assert.deepEqual(sent.map((s) => s.t), expected, 'the progress times');
  assert.equal(sent[0].t, deltas[0].t, 'the first progress did not go out at once');
  for (const { payload } of sent) {
    assert.deepEqual(Object.keys(payload.data), ['sessionID'], `a progress carries ${Object.keys(payload.data)}`);
  }
});

test('plugin__should_send_one_progress_per_session_per_10_s per session apart', async () => {
  const a = sessionId(21);
  const b = sessionId(22);
  const times = [];
  // Given
  const sent = await withInstance(async (fg, clock) => {
    const delta = async (type, session) => {
      const got = await sentBy(() => fg.context.feed(event(type, { sessionID: session, assistantMessageID: 'msg_1', ordinal: 0, delta: 'word ' }, FG)));
      times.push(...got.map((p) => `${clock.now - start} ${p.data.sessionID === a ? 'a' : 'b'} ${p.name}`));
    };
    const start = clock.now;

    // When
    await delta('session.text.delta', a);
    await clock.advance(1_000);
    await delta('session.text.delta', b);
    await delta('session.reasoning.delta', a);
    await clock.advance(8_999);
    await delta('session.tool.input.delta', a);
    await clock.advance(1);
    await delta('session.tool.input.delta', a);
    await delta('session.text.delta', b);
    await clock.advance(1_000);
    await delta('session.text.delta', b);
  });

  // Then
  assert.deepEqual(times, ['0 a session.text.delta', '1000 b session.text.delta', '10000 a session.tool.input.delta', '11000 b session.text.delta']);
  assert.equal(sent.length, 4);
});

test('plugin__should_send_one_progress_per_session_per_10_s and a model only when it changes', async () => {
  const a = sessionId(23);
  const b = sessionId(24);
  const step = (session, providerID, id) =>
    event('session.step.started', { sessionID: session, agent: 'build', assistantMessageID: 'msg_1', snapshot: 'x', started: 1, model: { id, providerID } }, FG);
  // Given
  const sent = await withInstance(async (fg) => {
    // When
    for (const envelope of [
      step(a, 'stub', 'scripted'),
      step(a, 'stub', 'scripted'),
      step(a, 'opencode', 'scripted'),
      step(a, 'opencode', 'glm-5.3-flash-free'),
      step(a, 'opencode', 'glm-5.3-flash-free'),
      step(b, 'stub', 'scripted'),
    ]) {
      await fg.context.feed(envelope);
    }
  });

  // Then
  const models = sent.map((p) => `${p.data.sessionID === a ? 'a' : 'b'} ${p.data.model?.providerID}/${p.data.model?.id}`);
  assert.deepEqual(models, ['a stub/scripted', 'a opencode/scripted', 'a opencode/glm-5.3-flash-free', 'b stub/scripted']);
});

test('plugin__should_attach_the_root_and_the_call', async () => {
  const root = sessionId(31);
  const child = sessionId(32);
  const grandchild = sessionId(33);
  // Given
  const sent = await withInstance(async (fg) => {
    const created = (session, parentID) =>
      event('session.created', { sessionID: session, parentID, title: `of ${parentID}`, agent: 'explore', location: { directory: FG } }, FG);

    // When
    await fg.context.feed(event('session.inbox.enqueued', { sessionID: root, inboxID: 'msg_1' }, FG));
    await fg.context.feed(created(child, root));
    await fg.context.feed(created(grandchild, child));
    await fg.context.call('tool.execute.before', toolCall(grandchild, 'call_g', 'read', { path: 'a.txt' }));
    await fg.context.feed(event('session.tool.success', { sessionID: grandchild, id: 'call_g', executed: true }, FG));
    await fg.context.feed(event('session.execution.succeeded', { sessionID: child }));
    await fg.context.feed(event('session.renamed', { sessionID: root, title: 'root' }, FG));
    await fg.context.feed(event('permission.asked', { id: 'per_1', sessionID: child, action: 'shell', resources: ['ls'], save: [], source: { type: 'tool', messageID: 'msg_1', id: 'call_p' } }, FG));
    await fg.context.feed(event('form.created', { form: { id: 'frm_1', sessionID: root, title: 'Questions', metadata: { kind: 'question', tool: { messageID: 'msg_1', id: 'call_q' } }, fields: [] } }, FG));
    await fg.context.feed(event('permission.replied', { sessionID: child, requestID: 'per_1', reply: 'once' }, FG));
    await fg.context.feed(event('form.replied', { sessionID: root, id: 'frm_1', answer: { q0: 'Orange' } }, FG));
    await fg.context.feed(event('permission.replied', { sessionID: child, requestID: 'per_1', reply: 'once' }, FG));
    await fg.context.feed(event('permission.replied', { sessionID: root, requestID: 'per_unknown', reply: 'reject' }, FG));
    await fg.context.feed(event('form.replied', { sessionID: root, id: 'frm_unknown', answer: {} }, FG));
  });

  // Then
  const view = sent.map((p) => [p.name, p.data.sessionID ?? p.data.form?.sessionID, p.parent ?? null, p.call ?? null]);
  assert.deepEqual(view, [
    ['session.created', child, root, null],
    ['session.created', grandchild, root, null],
    ['tool.execute.before', grandchild, root, null],
    ['session.tool.success', grandchild, root, null],
    ['session.execution.succeeded', child, root, null],
    ['session.renamed', root, null, null],
    ['permission.asked', child, root, null],
    ['form.created', root, null, null],
    ['permission.replied', child, root, 'call_p'],
    ['form.replied', root, null, 'call_q'],
    ['permission.replied', child, root, null],
    ['permission.replied', root, null, null],
    ['form.replied', root, null, null],
  ]);
});

test('plugin__should_attach_the_root_and_the_call within each bound', async () => {
  const problems = [];
  // Given
  await withInstance(async (fg) => {
    // When: 1025 sessions learned, the first seen again before the last, so the second is the least recently seen.
    for (let n = 0; n < 1025; n += 1) {
      if (n === 1024) await fg.context.feed(event('session.inbox.enqueued', { sessionID: sessionId(1000), inboxID: 'msg_1' }, FG));
      await fg.context.feed(event('session.inbox.enqueued', { sessionID: sessionId(1000 + n), inboxID: 'msg_1' }, FG));
    }
    for (const [n, expected] of [
      [0, 1],
      [1, 0],
      [2, 1],
      [1024, 1],
    ]) {
      const got = await sentBy(() => fg.context.feed(event('session.execution.started', { sessionID: sessionId(1000 + n) })));
      if (got.length !== expected) problems.push(`session ${n} of 1025: ${got.length} payloads, not ${expected}`);
    }
    // 257 permissions and 257 forms pending: the oldest of each is dropped.
    const asker = sessionId(2000);
    await fg.context.feed(event('session.inbox.enqueued', { sessionID: asker, inboxID: 'msg_1' }, FG));
    for (let n = 0; n < 257; n += 1) {
      await fg.context.feed(event('permission.asked', { id: `per_${n}`, sessionID: asker, action: 'shell', source: { type: 'tool', messageID: 'msg_1', id: `call_p${n}` } }, FG));
      await fg.context.feed(event('form.created', { form: { id: `frm_${n}`, sessionID: asker, metadata: { kind: 'question', tool: { messageID: 'msg_1', id: `call_f${n}` } } } }, FG));
    }
    for (const [n, call] of [
      [0, undefined],
      [1, 'call_p1'],
      [256, 'call_p256'],
    ]) {
      const [reply] = await sentBy(() => fg.context.feed(event('permission.replied', { sessionID: asker, requestID: `per_${n}`, reply: 'once' }, FG)));
      if (reply?.call !== call) problems.push(`permission ${n} of 257: call ${reply?.call}, not ${call}`);
    }
    for (const [n, call] of [
      [0, undefined],
      [1, 'call_f1'],
      [256, 'call_f256'],
    ]) {
      const [reply] = await sentBy(() => fg.context.feed(event('form.replied', { sessionID: asker, id: `frm_${n}`, answer: {} }, FG)));
      if (reply?.call !== call) problems.push(`form ${n} of 257: call ${reply?.call}, not ${call}`);
    }
  });

  // Then
  assert.deepEqual(problems, []);
});
