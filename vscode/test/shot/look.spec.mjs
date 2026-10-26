import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { everyActivity, mustValidate } from '../support/docs.mjs';
import { actionBoxes, boundaries, iconBoxes, stripBoundary, textContrast } from '../support/measure.mjs';
import { THEME_NAMES } from '../support/page.mjs';
import { changedPixels, inkBounds, pixelHelper } from '../support/pixels.mjs';
import { away, clickRow, hoverActions, hoverRow, inViewport, rowIds, settle, short, showPlain, visualPanel } from '../support/states.mjs';
import { bigCounts, stateDocuments, visualDocuments } from '../support/visual-docs.mjs';

/**
 * The most, in CSS px, an activity icon's longer ink side may differ from the median icon's. Glyph
 * shapes differ, so width and height alone vary: the bell is narrow, the archive flat.
 */
const ICON_PX = 1;
/** Share of a hovered row's pixels, outside its actions, that must change. */
const ROW_CHANGE = 0.01;
/** Share of a hovered action's pixels, with 3 px around it, that must change. */
const ACTION_CHANGE = 0.02;

test.describe.configure({ mode: 'parallel' });

for (const theme of THEME_NAMES) {
  test(`contrast__should_meet_aa_in_every_theme ${theme}`, async ({ page }) => {
    // Given
    const panel = await visualPanel(page, { theme, extraBodyClass: 'vscode-reduce-motion' });
    const found = new Map();
    let states = 0;
    const measure = async (at) => {
      states += 1;
      for (const f of await page.evaluate(textContrast)) if (!found.has(f.key)) found.set(f.key, { ...f, at });
    };

    // When
    for (const item of visualDocuments()) {
      await showPlain(page, panel, item.doc, { options: item.options, ms: 0 });
      await measure(`${item.name} plain`);
    }
    for (const { name, doc } of stateDocuments()) {
      await showPlain(page, panel, doc);
      for (const id of await rowIds(page)) {
        await hoverRow(page, id);
        await measure(`${name} ${short(id)} hovered`);
        await clickRow(page, id);
        await measure(`${name} ${short(id)} selected and hovered`);
        await away(page);
        await settle(page);
        await measure(`${name} ${short(id)} selected`);
        await clickRow(page, id);
        await away(page);
      }
    }

    // Then
    assert.ok(states > 100, `setup: only ${states} states measured`);
    const low = [...found.values()].map((f) => `${f.key}: ${f.ratio}:1 under ${f.need}:1, first "${f.text}" at ${f.at}`);
    assert.deepEqual(low, [], `text under WCAG AA contrast in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });

  test(`hover__should_show_in_every_theme ${theme}`, async ({ page, browser }) => {
    // Given
    const helper = await pixelHelper(browser);
    const panel = await visualPanel(page, { theme, extraBodyClass: 'vscode-reduce-motion' });
    const all = mustValidate(everyActivity());
    await showPlain(page, panel, all);
    const shot = () => page.screenshot({ animations: 'disabled' });
    const rows = [];
    const actions = [];

    // When
    for (const id of await rowIds(page)) {
      await away(page);
      // The same scroll position for both shots: the row's start in view.
      const box = await page.evaluate((rowId) => {
        const row = [...document.querySelectorAll('tr[data-row-id]')].find((r) => r.dataset.rowId === rowId);
        row.querySelector(':scope > td').scrollIntoView({ block: 'nearest', inline: 'nearest' });
        const r = row.getBoundingClientRect();
        const a = row.querySelector('td.actions, :scope > td:last-child').getBoundingClientRect();
        return { x: r.left, y: r.top, w: a.left - r.left, h: r.height };
      }, id);
      await settle(page);
      const plain = await shot();
      await hoverRow(page, id);
      const hovered = await shot();
      const { changed, of } = await changedPixels(helper, plain, hovered, inViewport(page, box), 1);
      rows.push({ row: short(id), share: changed / of });
      if (!['/fg', '/wt2', '/wt8'].some((end) => id.endsWith(end))) continue;
      await hoverActions(page, id);
      for (const b of await page.evaluate(actionBoxes, id)) {
        await hoverActions(page, id);
        const before = await shot();
        await page.mouse.move(b.x + b.w / 2, b.y + b.h / 2);
        await settle(page);
        const after = await shot();
        const around = inViewport(page, { x: b.x - 3, y: b.y - 3, w: b.w + 6, h: b.h + 6 });
        const diff = await changedPixels(helper, before, after, around, 1);
        actions.push({ action: `${short(id)} ${b.action}`, share: diff.changed / diff.of });
      }
    }
    await helper.close();

    // Then
    assert.ok(rows.length === all.pools[0].worktrees.length && actions.length >= 9, `setup: measured ${rows.length} rows and ${actions.length} actions`);
    const pct = (v) => `${Math.round(v * 1000) / 10} %`;
    const flat = [
      ...rows.filter((r) => r.share < ROW_CHANGE).map((r) => `row ${r.row}: ${pct(r.share)} of its pixels change, under ${pct(ROW_CHANGE)}`),
      ...actions.filter((a) => a.share < ACTION_CHANGE).map((a) => `action ${a.action}: ${pct(a.share)} of its pixels change, under ${pct(ACTION_CHANGE)}`),
    ];
    assert.deepEqual(flat, [], `rows and actions that look the same hovered and not in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });

  test(`icons__should_share_one_size ${theme}`, async ({ browser }) => {
    // Given
    const context = await browser.newContext({ viewport: { width: 1100, height: 900 }, deviceScaleFactor: 2 });
    const page = await context.newPage();
    const helper = await pixelHelper(browser);
    const panel = await visualPanel(page, { theme, extraBodyClass: 'vscode-reduce-motion' });
    await showPlain(page, panel, mustValidate(everyActivity()));

    // When
    const icons = await page.evaluate(iconBoxes);
    const png = await page.screenshot({ animations: 'disabled' });
    const ink = await inkBounds(
      helper,
      png,
      2,
      icons.map((i) => ({ key: i.row, rect: { x: i.x - 0.5, y: i.y - 0.5, w: i.w + 1, h: i.h + 1 }, ref: i.bg, threshold: 48 })),
    );
    await helper.close();
    await context.close();

    // Then
    assert.deepEqual(icons.filter((i) => !i.found || !ink[i.row].count).map((i) => i.word), [], `activities without an icon in ${theme}`);
    const sized = icons.map((i) => {
      const w = ink[i.row].right - ink[i.row].left;
      const h = ink[i.row].bottom - ink[i.row].top;
      return { word: i.word, w, h, side: Math.max(w, h) };
    });
    const sides = sized.map((i) => i.side).sort((a, b) => a - b);
    const median = sides[Math.floor(sides.length / 2)];
    const odd = sized
      .filter((i) => Math.abs(i.side - median) > ICON_PX + 0.01)
      .map((i) => `${i.word}: ${i.w} x ${i.h} px, longer side ${i.side} px against the median ${median} px`);
    assert.deepEqual(odd, [], `activity icons whose longer ink side is more than ${ICON_PX} px off the median in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });

  test(`chips__should_keep_their_outline_in_every_theme ${theme}`, async ({ page }) => {
    // Given
    const panel = await visualPanel(page, { theme });
    const found = [];

    // When
    for (const [name, doc] of [['every_activity', everyActivity()], ['big_counts', bigCounts()]]) {
      await showPlain(page, panel, mustValidate(doc), { ms: 0 });
      for (const chip of await page.evaluate(boundaries, '.chip')) found.push({ name, ...chip });
    }

    // Then
    assert.ok(found.length >= 6, `setup: only ${found.length} chips shown`);
    const faint = found
      .filter((c) => c.best < 3)
      .map((c) => `${c.name} "${c.text}": boundary ${c.best}:1 (fill ${c.fill}, borders ${c.border}, outline ${c.outline}, ring ${c.shadow})`);
    assert.deepEqual(faint, [], `chips whose shape has under 3:1 against the header in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });

  test(`detail__should_stand_out_from_the_page ${theme}`, async ({ page }) => {
    // Given
    const panel = await visualPanel(page, { theme });
    const all = mustValidate(everyActivity());
    await showPlain(page, panel, all);
    const found = [];

    // When
    for (const id of await rowIds(page)) {
      await clickRow(page, id);
      found.push({ row: short(id), strip: await page.evaluate(stripBoundary) });
      await clickRow(page, id);
    }

    // Then
    assert.deepEqual(found.filter((f) => !f.strip).map((f) => f.row), [], 'rows that show no detail strip when selected');
    const faint = found
      .filter((f) => f.strip && f.strip.best < 3)
      .map((f) => `${f.row}: best boundary ${f.strip.best}:1 of ${JSON.stringify(f.strip.candidates)}`);
    assert.deepEqual(faint, [], `detail strips with under 3:1 against the page in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });
}
