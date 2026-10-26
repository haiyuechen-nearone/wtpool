import { homedir } from 'node:os';
import * as vscode from 'vscode';

/**
 * Gives the folder `wtpool` runs in: the window's first workspace folder, or undefined with none
 * open. The extension and `wtpool` run on the local machine, so a folder that is not on it
 * (Remote-SSH, WSL, a dev container, a virtual folder) gives the home folder instead.
 */
export function workFolder(): string | undefined {
  const first = vscode.workspace.workspaceFolders?.[0];
  if (!first) {
    return undefined;
  }
  return first.uri.scheme === 'file' ? first.uri.fsPath : homedir();
}

/**
 * Tells a window whose first folder is not on this computer, so `wtpool` runs in the home folder.
 */
export function isFolderRemote(): boolean {
  const first = vscode.workspace.workspaceFolders?.[0];
  return first !== undefined && first.uri.scheme !== 'file';
}
