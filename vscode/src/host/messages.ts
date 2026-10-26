// The messages a webview may send (X11), read into the requests a tab acts on. A message counts
// only as a plain object of exactly `type` and `id`, with `id` a string. A row action and a
// `select` name a row of the tab's last document, a `select` the empty string for none (R12), and
// a `follow` an output path that document lists, of the shape R12 reads (X19). A missing worktree
// offers only `copyPath`, and `free` and `fg` need a slot that is there and holds a branch (U4,
// X12). Any other message makes no request.
import type { Output, StatusDocument, Worktree } from '../generated/status';
import type { ConfirmedActionType } from '../shared/confirmation';
import {
  holdsBranch,
  isActionType,
  isRecord,
  offeredActions,
  type ActionType,
  type BranchSlot,
} from '../shared/protocol';
import { isTaskOutputPath } from './outputs';

/** The row actions that run at once. */
type DirectActionType = Exclude<ActionType, ConfirmedActionType>;

/** Asks for `open`, `terminal` or `copyPath` on a row. */
export interface DirectAction {
  type: DirectActionType;
  worktree: Worktree;
}

/** Asks for `free` or `fg` on a slot that holds a branch. */
interface ConfirmedAction {
  type: ConfirmedActionType;
  worktree: BranchSlot;
}

/** Names the selected row, whose outputs the tab then reads (R12). */
interface Selection {
  type: 'select';
  /** The row's path, or null when no row is selected. */
  row: string | null;
}

/** Asks for a Follow terminal of an output the document lists (X19). */
interface FollowRequest {
  type: 'follow';
  output: Output;
  /** The label of the actor that lists the output, the terminal's name. */
  label: string;
}

/** A request of the webview, by its message's `type`. */
export type WebviewRequest = DirectAction | ConfirmedAction | Selection | FollowRequest;

/**
 * Finds the worktree at `path` of the tab's pool: the document's first, the only one it shows
 * (U8).
 */
export function worktreeAt(document: StatusDocument | null, path: unknown): Worktree | undefined {
  return document?.pools[0].worktrees.find((row) => row.path === path);
}

/** Tells a row action that asks before it runs. */
function isConfirmed(type: ActionType): type is ConfirmedActionType {
  return type === 'free' || type === 'fg';
}

/** Reads the row action `type` on row `id`, if that row offers it. */
function actionOf(
  type: ActionType,
  id: string,
  document: StatusDocument | null,
): DirectAction | ConfirmedAction | undefined {
  const worktree = worktreeAt(document, id);
  if (!worktree) {
    return undefined;
  }
  if (isConfirmed(type)) {
    return holdsBranch(worktree) ? { type, worktree } : undefined;
  }
  return offeredActions(worktree).includes(type) ? { type, worktree } : undefined;
}

/** Reads a `select` of row `id`, or of no row for the empty id. */
function selectionOf(id: string, document: StatusDocument | null): Selection | undefined {
  if (id === '') {
    return { type: 'select', row: null };
  }
  return worktreeAt(document, id) ? { type: 'select', row: id } : undefined;
}

/** Reads a `follow` of the output at `id`, with the label of the actor that lists it. */
function followOf(id: string, document: StatusDocument | null): FollowRequest | undefined {
  if (!isTaskOutputPath(id)) {
    return undefined;
  }
  for (const worktree of document?.pools[0].worktrees ?? []) {
    for (const actor of worktree.actors) {
      const output = actor.outputs?.find((listed) => listed.path === id);
      if (output) {
        return { type: 'follow', output, label: actor.label };
      }
    }
  }
  return undefined;
}

/** Reads `message` as the request it makes of the tab with `document`, if it is one (X11). */
export function readRequest(
  message: unknown,
  document: StatusDocument | null,
): WebviewRequest | undefined {
  if (!isRecord(message) || Object.getPrototypeOf(message) !== Object.prototype) {
    return undefined;
  }
  const { type, id } = message;
  if (Object.keys(message).sort().join() !== 'id,type' || typeof id !== 'string') {
    return undefined;
  }
  if (type === 'select') {
    return selectionOf(id, document);
  }
  if (type === 'follow') {
    return followOf(id, document);
  }
  return isActionType(type) ? actionOf(type, id, document) : undefined;
}
