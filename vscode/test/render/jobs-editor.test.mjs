import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { mustValidate } from '../support/docs.mjs';
import { normalizeEscapes, visible } from '../support/grammar.mjs';
import { PATTERN_CASES, TEXT, busyDocument, firstOpenDocument, isPattern, jobs, scopePatterns, suggestionGroups, withJobs } from '../support/jobs.mjs';
import {
  accessibleName,
  announced,
  answer,
  chips,
  mustFind,
  openStrip,
  press,
  scope,
  settingsSince,
  stopsAfterButton,
  suggestions,
  type,
} from '../support/jobs-render.mjs';
import { mount, text } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

const busy = busyDocument();
const first = firstOpenDocument();
const SCOPE_WORDS = { machine: 'this machine', repository: 'this repository' };

function mustScope(name) {
  const parts = scope(webview, name);
  assert.ok(parts.input, `the ${name} list has no input.heavy-input (U15)`);
  assert.ok(parts.add, `the ${name} list has no button.heavy-add (U15)`);
  return parts;
}

/** The text of an error tied to `input` by aria-describedby. */
function describedError(input) {
  const ids = (input.getAttribute('aria-describedby') ?? '').split(/\s+/).filter(Boolean);
  return ids.map((id) => text(webview.document.getElementById(id))).join(' ').trim();
}

/** A case name in printable ASCII: the pattern as JSON, other characters as `\uXXXX`. */
const ascii = (pattern) => JSON.stringify(pattern).replace(/[^\x20-\x7e]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`);

const editable = () =>
  mustValidate(withJobs(busy, jobs({ heavy: [{ pattern: 'just', source: 'machine' }, { pattern: 'protoc', source: 'build_processes' }] })));

for (const [pattern, valid] of PATTERN_CASES) {
  test(`pattern__should_match_the_cli_grammar ${ascii(pattern)}`, async () => {
    // Given
    assert.equal(isPattern(pattern), valid, 'setup: the oracle model differs from the case');
    webview = await mount();
    webview.show(editable());
    openStrip(webview);

    // When
    const seen = [];
    for (const name of ['machine', 'repository']) {
      for (const how of ['enter', 'add']) {
        const parts = mustScope(name);
        const before = webview.posted.length;
        type(webview, parts.input, pattern);
        if (how === 'enter') press(webview, parts.input, 'Enter');
        else parts.add.click();
        webview.tick(20);
        const sent = settingsSince(webview, before);
        seen.push({ name, how, sent, invalid: parts.input.getAttribute('aria-invalid'), error: describedError(parts.input) });
        if (sent.length) answer(webview, { ok: true, scope: name });
      }
    }

    // Then
    for (const { name, how, sent, invalid, error } of seen) {
      const what = `${JSON.stringify(pattern)} by ${how} in ${name}`;
      if (valid) {
        assert.deepEqual(sent, [{ type: 'jobSettings', heavy: { op: 'add', scope: name, pattern } }], `${what}: one add (U15, X11)`);
      } else {
        assert.deepEqual(sent, [], `${what}: a pattern outside J18 was sent (U15)`);
        assert.equal(invalid, 'true', `${what}: the input is not marked aria-invalid`);
        assert.equal(error, TEXT.patternError, `${what}: the error tied to the input (U15)`);
      }
    }
  });
}

for (const [pattern, name, duplicate] of [
  ['just', 'machine', true],
  ['protoc', 'repository', true],
  ['just', 'repository', false],
  ['protoc', 'machine', false],
]) {
  test(`pattern__should_match_the_cli_grammar ${duplicate ? 'already' : 'not_yet'}_in_${name}_${pattern}`, async () => {
    // Given
    webview = await mount();
    webview.show(editable());
    openStrip(webview);
    const parts = mustScope(name);
    const before = webview.posted.length;

    // When
    type(webview, parts.input, pattern);
    press(webview, parts.input, 'Enter');

    // Then
    const sent = settingsSince(webview, before);
    if (duplicate) {
      assert.deepEqual(sent, [], `${pattern} is already in the ${name} list and was sent (U15)`);
      assert.equal(describedError(parts.input), TEXT.duplicate, 'the duplicate error tied to the input');
    } else {
      assert.deepEqual(sent, [{ type: 'jobSettings', heavy: { op: 'add', scope: name, pattern } }], `${pattern} in another list blocks the ${name} add`);
    }
  });
}

test('heavy_editor__should_show_each_scope busy', async () => {
  // Given
  const value = busy.machine.jobs;
  webview = await mount();
  webview.show(mustValidate(busy));

  // When
  const shown = openStrip(webview);

  // Then
  const all = text(shown);
  for (const part of ['Heavy commands', 'This machine', 'This repository · .git/wtpool.toml', 'Suggested for this repository', TEXT.nothingHeavy]) {
    assert.ok(all.includes(part), `the editor lacks "${part}" (U15): ${all}`);
  }
  for (const name of ['machine', 'repository']) {
    const expected = scopePatterns(value, name);
    const found = chips(webview, name);
    assert.deepEqual(
      found.map((c) => c.text.replace(/\s*build_processes$/, '')),
      expected.map((e) => e.pattern),
      `the ${name} chips ul.heavy-chips[data-scope] of li.heavy-chip (U15)`,
    );
    found.forEach((chip, index) => {
      const tagged = expected[index].source === 'build_processes';
      assert.equal(/build_processes$/.test(chip.text), tagged, `${expected[index].pattern}: the build_processes tag (U15)`);
      assert.ok(chip.remove, `${expected[index].pattern}: no button.chip-remove`);
      assert.equal(accessibleName(chip.remove), `Remove ${expected[index].pattern} from ${SCOPE_WORDS[name]}`, 'the remove button name (U15)');
    });
    const parts = mustScope(name);
    assert.equal(parts.input.getAttribute('placeholder'), TEXT.placeholder, `the ${name} input placeholder`);
  }
  const groups = suggestionGroups(value);
  const shownSuggestions = suggestions(webview);
  assert.deepEqual(
    shownSuggestions.map((s) => [s.from, s.text]),
    groups.flatMap((g) => g.patterns.map((p) => [g.from, p])),
    'the suggestions ul.heavy-suggested of li.heavy-chip[data-from], grouped by from (U15)',
  );
  for (const suggestion of shownSuggestions) {
    assert.ok(suggestion.add, `${suggestion.text}: no button.suggest-add`);
    assert.equal(accessibleName(suggestion.add), `Add ${suggestion.text} to this repository`, 'the add button name (U15)');
  }
  assert.ok(mustFind(shown, 'button.suggest-all', 'Add all'));
  assert.ok(all.indexOf('from Cargo.toml') >= 0 && all.indexOf('from Cargo.toml') < all.indexOf('cargo nextest', all.indexOf('Suggested')), `the group label precedes its chips: ${all}`);
});

test('heavy_editor__should_show_each_scope first_open', async () => {
  // Given
  webview = await mount();
  webview.show(mustValidate(first));

  // When
  const shown = openStrip(webview);

  // Then
  for (const name of ['machine', 'repository']) {
    const parts = mustScope(name);
    assert.equal(chips(webview, name).length, 0, `the ${name} list holds chips`);
    assert.ok(text(parts.box).includes('none'), `the empty ${name} list does not read none (U15): ${text(parts.box)}`);
  }
  const all = text(shown);
  let at = all.indexOf('Suggested for this repository');
  assert.ok(at >= 0, `no suggestions heading: ${all}`);
  for (const group of suggestionGroups(first.machine.jobs)) {
    const label = all.indexOf(group.label, at);
    assert.ok(label >= 0, `no group "${group.label}" after the previous group (U15): ${all}`);
    for (const pattern of group.patterns) {
      const found = all.indexOf(pattern, label + group.label.length);
      assert.ok(found >= 0, `${pattern} does not follow "${group.label}"`);
      at = Math.max(at, found);
    }
  }
  assert.deepEqual(suggestions(webview).map((s) => s.text), first.machine.jobs.suggested.map((s) => s.pattern), 'the seven suggestions in order');
});

test('heavy_editor__should_show_each_scope no_suggestions', async () => {
  // Given
  webview = await mount();
  webview.show(mustValidate(withJobs(busy, { ...busy.machine.jobs, suggested: [] })));

  // When
  const shown = openStrip(webview);

  // Then
  assert.ok(chips(webview, 'repository').length > 0, 'the repository chips (U15)');
  assert.ok(!text(shown).includes('Suggested for this repository'), 'the suggestions section shows without suggestions');
  assert.equal(shown.querySelectorAll('button.suggest-all, ul.heavy-suggested li').length, 0, 'Add all or a suggestion shows without suggestions');
});

test('heavy_editor__should_show_each_scope hostile_from', async () => {
  // Given
  const from = 'x<img src=x onerror=alert(1)>‮bad.csproj';
  webview = await mount();
  webview.show(mustValidate(withJobs(first, { ...first.machine.jobs, suggested: [{ pattern: 'dotnet build', from }] })));

  // When
  const shown = openStrip(webview);

  // Then
  assert.equal(shown.querySelectorAll('img').length, 0, 'a marker name became markup (X10)');
  assert.ok(normalizeEscapes(text(shown)).includes(normalizeEscapes(`from ${visible(from)}`)), `the marker name is not shown literally (X10): ${text(shown)}`);
  assert.equal(suggestions(webview)[0]?.from, from, 'data-from holds the marker');
});

test('heavy_editor__should_show_each_scope refusal_under_its_input', async () => {
  // Given
  const error = 'wtpool: /Users/verify/pool/fg/.git/wtpool.toml: a comment inside heavy, line 3; edit it by hand';
  webview = await mount();
  webview.show(mustValidate(busy));
  openStrip(webview);
  const repository = mustScope('repository');
  type(webview, repository.input, 'go test');
  press(webview, repository.input, 'Enter');

  // When
  answer(webview, { ok: false, error, scope: 'repository' });
  webview.show(mustValidate(busy));

  // Then
  assert.equal(describedError(mustScope('repository').input), error, 'the CLI refusal under the repository input (U15, X21)');
  assert.notEqual(describedError(mustScope('machine').input), error, 'the refusal shows under the machine input too');
});

test('heavy_editor__should_follow_the_keyboard one_stop_per_list', async () => {
  // Given
  webview = await mount();
  webview.show(mustValidate(busy));

  // When
  openStrip(webview);

  // Then
  const stops = stopsAfterButton(webview);
  assert.deepEqual(
    stops.slice(stops.indexOf('chips:machine')),
    ['chips:machine', 'input:machine', 'add:machine', 'chips:repository', 'input:repository', 'add:repository', 'suggested', 'suggest-all', 'row'],
    'each list one tab stop, then its input and Add, or Add all (X22)',
  );
  const value = busy.machine.jobs;
  const labels = [
    [`ul.heavy-chips[data-scope="machine"]`, `Heavy commands of this machine, ${scopePatterns(value, 'machine').length}`],
    [`ul.heavy-chips[data-scope="repository"]`, `Heavy commands of this repository, ${scopePatterns(value, 'repository').length}`],
    ['ul.heavy-suggested', `Suggested for this repository, ${value.suggested.length}`],
  ];
  for (const [selector, label] of labels) {
    const list = mustFind(webview.document, `tr.machine-detail ${selector}`);
    assert.equal(list.getAttribute('role'), 'list', `${selector} has no role="list" (X22)`);
    assert.equal(accessibleName(list), label, `${selector}: its label (X22)`);
  }
});

test('heavy_editor__should_follow_the_keyboard arrows_home_end', async () => {
  // Given
  webview = await mount();
  webview.show(mustValidate(busy));
  openStrip(webview);
  const buttons = chips(webview, 'repository').map((c) => c.remove);
  assert.equal(buttons.length, 4, 'the four repository chips');
  buttons[0].focus();

  // When
  const path = [];
  for (const key of ['ArrowRight', 'ArrowDown', 'ArrowLeft', 'End', 'ArrowUp', 'Home']) {
    press(webview, webview.document.activeElement, key);
    path.push(buttons.indexOf(webview.document.activeElement));
  }
  press(webview, webview.document.activeElement, 'ArrowRight');
  webview.document.activeElement.blur();
  const tabbable = buttons.map((b) => b.getAttribute('tabindex'));

  // Then
  assert.deepEqual(path, [1, 2, 1, 3, 2, 0], 'Right, Down, Left, End, Up and Home move focus among the chips (X22)');
  assert.deepEqual(tabbable, ['-1', '0', '-1', '-1'], 'focus does not enter the list on the last focused chip (X22)');
});

/**
 * Presses `key` on `target`, answers with `fields`, then shows `next`. Gives the settings sent and
 * the live region's text before and after the answer.
 */
function act(key, target, fields, next) {
  const before = webview.posted.length;
  target.focus();
  press(webview, target, key);
  const sent = settingsSince(webview, before);
  const early = announced(webview);
  answer(webview, { ok: true, ...fields });
  const said = announced(webview);
  webview.show(mustValidate(next));
  return { sent, early, said };
}

const without = (pattern) => withJobs(busy, { ...busy.machine.jobs, heavy: busy.machine.jobs.heavy.filter((h) => h.pattern !== pattern) });

for (const key of ['Delete', 'Backspace', 'Enter', ' ']) {
  test(`heavy_editor__should_follow_the_keyboard remove_by_${key === ' ' ? 'space' : key.toLowerCase()}`, async () => {
    // Given
    webview = await mount();
    webview.show(mustValidate(busy));
    openStrip(webview);
    const target = chips(webview, 'repository')[1]?.remove;
    assert.ok(target, 'no remove button on the second repository chip');

    // When
    const { sent, early, said } = act(key, target, { scope: 'repository' }, without('cargo test'));

    // Then
    assert.deepEqual(sent, [{ type: 'jobSettings', heavy: { op: 'rm', scope: 'repository', pattern: 'cargo test' } }], `${key} on a chip (X22)`);
    assert.ok(!early.includes('Removed cargo test.'), 'Removed was announced before the answer');
    assert.ok(said.includes('Removed cargo test.'), `the live region does not say Removed cargo test. (X22): ${said}`);
    assert.equal(webview.document.activeElement, chips(webview, 'repository')[1]?.remove, 'focus did not move to the next chip (X22)');
  });
}

test('heavy_editor__should_follow_the_keyboard last_chip_goes_to_the_input', async () => {
  // Given
  webview = await mount();
  webview.show(mustValidate(busy));
  openStrip(webview);
  const target = chips(webview, 'machine')[0]?.remove;
  assert.ok(target, 'no remove button on the machine chip');

  // When
  act('Delete', target, { scope: 'machine' }, without('just'));

  // Then
  assert.equal(webview.document.activeElement, mustScope('machine').input, 'focus did not move to the machine input after its last chip (X22)');
});

test('heavy_editor__should_follow_the_keyboard last_of_a_list_goes_to_the_previous', async () => {
  // Given
  webview = await mount();
  webview.show(mustValidate(busy));
  openStrip(webview);
  const target = chips(webview, 'repository')[3]?.remove;
  assert.ok(target, 'no remove button on the last repository chip');

  // When
  act('Backspace', target, { scope: 'repository' }, without('protoc'));

  // Then
  assert.equal(webview.document.activeElement, chips(webview, 'repository')[2]?.remove, 'focus did not move to the previous chip (X22)');
});

for (const key of ['Enter', ' ']) {
  test(`heavy_editor__should_follow_the_keyboard add_suggestion_by_${key === ' ' ? 'space' : 'enter'}`, async () => {
    // Given
    webview = await mount();
    webview.show(mustValidate(busy));
    openStrip(webview);
    const target = suggestions(webview)[0]?.add;
    assert.ok(target, 'no button.suggest-add');
    const next = withJobs(busy, {
      ...busy.machine.jobs,
      heavy: [...busy.machine.jobs.heavy, { pattern: 'cargo nextest', source: 'repository' }],
      suggested: busy.machine.jobs.suggested.slice(1),
    });

    // When
    const { sent, said } = act(key, target, { scope: 'repository' }, next);

    // Then
    assert.deepEqual(sent, [{ type: 'jobSettings', heavy: { op: 'add', scope: 'repository', pattern: 'cargo nextest' } }], `${key} on a suggestion (X22)`);
    assert.ok(said.includes('Added cargo nextest.'), `the live region does not say Added cargo nextest. (X22): ${said}`);
    assert.equal(webview.document.activeElement, suggestions(webview)[0]?.add, 'focus did not move to the next suggestion (X22)');
  });
}

test('heavy_editor__should_follow_the_keyboard add_all', async () => {
  // Given
  webview = await mount();
  webview.show(mustValidate(busy));
  const shown = openStrip(webview);
  const all = mustFind(shown, 'button.suggest-all', 'Add all');
  const before = webview.posted.length;

  // When
  all.click();
  webview.tick(20);
  const sent = settingsSince(webview, before);
  answer(webview, { ok: true, scope: 'repository' });

  // Then
  assert.deepEqual(sent, [{ type: 'jobSettings', heavy: { op: 'add-suggested', scope: 'repository', pattern: null } }], 'Add all (X11, X22)');
  assert.ok(announced(webview).includes('Added 2 suggestions.'), `the live region does not say Added 2 suggestions. (X22): ${announced(webview)}`);
});

test('heavy_editor__should_follow_the_keyboard enter_in_an_input_and_errors', async () => {
  // Given
  const error = 'wtpool: go test is already in /Users/verify/.config/wtpool/machine.toml';
  webview = await mount();
  webview.show(mustValidate(busy));
  openStrip(webview);
  const machine = mustScope('machine');
  const before = webview.posted.length;

  // When
  type(webview, machine.input, 'go test');
  press(webview, machine.input, 'Enter');
  const sent = settingsSince(webview, before);
  answer(webview, { ok: false, error, scope: 'machine' });

  // Then
  assert.deepEqual(sent, [{ type: 'jobSettings', heavy: { op: 'add', scope: 'machine', pattern: 'go test' } }], 'Enter in the machine input adds (X22)');
  assert.ok(announced(webview).includes(error), `the live region does not say the error (X22): ${announced(webview)}`);
});
