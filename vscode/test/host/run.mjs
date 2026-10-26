// Starts VS Code with the extension under development and runs the host checks in its extension
// host (@vscode/test-electron). Everything VS Code writes goes below <run>/host/: user data,
// extensions, home, temp, a workspace folder and the workspace file VS Code opens it from.
// `wtpool.path` names the fake wtpool.
//
// WTPOOL_VERIFY_HOST_COVERAGE names a folder for the extension host's V8 coverage.
// WTPOOL_VERIFY_HOST_GREP selects checks by a regular expression on their titles;
// WTPOOL_VERIFY_HOST_INVERT=1 runs every check but those. WTPOOL_VERIFY_HOST_RETRIES and
// WTPOOL_VERIFY_HOST_ORDER reach the runner (index.cjs). VS Code comes from WTPOOL_VERIFY_VSCODE, else
// the platform's default install location, else a download (support/vscode-app.mjs).
import { chmodSync, copyFileSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { runTests } from '@vscode/test-electron';

import { inBackground } from '../support/background.mjs';
import { CODE_FLAGS, QUIET_SETTINGS, keychainErrors, removeLink, shortLink, writeEarlierBuilds } from '../support/code.mjs';
import { FAKE_WTPOOL, TEST, VSCODE, runDir } from '../support/paths.mjs';
import { resolveVSCode } from '../support/vscode-app.mjs';

// @vscode/test-electron starts VS Code with this process's environment. A run started from a
// VS Code terminal or extension carries ELECTRON_RUN_AS_NODE, which makes VS Code a plain Node,
// and VSCODE_* handles of the owner's own VS Code, so none of them may reach the sandbox.
for (const key of Object.keys(process.env)) {
  if (/^(ELECTRON_|VSCODE_)/.test(key)) delete process.env[key];
}

const name = process.env.WTPOOL_VERIFY_HOST_NAME ?? 'host';
const root = path.join(runDir(), name);
rmSync(path.join(root, 'sandbox'), { recursive: true, force: true });
const sandbox = path.join(root, 'sandbox');
const dirs = {
  userData: path.join(sandbox, 'user-data'),
  extensions: path.join(sandbox, 'extensions'),
  home: path.join(sandbox, 'home'),
  tmp: path.join(sandbox, 'tmp'),
  workspace: path.join(sandbox, 'workspace'),
  other: path.join(sandbox, 'other-folder'),
  fakes: path.join(sandbox, 'fakes'),
};
for (const dir of Object.values(dirs)) mkdirSync(dir, { recursive: true });
writeEarlierBuilds(dirs.home);
mkdirSync(path.join(dirs.userData, 'User'), { recursive: true });
mkdirSync(path.join(dirs.workspace, '.vscode'), { recursive: true });

const workspaceFake = path.join(dirs.fakes, 'workspace-wtpool');
copyFileSync(FAKE_WTPOOL, workspaceFake);
chmodSync(workspaceFake, 0o755);
writeFileSync(
  path.join(dirs.userData, 'User', 'settings.json'),
  JSON.stringify(
    {
      ...QUIET_SETTINGS,
      'wtpool.path': FAKE_WTPOOL,
      'window.restoreWindows': 'none',
    },
    null,
    2,
  ),
);
writeFileSync(
  path.join(dirs.workspace, '.vscode', 'settings.json'),
  JSON.stringify({ 'wtpool.path': workspaceFake }, null, 2),
);
// A workspace file with the one workspace folder, so a check can change the window's first
// folder (folder.test.cjs, R9). VS Code restarts no extension host for that while it runs tests,
// whereas a window opened on a folder would enter a workspace, which stops the checks' host.
const workspaceFile = path.join(sandbox, 'host.code-workspace');
writeFileSync(
  workspaceFile,
  JSON.stringify({ folders: [{ path: dirs.workspace }], settings: { 'wtpool.path': workspaceFake } }, null, 2),
);

const results = path.join(root, 'results.json');
rmSync(results, { force: true });
const keychainFile = path.join(root, 'keychain.txt');
rmSync(keychainFile, { force: true });
// The main process hooks note each side effect they refuse here, which fails the layer (layers.rs).
const sideEffects = path.join(root, 'side-effects.txt');
rmSync(sideEffects, { force: true });
// Short links for the folders VS Code puts sockets in (support/code.mjs).
const link = shortLink(sandbox);
const shortUserData = path.join(link, 'user-data');
const shortTmp = path.join(link, 'tmp');
// VS Code's output reaches this process through @vscode/test-electron; a keychain error in it
// fails the layer (layers.rs reads keychain.txt).
const seen = [];
for (const stream of [process.stdout, process.stderr]) {
  const write = stream.write.bind(stream);
  stream.write = (chunk, ...rest) => {
    seen.push(...keychainErrors(chunk));
    return write(chunk, ...rest);
  };
}
const { app: executable } = await resolveVSCode();
// On macOS VS Code runs as an app that is never active, with a private clipboard and a window
// that does not show (support/background.mjs).
const background = await inBackground({ log: sideEffects });

try {
  const tests = runTests({
    vscodeExecutablePath: executable,
    extensionDevelopmentPath: VSCODE,
    extensionTestsPath: path.join(TEST, 'host', 'index.cjs'),
    launchArgs: [
      workspaceFile,
      '--user-data-dir',
      shortUserData,
      '--extensions-dir',
      dirs.extensions,
      '--disable-extensions',
      ...CODE_FLAGS,
      ...background.args,
      '--new-window',
    ],
    extensionTestsEnv: {
      WTPOOL_VERIFY: '1',
      WTPOOL_VERIFY_SANDBOX: '1',
      WTPOOL_VERIFY_RESULTS: results,
      WTPOOL_VERIFY_HOST_DIR: root,
      WTPOOL_VERIFY_HOST_FAKES: dirs.fakes,
      WTPOOL_VERIFY_WORKSPACE_FAKE: workspaceFake,
      WTPOOL_VERIFY_OTHER_FOLDER: dirs.other,
      WTPOOL_FAKE_CONFIG: path.join(root, 'fake-config.json'),
      HOME: dirs.home,
      TMPDIR: shortTmp,
      TZ: 'UTC',
      ...(process.env.WTPOOL_VERIFY_HOST_COVERAGE ? { NODE_V8_COVERAGE: process.env.WTPOOL_VERIFY_HOST_COVERAGE } : {}),
      ...Object.fromEntries(
        ['WTPOOL_VERIFY_HOST_RETRIES', 'WTPOOL_VERIFY_HOST_ORDER', 'WTPOOL_VERIFY_PHASE']
          .filter((key) => process.env[key] !== undefined)
          .map((key) => [key, process.env[key]]),
      ),
    },
  });
  // A failed start shows at `await tests`, after a failed release, which stops VS Code.
  tests.catch(() => {});
  await background.release().catch(async (error) => {
    await tests.catch(() => {});
    throw error;
  });
  await tests;
} catch (error) {
  // The results file holds the outcome of each check; a failed check makes VS Code exit 1.
  process.stderr.write(`host: ${error}\n`);
} finally {
  removeLink(link);
}
if (seen.length) writeFileSync(keychainFile, `${seen.join('\n')}\n`);
