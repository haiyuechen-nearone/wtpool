import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { test } from '@playwright/test';

import { runDir } from '../support/paths.mjs';
import { frameWithText, repositoryDocument, repositoryPool, sizeWebview, textLayout } from './folders.mjs';
import { expectedRows, waitForRows } from './rows.mjs';
import { close, env, launch, panelFrame, runCommand, sandbox } from './vscode.mjs';

const OPEN_FOLDER = 'Open a folder inside a git repository to see its worktrees.';
const PAUSED = /updates paused, retrying in \d+ s/;

/** The variables that name a pool to wtpool: none may reach VS Code in these flows. */
const poolVariables = (variables) => Object.keys(variables).filter((name) => name === 'WTPOOL_FOREGROUND' || name.startsWith('WTPOOL_SLOT'));

test('flow__workspace_folder_pool', async () => {
  // Given: the pool of app, opened from its main worktree, then from the slot app-wt2.
  const poolInfo = pool();
  const expected = expectedRows(repositoryDocument(poolInfo), poolInfo);
  const seen = [];

  // When
  for (const folder of [poolInfo.fg, path.join(poolInfo.dir, 'app-wt2')]) {
    const dirs = sandbox(`flow__workspace_folder_pool-${path.basename(folder)}`);
    const session = await launch(dirs, undefined, [], { folder });
    try {
      await runCommand(session.window, 'Worktrees: Open Status');
      const frame = await panelFrame(session.window, 30_000);
      const rows = frame ? await waitForRows(frame, expected, 15_000) : [];
      seen.push({ folder, variables: session.env, frame: Boolean(frame), rows });
    } finally {
      await close(session);
    }
  }

  // Then
  for (const { folder, variables, frame, rows } of seen) {
    const where = path.basename(folder);
    assert.deepEqual(poolVariables(variables), [], 'setup: VS Code got a variable that names the pool');
    assert.ok(variables.HOME.startsWith(runDir()), `setup: HOME ${variables.HOME} is not a folder of the run`);
    assert.ok(frame, `VS Code on ${where} shows no rows: wtpool did not find the pool from the workspace folder`);
    assert.deepEqual(rows, expected, `VS Code on ${where} does not show the pool of app, app its foreground`);
  }
});

/** The pool of the flow: app and app-wt1 to app-wt3, found by wtpool's defaults alone. */
function pool() {
  return repositoryPool(test.info().title.split(' ')[0]);
}

test('flow__no_folder_open', async () => {
  // Given
  const dirs = sandbox('flow__no_folder_open');
  const session = await launch(dirs, undefined, [], { folder: null });
  try {
    // When
    await runCommand(session.window, 'Worktrees: Open Status');
    const found = await frameWithText(session.window, (text) => text.includes(OPEN_FOLDER));
    const layout = found.frame ? await textLayout(found.frame, OPEN_FOLDER) : null;

    // Then
    assert.deepEqual(poolVariables(session.env), [], 'setup: VS Code got a variable that names the pool');
    assert.ok(found.frame, `with no folder open the tab does not say "${OPEN_FOLDER}": ${JSON.stringify(found.texts)}`);
    assert.ok(layout?.shown, `the text "${OPEN_FOLDER}" is not shown: ${JSON.stringify(layout)}`);
  } finally {
    await close(session);
  }
});

/** The last non-empty stderr line of `wtpool status --watch`, run as the extension runs it. */
function wtpoolError(dirs, cwd) {
  const ran = spawnSync(dirs.wtpoolPath, ['status', '--watch'], { cwd, env: env(dirs, undefined), input: '', encoding: 'utf8', timeout: 15_000 });
  assert.ok(ran.status !== 0 && !ran.error, `setup: wtpool status --watch outside a repository exited ${ran.status}: ${ran.error ?? ''}`);
  const line = ran.stderr.split('\n').map((l) => l.trim()).filter(Boolean).at(-1);
  assert.ok(line, 'setup: wtpool status --watch failed with no stderr');
  return line.replace(/\s+/g, ' ');
}

test('flow__wtpool_error_shown', async () => {
  // Given: VS Code on a folder in no git repository. The run folder's repository lies above
  // GIT_CEILING_DIRECTORIES, so wtpool fails.
  const dirs = sandbox('flow__wtpool_error_shown');
  const expected = wtpoolError(dirs, dirs.workspace);
  const session = await launch(dirs, undefined);
  try {
    await runCommand(session.window, 'View: Close Primary Side Bar');
    await runCommand(session.window, 'Worktrees: Open Status');
    const found = await frameWithText(session.window, (text) => PAUSED.test(text));
    assert.ok(found.frame, `the tab never shows the paused header: ${JSON.stringify(found.texts)}`);

    // When: the webview as narrow as the window allows, below the error's width on one line.
    const width = await sizeWebview(session, found.frame, 360);
    const shown = await frameWithText(session.window, (text) => text.includes(expected) && PAUSED.test(text), 10_000);
    const layout = shown.frame ? await textLayout(shown.frame, expected) : null;

    // Then
    assert.ok(shown.frame, `the tab does not show wtpool's error "${expected}" next to the paused state: ${JSON.stringify(shown.texts)}`);
    assert.ok(layout?.shown, `wtpool's error is not shown: ${JSON.stringify(layout)}`);
    assert.ok(width <= 420, `setup: the webview is ${width} px wide, not narrow`);
    assert.deepEqual(layout.cuts, [], `wtpool's error is clipped at ${layout.width} px`);
    assert.deepEqual(layout.outside, [], `wtpool's error runs out of the ${layout.width} px tab`);
    assert.ok(layout.pageScroll <= 1, `the page scrolls sideways by ${layout.pageScroll} px`);
    if (layout.oneLine > layout.width) {
      assert.ok(layout.lines >= 2, `wtpool's error, ${layout.oneLine} px on one line, does not wrap in the ${layout.width} px tab: ${JSON.stringify(layout)}`);
    }
  } finally {
    await close(session);
  }
});
