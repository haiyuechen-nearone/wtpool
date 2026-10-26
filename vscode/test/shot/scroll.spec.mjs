// U1 and U3 in Chromium (status-panel.md revisions 14 and 18): the panel's sideways scroll at
// 600 px, in all four themes, on every_activity, the real-names pool and long values.
//
// scroll__should_scroll_the_table_inside_the_tab: the table is wider than the tab and a box
// around it scrolls sideways while the page itself does not; scrolled to its end, every row's
// Last and actions cells are in view and nothing is clipped (layout__'s measure); the header's
// chips, connection text and heading keep their place in view; a selected row's detail strip,
// its text and its box, stays inside the visible width at both ends. In a tab much taller than
// the table (600 x 900 px), the box that scrolls the table sideways reaches the tab's bottom
// edge, so its horizontal scrollbar is drawn there and not under the last row: for two short
// tables (idle_pool with a long branch, the real-names pool) and for a long one whose rows
// overflow the tab, which scroll down inside that box while the page and the header stay put.
//
// scroll__should_bring_focus_and_waits_into_view: Tab from the top through every stop: a focused
// action (or other control) shows whole with its focus ring, a focused row shows its start.
// Scrolled to the end, a click on the waiting chip focuses the first waiting row with its start
// in view.
import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { everyActivity, mustValidate, scenarioDocuments, worktree } from '../support/docs.mjs';
import { clipping } from '../support/measure.mjs';
import { THEME_NAMES } from '../support/page.mjs';
import { focusInView, scrollDown, scrollLayout, scrollState, scrollTable, tallLayout } from '../support/scroll.mjs';
import { clickRow, short, showPlain, visualPanel } from '../support/states.mjs';
import { realistic, stateDocuments } from '../support/visual-docs.mjs';

const WIDTH = 600;
/** A tab much taller than a short table (U1): its sideways scrollbar belongs at the bottom. */
const TALL = 900;
/** The first waiting row of each state document (U3). */
const FIRST_WAITING = { every_activity: 'wt1', realistic: 'app-wt2', long_values: 'app-wt1' };

test.describe.configure({ mode: 'parallel' });

const outside = (r, view) => r.left < view.left - 0.5 || r.right > view.right + 0.5;

/** Two short tables that still scroll sideways at 600 px, and a long one whose rows overflow 900 px. */
function tallDocuments() {
  // idle_pool fits 600 px; with shot__'s long branch it does not.
  const idle = scenarioDocuments('idle_pool').documents.at(-1);
  idle.pools[0].worktrees[2].branch = '1234-check-widget-cache-health-before-each-deploy-and-more';
  const long = everyActivity();
  long.pools[0].worktrees.push(...Array.from({ length: 20 }, (_, i) => worktree(`wt${i + 20}`)));
  return [
    { name: 'idle_pool with a long branch', doc: mustValidate(idle) },
    { name: 'realistic', doc: mustValidate(realistic()) },
    { name: 'every_activity and 20 free slots', doc: mustValidate(long), long: true },
  ];
}

for (const theme of THEME_NAMES) {
  test(`scroll__should_scroll_the_table_inside_the_tab ${theme}`, async ({ page }) => {
    // Given
    const panel = await visualPanel(page, { theme, width: WIDTH });
    const problems = [];

    // When
    for (const { name, doc } of stateDocuments()) {
      await showPlain(page, panel, doc, { ms: 0 });
      const state = await page.evaluate(scrollState);
      if (state.table <= state.tab) problems.push(`${name}: the ${state.table} px table fits the ${state.tab} px tab, its columns are squeezed`);
      if (state.page > 1) problems.push(`${name}: the page itself scrolls sideways by ${state.page} px`);
      if (!state.scrolls) problems.push(`${name}: no box around the table scrolls it sideways: ${JSON.stringify(state.scrollers)}`);
      const start = await page.evaluate(scrollLayout);
      await page.evaluate(scrollTable, true);
      const end = await page.evaluate(scrollLayout);
      for (const c of end.cells.filter((c) => outside(c, end.view))) problems.push(`${name}: scrolled to the end, ${c.cell} is out of view ${JSON.stringify({ cell: c, view: end.view })}`);
      const clipped = new Map();
      for (const issue of (await page.evaluate(clipping, false)).issues) {
        const key = `${issue.kind} in the ${issue.place ?? issue.side}`;
        clipped.set(key, (clipped.get(key) ?? 0) + 1);
      }
      for (const [key, count] of clipped) problems.push(`${name}: scrolled to the end, ${key} x${count}`);
      start.header.forEach((h, i) => {
        const after = end.header[i];
        const tab = { left: 0, right: state.tab };
        if (outside(h, tab) || !after || outside(after, tab) || Math.abs(after.left - h.left) > 0.5) {
          problems.push(`${name}: header part "${h.part}" moves or leaves the view when the table scrolls: ${JSON.stringify({ start: h, end: after })}`);
        }
      });
      // The detail strip of the first row with an actor line, at the start and at the end.
      await page.evaluate(scrollTable, false);
      const id = await page.evaluate(() => [...document.querySelectorAll('tr[data-row-id]')].find((r) => /·/.test(r.textContent))?.dataset.rowId);
      await clickRow(page, id, 0);
      for (const toEnd of [false, true]) {
        await page.evaluate(scrollTable, toEnd);
        const at = await page.evaluate(scrollLayout);
        const where = toEnd ? 'at the end' : 'at the start';
        if (!at.strip) problems.push(`${name}: no detail strip for ${short(id)}`);
        else if (outside(at.strip.text, at.view) || outside(at.strip.box, at.view)) {
          problems.push(`${name}: ${short(id)}'s detail strip leaves the visible width ${where}: ${JSON.stringify({ ...at.strip, view: at.view })}`);
        }
      }
      await page.evaluate(scrollTable, false);
      await clickRow(page, id, 0);
    }

    // Then
    assert.deepEqual([...new Set(problems)], [], `the table does not scroll inside the ${WIDTH} px tab in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });

  test(`scroll__should_scroll_the_table_inside_the_tab ${theme} tall`, async ({ page }) => {
    // Given
    const panel = await visualPanel(page, { theme, width: WIDTH, height: TALL });
    const problems = [];
    let overflowing = 0;

    // When
    for (const { name, doc, long } of tallDocuments()) {
      await showPlain(page, panel, doc, { ms: 0 });
      const top = await page.evaluate(tallLayout);
      await page.evaluate(scrollDown);
      const down = await page.evaluate(tallLayout);
      if (long && top.page + (top.scroller?.hidden ?? 0) > 0) overflowing += 1;
      if (!top.scroller) {
        problems.push(`${name}: no box around the ${top.table} px high table scrolls it sideways in the ${WIDTH} px tab`);
        continue;
      }
      for (const [where, at] of [['', top], [' with its rows scrolled down', down]]) {
        if (Math.abs(at.scroller.bottom - at.height) > 1) {
          problems.push(`${name}${where}: the table's sideways scrollbar is drawn at ${at.scroller.bottom} px, not at the bottom edge of the ${at.height} px tab: ${JSON.stringify({ ...at.scroller, table: at.table })}`);
        }
      }
      if (top.page > 1) problems.push(`${name}: the page itself scrolls down by ${top.page} px, not the box around the table`);
      top.header.forEach((h, i) => {
        const after = down.header[i];
        if (h.top < -0.5 || h.bottom > top.height + 0.5 || !after || Math.abs(after.top - h.top) > 0.5) {
          problems.push(`${name}: header part "${h.part}" moves or leaves the view when the rows scroll down: ${JSON.stringify({ top: h, down: after })}`);
        }
      });
    }

    // Then
    assert.equal(overflowing, 1, `setup: the long table's rows fit the ${TALL} px tab`);
    assert.deepEqual([...new Set(problems)], [], `the table's sideways scrollbar is not at the bottom of the ${WIDTH} x ${TALL} px tab in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });

  test(`scroll__should_bring_focus_and_waits_into_view ${theme}`, async ({ page }) => {
    // Given
    const panel = await visualPanel(page, { theme, width: WIDTH, extraBodyClass: 'vscode-reduce-motion' });
    const problems = new Map();
    let stops = 0;

    // When
    for (const { name, doc } of stateDocuments()) {
      await showPlain(page, panel, doc, { ms: 0 });
      const state = await page.evaluate(scrollState);
      if (!state.scrolls) problems.set(`${name}: nothing scrolls the ${state.table} px table in the ${state.tab} px tab`, true);
      const seen = new Set();
      for (let step = 0; step < 400; step += 1) {
        await page.keyboard.press('Tab');
        await page.clock.runFor(20);
        const f = await page.evaluate(focusInView);
        if (!f) {
          if (seen.size) break;
          continue;
        }
        if (seen.has(f.key)) break;
        seen.add(f.key);
        stops += 1;
        if (!f.shown) {
          const what = f.isRow ? 'its start is not in view' : 'it or its focus ring is not wholly in view';
          problems.set(`${name}: ${f.key} focused by Tab, ${what}: ${JSON.stringify({ left: f.left, right: f.right, ring: f.ring, view: f.view })}`, true);
        }
      }
      // The waiting chip, with the table scrolled to its end.
      await page.evaluate(() => document.activeElement?.blur?.());
      await page.evaluate(scrollTable, true);
      await page.locator('.chip').filter({ hasText: /waiting/ }).first().click();
      await page.clock.runFor(50);
      const f = await page.evaluate(focusInView);
      const want = `row ${FIRST_WAITING[name]}`;
      if (!f || f.key !== want) problems.set(`${name}: the waiting chip focuses ${f?.key ?? 'nothing'}, not ${want}`, true);
      else if (!f.shown) problems.set(`${name}: the waiting chip focuses ${want} without its start in view: ${JSON.stringify(f)}`, true);
    }

    // Then
    assert.ok(stops >= 3 * 20, `setup: only ${stops} focus stops`);
    assert.deepEqual([...problems.keys()], [], `focus and the waiting chip at ${WIDTH} px in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });
}
