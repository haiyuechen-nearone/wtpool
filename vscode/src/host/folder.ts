// Where `wtpool` runs (R9): the window's first workspace folder, for the watch and every action, so
// `wtpool` finds the pool of the repository the window has open. The extension sets no WTPOOL_FOREGROUND
// of its own: one in the environment VS Code started with still names the pool.
import { homedir } from 'node:os';
import * as vscode from 'vscode';

/**
 * Gives the folder `wtpool` runs in, or undefined with no folder open. The extension and `wtpool`
 * run on this machine (X3), so a first folder that is not on it (Remote-SSH, WSL, a dev container,
 * a virtual folder) gives the home folder instead: there a WTPOOL_FOREGROUND of the environment
 * names a local pool, and without one the tab shows wtpool's own error (R10).
 */
export function workFolder(): string | undefined {
  const first = vscode.workspace.workspaceFolders?.[0];
  if (!first) {
    return undefined;
  }
  return first.uri.scheme === 'file' ? first.uri.fsPath : homedir();
}
