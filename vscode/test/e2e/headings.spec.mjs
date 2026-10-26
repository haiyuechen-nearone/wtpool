import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { buttonText } from '../support/jobs.mjs';
import { PHASE, writeMachineToml } from './jobs.mjs';
import { panelFrame, close, git, launch, pool, runCommand, sandbox, statusJson } from './vscode.mjs';

const GITHUB = 'https://github.com/acme/app.git';

/** The shown headings of the tab, each with its text and, for a pool heading, its `data-pool`. */
async function headings(frame) {
  return frame.locator('tr[data-machine]').evaluateAll((rows) =>
    rows
      .filter((row) => row.checkVisibility({ opacityProperty: true, visibilityProperty: true }))
      .map((row) => [
        (row.textContent ?? '').replace(/\s+/g, ' ').trim(),
        row.matches('tr.pool-heading[data-machine][data-pool]') ? row.getAttribute('data-pool') : null,
      ]),
  );
}

/** Waits up to 15 s for the headings to read `expected`, and gives the last seen. */
async function waitForHeadings(frame, expected) {
  const deadline = Date.now() + 15_000;
  let seen = [];
  while (Date.now() < deadline) {
    seen = await headings(frame).catch(() => []);
    if (JSON.stringify(seen) === JSON.stringify(expected)) break;
    await frame.page().waitForTimeout(250);
  }
  return seen;
}

/** Remotes a check set outside the run folder, put back even when the check timed out. */
const restore = [];
test.afterEach(() => {
  for (const undo of restore.splice(0)) undo();
});

test('heading__should_name_the_repository_then_the_machine e2e', async () => {
  // Given: the pool's origin on GitHub. isolation__left_no_trace fails on a remote outside the
  // run folder, so the local one comes back after.
  const poolInfo = pool('idle_pool', 'headings');
  const local = git(poolInfo.fg, ['remote', 'get-url', 'origin']).trim();
  restore.push(() => git(poolInfo.fg, ['remote', 'set-url', 'origin', local]));
  git(poolInfo.fg, ['remote', 'set-url', 'origin', GITHUB]);
  const dirs = sandbox('headings');
  // Heavy processes of the whole machine come and go, and `off` leaves their count out of the button.
  writeMachineToml(dirs, { enforce: 'off' });
  const { machine: status } = statusJson(dirs, poolInfo);
  const machine = buttonText(PHASE >= 4 ? status.jobs : undefined, status.name);
  const session = await launch(dirs, poolInfo);
  let onGithub;
  let onLocal;
  try {
    // When: the tab on the GitHub remote, then, closed and opened again, on the local one.
    await runCommand(session.window, 'Worktrees: Open Status');
    const first = await panelFrame(session.window);
    assert.ok(first, 'the Worktrees tab shows no rows');
    onGithub = await waitForHeadings(first, [[`acme/app · ${machine}`, poolInfo.common]]);
    await runCommand(session.window, 'View: Close All Editors');
    git(poolInfo.fg, ['remote', 'set-url', 'origin', local]);
    await session.window.waitForTimeout(1500);
    await runCommand(session.window, 'Worktrees: Open Status');
    const second = await panelFrame(session.window);
    assert.ok(second, 'the Worktrees tab opened again shows no rows');
    onLocal = await waitForHeadings(second, [[`fg · ${machine}`, poolInfo.common]]);
  } finally {
    await close(session);
  }

  // Then
  assert.deepEqual(onGithub, [[`acme/app · ${machine}`, poolInfo.common]], `the heading and its data-pool with origin ${GITHUB}`);
  assert.deepEqual(onLocal, [[`fg · ${machine}`, poolInfo.common]], 'the heading with a local origin: the main worktree folder name');
});
