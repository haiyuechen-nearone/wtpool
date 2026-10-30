import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { POOL, actor, ago, document, mustValidate, withActor, worktree } from '../support/docs.mjs';
import { mount, rowById, text } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

/**
 * The cases of U18's table: a model, as a harness names it, and the maker mark it gives. OpenCode's
 * models read `<provider>/<id>`, so the table matches the last part first.
 */
const MAKERS = [
  ['claude', 'claude-opus-5-5', 'anthropic'],
  ['claude_any_case', 'Claude-Sonnet-5', 'anthropic'],
  ['gpt', 'openai/gpt-5.2', 'openai'],
  ['o1', 'o1-preview', 'openai'],
  ['o3', 'openai/o3-mini', 'openai'],
  ['o4', 'o4-mini', 'openai'],
  ['codex', 'codex-mini-latest', 'openai'],
  ['gemini', 'google/gemini-3-pro', 'google'],
  ['gemma', 'gemma-3-27b', 'google'],
  ['deepseek', 'opencode/deepseek-v4.1-flash-free', 'deepseek'],
  ['glm', 'opencode/glm-5.3-flash-free', 'zhipu'],
  ['z_ai', 'z-ai/air-9', 'zhipu'],
  ['qwen', 'qwen3-coder', 'qwen'],
  ['kimi', 'moonshotai/kimi-k2', 'moonshot'],
  ['moonshot', 'moonshot-v1-8k', 'moonshot'],
  ['mistral', 'mistral-large', 'mistral'],
  ['codestral', 'codestral-latest', 'mistral'],
  ['devstral', 'mistralai/devstral-small', 'mistral'],
  ['llama', 'meta-llama/llama-4-scout', 'meta'],
  ['grok', 'xai/grok-4', 'xai'],
  ['minimax', 'minimax-m2', 'minimax'],
  ['last_part_first', 'deepseek/qwen3-8b', 'qwen'],
];

/** One worktree per case, `wt1` on, each with one working agent. */
function casesDocument(cases) {
  const worktrees = [worktree('fg')];
  cases.forEach((one, index) => {
    worktrees.push(withActor(`wt${index + 1}`, { key: `k${index + 1}`, label: `case ${index + 1}`, ...one }));
  });
  return mustValidate(document(worktrees));
}

const row = (name) => rowById(webview.document, `${POOL}/${name}`);

/** The first agent line of a row: the line, its state icon and its mark box. */
function firstLine(name) {
  const line = row(name).querySelectorAll(':scope > td')[4]?.querySelector('.line');
  return { line, icon: line?.querySelector('.line-icon') ?? null, mark: line?.querySelector('.line-mark') ?? null };
}

/** The ids a mark box names: its mark and its badge, null for each it does not carry. */
function marked(name) {
  const { mark } = firstLine(name);
  return { mark: mark?.getAttribute('data-mark') ?? null, badge: mark?.getAttribute('data-badge') ?? null };
}

for (const [name, model, maker] of MAKERS) {
  test(`agent_mark__should_follow_model_and_harness maker_${name}`, async () => {
    // Given
    const status = casesDocument([{ harness: 'opencode', model }]);
    webview = await mount();

    // When
    webview.show(status);

    // Then
    assert.deepEqual(marked('wt1'), { mark: maker, badge: 'opencode' }, `${model} under OpenCode: the maker's mark with OpenCode's badge (U18)`);
  });
}

/** The cases of the badge rule and of the harness's own mark, with the label each hover shows. */
const HARNESS_CASES = [
  ['own_maker_has_no_badge', { harness: 'claude-code', model: 'claude-opus-5-5' }, { mark: 'anthropic', badge: null }, 'claude-opus-5-5 · Claude Code'],
  ['another_maker_has_a_badge', { harness: 'claude-code', model: 'gpt-5.2' }, { mark: 'openai', badge: 'claude-code' }, 'gpt-5.2 · Claude Code'],
  ['harness_mark_without_a_model', { harness: 'opencode' }, { mark: 'opencode', badge: null }, 'OpenCode'],
  ['claude_code_without_a_model', { harness: 'claude-code' }, { mark: 'anthropic', badge: null }, 'Claude Code'],
  ['unknown_maker_gives_the_harness_mark', { harness: 'opencode', model: 'acme/foo-1' }, { mark: 'opencode', badge: null }, 'foo-1 · OpenCode'],
  ['harness_without_a_mark_or_a_model', { harness: 'aider' }, { mark: null, badge: null }, null],
  ['harness_without_a_mark_and_unknown_maker', { harness: 'aider', model: 'acme/foo-1' }, { mark: null, badge: null }, null],
];

for (const [name, overrides, expected] of HARNESS_CASES) {
  test(`agent_mark__should_follow_model_and_harness ${name}`, async () => {
    // Given
    const status = casesDocument([overrides]);
    webview = await mount();

    // When
    webview.show(status);

    // Then
    assert.deepEqual(marked('wt1'), expected, `the mark box of ${JSON.stringify(overrides)} (U18)`);
  });
}

test('agent_mark__should_follow_model_and_harness harness_without_a_mark_names_the_maker_of_its_model', async () => {
  // Given
  const status = casesDocument([{ harness: 'aider', model: 'claude-sonnet-5' }]);
  webview = await mount();

  // When
  webview.show(status);

  // Then
  assert.equal(marked('wt1').mark, 'anthropic', "the maker's mark of a model, whatever the harness (U18)");
});

test('agent_mark__should_follow_model_and_harness keeps_the_state_icon_and_adds_the_mark_box', async () => {
  // Given
  const states = [
    { state: 'working' },
    { state: 'quiet' },
    { state: 'waiting', reason: 'permission', action: { tool: 'shell', target: 'git status' } },
    { state: 'idle', action: null },
    { state: 'idle', action: null, error: 'rate_limit' },
  ].map((one) => ({ harness: 'opencode', model: 'opencode/glm-5.3-flash-free', ...one }));
  const withMarks = mustValidate(document([worktree('fg'), ...states.map((one, index) => withActor(`wt${index + 1}`, { key: `k${index + 1}`, ...one }))]));
  const bare = mustValidate(
    document([
      worktree('fg'),
      ...states.map(({ model: _, ...one }, index) => withActor(`wt${index + 1}`, { key: `k${index + 1}`, ...one, harness: 'aider' })),
    ]),
  );
  webview = await mount();

  // When
  webview.show(bare);
  const icons = states.map((_, index) => firstLine(`wt${index + 1}`).icon?.outerHTML);
  webview.show(withMarks);

  // Then
  states.forEach((_, index) => {
    const { line, icon, mark } = firstLine(`wt${index + 1}`);
    assert.ok(icon && mark, `wt${index + 1}: the line holds no .line-icon and .line-mark (U13)`);
    assert.equal(line.firstElementChild, icon, `wt${index + 1}: the state icon is not first (U13)`);
    assert.equal(icon.nextElementSibling, mark, `wt${index + 1}: the mark box does not follow the state icon (U13)`);
    assert.equal(icon.outerHTML, icons[index], `wt${index + 1}: the mark changed the state icon (U13)`);
    assert.equal(mark.getAttribute('aria-hidden'), 'true', `wt${index + 1}: screen readers read the mark box (U18)`);
    assert.ok(mark.querySelector('svg'), `wt${index + 1}: the mark box holds no SVG mark (U18)`);
  });
});

test('agent_mark__should_follow_model_and_harness leaves_the_box_empty_on_build_and_no_agent_lines', async () => {
  // Given
  const status = mustValidate(
    document([
      worktree('fg'),
      worktree('wt1', { branch: 'b', free: false, reasons: ['branch'], activity: 'building', builds: [{ pid: 4242, name: 'cargo-nextest' }] }),
      worktree('wt2'),
    ]),
  );
  webview = await mount();

  // When
  webview.show(status);

  // Then
  for (const name of ['wt1', 'wt2']) {
    const { line, icon, mark } = firstLine(name);
    assert.ok(line, `${name}: no .line`);
    assert.ok(icon && mark, `${name}: the line holds no .line-icon and .line-mark (U13)`);
    assert.equal(icon.nextElementSibling, mark, `${name}: the mark box does not follow the icon box (U13)`);
    assert.equal(mark.children.length, 0, `${name}: the mark box is not empty (U13)`);
    assert.equal(mark.textContent, '', `${name}: the mark box holds text (U13)`);
    assert.equal(mark.getAttribute('data-mark'), null, `${name}: the mark box names a mark (U13)`);
    assert.equal(mark.getAttribute('data-badge'), null, `${name}: the mark box names a badge (U13)`);
    assert.equal(line.getAttribute('title'), null, `${name}: the line has a tooltip (U18)`);
  }
});

test('agent_mark__should_follow_model_and_harness shows_a_mark_for_a_subagent_by_its_own_model', async () => {
  // Given
  const status = casesDocument([{ harness: 'opencode', kind: 'subagent', model: 'opencode/qwen3-coder', label: 'Explore' }]);
  webview = await mount();

  // When
  webview.show(status);

  // Then
  assert.deepEqual(marked('wt1'), { mark: 'qwen', badge: 'opencode' }, "a subagent's mark follows its own model (U18)");
});

/** The harness's display name, as the hover label reads it. */
const DISPLAY = { 'claude-code': 'Claude Code', opencode: 'OpenCode' };

/** The text of the hover label: the model's last `/` part and the harness, or the harness alone. */
function labelOf({ harness, model }) {
  const name = DISPLAY[harness] ?? harness;
  return model ? `${model.split('/').at(-1)} · ${name}` : name;
}

/** The label the page draws for a hovered mark: one element, hidden while no label shows. */
const LABEL = '.float-label';

/**
 * Sends the events of a pointer that moves onto `target` from elsewhere, as a browser orders them:
 * `pointerover` on the innermost element, then the compatibility mouse events.
 */
function pointerOnto(target) {
  const { window } = webview;
  const Kind = window.PointerEvent ?? window.MouseEvent;
  for (const type of ['pointerover', 'pointerenter', 'pointermove']) {
    target.dispatchEvent(new Kind(type, { bubbles: type !== 'pointerenter', cancelable: true, view: window, clientX: 40, clientY: 40 }));
  }
}

/** Moves the pointer onto the mark of the first line of a row. */
function enter(name) {
  const { mark } = firstLine(name);
  pointerOnto(mark.firstElementChild ?? mark);
}

/** Moves the pointer off the mark of the first line of a row onto the line's own text. */
function leave(name) {
  const { line, mark } = firstLine(name);
  const { window } = webview;
  const Kind = window.PointerEvent ?? window.MouseEvent;
  const text = [...line.children].find((child) => !child.matches('.line-icon, .line-mark, .visually-hidden')) ?? line;
  mark.dispatchEvent(new Kind('pointerout', { bubbles: true, cancelable: true, view: window, relatedTarget: text }));
  pointerOnto(text);
}

/** The texts of the labels that show. */
const labels = () => [...webview.document.querySelectorAll(`${LABEL}:not([hidden])`)].map(text);

/** The hidden text of the first line of a row, as screen readers read it. */
function spoken(name) {
  return [...firstLine(name).line.querySelectorAll('.visually-hidden')].map((el) => el.textContent).join('');
}

test('agent_mark__should_follow_model_and_harness hover_label_shows_300_ms_after_entering_and_goes_on_leaving', async () => {
  // Given
  const cases = MAKERS.map(([, model]) => ({ harness: 'opencode', model }));
  webview = await mount();
  webview.show(casesDocument(cases));

  for (const [index, one] of cases.entries()) {
    const name = `wt${index + 1}`;

    // When
    enter(name);
    webview.tick(299);
    const early = labels();
    webview.tick(1);
    const shown = labels();
    leave(name);
    webview.tick(20);
    const after = labels();

    // Then
    assert.deepEqual(early, [], `${one.model}: a label shows before 300 ms (U18)`);
    assert.deepEqual(shown, [labelOf(one)], `${one.model}: the label 300 ms after the pointer enters the mark (U18)`);
    assert.deepEqual(after, [], `${one.model}: the label stays after the pointer left the mark (U18)`);
  }
});

test('agent_mark__should_follow_model_and_harness hover_label_names_the_harness_alone_without_a_model', async () => {
  // Given
  const cases = HARNESS_CASES.map(([, overrides]) => overrides);
  webview = await mount();
  webview.show(casesDocument(cases));

  for (const [index, [name, overrides, , label]] of HARNESS_CASES.entries()) {
    // When
    enter(`wt${index + 1}`);
    webview.tick(300);
    const shown = labels();
    leave(`wt${index + 1}`);
    webview.tick(20);

    // Then
    assert.deepEqual(shown, label === null ? [] : [label], `${name}: the label of ${JSON.stringify(overrides)} (U18)`);
    assert.deepEqual(labels(), [], `${name}: the label stays after the pointer left the mark (U18)`);
  }
});

test('agent_mark__should_follow_model_and_harness hover_label_waits_for_a_pointer_that_stays', async () => {
  // Given
  webview = await mount();
  webview.show(casesDocument([{ harness: 'opencode', model: 'opencode/deepseek-v4.1-flash-free' }]));

  // When
  enter('wt1');
  webview.tick(200);
  leave('wt1');
  webview.tick(400);

  // Then
  assert.deepEqual(labels(), [], 'a pointer that left the mark within 300 ms still got its label (U18)');
});

test('agent_mark__should_follow_model_and_harness hover_label_shows_one_at_a_time', async () => {
  // Given
  const cases = [
    { harness: 'opencode', model: 'opencode/deepseek-v4.1-flash-free' },
    { harness: 'claude-code', model: 'claude-opus-5-5' },
  ];
  webview = await mount();
  webview.show(casesDocument(cases));

  // When
  enter('wt1');
  webview.tick(300);
  const first = labels();
  leave('wt1');
  enter('wt2');
  webview.tick(300);
  const second = labels();

  // Then
  assert.deepEqual(first, [labelOf(cases[0])], 'the first mark shows no label (U18)');
  assert.deepEqual(second, [labelOf(cases[1])], 'the labels of both marks show at once, or the second does not show (U18)');
});

test('agent_mark__should_follow_model_and_harness line_has_no_title_and_screen_readers_get_the_label', async () => {
  // Given
  const cases = [
    { harness: 'opencode', model: 'opencode/deepseek-v4.1-flash-free' },
    { harness: 'claude-code', model: 'claude-opus-5-5' },
    { harness: 'opencode' },
  ];
  webview = await mount();

  // When
  webview.show(casesDocument(cases));

  // Then
  cases.forEach((one, index) => {
    const name = `wt${index + 1}`;
    const { line, mark } = firstLine(name);
    const titled = [line, ...line.querySelectorAll('[title]')].filter((el) => el.hasAttribute('title'));
    assert.deepEqual(titled.map((el) => el.className), [], `${name}: the line or one of its parts has a title, which would show a second label (U18)`);
    assert.ok(mark, `${name}: no mark box`);
    assert.equal(mark.getAttribute('data-label'), labelOf(one), `${name}: the mark box does not carry its label's text in data-label (U18)`);
    const hidden = spoken(name);
    assert.ok(hidden.startsWith('working: '), `${name}: the hidden text does not start with the state: ${JSON.stringify(hidden)} (U13)`);
    assert.ok(hidden.slice('working: '.length).includes(labelOf(one)), `${name}: screen readers get no ${JSON.stringify(labelOf(one))} after the state: ${JSON.stringify(hidden)} (U18)`);
  });
});

/** fg holds the coordinator, wt1 the agent it launched, wt2 one whose launcher no row lists. */
function launchedDocument() {
  const coordinator = actor({ key: 's1', label: 'Coordinate the task', harness: 'claude-code', model: 'claude-opus-5-5', updated: ago(5) });
  const launched = actor({
    key: 'opencode-ses-e76df7aa0ffe3vdlGbn0I4xCYq',
    label: 'fix the docs',
    harness: 'opencode',
    model: 'opencode/deepseek-v4.1-flash-free',
    launched_by: 's1',
    updated: ago(4),
  });
  const stray = actor({ key: 'opencode-ses-e76df7a90ffd2KqPz81mWx0Tfn', label: 'stray run', harness: 'opencode', launched_by: 'gone-session', updated: ago(3) });
  const bare = actor({ key: 'plain', label: 'plain agent', harness: 'aider', updated: ago(2) });
  return mustValidate(
    document([
      worktree('fg', { activity: 'working', actors: [coordinator], last_event: coordinator.updated }),
      withActor('wt1', launched),
      withActor('wt2', stray),
      withActor('wt3', bare),
    ]),
  );
}

/** The detail strip's text once the row `name` is clicked open. */
function opened(name) {
  row(name).click();
  const strip = row(name).nextElementSibling;
  assert.ok(strip?.matches('tr.detail'), `setup: clicking ${name} opened no detail strip after it`);
  return text(strip);
}

test('agent_mark__should_follow_model_and_harness detail_strip_names_harness_model_and_launcher', async () => {
  // Given
  webview = await mount();
  webview.show(launchedDocument());

  // When
  const launched = opened('wt1');

  // Then
  assert.ok(launched.includes('opencode · opencode/deepseek-v4.1-flash-free'), `the strip does not read "<harness> · <model>" (U5): ${launched}`);
  assert.ok(launched.includes('launched by Coordinate the task'), `the strip does not name the launcher's label (S17, U5): ${launched}`);
});

test('agent_mark__should_follow_model_and_harness detail_strip_names_the_launcher_key_when_no_row_lists_it', async () => {
  // Given
  webview = await mount();
  webview.show(launchedDocument());

  // When
  const stray = opened('wt2');

  // Then
  assert.ok(stray.includes('launched by gone-session'), `the strip does not name the launcher's key (S17, U5): ${stray}`);
  assert.ok(stray.includes('opencode'), `the strip does not name the harness of an agent with no model (U5): ${stray}`);
  assert.ok(!/opencode · (?!launched by)/.test(stray), `the strip shows a model for an agent with none (U5): ${stray}`);
});

test('agent_mark__should_follow_model_and_harness detail_strip_omits_launched_by_for_an_agent_nobody_launched', async () => {
  // Given
  webview = await mount();
  webview.show(launchedDocument());

  // When
  const coordinator = opened('fg');
  const bare = opened('wt3');

  // Then
  assert.ok(coordinator.includes('claude-code · claude-opus-5-5'), `the strip does not read "<harness> · <model>" for Claude Code (U5): ${coordinator}`);
  assert.ok(!coordinator.includes('launched by'), `the strip says the coordinator was launched (S17): ${coordinator}`);
  assert.ok(bare.includes('aider'), `the strip does not name the harness (U5): ${bare}`);
  assert.ok(!bare.includes('launched by'), `the strip says an agent nobody launched was launched (S17): ${bare}`);
});

/** The labelled element under the pointer that a later document must hand the label to. */
function handedTo(webview, element) {
  webview.document.elementFromPoint = () => element.querySelector?.('svg') ?? element;
}

test('agent_mark__should_follow_model_and_harness hover_label_moves_with_its_row', async () => {
  // Given: a shown label on wt1's mark, placed by the box of that mark.
  const status = casesDocument([{ harness: 'opencode', model: 'opencode/qwen3-coder' }]);
  webview = await mount();
  webview.show(status);
  const target = firstLine('wt1').mark;
  target.getBoundingClientRect = () => ({ left: 120, top: 40, bottom: 56, right: 134, width: 14, height: 16 });
  enter('wt1');
  webview.tick(300);
  const label = webview.document.querySelector(`${LABEL}:not([hidden])`);
  const placed = label?.style.top;

  // When: the mark's box moves, as when its row does, and the next document shows the same rows.
  target.getBoundingClientRect = () => ({ left: 30, top: 90, bottom: 106, right: 44, width: 14, height: 16 });
  webview.show(casesDocument([{ harness: 'opencode', model: 'opencode/qwen3-coder' }]));

  // Then: the same label is still shown and placed again by the mark's new box.
  assert.ok(label, 'setup: no label shows on the mark');
  assert.equal(placed, '36px', 'setup: the label is not placed above its mark');
  assert.deepEqual(labels(), ['qwen3-coder · OpenCode'], 'the moved label is no longer the mark\'s');
  assert.equal(label.style.top, '86px', 'the shown label was not placed again after its row moved');
});

test('agent_mark__should_follow_model_and_harness hover_label_hands_over_to_the_mark_under_the_pointer', async () => {
  // Given: a shown label on wt1's mark, and wt2's mark lying under the pointer.
  webview = await mount();
  webview.show(casesDocument([{ harness: 'opencode', model: 'opencode/deepseek-v4.1-flash-free' }, { harness: 'opencode', model: 'opencode/qwen3-coder' }]));
  handedTo(webview, firstLine('wt2').mark);
  enter('wt1');
  webview.tick(300);

  // When: the next document retires wt1's row, with the pointer left where it was.
  webview.show(casesDocument([{ harness: 'opencode', model: 'opencode/qwen3-coder' }]));

  // Then: the shown label belongs to the labelled element now under the pointer.
  assert.deepEqual(labels(), ['qwen3-coder · OpenCode'], 'the label of a retired row did not hand over to the mark under the pointer');
});

test('agent_mark__should_follow_model_and_harness hover_label_waits_again_for_the_mark_under_the_pointer', async () => {
  // Given: the pointer rests on wt1's mark, its label not shown yet, and wt2's mark lies under it.
  webview = await mount();
  webview.show(casesDocument([{ harness: 'opencode', model: 'opencode/deepseek-v4.1-flash-free' }, { harness: 'opencode', model: 'opencode/qwen3-coder' }]));
  handedTo(webview, firstLine('wt2').mark);
  enter('wt1');
  webview.tick(200);

  // When
  webview.show(casesDocument([{ harness: 'opencode', model: 'opencode/qwen3-coder' }]));
  const early = labels();
  webview.tick(300);
  const shown = labels();

  // Then
  assert.deepEqual(early, [], 'a label shows at once for the mark under the pointer');
  assert.deepEqual(shown, ['qwen3-coder · OpenCode'], 'the label of the mark under the pointer never showed');
});

test('agent_mark__should_follow_model_and_harness hover_label_forgets_a_retired_row_with_nothing_labelled_under_it', async () => {
  // Given: a shown label on wt1's mark, with no labelled element under the pointer.
  webview = await mount();
  webview.show(casesDocument([{ harness: 'opencode', model: 'opencode/deepseek-v4.1-flash-free' }, { harness: 'opencode', model: 'opencode/qwen3-coder' }]));
  webview.document.elementFromPoint = () => webview.document.body;
  enter('wt1');
  webview.tick(300);

  // When
  webview.show(casesDocument([{ harness: 'opencode', model: 'opencode/qwen3-coder' }]));
  const after = labels();
  webview.tick(400);

  // Then
  assert.deepEqual(after, [], 'the label of a retired row stays with no labelled element under the pointer');
  assert.deepEqual(labels(), [], 'the label came back after its delay');
});
