import * as vscode from 'vscode';

import { plain } from '../shared/invisible';
import type { DirectAction } from './messages';

/**
 * VS Code renders `[label](command:...)` in a notification as a link, so brackets become fullwidth
 * ones. A backslash escape would stay visible and still leave the link pattern intact.
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
      await vscode.commands.executeCommand('vscode.openFolder', vscode.Uri.file(worktree.path), {
        forceNewWindow: true,
      });
      break;
    case 'terminal':
      vscode.window
        .createTerminal({ cwd: worktree.path, name: `wtpool ${plain(worktree.name)}` })
        .show();
      break;
    case 'copyPath':
      await vscode.env.clipboard.writeText(worktree.path);
      break;
    default:
  }
}
