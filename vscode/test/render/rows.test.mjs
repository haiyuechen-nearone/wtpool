import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import {
  NOW,
  ago,
  document,
  git,
  mustValidate,
  withActor,
  worktree,
} from '../support/docs.mjs';
import { expectedRow, normalizeEscapes, rowOrder } from '../support/grammar.mjs';
import { cells, mount, rows, seen, seenLines, text } from '../support/webview.mjs';

let webview;
afterEach(() => webview?.close());

/** The text of every cell of `row`, as {@link expectedRow} lists them. */
function rowText(row) {
  const { data } = cells(row);
  const window = row.ownerDocument.defaultView;
  assert.equal(data.length, 6, `a row has six data cells before td.actions: ${row.outerHTML}`);
  return {
    activity: normalizeEscapes(seen(window, data[0])),
    worktree: normalizeEscapes(seen(window, data[1])),
    branch: normalizeEscapes(seen(window, data[2])),
    git: normalizeEscapes(seen(window, data[3])),
    agent: seenLines(window, data[4]).map(normalizeEscapes),
    last: seenLines(window, data[5]),
  };
}

test('rows__should_show_name_role_and_branch', async () => {
  // Given
  const status = mustValidate(
    document([
      worktree('x', { branch: 'side' }),
      worktree('wt2', { branch: 'feature-a', free: false, reasons: ['branch'], activity: 'unused' }),
      worktree('fg'),
      worktree('wt1'),
      worktree('wt3', { last_change: ago(30) }),
    ]),
  );
  webview = await mount();

  // When
  webview.show(status);

  // Then
  const heading = webview.document.querySelector('tr[data-machine]');
  assert.ok(heading, 'no heading row tr[data-machine]');
  assert.ok(text(heading).includes('verify-mac'), `the heading names no machine: ${text(heading)}`);
  const shown = rows(webview.document);
  assert.equal(shown.length, 5, 'one row per worktree');
  assert.ok(
    heading.compareDocumentPosition(shown[0]) & webview.window.Node.DOCUMENT_POSITION_FOLLOWING,
    'the heading row comes before the worktree rows',
  );
  const byPath = new Map(status.pools[0].worktrees.map((w) => [w.path, w]));
  for (const row of shown) {
    const id = row.getAttribute('data-row-id');
    const expected = byPath.get(id);
    assert.ok(expected, `data-row-id ${id} is the path of no worktree`);
    const { worktree: name, branch } = rowText(row);
    assert.equal(name, expectedRow(expected, NOW).worktree, `the name and role of ${id}`);
    assert.equal(branch, expectedRow(expected, NOW).branch, `the branch of ${id}`);
  }
});

const grammarCases = [
  ['clean_foreground', worktree('fg')],
  ['free_slot', worktree('wt1')],
  ['every_git_fact', worktree('wt2', {
    branch: 'feature-a',
    upstream: 'origin/feature-a',
    free: false,
    reasons: ['branch', 'dirty', 'unreferenced', 'operation'],
    activity: 'unused',
    git: git({ dirty: 3, ahead: 2, behind: 1, unreferenced: 4, operation: 'rebase' }),
  })],
  ['git_unknown', worktree('wt3', {
    free: false,
    reasons: ['unknown'],
    activity: 'not_reusable',
    git: git({ dirty: null, unknown: true }),
  })],
  ['missing', worktree('wt3', {
    missing: true,
    free: false,
    reasons: ['missing'],
    activity: 'missing',
    git: git({ dirty: null, unreferenced: null, unknown: true }),
    last_change: null,
  })],
  ['unmanaged', worktree('x', { git: git({ dirty: 1 }), reasons: ['unmanaged', 'dirty'] })],
  ['build', worktree('wt1', { activity: 'building', builds: [{ pid: 7, name: 'cargo-nextest' }] })],
  ['working_with_action', withActor('wt1', { state: 'working', label: 'review' })],
  ['working_without_action', withActor('wt1', { state: 'working', action: null })],
  ['quiet_with_action', withActor('wt1', { state: 'quiet' })],
  ['quiet_without_action', withActor('wt1', { state: 'quiet', action: null })],
  ['waiting_permission', withActor('wt1', {
    state: 'waiting',
    reason: 'permission',
    action: { tool: 'Bash', target: 'git push' },
  })],
  ['waiting_question', withActor('wt1', { state: 'waiting', reason: 'question', action: null })],
  ['waiting_plan', withActor('wt1', { state: 'waiting', reason: 'plan', action: null })],
  ['waiting_input', withActor('wt1', { state: 'waiting', reason: 'input', action: null })],
  ['idle', withActor('wt1', { state: 'idle', action: null })],
  ['idle_after_an_api_error', withActor('wt1', { state: 'idle', action: null, error: 'rate_limit' })],
  ['done_subagent', withActor('wt1', { state: 'done', kind: 'subagent', label: 'test', action: null })],
  ['edit_target', withActor('wt1', { action: { tool: 'Edit', target: 'src/resolve.rs' } })],
];

test.each(grammarCases)('rows__should_follow_the_grammar %s', async (_, subject) => {
  // Given
  const others = subject.name === 'fg' ? [] : [worktree('fg')];
  const status = mustValidate(document([...others, subject]));
  webview = await mount();

  // When
  webview.show(status);

  // Then
  const row = rows(webview.document).find((r) => r.getAttribute('data-row-id') === subject.path);
  assert.ok(row, `no row for ${subject.path}`);
  assert.deepEqual(rowText(row), expectedRow(subject, NOW));
});

const ageCases = [
  ['just_now', 0, 'now'],
  ['under_5_s', 4.9, 'now'],
  ['5_s', 5, '5 s'],
  ['59_s', 59, '59 s'],
  ['1_m', 60, '1 m'],
  ['59_m', 3599, '59 m'],
  ['1_h', 3600, '1 h'],
  ['47_h', 48 * 3600 - 1, '47 h'],
  ['2_d', 48 * 3600, '2 d'],
  ['unknown', null, ''],
];

test.each(ageCases)('ages__should_use_the_format %s', async (_, seconds, expected) => {
  // Given
  const lastChange = seconds === null ? null : ago(seconds);
  const status = mustValidate(
    document([worktree('fg'), worktree('wt1', { last_change: lastChange })]),
  );
  webview = await mount();

  // When
  webview.show(status);

  // Then
  const row = rows(webview.document).find((r) => r.getAttribute('data-row-id').endsWith('/wt1'));
  assert.ok(row, 'no row for wt1');
  assert.deepEqual(rowText(row).last, [expected]);
});

test('ages__should_use_the_format actor_lines', async () => {
  // Given
  const busy = withActor('wt1', { updated: ago(30) });
  const status = mustValidate(document([worktree('fg'), busy]));
  webview = await mount();

  // When
  webview.show(status);

  // Then
  const row = rows(webview.document).find((r) => r.getAttribute('data-row-id') === busy.path);
  assert.deepEqual(rowText(row).last, ['30 s'], 'an actor line shows the age of its updated');
});

test('order__should_not_change_when_states_change', async () => {
  // Given
  const shuffled = [
    worktree('x'),
    worktree('wt3'),
    worktree('a-side', { branch: 'a-side' }),
    worktree('fg'),
    worktree('wt1'),
    worktree('wt2'),
  ];
  const first = mustValidate(document(shuffled));
  const changed = mustValidate(
    document([
      withActor('wt1', { state: 'waiting', reason: 'question', action: null }),
      worktree('x', { activity: 'unknown', git: git({ dirty: null, unknown: true }), reasons: ['unmanaged', 'unknown'] }),
      worktree('fg'),
      worktree('wt3', { free: false, reasons: ['dirty'], git: git({ dirty: 1 }), activity: 'not_reusable' }),
      worktree('a-side', { branch: 'a-side' }),
      withActor('wt2', { state: 'working' }),
    ]),
  );
  const expectedOrder = rowOrder(shuffled).map((w) => w.path);
  webview = await mount();

  // When
  webview.show(first);
  const before = rows(webview.document);
  webview.show(changed);
  const after = rows(webview.document);

  // Then
  assert.deepEqual(
    before.map((r) => r.getAttribute('data-row-id')),
    expectedOrder,
    'foreground, slots, unmanaged, each by name',
  );
  assert.deepEqual(
    after.map((r) => r.getAttribute('data-row-id')),
    expectedOrder,
    'rows keep their place when states change',
  );
  assert.ok(
    before.every((row, index) => row === after[index]),
    'rows update in place instead of being replaced',
  );
});

test('ages__should_tick_without_messages', async () => {
  // Given
  const status = mustValidate(
    document([worktree('fg'), worktree('wt1', { last_change: ago(58) }), withActor('wt2', { updated: ago(3) })]),
  );
  webview = await mount();
  webview.show(status);
  const postedBefore = webview.posted.length;
  const last = (suffix) =>
    rowText(rows(webview.document).find((r) => r.getAttribute('data-row-id').endsWith(suffix))).last;
  const start = [last('/wt1'), last('/wt2')];

  // When
  webview.tick(1000);
  const oneSecond = [last('/wt1'), last('/wt2')];
  webview.tick(1000);
  const twoSeconds = [last('/wt1'), last('/wt2')];

  // Then
  assert.deepEqual(start, [['58 s'], ['now']]);
  assert.deepEqual(oneSecond, [['59 s'], ['now']]);
  assert.deepEqual(twoSeconds, [['1 m'], ['5 s']]);
  assert.equal(webview.posted.length, postedBefore, 'ticking posted messages');
});


test('budget__b7 tick_changes_only_ages', async () => {
  // Given
  const status = mustValidate(
    document([
      worktree('fg', { last_change: ago(58) }),
      withActor('wt1', { updated: ago(3) }),
      withActor('wt2', { state: 'waiting', reason: 'permission', updated: ago(40) }),
      worktree('wt3', { last_change: ago(3590) }),
      worktree('x', { branch: 'side' }),
    ]),
  );
  webview = await mount();
  webview.show(status);
  const changed = [];
  const observer = new webview.window.MutationObserver((records) => changed.push(...records));
  observer.observe(webview.document.body, { subtree: true, childList: true, characterData: true, attributes: true });
  const postedBefore = webview.posted.length;

  // When
  for (let second = 0; second < 15; second += 1) webview.tick(1000);
  changed.push(...observer.takeRecords());
  observer.disconnect();

  // Then
  const outside = changed
    .map((record) => (record.target.nodeType === 1 ? record.target : record.target.parentElement))
    .filter((element) => {
      const cell = element?.closest('td');
      const row = cell?.closest('tr[data-row-id]');
      return !(row && cells(row).data[5] === cell);
    })
    .map((element) => element?.outerHTML?.slice(0, 120) ?? String(element));
  assert.ok(changed.length > 0, 'fifteen seconds of ticks changed no age');
  assert.deepEqual([...new Set(outside)], [], 'a tick changed more than the Last cells');
  assert.equal(webview.posted.length, postedBefore, 'a tick posted a message');
});
