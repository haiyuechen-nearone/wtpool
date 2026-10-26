import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { ago, actor, document, git, mustValidate, worktree } from '../support/docs.mjs';
import { clock, withoutRunTime } from '../support/grammar.mjs';
import { mount, rowById, text } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

const TASK = '/private/tmp/claude-501/-Users-verify-pool/s1/tasks/b1.output';

/** wt2 with the one actor `s1`: the facts the strip shows, as one document of a series. */
function wt2Document({ dirty, ahead, upstream, reasons, state, reason, action, since, updated, ended = null }) {
  const outputs = [{ task: 'b1', path: TASK, summary: 'cargo build', running: ended === null, started: ago(3000), ended }];
  const one = actor({ key: 's1', label: 'session 13:41', state, reason, action, since, updated, outputs });
  return mustValidate(
    document([
      worktree('fg'),
      worktree('wt2', {
        branch: 'wt2-branch',
        upstream,
        git: git({ dirty, ahead, behind: ahead === null ? null : 0 }),
        free: false,
        reasons,
        activity: state === 'waiting' ? 'waiting' : state === 'working' ? 'working' : 'idle',
        actors: [one],
        last_event: updated,
      }),
    ]),
  );
}

test('detail__should_update_while_open', async () => {
  // Given
  const steps = [
    { dirty: 0, ahead: null, upstream: null, reasons: ['branch'], state: 'working', reason: null, action: { tool: 'Bash', target: 'cargo build' }, since: ago(3600), updated: ago(30) },
    { dirty: 3, ahead: 2, upstream: 'origin/wt2-branch', reasons: ['branch', 'dirty'], state: 'waiting', reason: 'permission', action: { tool: 'Bash', target: 'git push' }, since: ago(600), updated: ago(5) },
    { dirty: 7, ahead: 4, upstream: 'origin/other', reasons: ['branch', 'dirty', 'unreferenced'], state: 'idle', reason: null, action: { tool: 'Edit', target: 'src/lib.rs' }, since: ago(60), updated: ago(1), ended: ago(2) },
  ];
  const documents = steps.map(wt2Document);
  webview = await mount();
  webview.show(documents[0]);
  const id = documents[0].pools[0].worktrees[1].path;
  const row = rowById(webview.document, id);
  row.click();
  const strip = webview.document.querySelector('tr.detail');
  assert.ok(strip, 'setup: clicking wt2 opened no detail strip');
  row.focus();

  for (const [index, step] of steps.entries()) {
    // When
    if (index > 0) webview.show(documents[index]);

    // Then
    const detail = webview.document.querySelector('tr.detail');
    assert.ok(detail, `document ${index}: the detail strip closed`);
    assert.equal(rowById(webview.document, id).nextElementSibling, detail, `document ${index}: the strip left wt2`);
    assert.equal(webview.document.activeElement, row, `document ${index}: focus left the wt2 row`);
    assert.equal(row.getAttribute('aria-selected'), 'true', `document ${index}: wt2 is no longer selected`);
    const shown = text(detail);
    const facts = [
      `${step.dirty} dirty`,
      ...(step.ahead === null ? [] : [`${step.ahead} ahead`]),
      step.upstream ?? 'none',
      step.reasons.join(', '),
      step.state === 'waiting' ? 'waiting for permission' : step.state,
      `${step.action.tool} ${step.action.target}`,
      `since ${clock(step.since)}`,
    ];
    for (const fact of facts) {
      assert.ok(shown.includes(fact), `document ${index}: the open strip does not show "${fact}": ${shown}`);
    }
    const task = withoutRunTime(text(detail.querySelector(`.output[data-output-path="${TASK}"] .output-heading`)));
    assert.equal(task, `cargo build · ${step.ended ? `ended ${clock(step.ended)}` : 'running'}`, `document ${index}: the Output section is stale`);
    if (index > 0) {
      const before = steps[index - 1];
      assert.ok(!shown.includes(`${before.action.tool} ${before.action.target}`), `document ${index}: the strip still shows the old action: ${shown}`);
    }
  }
});

test('detail__should_update_while_open keeps_a_focused_follow_button', async () => {
  // Given
  const base = { dirty: 0, ahead: null, upstream: null, reasons: ['branch'], reason: null, action: { tool: 'Bash', target: 'cargo build' }, since: ago(60), updated: ago(5) };
  const first = wt2Document({ ...base, state: 'working' });
  const second = wt2Document({ ...base, state: 'idle', dirty: 2, updated: ago(1), ended: ago(1) });
  webview = await mount();
  webview.show(first);
  const id = first.pools[0].worktrees[1].path;
  rowById(webview.document, id).click();
  const button = webview.document.querySelector(`.output[data-output-path="${TASK}"] button.follow`);
  const block = webview.document.querySelector(`.output[data-output-path="${TASK}"] pre.output-tail`);
  assert.ok(button && block, 'the strip shows no Output section with a Follow button and a tail block for the task');
  webview.post({ type: 'output', version: 1, row: id, tails: [{ path: TASK, lines: ['compiling'] }] });
  button.focus();

  // When
  webview.show(second);

  // Then
  const detail = webview.document.querySelector('tr.detail');
  assert.equal(webview.document.activeElement, button, 'a new document took focus from the Follow button');
  assert.ok(button.isConnected && detail.contains(button), 'a new document replaced the Follow button');
  assert.ok(block.isConnected && block.textContent === 'compiling', 'a new document replaced the tail block or dropped its tail');
  assert.ok(text(detail).includes('2 dirty') && text(detail).includes('idle'), `the strip did not update: ${text(detail)}`);
});
