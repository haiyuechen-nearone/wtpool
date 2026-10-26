// The rows of the Worktrees tab as a user reads them, and what they must read, for the e2e
// layer. Expected rows come from the oracle's text model (support/grammar.mjs).
import { age, expectedRow, normalizeEscapes, rowOrder } from '../support/grammar.mjs';

/** The rows of the panel as the oracle's text model lists them, Last included. */
export async function shownRows(frame) {
  return frame.locator('tr[data-row-id]').evaluateAll((rows) =>
    rows.map((row) => {
      const cells = [...row.children].filter((c) => c.tagName === 'TD' && !c.classList.contains('actions'));
      const text = (el) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
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
    const { last: _, ...row } = expectedRow(w, Date.now());
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
