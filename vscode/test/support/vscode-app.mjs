// Where the oracle finds VS Code (verification.md, "Portability"), in this order:
// `WTPOOL_VERIFY_VSCODE` (the binary) and `WTPOOL_VERIFY_VSCODE_CLI` (its `code` CLI); else the
// platform's default install location; else the pinned version, downloaded once through
// @vscode/test-electron into `<cache>/wtpool-verify/vscode/`, where `<cache>` is `XDG_CACHE_HOME`
// when absolute, else ~/.cache. xtask/src/reuse.rs finds the CLI in the same order for the part
// hashes.
import { existsSync, realpathSync, statSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { downloadAndUnzipVSCode, resolveCliPathFromVSCodeExecutablePath } from '@vscode/test-electron';

/** The VS Code a download gets: the oldest the extension supports (`engines.vscode`). */
export const PINNED_VERSION = '1.140.0';

/**
 * `<cache>/wtpool-verify/`, the harness's cache folder outside the run folder: `XDG_CACHE_HOME` when
 * it is set and absolute, else ~/.cache, as xtask reads it.
 */
export function cacheRoot() {
  const xdg = process.env.XDG_CACHE_HOME;
  const base = xdg && path.isAbsolute(xdg) ? xdg : path.join(os.homedir(), '.cache');
  return path.join(base, 'wtpool-verify');
}

/** The folder the pinned VS Code downloads into. */
export function downloadDir() {
  return path.join(cacheRoot(), 'vscode');
}

const isFile = (file) => {
  try {
    return statSync(file).isFile();
  } catch {
    return false;
  }
};

/** The first `name` on PATH, symlinks resolved, or undefined; no `which`, which Windows lacks. */
function onPath(name) {
  for (const dir of (process.env.PATH ?? '').split(path.delimiter).filter(Boolean)) {
    const file = path.join(dir, name);
    if (isFile(file)) return realpathSync(file);
  }
  return undefined;
}

/** The default install locations of this platform, [{app, cli}], those of xtask/src/reuse.rs. */
function defaultInstalls() {
  if (process.platform === 'darwin') {
    const bundle = path.join('/Applications', 'Visual Studio Code.app', 'Contents');
    return [{ app: path.join(bundle, 'MacOS', 'Code'), cli: path.join(bundle, 'Resources', 'app', 'bin', 'code') }];
  }
  if (process.platform === 'win32') {
    if (!process.env.LOCALAPPDATA) return [];
    const dir = path.join(process.env.LOCALAPPDATA, 'Programs', 'Microsoft VS Code');
    return [{ app: path.join(dir, 'Code.exe'), cli: path.join(dir, 'bin', 'code.cmd') }];
  }
  const found = [{ app: '/usr/share/code/code', cli: '/usr/share/code/bin/code' }];
  // `code` on PATH is the CLI script <root>/bin/code of an install whose binary is <root>/code.
  const cli = onPath('code');
  if (cli && path.basename(path.dirname(cli)) === 'bin') {
    found.push({ app: path.join(path.dirname(path.dirname(cli)), 'code'), cli });
  }
  return found;
}

/**
 * The VS Code to run without a download, {app, cli, source}, or undefined: the variables, then
 * the default install locations. A variable that names no file is a setup error, never a
 * fallback, so a typo does not test another VS Code.
 */
export function installedVSCode() {
  const fromEnv = process.env.WTPOOL_VERIFY_VSCODE;
  const cliFromEnv = process.env.WTPOOL_VERIFY_VSCODE_CLI;
  for (const [name, file] of [['WTPOOL_VERIFY_VSCODE', fromEnv], ['WTPOOL_VERIFY_VSCODE_CLI', cliFromEnv]]) {
    if (file && !isFile(file)) throw new Error(`setup: ${name} names no file: ${file}`);
  }
  if (fromEnv) {
    return { app: fromEnv, cli: cliFromEnv ?? resolveCliPathFromVSCodeExecutablePath(fromEnv), source: 'WTPOOL_VERIFY_VSCODE' };
  }
  const installed = defaultInstalls().find((install) => isFile(install.app) && existsSync(install.cli));
  if (installed) return { ...installed, cli: cliFromEnv ?? installed.cli, source: 'the default install location' };
  return undefined;
}

/**
 * The VS Code the oracle runs, {app, cli, source}: {@link installedVSCode}, else the pinned
 * version, downloaded on first use and reused from the cache after.
 */
export async function resolveVSCode() {
  const installed = installedVSCode();
  if (installed) return installed;
  const app = await downloadAndUnzipVSCode({ version: PINNED_VERSION, cachePath: downloadDir() });
  const cli = process.env.WTPOOL_VERIFY_VSCODE_CLI ?? resolveCliPathFromVSCodeExecutablePath(app);
  return { app, cli, source: `VS Code ${PINNED_VERSION} downloaded into ${downloadDir()}` };
}

/**
 * Resolves VS Code once and names it in WTPOOL_VERIFY_VSCODE and WTPOOL_VERIFY_VSCODE_CLI of this
 * process, so the test processes it starts find the same one with {@link installedVSCode}.
 */
export async function exportVSCode() {
  const found = await resolveVSCode();
  process.env.WTPOOL_VERIFY_VSCODE = found.app;
  process.env.WTPOOL_VERIFY_VSCODE_CLI = found.cli;
  return found;
}
