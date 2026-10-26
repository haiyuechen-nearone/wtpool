import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { mustValidate } from '../support/docs.mjs';
import { busyDocument, firstOpenDocument, TEXT } from '../support/jobs.mjs';
import { buttonFacts, chipFacts, notQueuedDocument, openStrip, showView, stripFacts, stripStates, tabToButton } from '../support/jobs-shot.mjs';
import { axeRun, clipping, ring, ringProblems, textContrast } from '../support/measure.mjs';
import { THEME_NAMES, computedThemeColours } from '../support/page.mjs';
import { away, settle, visualPanel } from '../support/states.mjs';

const WIDTHS = [1100, 600];
const HIGH_CONTRAST = new Set(['high-contrast', 'high-contrast-light']);

test.describe.configure({ mode: 'parallel' });

/** Opens the panel in a context that lets axe-core run, at `width` in `theme`. */
async function jobsPanel(browser, theme, width) {
  const context = await browser.newContext({ bypassCSP: true, viewport: { width, height: 1300 } });
  const page = await context.newPage();
  const panel = await visualPanel(page, { theme, width, height: 1300, extraBodyClass: 'vscode-reduce-motion' });
  const colours = await computedThemeColours(page, panel.theme);
  return { context, page, panel, colours };
}

async function showOpen(page, doc, extra, at) {
  await showView(page, doc, extra);
  await away(page);
  await openStrip(page, at);
  await away(page);
  await settle(page);
}

/** The clipping issues and AA failures of the page, one line each, `at` naming the state. */
async function looks(page, at) {
  const { issues } = await page.evaluate(clipping, false);
  const low = await page.evaluate(textContrast);
  return [
    ...issues.map((i) => `${at}: ${i.kind} in the ${i.place ?? i.side} ${JSON.stringify(i)}`),
    ...low.map((f) => `${at}: ${f.key} ${f.ratio}:1 under ${f.need}:1, "${f.text}"`),
  ];
}

/** The focused control's selector, else the focused element's tag and classes. */
function focusedControl() {
  const el = document.activeElement;
  if (!el || el === document.body) return null;
  const names = ['input.job-limit', 'select.job-enforce', 'button.job-save', 'button.job-default', 'button.chip-remove', 'button.suggest-add', 'button.suggest-all', 'button.heavy-add', 'input.heavy-input', 'button.machine-button'];
  const name = names.find((selector) => el.matches(selector));
  return { name: name ?? `${el.tagName.toLowerCase()}.${String(el.className).trim().split(/\s+/).join('.')}`, inStrip: Boolean(el.closest('tr.machine-detail')) };
}

for (const theme of THEME_NAMES) {
  for (const width of WIDTHS) {
    test(`machine_strip__should_clip_nothing_in_every_theme ${theme} ${width}`, async ({ browser }) => {
      // Given
      const { context, page, panel, colours } = await jobsPanel(browser, theme, width);
      const problems = [];
      const description = colours['--vscode-descriptionForeground'];

      // When
      for (const state of stripStates()) {
        const at = `${state.name} at ${width} px`;
        await showOpen(page, state.doc, state.extra, at);
        const facts = await page.evaluate(stripFacts);
        problems.push(...(await looks(page, at)));
        for (const v of (await axeRun(page)).violations) problems.push(`${at}: axe ${v}`);
        assert.ok(facts.present && facts.shown, `${at}: the machine strip tr.machine-detail is not shown (U11)`);
        if (state.text) assert.ok(facts.text.includes(state.text), `${at}: the strip does not say "${state.text}": ${facts.text}`);
        const boxed = facts.boxes.some((b) => b.sides.every((s) => Math.abs(s.width - 1) < 0.01 && s.style !== 'none' && s.colour === description) && b.ratio >= 3);
        if (!boxed) problems.push(`${at}: no element holding the strip's text has a 1 px ${description} border at 3:1 (U11, U6): ${JSON.stringify(facts.boxes)}`);
        for (const c of facts.controls) {
          const select = c.el.startsWith('select');
          const [bgToken, fgToken] = select ? ['--vscode-dropdown-background', '--vscode-dropdown-foreground'] : ['--vscode-input-background', '--vscode-input-foreground'];
          // The high contrast themes of verify/themes/ define no dropdown colours.
          if (colours[bgToken] && c.background !== colours[bgToken]) problems.push(`${at}: ${c.el} background ${c.background}, not ${bgToken} ${colours[bgToken]}`);
          if (colours[fgToken] && c.colour !== colours[fgToken]) problems.push(`${at}: ${c.el} text ${c.colour}, not ${fgToken} ${colours[fgToken]}`);
          if (c.ratio < 4.5) problems.push(`${at}: ${c.el} text ${c.ratio}:1, under 4.5:1`);
        }
        if (!['old_wtpool', 'config_error'].includes(state.name)) {
          const names = facts.controls.map((c) => c.el.split('.')[0]);
          if (!names.includes('input') || !names.includes('select')) problems.push(`${at}: the strip shows no Limit field and Policy menu: ${JSON.stringify(facts.controls)}`);
        }
      }
      await context.close();

      // Then
      assert.deepEqual(problems, [], `the machine strip clips, spills, misses AA or its box at ${width} px in ${theme}`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });

    test(`heavy_editor__should_clip_nothing_in_every_theme ${theme} ${width}`, async ({ browser }) => {
      // Given
      const { context, page, panel, colours } = await jobsPanel(browser, theme, width);
      const problems = [];
      const description = colours['--vscode-descriptionForeground'];
      const states = [
        { name: 'busy', doc: mustValidate(busyDocument()), lists: { machine: 1, repository: 4, suggested: 2 } },
        { name: 'first_open', doc: mustValidate(firstOpenDocument()), lists: { machine: 0, repository: 0, suggested: 7 } },
      ];

      // When
      for (const state of states) {
        const at = `${state.name} at ${width} px`;
        await showOpen(page, state.doc, {}, at);
        const chips = await page.evaluate(chipFacts);
        for (const [list, count] of Object.entries(state.lists)) {
          const shown = chips.filter((c) => c.list === list).length;
          assert.equal(shown, count, `${at}: ${shown} chips li.heavy-chip in the ${list} list, not ${count} (U15)`);
        }
        for (const chip of chips) {
          const where = `${at}, ${chip.list} chip "${chip.text}"`;
          if (!chip.border) {
            problems.push(`${where}: no element with a border on all four sides holds its text`);
          } else {
            const style = chip.list === 'suggested' ? 'dashed' : 'solid';
            if (!chip.border.widths.every((w) => Math.abs(w - 1) < 0.01)) problems.push(`${where}: border widths ${chip.border.widths}, not 1 px`);
            if (chip.border.styles.length !== 1 || chip.border.styles[0] !== style) problems.push(`${where}: border style ${chip.border.styles}, not ${style}`);
            if (chip.border.colours.length !== 1 || chip.border.colours[0] !== description) problems.push(`${where}: border ${chip.border.colours}, not ${description}`);
            if (chip.border.ratio < 3) problems.push(`${where}: border ${chip.border.ratio}:1, under 3:1`);
          }
          if (!/monospace|Menlo|Monaco|Courier|Consolas/i.test(chip.font)) problems.push(`${where}: text in ${chip.font}, not monospace`);
          if (!chip.glyphs.length || chip.glyphs.some((g) => !g.found)) problems.push(`${where}: no remove or add glyph ${JSON.stringify(chip.glyphs)}`);
          for (const g of chip.glyphs.filter((x) => x.found && x.ratio < 3)) problems.push(`${where}: ${g.el} glyph ${g.ratio}:1, under 3:1`);
        }
        problems.push(...(await looks(page, at)));
        for (const v of (await axeRun(page)).violations) problems.push(`${at}: axe ${v}`);
      }
      // An inline error under This repository's input.
      await showOpen(page, states[0].doc, {}, `error at ${width} px`);
      const inputs = page.locator('tr.machine-detail input.heavy-input');
      assert.ok((await inputs.count()) >= 2, `the strip shows ${await inputs.count()} input.heavy-input, not one per list (U15)`);
      await inputs.nth(1).fill('make *');
      await inputs.nth(1).press('Enter');
      await settle(page);
      const error = await page.locator('tr.machine-detail .heavy-error').evaluateAll((els) => els.map((el) => el.textContent.replace(/\s+/g, ' ').trim()).filter(Boolean));
      problems.push(...(await looks(page, `error at ${width} px`)));
      for (const v of (await axeRun(page)).violations) problems.push(`error at ${width} px: axe ${v}`);
      await context.close();

      // Then
      assert.ok(error.some((t) => t.includes(TEXT.patternError)), `make * shows no .heavy-error "${TEXT.patternError}": ${JSON.stringify(error)}`);
      assert.deepEqual(problems, [], `the pattern editor clips, spills or misses its chip look at ${width} px in ${theme}`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });

    test(`machine_strip__should_show_every_ring_whole_in_contrast ${theme} ${width}`, async ({ browser }) => {
      // Given
      const { context, page, panel } = await jobsPanel(browser, theme, width);
      await showView(page, mustValidate(busyDocument()));
      await away(page);
      assert.ok((await page.locator('tr.pool-heading button.machine-button').count()) > 0, 'the heading holds no button.machine-button (U10)');
      const problems = [];
      const reached = [];

      // When
      assert.ok(await tabToButton(page), 'Tab never reaches the machine button (X20)');
      await page.keyboard.press('Enter');
      await settle(page, 0);
      for (let step = 0; step < 4; step += 1) {
        const control = await page.evaluate(focusedControl);
        if (control) {
          reached.push(control.name);
          problems.push(...ringProblems(`${control.name} at ${width} px`, await page.evaluate(ring)));
        }
        await page.keyboard.press('Tab');
        await page.clock.runFor(20);
      }
      await context.close();

      // Then
      const wanted = ['input.job-limit', 'select.job-enforce', 'button.job-save', 'button.job-default'];
      assert.deepEqual(wanted.filter((name) => !reached.includes(name)), [], `strip controls the keyboard did not reach from the button: ${reached}`);
      assert.deepEqual(problems, [], `focus rings in the strip clipped, missing or under 3:1 at ${width} px in ${theme}`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });

    test(`heavy_editor__should_show_every_ring_whole_in_contrast ${theme} ${width}`, async ({ browser }) => {
      // Given
      const { context, page, panel } = await jobsPanel(browser, theme, width);
      await showView(page, mustValidate(busyDocument()));
      await away(page);
      assert.ok((await page.locator('tr.pool-heading button.machine-button').count()) > 0, 'the heading holds no button.machine-button (U10)');
      const problems = [];
      const reached = new Set();

      // When
      assert.ok(await tabToButton(page), 'Tab never reaches the machine button (X20)');
      await page.keyboard.press('Enter');
      await settle(page, 0);
      for (let step = 0; step < 40; step += 1) {
        const control = await page.evaluate(focusedControl);
        if (!control || !control.inStrip) break;
        if (!reached.has(control.name) || control.name === 'button.chip-remove') {
          problems.push(...ringProblems(`${control.name} at ${width} px, stop ${step}`, await page.evaluate(ring)));
        }
        if (control.name === 'button.chip-remove' && !reached.has('button.chip-remove')) {
          // The arrows move between a list's chips, and the second chip's ring counts too.
          await page.keyboard.press('ArrowRight');
          await page.clock.runFor(20);
          problems.push(...ringProblems(`second chip at ${width} px`, await page.evaluate(ring)));
        }
        reached.add(control.name);
        await page.keyboard.press('Tab');
        await page.clock.runFor(20);
      }
      await context.close();

      // Then
      const wanted = ['button.chip-remove', 'input.heavy-input', 'button.heavy-add', 'button.suggest-add', 'button.suggest-all'];
      assert.deepEqual(wanted.filter((name) => !reached.has(name)), [], `pattern editor controls Tab did not reach: ${[...reached]}`);
      assert.deepEqual(problems, [], `focus rings in the pattern editor clipped, missing or under 3:1 at ${width} px in ${theme}`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });
  }

  test(`machine_button__should_meet_aa_and_show_its_ring ${theme}`, async ({ browser }) => {
    // Given
    const { context, page, panel, colours } = await jobsPanel(browser, theme, 1100);
    const busy = mustValidate(busyDocument());
    const error = structuredClone(busy);
    error.machine.jobs = { ...error.machine.jobs, limit: null, error: 'machine.toml: limit = 0 is outside 1 to 64', running: [], waiting: [] };
    const states = [
      { name: 'busy', doc: busy, attention: ['2 queued', '1 not queued'] },
      { name: 'not_queued', doc: notQueuedDocument(), attention: ['2 not queued'] },
      { name: 'first_open', doc: mustValidate(firstOpenDocument()), muted: ['7 suggested'] },
      { name: 'config_error', doc: mustValidate(error), problem: ['jobs'] },
    ];
    const attention = colours['--vscode-list-warningForeground'];
    const muted = colours['--vscode-descriptionForeground'];
    const problem = colours['--vscode-list-errorForeground'];
    const problems = [];
    const button = page.locator('tr.pool-heading button.machine-button');

    // When
    for (const state of states) {
      await showView(page, state.doc);
      await away(page);
      await settle(page);
      assert.ok((await button.count()) > 0, `${state.name}: the heading holds no button.machine-button (U10)`);
      const plain = await page.evaluate(buttonFacts);
      await button.hover();
      await settle(page);
      const hovered = await page.evaluate(buttonFacts);
      await away(page);
      await settle(page);
      for (const [mode, facts] of [['plain', plain], ['hovered', hovered]]) {
        for (const part of facts.parts.filter((p) => p.ratio < 4.5)) problems.push(`${state.name} ${mode}: "${part.text}" ${part.ratio}:1, under 4.5:1`);
      }
      const colourOf = (text) => plain.parts.find((p) => p.text.includes(text))?.colour;
      for (const text of state.attention ?? []) if (colourOf(text) !== attention) problems.push(`${state.name}: "${text}" in ${colourOf(text)}, not the attention colour ${attention}`);
      for (const text of state.muted ?? []) if (colourOf(text) !== muted) problems.push(`${state.name}: "${text}" in ${colourOf(text)}, not ${muted}`);
      for (const text of state.problem ?? []) {
        if (colourOf(text) !== problem) problems.push(`${state.name}: "${text}" in ${colourOf(text)}, not the problem colour ${problem}`);
        if (!plain.glyphs.includes(problem)) problems.push(`${state.name}: no warning glyph in the problem colour ${problem}: ${plain.glyphs}`);
      }
      if (HIGH_CONTRAST.has(theme)) {
        const { style, width, colour } = hovered.outline;
        if (style !== 'dashed' || Math.abs(width - 1) > 0.01 || colour !== colours['--vscode-contrastActiveBorder']) problems.push(`${state.name} hovered: outline ${width}px ${style} ${colour}, not 1px dashed --vscode-contrastActiveBorder`);
      } else if (hovered.background !== colours['--vscode-toolbar-hoverBackground']) {
        problems.push(`${state.name} hovered: background ${hovered.background}, not --vscode-toolbar-hoverBackground ${colours['--vscode-toolbar-hoverBackground']}`);
      }
    }
    await showView(page, busy);
    await away(page);
    const focused = await tabToButton(page);
    const r = focused ? await page.evaluate(ring) : null;
    await context.close();

    // Then
    assert.ok(focused, 'Tab never reaches the machine button (U10)');
    problems.push(...ringProblems('machine button', r));
    if (r && !r.missing && !r.outline.includes(colours['--vscode-focusBorder'])) problems.push(`machine button: ring ${r.outline}, not --vscode-focusBorder ${colours['--vscode-focusBorder']}`);
    assert.deepEqual(problems, [], `the machine button's text, colours, hover or ring in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });
}
