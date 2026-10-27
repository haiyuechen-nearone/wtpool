import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { mustValidate } from '../support/docs.mjs';
import { CONFIG, busyDocument, withJobs } from '../support/jobs.mjs';
import { announced, answer, mustFind, openStrip, strip, type } from '../support/jobs-render.mjs';
import { isShown, isVisuallyHidden, mount, seen } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

const busy = busyDocument();
/** Longer than the wait for the answer to one call: 10 s, and some slack. */
const PAST_ONE_ANSWER_MS = 13_000;
/** Longer than a document that already shows a save keeps its answer waited for. */
const PAST_SHOWN_MS = 3_000;
const NO_ANSWER = 'No answer from wtpool';

/** Opens the strip, types 6 in the Limit field and clicks Save. */
async function saveSix() {
  webview = await mount();
  webview.show(mustValidate(busy));
  const shown = openStrip(webview);
  const field = mustFind(shown, 'input.job-limit');
  const save = mustFind(shown, 'button.job-save');
  type(webview, field, '6');
  save.click();
  webview.tick(20);
  return { shown, field, save, status: mustFind(shown, '.limit-status', 'U11') };
}

/** The save button and the status as a sighted user sees them. */
function looks(webview, { save, status }) {
  return {
    button: save.textContent,
    disabled: save.disabled,
    status: isShown(webview.window, status) && !isVisuallyHidden(webview.window, status) ? seen(webview.window, status) : '',
    failed: status.classList.contains('failed'),
  };
}

test('machine_strip__should_show_each_save_state saving', async () => {
  // Given, When
  const parts = await saveSix();

  // Then
  assert.deepEqual(
    looks(webview, parts),
    { button: 'Saving…', disabled: true, status: '', failed: false },
    'while the save waits for its answer, Save does not read Saving… disabled (U11)',
  );
  assert.ok(seen(webview.window, strip(webview)).includes('Saving…'), 'Saving… is not visible in the strip (U11)');
});

test('machine_strip__should_show_each_save_state saved', async () => {
  // Given
  const parts = await saveSix();

  // When
  answer(webview, { ok: true });
  const afterAnswer = looks(webview, parts);
  const live = announced(webview);
  webview.show(mustValidate(withJobs(busy, { ...busy.machine.jobs, limit: 6 })));
  webview.tick(1_000);
  const afterDocument = looks(webview, parts);
  type(webview, parts.field, '7');
  const afterEdit = looks(webview, parts);

  // Then
  assert.deepEqual(afterAnswer, { button: 'Save', disabled: false, status: 'Saved.', failed: false }, 'an ok answer does not show Saved. next to an enabled Save (U11)');
  assert.ok(live.includes('Saved.'), `the live region does not announce Saved. (X20): ${live}`);
  assert.deepEqual(afterDocument, afterAnswer, 'the next document hides Saved. (U11)');
  assert.deepEqual(afterEdit, { ...afterAnswer, status: '' }, 'an edit keeps the old Saved. (U11)');
});

test('machine_strip__should_show_each_save_state refused', async () => {
  // Given
  const error = `wtpool: ${CONFIG} is a symlink to /etc/x; edit it by hand`;
  const parts = await saveSix();

  // When
  answer(webview, { ok: false, error });

  // Then
  assert.deepEqual(looks(webview, parts), { button: 'Save', disabled: false, status: error, failed: true }, 'a refusal does not show in the attention colour next to an enabled Save (U11)');
  assert.ok(announced(webview).includes(error), 'the refusal is not announced (X20)');
});

test('machine_strip__should_show_each_save_state no_answer', async () => {
  // Given
  const parts = await saveSix();

  // When
  webview.tick(PAST_ONE_ANSWER_MS - 1_000);
  const beforeDue = looks(webview, parts);
  webview.tick(1_000);

  // Then
  assert.deepEqual(beforeDue, { button: 'Saving…', disabled: true, status: '', failed: false }, 'the strip gives up before the answer is due (X21)');
  assert.deepEqual(looks(webview, parts), { button: 'Save', disabled: false, status: NO_ANSWER, failed: true }, `an unanswered save does not read ${NO_ANSWER} next to an enabled Save (U11)`);
  assert.ok(announced(webview).includes(NO_ANSWER), `the live region does not announce ${NO_ANSWER} (U11)`);
});

test('machine_strip__should_show_each_save_state shown_without_answer', async () => {
  // Given
  const parts = await saveSix();

  // When
  webview.show(mustValidate(withJobs(busy, { ...busy.machine.jobs, limit: 6 })));
  webview.tick(PAST_SHOWN_MS);

  // Then
  assert.deepEqual(looks(webview, parts), { button: 'Save', disabled: false, status: 'Saved.', failed: false }, 'a save the next document shows, with no answer, does not read Saved. (U11)');
});
