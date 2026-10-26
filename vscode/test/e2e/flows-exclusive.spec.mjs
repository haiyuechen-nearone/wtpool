// The flows that need the machine to themselves (verification.md, "User flows"): copying a path,
// a missing worktree's actions, a narrow window and keyboard only use read the clipboard, each
// its VS Code's private one (support/main-hooks.mjs), and keyboard only use, the narrow scroll
// and live updates keep focus in the webview, which any other VS Code window that opens takes
// away. playwright.exclusive.config.mjs
// runs this file alone, one check after another, after the parallel checks (layers/e2e.rs). Each
// still fails or passes on its own (U1, U3, U4, U5, S3, X14, X15, R6).
import assert from 'node:assert/strict';
import { realpathSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { test } from '@playwright/test';

import { scenarioDocuments } from '../support/docs.mjs';
import { rowOrder } from '../support/grammar.mjs';
import { installHelpers } from '../support/measure.mjs';
import { focusInView, scrollLayout, scrollState, scrollTable } from '../support/scroll.mjs';
import { AgentSteps } from './agents.mjs';
import { expectedRows, resolved } from './rows.mjs';
import {
  clickAction,
  detailOwners,
  openStatus,
  scenarioRows,
  poll,
  rowOf,
  terminalFolder,
  withClipboard,
  withFlow,
  worktreePath,
} from './flows.mjs';
import { waitForRows } from './rows.mjs';
import { git, panelFrame, readClipboard, runCommand, sleep, workbench } from './vscode.mjs';


test('flow__copy_path', async () => {
  await withFlow('flow__copy_path', {}, async (session) => {
    // Given
    const { app } = session;
    const frame = await openStatus(session);
    const wt1 = worktreePath(session, 'wt1');

    // When
    const copied = await withClipboard(app, async () => {
      await clickAction(frame, session, 'wt1', 'copyPath');
      return poll(async () => {
        const text = await readClipboard(app);
        return text === wt1 ? text : undefined;
      }, 3_000).then(async (found) => found ?? readClipboard(app));
    });

    // Then
    assert.equal(copied, wt1, 'the clipboard does not hold wt1\'s path');
  });
});

/** The row actions a row offers: shown and enabled buttons, by data-action. */
async function offered(frame, session, name) {
  const row = rowOf(frame, session, name);
  await row.hover();
  await sleep(200);
  return row.locator('button[data-action]').evaluateAll((buttons) =>
    buttons
      .filter((b) => !b.disabled && b.checkVisibility({ opacityProperty: true, visibilityProperty: true }))
      .map((b) => b.dataset.action),
  );
}

test('flow__missing_worktree_actions', async () => {
  await withFlow('flow__missing_worktree_actions', { scenario: 'folder_missing' }, async (session) => {
    // Given
    const { app } = session;
    const frame = await openStatus(session);
    const wt3 = worktreePath(session, 'wt3');

    // When
    const missing = await offered(frame, session, 'wt3');
    const present = await offered(frame, session, 'wt1');
    const copied = await withClipboard(app, async () => {
      await clickAction(frame, session, 'wt3', 'copyPath');
      return (await poll(async () => ((await readClipboard(app)) === wt3 ? wt3 : undefined), 3_000)) ?? readClipboard(app);
    });

    // Then
    // From phase 3 a slot may also offer free and fg (X12); U4's three come first.
    const u4 = present.filter((action) => !['free', 'fg'].includes(action));
    assert.deepEqual(u4, ['open', 'terminal', 'copyPath'], 'wt1 does not offer the three actions');
    assert.deepEqual(missing, ['copyPath'], 'the missing wt3 does not offer copy path only');
    assert.equal(copied, wt3, 'wt3\'s copy action does not copy its path');
  });
});

const LONG_BRANCH = '1234-check-widget-cache-health-before-each-deploy-and-more-words';

/** Closes the side bar, opens the tab and sizes the window so the webview is 600 px wide. */
async function narrowPanel(session) {
  const { app, window } = session;
  await runCommand(window, 'View: Close Primary Side Bar');
  await runCommand(window, 'Worktrees: Open Status');
  const first = await panelFrame(window);
  assert.ok(first, 'Worktrees: Open Status showed no tab with rows');
  const frameWidth = await first.evaluate(() => window.innerWidth);
  await app.evaluate(({ BrowserWindow }, extra) => {
    const [win] = BrowserWindow.getAllWindows();
    const [width, height] = win.getContentSize();
    win.setContentSize(width + extra, height);
  }, 600 - frameWidth);
  await sleep(1500);
  return panelFrame(window, 5_000);
}

test('flow__narrow_window', async () => {
  await withFlow('flow__narrow_window', {}, async (session) => {
    // Given
    const { app } = session;
    const wt2 = worktreePath(session, 'wt2');
    git(wt2, ['branch', '-m', 'feature-a', LONG_BRANCH]);
    const frame = await narrowPanel(session);

    // When
    const layout = await frame.evaluate((id) => {
      const row = document.querySelector(`tr[data-row-id="${CSS.escape(id)}"]`);
      const cell = row.querySelectorAll(':scope > td')[2];
      const clipped = [cell, ...cell.querySelectorAll('*')].find((el) => el.scrollWidth > el.clientWidth + 1);
      const table = document.querySelector('table');
      // The tab's horizontal scrollers: the page itself, or an ancestor of the table.
      const scrollers = [document.scrollingElement ?? document.documentElement];
      for (let e = table.parentElement; e && e !== document.documentElement; e = e.parentElement) {
        if (['auto', 'scroll'].includes(getComputedStyle(e).overflowX)) scrollers.push(e);
      }
      return {
        width: window.innerWidth,
        table: Math.round(table.getBoundingClientRect().width),
        scrolls: scrollers.some((e) => e.scrollWidth > e.clientWidth + 1),
        text: cell.textContent.replace(/\s+/g, ' ').trim(),
        clipped: clipped ? `${clipped.tagName.toLowerCase()} text-overflow ${getComputedStyle(clipped).textOverflow}` : null,
      };
    }, wt2);
    // Each action, scrolled into view and hovered, shows on top of the page.
    const reach = [];
    for (const name of ['fg', 'wt1', 'wt2', 'wt3', 'x']) {
      const buttons = rowOf(frame, session, name).locator('button[data-action]');
      for (let i = 0; i < (await buttons.count()); i += 1) {
        const button = buttons.nth(i);
        await button.evaluate((b) => b.scrollIntoView({ block: 'nearest', inline: 'nearest' }));
        await button.hover({ force: true });
        await sleep(150);
        reach.push(await button.evaluate((b, rowName) => {
          const box = b.getBoundingClientRect();
          const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
          return {
            key: `${b.dataset.action} ${rowName}`,
            shown: b.checkVisibility({ opacityProperty: true, visibilityProperty: true }) && box.width > 0,
            inside: box.left >= 0 && box.right <= window.innerWidth + 0.5,
            onTop: Boolean(hit && (hit === b || b.contains(hit))),
          };
        }, name));
      }
    }
    const copied = await withClipboard(app, async () => {
      await clickAction(frame, session, 'wt2', 'copyPath');
      return poll(async () => ((await readClipboard(app)) === wt2 ? wt2 : undefined), 3_000);
    });

    // Then
    assert.ok(Math.abs(layout.width - 600) <= 2, `setup: the webview is ${layout.width} px wide, not 600`);
    // U1 (status-panel.md revision 14): the branch shows whole and the table scrolls sideways.
    assert.equal(layout.text, LONG_BRANCH, `the long branch is not shown whole at 600 px: ${JSON.stringify(layout)}`);
    assert.equal(layout.clipped, null, `the long branch is cut at 600 px: ${JSON.stringify(layout)}`);
    assert.ok(layout.table > layout.width && layout.scrolls, `the ${layout.table} px table does not scroll sideways in the 600 px tab: ${JSON.stringify(layout)}`);
    // U4's three actions on each of five rows, plus from phase 3 free and fg on wt2, the slot
    // that holds a branch (X12); free and fg elsewhere only on the other slots.
    const keys = reach.map((r) => r.key);
    const wanted = ['fg', 'wt1', 'wt2', 'wt3', 'x'].flatMap((name) => ['open', 'terminal', 'copyPath'].map((a) => `${a} ${name}`));
    wanted.push('free wt2', 'fg wt2');
    assert.deepEqual(wanted.filter((key) => !keys.includes(key)), [], `actions missing at 600 px; found ${JSON.stringify(keys)}`);
    const unexpected = keys.filter((key) => !wanted.includes(key) && !/^(free|fg) wt[13]$/.test(key));
    assert.deepEqual(unexpected, [], 'row actions outside U4 and X12 at 600 px');
    assert.deepEqual(reach.filter((r) => !r.shown || !r.inside || !r.onTop).map((r) => r.key), [], `actions out of reach at 600 px, scrolled into view: ${JSON.stringify(reach)}`);
    assert.equal(copied, wt2, 'wt2\'s copy action does not work at 600 px');
  });
});

const WAIT_SCENARIO = 'permission_wait_and_answer';

test('flow__narrow_scroll', async () => {
  await withFlow('flow__narrow_scroll', { scenario: WAIT_SCENARIO }, async (session) => {
    // Given: a 600 px tab while an agent waits for permission in wt2.
    const { window } = session;
    await window.mouse.move(1, 1);
    const frame = await narrowPanel(session);
    const { scenario, documents } = scenarioDocuments(WAIT_SCENARIO);
    const steps = new AgentSteps(session, WAIT_SCENARIO, scenario);
    await steps.run(0, scenario.steps.findIndex((step) => step.expect), async () => {});
    let state;
    let start;
    let end;
    const stops = [];
    let chip;
    try {
      const waiting = expectedRows(documents[0], session.pool);
      assert.deepEqual(await waitForRows(frame, waiting, 3_000), waiting, 'setup: wt2 does not show waiting');
      await frame.evaluate(installHelpers);

      // When
      state = await frame.evaluate(scrollState);
      start = await frame.evaluate(scrollLayout);
      await frame.evaluate(scrollTable, true);
      await sleep(200);
      end = await frame.evaluate(scrollLayout);
      await frame.evaluate(scrollTable, false);
      for (let step = 0; step < 3 && !(await frame.evaluate(() => document.hasFocus())); step += 1) {
        await runCommand(window, 'View: Focus Next Editor Group', { keyboardOnly: true });
      }
      const seen = new Set();
      for (let step = 0; step < 80; step += 1) {
        await window.keyboard.press('Tab');
        await sleep(50);
        const f = await frame.evaluate(focusInView);
        if (!f) {
          if (seen.size) break;
          continue;
        }
        if (seen.has(f.key)) break;
        seen.add(f.key);
        stops.push(f);
      }
      await frame.evaluate(() => document.activeElement?.blur?.());
      await frame.evaluate(scrollTable, true);
      await frame.locator('.chip').filter({ hasText: /waiting/ }).first().click();
      chip = await poll(async () => {
        const f = await frame.evaluate(focusInView);
        return f?.key === 'row wt2' && f.shown ? f : undefined;
      }, 3_000) ?? (await frame.evaluate(focusInView));
    } finally {
      await steps.stop();
    }

    // Then
    assert.deepEqual(steps.problems, [], 'the hooks of the wait');
    // U1 (status-panel.md revision 14): the table scrolls inside the tab, the page does not.
    assert.ok(state.table > state.tab, `the ${state.table} px table fits the ${state.tab} px tab: its columns are squeezed`);
    assert.ok(state.page <= 1, `the page itself scrolls sideways by ${state.page} px`);
    assert.ok(state.scrolls, `no box around the table scrolls it sideways: ${JSON.stringify(state.scrollers)}`);
    const away = (c) => c.left < end.view.left - 0.5 || c.right > end.view.right + 0.5;
    assert.deepEqual(end.cells.filter(away).map((c) => c.cell), [], `cells out of view with the table scrolled to its end: ${JSON.stringify(end.view)}`);
    const moved = start.header.filter((h, i) => !end.header[i] || Math.abs(end.header[i].left - h.left) > 0.5 || h.left < -0.5 || h.right > state.tab + 0.5);
    assert.deepEqual(moved.map((h) => h.part), [], 'header parts that move or leave the view when the table scrolls');
    assert.ok(stops.some((f) => !f.isRow) && stops.some((f) => f.isRow), `setup: Tab reached ${stops.map((f) => f.key).join(', ')}`);
    assert.deepEqual(stops.filter((f) => !f.shown).map((f) => `${f.key} ${JSON.stringify(f)}`), [], 'focused by Tab but not in view: a row without its start, an action without its whole ring');
    // U3: the waiting chip focuses wt2 and scrolls its start into view.
    assert.equal(chip?.key, 'row wt2', 'the waiting chip does not focus wt2, the first waiting row');
    assert.ok(chip.shown, `the waiting chip focuses wt2 without its start in view: ${JSON.stringify(chip)}`);
  });
});

/** What has focus in the page: a row or action key, whether its focus shows, and whether it is shown. */
async function focus(frame) {
  return frame.evaluate(() => {
    const el = document.activeElement;
    if (!el || el === document.body) return null;
    const style = getComputedStyle(el);
    return {
      key: el.matches('tr[data-row-id]')
        ? `row ${el.dataset.rowId}`
        : el.matches('button[data-action]')
          ? `${el.dataset.action} ${el.closest('tr[data-row-id]')?.dataset.rowId}`
          : `${el.tagName.toLowerCase()} ${el.className}`,
      visibleFocus: (style.outlineStyle !== 'none' && parseFloat(style.outlineWidth) > 0) || style.boxShadow !== 'none',
      shown: el.checkVisibility({ opacityProperty: true, visibilityProperty: true }),
    };
  });
}

/** Presses Tab, then Shift+Tab, until `key` has focus; whether it got there. */
async function tabTo(window, frame, key) {
  for (const press of ['Tab', 'Shift+Tab']) {
    for (let step = 0; step < 60; step += 1) {
      if ((await focus(frame))?.key === key) return true;
      await window.keyboard.press(press);
    }
  }
  return (await focus(frame))?.key === key;
}

test('flow__keyboard_only', async () => {
  await withFlow('flow__keyboard_only', { scenario: 'folder_missing' }, async (session) => {
    // Given
    const { app, window, dirs } = session;
    await window.mouse.move(1, 1);
    const frame = await openStatus(session);
    for (let step = 0; step < 3 && !(await frame.evaluate(() => document.hasFocus())); step += 1) {
      await runCommand(window, 'View: Focus Next Editor Group', { keyboardOnly: true });
    }
    const status = resolved(scenarioDocuments(session.scenario).documents.at(-1), session.pool);
    const expected = rowOrder(status.pools[0].worktrees).flatMap((w) => [
      `row ${w.path}`,
      ...(w.missing ? ['copyPath'] : ['open', 'terminal', 'copyPath']).map((a) => `${a} ${w.path}`),
    ]);

    // When
    const reached = [];
    for (let step = 0; step < 40; step += 1) {
      await window.keyboard.press('Tab');
      const found = await focus(frame);
      if (found) reached.push(found);
    }
    const [wt1, wt2] = ['wt1', 'wt2'].map((name) => worktreePath(session, name));
    const atRow = await tabTo(window, frame, `row ${wt1}`);
    await window.keyboard.press('Enter');
    const selected = await poll(async () => (await detailOwners(frame)).join() === wt1, 3_000);
    const atCopy = await tabTo(window, frame, `copyPath ${wt2}`);
    const copied = await withClipboard(app, async () => {
      await window.keyboard.press('Enter');
      return (await poll(async () => ((await readClipboard(app)) === wt2 ? wt2 : undefined), 3_000)) ?? readClipboard(app);
    });
    const atOpen = await tabTo(window, frame, `open ${wt1}`);
    const opened = app.waitForEvent('window', { timeout: 30_000 }).catch(() => undefined);
    await window.keyboard.press('Enter');
    const second = await opened;
    if (second) {
      // Cleanup, not part of the flow: the window the open action made goes away.
      await workbench(second);
      await second.close();
      await sleep(1000);
    }
    const atTerminal = await tabTo(window, frame, `terminal ${wt1}`);
    let folder;
    if (atTerminal) {
      await window.keyboard.press('Enter');
      folder = await terminalFolder(window, dirs, { keyboard: true });
    }

    // Then
    const keys = new Set(reached.map((r) => r.key));
    assert.deepEqual(expected.filter((key) => !keys.has(key)), [], 'rows and actions Tab does not reach');
    assert.deepEqual([...new Set(reached.filter((r) => !r.visibleFocus).map((r) => r.key))], [], 'focus without a visible outline');
    assert.deepEqual([...new Set(reached.filter((r) => !r.shown).map((r) => r.key))], [], 'focused elements that are not shown');
    assert.ok(atRow && selected, 'Enter on wt1\'s row does not select it');
    assert.ok(atCopy, 'Tab does not reach wt2\'s copy action');
    assert.equal(copied, wt2, 'Enter on wt2\'s copy action does not copy its path');
    assert.ok(atOpen && second, 'Enter on wt1\'s open action opens no window');
    assert.ok(atTerminal, 'Tab does not reach wt1\'s terminal action');
    assert.equal(folder, realpathSync(wt1), 'Enter on wt1\'s terminal action opens no terminal in wt1');
  });
});

/** What the page's focused element is: `row <id>`, `<action> <id>`, or a tag. */
async function focused(frame) {
  return frame.evaluate(() => {
    const el = document.activeElement;
    if (!el || el === document.body) return null;
    if (el.matches('tr[data-row-id]')) return `row ${el.dataset.rowId}`;
    if (el.matches('button[data-action]')) return `${el.dataset.action} ${el.closest('tr[data-row-id]')?.dataset.rowId}`;
    return el.tagName.toLowerCase();
  });
}

test('flow__live_update_keeps_place', async () => {
  await withFlow('flow__live_update_keeps_place', {}, async (session) => {
    // Given
    const { window } = session;
    const frame = await openStatus(session);
    const [wt1, wt2, wt3] = ['wt1', 'wt2', 'wt3'].map((name) => worktreePath(session, name));
    await rowOf(frame, session, 'wt2').click();
    const target = `copyPath ${wt3}`;
    for (let step = 0; step < 40 && (await focused(frame)) !== target; step += 1) {
      await window.keyboard.press('Tab');
    }
    const focusBefore = await focused(frame);
    const ownersBefore = await detailOwners(frame);
    const expected = scenarioRows(session).map((row) =>
      row.id === wt1 ? { ...row, activity: 'not reusable', git: '1 unreferenced' } : row,
    );

    // When
    writeFileSync(path.join(wt1, 'change.txt'), 'wtpool-verify\n');
    git(wt1, ['add', 'change.txt']);
    git(wt1, ['commit', '--quiet', '-m', 'wtpool-verify: a commit on the detached slot']);
    const rows = await waitForRows(frame, expected, 5_000);
    await sleep(500);

    // Then
    assert.equal(focusBefore, target, 'Tab did not reach wt3\'s copy button');
    assert.deepEqual(ownersBefore, [wt2], 'clicking wt2 did not select it');
    assert.deepEqual(rows, expected, 'wt1 did not update within 5 s of the commit');
    assert.equal(await focused(frame), target, 'focus left wt3\'s copy button when wt1 updated');
    assert.deepEqual(await detailOwners(frame), [wt2], 'the selection or its detail strip moved when wt1 updated');
  });
});
