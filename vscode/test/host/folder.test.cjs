// R9 in the extension host: a change of the window's first workspace folder. VS Code restarts the
// extension host for it, unless a debug session, an open custom editor or a running chat request
// vetoes the restart, and it never restarts it while it runs tests. Then the tab itself stops the
// old watch, drops the old document, resets the title to `Worktrees` and watches the new folder;
// with no folder left it runs no wtpool and says so. A change of another folder changes nothing.
// The host window opens from a workspace file (run.mjs), so the checks can change its folders.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vscode = require('vscode');

const { created, ended, openStatus, posted, resetFake, scenarioDocuments, sleep, starts, waitFor } = require('./support.cjs');

/** How long a check waits for a `wtpool` the extension must not start or stop. */
const QUIET_MS = 1500;

async function viewsSince(from) {
  return (await posted()).slice(from).filter((m) => m && m.type === 'view');
}

/** Changes the workspace folders with `update` and waits until VS Code says they changed. */
async function changeFolders(update) {
  let subscription;
  const changed = new Promise((resolve) => {
    subscription = vscode.workspace.onDidChangeWorkspaceFolders(() => resolve(true));
  });
  try {
    assert.ok(update(), 'setup: VS Code refused the workspace folder change');
    const done = await Promise.race([changed, sleep(10_000).then(() => false)]);
    assert.ok(done, 'setup: the workspace folders did not change');
  } finally {
    subscription.dispose();
  }
}

/** Puts back `home` as the window's only folder, for the checks after this one. */
async function restoreFolders(home) {
  const folders = vscode.workspace.workspaceFolders ?? [];
  if (folders.length === 1 && folders[0].uri.toString() === home.toString()) return;
  await changeFolders(() => vscode.workspace.updateWorkspaceFolders(0, folders.length, { uri: home }));
}

describe('first folder', () => {
  it('spawn__should_run_in_the_workspace_folder first_folder_changes', async () => {
    // Given: the tab on a pool with one wait, so its title counts it. Every later start of wtpool
    // prints after 5 s, so the tab shows what it keeps of the old pool before any new document.
    const folders = vscode.workspace.workspaceFolders ?? [];
    assert.ok(vscode.workspace.workspaceFile, 'setup: the host window has no workspace file, so its first folder cannot change');
    assert.equal(folders.length, 1, 'setup: the host window has not one workspace folder');
    const home = folders[0].uri;
    const other = process.env.WTPOOL_VERIFY_OTHER_FOLDER;
    assert.ok(other, 'setup: run.mjs named no other folder');
    fs.mkdirSync(other, { recursive: true });
    const [waiting] = scenarioDocuments('permission_wait_and_answer');
    await resetFake({ documents: [waiting], faults: ['none', 'slow_start'], slowStartMs: 5000, aliveMs: 200 });
    await openStatus();
    const panel = created.filter((c) => c.viewType === 'wtpool.status').at(-1)?.panel;
    const first = await waitFor(() => starts()[0], 10_000);
    const counted = await waitFor(() => panel?.title === 'Worktrees (1)', 10_000);
    const seen = {};

    try {
      // When: another folder joins after the first, then leaves.
      await changeFolders(() => vscode.workspace.updateWorkspaceFolders(1, 0, { uri: vscode.Uri.file(other) }));
      await sleep(QUIET_MS);
      seen.second = { starts: starts().length, ended: Boolean(first && ended(first.pid)), title: panel?.title };
      await changeFolders(() => vscode.workspace.updateWorkspaceFolders(1, 1));

      // When: the other folder replaces the first.
      const fromMove = (await posted()).length;
      await changeFolders(() => vscode.workspace.updateWorkspaceFolders(0, 1, { uri: vscode.Uri.file(other) }));
      seen.title = panel?.title;
      seen.moved = await waitFor(() => starts()[1], 5_000);
      seen.firstEnded = await waitFor(() => first && ended(first.pid), 5_000);
      seen.movedViews = await viewsSince(fromMove);

      // When: no folder is left, then the workspace folder comes back.
      const fromNone = (await posted()).length;
      await changeFolders(() => vscode.workspace.updateWorkspaceFolders(0, 1));
      seen.noFolder = await waitFor(async () => (await viewsSince(fromNone)).find((v) => v.no_folder === true), 5_000);
      seen.movedEnded = await waitFor(() => seen.moved && ended(seen.moved.pid), 5_000);
      await sleep(QUIET_MS);
      seen.startsWithoutFolder = starts().length;
      await changeFolders(() => vscode.workspace.updateWorkspaceFolders(0, 0, { uri: home }));
      seen.back = await waitFor(() => starts()[2], 5_000);
    } finally {
      await restoreFolders(home);
    }

    // Then
    assert.ok(first, 'wtpool status --watch did not start');
    assert.ok(counted, `setup: the title never counted the wait: ${panel?.title}`);
    assert.deepEqual(seen.second, { starts: 1, ended: false, title: 'Worktrees (1)' }, 'a folder after the first changed the watch or the title');
    assert.ok(seen.moved, 'no wtpool started in the new first folder');
    assert.deepEqual(seen.moved.argv, ['status', '--watch'], 'the arguments in the new first folder');
    assert.equal(seen.moved.cwd, fs.realpathSync(other), 'the new watch does not run in the new first folder');
    assert.ok(seen.firstEnded, 'the watch in the old first folder still runs');
    assert.equal(seen.title, 'Worktrees', 'the title kept the old pool\'s count');
    assert.ok(seen.movedViews.length > 0, 'the tab posted nothing when the first folder changed');
    assert.deepEqual(
      seen.movedViews.filter((v) => v.status !== null).map((v) => v.status.generated),
      [],
      'the tab kept the old pool\'s document',
    );
    assert.ok(seen.noFolder, 'with no folder left the tab does not say a folder is needed');
    assert.equal(seen.noFolder.status, null, 'the no folder view kept a document');
    assert.ok(seen.movedEnded, 'the watch still runs with no folder open');
    assert.equal(seen.startsWithoutFolder, 2, 'wtpool started with no folder open');
    assert.ok(seen.back, 'no wtpool started when a folder came back');
    assert.equal(seen.back.cwd, fs.realpathSync(home.fsPath), 'the watch does not run in the folder that came back');
  });
});
