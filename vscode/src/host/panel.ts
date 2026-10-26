import { randomBytes } from 'node:crypto';
import * as vscode from 'vscode';

import type { StatusDocument } from '../generated/status';
import { buildHtml } from '../html';
import type { OutputMessage, PanelMessage } from '../shared/protocol';
import { TITLE, titleOf, viewOf } from '../shared/view';
import { run, showError } from './actions';
import { systemClock, type Clock } from './clock';
import { confirm, type ConfirmContext } from './confirm';
import { assertIsError } from './errors';
import { workFolder } from './folder';
import { follow } from './follow';
import { readRequest, worktreeAt, type WebviewRequest } from './messages';
import { OutputWatch } from './outputs';
import { WatchStream, type DocumentUpdate } from './stream';

/** The view type of the Worktrees tab, the key of its serializer. */
export const VIEW_TYPE = 'wtpool.status';

/** Gives a tab the parts of the window it uses: the clock, the media folder, a post and a log. */
export interface PanelContext {
  clock: Clock;
  /** vscode/media, the only local resource root. */
  media: vscode.Uri;
  post(webview: vscode.Webview, message: PanelMessage): void;
  log(message: string): void;
}

/** Gives the webview options of a created and of a restored tab. */
export function webviewOptions(media: vscode.Uri): vscode.WebviewOptions {
  return { enableScripts: true, enableCommandUris: false, enableForms: false, localResourceRoots: [media] };
}

/**
 * Gives the tab's icon: the codicon worktree glyph in VS Code's tab icon greys, from media/. The
 * light one serves the light and high contrast light themes, the dark one the dark and high
 * contrast.
 */
function tabIcon(media: vscode.Uri): { light: vscode.Uri; dark: vscode.Uri } {
  return { light: vscode.Uri.joinPath(media, 'icon-light.svg'), dark: vscode.Uri.joinPath(media, 'icon-dark.svg') };
}

/** Gives the `wtpool` binary: the application scoped `wtpool.path`, read at each start. */
function binary(): string {
  const value = vscode.workspace.getConfiguration('wtpool').get<unknown>('path');
  return typeof value === 'string' && value !== '' ? value : 'wtpool';
}

/**
 * Runs one Worktrees tab: its watch, its tails and the requests of its webview. A hidden tab keeps
 * its watch and its title but posts nothing, and showing it posts the last view at once.
 */
export class StatusPanel {
  /** The tab's last document. A new tab starts without one and never borrows another's. */
  private document: StatusDocument | null = null;
  /** When the tab opened, or its folder last changed: `last_line` before any line. */
  private opened: number;
  private visible: boolean;
  /** The folder `wtpool` runs in, undefined with no folder open. */
  private folder?: string;
  /** The watch in `folder`, none with no folder open. */
  private stream?: WatchStream;
  /** The row the webview last said it selected. The tab watches its outputs. */
  private selected: string | null = null;
  /** The tails of the selected row. They run on the system clock, as a file grows in real time. */
  private readonly outputs: OutputWatch;
  private readonly subscriptions: vscode.Disposable[];
  private readonly confirmContext: ConfirmContext = {
    latest: (path) => worktreeAt(this.document, path),
    binary,
    cwd: () => this.folder,
    log: (message) => {
      this.context.log(message);
    },
  };

  constructor(
    readonly panel: vscode.WebviewPanel,
    private readonly context: PanelContext,
    onDispose: () => void,
  ) {
    // A created and a restored tab both come here, so both get the icon.
    panel.iconPath = tabIcon(context.media);
    panel.webview.options = webviewOptions(context.media);
    panel.webview.html = buildHtml(panel.webview, randomBytes(16).toString('base64'));
    this.visible = panel.visible;
    this.opened = context.clock.now();
    this.folder = workFolder();
    this.outputs = new OutputWatch((message) => {
      this.postOutput(message);
    }, systemClock);
    this.subscriptions = [
      panel.webview.onDidReceiveMessage((message: unknown) => this.receive(message)),
      panel.onDidChangeViewState(() => {
        const shown = panel.visible && !this.visible;
        this.visible = panel.visible;
        if (shown) {
          this.post();
        }
        // A hidden tab watches no output. A shown one reloads its webview, whose `select` follows.
        if (!panel.visible) {
          this.outputs.stop();
        }
      }),
      panel.onDidDispose(() => {
        this.stream?.stop();
        this.outputs.stop();
        for (const subscription of this.subscriptions) {
          subscription.dispose();
        }
        onDispose();
      }),
      // VS Code restarts the extensions when the first folder changes, unless a debug session, an
      // open custom editor or a running chat request vetoes the restart: then this tab lives on
      // and follows the folder here. A change of another folder leaves the tab as it is.
      vscode.workspace.onDidChangeWorkspaceFolders(() => {
        this.refolder();
      }),
    ];
    this.stream = this.watch();
    // The tab names at once the thing it waits for: the first line of its starting watch, or with
    // no folder open a folder. VS Code restarts the extensions when the last folder goes, so a
    // restored tab comes here again and says it too.
    this.post();
  }

  /** Acts on a webview message or ignores it. A failed request shows and logs its error. */
  async receive(message: unknown): Promise<void> {
    const request = readRequest(message, this.document);
    if (!request) {
      return;
    }
    try {
      await this.act(request);
    } catch (error: unknown) {
      this.report(request, error);
    }
  }

  /** Carries out `request`: a row action through {@link run}, or after asking, {@link confirm}. */
  private async act(request: WebviewRequest): Promise<void> {
    switch (request.type) {
      case 'select':
        this.selected = request.row;
        this.outputs.set(this.document, request.row, this.visible, true);
        break;
      case 'follow':
        follow(request.output.path, request.label, request.output.summary);
        break;
      case 'free':
      case 'fg':
        // The modal outlives the message, and confirm shows its own errors, a failed spawn included.
        void confirm(this.confirmContext, request.type, request.worktree).catch((error: unknown) => void this.report(request, error));
        break;
      case 'open':
      case 'terminal':
      case 'copyPath':
        await run(request);
        break;
      default:
    }
  }

  /** Starts a watch in the tab's folder, or none with no folder open. */
  private watch(): WatchStream | undefined {
    if (this.folder === undefined) {
      return undefined;
    }
    const stream = new WatchStream(this.context.clock, binary, this.folder, {
      changed: (update) => void this.changed(update),
      log: (message) => void this.context.log(message),
    });
    stream.start();
    return stream;
  }

  /** Takes a document, or a change of the connection, from the tab's watch. */
  private changed(update: DocumentUpdate | undefined): void {
    if (update) {
      this.document = update.document;
      this.panel.title = titleOf(update.document);
    }
    this.post();
    // A new document may list other outputs for the selected row, or none.
    if (update) {
      this.outputs.set(this.document, this.selected, this.visible);
    }
  }

  /** Posts the selected row's tails, only while the tab is visible. */
  private postOutput(message: OutputMessage): void {
    if (this.visible) {
      this.context.post(this.panel.webview, message);
    }
  }

  /** Posts the tab's view, only while the tab is visible. */
  private post(): void {
    if (this.visible) {
      this.context.post(this.panel.webview, viewOf(this.stream, this.opened, this.document));
    }
  }

  /** Logs and shows the failure of `request`. */
  private report(request: WebviewRequest, error: unknown): void {
    assertIsError(error);
    this.context.log(`${request.type}: ${error.message}`);
    showError(`${request.type} failed: ${error.message}`);
  }

  /**
   * Follows a change of the workspace folders. When the first one is another folder, the old
   * pool's document and title go and a watch starts in the new folder, or none runs when no
   * folder is left.
   */
  private refolder(): void {
    const folder = workFolder();
    if (folder === this.folder) {
      return;
    }
    this.stream?.stop();
    this.folder = folder;
    this.document = null;
    this.selected = null;
    this.outputs.stop();
    this.opened = this.context.clock.now();
    this.panel.title = TITLE;
    this.stream = this.watch();
    this.post();
  }
}
