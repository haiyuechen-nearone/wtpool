// The tab in the extension host, with the fake wtpool. Confirmed actions are in actions.test.cjs.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vscode = require('vscode');

const {
  control,
  created,
  expectedDocuments,
  fakeLog,
  openStatus,
  openStatusTab,
  posted,
  resetFake,
  serializers,
  sleep,
  starts,
  statusTabs,
  verifyApi,
  views,
  waitFor,
  wrap,
  wrapClipboard,
} = require('./support.cjs');

/** The calls that show a message, or with `{modal: true}` a modal dialog. */
const MESSAGE_KINDS = ['showInformationMessage', 'showWarningMessage', 'showErrorMessage'];
const SHELLS = ['sh', 'bash', 'zsh', 'dash', 'fish', 'ksh', 'tcsh', 'csh'];

/** The Worktrees tab once it is the active tab of the active group. */
function shownStatusTab() {
  const found = openStatusTab();
  return found?.tab.isActive && found.group === vscode.window.tabGroups.activeTabGroup ? found : undefined;
}

async function textEditor(column) {
  const document = await vscode.workspace.openTextDocument({ content: 'wtpool-verify' });
  return vscode.window.showTextDocument(document, { viewColumn: column, preview: false });
}

describe('tab', () => {
  it('open__should_reveal_or_create_beside', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool' });
    // An extension that does not activate fails here on an assertion, not on a missing command.
    await verifyApi();
    await textEditor(vscode.ViewColumn.One);

    // When
    await vscode.commands.executeCommand('wtpool.openStatus');
    const opened = await waitFor(openStatusTab, 10_000);
    await textEditor(vscode.ViewColumn.One);
    await vscode.commands.executeCommand('wtpool.openStatus');
    const revealedFromOtherGroup = (await waitFor(shownStatusTab, 10_000)) ?? openStatusTab();
    await textEditor(opened?.group.viewColumn);
    await vscode.commands.executeCommand('wtpool.openStatus');
    const revealedInItsGroup = (await waitFor(shownStatusTab, 10_000)) ?? openStatusTab();

    // Then
    assert.ok(opened, 'Worktrees: Open Status opened no tab');
    assert.equal(opened.group.viewColumn, vscode.ViewColumn.Two, 'the tab opens beside the active group');
    assert.equal(statusTabs().length, 1, 'a second command opened a second tab');
    assert.ok(revealedFromOtherGroup.tab.isActive, 'the second command did not reveal the tab');
    assert.equal(vscode.window.tabGroups.activeTabGroup, revealedFromOtherGroup.group, 'the revealed tab has no focus');
    assert.ok(revealedInItsGroup.tab.isActive, 'a tab hidden behind an editor was not revealed');
    assert.equal(revealedInItsGroup.group.viewColumn, opened.group.viewColumn, 'the tab moved to another group');
  });

  it('options__should_limit_roots_and_disable_commands_and_forms', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool' });
    const before = created.length;
    const extension = vscode.extensions.getExtension('haiyuechen-nearone.wtpool');

    // When
    await openStatus();
    const made = created.slice(before).filter((c) => c.viewType === 'wtpool.status');
    const registered = serializers.find((s) => s.viewType === 'wtpool.status');
    const restored = vscode.window.createWebviewPanel('wtpool.status', 'Worktrees', vscode.ViewColumn.Three, {
      enableScripts: true,
      enableCommandUris: true,
      enableForms: true,
      localResourceRoots: [extension.extensionUri],
    });
    if (registered) await registered.serializer.deserializeWebviewPanel(restored, { selected: null });
    const restoredOptions = restored.webview.options;
    restored.dispose();

    // Then
    assert.equal(made.length, 1, 'the tab was not made by createWebviewPanel("wtpool.status", ...)');
    const { options } = made[0];
    const media = vscode.Uri.joinPath(extension.extensionUri, 'media').fsPath;
    assert.equal(options.enableScripts, true, 'enableScripts');
    assert.equal(options.enableCommandUris, false, 'enableCommandUris must be false');
    assert.equal(options.enableForms, false, 'enableForms must be false');
    assert.deepEqual((options.localResourceRoots ?? []).map((u) => u.fsPath), [media], 'localResourceRoots');
    assert.notEqual(options.retainContextWhenHidden, true, 'retainContextWhenHidden must stay off');
    assert.ok(registered, 'no WebviewPanelSerializer for wtpool.status');
    assert.equal(restoredOptions.enableCommandUris, false, 'a restored tab keeps command URIs on');
    assert.equal(restoredOptions.enableForms, false, 'a restored tab keeps forms on');
    assert.deepEqual((restoredOptions.localResourceRoots ?? []).map((u) => u.fsPath), [media], 'a restored tab keeps wider roots');
    const manifest = extension.packageJSON;
    assert.ok(manifest.activationEvents.includes('onWebviewPanel:wtpool.status'), 'no onWebviewPanel:wtpool.status');
  });

  // The tab shows the extension's own icon, a light and a dark variant, also when the serializer
  // restores it. Each must be a different SVG file inside the extension's folder.
  it('tab__should_show_the_extension_icon', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool' });
    const before = created.length;
    const extension = vscode.extensions.getExtension('haiyuechen-nearone.wtpool');
    const root = extension.extensionUri.fsPath;

    // When
    await openStatus();
    const opened = created.slice(before).filter((c) => c.viewType === 'wtpool.status').at(-1)
      ?? created.filter((c) => c.viewType === 'wtpool.status').at(-1);
    const openedIcon = opened?.panel.iconPath;
    const registered = serializers.find((s) => s.viewType === 'wtpool.status');
    const restored = vscode.window.createWebviewPanel('wtpool.status', 'Worktrees', vscode.ViewColumn.Three, {
      enableScripts: true,
      localResourceRoots: [vscode.Uri.joinPath(extension.extensionUri, 'media')],
    });
    if (registered) await registered.serializer.deserializeWebviewPanel(restored, { selected: null });
    const restoredIcon = restored.iconPath;
    restored.dispose();

    // Then
    assert.ok(opened, 'setup: no wtpool.status panel was made');
    assert.ok(registered, 'no WebviewPanelSerializer for wtpool.status');
    const problems = [];
    for (const [which, icon] of [['the opened tab', openedIcon], ['a restored tab', restoredIcon]]) {
      if (!icon || !icon.light || !icon.dark) {
        problems.push(`${which} has no light and dark iconPath: ${JSON.stringify(icon)}`);
        continue;
      }
      if (icon.light.fsPath === icon.dark.fsPath) problems.push(`${which} uses one file for light and dark: ${icon.light.fsPath}`);
      for (const [variant, uri] of [['light', icon.light], ['dark', icon.dark]]) {
        const file = uri.fsPath;
        const inside = path.relative(root, file);
        if (uri.scheme !== 'file' || inside.startsWith('..') || path.isAbsolute(inside)) {
          problems.push(`${which}: the ${variant} icon ${uri.toString()} is not a file inside the extension`);
        } else if (!fs.existsSync(file) || !fs.statSync(file).isFile()) {
          problems.push(`${which}: the ${variant} icon ${inside} does not exist`);
        } else if (path.extname(file) !== '.svg' || !/<svg[\s>]/.test(fs.readFileSync(file, 'utf8'))) {
          problems.push(`${which}: the ${variant} icon ${inside} is no SVG file`);
        }
      }
    }
    assert.deepEqual(problems, [], 'the Worktrees tab does not show the extension\'s icon');
  });

  it('spawn__should_use_the_setting_and_no_shell', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool' });
    const fakes = process.env.WTPOOL_VERIFY_HOST_FAKES;
    const odd = path.join(fakes, 'dir with space;$(touch pwned)', 'fake wtpool`id`.mjs');
    fs.mkdirSync(path.dirname(odd), { recursive: true });
    fs.copyFileSync(path.join(__dirname, '..', 'fake-wtpool.mjs'), odd);
    fs.chmodSync(odd, 0o755);
    const configuration = vscode.workspace.getConfiguration('wtpool');
    const previous = configuration.inspect('path').globalValue;
    await configuration.update('path', odd, vscode.ConfigurationTarget.Global);

    // When
    try {
      await openStatus();
      await waitFor(() => starts().length >= 1, 10_000);
      await sleep(1000);
    } finally {
      await vscode.workspace.getConfiguration('wtpool').update('path', previous, vscode.ConfigurationTarget.Global);
    }
    // inspect() hides a workspace value of an application scoped setting, so read the file.
    const folder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? '';
    const workspaceSettings = path.join(folder, '.vscode', 'settings.json');
    const workspaceValue = fs.existsSync(workspaceSettings) ? JSON.parse(fs.readFileSync(workspaceSettings, 'utf8'))['wtpool.path'] : undefined;

    // Then
    const all = starts();
    assert.equal(all.length, 1, `${all.length} starts of wtpool`);
    assert.equal(fs.realpathSync(all[0].script), fs.realpathSync(odd), 'wtpool.path did not name the binary that ran');
    assert.deepEqual(all[0].argv, ['status', '--watch'], 'the arguments');
    const parent = path.basename(all[0].parent).replace(/^-/, '');
    assert.ok(!SHELLS.includes(parent), `wtpool ran under a shell: ${all[0].parent}`);
    assert.ok(!fs.existsSync(path.join(fakes, 'pwned')), 'a shell expanded the path');
    assert.ok(!fs.existsSync(path.join(process.env.HOME, 'pwned')), 'a shell expanded the path');
    assert.equal(workspaceValue, process.env.WTPOOL_VERIFY_WORKSPACE_FAKE, 'setup: the workspace sets wtpool.path');
  });

  it('spawn__should_use_the_setting_and_no_shell workspace_value_ignored', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool' });

    // When
    await openStatus();
    await waitFor(() => starts().length >= 1, 10_000);

    // Then
    const scripts = starts().map((s) => fs.realpathSync(s.script));
    assert.ok(scripts.length >= 1, 'wtpool did not start');
    assert.ok(
      !scripts.includes(fs.realpathSync(process.env.WTPOOL_VERIFY_WORKSPACE_FAKE)),
      'the workspace value of the application scoped wtpool.path ran',
    );
  });

  it('spawn__should_use_the_setting_and_no_shell bare_name_from_absolute_path', async () => {
    // Given: a bare wtpool.path and a PATH whose first entry is relative, then absolute folders
    // with a file of that name that may not run, a folder of that name, and the fake. The working
    // folder, the workspace folder, holds a fake of that name, and one of a name on no entry.
    await resetFake({ scenario: 'idle_pool' });
    const fakes = process.env.WTPOOL_VERIFY_HOST_FAKES;
    const fake = path.join(__dirname, '..', 'fake-wtpool.mjs');
    const name = 'wtpool-bare.mjs';
    const nowhere = 'wtpool-nowhere.mjs';
    const notRunnable = path.join(fakes, 'bare-not-runnable');
    const folders = path.join(fakes, 'bare-folders');
    const bin = path.join(fakes, 'bare-bin');
    fs.mkdirSync(notRunnable, { recursive: true });
    fs.mkdirSync(path.join(folders, name), { recursive: true });
    fs.mkdirSync(bin, { recursive: true });
    fs.copyFileSync(fake, path.join(notRunnable, name));
    fs.chmodSync(path.join(notRunnable, name), 0o644);
    fs.copyFileSync(fake, path.join(bin, name));
    fs.chmodSync(path.join(bin, name), 0o755);
    const workspace = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    assert.ok(workspace, 'setup: the host runs without a workspace folder');
    const planted = [path.join(workspace, name), path.join(workspace, nowhere)];
    for (const file of planted) {
      fs.copyFileSync(fake, file);
      fs.chmodSync(file, 0o755);
    }
    const previous = vscode.workspace.getConfiguration('wtpool').inspect('path').globalValue;
    const previousPath = process.env.PATH;

    // When
    let found;
    let missing;
    let missingStarts;
    try {
      process.env.PATH = ['.', notRunnable, folders, bin, previousPath].join(path.delimiter);
      await vscode.workspace.getConfiguration('wtpool').update('path', name, vscode.ConfigurationTarget.Global);
      await openStatus();
      found = await waitFor(() => starts()[0], 10_000);
      await resetFake({ scenario: 'idle_pool' });
      await vscode.workspace.getConfiguration('wtpool').update('path', nowhere, vscode.ConfigurationTarget.Global);
      const from = (await views()).length;
      await openStatus();
      missing = await waitFor(async () => (await views()).slice(from).find((v) => v.connection === 'paused'), 5_000);
      await sleep(500);
      missingStarts = starts();
    } finally {
      process.env.PATH = previousPath;
      await vscode.workspace.getConfiguration('wtpool').update('path', previous, vscode.ConfigurationTarget.Global);
      for (const file of planted) fs.rmSync(file, { force: true });
    }

    // Then
    assert.ok(found, 'a bare wtpool.path on an absolute PATH entry did not start');
    assert.equal(
      fs.realpathSync(found.script),
      fs.realpathSync(path.join(bin, name)),
      'a bare wtpool.path ran another file than the first runnable one in an absolute PATH entry',
    );
    assert.ok(missing, 'a bare wtpool.path on no absolute PATH entry posted no paused view');
    assert.equal(missing.error, `wtpool did not start: spawn ${nowhere} ENOENT`, 'the paused view of a bare name on no absolute PATH entry');
    assert.deepEqual(missingStarts, [], 'a bare wtpool.path ran the file of that name in the workspace folder');
  });

  it('spawn__should_use_the_setting_and_no_shell pathext_fallback', async () => {
    // Given: a bare wtpool.path whose file exists only with a PATHEXT extension appended, in an
    // absolute PATH entry, as a Windows search finds it. The platform is told it is Windows for
    // the run, and PATHEXT is unset, so the file's own extension is not tried.
    await resetFake({ scenario: 'idle_pool' });
    const fakes = process.env.WTPOOL_VERIFY_HOST_FAKES;
    const fake = path.join(__dirname, '..', 'fake-wtpool.mjs');
    const name = 'wtpool-pathext';
    const bin = path.join(fakes, 'pathext-bin');
    fs.mkdirSync(bin, { recursive: true });
    // Node refuses a `.mjs` under another extension, so the found file runs the fake through node.
    fs.writeFileSync(
      path.join(bin, `${name}.EXE`),
      `#!/bin/sh\nexec ${JSON.stringify(process.execPath)} ${JSON.stringify(fake)} "$@"\n`,
    );
    fs.chmodSync(path.join(bin, `${name}.EXE`), 0o755);
    const realFake = fs.realpathSync(fake);
    const previous = vscode.workspace.getConfiguration('wtpool').inspect('path').globalValue;
    const previousPath = process.env.PATH;
    const previousPathext = process.env.PATHEXT;
    const platform = Object.getOwnPropertyDescriptor(process, 'platform');

    // When
    let found;
    let threw;
    let paused;
    let ran;
    try {
      process.env.PATH = [bin, previousPath].join(path.delimiter);
      delete process.env.PATHEXT;
      Object.defineProperty(process, 'platform', { value: 'win32', configurable: true });
      await vscode.workspace.getConfiguration('wtpool').update('path', name, vscode.ConfigurationTarget.Global);
      await openStatus();
      found = await waitFor(() => starts()[0], 10_000);
      paused = (await views()).filter((v) => v.connection === 'paused').map((v) => v.error);
      ran = found ? fs.realpathSync(found.script) : undefined;
    } catch (error) {
      threw = error;
    } finally {
      if (platform) Object.defineProperty(process, 'platform', platform);
      process.env.PATH = previousPath;
      if (previousPathext === undefined) delete process.env.PATHEXT;
      else process.env.PATHEXT = previousPathext;
      await vscode.workspace.getConfiguration('wtpool').update('path', previous, vscode.ConfigurationTarget.Global);
      fs.rmSync(bin, { recursive: true, force: true });
    }

    // Then
    assert.equal(threw, undefined, `the run on a told-Windows platform threw: ${threw}`);
    assert.ok(found, `a bare wtpool.path found no file named with the default PATHEXT extensions: ${paused}`);
    assert.equal(ran, realFake, 'a bare wtpool.path on Windows did not run the file with the default PATHEXT extension');
  });

  it('messages__should_ignore_everything_outside_the_set failed_action', async () => {
    // Given: open and terminal fail as the VS Code API can, by a rejection and by a throw.
    await resetFake({ scenario: 'idle_pool' });
    const verify = await verifyApi();
    // Earlier checks posted idle_pool too, so only views of this tab count.
    const from = (await views()).length;
    await openStatus();
    const final = expectedDocuments('idle_pool').at(-1);
    const shown = await waitFor(
      async () => (await views()).slice(from).some((v) => JSON.stringify(v.status) === JSON.stringify(final)),
      10_000,
    );
    assert.ok(shown, 'setup: the document never reached the webview');
    const wt1 = final.pools[0].worktrees.find((w) => w.name === 'wt1').path;
    const commands = wrap(
      vscode.commands,
      'executeCommand',
      (id) => id !== 'vscode.openFolder',
      () => Promise.reject(new Error('the window did not open')),
    );
    const terminals = wrap(vscode.window, 'createTerminal', () => false, () => {
      throw new Error('no terminal');
    });
    const errors = wrap(vscode.window, 'showErrorMessage', () => false, () => Promise.resolve(undefined));

    // When
    let threw = 0;
    try {
      for (const type of ['open', 'terminal']) {
        try {
          await verify.fromWebview({ type, id: wt1 });
        } catch {
          threw += 1;
        }
      }
    } finally {
      commands.restore();
      terminals.restore();
      errors.restore();
    }

    // Then
    assert.equal(threw, 0, 'a row action that failed rejected the message of the webview');
    assert.deepEqual(
      errors.calls.map(([text]) => text),
      ['Worktrees: open failed: the window did not open', 'Worktrees: terminal failed: no terminal'],
      'a row action that failed did not show its error',
    );
  });

  it('messages__should_ignore_everything_outside_the_set', async () => {
    // Given
    await resetFake({ scenario: 'folder_missing' });
    const verify = await verifyApi();
    // The scenario checks posted folder_missing too, so only views of this tab count.
    const from = (await views()).length;
    await openStatus();
    const final = expectedDocuments('folder_missing').at(-1);
    const shown = await waitFor(
      async () => (await views()).slice(from).some((v) => JSON.stringify(v.status) === JSON.stringify(final)),
      10_000,
    );
    assert.ok(shown, 'setup: the document never reached the webview');
    const wt1 = final.pools[0].worktrees.find((w) => w.name === 'wt1').path;
    const wt3 = final.pools[0].worktrees.find((w) => w.name === 'wt3').path;
    const commands = wrap(vscode.commands, 'executeCommand', (id) => !String(id).startsWith('vscode.open') && id !== 'vscode.newWindow');
    const terminals = wrap(vscode.window, 'createTerminal', () => false, () => ({ show() {}, sendText() {}, dispose() {} }));
    const clipboard = wrapClipboard();
    const dialogs = MESSAGE_KINDS.map((name) => wrap(vscode.window, name));
    const outside = [
      { type: 'delete', id: wt1 },
      { type: 'open', id: wt1, extra: 1 },
      { type: 'open', id: '/Users/verify/pool/nope' },
      { type: 'terminal', path: '/etc' },
      { type: 'terminal', id: wt1, path: '/etc' },
      JSON.parse(`{"__proto__": {"type": "open", "id": ${JSON.stringify(wt1)}}}`),
      JSON.parse(`{"type": "copyPath", "id": ${JSON.stringify(wt1)}, "__proto__": {"x": 1}}`),
      { type: 'copyPath', id: 'x'.repeat(1_000_000) },
      // Phase 3 adds free and fg to the set. These break it by id or by an extra field.
      { type: 'free', id: '/Users/verify/pool/nope' },
      { type: 'fg', id: '/Users/verify/pool/nope' },
      { type: 'free', id: wt1, extra: 1 },
      { type: 'fg', id: wt1, path: '/etc' },
      { type: 'free' },
      { type: 'fg', id: 7 },
      { type: 'open' },
      { type: 'open', id: 7 },
      { type: 'open', id: wt3 },
      { type: 'terminal', id: wt3 },
      'open',
      null,
      42,
      ['open', wt1],
    ];

    // When
    let threw = 0;
    try {
      for (const message of outside) {
        try {
          await verify.fromWebview(message);
        } catch {
          threw += 1;
        }
      }
      await sleep(500);
      const ignored = {
        commands: commands.calls.filter(([id]) => id !== 'setContext').map(([id]) => id),
        terminals: terminals.calls.length,
        clipboard: clipboard.calls.length,
        dialogs: dialogs.flatMap((d) => d.calls).filter((args) => args.some((a) => a && typeof a === 'object' && a.modal === true)).length,
      };
      await verify.fromWebview({ type: 'open', id: wt1 });
      await verify.fromWebview({ type: 'terminal', id: wt1 });
      await verify.fromWebview({ type: 'copyPath', id: wt3 });
      await sleep(500);

      // Then
      assert.deepEqual(ignored, { commands: [], terminals: 0, clipboard: 0, dialogs: 0 }, 'messages outside the set acted');
      const actions = starts().filter((s) => ['free', 'fg'].includes(s.argv?.[0]));
      assert.deepEqual(actions, [], 'a message outside the set ran wtpool free or wtpool fg');
      assert.equal(threw, 0, 'fromWebview threw on a message outside the set');
      const opens = commands.calls.filter(([id]) => id === 'vscode.openFolder');
      assert.equal(opens.length, 1, 'open ran vscode.openFolder once');
      assert.equal(opens[0][1]?.fsPath, wt1, 'open names the row path');
      assert.equal(opens[0][2]?.forceNewWindow, true, 'open uses a new window');
      assert.equal(terminals.calls.length, 1, 'terminal created one terminal');
      const cwd = terminals.calls[0][0]?.cwd;
      assert.equal(typeof cwd === 'string' ? cwd : cwd?.fsPath, wt1, 'the terminal starts in the row path');
      assert.deepEqual(clipboard.calls, [[wt3]], 'copyPath copies the exact path, a missing row included');
    } finally {
      commands.restore();
      terminals.restore();
      clipboard.restore();
      for (const dialog of dialogs) dialog.restore();
    }
  });

  it('hidden_then_shown', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool', aliveMs: 0 });
    const from = (await views()).length;
    const opened = await openStatus();
    const docs = expectedDocuments('idle_pool');
    // The fake reads control lines only once it has printed its documents, and earlier checks
    // posted idle_pool too, so only a view of this tab counts.
    const first = await waitFor(
      async () => (await views()).slice(from).some((v) => JSON.stringify(v.status) === JSON.stringify(docs.at(-1))),
      10_000,
    );
    assert.ok(first, 'setup: the document never reached the webview');
    await textEditor(opened.group.viewColumn);
    await sleep(500);
    const hiddenFrom = (await posted()).length;
    const later = structuredClone(docs.at(-1));
    later.generated = '2026-10-20T12:05:00Z';
    later.pools[0].worktrees[1].last_change = '2026-10-20T12:04:00Z';

    // When
    control(`emit ${JSON.stringify(later)}`);
    control('alive');
    await sleep(1500);
    const whileHidden = (await posted()).slice(hiddenFrom);
    const running = starts().filter((s) => !fakeLog().some((e) => e.pid === s.pid && e.event !== 'start'));
    await vscode.commands.executeCommand('wtpool.openStatus');
    const shown = await waitFor(async () => {
      const after = (await posted()).slice(hiddenFrom + whileHidden.length);
      return after.find((m) => m.type === 'view' && JSON.stringify(m.status) === JSON.stringify(later));
    }, 2_000);

    // Then
    assert.deepEqual(whileHidden, [], 'the extension posted to a hidden tab');
    assert.equal(running.length, 1, 'the watch stopped while the tab was hidden');
    assert.ok(shown, 'showing the tab did not post the last document at once');
    const panelOptions = created.filter((c) => c.viewType === 'wtpool.status').at(-1)?.options;
    assert.notEqual(panelOptions?.retainContextWhenHidden, true, 'retainContextWhenHidden must stay off');
  });

  it('ready__should_answer_with_the_view', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool', aliveMs: 0 });
    const from = (await views()).length;
    await openStatus();
    const last = JSON.stringify(expectedDocuments('idle_pool').at(-1));
    const first = await waitFor(async () => (await views()).slice(from).some((v) => JSON.stringify(v.status) === last), 10_000);
    assert.ok(first, 'setup: the document never reached the webview');
    // The page's own first messages come and go before the check counts.
    await sleep(1000);
    const before = (await posted()).length;

    // When
    await (await verifyApi()).fromWebview({ type: 'ready' });
    const answer = await waitFor(async () => {
      const after = (await posted()).slice(before);
      return after.find((m) => m.type === 'view' && JSON.stringify(m.status) === last);
    }, 2_000);

    // Then
    assert.ok(answer, 'a ready message got no view with the last document');
  });
});
