import type { StatusDocument } from '../generated/status';
import type { Connection, ViewMessage } from './protocol';

/** The title of a tab while no worktree waits. */
export const TITLE = 'Worktrees';

/** Holds the state of a tab's watch that its view shows. */
interface WatchState {
  readonly connection: Connection;
  /** The time the last line arrived, in clock milliseconds, if one did. */
  readonly lastLine?: number;
  /** The time the next start is due, shown while paused. */
  readonly retryAt: number;
  /** While paused after a start that failed: wtpool's error. */
  readonly error?: string;
}

/**
 * Gives `Worktrees (N)` while N worktrees of the document's first pool wait, else {@link TITLE}.
 */
export function titleOf(document: StatusDocument): string {
  const waiting = document.pools[0].worktrees.filter((row) => row.activity === 'waiting');
  return waiting.length ? `${TITLE} (${waiting.length})` : TITLE;
}

function isoTime(ms: number): string {
  return new Date(ms).toISOString();
}

/**
 * Builds the view of a tab with `document` and the state of its `watch`, or of no watch when no
 * folder is open. Before any line, `last_line` is the time the tab `opened`.
 */
export function viewOf(
  watch: WatchState | undefined,
  opened: number,
  document: StatusDocument | null,
): ViewMessage {
  const message: ViewMessage = {
    type: 'view',
    version: 1,
    connection: watch?.connection ?? 'paused',
    last_line: isoTime(watch?.lastLine ?? opened),
    status: document,
  };
  if (!watch) {
    message.no_folder = true;
  } else if (watch.connection === 'paused') {
    message.retry_at = isoTime(watch.retryAt);
    if (watch.error !== undefined) {
      message.error = watch.error;
    }
  }
  return message;
}
