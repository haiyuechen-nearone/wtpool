import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { test } from '@playwright/test';

import { actor, ago, document, mustValidate, worktree } from '../support/docs.mjs';
import { runDir } from '../support/paths.mjs';
import { THEME_NAMES, computedThemeColours } from '../support/page.mjs';
import { away, rowLocator, settle, showPlain, visualPanel } from '../support/states.mjs';

/** The delay before the label shows, in ms. */
const DELAY_MS = 300;
/** The tolerance of a position or a size, in CSS px. */
const PX = 0.5;

test.describe.configure({ mode: 'parallel' });

/** wt1 lists a badged OpenCode line, then a Claude Code line; wt2 and wt3 lie under them. */
function topDocument() {
  const lines = [
    actor({ key: 'o1', label: 'OpenCode agent', harness: 'opencode', model: 'opencode/deepseek-v4.1-flash-free', updated: ago(1) }),
    actor({ key: 'c1', label: 'Claude Code agent', harness: 'claude-code', model: 'claude-opus-5-5', updated: ago(2) }),
    actor({ key: 'c2', label: 'No model', harness: 'claude-code', updated: ago(3) }),
  ];
  const wt1 = worktree('wt1', { branch: 'labels', free: false, reasons: ['branch'], activity: 'working', actors: lines, last_event: ago(1) });
  const under = (name, key) =>
    worktree(name, {
      branch: name,
      free: false,
      reasons: ['branch'],
      activity: 'working',
      actors: [actor({ key, label: `${name} agent`, harness: 'opencode', model: 'opencode/glm-5.3-flash-free', updated: ago(10) })],
      last_event: ago(10),
    });
  return mustValidate(document([worktree('fg'), wt1, under('wt2', 'u2'), under('wt3', 'u3')]));
}

/** Many rows of one OpenCode line each, so the last row's mark lies at the bottom of a short panel. */
function bottomDocument() {
  const rows = [1, 2, 3, 4, 5, 6, 7, 8].map((n) =>
    worktree(`wt${n}`, {
      branch: `row-${n}`,
      free: false,
      reasons: ['branch'],
      activity: 'working',
      actors: [actor({ key: `b${n}`, label: `row ${n} agent`, harness: 'opencode', model: 'opencode/deepseek-v4.1-flash-free', updated: ago(n) })],
      last_event: ago(n),
    }),
  );
  return mustValidate(document([worktree('fg'), ...rows]));
}

/** The rectangle of each row, by id. */
function rowRects() {
  return Object.fromEntries([...document.querySelectorAll('tr[data-row-id]')].map((row) => {
    const r = row.getBoundingClientRect();
    return [row.dataset.rowId, { x: r.left, y: r.top, w: r.width, h: r.height }];
  }));
}

/**
 * The floating label, if one shows: its text, box, colours, contrast against its own background,
 * whether it lies out of the flow and above the rows (the topmost element at its centre), and what
 * cuts it off.
 */
function labelFacts() {
  const V = window.__visual;
  const labels = [...document.querySelectorAll('.float-label:not([hidden])')];
  if (!labels.length) return { count: 0 };
  const label = labels[0];
  const r = label.getBoundingClientRect();
  const s = V.st(label);
  const bg = V.bgOf(label);
  // A label that lets the pointer through is still on top when it takes the pointer for the test.
  const before = label.style.pointerEvents;
  label.style.pointerEvents = 'auto';
  const hit = document.elementFromPoint((r.left + r.right) / 2, (r.top + r.bottom) / 2);
  label.style.pointerEvents = before;
  return {
    count: labels.length,
    text: label.textContent.replace(/\s+/g, ' ').trim(),
    rect: { left: r.left, right: r.right, top: r.top, bottom: r.bottom },
    view: { width: window.innerWidth, height: window.innerHeight },
    colour: s.color,
    background: s.backgroundColor,
    borders: ['Top', 'Right', 'Bottom', 'Left'].map((side) => s[`border${side}Color`]),
    ratio: Math.round(V.ratio(V.paint(s.color, label, bg), bg) * 100) / 100,
    outOfFlow: s.position === 'absolute' || s.position === 'fixed',
    topmost: hit === label || label.contains(hit),
    clippedBy: V.visible(r, label.parentElement).cutters.map((c) => c.by),
  };
}

/** The mark box `line` of row `id`, in the page. */
const markOf = (page, id, line) => rowLocator(page, id).locator('.line').nth(line).locator('.line-mark');

/** The box of a locator, in CSS px. */
async function boxOf(locator) {
  const box = await locator.boundingBox();
  assert.ok(box, 'setup: the mark box has no box');
  return { left: box.x, right: box.x + box.width, top: box.y, bottom: box.y + box.height };
}

/**
 * Stops the page's clock at the time it reads. The clock keeps following real time until the pause
 * arrives, so a slow round trip aims at a moment the page passed; read it again and pause there.
 */
async function pauseClock(page) {
  for (let attempt = 0; ; attempt++) {
    const now = await page.evaluate(() => Date.now());
    try {
      await page.clock.pauseAt(now);
      return;
    } catch (error) {
      if (attempt === 2) throw error;
    }
  }
}

/** Points at `mark` with the page's clock stopped, so only `runFor` moves the page's time. */
async function hoverOnStoppedClock(page, mark) {
  await pauseClock(page);
  await mark.hover();
}

async function save(page, name) {
  const dir = path.join(runDir(), 'shots');
  mkdirSync(dir, { recursive: true });
  await page.screenshot({ path: path.join(dir, `${name}.png`) });
}

for (const theme of THEME_NAMES) {
  test(`agent_mark__should_show_its_label_on_hover ${theme}`, async ({ page }) => {
    // Given
    const panel = await visualPanel(page, { theme, extraBodyClass: 'vscode-reduce-motion' });
    const colours = await computedThemeColours(page, panel.theme);
    const top = topDocument();
    const wt1 = top.pools[0].worktrees.find((w) => w.name === 'wt1').path;
    await showPlain(page, panel, top);
    const problems = [];
    const rowsBefore = await page.evaluate(rowRects);

    // When
    const wanted = [
      [0, 'deepseek-v4.1-flash-free · OpenCode', 'a badged OpenCode line'],
      [1, 'claude-opus-5-5 · Claude Code', 'a Claude Code line'],
      [2, 'Claude Code', 'a Claude Code line with no model'],
    ];
    const seen = [];
    for (const [line, text] of wanted) {
      const mark = markOf(page, wt1, line);
      const markBox = await boxOf(mark);
      await hoverOnStoppedClock(page, mark);
      await page.clock.runFor(DELAY_MS - 1);
      const early = await page.evaluate(labelFacts);
      await page.clock.runFor(1);
      await page.waitForTimeout(300);
      const facts = await page.evaluate(labelFacts);
      const rowsDuring = await page.evaluate(rowRects);
      await save(page, `agent_mark_label-${theme}-${line}`);
      await away(page);
      await page.clock.runFor(50);
      const after = await page.evaluate(labelFacts);
      seen.push({ line, text, markBox, early, facts, rowsDuring, after });
    }
    const rowsAfter = await page.evaluate(rowRects);
    const small = await page.evaluate(() => ({ width: window.innerWidth, height: window.innerHeight }));
    await page.setViewportSize({ width: small.width, height: 300 });
    await showPlain(page, panel, bottomDocument());
    const last = (await page.evaluate(() => [...document.querySelectorAll('tr[data-row-id]')].at(-1).dataset.rowId));
    const bottomMark = markOf(page, last, 0);
    await bottomMark.scrollIntoViewIfNeeded();
    const bottomBox = await boxOf(bottomMark);
    await hoverOnStoppedClock(page, bottomMark);
    await page.clock.runFor(DELAY_MS);
    await page.waitForTimeout(300);
    const bottom = await page.evaluate(labelFacts);
    await save(page, `agent_mark_label-${theme}-bottom`);

    // Then
    for (const { line, text, markBox, early, facts, rowsDuring, after } of seen) {
      const at = `line ${line + 1} (${text})`;
      if (early.count) problems.push(`${at}: a label shows ${DELAY_MS - 1} ms after the pointer entered (U18)`);
      if (facts.count !== 1) {
        problems.push(`${at}: ${facts.count} labels show ${DELAY_MS} ms after the pointer entered, not one (U18)`);
        continue;
      }
      if (facts.text !== text) problems.push(`${at}: the label reads "${facts.text}", not "${text}" (U18)`);
      if (facts.rect.left < -PX || facts.rect.top < -PX || facts.rect.right > facts.view.width + PX || facts.rect.bottom > facts.view.height + PX) problems.push(`${at}: the label ${JSON.stringify(facts.rect)} lies outside the panel's visible area ${facts.view.width} x ${facts.view.height} (U18)`);
      if (facts.rect.top < markBox.bottom - 1) problems.push(`${at}: the label (top ${facts.rect.top}) is not below its mark (bottom ${markBox.bottom}) (U18)`);
      if (!facts.outOfFlow) problems.push(`${at}: the label is in the flow of the rows (U18)`);
      if (!facts.topmost) problems.push(`${at}: the label is under another element at its centre (U18)`);
      if (facts.clippedBy.length) problems.push(`${at}: the label is cut off by ${facts.clippedBy} (U18)`);
      if (facts.ratio < 3) problems.push(`${at}: the label's text is ${facts.ratio}:1 on its background, under 3:1 (U18)`);
      if (facts.colour !== colours['--vscode-editorHoverWidget-foreground']) problems.push(`${at}: the label's text is ${facts.colour}, not --vscode-editorHoverWidget-foreground ${colours['--vscode-editorHoverWidget-foreground']} (U18)`);
      if (facts.background !== colours['--vscode-editorHoverWidget-background']) problems.push(`${at}: the label's background is ${facts.background}, not --vscode-editorHoverWidget-background ${colours['--vscode-editorHoverWidget-background']} (U18)`);
      if (!facts.borders.includes(colours['--vscode-editorHoverWidget-border'])) problems.push(`${at}: no border of the label is --vscode-editorHoverWidget-border ${colours['--vscode-editorHoverWidget-border']}: ${facts.borders} (U18)`);
      if (JSON.stringify(rowsDuring) !== JSON.stringify(rowsBefore)) problems.push(`${at}: the rows moved while the label showed (U18)`);
      if (after.count) problems.push(`${at}: the label stays after the pointer left the mark (U18)`);
    }
    if (JSON.stringify(rowsAfter) !== JSON.stringify(rowsBefore)) problems.push('the rows moved after the labels went (U18)');
    if (bottom.count !== 1) {
      problems.push(`the mark at the bottom of a ${small.width} x 300 px panel shows ${bottom.count} labels, not one (U18)`);
    } else {
      if (bottom.rect.left < -PX || bottom.rect.top < -PX || bottom.rect.right > bottom.view.width + PX || bottom.rect.bottom > bottom.view.height + PX) problems.push(`the label of the bottom mark ${JSON.stringify(bottom.rect)} lies outside the panel's visible area ${bottom.view.width} x ${bottom.view.height} (U18)`);
      if (bottom.clippedBy.length) problems.push(`the label of the bottom mark is cut off by ${bottom.clippedBy} (U18)`);
      if (bottom.ratio < 3) problems.push(`the label of the bottom mark is ${bottom.ratio}:1, under 3:1 (U18)`);
      if (bottom.rect.top < bottomBox.bottom - 1 && bottom.rect.bottom > bottomBox.top + 1) problems.push('the label of the bottom mark covers its mark (U18)');
    }
    await settle(page, 0);
    assert.deepEqual(problems, [], `the hover label of an agent mark is off text, place, motion or contrast in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });
}
