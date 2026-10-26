// Both tsconfigs include this file, so it imports only types of the generated schema: no Node,
// VS Code or DOM API.
import type { StatusDocument, Worktree } from '../generated/status';
import type { JobSettingsAnswer, JobSettingsMessage } from './jobs';

/** The state of a tab's watch. `starting` lasts until wtpool's first line. */
export type Connection = 'starting' | 'live' | 'stale' | 'paused';

/**
 * Carries the connection and the last document to the webview. The wire names are frozen, so
 * `last_line`, `retry_at` and `no_folder` keep their snake case.
 */
export interface ViewMessage {
  type: 'view';
  version: 1;
  connection: Connection;
  /** When the last line of `wtpool` arrived, RFC 3339. Before any line, when the tab opened. */
  last_line: string;
  /** The last document, or null before the tab's first one. */
  status: StatusDocument | null;
  /** While paused: when the next start of `wtpool` is due, RFC 3339. */
  retry_at?: string;
  /**
   * While paused after a start that failed: wtpool's last non-empty stderr line, or the reason
   * it did not start.
   */
  error?: string;
  /** No folder is open, so no `wtpool` runs: `connection` is paused, with no `retry_at`. */
  no_folder?: true;
  /** The first folder is not on this computer, so `wtpool` runs in the home folder. */
  folder_remote?: true;
}

/** Holds the tail of one output file: its last lines, or the reason there are none. */
export interface Tail {
  path: string;
  lines: string[];
  error?: string;
}

/** Carries the tails of the selected row's outputs, in the document's order. */
export interface OutputMessage {
  type: 'output';
  version: 1;
  /** The selected row. */
  row: string;
  tails: Tail[];
}

/** A message the extension posts to a webview. */
export type PanelMessage = ViewMessage | OutputMessage | JobSettingsAnswer;

/** The row actions a webview may ask for, in the order a row shows them. */
const ACTION_TYPES = ['open', 'terminal', 'copyPath', 'free', 'fg'] as const;

/** A row action of {@link ACTION_TYPES}. */
export type ActionType = (typeof ACTION_TYPES)[number];

/** Tells a row action of {@link ACTION_TYPES}. */
export function isActionType(value: unknown): value is ActionType {
  return ACTION_TYPES.some((type) => type === value);
}

/**
 * Holds a message a webview may send: a row action or a `select` names a row by its path, the
 * empty string for none in a `select`, and a `follow` names an output path instead.
 */
export interface RowMessage {
  type: ActionType | 'select' | 'follow';
  id: string;
}

/** Tells the extension that a page started, so it posts the tab's view to that page. */
export interface ReadyMessage {
  type: 'ready';
}

/** A message a webview may send. */
export type WebviewMessage = RowMessage | JobSettingsMessage | ReadyMessage;

/** A worktree that holds a branch, the one kind of row that `free` and `fg` act on. */
export interface BranchSlot extends Worktree {
  branch: string;
}

/** Tells a slot that is there and holds a branch: the rows that offer `free` and `fg`. */
export function holdsBranch(worktree: Worktree): worktree is BranchSlot {
  return worktree.role === 'slot' && !worktree.missing && worktree.branch !== null;
}

/**
 * Lists the actions a row offers, in {@link ACTION_TYPES} order: only `copyPath` for a missing
 * worktree, and `free` and `fg` too on a slot that holds a branch.
 */
export function offeredActions(worktree: Worktree): ActionType[] {
  if (worktree.missing) {
    return ['copyPath'];
  }
  return ACTION_TYPES.filter((type) => (type !== 'free' && type !== 'fg') || holdsBranch(worktree));
}

/** Tells an object that is neither null nor an array, so its properties can be read. */
export function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
