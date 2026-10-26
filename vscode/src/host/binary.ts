// The file a spawn of `wtpool` runs (X4). A bare `wtpool.path`, a name with no folder in it, is
// found here in the absolute entries of PATH only: libuv on Windows looks in the working folder
// first, the workspace folder (R9), and a relative entry names a folder below it on every
// system, so a repository could otherwise supply the binary. Any other path stays as it is.
import { accessSync, constants, statSync } from 'node:fs';
import * as path from 'node:path';

/** The extensions Windows tries for a bare name when PATHEXT is unset. */
const DEFAULT_PATHEXT = '.COM;.EXE;.BAT;.CMD';

/**
 * Lists the file names to look for in each folder: `name` alone, or on Windows `name` with each
 * PATHEXT extension, after `name` itself when it has one.
 */
function fileNames(name: string): string[] {
  if (process.platform !== 'win32') {
    return [name];
  }
  const extensions = (process.env.PATHEXT ?? DEFAULT_PATHEXT).split(';').filter((extension) => extension !== '');
  return [...(path.extname(name) === '' ? [] : [name]), ...extensions.map((extension) => `${name}${extension}`)];
}

/** Tells a regular file that this process may run. */
function isRunnable(file: string): boolean {
  try {
    accessSync(file, constants.X_OK);
    return statSync(file).isFile();
  } catch {
    // Missing, not allowed, or in a folder that cannot be read: not this one.
    return false;
  }
}

/**
 * Resolves `command` for a spawn: a bare name to the first runnable match in an absolute PATH
 * entry, anything else to itself. Throws when no absolute entry holds a bare name, worded as
 * spawn words a missing binary, so the tab still says `wtpool did not start: spawn wtpool ENOENT`
 * (R10).
 */
export function resolveBinary(command: string): string {
  if (path.basename(command) !== command) {
    return command;
  }
  const entries = (process.env.PATH ?? '').split(path.delimiter);
  const folders = entries.filter((folder) => path.isAbsolute(folder));
  for (const folder of folders) {
    for (const name of fileNames(command)) {
      const file = path.join(folder, name);
      if (isRunnable(file)) {
        return file;
      }
    }
  }
  throw new Error(`spawn ${command} ENOENT`);
}
