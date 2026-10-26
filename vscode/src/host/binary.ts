import { accessSync, constants, statSync } from 'node:fs';
import * as path from 'node:path';

/** The extensions Windows tries for a bare name when PATHEXT is unset. */
const DEFAULT_PATHEXT = '.COM;.EXE;.BAT;.CMD';

function fileNames(name: string): string[] {
  if (process.platform !== 'win32') {
    return [name];
  }
  const extensions = (process.env.PATHEXT ?? DEFAULT_PATHEXT)
    .split(';')
    .filter((extension) => extension !== '');
  return [
    ...(path.extname(name) === '' ? [] : [name]),
    ...extensions.map((extension) => `${name}${extension}`),
  ];
}

function isRunnable(file: string): boolean {
  try {
    accessSync(file, constants.X_OK);
    return statSync(file).isFile();
  } catch {
    return false;
  }
}

/**
 * Resolves `command` for a spawn: a bare name to its first runnable match in an absolute PATH
 * entry, any other path to itself. `wtpool` runs in the workspace folder, so a relative PATH entry,
 * or libuv's own search of the working folder on Windows, would let a repository supply the binary.
 * Throws `spawn <command> ENOENT`, as spawn does, when no absolute entry holds a bare name.
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
