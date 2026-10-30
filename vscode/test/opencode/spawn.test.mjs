// How the plugin runs wtpool: the record path never awaits or throws, the deny throws only on a
// deny line of its own policy command, and a shell call gets WTPOOL_SESSION only when it binds.
import assert from 'node:assert/strict';
import test from 'node:test';

import { FakeClock, flush, realNow, realSleep } from './fakes.mjs';
import {
  BIN,
  POLICY_ARGS,
  RECORD_ARGS,
  Server,
  bun,
  event,
  payloadOf,
  policySpawns,
  recordSpawns,
  sessionId,
  shellCreate,
  toolCall,
  withoutStrayErrors,
} from './plugin.mjs';

const FG = '/sandbox/repo/fg';

/** Runs `fn` with one fg instance of a plugin with `policy`, on a fake clock. */
async function withInstance(fn, { policy = false } = {}) {
  const clock = new FakeClock().install();
  bun.reset();
  bun.clock = clock;
  const server = new Server({ policy, clock });
  try {
    const fg = await server.start('fg', FG);
    return await fn(fg, clock);
  } finally {
    server.end();
    clock.restore();
    bun.reset();
  }
}

/** Settles `promise` within `ms` of real time: `{state: 'resolved'|'rejected'|'pending', value}`. */
async function within(promise, ms) {
  const late = Symbol('late');
  try {
    const value = await Promise.race([promise, realSleep(ms).then(() => late)]);
    return value === late ? { state: 'pending' } : { state: 'resolved', value };
  } catch (error) {
    return { state: 'rejected', value: error };
  }
}

/** Waits until a {@link track}ed promise settles, at most `ms` of real time. */
async function settled(status, ms) {
  const deadline = realNow() + ms;
  while (status.state === 'pending' && realNow() < deadline) await realSleep(5);
}

/** Tracks `promise` without awaiting it. */
function track(promise) {
  const status = { state: 'pending' };
  Promise.resolve(promise).then(
    (value) => Object.assign(status, { state: 'resolved', value }),
    (value) => Object.assign(status, { state: 'rejected', value }),
  );
  return status;
}

const CHILDREN = [
  ['a spawn that throws', { throws: true }],
  ['a child that hangs', { hang: true }],
  ['a child that exits 1', { code: 1, stderr: 'wtpool: error\n' }],
  ['a child that refuses its stdin', { refuseStdin: true, hang: true }],
  ['a child that dies of a signal', { signal: 'SIGKILL' }],
];

test('plugin__should_never_await_or_throw_on_the_record_path', async () => {
  for (const [what, behaviour] of CHILDREN) {
    // Given
    const files = [];
    const fetches = [];
    const realFetch = globalThis.fetch;
    const realBuiltin = process.getBuiltinModule;
    globalThis.fetch = (...args) => {
      fetches.push(String(args[0]));
      return Promise.reject(new Error('wtpool-verify: no network'));
    };
    process.getBuiltinModule = (name) => {
      files.push(name);
      return realBuiltin.call(process, name);
    };
    try {
      const stray = await withoutStrayErrors(() =>
        withInstance(async (fg) => {
          bun.behave = () => behaviour;
          const session = sessionId(41);
          const hooks = [];

          // When
          await fg.context.feed(event('session.inbox.enqueued', { sessionID: session, inboxID: 'msg_1' }, FG));
          for (const [n, tool] of ['read', 'shell', 'edit'].entries()) {
            hooks.push(await within(fg.context.call('tool.execute.before', toolCall(session, `call_${n}`, tool, { path: 'a.txt', command: 'ls' })), 500));
          }
          hooks.push(await within(Promise.resolve(fg.context.call('shell.create.before', shellCreate('ls', FG))), 500));
          await fg.context.feed(event('session.tool.success', { sessionID: session, id: 'call_0', executed: true }, FG));
          await fg.context.feed(event('session.execution.succeeded', { sessionID: session }));
          await fg.context.feed(event('session.renamed', { sessionID: session, title: 'after' }, FG));

          // Then
          for (const hook of hooks) assert.equal(hook.state, 'resolved', `${what}: a hook ${hook.state}: ${hook.value?.message ?? ''}`);
          const spawned = bun.spawns;
          assert.ok(spawned.length >= 6, `${what}: events stopped flowing after ${spawned.length} spawns`);
          for (const spawn of spawned) {
            assert.deepEqual(spawn.argv, [BIN, ...RECORD_ARGS], `${what}: a spawn's argv`);
            assert.ok(['ignore', null].includes(spawn.options?.stdout), `${what}: stdout ${spawn.options?.stdout}, not ignored`);
            assert.ok(['ignore', null].includes(spawn.options?.stderr), `${what}: stderr ${spawn.options?.stderr}, not ignored`);
            assert.notEqual(spawn.options?.stdin, undefined, `${what}: a spawn got no stdin`);
          }
          if (!behaviour.throws && !behaviour.refuseStdin) {
            await bun.settle();
            for (const spawn of spawned) assert.equal(payloadOf(spawn).v, 1, `${what}: a payload`);
          }
        }),
      );
      assert.deepEqual(stray, [], what);
      assert.deepEqual(files, [], `${what}: the plugin loaded a builtin module`);
      assert.deepEqual(fetches, [], `${what}: the plugin fetched`);
      assert.deepEqual(bun.files, [], `${what}: the plugin opened a file`);
    } finally {
      globalThis.fetch = realFetch;
      process.getBuiltinModule = realBuiltin;
    }
  }
});

/** What a policy child does in each case, and whether the call then runs. */
const POLICY_CASES = [
  ['a deny line', { stdout: '{"decision":"deny","reason":"wtpool: cargo test waits for a job slot"}\n' }, 'wtpool: cargo test waits for a job slot'],
  ['a deny line without a newline', { stdout: '{"decision":"deny","reason":"denied: x"}' }, 'denied: x'],
  ['a deny line after 1.5 s', { stdout: '{"decision":"deny","reason":"slow deny"}\n', delayMs: 1500 }, 'slow deny'],
  ['empty stdout', { stdout: '' }, null],
  ['garbage', { stdout: 'not json\n' }, null],
  ['two lines', { stdout: '{"decision":"deny","reason":"a"}\n{"decision":"deny","reason":"b"}\n' }, null],
  ['allow', { stdout: '{"decision":"allow","reason":"fine"}\n' }, null],
  ['a deny on stderr', { stdout: '', stderr: '{"decision":"deny","reason":"stderr"}\n' }, null],
  ['a crash', { stdout: '{"decision":"de', signal: 'SIGABRT' }, null],
  ['an exit code', { stdout: '{"decision":"deny","reason":"exit 2"}\n', code: 2 }, null],
  ['a missing binary', { throws: true }, null],
];

test('plugin__should_deny_only_on_a_deny_line', async () => {
  for (const [what, behaviour, reason] of POLICY_CASES) {
    // Given
    const stray = await withoutStrayErrors(() =>
      withInstance(
        async (fg, clock) => {
          bun.behave = (argv) => (argv.includes('policy') ? behaviour : {});
          const session = sessionId(51);
          const call = toolCall(session, 'call_1', 'shell', { command: 'cargo test', description: 'tests' });

          // When
          const status = track(fg.context.call('tool.execute.before', call));
          await flush();
          await clock.advance(behaviour.delayMs ?? 0);
          await settled(status, 3000);

          // Then
          await bun.settle();
          const record = recordSpawns();
          const policy = policySpawns();
          assert.equal(record.length, 1, `${what}: ${record.length} record spawns, not the call's one`);
          assert.equal(payloadOf(record[0]).name, 'tool.execute.before', `${what}: the record spawn`);
          assert.equal(policy.length, 1, `${what}: ${policy.length} policy commands`);
          assert.ok(bun.spawns.indexOf(record[0]) < bun.spawns.indexOf(policy[0]), `${what}: the policy ran before the call_start was sent`);
          assert.deepEqual(policy[0].argv, [BIN, ...POLICY_ARGS], `${what}: the policy argv`);
          if (!behaviour.throws) {
            const input = JSON.parse(policy[0].stdin);
            assert.equal(input.tool, 'shell', `${what}: the policy input's tool`);
            assert.equal(input.sessionID, session, `${what}: the policy input's sessionID`);
            assert.equal(input.id, 'call_1', `${what}: the policy input's id`);
            assert.equal(input.directory, FG, `${what}: the policy input's directory`);
            assert.equal(input.input?.command ?? input.command, 'cargo test', `${what}: the policy input's command`);
          }
          if (reason === null) {
            assert.equal(status.state, 'resolved', `${what}: the hook ${status.state} ${status.value?.message ?? ''}`);
          } else {
            assert.equal(status.state, 'rejected', `${what}: the hook ${status.state}, not thrown`);
            assert.ok(status.value instanceof Error, `${what}: the hook threw no Error`);
            assert.equal(status.value.message, reason, `${what}: the thrown message`);
          }
        },
        { policy: true },
      ),
    );
    assert.deepEqual(stray, [], what);
  }
});

test('plugin__should_deny_only_on_a_deny_line once 2 s pass', async () => {
  // Given
  await withInstance(
    async (fg, clock) => {
      bun.behave = (argv) => (argv.includes('policy') ? { hang: true } : {});
      const status = track(fg.context.call('tool.execute.before', toolCall(sessionId(52), 'call_1', 'shell', { command: 'sleep 100' })));

      // When
      await flush();
      await clock.advance(1000);
      await realSleep(50);
      const early = status.state;
      await clock.advance(1100);
      await settled(status, 3000);

      // Then
      assert.equal(early, 'pending', 'the hook ended before the policy had 2 s');
      assert.equal(status.state, 'resolved', `the hook ${status.state} once 2 s passed`);
      assert.equal(policySpawns()[0]?.killed, true, 'the policy command still runs after 2 s');
    },
    { policy: true },
  );
});

test('plugin__should_deny_only_on_a_deny_line and a denied call binds no shell', async () => {
  // Given
  await withInstance(
    async (fg) => {
      const denied = sessionId(53);
      const other = sessionId(54);
      for (const session of [denied, other]) await fg.context.feed(event('session.inbox.enqueued', { sessionID: session, inboxID: 'msg_1' }, FG));
      const deny = { stdout: '{"decision":"deny","reason":"no"}\n' };
      bun.behave = (argv) => (argv.includes('policy') && policySpawns().length === 0 ? deny : {});

      // When
      const first = await within(fg.context.call('tool.execute.before', toolCall(denied, 'call_1', 'shell', { command: 'cargo test' })), 3000);
      const second = await within(fg.context.call('tool.execute.before', toolCall(other, 'call_2', 'shell', { command: 'cargo test' })), 3000);
      const input = shellCreate('cargo test', FG);
      await fg.context.call('shell.create.before', input);

      // Then
      assert.equal(first.state, 'rejected', 'the first call was not denied');
      assert.equal(second.state, 'resolved', 'the second call was denied');
      assert.equal(input.env.WTPOOL_SESSION, `opencode-${other.replace('_', '-')}`, 'the denied call still pends');
    },
    { policy: true },
  );
});

test('plugin__should_deny_only_on_a_deny_line and starts no policy without --policy', async () => {
  // Given
  await withInstance(async (fg) => {
    bun.behave = (argv) => (argv.includes('policy') ? { stdout: '{"decision":"deny","reason":"no"}\n' } : {});

    // When
    const hook = await within(fg.context.call('tool.execute.before', toolCall(sessionId(55), 'call_1', 'shell', { command: 'cargo test' })), 1000);

    // Then
    assert.equal(hook.state, 'resolved', 'the call did not run');
    assert.equal(policySpawns().length, 0, 'a policy command started with policy false');
  });
});

/** The stem the plugin sets for a session id of OpenCode's own form. */
const stem = (id) => `opencode-${id.replace('_', '-')}`;

test('plugin__should_set_wtpool_session_for_its_own_shell_calls', async () => {
  const root = sessionId(61);
  const child = sessionId(62);
  const grandchild = sessionId(63);
  const second = sessionId(64);
  const cases = [];
  // Given
  await withInstance(async (fg, clock) => {
    await fg.context.feed(event('session.inbox.enqueued', { sessionID: root, inboxID: 'msg_1' }, FG));
    await fg.context.feed(event('session.inbox.enqueued', { sessionID: second, inboxID: 'msg_1' }, FG));
    for (const [session, parentID] of [
      [child, root],
      [grandchild, child],
    ]) {
      await fg.context.feed(event('session.created', { sessionID: session, parentID, title: 'sub', agent: 'explore', location: { directory: FG } }, FG));
    }
    const shell = async (what, calls, command, inherited) => {
      for (const [session, id] of calls) await fg.context.call('tool.execute.before', toolCall(session, id, 'shell', { command }));
      const input = shellCreate(command, FG, inherited === undefined ? {} : { WTPOOL_SESSION: inherited });
      const from = bun.spawns.length;
      await fg.context.call('shell.create.before', input);
      const value = input.env.WTPOOL_SESSION;
      cases.push([what, value === undefined ? null : value, recordSpawns(from).length]);
    };

    // When
    await shell('a root call', [[root, 'call_1']], 'ls', undefined);
    await shell("a child's call", [[child, 'call_2']], 'git status', 'opencode-ses-forged');
    await shell("a grandchild's call", [[grandchild, 'call_3']], 'pwd', undefined);
    await shell('two roots pend the command', [[root, 'call_4'], [second, 'call_5']], 'make', 'opencode-ses-forged');
    await shell('none pends', [], 'whoami', 'opencode-ses-forged');
    await shell('an id of another form', [['session.with.dots', 'call_6']], 'date', 'opencode-ses-forged');
    await shell('an id over 64 characters', [[`ses_${'a'.repeat(61)}`, 'call_7']], 'uname', undefined);
    await shell('an id with a capital prefix', [['SES_e76d00000065AbCdEfGhIjKlMn', 'call_8']], 'id', undefined);
    await shell('the oldest of two pending calls', [[second, 'call_9'], [second, 'call_10']], 'echo', undefined);
    await shell('the second of the two', [], 'echo', undefined);
    await fg.context.call('tool.execute.before', toolCall(root, 'call_11', 'shell', { command: 'late' }));
    await clock.advance(9_900);
    await shell('9.9 s after its call', [], 'late', undefined);
    await fg.context.call('tool.execute.before', toolCall(root, 'call_12', 'shell', { command: 'later' }));
    await clock.advance(10_100);
    await shell('10.1 s after its call', [], 'later', 'opencode-ses-forged');
    for (let n = 0; n < 65; n += 1) await fg.context.call('tool.execute.before', toolCall(root, `call_b${n}`, 'shell', { command: `cmd ${n}` }));
    await shell('the oldest of 65 pending calls', [], 'cmd 0', undefined);
    await shell('the newest of 65 pending calls', [], 'cmd 64', undefined);
    await shell('the second of 65 pending calls', [], 'cmd 1', undefined);
  });

  // Then
  assert.deepEqual(cases, [
    ['a root call', stem(root), 0],
    ["a child's call", stem(root), 0],
    ["a grandchild's call", stem(root), 0],
    ['two roots pend the command', null, 0],
    ['none pends', null, 0],
    ['an id of another form', null, 0],
    ['an id over 64 characters', null, 0],
    ['an id with a capital prefix', null, 0],
    ['the oldest of two pending calls', stem(second), 0],
    ['the second of the two', stem(second), 0],
    ['9.9 s after its call', stem(root), 0],
    ['10.1 s after its call', null, 0],
    ['the oldest of 65 pending calls', null, 0],
    ['the newest of 65 pending calls', stem(root), 0],
    ['the second of 65 pending calls', stem(root), 0],
  ]);
});
