import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { mustValidate, scenarioDocuments, withActor } from '../support/docs.mjs';
import { LINE_STATES, countFacts, lineAnimations, lineFacts, lineStatesDocument, notQueuedDocument, removeProbes } from '../support/jobs-shot.mjs';
import { THEME_NAMES, computedThemeColours } from '../support/page.mjs';
import { changedPixels, inkBounds, pixelHelper } from '../support/pixels.mjs';
import { away, clickRow, hoverRow, settle, showPlain, visualPanel } from '../support/states.mjs';

/** The tolerances of an icon's box and of its ink, in CSS px. */
const BOX_PX = 0.5;
const INK_PX = 1;
/** The colour token of each line state: its group's, the problem colour for an API error. */
const STATE_TOKENS = {
  working: '--vscode-textLink-foreground',
  quiet: '--vscode-descriptionForeground',
  waiting: '--vscode-list-warningForeground',
  idle: '--vscode-descriptionForeground',
  error: '--vscode-list-errorForeground',
  done: '--vscode-descriptionForeground',
};
/** The states whose icon takes the selection's foreground on a selected row, as quiet text does. */
const QUIET_STATES = new Set(['quiet', 'idle', 'done']);

test.describe.configure({ mode: 'parallel' });

/** The documents of the count check, each worktree's agent line count by row name. */
function countDocuments() {
  const docs = [
    ...scenarioDocuments('five_agents_one_worktree').documents.map((doc, i) => ({ name: `five_agents_one_worktree#${i}`, doc })),
    { name: 'several_agents_one_worktree', doc: scenarioDocuments('several_agents_one_worktree').documents.at(-1) },
  ];
  return docs.map(({ name, doc }) => ({
    name,
    doc: mustValidate(doc),
    lines: Object.fromEntries(doc.pools[0].worktrees.map((w) => [w.name, w.actors.length])),
  }));
}

for (const theme of THEME_NAMES) {
  for (const width of [1100, 600, 320]) {
    test(`count__should_keep_to_the_word_line ${theme} ${width}`, async ({ page }) => {
      // Given
      const panel = await visualPanel(page, { theme, width, extraBodyClass: 'vscode-reduce-motion' });
      const problems = [];
      let shownCounts = 0;

      // When
      for (const { name, doc, lines } of countDocuments()) {
        await showPlain(page, panel, doc);
        const busy = Object.entries(lines).find(([, n]) => n >= 2)?.[0];
        const busyId = doc.pools[0].worktrees.find((w) => w.name === busy)?.path;
        for (const [mode, enter] of [['plain', async () => {}], ['hovered', () => hoverRow(page, busyId)], ['selected', () => clickRow(page, busyId)]]) {
          await enter();
          for (const row of await page.evaluate(countFacts)) {
            const n = lines[row.row];
            const at = `${name} ${row.row} ${mode} at ${width} px`;
            if (n === undefined) continue;
            if (n < 2) {
              if (row.counts.length) problems.push(`${at}: ${n} agent lines, yet a count "${row.counts.map((c) => c.text)}" shows`);
              continue;
            }
            if (row.counts.length !== 1) {
              problems.push(`${at}: ${n} agent lines show ${row.counts.length} .count, not one reading ×${n}`);
              continue;
            }
            shownCounts += 1;
            const [count] = row.counts;
            if (count.text !== `×${n}`) problems.push(`${at}: the count reads "${count.text}", not "×${n}"`);
            const w = row.wordRect;
            if (!w || !(count.rect.top < w.bottom && count.rect.bottom > w.top)) problems.push(`${at}: the count is not on the word's line: ${JSON.stringify({ count: count.rect, word: w })}`);
            if (w && count.rect.left < w.right - 0.5) problems.push(`${at}: the count starts before the word "${row.word}" ends`);
            if (count.colour !== row.wordColour) problems.push(`${at}: the count's text ${count.colour}, not the word's ${row.wordColour}`);
            if (!count.sides.every((s) => Math.abs(s.width - 1) < 0.01 && s.style === 'solid' && s.colour === row.wordColour)) {
              problems.push(`${at}: the count's border is not 1 px solid ${row.wordColour}: ${JSON.stringify(count.sides)}`);
            }
            if (count.textRatio < 4.5) problems.push(`${at}: the count's text ${count.textRatio}:1, under 4.5:1`);
            if (count.borderRatio < 3) problems.push(`${at}: the count's border ${count.borderRatio}:1, under 3:1`);
            if (count.clippedBy.length) problems.push(`${at}: the count is cut by ${count.clippedBy}`);
          }
        }
        await clickRow(page, busyId);
        await away(page);
      }

      // Then
      assert.ok(shownCounts > 0, `wt2 with five agent lines shows no .count ×5 after its Activity word at ${width} px (U12)`);
      assert.deepEqual(problems, [], `counts off the word's line, colour or contrast at ${width} px in ${theme}`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });
  }

  test(`line_icons__should_align_and_share_one_size ${theme}`, async ({ browser }) => {
    // Given
    const context = await browser.newContext({ viewport: { width: 1100, height: 900 }, deviceScaleFactor: 2 });
    const page = await context.newPage();
    const helper = await pixelHelper(browser);
    const panel = await visualPanel(page, { theme, extraBodyClass: 'vscode-reduce-motion' });
    const colours = await computedThemeColours(page, panel.theme);
    const doc = lineStatesDocument();
    const wt1 = doc.pools[0].worktrees.find((w) => w.name === 'wt1').path;
    await showPlain(page, panel, doc);

    // When
    const plain = await page.evaluate(lineFacts);
    const png = await page.screenshot({ animations: 'disabled' });
    await page.evaluate(removeProbes);
    const regions = plain.flatMap((f, i) => [
      ...(f.icon ? [{ key: `icon${i}`, rect: { x: f.icon.x - 0.5, y: f.icon.y - 0.5, w: f.icon.w + 1, h: f.icon.h + 1 }, ref: f.bg, threshold: 48 }] : []),
      { key: `probe${i}`, rect: { x: f.probe.x, y: f.probe.y, w: f.probe.w, h: f.probe.h }, ref: f.bg, threshold: 48 },
    ]);
    const ink = await inkBounds(helper, png, 2, regions);
    await hoverRow(page, wt1);
    const hovered = await page.evaluate(lineFacts);
    await page.evaluate(removeProbes);
    await clickRow(page, wt1);
    await away(page);
    await settle(page);
    const selected = await page.evaluate(lineFacts);
    await page.evaluate(removeProbes);
    await helper.close();
    await context.close();

    // Then
    const agentLines = plain.filter((f) => f.row === 'wt1');
    assert.ok(agentLines.some((f) => f.icon), `wt1's agent lines hold no .line-icon (U13): ${JSON.stringify(agentLines.map((f) => f.text))}`);
    const problems = [];
    for (const f of plain) {
      const at = `${f.row} line ${f.index + 1} "${f.text}"`;
      if (!f.icon) {
        problems.push(`${at}: no .line-icon box`);
        continue;
      }
      if (Math.abs(f.icon.w - 14) > BOX_PX || Math.abs(f.icon.h - 14) > BOX_PX) problems.push(`${at}: icon box ${f.icon.w} x ${f.icon.h} px, not 14 px`);
      if (f.textLeft === null || Math.abs(f.textLeft - (f.icon.x + f.icon.w) - 5) > BOX_PX) problems.push(`${at}: text starts ${f.textLeft === null ? 'nowhere' : `${Math.round((f.textLeft - f.icon.x - f.icon.w) * 10) / 10} px`} after the icon box, not 5 px`);
    }
    const lefts = plain.filter((f) => f.textLeft !== null).map((f) => f.textLeft);
    if (Math.max(...lefts) - Math.min(...lefts) > BOX_PX) problems.push(`the Agent column's lines start at ${[...new Set(lefts.map((x) => Math.round(x * 10) / 10))].join(', ')} px, not one x`);
    const sized = agentLines
      .map((f) => ({ f, i: plain.indexOf(f) }))
      .filter(({ f }) => f.icon)
      .map(({ f, i }) => ({ f, ink: ink[`icon${i}`], probe: ink[`probe${i}`] }));
    for (const { f } of sized.filter((s) => !s.ink.count)) problems.push(`wt1 line ${f.index + 1}: the icon box holds no ink`);
    const inked = sized.filter((s) => s.ink.count);
    const sides = inked.map((s) => Math.max(s.ink.right - s.ink.left, s.ink.bottom - s.ink.top)).sort((a, b) => a - b);
    const median = sides[Math.floor(sides.length / 2)];
    for (const s of inked) {
      const state = LINE_STATES[s.f.index];
      const side = Math.max(s.ink.right - s.ink.left, s.ink.bottom - s.ink.top);
      if (Math.abs(side - median) > INK_PX + 0.01) problems.push(`${state}: icon ink's longer side ${side} px against the median ${median} px`);
      if (!s.probe.count) problems.push(`${state}: setup: no ink of the capital H probe`);
      else if (Math.abs(s.ink.cy - s.probe.cy) > INK_PX + 0.01) problems.push(`${state}: icon ink centre ${s.ink.cy} px, the capitals' centre ${s.probe.cy} px`);
    }
    for (const [mode, facts] of [['plain', plain], ['hovered', hovered], ['selected', selected]]) {
      for (const f of facts.filter((x) => x.row === 'wt1' && x.icon)) {
        const state = LINE_STATES[f.index];
        const want = mode === 'selected' && QUIET_STATES.has(state) ? '--vscode-list-activeSelectionForeground' : STATE_TOKENS[state];
        if (!f.colours.length || f.colours.some((c) => c !== colours[want])) problems.push(`${state} ${mode}: icon in ${f.colours.join(' and ') || 'no colour'}, not ${want} ${colours[want]}`);
        for (const ratio of f.ratios.filter((r) => r < 3)) problems.push(`${state} ${mode}: icon ${ratio}:1 against its row, under 3:1`);
      }
    }
    assert.deepEqual(problems, [], `agent line icons misaligned, mis-sized or off colour in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });

  test(`not_queued__should_meet_aa_in_every_theme ${theme}`, async ({ page }) => {
    // Given
    const panel = await visualPanel(page, { theme });
    const colours = await computedThemeColours(page, panel.theme);
    const doc = notQueuedDocument();
    await showPlain(page, panel, doc);
    const attention = colours['--vscode-list-warningForeground'];
    const facts = () =>
      page.evaluate(() => {
        const V = window.__visual;
        return [...document.querySelectorAll('tr[data-row-id]')].map((row) => ({
          row: row.dataset.rowId.split('/').pop(),
          line: row.querySelector(':scope > td:nth-child(5)')?.textContent.replace(/\s+/g, ' ').trim() ?? '',
          marks: [...row.querySelectorAll('.not-queued')].filter((el) => V.shown(el)).map((el) => {
            const bg = V.bgOf(el);
            const s = V.st(el);
            return { text: el.textContent.replace(/\s+/g, ' ').trim(), colour: s.color, ratio: Math.round(V.ratio(V.paint(s.color, el, bg), bg) * 100) / 100 };
          }),
        }));
      });
    const seen = [];

    // When
    seen.push(['plain', await facts()]);
    for (const name of ['wt1', 'wt2']) {
      const id = doc.pools[0].worktrees.find((w) => w.name === name).path;
      await hoverRow(page, id);
      seen.push([`${name} hovered`, await facts()]);
      await clickRow(page, id);
      seen.push([`${name} selected and hovered`, await facts()]);
      await away(page);
      await settle(page);
      seen.push([`${name} selected`, await facts()]);
      await clickRow(page, id);
      await away(page);
    }

    // Then
    const marked = seen[0][1].filter((r) => r.marks.length).map((r) => r.row);
    assert.deepEqual(marked, ['wt1', 'wt2'], `rows with a shown .not-queued under warn (A8, U14): ${JSON.stringify(seen[0][1].map((r) => [r.row, r.line]))}`);
    const problems = [];
    for (const [at, rows] of seen) {
      for (const r of rows) {
        for (const m of r.marks) {
          if (m.text !== '(not queued)') problems.push(`${at} ${r.row}: .not-queued reads "${m.text}"`);
          if (m.colour !== attention) problems.push(`${at} ${r.row}: (not queued) in ${m.colour}, not the attention colour ${attention}`);
          if (m.ratio < 4.5) problems.push(`${at} ${r.row}: (not queued) ${m.ratio}:1, under 4.5:1`);
        }
      }
    }
    assert.deepEqual(problems, [], `(not queued) off colour or under AA in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });
}

test('line_icons__should_never_move', async ({ browser }) => {
  // Given
  const doc = lineStatesDocument();
  const pool = doc.pools[0];
  pool.worktrees = pool.worktrees.map((w) => (w.name === 'wt3' ? withActor('wt3', { key: 'm1', label: 'mover', state: 'working' }) : w));
  mustValidate(doc);
  const helper = await pixelHelper(browser);
  const runs = {};

  // When
  for (const reduce of [false, true]) {
    const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });
    const panel = await visualPanel(page, { theme: 'dark', extraBodyClass: reduce ? 'vscode-reduce-motion' : '' });
    await showPlain(page, panel, doc);
    const boxes = await page.evaluate(() =>
      [...document.querySelectorAll('tr[data-row-id] > td:nth-child(5) .line-icon')].map((icon) => {
        const r = icon.getBoundingClientRect();
        return { x: r.left - 1, y: r.top - 1, w: r.width + 2, h: r.height + 2 };
      }),
    );
    const shots = [];
    for (let k = 0; k < 3; k += 1) {
      shots.push(await page.screenshot({ animations: 'allow' }));
      await page.waitForTimeout(250);
    }
    const changed = [];
    for (const box of boxes) {
      for (const later of shots.slice(1)) {
        const diff = await changedPixels(helper, shots[0], later, box, 1);
        if (diff.changed) changed.push({ box, changed: diff.changed });
      }
    }
    const arms = await page.evaluate(() => [...document.querySelectorAll('tr[data-row-id] > td:nth-child(5) .line-icon .robot-arm')].map((arm) => getComputedStyle(arm).rotate));
    runs[reduce ? 'reduced' : 'normal'] = { boxes: boxes.length, changed, arms, animations: await page.evaluate(lineAnimations), errors: panel.errors };
    await page.close();
  }
  await helper.close();

  // Then
  assert.ok(runs.normal.boxes > 0, 'no agent line holds a .line-icon (U13)');
  for (const [mode, run] of Object.entries(runs)) {
    assert.deepEqual(run.animations.filter((a) => a.where === 'agent'), [], `animations run on agent lines (${mode} motion)`);
    assert.deepEqual(run.changed, [], `agent line icons change their pixels over 500 ms (${mode} motion)`);
    for (let i = 0; i < run.arms.length; i += 2) {
      assert.deepEqual(run.arms.slice(i, i + 2), ['30deg', '-30deg'], `a working line's robot is not in its rest pose, arms 30 degrees out (${mode} motion)`);
    }
    assert.deepEqual(run.errors, [], `page errors (${mode} motion)`);
  }
  assert.ok(runs.normal.animations.some((a) => a.where === 'activity' && a.row === 'wt3'), `the Activity cell's robot of working wt3 does not swing without reduce motion: ${JSON.stringify(runs.normal.animations)}`);
});
