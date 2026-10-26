// The demo's sandbox: a temporary folder that holds everything the recorded VS Code and its fake
// `wtpool` read or write: user data, extensions, HOME, TMPDIR, the workspace folder, the fake's
// config, control file and log, and the live build log. Nothing outside it is read but the
// .vsix, the harness's fake wtpool and the VS Code binary, and `remove` deletes it all.
import { execFileSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { QUIET_SETTINGS, removeLink, shortLink } from '../../test/support/code.mjs';
import { FAKE_WTPOOL, VSIX } from '../../test/support/paths.mjs';
import { env, vscodePaths } from '../../test/e2e/vscode.mjs';

export const SCRIPTS = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

/** The look of the recording: Dark Modern, no side bar clutter, no minimap, no breadcrumbs. */
const LOOK = {
  'workbench.colorTheme': 'Default Dark Modern',
  'workbench.layoutControl.enabled': false,
  'workbench.editor.empty.hint': 'hidden',
  'breadcrumbs.enabled': false,
  'editor.minimap.enabled': false,
  'window.restoreWindows': 'none',
  'window.newWindowDimensions': 'default',
  // The working spinner turns (icons.css), whatever the system's reduce motion setting says.
  'workbench.reduceMotion': 'off',
  'terminal.integrated.profiles.osx': { 'wtpool-demo sh': { path: '/bin/sh', args: [] } },
  'terminal.integrated.defaultProfile.osx': 'wtpool-demo sh',
};

/**
 * A fresh sandbox below the system temporary folder, with the .vsix installed and `wtpool.path`
 * naming a wrapper that runs the harness's fake wtpool on the sandbox's config. The workspace folder
 * is called `name` (the repository's folder name), so the window title reads like the story.
 */
export function create(name) {
  if (!existsSync(VSIX)) throw new Error('demo: no vscode/wtpool.vsix; run `npm run package` first');
  const root = mkdtempSync(path.join(os.tmpdir(), 'wtpool-demo-'));
  // The harness's env() and close() keep their run files below WTPOOL_VERIFY_RUN_DIR.
  process.env.WTPOOL_VERIFY_RUN_DIR = root;
  const link = shortLink(root);
  const dirs = {
    root,
    link,
    // VS Code's sockets go in these two, so VS Code gets them through the short link.
    userData: path.join(link, 'user-data'),
    tmp: path.join(link, 'tmp'),
    extensions: path.join(root, 'extensions'),
    home: path.join(root, 'home'),
    workspace: path.join(root, 'code', name),
    bin: path.join(root, 'bin'),
    logs: path.join(root, 'logs'),
    control: path.join(root, 'fake-control.txt'),
    fakeLog: path.join(root, 'fake-log.jsonl'),
    fakeConfig: path.join(root, 'fake-config.json'),
  };
  for (const key of ['userData', 'tmp', 'extensions', 'home', 'workspace', 'bin', 'logs']) {
    mkdirSync(dirs[key], { recursive: true });
  }
  mkdirSync(path.join(dirs.userData, 'User'), { recursive: true });
  writeFileSync(dirs.control, '');
  dirs.wtpoolPath = path.join(dirs.bin, 'wtpool');
  // wtpool.path takes a binary with no shell (R8), so a two line wrapper hands the fake its config.
  writeFileSync(
    dirs.wtpoolPath,
    `#!/bin/sh\nWTPOOL_FAKE_CONFIG='${dirs.fakeConfig}' exec '${process.execPath}' '${FAKE_WTPOOL}' "$@"\n`,
  );
  chmodSync(dirs.wtpoolPath, 0o755);
  const settings = { ...QUIET_SETTINGS, ...LOOK, 'wtpool.path': dirs.wtpoolPath };
  writeFileSync(path.join(dirs.userData, 'User', 'settings.json'), JSON.stringify(settings, null, 2));
  execFileSync(
    vscodePaths().cli,
    ['--install-extension', VSIX, '--force', '--extensions-dir', dirs.extensions, '--user-data-dir', dirs.userData],
    { env: env(dirs, undefined), stdio: 'pipe', timeout: 120_000 },
  );
  return dirs;
}

/**
 * Configures the fake wtpool: `wtpool status --watch` prints `first`, then whatever `emit` appends to
 * the control file, with an alive line each second; `wtpool free` and `wtpool fg` only log and succeed.
 */
export function configureFake(dirs, first) {
  writeFileSync(
    dirs.fakeConfig,
    JSON.stringify({ log: dirs.fakeLog, documents: [first], control: dirs.control, aliveMs: 1000 }),
  );
}

/** Makes the running fake print `document` as its next line. */
export function emit(dirs, document) {
  writeFileSync(dirs.control, `emit ${JSON.stringify(document)}\n`, { flag: 'a' });
}

/** The argv of each start of the fake so far, oldest first. */
export function fakeStarts(dirs) {
  if (!existsSync(dirs.fakeLog)) return [];
  return readFileSync(dirs.fakeLog, 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line))
    .filter((entry) => entry.event === 'start')
    .map((entry) => entry.argv);
}

/** Deletes the sandbox and its short link. */
export function remove(dirs) {
  removeLink(dirs.link);
  rmSync(dirs.root, { recursive: true, force: true });
}
