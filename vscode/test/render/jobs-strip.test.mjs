import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { document, mustValidate, view, worktree } from '../support/docs.mjs';
import { TEXT, busyDocument, jobLineParts, jobs, withJobs } from '../support/jobs.mjs';
import {
  announced,
  answer,
  choose,
  mustButton,
  mustFind,
  openStrip,
  press,
  settingsSince,
  stopsAfterButton,
  strip,
  type,
} from '../support/jobs-render.mjs';
import { mount, rows, text } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

const busy = busyDocument();

function inOrder(shown, parts) {
  let at = 0;
  for (const part of parts) {
    const found = shown.indexOf(part, at);
    if (found < 0) return false;
    at = found + part.length;
  }
  return true;
}

/** The job lines of one list: each `li[data-job-id]`'s id and text. */
function jobLines(root, selector) {
  const list = root.querySelector(selector);
  return { list, lines: [...(list?.querySelectorAll('li[data-job-id]') ?? [])].map((li) => ({ id: li.dataset.jobId, text: text(li) })) };
}

function readsNone(list) {
  if (!list) return false;
  return [list, list.previousElementSibling, list.nextElementSibling].some((el) => el && text(el) === 'none');
}

test('machine_strip__should_show_settings_patterns_and_jobs busy', async () => {
  // Given
  const status = mustValidate(busy);
  const value = status.machine.jobs;
  webview = await mount();
  webview.show(status);

  // When
  const shown = openStrip(webview);

  // Then
  const heading = webview.document.querySelector('tr.pool-heading');
  assert.equal(heading.nextElementSibling, shown, 'the strip is not right under the heading (U11)');
  const all = text(shown);
  for (const part of [`Heavy jobs on verify-mac`, 'Heavy commands', TEXT.saved(value), TEXT.hint]) {
    assert.ok(all.includes(part), `the strip lacks "${part}" (U11): ${all}`);
  }
  assert.equal(mustFind(shown, 'input.job-limit', 'the Limit field').value, '4', 'the Limit field holds the limit');
  const policy = mustFind(shown, 'select.job-enforce', 'the Policy menu');
  assert.deepEqual([...policy.options].map((o) => o.value), ['off', 'warn', 'require'], 'the Policy menu offers off, warn and require');
  assert.equal(policy.value, 'warn', 'the Policy menu shows the policy');
  mustFind(shown, 'button.job-save', 'Save');
  mustFind(shown, 'button.job-default', 'Use defaults');
  for (const part of ['Running 4 of 4', 'Queued 2', 'Not queued 1']) {
    assert.ok(all.includes(part), `the strip lacks "${part}" (U11): ${all}`);
  }
  const lists = [
    ['ol.jobs-running', value.running, false],
    ['ol.jobs-waiting', value.waiting, true],
    ['ol.jobs-outside', value.outside, false],
  ];
  for (const [selector, entries, numbered] of lists) {
    const { list, lines } = jobLines(shown, selector);
    assert.ok(list, `the strip holds no ${selector} (U11)`);
    assert.deepEqual(lines.map((l) => l.id), entries.map((e) => e.id), `${selector}: one li[data-job-id] per job, in order`);
    lines.forEach((line, index) => {
      const parts = jobLineParts(entries[index], status, numbered ? index + 1 : undefined);
      assert.ok(inOrder(line.text, parts), `${selector} line ${index + 1} reads "${line.text}", not ${JSON.stringify(parts)} in order`);
      if (!numbered) assert.ok(!/^\d/.test(line.text), `${selector} line ${index + 1} has a position: ${line.text}`);
    });
  }
  assert.ok(!all.includes('over the limit'), 'the over the limit note shows at the limit');
});

test('machine_strip__should_show_settings_patterns_and_jobs empty_and_defaults', async () => {
  // Given
  const status = mustValidate(withJobs(document([worktree('fg'), worktree('wt1')]), jobs({ source: { limit: 'default', enforce: 'default' }, enforce: 'off' })));
  webview = await mount();
  webview.show(status);

  // When
  const shown = openStrip(webview);

  // Then
  const all = text(shown);
  assert.ok(all.includes(TEXT.notSaved), `the strip lacks "${TEXT.notSaved}" while the defaults apply (U11): ${all}`);
  assert.ok(!all.includes('Saved in'), 'the strip says Saved in while nothing is saved');
  for (const part of ['Running 0 of 4', 'Queued 0']) assert.ok(all.includes(part), `the strip lacks "${part}": ${all}`);
  assert.ok(!/Not queued/.test(all), `Not queued shows under off (U11): ${all}`);
  for (const selector of ['ol.jobs-running', 'ol.jobs-waiting']) {
    assert.ok(readsNone(shown.querySelector(selector)), `${selector} does not read none while empty (U11)`);
  }
});

test('machine_strip__should_show_settings_patterns_and_jobs over_the_limit', async () => {
  // Given
  const status = mustValidate(withJobs(busy, { ...busy.machine.jobs, limit: 2 }));
  webview = await mount();
  webview.show(status);

  // When
  const shown = openStrip(webview);

  // Then
  assert.ok(text(shown).includes(TEXT.over(4, 2)), `the strip lacks "${TEXT.over(4, 2)}" (U11): ${text(shown)}`);
});

test('machine_strip__should_show_settings_patterns_and_jobs old_wtpool', async () => {
  // Given
  const status = mustValidate(document([worktree('fg'), worktree('wt1')]));
  webview = await mount();
  webview.show(status);

  // When
  const shown = openStrip(webview);

  // Then
  assert.equal(text(shown), TEXT.oldWtpool, 'without machine.jobs the strip holds only the update note (U11)');
});

test('machine_strip__should_show_settings_patterns_and_jobs remote_folder', async () => {
  // Given
  const status = mustValidate(busy);
  webview = await mount();
  webview.post({ ...view(status), folder_remote: true });

  // When
  const shown = openStrip(webview);
  const remote = text(shown);
  webview.post(view(status));
  const local = text(strip(webview));

  // Then
  assert.ok(remote.includes(TEXT.remote('verify-mac')), `the strip lacks the remote note (U11, R9): ${remote}`);
  assert.ok(!local.includes(TEXT.remote('verify-mac')), 'the remote note shows for a local folder');
});

test('machine_strip__should_show_settings_patterns_and_jobs config_error', async () => {
  // Given
  const error = '/Users/verify/.config/wtpool/machine.toml: limit = 0 is out of bounds 1 to 64';
  const status = mustValidate(withJobs(busy, jobs({ limit: null, error })));
  webview = await mount();
  webview.show(status);

  // When
  const shown = openStrip(webview);

  // Then
  assert.ok(text(shown).includes(error), `the strip does not show the fault (jobs.md, "Failure modes"): ${text(shown)}`);
});

test('machine_strip__should_show_settings_patterns_and_jobs apart_from_a_row_strip', async () => {
  // Given
  webview = await mount();
  webview.show(mustValidate(busy));
  const shown = openStrip(webview);

  // When
  rows(webview.document)[1].click();
  webview.tick(20);

  // Then
  const detail = webview.document.querySelector('tr.detail');
  assert.ok(detail, 'setup: a selected row shows its detail strip');
  assert.notEqual(detail, shown, 'the machine strip is the row detail strip');
  assert.equal(strip(webview), shown, 'selecting a row closed the machine strip');
});

test('machine_strip__should_update_in_place', async () => {
  // Given
  const status = mustValidate(busy);
  const next = mustValidate(withJobs(busy, { ...busy.machine.jobs, running: busy.machine.jobs.running.slice(1), waiting: [busy.machine.jobs.waiting[1]] }));
  webview = await mount();
  webview.show(status);
  const shown = openStrip(webview);
  const limit = mustFind(shown, 'input.job-limit');
  choose(webview, mustFind(shown, 'select.job-enforce'), 'require');
  type(webview, limit, '6');

  // When
  webview.show(next);

  // Then
  assert.equal(strip(webview), shown, 'the strip was replaced, not updated in place (X15)');
  assert.equal(webview.document.activeElement, limit, 'focus left the Limit field');
  assert.equal(limit.value, '6', 'an unsaved Limit edit was lost (U11)');
  assert.equal(mustFind(shown, 'select.job-enforce').value, 'require', 'an unsaved Policy edit was lost (U11)');
  assert.ok(text(shown).includes('Running 3 of 4') && text(shown).includes('Queued 1'), `the counts did not follow the document: ${text(shown)}`);
  assert.deepEqual(
    [...shown.querySelectorAll('ol.jobs-waiting li[data-job-id]')].map((li) => li.dataset.jobId),
    [busy.machine.jobs.waiting[1].id],
    'the Queued list did not follow the document',
  );
});

test('machine_strip__should_update_in_place saved_values_follow', async () => {
  // Given
  webview = await mount();
  webview.show(mustValidate(busy));
  const shown = openStrip(webview);
  const limit = mustFind(shown, 'input.job-limit');
  type(webview, limit, '6');
  mustFind(shown, 'button.job-save').click();
  webview.tick(20);
  answer(webview, { ok: true });

  // When
  webview.show(mustValidate(withJobs(busy, { ...busy.machine.jobs, limit: 6 })));
  const saved = limit.value;
  webview.show(mustValidate(withJobs(busy, { ...busy.machine.jobs, limit: 8 })));

  // Then
  assert.equal(saved, '6', 'the saved limit');
  assert.equal(limit.value, '8', 'a limit set elsewhere does not show once the edit is saved (U11)');
});

const EXPECTED_STOPS = [
  'limit',
  'enforce',
  'save',
  'default',
  'chips:machine',
  'input:machine',
  'add:machine',
  'chips:repository',
  'input:repository',
  'add:repository',
  'suggested',
  'suggest-all',
  'row',
];

test('machine_strip__should_follow_the_keyboard focus_and_tab_order', async () => {
  // Given
  webview = await mount();
  webview.show(mustValidate(busy));

  // When
  const shown = openStrip(webview);

  // Then
  assert.equal(webview.document.activeElement, mustFind(shown, 'input.job-limit'), 'opening the strip does not focus the Limit field (X20)');
  assert.deepEqual(stopsAfterButton(webview), EXPECTED_STOPS, 'the Tab order from the machine button (X20, X22)');
});

for (const from of ['input.job-limit', 'select.job-enforce', 'button.job-save', 'button.chip-remove']) {
  test(`machine_strip__should_follow_the_keyboard escape_from_${from}`, async () => {
    // Given
    webview = await mount();
    webview.show(mustValidate(busy));
    const shown = openStrip(webview);
    const target = mustFind(shown, from);
    target.focus();
    const before = webview.posted.length;

    // When
    press(webview, target, 'Escape');

    // Then
    const found = mustButton(webview);
    assert.equal(strip(webview), null, `Escape in ${from} leaves the strip open (X20)`);
    assert.equal(found.getAttribute('aria-expanded'), 'false', 'aria-expanded after Escape');
    assert.equal(webview.document.activeElement, found, 'Escape does not return focus to the machine button');
    assert.deepEqual(settingsSince(webview, before), [], 'Escape sent a jobSettings message');
  });
}

for (const [name, act, expected] of [
  ['save', (s) => { type(webview, s.querySelector('input.job-limit'), '6'); s.querySelector('button.job-save').click(); }, { limit: 6 }],
  ['enter_in_the_field', (s) => { const f = s.querySelector('input.job-limit'); type(webview, f, '5'); press(webview, f, 'Enter'); }, { limit: 5 }],
  ['enter_in_the_menu', (s) => { const m = s.querySelector('select.job-enforce'); choose(webview, m, 'require'); press(webview, m, 'Enter'); }, { enforce: 'require' }],
  ['both', (s) => { type(webview, s.querySelector('input.job-limit'), '6'); choose(webview, s.querySelector('select.job-enforce'), 'off'); s.querySelector('button.job-save').click(); }, { limit: 6, enforce: 'off' }],
  ['use_defaults', (s) => s.querySelector('button.job-default').click(), { limit: null, enforce: null }],
  ['nothing_changed', (s) => s.querySelector('button.job-save').click(), null],
]) {
  test(`machine_strip__should_follow_the_keyboard sends_${name}`, async () => {
    // Given
    webview = await mount();
    webview.show(mustValidate(busy));
    const shown = openStrip(webview);
    for (const selector of ['input.job-limit', 'select.job-enforce', 'button.job-save', 'button.job-default']) mustFind(shown, selector);
    const before = webview.posted.length;

    // When
    act(shown);
    webview.tick(20);

    // Then
    const sent = settingsSince(webview, before);
    assert.deepEqual(sent, expected ? [{ type: 'jobSettings', ...expected }] : [], `${name}: the jobSettings sent (X20, X11)`);
  });
}

const invalid = [
  ['0', 1, 64],
  ['65', 1, 64],
  ['2.5', 1, 64],
  ['four', 1, 64],
  ['', 1, 64],
  ['-3', 1, 64],
];

for (const [value, min, max] of invalid) {
  test(`machine_strip__should_follow_the_keyboard invalid_${value || 'empty'}_of_${min}_to_${max}`, async () => {
    // Given
    webview = await mount();
    webview.show(mustValidate(withJobs(busy, { ...busy.machine.jobs, min, max, limit: min + 1, default: min + 1 })));
    const shown = openStrip(webview);
    const field = mustFind(shown, 'input.job-limit');
    const before = webview.posted.length;

    // When
    type(webview, field, value);
    const marked = field.getAttribute('aria-invalid');
    press(webview, field, 'Enter');
    mustFind(shown, 'button.job-save').click();
    webview.tick(20);

    // Then
    const message = TEXT.limitError(min, max);
    const error = shown.querySelector('.limit-error');
    assert.equal(marked, 'true', `typing "${value}" does not set aria-invalid (X20)`);
    assert.ok(error && text(error) === message, `.limit-error does not read "${message}": ${text(error)}`);
    const described = (field.getAttribute('aria-describedby') ?? '').split(/\s+/);
    assert.ok(error.id && described.includes(error.id), 'the error is not tied to the field by aria-describedby (X20)');
    assert.deepEqual(settingsSince(webview, before), [], `a save of "${value}" sent a jobSettings message (X20)`);
    assert.equal(webview.document.activeElement, field, 'focus left the field (jobs_invalid_value)');
    assert.ok(announced(webview).includes(message) || error.matches('[aria-live="polite"], [role="status"], [role="alert"]'), 'the error is not announced');
  });
}

for (const [value, min, max] of [['64', 1, 64], ['1', 1, 64]]) {
  test(`machine_strip__should_follow_the_keyboard valid_${value}_of_${min}_to_${max}`, async () => {
    // Given
    webview = await mount();
    webview.show(mustValidate(withJobs(busy, { ...busy.machine.jobs, min, max, limit: 4, default: 4 })));
    const shown = openStrip(webview);
    const field = mustFind(shown, 'input.job-limit');
    const before = webview.posted.length;

    // When
    type(webview, field, value);
    press(webview, field, 'Enter');

    // Then
    assert.notEqual(field.getAttribute('aria-invalid'), 'true', `"${value}" of ${min} to ${max} is marked invalid`);
    assert.deepEqual(settingsSince(webview, before), [{ type: 'jobSettings', limit: Number(value) }], `the save of "${value}"`);
  });
}

test('machine_strip__should_follow_the_keyboard waits_for_the_answer', async () => {
  // Given
  webview = await mount();
  webview.show(mustValidate(busy));
  const shown = openStrip(webview);
  const field = mustFind(shown, 'input.job-limit');
  const save = mustFind(shown, 'button.job-save');
  const start = webview.posted.length;
  type(webview, field, '6');
  save.click();
  webview.tick(20);

  // When
  type(webview, field, '7');
  save.click();
  webview.tick(20);
  const beforeAnswer = settingsSince(webview, start);
  const announcedBefore = announced(webview);
  answer(webview, { ok: true });
  const saidSaved = { strip: text(strip(webview)), live: announced(webview) };
  save.click();
  webview.tick(20);

  // Then
  assert.deepEqual(beforeAnswer, [{ type: 'jobSettings', limit: 6 }], 'a second save did not wait for the answer (X20)');
  assert.ok(!announcedBefore.includes('Saved.'), 'Saved. was announced before the answer');
  assert.ok(saidSaved.strip.includes('Saved.'), `the strip does not say Saved. (X20): ${saidSaved.strip}`);
  assert.ok(saidSaved.live.includes('Saved.'), `the live region does not announce Saved. (X20): ${saidSaved.live}`);
  assert.deepEqual(
    settingsSince(webview, start),
    [{ type: 'jobSettings', limit: 6 }, { type: 'jobSettings', limit: 7 }],
    'after the answer the second save goes out once',
  );
});

test('machine_strip__should_follow_the_keyboard shows_the_refusal', async () => {
  // Given
  const error = 'wtpool: /Users/verify/.config/wtpool/machine.toml is a symlink to /etc/x; edit it by hand';
  webview = await mount();
  webview.show(mustValidate(busy));
  const shown = openStrip(webview);
  const field = mustFind(shown, 'input.job-limit');
  type(webview, field, '6');
  press(webview, field, 'Enter');

  // When
  answer(webview, { ok: false, error });
  webview.show(mustValidate(busy));

  // Then
  assert.ok(text(strip(webview)).includes(error), `the strip does not show wtpool's refusal (X20, X21): ${text(strip(webview))}`);
  assert.ok(announced(webview).includes(error), 'the refusal is not announced (X20)');
  assert.equal(field.value, '6', 'the refused value left the field (jobs_invalid_value)');
  assert.ok(!text(strip(webview)).includes('Saved.'), 'a refusal says Saved.');
});
