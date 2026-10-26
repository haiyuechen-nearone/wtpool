// The rows of the Worktrees tab as a user reads them, and what they must read, for the e2e
// layer. Expected rows come from the oracle's text model (support/grammar.mjs).
import { age, expectedRow, normalizeEscapes, rowOrder } from '../support/grammar.mjs';

/**
 * The rows of the panel as the oracle's text model lists them, Last included. A cell's text
 * leaves out U12's count, `.count`, and the visually hidden text U12 and U13 add for screen
 * readers, an element taken out of the flow whose box is at most 1 px square or clipped away,
 * so the text model of phases 1 to 3 reads a phase 4 row as before.
 */
export async function shownRows(frame) {
  return frame.locator('tr[data-row-id]').evaluateAll((rows) =>
    rows.map((row) => {
      const cells = [...row.children].filter((c) => c.tagName === 'TD' && !c.classList.contains('actions'));
      const hidden = (el) => {
        if (el.matches('.count')) return true;
        const style = getComputedStyle(el);
        if (style.position !== 'absolute' && style.position !== 'fixed') return false;
        const box = el.getBoundingClientRect();
        const clipped = /rect\(0(px)?,? 0(px)?,? 0(px)?,? 0(px)?\)/.test(style.clip) || /inset\(50%\)/.test(style.clipPath);
        return clipped || (box.width <= 1 && box.height <= 1);
      };
      const text = (el) => {
        if (!el) return '';
        const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
        let found = '';
        for (let node = walker.nextNode(); node; node = walker.nextNode()) {
          let skip = false;
          for (let up = node.parentElement; up && up !== el.parentElement; up = up.parentElement) {
            if (hidden(up)) {
              skip = true;
              break;
            }
          }
          if (!skip) found += node.textContent;
        }
        return found.replace(/\s+/g, ' ').trim();
      };
      const lines = (el) => {
        const found = [...(el?.querySelectorAll('.line') ?? [])];
        return found.length ? found.map(text) : [text(el)];
      };
      return {
        id: row.dataset.rowId,
        activity: text(cells[0]),
        worktree: text(cells[1]),
        branch: text(cells[2]),
        git: text(cells[3]),
        agent: lines(cells[4]),
        last: lines(cells[5]),
      };
    }),
  );
}

/** A scenario document with its paths resolved to the pool the harness built. */
export function resolved(status, poolInfo) {
  return JSON.parse(
    JSON.stringify(status)
      .replaceAll('/Users/verify/pool/fg/.git', poolInfo.common)
      .replaceAll('/Users/verify/pool', poolInfo.dir),
  );
}

/** What the rows of `status` must read, Last left out: scenario times are not the pool's. */
export function expectedRows(status, poolInfo) {
  return rowOrder(resolved(status, poolInfo).pools[0].worktrees).map((w) => {
    const { last: _, ...row } = expectedRow(w, Date.now(), status.machine.jobs);
    return { id: w.path, ...row };
  });
}

/** What each row's Last must read at `nowMs`, from a document `wtpool status --json` printed. */
export function expectedLast(status, nowMs) {
  return Object.fromEntries(rowOrder(status.pools[0].worktrees).map((w) => [w.path, expectedRow(w, nowMs).last]));
}

function normalizedRow(row) {
  return {
    ...row,
    activity: normalizeEscapes(row.activity),
    worktree: normalizeEscapes(row.worktree),
    branch: normalizeEscapes(row.branch),
    git: normalizeEscapes(row.git),
    agent: row.agent.map(normalizeEscapes),
  };
}

/** The shown rows without Last, escapes normalised, to compare with {@link expectedRows}. */
export async function rowsWithoutLast(frame) {
  return (await shownRows(frame)).map(({ last: _, ...row }) => normalizedRow(row));
}

/** Waits until the rows read `expected`, at most `timeoutMs`; returns the last rows seen. */
export async function waitForRows(frame, expected, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let seen = [];
  for (;;) {
    try {
      seen = await rowsWithoutLast(frame);
    } catch {
      seen = [];
    }
    if (JSON.stringify(seen) === JSON.stringify(expected)) return seen;
    if (Date.now() >= deadline) return seen;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}

/** Whether a shown Last reads the age of `time` at some instant between `fromMs` and `toMs`. */
export function lastMatches(shown, time, fromMs, toMs) {
  const allowed = new Set();
  for (let at = fromMs; at <= toMs + 999; at += 500) allowed.add(age(time, at));
  return allowed.has(shown);
}

/**
 * U12 and U13 as a row shows them: per row its count `.count` (text, whether screen readers skip
 * it, its colour and border, and whether it sits on the word's line), the activity cell's
 * visually hidden text, and per agent line its visually hidden text and its `.line-icon` box.
 */
export async function lineFacts(frame) {
  return frame.locator('tr[data-row-id]').evaluateAll((rows) =>
    rows.map((row) => {
      const cells = [...row.children].filter((c) => c.tagName === 'TD' && !c.classList.contains('actions'));
      const hidden = (el) => {
        const style = getComputedStyle(el);
        if (style.position !== 'absolute' && style.position !== 'fixed') return false;
        const box = el.getBoundingClientRect();
        const clipped = /rect\(0(px)?,? 0(px)?,? 0(px)?,? 0(px)?\)/.test(style.clip) || /inset\(50%\)/.test(style.clipPath);
        return clipped || (box.width <= 1 && box.height <= 1);
      };
      const hiddenText = (el) => [...el.querySelectorAll('*')].filter(hidden).map((e) => e.textContent).join('');
      const skipped = (el) => {
        for (let up = el; up && up !== row; up = up.parentElement) if (up.getAttribute('aria-hidden') === 'true') return true;
        return false;
      };
      const activity = cells[0];
      const count = activity.querySelector('.count');
      // The Activity word: the cell's first shown text outside the count.
      const walker = document.createTreeWalker(activity, NodeFilter.SHOW_TEXT);
      let wordNode = null;
      for (let node = walker.nextNode(); node && !wordNode; node = walker.nextNode()) {
        if (!node.textContent.trim() || node.parentElement.closest('.count')) continue;
        let shown = true;
        for (let up = node.parentElement; up && up !== row; up = up.parentElement) if (hidden(up)) shown = false;
        if (shown) wordNode = node;
      }
      const word = wordNode?.parentElement ?? null;
      const countStyle = count ? getComputedStyle(count) : null;
      const countBox = count?.getBoundingClientRect();
      let wordBox = null;
      if (wordNode) {
        const range = document.createRange();
        range.selectNodeContents(wordNode);
        wordBox = [...range.getClientRects()].at(-1) ?? null;
      }
      return {
        id: row.dataset.rowId,
        count: count ? count.textContent.replace(/\s+/g, ' ').trim() : null,
        countSkipped: count ? skipped(count) : null,
        countColour: countStyle?.color ?? null,
        countBorder: countStyle ? ['Top', 'Right', 'Bottom', 'Left'].map((side) => `${countStyle[`border${side}Width`]} ${countStyle[`border${side}Style`]} ${countStyle[`border${side}Color`]}`) : null,
        wordColour: word ? getComputedStyle(word).color : null,
        sameLine: count && wordBox ? countBox.top < wordBox.bottom && countBox.bottom > wordBox.top && countBox.left >= wordBox.right - 1 : null,
        activityHidden: hiddenText(activity),
        lines: [...(cells[4]?.querySelectorAll('.line') ?? [])].map((line) => {
          const icon = line.querySelector('.line-icon');
          const box = icon?.getBoundingClientRect();
          // The first shown character after the icon: where the line's text starts (U13).
          const walker = document.createTreeWalker(line, NodeFilter.SHOW_TEXT);
          let textLeft = null;
          for (let node = walker.nextNode(); node && textLeft === null; node = walker.nextNode()) {
            if (!node.textContent.trim() || [...(function* up(el) { for (let e = el; e && e !== line; e = e.parentElement) yield e; })(node.parentElement)].some(hidden)) continue;
            const range = document.createRange();
            range.selectNodeContents(node);
            const rect = [...range.getClientRects()].find((r) => r.width > 0);
            if (rect) textLeft = rect.left;
          }
          const before = icon ? getComputedStyle(icon, '::before') : null;
          const svg = icon?.querySelector('svg');
          return {
            hidden: hiddenText(line),
            textLeft,
            icon: icon
              ? {
                  width: box.width,
                  height: box.height,
                  right: box.right,
                  skipped: skipped(icon),
                  signature: svg ? `svg ${svg.querySelectorAll('*').length}` : before && before.content !== 'none' && before.content !== 'normal' ? `glyph ${before.content}` : 'empty',
                }
              : null,
          };
        }),
      };
    }),
  );
}

/** U13's visually hidden start of an actor's line. */
export function statePrefix(actor) {
  return actor.state === 'idle' && actor.error ? 'idle, API error: ' : `${actor.state}: `;
}

/** What U12 and U13 make of each row of `status`: the count, the hidden text, the line prefixes. */
export function expectedLineFacts(status, poolInfo) {
  return rowOrder(resolved(status, poolInfo).pools[0].worktrees).map((w) => {
    const n = w.actors.length;
    const shown = [...w.actors].sort((a, b) => Date.parse(b.updated) - Date.parse(a.updated));
    return {
      id: w.path,
      count: n >= 2 ? `×${n}` : null,
      activityHidden: n >= 2 ? `, ${n} agents` : '',
      prefixes: shown.map(statePrefix),
    };
  });
}

/** The problems of `facts` (from {@link lineFacts}) against {@link expectedLineFacts}. */
export function lineProblems(facts, expected) {
  const problems = [];
  const byId = new Map(facts.map((f) => [f.id, f]));
  for (const want of expected) {
    const got = byId.get(want.id);
    if (!got) {
      problems.push(`${want.id}: no row`);
      continue;
    }
    if (got.count !== want.count) problems.push(`${want.id}: the count reads ${JSON.stringify(got.count)}, not ${JSON.stringify(want.count)} (U12)`);
    if (want.count && got.countSkipped !== true) problems.push(`${want.id}: screen readers read the count ${want.count} (U12)`);
    if (got.activityHidden.trim() !== want.activityHidden.trim()) problems.push(`${want.id}: the cell's visually hidden text is ${JSON.stringify(got.activityHidden)}, not ${JSON.stringify(want.activityHidden)} (U12)`);
    if (want.prefixes.length) {
      const prefixes = got.lines.map((line) => line.hidden);
      if (JSON.stringify(prefixes.map((p) => p.trimEnd())) !== JSON.stringify(want.prefixes.map((p) => p.trimEnd()))) {
        problems.push(`${want.id}: the lines' visually hidden starts are ${JSON.stringify(prefixes)}, not ${JSON.stringify(want.prefixes)} (U13)`);
      }
    }
  }
  return problems;
}
