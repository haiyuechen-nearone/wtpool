import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { actionBoxes, ring, ringProblems } from '../support/measure.mjs';
import { THEME_NAMES } from '../support/page.mjs';
import { inkBounds, pixelHelper } from '../support/pixels.mjs';
import { hoverActions, rowIds, short, showPlain, visualPanel } from '../support/states.mjs';
import { stateDocuments } from '../support/visual-docs.mjs';

/** Tolerance of the glyph centre, in CSS px. */
const CENTRE_PX = 0.5;
/** Ink is a pixel this far (0 to 255, per channel) from the button's background. */
const INK_THRESHOLD = 60;

test.describe.configure({ mode: 'parallel' });

for (const theme of THEME_NAMES) {
  for (const width of [1100, 600]) {
    test(`focus__should_show_every_ring_whole_in_contrast ${theme} ${width}`, async ({ page }) => {
      // Given
      const panel = await visualPanel(page, { theme, width, extraBodyClass: 'vscode-reduce-motion' });
      const problems = new Map();
      let stops = 0;
      let selectedStops = 0;
      const record = async (at) => {
        const r = await page.evaluate(ring);
        if (!r) return null;
        stops += 1;
        if (r.state.startsWith('selected')) selectedStops += 1;
        for (const p of ringProblems(`${r.key} (${r.state})`, r)) if (!problems.has(p)) problems.set(p, at);
        return r;
      };
      const tab = async (key = 'Tab') => {
        await page.keyboard.press(key);
        await page.clock.runFor(20);
      };

      // When
      for (const { name, doc } of stateDocuments()) {
        await showPlain(page, panel, doc, { ms: 0 });
        const rows = new Set();
        for (let step = 0; step < 400; step += 1) {
          await tab();
          const r = await record(`${name} keyboard`);
          if (!r) {
            if (rows.size) break;
            continue;
          }
          if (!r.key.startsWith('row ')) continue;
          if (rows.has(r.key)) break;
          rows.add(r.key);
          // The row's actions follow it in the Tab order: once plain, once with the row selected
          // by Enter. Shift+Tab returns to the row each time.
          const actions = await page.evaluate(() => document.activeElement?.closest('tr[data-row-id]')?.querySelectorAll('button[data-action]').length ?? 0);
          for (const selected of [false, true]) {
            if (selected) {
              await tab('Enter');
              await record(`${name} ${r.key} selected by Enter`);
            }
            for (let k = 0; k < actions; k += 1) {
              await tab();
              await record(`${name} ${r.key}${selected ? ' selected' : ''}, action ${k + 1}`);
            }
            for (let k = 0; k < actions; k += 1) await tab('Shift+Tab');
          }
          await tab('Enter');
        }
      }

      // Then
      assert.ok(stops >= 3 * 30, `setup: only ${stops} focus stops measured`);
      assert.ok(selectedStops > 0, 'setup: no focus stop on a selected row');
      const found = [...problems].map(([p, at]) => `${p}, first at ${at}`);
      assert.deepEqual(found, [], `focus rings clipped, missing or under 3:1 at ${width} px in ${theme}`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });
  }
}

for (const theme of THEME_NAMES) {
  test(`actions__should_centre_their_glyphs ${theme}`, async ({ browser }) => {
    // Given
    const context = await browser.newContext({ viewport: { width: 1100, height: 900 }, deviceScaleFactor: 2 });
    const page = await context.newPage();
    const helper = await pixelHelper(browser);
    const panel = await visualPanel(page, { theme, extraBodyClass: 'vscode-reduce-motion' });
    const found = [];

    // When
    for (const { name, doc } of stateDocuments().slice(0, 2)) {
      await showPlain(page, panel, doc);
      for (const id of await rowIds(page)) {
        await hoverActions(page, id);
        const boxes = await page.evaluate(actionBoxes, id);
        const png = await page.screenshot({ animations: 'disabled' });
        const regions = boxes.map((b) => ({
          key: b.action,
          rect: { x: b.x + 2, y: b.y + 2, w: b.w - 4, h: b.h - 4 },
          bg: { x: b.x + 3, y: b.y + 3 },
          threshold: INK_THRESHOLD,
        }));
        const ink = await inkBounds(helper, png, 2, regions);
        for (const b of boxes) {
          const glyph = ink[b.action];
          const dx = glyph.count ? glyph.cx - (b.x + b.w / 2) : null;
          const dy = glyph.count ? glyph.cy - (b.y + b.h / 2) : null;
          found.push({ at: `${name} ${short(id)} ${b.action}`, dx, dy });
        }
      }
    }
    await helper.close();
    await context.close();

    // Then
    assert.ok(found.length >= 20, `setup: only ${found.length} action buttons measured`);
    const blank = found.filter((f) => f.dx === null).map((f) => f.at);
    assert.deepEqual(blank, [], `actions without a visible glyph in ${theme}`);
    const off = found
      .filter((f) => f.dx !== null && (Math.abs(f.dx) > CENTRE_PX + 0.01 || Math.abs(f.dy) > CENTRE_PX + 0.01))
      .map((f) => `${f.at}: glyph off centre by ${f.dx} px across, ${f.dy} px down`);
    assert.deepEqual(off, [], `action glyphs more than ${CENTRE_PX} px off their button's centre in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });
}
