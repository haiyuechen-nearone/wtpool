import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { test } from '@playwright/test';

import { actor, ago, document, mustValidate, worktree } from '../support/docs.mjs';
import { runDir } from '../support/paths.mjs';
import { THEME_NAMES } from '../support/page.mjs';
import { pixelHelper } from '../support/pixels.mjs';
import { away, clickRow, hoverRow, settle, showPlain, visualPanel } from '../support/states.mjs';

/** The tolerances of a box and of the ink in it, in CSS px. */
const BOX_PX = 0.5;
const INK_PX = 1;
/** U13's box and the gap after it. */
const BOX = 14;
const GAP = 5;
/**
 * U18's sizes: an unbadged mark is 12 px and centred in its box; on a badged line the maker's mark
 * is 10 px in the upper left, and the badge's 7 px glyph sits on a 1 px ring in the lower right
 * corner, so the two keep a ring's width apart. The device pixels of that width are 2.
 */
const MARK_PX = 12;
const BADGED_MARK_PX = 10;
const CLIPPED_PX = 2.5;
const BADGE_GLYPH_PX = [5.5, 8.5];
const RING_DEVICE_PX = 2;
/** The ink threshold of a channel, as the other pixel checks use. */
const INK = 48;
const SCALE = 2;
/** How far a mark's ink colour may be from the colour of its line's text, per channel. */
const COLOUR_TOLERANCE = 24;

test.describe.configure({ mode: 'parallel' });

const MAKER_MODELS = [
  'claude-opus-5-5',
  'openai/gpt-5.2',
  'google/gemini-3-pro',
  'opencode/deepseek-v4.1-flash-free',
  'opencode/glm-5.3-flash-free',
  'qwen3-coder',
  'moonshotai/kimi-k2',
  'mistral-large',
  'meta-llama/llama-4-scout',
  'xai/grok-4',
  'minimax-m2',
];

/**
 * wt1 lists a line for each maker under OpenCode (a badge each), Claude Code with its own maker (no
 * badge), OpenCode and Aider without a model (the harness's mark, none), wt2 a build line and wt3 the
 * `no agent` line.
 */
function marksDocument() {
  const lines = MAKER_MODELS.map((model, index) =>
    actor({ key: `m${index}`, label: `maker ${index + 1}`, harness: 'opencode', model, state: 'working', updated: ago(1 + index) }),
  );
  lines.push(
    actor({ key: 'c1', label: 'own maker', harness: 'claude-code', model: 'claude-opus-5-5', state: 'idle', action: null, updated: ago(20) }),
    actor({ key: 'o1', label: 'harness mark', harness: 'opencode', state: 'waiting', reason: 'permission', action: { tool: 'shell', target: 'git status' }, updated: ago(21) }),
    actor({ key: 'a1', label: 'no mark', harness: 'aider', state: 'quiet', updated: ago(22) }),
  );
  const wt1 = worktree('wt1', { branch: 'marks', free: false, reasons: ['branch'], activity: 'waiting', actors: lines, last_event: ago(1) });
  const wt2 = worktree('wt2', { branch: 'build', free: false, reasons: ['branch'], activity: 'building', builds: [{ pid: 4242, name: 'cargo-nextest' }] });
  return mustValidate(document([worktree('fg'), wt1, wt2, worktree('wt3')]));
}

/**
 * Each agent line of the Agent column: its row, its state icon's and mark box's rectangles, the
 * ids the mark box names, where the line's text starts and in what colour, and the background.
 */
function markFacts() {
  const V = window.__visual;
  const box = (el) => {
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  };
  const out = [];
  for (const row of document.querySelectorAll('tr[data-row-id]')) {
    const cell = row.querySelector(':scope > td:nth-child(5)');
    if (!cell) continue;
    [...cell.querySelectorAll('.line')].forEach((line, index) => {
      const icon = line.querySelector('.line-icon');
      const mark = line.querySelector('.line-mark');
      const texts = V.texts(line).filter((t) => !(icon && icon.contains(t.node)) && !(mark && mark.contains(t.node)));
      const text = texts[0];
      const bg = V.bgOf(mark ?? icon ?? line);
      const painted = (el) => {
        const c = V.paint(V.st(el).color, el, bg);
        return [c.r, c.g, c.b].map(Math.round);
      };
      out.push({
        row: row.dataset.rowId.split('/').pop(),
        index,
        label: line.textContent.replace(/\s+/g, ' ').trim().slice(0, 30),
        icon: box(icon),
        mark: box(mark),
        iconFirst: line.firstElementChild === icon && icon !== null,
        markAfterIcon: mark !== null && icon?.nextElementSibling === mark,
        id: mark?.getAttribute('data-mark') ?? null,
        badge: mark?.getAttribute('data-badge') ?? null,
        textLeft: text ? Math.min(...text.rects.map((r) => r.left)) : null,
        // The colours the line's texts are painted in, each as its opacity leaves it: a line's label
        // and what it does may differ, and the mark takes the line's own colour.
        textColours: [...new Set([painted(line), ...texts.map((t) => painted(t.parent))].map(String))].map((c) => c.split(',').map(Number)),
        bg: [bg.r, bg.g, bg.b].map(Math.round),
      });
    });
  }
  return out;
}

function luminance([r, g, b]) {
  const f = (v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * The ink of each region of a PNG taken at `scale`, in CSS px: its bounds, the colour of the pixel
 * that differs most from the background, and its connected parts (8 neighbours) with their bounds
 * and device pixel lists. Ink is a pixel over `threshold` in a channel from the background `ref`.
 */
async function inkParts(helper, png, scale, regions, threshold) {
  return helper.page.evaluate(
    async ({ b64, s, list, limit }) => {
      const bitmap = await createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob());
      const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
      const ctx = canvas.getContext('2d');
      ctx.drawImage(bitmap, 0, 0);
      const out = {};
      for (const { key, rect, ref } of list) {
        const x0 = Math.max(0, Math.round(rect.x * s));
        const y0 = Math.max(0, Math.round(rect.y * s));
        const w = Math.min(bitmap.width, Math.round((rect.x + rect.w) * s)) - x0;
        const h = Math.min(bitmap.height, Math.round((rect.y + rect.h) * s)) - y0;
        const data = ctx.getImageData(x0, y0, w, h).data;
        const diff = (i) => Math.max(Math.abs(data[i] - ref[0]), Math.abs(data[i + 1] - ref[1]), Math.abs(data[i + 2] - ref[2]));
        const seen = new Uint8Array(w * h);
        let strongest = { diff: 0, colour: null };
        const parts = [];
        let [bx0, by0, bx1, by1, count] = [Infinity, Infinity, -Infinity, -Infinity, 0];
        for (let y = 0; y < h; y += 1) {
          for (let x = 0; x < w; x += 1) {
            const i = (y * w + x) * 4;
            const d = diff(i);
            if (d <= limit) continue;
            count += 1;
            [bx0, by0, bx1, by1] = [Math.min(bx0, x), Math.min(by0, y), Math.max(bx1, x), Math.max(by1, y)];
            if (d > strongest.diff) strongest = { diff: d, colour: [data[i], data[i + 1], data[i + 2]] };
            if (seen[y * w + x]) continue;
            const pixels = [];
            const stack = [[x, y]];
            seen[y * w + x] = 1;
            while (stack.length) {
              const [px, py] = stack.pop();
              pixels.push([px, py]);
              for (let dy = -1; dy <= 1; dy += 1) {
                for (let dx = -1; dx <= 1; dx += 1) {
                  const nx = px + dx;
                  const ny = py + dy;
                  if (nx < 0 || ny < 0 || nx >= w || ny >= h || seen[ny * w + nx]) continue;
                  if (diff((ny * w + nx) * 4) <= limit) continue;
                  seen[ny * w + nx] = 1;
                  stack.push([nx, ny]);
                }
              }
            }
            const xs = pixels.map((p) => p[0]);
            const ys = pixels.map((p) => p[1]);
            parts.push({ pixels, left: Math.min(...xs), right: Math.max(...xs) + 1, top: Math.min(...ys), bottom: Math.max(...ys) + 1 });
          }
        }
        out[key] = {
          count,
          left: count ? (x0 + bx0) / s : null,
          right: count ? (x0 + bx1 + 1) / s : null,
          top: count ? (y0 + by0) / s : null,
          bottom: count ? (y0 + by1 + 1) / s : null,
          colour: strongest.colour,
          parts: parts.map((p) => ({
            left: (x0 + p.left) / s,
            right: (x0 + p.right) / s,
            top: (y0 + p.top) / s,
            bottom: (y0 + p.bottom) / s,
            pixels: p.pixels,
          })),
        };
      }
      return out;
    },
    { b64: png.toString('base64'), s: scale, list: regions, limit: threshold },
  );
}

const longer = (ink) => Math.max(ink.right - ink.left, ink.bottom - ink.top);

/** The smallest device pixel distance (Chebyshev) between two parts' pixel lists. */
function gap(a, b) {
  let nearest = Infinity;
  for (const [ax, ay] of a.pixels) {
    for (const [bx, by] of b.pixels) nearest = Math.min(nearest, Math.max(Math.abs(ax - bx), Math.abs(ay - by)));
  }
  return nearest;
}

/** Saves the shot for the owner to see. Nothing is compared: no baseline is approved yet. */
async function save(page, name) {
  const dir = path.join(runDir(), 'shots');
  mkdirSync(dir, { recursive: true });
  await page.screenshot({ path: path.join(dir, `${name}.png`), fullPage: true });
}

for (const theme of THEME_NAMES) {
  test(`agent_mark__should_meet_contrast_and_size ${theme}`, async ({ browser }) => {
    // Given
    const context = await browser.newContext({ viewport: { width: 1100, height: 1400 }, deviceScaleFactor: SCALE });
    const page = await context.newPage();
    const helper = await pixelHelper(browser);
    const panel = await visualPanel(page, { theme, height: 1400, extraBodyClass: 'vscode-reduce-motion' });
    const doc = marksDocument();
    const wt1 = doc.pools[0].worktrees.find((w) => w.name === 'wt1').path;
    await showPlain(page, panel, doc);
    const problems = [];

    // When
    const modes = [
      ['plain', async () => {}],
      ['hovered', () => hoverRow(page, wt1)],
      [
        'selected',
        async () => {
          await clickRow(page, wt1);
          await away(page);
          await settle(page);
        },
      ],
    ];
    for (const [mode, enter] of modes) {
      await enter();
      const facts = await page.evaluate(markFacts);
      const png = await page.screenshot({ animations: 'disabled' });
      await save(page, `agent_mark-${theme}-${mode}`);
      const grow = (b) => ({ x: b.x - 1, y: b.y - 1, w: b.w + 2, h: b.h + 2 });
      const regions = facts.flatMap((f, i) => [
        ...(f.icon ? [{ key: `icon${i}`, rect: grow(f.icon), ref: f.bg }] : []),
        ...(f.mark ? [{ key: `mark${i}`, rect: grow(f.mark), ref: f.bg }] : []),
      ]);
      const ink = await inkParts(helper, png, SCALE, regions, INK);
      const sizes = facts.flatMap((f, i) => (f.icon && ink[`icon${i}`].count ? [longer(ink[`icon${i}`])] : [])).sort((a, b) => a - b);
      const median = sizes[Math.floor(sizes.length / 2)];

      // Then
      const starts = facts.filter((f) => f.textLeft !== null).map((f) => f.textLeft);
      if (Math.max(...starts) - Math.min(...starts) > BOX_PX) problems.push(`${mode}: the lines' text starts at ${[...new Set(starts.map((x) => Math.round(x * 10) / 10))].join(', ')} px, not one x (U13)`);
      for (const [i, f] of facts.entries()) {
        const at = `${mode} ${f.row} line ${f.index + 1} "${f.label}"`;
        if (!f.iconFirst || !f.markAfterIcon || !f.mark) {
          problems.push(`${at}: the state icon is not first, or the mark box does not follow it (U13)`);
          continue;
        }
        for (const [name, b] of [['state icon', f.icon], ['mark box', f.mark]]) {
          if (Math.abs(b.w - BOX) > BOX_PX || Math.abs(b.h - BOX) > BOX_PX) problems.push(`${at}: ${name} ${b.w} x ${b.h} px, not ${BOX} px (U13)`);
        }
        if (Math.abs(f.icon.y - f.mark.y) > BOX_PX) problems.push(`${at}: the boxes' tops differ: ${f.icon.y} and ${f.mark.y} px (U13)`);
        if (Math.abs(f.mark.x - (f.icon.x + f.icon.w) - GAP) > BOX_PX) problems.push(`${at}: the mark box starts ${Math.round((f.mark.x - f.icon.x - f.icon.w) * 10) / 10} px after the state icon, not ${GAP} px (U13)`);
        if (f.textLeft !== null && Math.abs(f.textLeft - (f.mark.x + f.mark.w) - GAP) > BOX_PX) problems.push(`${at}: the text starts ${Math.round((f.textLeft - f.mark.x - f.mark.w) * 10) / 10} px after the mark box, not ${GAP} px (U13)`);
        const mark = ink[`mark${i}`];
        if (!f.id) {
          if (mark.count) problems.push(`${at}: an empty mark box holds ink (U13)`);
          continue;
        }
        if (!mark.count) {
          problems.push(`${at}: the ${f.id} mark box holds no ink (U18)`);
          continue;
        }
        if (mark.left < f.mark.x - BOX_PX || mark.right > f.mark.x + f.mark.w + BOX_PX || mark.top < f.mark.y - BOX_PX || mark.bottom > f.mark.y + f.mark.h + BOX_PX) {
          problems.push(`${at}: the ${f.id} mark's ink leaves its ${BOX} px box (U18)`);
        }
        const ratio = contrast(mark.colour, f.bg);
        if (ratio < 3) problems.push(`${at}: the ${f.id} mark ${Math.round(ratio * 100) / 100}:1 against its row, under 3:1 (U18)`);
        if (!f.textColours.some((want) => mark.colour.every((v, k) => Math.abs(v - want[k]) <= COLOUR_TOLERANCE))) problems.push(`${at}: the ${f.id} mark is rgb(${mark.colour}), in none of the colours of the line's text ${JSON.stringify(f.textColours)} (U18)`);
        const centre = { x: f.mark.x + f.mark.w / 2, y: f.mark.y + f.mark.h / 2 };
        if (!f.badge) {
          if (Math.abs(longer(mark) - median) > INK_PX + 0.01) problems.push(`${at}: the ${f.id} mark's longer side is ${longer(mark)} px against the state icons' median ${median} px (U13)`);
          if (Math.abs(longer(mark) - MARK_PX) > INK_PX + 0.01) problems.push(`${at}: the unbadged ${f.id} mark is ${longer(mark)} px long, not ${MARK_PX} px (U18)`);
          if (Math.abs((mark.left + mark.right) / 2 - centre.x) > INK_PX || Math.abs((mark.top + mark.bottom) / 2 - centre.y) > INK_PX) problems.push(`${at}: the ${f.id} mark is not centred in its box (U18)`);
          continue;
        }
        const badges = mark.parts.filter((p) => {
          const [w, h] = [p.right - p.left, p.bottom - p.top];
          return w >= BADGE_GLYPH_PX[0] && w <= BADGE_GLYPH_PX[1] && h >= BADGE_GLYPH_PX[0] && h <= BADGE_GLYPH_PX[1] && (p.left + p.right) / 2 > centre.x && (p.top + p.bottom) / 2 > centre.y;
        });
        if (badges.length !== 1) {
          problems.push(`${at}: the ${f.badge} badge is not one separate 7 px glyph in the lower right of the mark box: its ink has ${mark.parts.length} parts ${JSON.stringify(mark.parts.map((p) => [p.right - p.left, p.bottom - p.top]))}, so the ring of the row's background is not visible (U18)`);
          continue;
        }
        const [badge] = badges;
        const rest = mark.parts.filter((p) => p !== badge);
        if (!rest.length) {
          problems.push(`${at}: the ${f.badge} badge shows without the ${f.id} mark (U18)`);
          continue;
        }
        const body = { left: Math.min(...rest.map((p) => p.left)), right: Math.max(...rest.map((p) => p.right)), top: Math.min(...rest.map((p) => p.top)), bottom: Math.max(...rest.map((p) => p.bottom)) };
        // The badge's ring may clip a mark's lower right corner, so its ink is at most 10 px, and
        // starts at the box's upper left.
        if (longer(body) > BADGED_MARK_PX + INK_PX + 0.01 || longer(body) < BADGED_MARK_PX - CLIPPED_PX) problems.push(`${at}: the ${f.id} mark beside its badge is ${longer(body)} px long, not ${BADGED_MARK_PX} px, or ${CLIPPED_PX} px less where the ring clips it (U18)`);
        if (body.left - f.mark.x > 2.5 || body.top - f.mark.y > 2.5) problems.push(`${at}: the ${f.id} mark beside its badge starts ${Math.round((body.left - f.mark.x) * 10) / 10} px right and ${Math.round((body.top - f.mark.y) * 10) / 10} px below the box's upper left corner (U18)`);
        if ((body.left + body.right) / 2 > centre.x + BOX_PX || (body.top + body.bottom) / 2 > centre.y + BOX_PX) problems.push(`${at}: the ${f.id} mark is not in the upper left of its box (U18)`);
        if (badge.right < f.mark.x + f.mark.w - 2.5 || badge.bottom < f.mark.y + f.mark.h - 2.5 || badge.right > f.mark.x + f.mark.w + BOX_PX || badge.bottom > f.mark.y + f.mark.h + BOX_PX) problems.push(`${at}: the ${f.badge} badge is not flush in the lower right corner of the box (U18)`);
        for (const other of rest.filter((p) => p.pixels.length >= 4)) {
          if (gap(badge, other) < RING_DEVICE_PX) problems.push(`${at}: the ${f.badge} badge touches the ${f.id} mark: no ring of the row's background between them (U18)`);
        }
      }
      if (theme === THEME_NAMES[0] && mode === 'plain') {
        const ids = facts.map((f) => f.id);
        for (const want of ['anthropic', 'openai', 'google', 'deepseek', 'zhipu', 'qwen', 'moonshot', 'mistral', 'meta', 'xai', 'minimax', 'opencode']) {
          if (!ids.includes(want)) problems.push(`setup: no line shows the ${want} mark (U18)`);
        }
      }
      await page.evaluate(() => document.activeElement?.blur?.());
    }
    await helper.close();
    await context.close();

    // Then
    assert.deepEqual(problems, [], `agent marks off contrast, size or alignment in ${theme}`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });
}
