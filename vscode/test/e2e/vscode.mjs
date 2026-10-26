// Drives a real VS Code with the packaged extension and the release wtpool for the e2e layer. Every
// folder VS Code and wtpool write is below <run>/e2e/: user data, extensions, home and temp. wtpool sees
// a sandbox pool each check attempt builds from a scenario file ({@link pool}), in Asia/Kolkata.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import {
  appendFileSync,
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { _electron as electron, test } from '@playwright/test';

import { inBackground } from '../support/background.mjs';
import { CODE_FLAGS, QUIET_SETTINGS, keychainErrors, removeLink, shortLink, writeEarlierBuilds } from '../support/code.mjs';
import { VSIX, runDir } from '../support/paths.mjs';
import { installedVSCode } from '../support/vscode-app.mjs';

/** The VS Code binary and CLI that global-setup.mjs found or downloaded: {app, cli}. */
export function vscodePaths() {
  const found = installedVSCode();
  if (!found) throw new Error('setup: no VS Code found; run the e2e layer through its playwright config, whose global setup downloads one');
  return found;
}

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** A folder name for `key` in this test run: `--repeat-each` copies of a test run at once. */
function attemptName(key) {
  const repeat = test.info().repeatEachIndex;
  return `${key.replaceAll('/', '-')}${repeat ? `-repeat${repeat}` : ''}`;
}

/**
 * A fresh sandbox pool of `scenario` in <run>/e2e/pools/<key>, built by `cargo xtask pool`
 * (WTPOOL_VERIFY_XTASK), replacing what an earlier attempt left: {scenario, dir, fg, common}.
 */
export function pool(scenario, key = scenario) {
  const xtask = process.env.WTPOOL_VERIFY_XTASK;
  if (!xtask || !existsSync(xtask)) throw new Error('setup: WTPOOL_VERIFY_XTASK names no xtask binary');
  const dir = path.join(runDir(), 'e2e', 'pools', attemptName(key));
  const text = execFileSync(xtask, ['pool', '--scenario', scenario, '--dir', dir], { encoding: 'utf8', timeout: 120_000 });
  return JSON.parse(text);
}

export function releaseWtpool() {
  const binary = process.env.WTPOOL_VERIFY_RELEASE_BIN;
  if (!binary || !existsSync(binary)) throw new Error('setup: WTPOOL_VERIFY_RELEASE_BIN names no release wtpool');
  return binary;
}

/** A sh terminal profile: a sandbox HOME holds no shell configuration to prompt for. */
const TERMINAL = {
  'terminal.integrated.profiles.osx': { 'wtpool-verify sh': { path: '/bin/sh', args: [] } },
  'terminal.integrated.defaultProfile.osx': 'wtpool-verify sh',
};

/** Writes the user settings file VS Code watches: the sandbox defaults plus `extra`. */
export function writeSettings(dirs, extra = {}) {
  const settings = {
    ...QUIET_SETTINGS,
    ...TERMINAL,
    'wtpool.path': dirs.wtpoolPath,
    'window.restoreWindows': 'all',
    'window.newWindowDimensions': 'default',
    ...extra,
  };
  writeFileSync(path.join(dirs.userData, 'User', 'settings.json'), JSON.stringify(settings, null, 2));
}

/**
 * A fresh sandbox with the .vsix installed and `wtpool.path` naming the release wtpool, or `wtpoolPath`.
 * `settings` adds user settings. Its home holds {@link writeEarlierBuilds}'s machine config unless
 * `earlierBuilds` is false.
 */
export function sandbox(name, { settings = {}, wtpoolPath, earlierBuilds = true } = {}) {
  assert.ok(existsSync(VSIX), 'npm run package wrote no vscode/wtpool.vsix');
  const root = path.join(runDir(), 'e2e', attemptName(name));
  // A retry starts from nothing, not from the user data an earlier attempt left.
  rmSync(root, { recursive: true, force: true });
  mkdirSync(root, { recursive: true });
  const link = shortLink(root);
  const dirs = {
    root,
    link,
    // VS Code's sockets go in these two, so VS Code gets them through the short link.
    userData: path.join(link, 'user-data'),
    tmp: path.join(link, 'tmp'),
    extensions: path.join(root, 'extensions'),
    home: path.join(root, 'home'),
    workspace: path.join(root, 'workspace'),
    bin: path.join(root, 'bin'),
    wtpoolPath: wtpoolPath ?? releaseWtpool(),
  };
  for (const key of ['userData', 'tmp', 'extensions', 'home', 'workspace', 'bin']) {
    mkdirSync(dirs[key], { recursive: true });
  }
  if (earlierBuilds) writeEarlierBuilds(dirs.home);
  mkdirSync(path.join(dirs.userData, 'User'), { recursive: true });
  writeSettings(dirs, settings);
  try {
    execFileSync(
      vscodePaths().cli,
      ['--install-extension', VSIX, '--force', '--extensions-dir', dirs.extensions, '--user-data-dir', dirs.userData],
      { env: env(dirs, undefined), stdio: 'pipe', timeout: 120_000 },
    );
  } catch (error) {
    removeLink(link);
    throw error;
  }
  return dirs;
}

/**
 * `<run>/e2e`, symlinks resolved: the `GIT_CEILING_DIRECTORIES` of every sandbox process, so git
 * and wtpool, looking for the repository a folder of the run is in, never climb into the repository
 * that holds the run folder (R9: the extension runs wtpool in the workspace folder).
 */
export function ceiling() {
  const dir = path.join(runDir(), 'e2e');
  mkdirSync(dir, { recursive: true });
  return realpathSync(dir);
}

/**
 * The environment of VS Code and every process it starts. With `poolInfo` it names the pool in
 * `WTPOOL_FOREGROUND` and `WTPOOL_SLOT_DIR`; without, wtpool finds the pool from its working directory (R9).
 */
export function env(dirs, poolInfo) {
  const variables = {
    PATH: process.env.PATH,
    HOME: dirs.home,
    TMPDIR: dirs.tmp,
    XDG_CONFIG_HOME: path.join(dirs.home, '.config'),
    XDG_CACHE_HOME: path.join(dirs.home, '.cache'),
    XDG_DATA_HOME: path.join(dirs.home, '.local', 'share'),
    XDG_STATE_HOME: path.join(dirs.home, '.local', 'state'),
    TZ: 'Asia/Kolkata',
    WTPOOL_VERIFY_SANDBOX: '1',
    GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_CONFIG_NOSYSTEM: '1',
    GIT_CEILING_DIRECTORIES: ceiling(),
  };
  if (poolInfo) {
    variables.WTPOOL_FOREGROUND = poolInfo.fg;
    variables.WTPOOL_SLOT_DIR = poolInfo.dir;
  }
  return variables;
}

/** Waits for a window's workbench. */
export async function workbench(window) {
  await window.waitForLoadState('domcontentloaded');
  await window.locator('.monaco-workbench').waitFor({ timeout: 60_000 });
  return window;
}

/**
 * Starts VS Code on `folder`, by default `dirs.workspace`, or with null on no folder, and returns
 * the session: {app, window, dirs, output, env}. `output` collects what VS Code prints, so
 * {@link close} can find keychain errors; `env` is the environment VS Code got. On macOS VS Code
 * runs as an app that is never active, with a private clipboard and windows that do not show
 * (support/background.mjs, support/main-hooks.mjs); a side effect the hooks refuse goes to
 * <run>/e2e/side-effects.txt, which fails the layer (layers.rs).
 */
export async function launch(dirs, poolInfo, extraArgs = [], { folder = dirs.workspace } = {}) {
  const variables = env(dirs, poolInfo);
  const background = await inBackground({ log: sideEffectsFile() });
  const launching = electron.launch({
    executablePath: vscodePaths().app,
    args: [
      '--user-data-dir',
      dirs.userData,
      '--extensions-dir',
      dirs.extensions,
      ...CODE_FLAGS,
      ...background.args,
      '--new-window',
      ...extraArgs,
      ...(folder === null ? [] : [folder]),
    ],
    env: variables,
    timeout: 120_000,
  });
  // A failed launch shows at `await launching`, after a failed release, which stops VS Code.
  launching.catch(() => {});
  await background.release().catch(async (error) => {
    await launching.then((started) => started.close(), () => {});
    throw error;
  });
  const app = await launching;
  const output = [];
  for (const stream of [app.process().stdout, app.process().stderr]) {
    stream?.on('data', (chunk) => output.push(String(chunk)));
  }
  app.on('window', (page) => appOf.set(page, app));
  const window = await workbench(await app.firstWindow());
  appOf.set(window, app);
  return { app, window, dirs, output, env: variables, main: app.process().pid };
}

/**
 * Opens a second window of the session's VS Code on `folder`, by default a second empty folder of
 * the sandbox, as a user does with `code --new-window <folder>`: the CLI hands the folder to the
 * running VS Code, which shows the window that has a folder open already rather than a second
 * one. A window needs a folder for the tab to run wtpool (R9), so the "New Window" command, which
 * opens none, no longer gives a window that shows the pool. Returns the window, its workbench
 * loaded.
 */
export async function newWindow(session, poolInfo, folder = path.join(session.dirs.root, 'workspace-2')) {
  const { dirs } = session;
  mkdirSync(folder, { recursive: true });
  const opened = session.app.waitForEvent('window', { timeout: 60_000 });
  execFileSync(
    vscodePaths().cli,
    ['--user-data-dir', dirs.userData, '--extensions-dir', dirs.extensions, ...CODE_FLAGS, '--new-window', folder],
    { env: env(dirs, poolInfo), stdio: 'pipe', timeout: 60_000 },
  );
  return workbench(await opened);
}

/**
 * Closes VS Code and removes the sandbox's short link. A keychain error in VS Code's output
 * goes to <run>/e2e/keychain.txt, which fails the layer (layers.rs).
 */
export async function close(session) {
  try {
    await session.app.close();
  } catch {
    try {
      process.kill(session.main, 'SIGKILL');
    } catch {
      // Already gone.
    }
  }
  removeLink(session.dirs.link);
  const errors = keychainErrors(session.output.join(''));
  if (errors.length) {
    appendFileSync(path.join(runDir(), 'e2e', 'keychain.txt'), `${session.dirs.root}\n${errors.join('\n')}\n`);
  }
}

/** The VS Code application of each window the e2e layer drives. */
const appOf = new WeakMap();

/**
 * Runs the command of an item of VS Code's application menu in `window`, such as
 * `workbench.action.showCommands` of View > Command Palette...: VS Code's main process sends the
 * window the `vscode:runAction` message a click on the item sends. A click itself acts only in
 * the focused window, and on macOS no window of the oracle has focus (support/background.mjs).
 */
async function menuCommand(window, command) {
  const app = appOf.get(window);
  if (!app) return;
  const win = await app.browserWindow(window);
  await win.evaluate((w, id) => w.webContents.send('vscode:runAction', { id, from: 'menu' }), command);
}

/**
 * Opens the quick input with `keys` and fills in `text`. A webview that takes focus late closes
 * a quick input that just opened, and the keys may never reach the workbench: a webview frame can
 * hold the keyboard while the workbench's active element is its body, and the workbench then
 * drops every key the webview forwards. So it tries again, from the second try on through the
 * menu command `fallback`, which a keyboard reaches through the menu bar as well.
 */
async function quickInput(window, keys, text, { fallback }) {
  const input = window.locator('.quick-input-widget input');
  for (let attempt = 0; attempt < 4; attempt += 1) {
    if (attempt > 0) await menuCommand(window, fallback);
    else await window.keyboard.press(keys);
    if (!(await input.waitFor({ timeout: 3_000 }).then(() => true, () => false))) continue;
    await sleep(400);
    if (!(await input.isVisible())) continue;
    if (await input.fill(text, { timeout: 3_000 }).then(() => true, () => false)) return input;
  }
  return input;
}

/** The labels of the quick input's entries, top to bottom. */
async function entries(window) {
  return window.locator('.quick-input-widget:visible .quick-input-list .monaco-list-row').evaluateAll((found) =>
    found
      .sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top)
      .map((row) => (row.querySelector('.label-name')?.textContent ?? row.textContent ?? '').trim()),
  );
}

/**
 * Opens the quick input, types `text` and waits up to 5 s for an entry `matches` accepts; tries
 * three times, as a quick input that loses focus closes. Returns the entry's index and the
 * entries last seen.
 */
async function pick(window, keys, text, matches, options) {
  let seen = [];
  for (let attempt = 0; attempt < 3; attempt += 1) {
    await quickInput(window, keys, text, options);
    const deadline = Date.now() + 5_000;
    while (Date.now() < deadline) {
      seen = await entries(window).catch(() => []);
      const index = seen.findIndex(matches);
      if (index >= 0) return { index, seen };
      await sleep(200);
    }
    await window.keyboard.press('Escape');
  }
  return { index: -1, seen };
}

/**
 * Runs a command through the Command Palette, as a user types it: the title, arrow keys down
 * to the entry whose label is exactly `title`, Enter. It never uses the mouse.
 */
export async function runCommand(window, title) {
  const fallback = 'workbench.action.showCommands';
  const { index, seen } = await pick(window, 'Meta+Shift+P', `>${title}`, (label) => label === title, { fallback });
  // A missing "Worktrees: Open Status" is the product's failure, so this is an assertion.
  if (index < 0) assert.fail(`the Command Palette lists no "${title}": ${JSON.stringify(seen)}`);
  for (let step = 0; step < index; step += 1) await window.keyboard.press('ArrowDown');
  await window.keyboard.press('Enter');
  await window.locator('.quick-input-widget').waitFor({ state: 'hidden', timeout: 5_000 }).catch(() => {});
}

/** The frame of the Worktrees tab's webview content that holds rows, or undefined. */
export async function panelFrame(window, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    for (const frame of window.frames()) {
      try {
        if (await frame.locator('tr[data-row-id]').count()) return frame;
      } catch {
        // A frame that navigates away cannot be queried; the next round finds the new one.
      }
    }
    if (Date.now() >= deadline) return undefined;
    await window.waitForTimeout(250);
  }
}

/** Every editor group of a window, left to right: {active, tabs: [{label, active}]}. */
export async function editorGroups(window) {
  return window.locator('.editor-group-container').evaluateAll((groups) =>
    groups
      .map((group) => ({
        left: group.getBoundingClientRect().left,
        top: group.getBoundingClientRect().top,
        active: group.classList.contains('active'),
        tabs: [...group.querySelectorAll('.tabs-container .tab')].map((tab) => ({
          label: (tab.getAttribute('aria-label') ?? tab.textContent ?? '').split(',')[0].trim(),
          active: tab.classList.contains('active'),
        })),
      }))
      .sort((a, b) => a.left - b.left || a.top - b.top),
  );
}

/** Whether a tab label is the Worktrees tab's: `Worktrees`, or from phase 2 `Worktrees (N)`. */
export const isStatusTab = (label) => /^Worktrees( \(\d+\))?$/.test(label);

/** Opens a file of the workspace in the active group with Quick Open (Cmd+P), as a user does. */
export async function openTextFile(window, dirs, name) {
  const file = path.join(dirs.workspace, name);
  if (!existsSync(file)) writeFileSync(file, `${name}\n`);
  // An absolute path lists the file at once, without waiting for the file search.
  const { index, seen } = await pick(window, 'Meta+P', file, (label) => label.startsWith(name), { fallback: 'workbench.action.quickOpen' });
  if (index < 0) throw new Error(`setup: Quick Open lists no ${name}: ${JSON.stringify(seen)}`);
  await window.locator('.quick-input-widget:visible .quick-input-list .monaco-list-row').filter({ hasText: name }).first().click();
  await window.locator(`.tabs-container .tab[aria-label^="${name}"]`).first().waitFor({ timeout: 10_000 });
}

/** Every `wtpool` process of the release wtpool below `main`: {pid, ppid, args}. */
export function wtpoolProcesses(main, binary = releaseWtpool()) {
  const below = new Set(descendants(main));
  const listing = execFileSync('ps', ['-axo', 'pid=,ppid=,args='], { encoding: 'utf8' });
  return listing
    .split('\n')
    .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(.*)$/))
    .filter((match) => match && match[3].startsWith(binary) && below.has(Number(match[1])))
    .map((match) => ({ pid: Number(match[1]), ppid: Number(match[2]), args: match[3] }));
}

/** Every `wtpool status --watch` process of the release wtpool: {pid, ppid}. */
export function watchers(binary = releaseWtpool()) {
  const listing = execFileSync('ps', ['-axo', 'pid=,ppid=,args='], { encoding: 'utf8' });
  return listing
    .split('\n')
    .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(.*)$/))
    .filter((match) => match && match[3].startsWith(binary) && match[3].includes('status') && match[3].includes('--watch'))
    .map((match) => ({ pid: Number(match[1]), ppid: Number(match[2]) }));
}

/** Every process below `pid`. */
export function descendants(pid) {
  const listing = execFileSync('ps', ['-axo', 'pid=,ppid='], { encoding: 'utf8' });
  const children = new Map();
  for (const line of listing.split('\n')) {
    const [child, parent] = line.trim().split(/\s+/).map(Number);
    if (!child) continue;
    if (!children.has(parent)) children.set(parent, []);
    children.get(parent).push(child);
  }
  const found = [];
  const stack = [pid];
  while (stack.length) {
    const next = stack.pop();
    for (const child of children.get(next) ?? []) {
      found.push(child);
      stack.push(child);
    }
  }
  return found;
}

/**
 * Starts a process called `name` in `cwd`, a copy of sleep, as a build would run there. Only
 * macOS kills a copy of a platform binary at exec, so only there the copy gets an ad hoc
 * signature, through the codesign macOS ships.
 */
export function startProcess(dirs, name, cwd) {
  const program = path.join(dirs.bin, name);
  if (!existsSync(program)) {
    copyFileSync('/bin/sleep', program);
    chmodSync(program, 0o755);
    if (process.platform === 'darwin') execFileSync('codesign', ['--force', '--sign', '-', program], { stdio: 'pipe' });
  }
  return spawn(program, ['600'], { cwd, stdio: 'ignore' });
}

/** Runs git in `cwd` as the sandbox does. */
export function git(cwd, args) {
  return execFileSync('git', args, {
    cwd,
    encoding: 'utf8',
    env: {
      PATH: process.env.PATH,
      GIT_CONFIG_GLOBAL: '/dev/null',
      GIT_CONFIG_NOSYSTEM: '1',
      GIT_AUTHOR_NAME: 'wtpool-verify',
      GIT_AUTHOR_EMAIL: 'verify@wtpool.invalid',
      GIT_COMMITTER_NAME: 'wtpool-verify',
      GIT_COMMITTER_EMAIL: 'verify@wtpool.invalid',
    },
  });
}

/** The document `wtpool status --json` prints for the pool, with the sandbox environment. */
export function statusJson(dirs, poolInfo) {
  const text = execFileSync(dirs.wtpoolPath, ['status', '--json'], {
    cwd: poolInfo.fg,
    encoding: 'utf8',
    env: env(dirs, poolInfo),
    timeout: 30_000,
  });
  return JSON.parse(text);
}

/** The file the main process hooks of an e2e VS Code note each refused side effect in. */
export function sideEffectsFile() {
  if (!process.env.WTPOOL_VERIFY_RUN_DIR) return undefined;
  const dir = path.join(runDir(), 'e2e');
  mkdirSync(dir, { recursive: true });
  return path.join(dir, 'side-effects.txt');
}

/**
 * Fails, as a setup error, unless the session's clipboard is VS Code's private one
 * (support/main-hooks.mjs): on macOS no check may read or write the system pasteboard. Under
 * xvfb on Linux the clipboard is the X server's own, private to the run.
 */
async function assertPrivateClipboard(app) {
  if (process.platform !== 'darwin') return;
  const ready = await app.evaluate(() => Boolean(globalThis.__wtpoolVerify?.clipboard));
  if (!ready) throw new Error('setup: this VS Code has no private clipboard, so a check would use the system pasteboard');
}

/** The text of VS Code's clipboard, read through Electron's clipboard in VS Code's main process. */
export async function readClipboard(app) {
  await assertPrivateClipboard(app);
  return app.evaluate(({ clipboard }) => clipboard.readText());
}

export async function writeClipboard(app, text) {
  await assertPrivateClipboard(app);
  await app.evaluate(({ clipboard }, value) => clipboard.writeText(value), text);
}

/**
 * The change count of the macOS system pasteboard, which every write moves, read without its
 * content and so without any permission; undefined elsewhere.
 */
export function pasteboardCount() {
  if (process.platform !== 'darwin') return undefined;
  const script = "ObjC.import('AppKit'); $.NSPasteboard.generalPasteboard.changeCount";
  return Number(execFileSync('osascript', ['-l', 'JavaScript', '-e', script], { encoding: 'utf8', timeout: 10_000 }).trim());
}
