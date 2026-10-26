import assert from 'node:assert/strict';
import { realpathSync } from 'node:fs';
import { test } from '@playwright/test';

import { clickAction, openStatus, poll, terminalFolder, withFlow, worktreePath } from './flows.mjs';
import { editorGroups, isStatusTab, workbench } from './vscode.mjs';

/** Splits a window's title into its parts. VS Code puts the window's folder name there. */
async function titleParts(window) {
  return (await window.title()).split(/\s+[—–-]\s+/).map((part) => part.trim());
}

test('flow__open_in_new_window', async () => {
  await withFlow('flow__open_in_new_window', {}, async (session) => {
    // Given
    const { app, window } = session;
    const frame = await openStatus(session);

    // When
    const opened = app.waitForEvent('window', { timeout: 30_000 }).catch(() => undefined);
    await clickAction(frame, session, 'wt1', 'open');
    const second = await opened;
    if (second) await workbench(second);
    const parts = second ? await poll(async () => {
      const found = await titleParts(second);
      return found.includes('wt1') && found;
    }, 15_000) : undefined;
    const firstStillShows = statusTabsOf(await editorGroups(window)).length;

    // Then
    assert.ok(second, 'clicking wt1\'s open action opened no new window');
    assert.ok(parts, `the new window does not show wt1: ${second ? await second.title() : ''}`);
    assert.equal(app.windows().length, 2, 'the open action did not keep the first window');
    assert.equal(firstStillShows, 1, 'the first window lost its Worktrees tab');
  });
});

function statusTabsOf(groups) {
  return groups.flatMap((g) => g.tabs.filter((t) => isStatusTab(t.label)));
}

test('flow__open_terminal', async () => {
  await withFlow('flow__open_terminal', {}, async (session) => {
    // Given
    const { window, dirs } = session;
    const frame = await openStatus(session);
    const before = await window.locator('.terminal-wrapper').count();

    // When
    await clickAction(frame, session, 'wt1', 'terminal');
    const folder = await terminalFolder(window, dirs);

    // Then
    assert.equal(before, 0, 'setup: a terminal was open before the click');
    assert.equal(folder, realpathSync(worktreePath(session, 'wt1')), 'the terminal does not start in wt1');
  });
});
