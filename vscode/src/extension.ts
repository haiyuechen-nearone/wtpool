// The Worktrees extension (status-panel.md, section 3): one command, one webview tab per window
// fed by `wtpool status --watch`, and nothing else: no view, no status bar item, no popup (D1, D3),
// no networking module and no server (X17).
//
// With WTPOOL_VERIFY=1, `activate` returns the test interface of verification.md, "Test interfaces",
// item 3: `fromWebview(message)`, `toWebview()` and `advance(ms)` on a fake clock.
import * as vscode from 'vscode';

import { FakeClock, systemClock } from './host/clock';
import { VIEW_TYPE } from './host/panel';
import { Tabs } from './host/tabs';
import type { PanelMessage } from './shared/protocol';

/** The test interface `activate` returns under WTPOOL_VERIFY=1. */
interface VerifyApi {
  /** Handles `message` as if the current tab's webview posted it. */
  fromWebview(message: unknown): Promise<void>;
  /** Lists every message posted to a webview so far, oldest first. */
  toWebview(): PanelMessage[];
  /** Moves the fake clock `ms` forward. */
  advance(ms: number): Promise<void>;
}

/** Activates the extension, and under WTPOOL_VERIFY=1 gives the verify API. */
export function activate(context: vscode.ExtensionContext): VerifyApi | undefined {
  const verify = process.env.WTPOOL_VERIFY === '1';
  const fake = new FakeClock(Date.now());
  const posted: PanelMessage[] = [];
  const channel = vscode.window.createOutputChannel('Worktrees', { log: true });
  const tabs = new Tabs({
    clock: verify ? fake : systemClock,
    media: vscode.Uri.joinPath(context.extensionUri, 'media'),
    post(webview, message) {
      if (verify) {
        posted.push(message);
      }
      void webview.postMessage(message);
    },
    log(message) {
      channel.info(message);
    },
  });
  // The tabs, and with them their watches, end with the context's subscriptions.
  context.subscriptions.push(
    channel,
    tabs,
    vscode.commands.registerCommand('wtpool.openStatus', () => {
      tabs.open();
    }),
    vscode.window.registerWebviewPanelSerializer(VIEW_TYPE, {
      deserializeWebviewPanel: async (panel) => {
        tabs.restore(panel);
      },
    }),
  );
  if (!verify) return undefined;
  return {
    fromWebview: (message) => tabs.receive(message),
    toWebview: () => [...posted],
    advance: async (ms) => {
      fake.advance(ms);
    },
  };
}
