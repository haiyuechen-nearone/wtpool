import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { mustValidate } from '../support/docs.mjs';
import { busyDocument, jobs, withJobs } from '../support/jobs.mjs';
import { announced, answer, mustFind, openStrip, press, scope, settingsSince, type } from '../support/jobs-render.mjs';
import { mount } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

const busy = busyDocument();
/** Longer than any wait for one answer: two calls of 10 s each, and some slack. */
const PAST_ANY_ANSWER_MS = 30_000;
/** Longer than a document that already shows an edit keeps its answer waited for. */
const PAST_SHOWN_MS = 3_000;

function editable(heavy = [{ pattern: 'just', source: 'machine' }]) {
  return mustValidate(withJobs(busy, jobs({ heavy })));
}

/** Adds `pattern` to This machine's list with Enter. */
function add(pattern) {
  const { input } = scope(webview, 'machine');
  assert.ok(input, "This machine's list has no input.heavy-input (U15)");
  type(webview, input, pattern);
  press(webview, input, 'Enter');
  return input;
}

test('heavy_editor__should_match_answers_in_order two_adds', async () => {
  // Given
  webview = await mount();
  webview.show(editable());
  openStrip(webview);
  const before = webview.posted.length;
  add('make');
  const input = add('ninja');

  // When
  answer(webview, { ok: true, scope: 'machine' });
  const first = { live: announced(webview), input: input.value };
  answer(webview, { ok: true, scope: 'machine' });
  const second = { live: announced(webview), input: input.value };

  // Then
  assert.equal(settingsSince(webview, before).length, 2, 'the two adds were not both sent');
  assert.ok(first.live.includes('Added make.') && !first.live.includes('ninja'), `the first answer does not announce Added make.: ${first.live} (U15)`);
  assert.equal(first.input, 'ninja', 'the first answer cleared the input that holds the second pattern');
  assert.ok(second.live.includes('Added ninja.'), `the second answer does not announce Added ninja.: ${second.live} (U15)`);
  assert.equal(second.input, '', 'the second answer left its pattern in the input');
});

test('heavy_editor__should_match_answers_in_order shown_before_answer', async () => {
  // Given
  webview = await mount();
  webview.show(editable());
  openStrip(webview);
  add('make');
  webview.show(editable([{ pattern: 'just', source: 'machine' }, { pattern: 'make', source: 'machine' }]));

  // When
  answer(webview, { ok: true, scope: 'machine' });

  // Then
  assert.ok(announced(webview).includes('Added make.'), `an answer after the document that shows the add says nothing: ${announced(webview)} (U15)`);
});

for (const [name, wait] of [
  ['refused_then_shown', 'shown'],
  ['refused_then_overdue', 'overdue'],
]) {
  test(`heavy_editor__should_match_answers_in_order ${name}`, async () => {
    // Given: the extension never answers the add of make.
    webview = await mount();
    webview.show(editable());
    openStrip(webview);
    add('make');
    if (wait === 'shown') {
      webview.show(editable([{ pattern: 'just', source: 'machine' }, { pattern: 'make', source: 'machine' }]));
      webview.tick(PAST_SHOWN_MS);
    } else {
      webview.tick(PAST_ANY_ANSWER_MS);
    }
    add('ninja');

    // When
    answer(webview, { ok: true, scope: 'machine' });

    // Then
    const live = announced(webview);
    assert.ok(live.includes('Added ninja.') && !live.includes('make'), `the answer to the add of ninja does not announce it: ${live} (U15)`);
  });
}

for (const name of ['refused_then_shown', 'refused_then_overdue']) {
  test(`machine_strip__should_save_again_without_an_answer ${name}`, async () => {
    // Given: the extension never answers the save of 6.
    webview = await mount();
    webview.show(mustValidate(busy));
    const shown = openStrip(webview);
    const field = mustFind(shown, 'input.job-limit');
    const start = webview.posted.length;
    type(webview, field, '6');
    press(webview, field, 'Enter');

    // When
    let early = [];
    if (name === 'refused_then_shown') {
      webview.show(mustValidate(withJobs(busy, { ...busy.machine.jobs, limit: 6 })));
      webview.tick(PAST_SHOWN_MS);
    } else {
      webview.tick(PAST_SHOWN_MS);
      type(webview, field, '7');
      press(webview, field, 'Enter');
      early = settingsSince(webview, start);
      webview.tick(PAST_ANY_ANSWER_MS);
    }
    type(webview, field, '7');
    press(webview, field, 'Enter');

    // Then
    if (name === 'refused_then_overdue') {
      assert.deepEqual(early, [{ type: 'jobSettings', limit: 6 }], 'a second save went out before the answer was overdue (X20)');
    }
    assert.deepEqual(
      settingsSince(webview, start),
      [{ type: 'jobSettings', limit: 6 }, { type: 'jobSettings', limit: 7 }],
      `Save stays dead after a save the extension never answered (X21): ${name}`,
    );
  });
}
