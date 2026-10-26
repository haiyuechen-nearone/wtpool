// The --watch stream against the fake wtpool's faults, on the extension's fake clock.
const assert = require('node:assert/strict');

const {
  advance,
  ended,
  expectedDocuments,
  fakeLog,
  openStatus,
  posted,
  resetFake,
  sleep,
  starts,
  viewsSince,
  waitFor,
} = require('./support.cjs');

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/**
 * The longest a fake wtpool may take to log its start, print a line or end: seconds on a loaded
 * machine.
 */
const RESTART_MS = 10_000;

/**
 * Opens the tab on a fake whose first two starts print a whole document, then a `fault` line,
 * and whose later starts work. The document makes the check independent of earlier checks: the
 * paused view of the backoff carries a document of this check's own run, whatever ran before.
 */
async function restartAfter(fault) {
  await resetFake({ scenario: 'idle_pool', faults: [fault, fault, 'none'], lead: true, aliveMs: 200 });
  const from = (await posted()).length;
  const expected = expectedDocuments('idle_pool').at(-1);
  await openStatus();
  const first = await waitFor(() => starts()[0], 10_000);
  assert.ok(first, 'wtpool did not start');
  const leadShown = await waitFor(async () => (await viewsSince(from)).find((v) => same(v.status, expected)), 10_000);
  // The fault pauses the stream, and that sets its restart on the fake clock.
  await waitFor(async () => (await viewsSince(from)).some((v) => v.connection === 'paused'), 10_000);
  await sleep(1500);
  const beforeBackoff = starts().length;
  const pausedViews = (await viewsSince(from)).filter((v) => v.connection === 'paused');
  await advance(999);
  await sleep(500);
  const atNearlyOneSecond = starts().length;
  const fromRestart = (await posted()).length;
  await advance(1);
  const restarted = await waitFor(() => starts().length >= 2, RESTART_MS);
  const live = await waitFor(async () =>
    (await viewsSince(fromRestart)).find((v) => v.connection === 'live' && same(v.status, expected)),
  RESTART_MS);
  const retry = await secondRetry(pausedViews, fromRestart);
  return { from, first, leadShown, beforeBackoff, pausedViews, atNearlyOneSecond, restarted, live, retry };
}

/**
 * The second start faults too. Its restart came exactly at the first paused view's `retry_at`
 * on the fake clock, so the clock now reads that time. The second paused view's `retry_at` must
 * be the time of the next restart, within a second either way, as an RFC 3339 time may hold
 * whole seconds only.
 */
async function secondRetry(pausedViews, fromRestart) {
  const firstAt = Date.parse(pausedViews[0]?.retry_at);
  const second = await waitFor(async () => (await viewsSince(fromRestart)).find((v) => v.connection === 'paused'), RESTART_MS);
  const secondAt = Date.parse(second?.retry_at);
  if (!Number.isFinite(firstAt) || !Number.isFinite(secondAt) || secondAt <= firstAt) {
    return { firstAt, secondAt, early: undefined, onTime: undefined };
  }
  const count = starts().length;
  await advance(Math.max(0, secondAt - firstAt - 1001));
  await sleep(500);
  const early = starts().length - count;
  await advance(2000);
  const onTime = await waitFor(() => starts().length > count, RESTART_MS);
  return { firstAt, secondAt, early, onTime: Boolean(onTime) };
}

describe('stream', () => {
  it('stream__should_spawn_one_watch_and_never_poll', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool', aliveMs: 200 });

    // When
    await openStatus();
    await waitFor(() => starts().length >= 1, 10_000);
    await advance(60_000, { step: 1000, realMs: 250 });
    await sleep(5000);

    // Then
    const all = starts();
    assert.equal(all.length, 1, `wtpool started ${all.length} times in 60 s`);
    assert.deepEqual(all[0].argv, ['status', '--watch']);
    assert.ok(!fakeLog().some((e) => e.event === 'start' && e.argv.includes('--json')), 'wtpool status --json ran');
    assert.ok(!ended(all[0].pid), 'the watch ended');
  });

  it('partial_lines', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool', faults: ['partial'], aliveMs: 200 });
    const from = (await posted()).length;

    // When
    await openStatus();
    const shown = await waitFor(async () =>
      (await viewsSince(from)).find((v) => same(v.status, expectedDocuments('idle_pool')[0])),
    10_000);
    await advance(2000, { step: 500, realMs: 200 });

    // Then
    assert.ok(shown, 'a document written in three parts never reached the webview');
    assert.equal(starts().length, 1, 'a partial line restarted the stream');
  });

  for (const [check, fault] of [
    ['oversize_line', 'oversize'],
    ['garbage_line', 'garbage'],
    ['unknown_version', 'unknown_version'],
  ]) {
    it(check, async () => {
      // Given
      // When
      const run = await restartAfter(fault);

      // Then
      assert.ok(run.leadShown, 'the whole document before the fault never reached the webview');
      assert.equal(run.beforeBackoff, 1, `the ${fault} line restarted at once instead of after 1 s`);
      assert.equal(run.atNearlyOneSecond, 1, 'the restart came before the 1 s backoff');
      assert.ok(run.restarted, `the ${fault} line did not restart the stream after 1 s`);
      assert.ok(ended(run.first.pid), 'the first wtpool still runs after the restart');
      assert.ok(run.pausedViews.length >= 1, 'the header did not show paused during the backoff');
      assert.ok(run.live, 'the restarted stream did not reach the webview');
      assert.ok(Number.isFinite(run.retry.firstAt), `a paused view without retry_at: ${JSON.stringify(run.pausedViews[0])}`);
      assert.ok(Number.isFinite(run.retry.secondAt) && run.retry.secondAt > run.retry.firstAt, 'the second paused view has no later retry_at');
      assert.equal(run.retry.early, 0, 'the restart came before retry_at');
      assert.ok(run.retry.onTime, 'no restart at retry_at');
      const bad = (await viewsSince(run.from)).filter(
        (v) => v.status !== null && (v.status?.version !== 1 || (v.status?.machine?.name ?? '').length > 1000),
      );
      assert.deepEqual(bad, [], `a ${fault} document reached the webview`);
    });
  }

  it('early_exit', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool', faults: ['early_exit'] });
    const from = (await posted()).length;
    await openStatus();
    await waitFor(() => starts().length >= 1, 10_000);
    const delays = [1000, 2000, 4000, 8000, 16000, 30000, 30000];

    // When
    const seen = [];
    for (const [exits, delay] of delays.entries()) {
      const count = starts().length;
      // Each exit pauses the stream, and that sets its restart on the fake clock.
      await waitFor(async () => (await viewsSince(from)).filter((v) => v.connection === 'paused').length > exits, 10_000);
      await advance(delay - 1);
      await sleep(400);
      const early = starts().length;
      await advance(1);
      const late = await waitFor(() => starts().length > count && starts().length, RESTART_MS);
      seen.push({ delay, early: early - count, late: (late || count) - count });
    }

    // Then
    assert.deepEqual(
      seen,
      delays.map((delay) => ({ delay, early: 0, late: 1 })),
      'restarts after an early exit back off 1 s, doubling to 30 s',
    );
  });

  it('stream__should_post_paused_before_the_first_document', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool', faults: ['early_exit', 'none'], aliveMs: 200 });
    const from = (await posted()).length;
    const expected = expectedDocuments('idle_pool').at(-1);

    // When
    await openStatus();
    const first = await waitFor(() => starts()[0], 10_000);
    await waitFor(() => first && ended(first.pid), RESTART_MS);
    const paused = await waitFor(async () => (await viewsSince(from)).find((v) => v.connection === 'paused'), RESTART_MS);
    await advance(999);
    await sleep(400);
    const beforeRetry = starts().length;
    await advance(1);
    const live = await waitFor(async () =>
      (await viewsSince(from)).find((v) => v.connection === 'live' && same(v.status, expected)),
    RESTART_MS);

    // Then
    assert.ok(first, 'wtpool did not start');
    assert.ok(paused, 'a fault before the first document posted no paused view');
    assert.equal(paused.status, null, 'the paused view before the first document has a status');
    assert.equal(paused.version, 1, 'the paused view has no version 1');
    assert.ok(Number.isFinite(Date.parse(paused.retry_at)), `the paused view has no retry_at: ${JSON.stringify(paused)}`);
    assert.equal(beforeRetry, 1, 'the restart came before the 1 s backoff');
    assert.ok(live, 'the document of the restarted stream never reached the webview');
  });

  it('stream__should_post_paused_before_the_first_document wtpool_error', async () => {
    // Given
    // The first start writes more stderr than the stream keeps, its last line before blank
    // ones, and exits 2. The second exits 1 with no stderr. The third works.
    const message = 'wtpool: not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND';
    const stderr = `${'wtpool: a line before the error\n'.repeat(300)}${message}\n  \n\n`;
    await resetFake({ scenario: 'idle_pool', faults: ['error_exit', 'early_exit', 'none'], stderr, exitCode: 2, aliveMs: 200 });
    const from = (await posted()).length;
    const expected = expectedDocuments('idle_pool').at(-1);

    // When
    await openStatus();
    const first = await waitFor(() => starts()[0], 10_000);
    await waitFor(() => first && ended(first.pid), RESTART_MS);
    const failed = await waitFor(async () => (await viewsSince(from)).find((v) => v.connection === 'paused'), RESTART_MS);
    const fromSecond = (await posted()).length;
    await advance(1000);
    const second = await waitFor(() => starts()[1], RESTART_MS);
    await waitFor(() => second && ended(second.pid), RESTART_MS);
    const silent = await waitFor(async () => (await viewsSince(fromSecond)).find((v) => v.connection === 'paused'), RESTART_MS);
    const fromThird = (await posted()).length;
    await advance(2000);
    const live = await waitFor(async () =>
      (await viewsSince(fromThird)).find((v) => v.connection === 'live' && same(v.status, expected)),
    RESTART_MS);

    // Then
    assert.ok(stderr.length > 4096, 'setup: the stderr fits the tail the stream keeps');
    assert.ok(first, 'wtpool did not start');
    assert.ok(failed, 'a start that exited with an error posted no paused view');
    assert.equal(failed.error, message, "the paused view does not carry wtpool's last non-empty stderr line");
    assert.ok(second, 'wtpool did not restart after its error');
    assert.ok(silent, 'a second start that exited with an error posted no paused view');
    assert.equal(silent.error, 'wtpool exited with code 1', 'the paused view of an exit without stderr');
    assert.ok(live, 'the document of the restarted stream never reached the webview');
    assert.equal(live.error, undefined, 'the live view still carries an error');
  });

  it('stream__should_post_starting_before_the_first_line', async () => {
    // Given
    // `wtpool` prints its first line 5 s after it starts, as after a reload it first
    // computes the git facts of every worktree.
    await resetFake({ scenario: 'idle_pool', faults: ['slow_start'], slowStartMs: 5000, aliveMs: 200 });
    const from = (await posted()).length;
    const expected = expectedDocuments('idle_pool')[0];

    // When
    await openStatus();
    const atOnce = await waitFor(async () => (await viewsSince(from))[0], 1_000);
    const started = await waitFor(() => starts()[0], 3_000);
    await advance(20_000, { step: 5000, realMs: 100 });
    const live = await waitFor(async () =>
      (await viewsSince(from)).find((v) => v.connection === 'live' && same(v.status, expected)),
    10_000);
    const views = await viewsSince(from);

    // Then
    assert.ok(atOnce, "the tab posted no view before wtpool's first line");
    assert.deepEqual(
      { connection: atOnce.connection, version: atOnce.version, status: atOnce.status, retry_at: atOnce.retry_at, error: atOnce.error },
      { connection: 'starting', version: 1, status: null, retry_at: undefined, error: undefined },
      `the view posted at once is no starting view: ${JSON.stringify(atOnce)}`,
    );
    assert.ok(Number.isFinite(Date.parse(atOnce.last_line)), `the starting view has no last_line: ${JSON.stringify(atOnce)}`);
    assert.ok(started, 'wtpool did not start');
    assert.ok(live, 'the first document of the slow start never reached the webview');
    // The tab posts the starting view, and its page asks for it once more when it starts.
    const beforeLive = views.slice(0, views.indexOf(live)).map((v) => v.connection);
    assert.ok(
      beforeLive.length <= 2 && beforeLive.every((connection) => connection === 'starting'),
      `20 s without a first line posted more than the starting view: ${JSON.stringify(beforeLive)}`,
    );
    assert.equal(starts().length, 1, 'a slow start restarted the stream');
  });

  it('slow_start', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool', faults: ['slow_start'], slowStartMs: 5000, aliveMs: 200 });
    const from = (await posted()).length;

    // When
    await openStatus();
    await advance(10_000, { step: 2000, realMs: 300 });
    const shown = await waitFor(async () =>
      (await viewsSince(from)).find((v) => same(v.status, expectedDocuments('idle_pool')[0]) && v.connection === 'live'),
    10_000);

    // Then
    assert.ok(shown, 'the first document of a slow start never reached the webview');
    assert.equal(starts().length, 1, 'a slow start restarted the stream');
  });

  it('stream__should_survive_2000_documents', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool', count: 2000, aliveMs: 0 });
    const from = (await posted()).length;
    const last = structuredClone(expectedDocuments('idle_pool').at(-1));
    last.generated = new Date(Date.parse(last.generated) + 2000 * 1000).toISOString().replace('.000Z', 'Z');
    last.pools[0].skipped_records = 2000;

    // When
    await openStatus();
    const reached = await waitFor(async () => (await viewsSince(from)).find((v) => same(v.status, last)), 60_000);
    await sleep(1000);

    // Then
    assert.ok(reached, 'the 2000th document never reached the webview');
    assert.ok(same((await viewsSince(from)).at(-1).status, last), 'a later message replaced the last document');
    assert.equal(starts().length, 1, '2000 documents restarted the stream');
  });

  it('stall_30s', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool', faults: ['stall', 'none'], aliveMs: 200 });
    const from = (await posted()).length;
    await openStatus();
    const first = await waitFor(() => starts()[0], 10_000);
    await waitFor(async () => (await viewsSince(from)).some((v) => v.status !== null), 10_000);

    // When
    await advance(14_999);
    await sleep(300);
    const before15 = (await viewsSince(from)).at(-1)?.connection;
    await advance(1);
    const stale = await waitFor(async () => (await viewsSince(from)).find((v) => v.connection === 'stale'), 2_000);
    await advance(14_999);
    await sleep(500);
    const before30 = starts().length;
    await advance(1);
    let restarted = await waitFor(() => starts().length >= 2, RESTART_MS);
    if (!restarted) {
      await advance(1000);
      restarted = await waitFor(() => starts().length >= 2, RESTART_MS);
    }

    // Then
    assert.equal(before15, 'live', 'the header left live before 15 s without a line');
    assert.ok(stale, 'no stale header after 15 s without a line');
    assert.equal(before30, 1, 'the stream restarted before 30 s without a line');
    assert.ok(restarted, 'the stream did not restart after 30 s without a line');
    assert.ok(ended(first.pid), 'the stalled wtpool still runs');
  });
});

