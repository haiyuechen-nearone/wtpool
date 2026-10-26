import * as vscode from 'vscode';

import { TITLE } from '../shared/view';
import { StatusPanel, VIEW_TYPE, webviewOptions, type PanelContext } from './panel';

/** Keeps the Worktrees tabs of this window and the one the command reveals. */
export class Tabs implements vscode.Disposable {
  private readonly panels = new Set<StatusPanel>();
  private current?: StatusPanel;

  constructor(private readonly context: PanelContext) {}

  /** Reveals the current tab where it is, or opens one beside the active group. */
  open(): void {
    if (this.current) {
      this.current.panel.reveal();
      return;
    }
    const panel = vscode.window.createWebviewPanel(
      VIEW_TYPE,
      TITLE,
      { viewColumn: vscode.ViewColumn.Beside, preserveFocus: false },
      { ...webviewOptions(this.context.media), retainContextWhenHidden: false },
    );
    this.adopt(panel);
  }

  /** Takes over a tab VS Code restored. Its saved state belongs to the webview alone. */
  restore(panel: vscode.WebviewPanel): void {
    this.adopt(panel);
  }

  /** Handles a message as if the current tab's webview sent it. */
  receive(message: unknown): Promise<void> {
    return this.current?.receive(message) ?? Promise.resolve();
  }

  /** Closes every tab, and with each its watch. */
  dispose(): void {
    for (const tab of [...this.panels]) {
      tab.panel.dispose();
    }
  }

  private adopt(panel: vscode.WebviewPanel): void {
    const tab = new StatusPanel(panel, this.context, () => {
      this.panels.delete(tab);
      if (this.current === tab) {
        this.current = [...this.panels].at(-1);
      }
    });
    this.panels.add(tab);
    this.current = tab;
  }
}
