// X2, X8, X16 and X17 in a real VS Code. The scenario catalog runs in flow__every_activity_shows.
import assert from 'node:assert/strict';
import path from 'node:path';
import { test } from '@playwright/test';

import { scenarioDocuments } from '../support/docs.mjs';
import { listening } from './sockets.mjs';
import { close, descendants, launch, newWindow, panelFrame, pool, runCommand, sandbox, watchers } from './vscode.mjs';

test('e2e_csp_clean', async () => {
  // Given
  const poolInfo = pool('idle_pool', 'csp');
  const dirs = sandbox('csp');
  const session = await launch(dirs, poolInfo);
  const { window } = session;
  const messages = [];
  window.on('console', (message) => messages.push(message.text()));
  try {
    await runCommand(window, 'Worktrees: Open Status');
    const frame = await panelFrame(window);
    assert.ok(frame, 'the Worktrees tab shows no rows');
    await frame.evaluate(() => {
      window.__violations = [];
      document.addEventListener('securitypolicyviolation', (event) =>
        window.__violations.push(`${event.violatedDirective} ${event.blockedURI}`),
      );
    });

    // When
    for (const row of await frame.locator('tr[data-row-id]').all()) await row.click();
    await frame.locator('tr[data-row-id]').first().focus();
    for (let step = 0; step < 20; step += 1) await window.keyboard.press('Tab');
    await window.waitForTimeout(3000);
    const violations = await frame.evaluate(() => window.__violations);
    const styled = await frame.evaluate(() => [...document.querySelectorAll('body [style], body[style]')].length);

    // Then
    assert.deepEqual(violations, [], 'CSP violations in the webview');
    assert.deepEqual(messages.filter((m) => /Content Security Policy|Trusted Type|Refused to/i.test(m)), [], 'CSP messages');
    assert.equal(styled, 0, 'inline style attributes in the webview');
  } finally {
    await close(session);
  }
});

test('e2e_no_listening_sockets', async () => {
  // Given
  const poolInfo = pool('idle_pool', 'sockets');
  const dirs = sandbox('sockets');
  const session = await launch(dirs, poolInfo);
  const { app, window } = session;
  try {
    const main = app.process().pid;
    await window.waitForTimeout(3000);
    const before = new Set(listening([main, ...descendants(main)]));

    // When
    await runCommand(window, 'Worktrees: Open Status');
    const frame = await panelFrame(window);
    await window.waitForTimeout(3000);
    const wtpool = watchers().filter((w) => descendants(main).includes(w.pid));
    const after = listening([main, ...descendants(main)]);

    // Then
    assert.ok(frame, 'the Worktrees tab shows no rows');
    assert.equal(wtpool.length, 1, `${wtpool.length} wtpool status --watch processes`);
    assert.deepEqual(listening(wtpool.map((w) => w.pid)), [], 'wtpool listens on a socket');
    assert.deepEqual(after.filter((line) => !before.has(line)), [], 'opening the tab added a listening socket');
  } finally {
    await close(session);
  }
});

test('windows__should_run_one_watcher_each', async () => {
  // Given
  const poolInfo = pool('idle_pool', 'windows');
  const dirs = sandbox('windows');
  const session = await launch(dirs, poolInfo);
  const { app, window } = session;
  try {
    const main = app.process().pid;
    await runCommand(window, 'Worktrees: Open Status');
    assert.ok(await panelFrame(window), 'the first window shows no rows');

    // When: a second window on the same folder (R9: a window without one runs no wtpool).
    const second = await newWindow(session, poolInfo);
    await runCommand(second, 'Worktrees: Open Status');
    const secondFrame = await panelFrame(second);
    await second.waitForTimeout(2000);
    const both = watchers().filter((w) => descendants(main).includes(w.pid));
    await runCommand(window, 'View: Close All Editors');
    await window.waitForTimeout(2000);
    const afterClose = watchers().filter((w) => descendants(main).includes(w.pid));

    // Then
    assert.ok(secondFrame, 'the second window shows no rows');
    assert.equal(both.length, 2, `${both.length} watchers for two windows`);
    assert.equal(new Set(both.map((w) => w.ppid)).size, 2, 'two windows share one watcher process parent');
    assert.equal(afterClose.length, 1, 'closing one window\'s tab did not leave exactly the other watcher');
  } finally {
    await close(session);
  }
});

test('restore__should_rebuild_after_reload', async () => {
  // Given
  const poolInfo = pool('idle_pool', 'restore');
  const dirs = sandbox('restore');
  const session = await launch(dirs, poolInfo);
  const { window } = session;
  try {
    await runCommand(window, 'Worktrees: Open Status');
    const frame = await panelFrame(window);
    assert.ok(frame, 'the Worktrees tab shows no rows');
    const target = path.join(poolInfo.dir, 'wt2');
    await frame.locator(`tr[data-row-id="${target}"]`).click();
    await frame.locator('tr.detail').waitFor({ timeout: 5000 });

    // When
    await runCommand(window, 'Developer: Reload Window');
    await window.waitForTimeout(3000);
    await window.locator('.monaco-workbench').waitFor({ timeout: 60_000 });
    const restored = await panelFrame(window, 60_000);

    // Then
    assert.ok(restored, 'the Worktrees tab did not come back after a reload');
    const rows = await restored.locator('tr[data-row-id]').count();
    assert.equal(rows, scenarioDocuments('idle_pool').documents.at(-1).pools[0].worktrees.length);
    const detailAfter = await restored.locator(`tr[data-row-id="${target}"] + tr.detail`).count();
    assert.equal(detailAfter, 1, 'the selected row was not restored');
  } finally {
    await close(session);
  }
});
