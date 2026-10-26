// The flows of the tab itself (verification.md, "User flows"): opening, revealing, restoring,
// hiding, closing and two windows (X1, X2, X7, X16, B10, U5).
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import path from 'node:path';
import { test } from '@playwright/test';

import { rowsWithoutLast, waitForRows } from './rows.mjs';
import {
  detailOwners,
  detailText,
  openStatus,
  poll,
  rowOf,
  scenarioRows,
  withFlow,
  worktreePath,
} from './flows.mjs';
import {
  editorGroups,
  git,
  isStatusTab,
  newWindow,
  openTextFile,
  panelFrame,
  runCommand,
  sleep,
  watchers,
  workbench,
  wtpoolProcesses,
} from './vscode.mjs';

const statusTabs = (groups) => groups.flatMap((g, index) => g.tabs.filter((t) => isStatusTab(t.label)).map((t) => ({ ...t, group: index })));

/** Clicks the Worktrees tab's label in the editor tab bar. */
async function clickStatusTab(window) {
  const tab = window.locator('.tabs-container .tab').filter({ hasText: /^Worktrees/ }).first();
  assert.ok(await tab.count(), 'no editor tab reads Worktrees');
  await tab.click();
}

test('flow__open_from_palette', async () => {
  await withFlow('flow__open_from_palette', {}, async (session) => {
    // Given
    const { window } = session;
    await openTextFile(window, session.dirs, 'notes.txt');
    const before = await editorGroups(window);

    // When
    await runCommand(window, 'Worktrees: Open Status');
    const frame = await panelFrame(window);
    const expected = scenarioRows(session);
    const rows = frame ? await waitForRows(frame, expected, 15_000) : [];
    const after = await editorGroups(window);

    // Then
    assert.equal(before.length, 1, 'setup: one editor group before the command');
    assert.ok(frame, 'Worktrees: Open Status showed no tab with rows');
    assert.equal(after.length, 2, `the tab did not open in a new group beside the active one: ${JSON.stringify(after)}`);
    assert.deepEqual(after[0].tabs.map((t) => t.label), ['notes.txt'], 'the active group changed');
    assert.equal(after[1].tabs.filter((t) => isStatusTab(t.label)).length, 1, 'no Worktrees tab in the group beside');
    assert.ok(after[1].tabs.find((t) => isStatusTab(t.label)).active, 'the Worktrees tab is not the shown tab of its group');
    assert.equal(statusTabs(after).length, 1, 'more than one Worktrees tab');
    assert.deepEqual(rows, expected, 'the tab does not show idle_pool');
  });
});

test('flow__open_again_reveals', async () => {
  await withFlow('flow__open_again_reveals', {}, async (session) => {
    // Given
    const { window } = session;
    await openTextFile(window, session.dirs, 'notes.txt');
    await openStatus(session);
    await clickStatusTab(window);
    await openTextFile(window, session.dirs, 'other.txt');
    await window.locator('.tabs-container .tab[aria-label^="notes.txt"]').first().click();
    await sleep(500);
    const before = await editorGroups(window);

    // When
    await runCommand(window, 'Worktrees: Open Status');
    await sleep(1000);
    const after = await editorGroups(window);
    const frame = await panelFrame(window, 10_000);
    const rows = frame ? await waitForRows(frame, scenarioRows(session), 10_000) : [];

    // Then
    const [hidden] = statusTabs(before);
    assert.ok(hidden && !hidden.active && before[hidden.group].tabs.some((t) => t.label === 'other.txt'), `the tab is not hidden behind other.txt: ${JSON.stringify(before)}`);
    assert.ok(!before[hidden.group].active, 'the Worktrees tab\'s group is active before the command');
    const tabs = statusTabs(after);
    assert.equal(tabs.length, 1, `the command opened a second Worktrees tab: ${JSON.stringify(after)}`);
    assert.equal(tabs[0].group, hidden.group, 'the Worktrees tab moved to another group');
    assert.ok(tabs[0].active, 'the Worktrees tab was not revealed in its group');
    assert.ok(after[tabs[0].group].active, 'the revealed tab\'s group has no focus');
    assert.deepEqual(rows, scenarioRows(session), 'the revealed tab does not show the pool');
  });
});

test('flow__reload_restores', async () => {
  await withFlow('flow__reload_restores', {}, async (session) => {
    // Given
    const { window } = session;
    const frame = await openStatus(session);
    const wt2 = worktreePath(session, 'wt2');
    await rowOf(frame, session, 'wt2').click();
    const selected = await poll(async () => (await detailOwners(frame)).join() === wt2, 5_000);

    // When
    await runCommand(window, 'Developer: Reload Window');
    await sleep(3000);
    await workbench(window);
    const restored = await panelFrame(window, 60_000);
    const rows = restored ? await waitForRows(restored, scenarioRows(session), 15_000) : [];
    const owners = restored ? await poll(async () => {
      const found = await detailOwners(restored);
      return found.length ? found : undefined;
    }, 10_000) : undefined;
    const strip = restored ? await detailText(restored) : null;
    const tabs = statusTabs(await editorGroups(window));

    // Then
    assert.ok(selected, 'clicking wt2 showed no detail strip under wt2');
    assert.ok(restored, 'the Worktrees tab did not come back with rows after a reload');
    assert.equal(tabs.length, 1, 'not exactly one Worktrees tab after the reload');
    assert.deepEqual(rows, scenarioRows(session), 'the restored tab does not show the pool');
    assert.deepEqual(owners, [wt2], 'wt2 is not selected with its detail strip after the reload');
    assert.ok(strip?.includes(wt2), `the restored detail strip lacks wt2's path: ${strip}`);
  });
});

test('flow__hide_and_show', async () => {
  await withFlow('flow__hide_and_show', {}, async (session) => {
    // Given
    const { window } = session;
    await openStatus(session);
    await clickStatusTab(window);
    await openTextFile(window, session.dirs, 'notes.txt');
    const hidden = statusTabs(await editorGroups(window));
    const wt1 = worktreePath(session, 'wt1');
    // A commit changes wt1's HEAD, which the watch sees at once (W2, R6), so the hidden tab's
    // extension holds the new document before the tab shows again.
    writeFileSync(path.join(wt1, 'change.txt'), 'wtpool-verify\n');
    git(wt1, ['add', 'change.txt']);
    git(wt1, ['commit', '--quiet', '-m', 'wtpool-verify: a commit while the tab is hidden']);
    await sleep(2500);
    const expected = scenarioRows(session).map((row) =>
      row.id === wt1 ? { ...row, activity: 'not reusable', git: '1 unreferenced' } : row,
    );

    // When
    await clickStatusTab(window);
    const shownAt = Date.now();
    const frame = await panelFrame(window, 2_000);
    const rows = frame ? await waitForRows(frame, expected, Math.max(0, 2_000 - (Date.now() - shownAt))) : [];
    const tookMs = Date.now() - shownAt;

    // Then
    assert.ok(hidden.length === 1 && !hidden[0].active, 'the Worktrees tab is not hidden behind notes.txt');
    assert.ok(frame, 'the shown tab has no rows within 2 s');
    assert.deepEqual(rows, expected, `wt1 is not current within 2 s of showing the tab (${tookMs} ms)`);
  });
});

/** The header a page shows before any view reaches it. */
const STARTING = 'starting…';
/** The pause between hiding the tab and showing it again, one per switch. */
const RESHOW_PAUSES_MS = [0, 0, 50, 0, 150, 0, 300, 0, 50, 0];
const RESHOW_BOUND_MS = 2_000;
/** A show that misses the bound is watched this much longer, to tell a late table from a lost view. */
const RESHOW_GRACE_MS = 8_000;

/** Clicks the label of the editor tab `name` in the editor tab bar. */
async function clickTab(window, name) {
  await window.locator(`.tabs-container .tab[aria-label^="${name}"]`).first().click();
}

/** Every frame of the window that holds the Worktrees page, rows or none. */
async function pageFrames(window) {
  const found = [];
  for (const frame of window.frames()) {
    try {
      if (await frame.locator('header .connection').count()) found.push(frame);
    } catch {
      // A frame that navigates away cannot be queried; the next round finds the new one.
    }
  }
  return found;
}

/**
 * Watches the tab from `shownAt` until a page other than `before` shows `expected` under a header
 * past starting, or the bound and the grace run out. Returns that page's frame, its header and
 * rows, and the milliseconds it took to fill, null when it never did.
 */
async function watchShow(window, expected, before, shownAt) {
  const deadline = shownAt + RESHOW_BOUND_MS + RESHOW_GRACE_MS;
  let seen = { frame: undefined, connection: null, rows: [], filledMs: null };
  for (;;) {
    for (const frame of await pageFrames(window)) {
      if (frame === before) continue;
      const connection = await frame.locator('header .connection').textContent().catch(() => null);
      const rows = await rowsWithoutLast(frame).catch(() => []);
      seen = { frame, connection, rows, filledMs: null };
      if (connection !== STARTING && JSON.stringify(rows) === JSON.stringify(expected)) {
        return { ...seen, filledMs: Date.now() - shownAt };
      }
    }
    if (Date.now() >= deadline) return seen;
    await sleep(50);
  }
}

test('reshow__should_fill_the_table_every_time', async () => {
  await withFlow('reshow__should_fill_the_table_every_time', {}, async (session) => {
    // Given
    const { window } = session;
    let before = await openStatus(session);
    await clickStatusTab(window);
    await openTextFile(window, session.dirs, 'notes.txt');
    const expected = scenarioRows(session);

    // When
    const shows = [];
    for (const pauseMs of RESHOW_PAUSES_MS) {
      if (pauseMs) await sleep(pauseMs);
      await clickStatusTab(window);
      const seen = await watchShow(window, expected, before, Date.now());
      shows.push({ pauseMs, filledMs: seen.filledMs, connection: seen.connection, rows: seen.rows.length });
      before = seen.frame ?? before;
      await clickTab(window, 'notes.txt');
    }
    test.info().annotations.push({ type: 'shows', description: JSON.stringify(shows) });

    // Then
    const all = JSON.stringify(shows);
    const unreloaded = shows.filter((show) => show.connection === null);
    const starting = shows.filter((show) => show.connection === STARTING);
    const late = shows.filter((show) => show.filledMs === null || show.filledMs > RESHOW_BOUND_MS);
    assert.deepEqual(unreloaded, [], `setup: a show reloaded no page, so the tab was not hidden before it: ${all}`);
    assert.deepEqual(starting, [], `a shown tab stayed at "${STARTING}" while the extension held a document: ${all}`);
    assert.deepEqual(late, [], `a shown tab did not show the pool within ${RESHOW_BOUND_MS} ms: ${all}`);
  });
});

test('flow__close_stops_watch', async () => {
  await withFlow('flow__close_stops_watch', {}, async (session) => {
    // Given
    const { window, main } = session;
    await openStatus(session);
    const running = await poll(() => wtpoolProcesses(main).length === 1 && wtpoolProcesses(main), 5_000);
    const tab = window.locator('.tabs-container .tab').filter({ hasText: /^Worktrees/ }).first();

    // When
    await tab.hover();
    await tab.locator('.tab-actions .action-label').first().click();
    const closed = await poll(async () => statusTabs(await editorGroups(window)).length === 0, 5_000, 50);
    const closedAt = Date.now();
    await sleep(Math.max(0, 1000 - (Date.now() - closedAt)));
    const left = wtpoolProcesses(main);

    // Then
    assert.ok(running, `not exactly one wtpool process while the tab is open: ${JSON.stringify(wtpoolProcesses(main))}`);
    assert.ok(running[0].args.includes('--watch'), `the running wtpool is not a watch: ${running[0].args}`);
    assert.ok(closed, 'the tab did not close');
    assert.deepEqual(left, [], 'a wtpool process still runs 1 s after the tab closed');
  });
});

test('flow__two_windows', async () => {
  await withFlow('flow__two_windows', {}, async (session) => {
    // Given
    const { window, main } = session;
    await openStatus(session);
    const one = await poll(() => watchers().filter((w) => wtpoolProcesses(main).some((p) => p.pid === w.pid)).length === 1, 5_000);

    // When: a second window on the same folder (R9: a window without one runs no wtpool).
    const second = await newWindow(session, session.pool);
    await openStatus(session, second);
    await sleep(2000);
    const both = watchers().filter((w) => wtpoolProcesses(main).some((p) => p.pid === w.pid));
    const firstRows = await rowsWithoutLast(await panelFrame(window, 5_000));
    const secondRows = await rowsWithoutLast(await panelFrame(second, 5_000));

    // Then
    assert.ok(one, 'not exactly one watcher for one window');
    assert.equal(both.length, 2, `${both.length} watchers for two windows`);
    assert.equal(new Set(both.map((w) => w.ppid)).size, 2, 'the two watchers do not run in two extension hosts');
    assert.deepEqual(firstRows, scenarioRows(session), 'the first window does not show the pool');
    assert.deepEqual(secondRows, scenarioRows(session), 'the second window does not show the pool');
  });
});
