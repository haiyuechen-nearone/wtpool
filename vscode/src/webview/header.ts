import type { Pool } from '../generated/status';
import { visible } from '../shared/invisible';
import type { ViewMessage } from '../shared/protocol';
import { bdi, div, setText, span } from './dom';

/** The connection of a watch before its first line, and of a page no view reached yet. */
const STARTING = 'starting…';

/** The activities the chips count, in their order. */
const CHIPS = ['free', 'working', 'waiting'] as const;

type ChipWord = (typeof CHIPS)[number];

/** Gives the header's words for the connection of `view` at `nowMs`. */
function connectionText(view: ViewMessage, nowMs: number): string {
  const since = Math.max(0, Math.floor((nowMs - Date.parse(view.last_line)) / 1000));
  const retry = Math.max(0, Math.ceil((Date.parse(view.retry_at ?? '') - nowMs) / 1000) || 0);
  const text: Readonly<Record<ViewMessage['connection'], string>> = {
    starting: STARTING,
    live: 'live',
    stale: `updated ${since} s ago`,
    paused: `updates paused, retrying in ${retry} s`,
  };
  return text[view.connection];
}

/** Counts the worktrees of `pool` that each chip counts. */
function chipCounts(pool: Pool): Readonly<Record<ChipWord, number>> {
  const count = (word: ChipWord): number =>
    pool.worktrees.filter((w) => w.activity === word).length;
  return { free: count('free'), working: count('working'), waiting: count('waiting') };
}

/** Shows the header: made once, then updated by each view and each tick. */
export class Header {
  readonly element = document.createElement('header');
  private readonly connection = span('connection');
  /** wtpool's error while paused, on its own line under the header. */
  private readonly error = document.createElement('p');
  private readonly errorText = bdi('');
  private readonly chips = new Map<ChipWord, HTMLElement>();

  /** Builds the header. `focusFirstWaiting` runs when the waiting chip is clicked. */
  constructor(focusFirstWaiting: () => void) {
    this.element.className = 'header';
    const title = document.createElement('h1');
    title.className = 'title';
    title.textContent = 'Worktrees';
    const chips = div('chips');
    for (const word of CHIPS) {
      const chip = word === 'waiting' ? document.createElement('button') : span('');
      chip.className = `chip chip-${word}`;
      chip.hidden = true;
      this.chips.set(word, chip);
      chips.append(chip);
    }
    this.chips.get('waiting')?.addEventListener('click', focusFirstWaiting);
    this.error.className = 'wtpool-error';
    this.error.hidden = true;
    this.error.append(this.errorText);
    // Before any view, so a late or lost one never leaves the header blank.
    this.showConnection('starting', STARTING);
    this.element.append(title, chips, this.connection, this.error);
  }

  /** Shows wtpool's error of `view`, and hides the connection with no folder open. */
  show(view: ViewMessage): void {
    const noFolder = view.no_folder === true;
    this.connection.hidden = noFolder;
    const error =
      view.connection === 'paused' && typeof view.error === 'string' && !noFolder
        ? visible(view.error)
        : '';
    setText(this.errorText, error);
    this.error.hidden = error === '';
  }

  /** Shows the counts of `pool` in the chips. A chip of none hides, and no pool hides all. */
  count(pool?: Pool): void {
    const counts = pool ? chipCounts(pool) : undefined;
    for (const [word, chip] of this.chips) {
      const shown = counts?.[word] ?? 0;
      if (counts) setText(chip, `${shown} ${word}`);
      chip.hidden = shown === 0;
    }
  }

  /** Shows the connection of `view` at `nowMs`, the time the stale and paused words count from. */
  tick(view: ViewMessage, nowMs: number): void {
    this.showConnection(view.connection, connectionText(view, nowMs));
  }

  private showConnection(connection: ViewMessage['connection'], text: string): void {
    setText(this.connection, text);
    const className = `connection connection-${connection}`;
    if (this.connection.className !== className) this.connection.className = className;
  }
}
