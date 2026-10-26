// The row actions that run at once (X11, U4): `open`, `terminal` and `copyPath`. messages.ts
// reads the webview's request for one, and confirm.ts runs `free` and `fg`, as they ask first (X12).
// Text from the document reaches a terminal name, a dialog or a notification only as plain text.
import * as vscode from 'vscode';

import { plain } from '../shared/invisible';
import type { DirectAction } from './messages';

// follow.ts takes plain text from here.
export { plain };

/**
 * Turns text into plain text for a notification. VS Code renders `[label](command:…)` in a toast
 * as a link, so `[` and `]` from the document or from wtpool become fullwidth brackets (a
 * backslash would stay visible and still leave the link pattern intact). Plain text otherwise (R8).
 */
function toast(text: string): string {
  return plain(text).replace(/\[/g, '［').replace(/\]/g, '］');
}

/** Shows `text` as an error notification of the Worktrees tab. */
export function showError(text: string): void {
  void vscode.window.showErrorMessage(toast(`Worktrees: ${text}`));
}

/** Runs `open`, `terminal` or `copyPath` on the row's worktree. */
export async function run({ type, worktree }: DirectAction): Promise<void> {
  switch (type) {
    case 'open':
      await vscode.commands.executeCommand('vscode.openFolder', vscode.Uri.file(worktree.path), { forceNewWindow: true });
      break;
    case 'terminal':
      vscode.window.createTerminal({ cwd: worktree.path, name: `wtpool ${plain(worktree.name)}` }).show();
      break;
    case 'copyPath':
      await vscode.env.clipboard.writeText(worktree.path);
      break;
    default:
    // A DirectAction has no other type.
  }
}
