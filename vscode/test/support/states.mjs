// Puts the panel into the states the visual checks measure: plain, a row hovered, a row selected
// (with its detail strip), focus moved by keyboard. Node side, on a page from support/page.mjs.
import { installHelpers } from './measure.mjs';
import { openPanel } from './page.mjs';

/** Real time for a transition that shows actions or changes a colour to end. */
export const SETTLE_MS = 250;

/** Opens the panel at `width` × `height` in `theme` with the measuring helpers installed. */
export async function visualPanel(page, { theme, width = 1100, height = 900, extraBodyClass = '' }) {
  await page.setViewportSize({ width, height });
  const panel = await openPanel(page, { theme, extraBodyClass });
  await page.evaluate(installHelpers);
  return panel;
}

/** Lets the fake clock run, then real transitions for `ms`. */
export async function settle(page, ms = SETTLE_MS) {
  await page.clock.runFor(50);
  if (ms) await page.waitForTimeout(ms);
}

/** No hover and no focus: the mouse in the page's bottom left corner, focus dropped. */
export async function away(page) {
  const { height } = page.viewportSize();
  await page.mouse.move(1, height - 1);
  await page.evaluate(() => document.activeElement?.blur?.());
}

/** Shows `doc` with no row selected, nothing hovered and nothing focused. */
export async function showPlain(page, panel, doc, { options, ms = SETTLE_MS } = {}) {
  await panel.show(doc, options);
  await away(page);
  const selected = page.locator('tr[data-row-id][aria-selected="true"]');
  if (await selected.count()) {
    await selected.first().locator(':scope > td').first().click({ position: { x: 4, y: 4 } });
    await away(page);
  }
  await settle(page, ms);
}

export async function rowIds(page) {
  return page.locator('tr[data-row-id]').evaluateAll((rows) => rows.map((r) => r.dataset.rowId));
}

/** The row of `id`. Every character of a hostile id outside `\w/.-` is CSS escaped. */
export function rowLocator(page, id) {
  const escaped = id.replace(/[^\w/.-]/gu, (c) => `\\${c.codePointAt(0).toString(16)} `);
  return page.locator(`tr[data-row-id="${escaped}"]`);
}

/** Hovers the row `id` on its first cell, away from its actions. */
export async function hoverRow(page, id, ms = SETTLE_MS) {
  const cell = rowLocator(page, id).locator(':scope > td').first();
  await cell.hover({ position: { x: 4, y: 4 } });
  await settle(page, ms);
}

/**
 * Scrolls the actions of row `id` into view (the table may be wider than the tab) and hovers the
 * row just left of them, off every button.
 */
export async function hoverActions(page, id, ms = SETTLE_MS) {
  const point = await page.evaluate((rowId) => {
    const row = [...document.querySelectorAll('tr[data-row-id]')].find((r) => r.dataset.rowId === rowId);
    const cell = row.querySelector('td.actions');
    cell.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    const c = cell.getBoundingClientRect();
    const r = row.getBoundingClientRect();
    return { x: Math.max(2, c.left - 8), y: (r.top + r.bottom) / 2 };
  }, id);
  await page.mouse.move(point.x, point.y);
  await settle(page, ms);
}

/** The part of `rect` (CSS px) inside the viewport. */
export function inViewport(page, rect) {
  const { width, height } = page.viewportSize();
  const x = Math.max(0, rect.x);
  const y = Math.max(0, rect.y);
  return { x, y, w: Math.max(0, Math.min(width, rect.x + rect.w) - x), h: Math.max(0, Math.min(height, rect.y + rect.h) - y) };
}

/**
 * Selects (or, when selected, deselects) the row `id` by a click on its first cell. The mouse stays
 * there.
 */
export async function clickRow(page, id, ms = SETTLE_MS) {
  const cell = rowLocator(page, id).locator(':scope > td').first();
  await cell.click({ position: { x: 4, y: 4 } });
  await settle(page, ms);
}

export function short(id) {
  return id.split('/').pop();
}
