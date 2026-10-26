// U1 and U3 in jsdom: keyboard focus scrolls a focused row's start into view and a focused row
// action wholly, with its focus ring, while focus that a click gives scrolls nothing. The shot
// check scroll__should_bring_focus_and_waits_into_view measures the same in Chromium; this case
// sits with the waiting chip's check, the render check of scrolling a row's start into view.
import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { document, mustValidate, worktree } from '../support/docs.mjs';
import { mount, rows } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

/**
 * Moves focus to `target` as the Tab key does: keydown, focus, keyup. jsdom's `:focus-visible`
 * takes a Tab focus as visible only when it comes from the page, not from another element, so
 * each move starts from the page.
 */
function tabTo(target) {
  const { window } = webview;
  window.document.activeElement?.blur();
  window.document.body.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Tab', bubbles: true }));
  target.focus();
  target.dispatchEvent(new window.KeyboardEvent('keyup', { key: 'Tab', bubbles: true }));
}

/** Focuses `target` as a mouse press does, then clicks it. */
function clickOn(target) {
  const { window } = webview;
  target.dispatchEvent(new window.MouseEvent('mousedown', { bubbles: true }));
  target.focus();
  target.dispatchEvent(new window.MouseEvent('mouseup', { bubbles: true }));
  target.click();
}

/**
 * The scrolls since the `from`th, each as its row id or action and its options, copied out of the
 * webview's realm.
 */
function scrollsSince(from) {
  return webview.scrolls.slice(from).map(({ element, options }) => ({
    target: element.getAttribute('data-row-id') ?? element.getAttribute('data-action'),
    options: { ...options },
  }));
}

test('chips__should_count_and_scroll_to_waiting keyboard_focus_scrolls_rows_and_actions', async () => {
  // Given
  const status = mustValidate(document([worktree('fg'), worktree('wt1'), worktree('wt2')]));
  webview = await mount();
  webview.show(status);
  const [, wt1, wt2] = rows(webview.document);
  const action = wt2?.querySelector('button[data-action]');
  assert.ok(wt1 && action, 'setup: no wt1 row, or wt2 offers no row action');

  // When
  let from = webview.scrolls.length;
  tabTo(wt1);
  const rowFocused = webview.document.activeElement === wt1;
  const byRow = scrollsSince(from);
  from = webview.scrolls.length;
  tabTo(action);
  const actionFocused = webview.document.activeElement === action;
  const byAction = scrollsSince(from);
  from = webview.scrolls.length;
  clickOn(wt1);
  clickOn(action);
  const byClicks = scrollsSince(from);

  // Then
  assert.ok(rowFocused && actionFocused, 'setup: Tab did not focus the row and the action');
  assert.deepEqual(
    byRow,
    [{ target: wt1.getAttribute('data-row-id'), options: { block: 'nearest', inline: 'start' } }],
    "a row's keyboard focus does not scroll the row's start into view",
  );
  assert.deepEqual(
    byAction,
    [{ target: action.getAttribute('data-action'), options: { block: 'nearest', inline: 'nearest' } }],
    "an action's keyboard focus does not scroll the whole action into view",
  );
  assert.deepEqual(byClicks, [], 'focus a click gave scrolled the table');
});
