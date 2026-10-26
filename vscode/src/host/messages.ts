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

interface Selection {
  type: 'select';
  /** The row's path, or null when no row is selected. */
  row: string | null;
}

/** Asks for a Follow terminal of an output the document lists. */
interface FollowRequest {
  type: 'follow';
  output: Output;
  /** The label of the actor that lists the output, the terminal's name. */
  label: string;
}

/** A request of the webview, by its message's `type`. */
export type WebviewRequest = DirectAction | ConfirmedAction | Selection | FollowRequest;

/** Finds the worktree at `path` in the document's first pool, the only one the tab shows. */
export function worktreeAt(document: StatusDocument | null, path: unknown): Worktree | undefined {
  return document?.pools[0].worktrees.find((row) => row.path === path);
}

function isConfirmed(type: ActionType): type is ConfirmedActionType {
  return type === 'free' || type === 'fg';
}

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

function selectionOf(id: string, document: StatusDocument | null): Selection | undefined {
  if (id === '') {
    return { type: 'select', row: null };
  }
  return worktreeAt(document, id) ? { type: 'select', row: id } : undefined;
}

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

/** Reads `message` as a request to the tab with `document`, if it is a valid one. */
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
