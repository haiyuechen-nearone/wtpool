// U4 in Chromium (status-panel.md revision 12): shown row actions never cover another column's
// text. Every row of every_activity is hovered, focused, and focused on its last action, at
// 1100 px and at 600 px; no shown action button may intersect the text of the row's other six
// cells (Activity, Worktree, Branch, Git, Agent · doing, Last), as far as that text is not
// clipped away. Each slot that holds a branch shows all five actions (open, terminal, copy path,
// free, fg), so the check cannot pass by hiding them.
import assert from 'node:assert/strict';
import { existsSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { expect, test } from '@playwright/test';

import { everyActivity, mustValidate } from '../support/docs.mjs';
import { REPO, runDir } from '../support/paths.mjs';
import { openPanel } from '../support/page.mjs';

const APPROVED = path.join(REPO, 'verify', 'shots', 'approved');
const HEIGHT = 900;
/** Real time for a transition that shows or moves the actions to end. */
const SETTLE_MS = 250;
/** The row whose focused shot the visual review can look at (phase 3 review 1, N27). */
const FOCUS_SHOT_ROW = '/wt2';

/** The actions a row offers by its kind (U4, X11, X12). */
function expectedActions(w) {
  if (w.missing) return ['copyPath'];
  const base = ['open', 'terminal', 'copyPath'];
  return w.role === 'slot' && w.branch !== null ? [...base, 'free', 'fg'] : base;
}

/**
 * For the row `id`: each shown action button and each place it covers text of another cell, and
 * which actions show. Text is each non-blank text node's line boxes, cut to the cell and to every
 * ancestor inside it that clips its overflow.
 */
function measure(id) {
  const row = [...document.querySelectorAll('tr[data-row-id]')].find((r) => r.dataset.rowId === id);
  if (!row) return { missingRow: true };
  const cut = (a, b) => ({
    left: Math.max(a.left, b.left),
    top: Math.max(a.top, b.top),
    right: Math.min(a.right, b.right),
    bottom: Math.min(a.bottom, b.bottom),
  });
  const area = (r) => (r.right - r.left > 0.5 && r.bottom - r.top > 0.5 ? r : null);
  const clips = (el) => {
    const style = getComputedStyle(el);
    return [style.overflowX, style.overflowY].some((v) => v !== 'visible');
  };
  const cells = [...row.querySelectorAll(':scope > td')].filter((td) => !td.classList.contains('actions'));
  const texts = [];
  cells.forEach((cell, column) => {
    const walker = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (!node.textContent.trim()) continue;
      const parent = node.parentElement;
      if (!parent.checkVisibility({ opacityProperty: true, visibilityProperty: true })) continue;
      let bound = cell.getBoundingClientRect();
      for (let el = parent; el && el !== cell; el = el.parentElement) {
        if (clips(el)) bound = cut(bound, el.getBoundingClientRect());
      }
      const range = document.createRange();
      range.selectNodeContents(node);
      for (const rect of range.getClientRects()) {
        const shown = area(cut(rect, bound));
        if (shown) texts.push({ column, text: node.textContent.trim().slice(0, 40), rect: shown });
      }
    }
  });
  const rowBox = row.getBoundingClientRect();
  const buttons = [...row.querySelectorAll('button[data-action]')]
    .filter((b) => b.checkVisibility({ opacityProperty: true, visibilityProperty: true }))
    .map((b) => ({ action: b.dataset.action, rect: b.getBoundingClientRect() }));
  const covered = [];
  for (const button of buttons) {
    for (const text of texts) {
      if (area(cut(button.rect, text.rect))) covered.push({ action: button.action, column: text.column, text: text.text });
    }
  }
  const outside = buttons
    .filter((b) => b.rect.left < rowBox.left - 0.5 || b.rect.right > rowBox.right + 0.5)
    .map((b) => b.action);
  return { shown: buttons.map((b) => b.action), covered, outside };
}

async function shoot(page, name) {
  const dir = path.join(runDir(), 'shots');
  mkdirSync(dir, { recursive: true });
  const shot = await page.screenshot({ path: path.join(dir, `${name}.png`), fullPage: true });
  // Not part of the approved set until the owner approves it; compared once approved.
  if (existsSync(path.join(APPROVED, `${name}.png`))) {
    expect(shot).toMatchSnapshot([`${name}.png`], { maxDiffPixelRatio: 0.001 });
  }
}

const COLUMNS = ['Activity', 'Worktree', 'Branch', 'Git', 'Agent · doing', 'Last'];

for (const width of [1100, 600]) {
  test(`actions__should_not_cover_another_column ${width}`, async ({ page }) => {
    // Given
    await page.setViewportSize({ width, height: HEIGHT });
    const all = mustValidate(everyActivity());
    const panel = await openPanel(page, { theme: 'dark' });
    await panel.show(all);
    const away = async () => {
      await page.mouse.move(1, HEIGHT - 1);
      await page.evaluate(() => document.activeElement?.blur?.());
    };
    const settle = async () => {
      await page.clock.runFor(50);
      await page.waitForTimeout(SETTLE_MS);
    };

    // When
    const found = [];
    for (const w of all.pools[0].worktrees) {
      const row = page.locator(`tr[data-row-id="${w.path}"]`);
      const wanted = expectedActions(w);
      const modes = {
        hovered: async () => row.hover(),
        focused: async () => row.focus(),
        action_focused: async () => row.locator(`button[data-action="${wanted.at(-1)}"]`).focus(),
      };
      for (const [mode, enter] of Object.entries(modes)) {
        await away();
        await settle();
        await enter();
        await settle();
        found.push({ name: w.name, mode, wanted, ...(await page.evaluate(measure, w.path)) });
        if (mode === 'focused' && w.path.endsWith(FOCUS_SHOT_ROW)) await shoot(page, `dark-every_activity_focused_slot-${width}`);
      }
    }

    // Then
    assert.deepEqual(found.filter((f) => f.missingRow).map((f) => f.name), [], 'setup: rows missing from the table');
    const unshown = found
      .map((f) => ({ at: `${f.name} ${f.mode}`, missing: f.wanted.filter((a) => !f.shown.includes(a)) }))
      .filter((f) => f.missing.length);
    assert.deepEqual(unshown, [], `actions that do not show on hover or focus at ${width} px`);
    const outside = found.filter((f) => f.outside.length).map((f) => `${f.name} ${f.mode}: ${f.outside.join(', ')}`);
    assert.deepEqual(outside, [], `actions outside their row at ${width} px`);
    const covered = found.flatMap((f) =>
      f.covered.map((c) => `${f.name} ${f.mode}: ${c.action} covers ${COLUMNS[c.column] ?? `column ${c.column}`} "${c.text}"`),
    );
    assert.deepEqual([...new Set(covered)], [], `shown actions cover another column's text at ${width} px`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });
}
