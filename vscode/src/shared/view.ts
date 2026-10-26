// A tab's title (X18) and the view it posts (X6, R9, R10), from its document and the state of its
// watch, with no IDE call: host/panel.ts sets the one and posts the other.
import type { StatusDocument } from '../generated/status';
import type { Connection, ViewMessage } from './protocol';

/** The title of a tab while no worktree waits (X18). */
export const TITLE = 'Worktrees';

/** Holds the state of a tab's watch that its view shows. */
interface WatchState {
  readonly connection: Connection;
  /** The time the last line arrived, in clock milliseconds, if one did. */
  readonly lastLine?: number;
  /** The time the next start is due, shown while paused. */
  readonly retryAt: number;
  /** While paused after a start that failed: wtpool's error (R10). */
  readonly error?: string;
}

/**
 * Gives `Worktrees (N)` while N worktrees of the tab's pool, the first, wait, else `Worktrees`
 * (X18).
 */
export function titleOf(document: StatusDocument): string {
  const waiting = document.pools[0].worktrees.filter((row) => row.activity === 'waiting');
  return waiting.length ? `${TITLE} (${waiting.length})` : TITLE;
}

/** Formats clock milliseconds as RFC 3339. */
function isoTime(ms: number): string {
  return new Date(ms).toISOString();
}

/**
 * Builds the view of a tab with `document` and the state of its `watch`, or of no watch when no
 * folder is open (R9). Before any line, `last_line` is the time the tab `opened`.
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
