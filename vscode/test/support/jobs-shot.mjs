// The phase 4 states the shot layer measures (status-panel.md U10 to U15, A8): documents with heavy
// jobs and agent lines, the strip opened through the machine button, and in-page measurements of
// the button, the strip, the chips, the `×N` count and the agent line icons. Each in-page function
// runs through page.evaluate after measure.mjs's installHelpers and uses nothing else from outside.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { POOL, actor, ago, document, mustValidate, view, withActor, worktree } from './docs.mjs';
import { busyDocument, firstOpenDocument, job, jobs, withJobs } from './jobs.mjs';
import { TEST } from './paths.mjs';

/** The states of the open strip that U11 names: busy, first open, over the limit, error, old, remote. */
export function stripStates() {
  const busy = busyDocument();
  const over = withJobs(busy, { ...busy.machine.jobs, limit: 2, default: 4 });
  const error = withJobs(busy, {
    ...busy.machine.jobs,
    limit: null,
    error: '/Users/verify/.config/wtpool/machine.toml: line 2: limit = 0 is outside 1 to 64',
    running: [],
    waiting: [],
  });
  const old = structuredClone(busy);
  delete old.machine.jobs;
  const defaults = withJobs(firstOpenDocument(), { ...firstOpenDocument().machine.jobs, source: { limit: 'default', enforce: 'default' } });
  return [
    { name: 'busy', doc: mustValidate(busy) },
    { name: 'first_open', doc: mustValidate(firstOpenDocument()) },
    { name: 'defaults', doc: mustValidate(defaults) },
    { name: 'over_the_limit', doc: mustValidate(over), text: '4 running, over the limit of 2' },
    { name: 'config_error', doc: mustValidate(error) },
    { name: 'old_wtpool', doc: mustValidate(old), text: 'This wtpool has no heavy job limit.' },
    { name: 'remote_folder', doc: mustValidate(busy), extra: { folder_remote: true }, text: "This window's folder is not on this computer" },
  ];
}

/**
 * The NotQueued mockup (A8, U14) under `warn`: wt1's agent works on a `cargo test` no job holds,
 * wt2 builds `make` outside a job, wt3 builds `cargo nextest` inside one.
 */
export function notQueuedDocument() {
  const wt1 = withActor(
    'wt1',
    { key: 'nq1', label: 'session 10:02', action: { tool: 'build:', target: 'cargo test' } },
    { branch: 'design-jobs', builds: [{ pid: 5101, name: 'cargo test', queued: false }] },
  );
  const building = (name, branch, build) =>
    worktree(name, { branch, reasons: ['branch'], free: false, activity: 'building', builds: [build] });
  const status = document([
    worktree('fg'),
    wt1,
    building('wt2', 'fix-tail', { pid: 5102, name: 'make', queued: false }),
    building('wt3', 'review-ts', { pid: 5103, name: 'cargo nextest', queued: true }),
    worktree('wt4'),
  ]);
  const pool = POOL;
  return mustValidate(
    withJobs(
      status,
      jobs({
        heavy: [
          { pattern: 'cargo test', source: 'repository' },
          { pattern: 'make', source: 'repository' },
          { pattern: 'cargo nextest', source: 'repository' },
        ],
        running: [job({ summary: 'cargo nextest', worktree: `${pool}/wt3` })],
        outside: [
          job({ summary: 'cargo test', worktree: `${pool}/wt1`, actor: 'nq1', since: ago(120) }),
          job({ summary: 'make', worktree: `${pool}/wt2`, since: ago(180) }),
        ],
      }),
    ),
  );
}

/** The agent line states of U13, one line each in wt1, then a `build:` line and `no agent` lines. */
export const LINE_STATES = ['working', 'quiet', 'waiting', 'idle', 'error', 'done'];

/** wt1 lists one agent line per state of U13, newest first in LINE_STATES order; wt2 a build. */
export function lineStatesDocument() {
  const lines = [
    actor({ key: 'l1', label: 'Review the TS host', state: 'working', action: { tool: 'Read', target: 'vscode/src/host/panel.ts' }, updated: ago(2) }),
    actor({ key: 'l2', label: 'session 08:40', state: 'quiet', action: { tool: 'Bash', target: 'cargo build' }, updated: ago(10) }),
    actor({ key: 'l3', label: 'session 09:12', state: 'waiting', reason: 'permission', action: { tool: 'Bash', target: 'git push' }, updated: ago(20) }),
    actor({ key: 'l4', label: 'session 08:12', state: 'idle', action: null, updated: ago(30) }),
    actor({ key: 'l5', label: 'session 07:55', state: 'idle', action: null, error: 'rate_limit', updated: ago(40) }),
    actor({ key: 'l6', kind: 'subagent', label: 'Explore icons', state: 'done', action: null, updated: ago(50) }),
  ];
  const wt1 = worktree('wt1', { branch: 'design-jobs', free: false, reasons: ['branch'], activity: 'waiting', actors: lines, last_event: ago(2) });
  const wt2 = worktree('wt2', { branch: 'fix-tail', free: false, reasons: ['branch'], activity: 'building', builds: [{ pid: 4242, name: 'cargo nextest' }] });
  return mustValidate(document([worktree('fg'), wt1, wt2, worktree('wt3')]));
}

/** Posts a view message of `status` with `extra` fields, such as `folder_remote` (jobs.md, item 4). */
export async function showView(page, status, extra = {}) {
  await page.evaluate((message) => {
    window.dispatchEvent(new MessageEvent('message', { data: message }));
  }, { ...view(status), ...extra });
  await page.clock.runFor(50);
}

/** Opens the machine strip with a click on the machine button, unless it is open (U10, U11). */
export async function openStrip(page, at) {
  const button = page.locator('tr.pool-heading button.machine-button');
  assert.ok((await button.count()) > 0, `${at}: the heading holds no button.machine-button (U10)`);
  if ((await page.locator('tr.machine-detail').count()) === 0) {
    await button.click();
    await page.clock.runFor(50);
  }
  assert.ok((await page.locator('tr.machine-detail').count()) > 0, `${at}: a click on the machine button opens no tr.machine-detail (U11)`);
}

/** Tabs until the machine button has focus, and returns whether it got it. */
export async function tabToButton(page, steps = 30) {
  for (let step = 0; step < steps; step += 1) {
    await page.keyboard.press('Tab');
    await page.clock.runFor(20);
    if (await page.evaluate(() => document.activeElement?.matches?.('button.machine-button') ?? false)) return true;
  }
  return false;
}

/** axe-core over the whole page, its contrast rule off as in axe__should_report_no_violation. */
export async function axeViolations(page) {
  const source = readFileSync(path.join(TEST, 'node_modules', 'axe-core', 'axe.min.js'), 'utf8');
  await page.evaluate(source);
  const result = await page.evaluate(() =>
    window.axe.run(document, {
      runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'] },
      rules: { 'color-contrast': { enabled: false } },
    }),
  );
  return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(', ')}`);
}

/**
 * U11 and U6: the strip's text, the elements that hold all of it with their borders, and the
 * colours of its form controls against their own backgrounds.
 */
export function stripFacts() {
  const V = window.__visual;
  const strip = document.querySelector('tr.machine-detail');
  if (!strip) return { present: false };
  const rects = V.texts(strip).flatMap((t) => t.rects);
  const text = V.texts(strip).map((t) => t.node.textContent).join(' ').replace(/\s+/g, ' ').trim();
  const all = rects.length
    ? { left: Math.min(...rects.map((r) => r.left)), right: Math.max(...rects.map((r) => r.right)), top: Math.min(...rects.map((r) => r.top)), bottom: Math.max(...rects.map((r) => r.bottom)) }
    : null;
  const boxes = all
    ? [strip, ...strip.querySelectorAll('*')]
        .filter((el) => {
          const r = el.getBoundingClientRect();
          return r.left <= all.left + 1 && r.right >= all.right - 1 && r.top <= all.top + 1 && r.bottom >= all.bottom - 1;
        })
        .map((el) => {
          const s = V.st(el);
          const sides = ['Top', 'Right', 'Bottom', 'Left'].map((side) => ({ width: V.px(s[`border${side}Width`]), style: s[`border${side}Style`], colour: s[`border${side}Color`] }));
          const outside = V.bgOf(el.parentElement);
          return { el: V.desc(el), sides, ratio: Math.min(...sides.map((side) => V.ratio(V.paint(side.colour, el, outside), outside))) };
        })
    : [];
  const controls = [...strip.querySelectorAll('input.job-limit, select.job-enforce, input.heavy-input')].map((el) => {
    const s = V.st(el);
    const bg = V.bgOf(el);
    return { el: V.desc(el), background: s.backgroundColor, colour: s.color, ratio: Math.round(V.ratio(V.paint(s.color, el, bg), bg) * 100) / 100 };
  });
  return { present: true, shown: V.shown(strip), text, boxes, controls };
}

/**
 * U15: each chip's pattern, its list, the element whose four borders hold its text, the font of
 * its text, and its remove or add glyph's colour against the glyph button's background.
 */
export function chipFacts() {
  const V = window.__visual;
  return [...document.querySelectorAll('tr.machine-detail li.heavy-chip')].map((chip) => {
    const list = chip.closest('ul.heavy-chips')?.dataset.scope ?? (chip.closest('ul.heavy-suggested') ? 'suggested' : '?');
    const texts = V.texts(chip);
    const rects = texts.flatMap((t) => t.rects);
    const holder = [chip, ...chip.querySelectorAll('*')].filter((el) => {
      const r = el.getBoundingClientRect();
      const s = V.st(el);
      const bordered = ['Top', 'Right', 'Bottom', 'Left'].every((side) => V.px(s[`border${side}Width`]) >= 1 && !['none', 'hidden'].includes(s[`border${side}Style`]));
      return bordered && rects.every((t) => t.left >= r.left - 0.5 && t.right <= r.right + 0.5 && t.top >= r.top - 0.5 && t.bottom <= r.bottom + 0.5);
    })[0];
    let border = null;
    if (holder) {
      const s = V.st(holder);
      const outside = V.bgOf(holder.parentElement);
      border = {
        widths: ['Top', 'Right', 'Bottom', 'Left'].map((side) => V.px(s[`border${side}Width`])),
        styles: [...new Set(['Top', 'Right', 'Bottom', 'Left'].map((side) => s[`border${side}Style`]))],
        colours: [...new Set(['Top', 'Right', 'Bottom', 'Left'].map((side) => s[`border${side}Color`]))],
        ratio: Math.round(V.ratio(V.paint(s.borderTopColor, holder, outside), outside) * 100) / 100,
      };
    }
    const first = texts.find((t) => !t.parent.closest('button'));
    const glyphs = [...chip.querySelectorAll('button.chip-remove, button.suggest-add')].map((button) => {
      const before = V.st(button, '::before');
      const glyph = [button, ...button.querySelectorAll('*')].find((el) => !['none', 'normal'].includes(V.st(el, '::before').content));
      const colour = glyph ? V.st(glyph, '::before').color : before.color;
      const bg = V.bgOf(button);
      return { el: V.desc(button), found: Boolean(glyph), ratio: Math.round(V.ratio(V.paint(colour, button, bg), bg) * 100) / 100 };
    });
    return { list, text: first ? first.node.textContent.trim() : '', font: first ? V.st(first.parent).fontFamily : '', border, glyphs };
  });
}

/** U10: the machine button's text parts with their colours, background, outline and contrast. */
export function buttonFacts() {
  const V = window.__visual;
  const button = document.querySelector('tr.pool-heading button.machine-button');
  if (!button) return { present: false };
  const s = V.st(button);
  const parts = V.texts(button).map((t) => {
    const ps = V.st(t.parent);
    const bg = V.bgOf(t.parent);
    return { text: t.node.textContent.trim(), colour: ps.color, ratio: Math.round(V.ratio(V.paint(ps.color, t.parent, bg), bg) * 100) / 100 };
  });
  const glyphs = [button, ...button.querySelectorAll('*')]
    .filter((el) => !['none', 'normal'].includes(V.st(el, '::before').content))
    .map((el) => V.st(el, '::before').color);
  return {
    present: true,
    background: s.backgroundColor,
    outline: { style: s.outlineStyle, width: V.px(s.outlineWidth), colour: s.outlineColor },
    parts,
    glyphs,
  };
}

/** U12: each row's `.count` elements and its Activity word, with boxes, colours and contrast. */
export function countFacts() {
  const V = window.__visual;
  return [...document.querySelectorAll('tr[data-row-id]')].map((row) => {
    const cell = row.querySelector(':scope > td');
    const word = V.texts(cell).find((t) => !t.parent.closest('.count'));
    const wordRect = word ? word.rects[0] : null;
    const counts = [...cell.querySelectorAll('.count')].filter((el) => V.shown(el)).map((el) => {
      const s = V.st(el);
      const r = el.getBoundingClientRect();
      const bg = V.bgOf(el);
      const outside = V.bgOf(el.parentElement);
      const sides = ['Top', 'Right', 'Bottom', 'Left'].map((side) => ({ width: V.px(s[`border${side}Width`]), style: s[`border${side}Style`], colour: s[`border${side}Color`] }));
      return {
        text: el.textContent.replace(/\s+/g, ' ').trim(),
        rect: { top: r.top, bottom: r.bottom, left: r.left, right: r.right },
        colour: s.color,
        sides,
        textRatio: Math.round(V.ratio(V.paint(s.color, el, bg), bg) * 100) / 100,
        borderRatio: Math.round(Math.min(...sides.map((side) => V.ratio(V.paint(side.colour, el, outside), outside))) * 100) / 100,
        clippedBy: V.visible(r, el.parentElement).cutters.map((c) => c.by),
      };
    });
    return {
      row: row.dataset.rowId.split('/').pop(),
      agentLines: row.querySelectorAll(':scope > td:nth-child(5) .line').length,
      word: word ? word.node.textContent.trim() : null,
      wordColour: word ? V.st(word.parent).color : null,
      wordRect: wordRect ? { top: wordRect.top, bottom: wordRect.bottom, right: wordRect.right } : null,
      counts,
    };
  });
}

/**
 * U13: each `.line` of the Agent column: its row, its line icon's box and colour (a codicon's
 * `::before`, else the fills of an inline SVG), the background behind it, and where its first
 * shown text starts. A probe `H` in the line's font is appended for the capital letters' ink.
 */
export function lineFacts() {
  const V = window.__visual;
  const out = [];
  for (const row of document.querySelectorAll('tr[data-row-id]')) {
    const cell = row.querySelector(':scope > td:nth-child(5)');
    if (!cell) continue;
    const lines = [...cell.querySelectorAll('.line')];
    (lines.length ? lines : [cell]).forEach((line, index) => {
      const icon = line.querySelector('.line-icon');
      const r = icon ? icon.getBoundingClientRect() : null;
      let colours = [];
      if (icon) {
        const glyph = [icon, ...icon.querySelectorAll('*')].find((el) => !['none', 'normal'].includes(V.st(el, '::before').content));
        if (glyph) {
          const b = V.st(glyph, '::before');
          colours = [b.maskImage && b.maskImage !== 'none' ? b.backgroundColor : b.color];
        } else {
          const shapes = [...icon.querySelectorAll('path, circle, rect, ellipse, polygon, line, polyline')];
          colours = [...new Set(shapes.map((shape) => V.st(shape).fill).filter((f) => f && f !== 'none'))];
        }
      }
      const text = V.texts(line).find((t) => !icon || !icon.contains(t.node));
      const textLeft = text ? Math.min(...text.rects.map((t) => t.left)) : null;
      const font = text ? V.st(text.parent) : V.st(line);
      const probe = document.createElement('span');
      probe.textContent = 'H';
      probe.dataset.verifyProbe = '1';
      for (const property of ['fontFamily', 'fontSize', 'fontWeight', 'fontStyle', 'lineHeight']) probe.style[property] = font[property];
      probe.style.color = font.color;
      probe.style.marginLeft = '24px';
      probe.style.whiteSpace = 'nowrap';
      line.append(probe);
      const p = probe.getBoundingClientRect();
      const bg = V.bgOf(icon ?? line);
      out.push({
        row: row.dataset.rowId.split('/').pop(),
        index,
        state: row.getAttribute('aria-selected') === 'true' ? 'selected' : row.matches(':hover') ? 'hovered' : 'plain',
        text: line.textContent.replace(/\s+/g, ' ').replace(/H$/, '').trim().slice(0, 40),
        icon: r ? { x: r.left, y: r.top, w: r.width, h: r.height } : null,
        colours,
        ratios: colours.map((c) => Math.round(V.ratio(V.paint(c, icon, bg), bg) * 100) / 100),
        bg: [bg.r, bg.g, bg.b].map(Math.round),
        textLeft,
        probe: { x: p.left, y: p.top, w: p.width, h: p.height },
      });
    });
  }
  return out;
}

/** Removes the probes {@link lineFacts} appended. */
export function removeProbes() {
  for (const el of document.querySelectorAll('[data-verify-probe]')) el.remove();
}

/** The running animations inside the Agent column, and those in the Activity cell. */
export function lineAnimations() {
  const where = (target) => {
    if (!target?.closest) return null;
    if (target.closest('tr[data-row-id] > td:nth-child(5)')) return 'agent';
    if (target.closest('tr[data-row-id] > td:first-child')) return 'activity';
    return null;
  };
  return document
    .getAnimations()
    .filter((a) => a.playState === 'running' && a.effect?.getComputedTiming().duration > 0)
    .map((a) => ({ where: where(a.effect?.target), row: a.effect?.target?.closest?.('tr[data-row-id]')?.dataset.rowId.split('/').pop() ?? null, target: `${a.effect?.target?.getAttribute?.('class') ?? a.effect?.target?.tagName}${a.effect?.pseudoElement ?? ''}` }));
}
