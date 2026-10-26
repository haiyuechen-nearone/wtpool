// The page of the tab: the header (header.ts), the no folder note (R9), the agents off note (S8),
// the table with the pool's heading row and one row per worktree (U1, U2, U8), the detail strip
// (U5) with the Output section of its agents (U9), the live region for new waits (X14), and the
// selection that getState keeps (X2) and `select` tells the extension, so it posts the selected
// row's output tails (R12). The tab shows one pool, the document's first: wtpool sends
// exactly one (S1), and any later pool is ignored.
import type { StatusDocument, Worktree } from '../generated/status';
import {
  isActionType,
  isRecord,
  type OutputMessage,
  type ViewMessage,
  type WebviewMessage,
} from '../shared/protocol';
import { DetailStrip } from './detail';
import { cell, div, span } from './dom';
import { Header } from './header';
import { PoolHeading } from './heading';
import { RowView } from './row';
import { rowOrder, waitAnnouncement } from './text';

const COLUMNS = ['Activity', 'Worktree', 'Branch', 'Git', 'Agent · doing', 'Last'];
const NO_FOLDER = 'Open a folder inside a git repository to see its worktrees.';
const AGENTS_OFF =
  'No agents shown: agent activity is off for this pool. To turn it on, run wtpool setup in the pool, ' +
  'then add the hooks: in Claude Code, run /plugin marketplace add haiyuechen-nearone/wtpool and ' +
  '/plugin install wtpool@wtpool, or add the block that wtpool hook install prints to your Claude Code settings.';
/** The longest selected row id a saved state may hold; anything longer is not a path. */
const MAX_ID = 4096;

/** Holds the webview API of VS Code that the page uses. */
export interface Host {
  postMessage(message: WebviewMessage): void;
  getState(): unknown;
  setState(state: { selected: string | null }): void;
}

/** Gives the selected row of a saved state, or null when the state holds nothing usable. */
function savedSelection(state: unknown): string | null {
  if (!isRecord(state)) return null;
  const selected = Object.hasOwn(state, 'selected') ? state.selected : null;
  return typeof selected === 'string' && selected.length <= MAX_ID ? selected : null;
}

/** Shows the tab's page: made once, then updated by each view, output message and tick. */
export class Page {
  private readonly header = new Header(() => this.focusFirstWaiting());
  private readonly noFolder = document.createElement('p');
  private readonly note = document.createElement('p');
  private readonly scroller = div('scroller');
  private readonly tbody = document.createElement('tbody');
  /** The pool's heading row (U8), hidden until the first document. */
  private readonly heading = new PoolHeading(COLUMNS.length + 1);
  private readonly detail = document.createElement('tr');
  private readonly detailCell = cell('detail-cell');
  /** The strip in the detail row, made once and updated in place (U5, X15). */
  private readonly strip = new DetailStrip();
  private readonly announcer = div('visually-hidden');
  private readonly rows = new Map<string, RowView>();
  /** The rows in their shown order. */
  private order: RowView[] = [];
  private view: ViewMessage | null = null;
  private waiting: Set<string> | null = null;
  private selected: string | null;
  /** The selection the extension last heard of: none at first, as a new tab's host has none (R12). */
  private told: string | null = null;

  constructor(private readonly host: Host) {
    this.selected = savedSelection(host.getState());
    this.detail.className = 'detail';
    this.detailCell.colSpan = COLUMNS.length + 1;
    this.detailCell.append(this.strip.element);
    this.detail.append(this.detailCell);
    this.announcer.setAttribute('role', 'status');
    this.announcer.setAttribute('aria-live', 'polite');
    document.body.append(this.header.element, this.notes(), this.agentsOff(), this.table(), this.announcer);
  }

  /** Makes the no folder note (R9), hidden until a view says no folder is open. */
  private notes(): HTMLElement {
    this.noFolder.className = 'no-folder';
    this.noFolder.textContent = NO_FOLDER;
    this.noFolder.hidden = true;
    return this.noFolder;
  }

  /** Makes the agents off note (S8), hidden until a pool has agent activity off. */
  private agentsOff(): HTMLElement {
    this.note.className = 'agents-off';
    this.note.textContent = AGENTS_OFF;
    this.note.hidden = true;
    return this.note;
  }

  /** Makes the table with its columns, header row and the pool's heading row, in its scroller. */
  private table(): HTMLElement {
    const table = document.createElement('table');
    table.className = 'grid';
    const colgroup = document.createElement('colgroup');
    const head = document.createElement('tr');
    for (const name of [...COLUMNS.map((column) => column.toLowerCase().split(' ')[0]), 'actions']) {
      const col = document.createElement('col');
      col.className = `col-${name}`;
      colgroup.append(col);
    }
    for (const label of [...COLUMNS, '']) {
      const th = document.createElement('th');
      th.setAttribute('scope', 'col');
      th.textContent = label;
      head.append(th);
    }
    const actionsLabel = span('visually-hidden');
    actionsLabel.textContent = 'Actions';
    head.lastElementChild?.append(actionsLabel);
    const thead = document.createElement('thead');
    thead.append(head);
    this.heading.element.hidden = true;
    this.tbody.append(this.heading.element);
    this.tbody.addEventListener('click', (event) => this.activate(event));
    this.tbody.addEventListener('keydown', (event) => this.activate(event));
    this.tbody.addEventListener('focusin', (event) => {
      if (event.target instanceof Element) this.reveal(event.target);
    });
    table.append(colgroup, thead, this.tbody);
    // A tab narrower than the table scrolls it sideways here, so no cell is cut (U1).
    this.scroller.append(table);
    return this.scroller;
  }

  /**
   * Shows a view message: the header with wtpool's error while paused, and with a document the note,
   * chips, rows in place, detail strip and live region. Without a document only the header shows,
   * over the empty table, and with no folder open the no folder note instead of the table.
   */
  render(view: ViewMessage, nowMs: number): void {
    this.view = view;
    const noFolder = view.no_folder === true;
    this.noFolder.hidden = !noFolder;
    this.scroller.hidden = noFolder;
    this.header.show(view);
    if (view.status) this.renderStatus(view.status, nowMs);
    else this.clear();
    this.tick(nowMs);
  }

  /** Shows a document: the pool's heading, note, chips, rows in place and live region. */
  private renderStatus(status: StatusDocument, nowMs: number): void {
    const [pool] = status.pools;
    this.heading.update(pool, status.machine.name);
    this.heading.element.hidden = false;
    this.note.hidden = pool.agents !== 'off';
    this.header.count(pool);
    this.renderRows(rowOrder(pool.worktrees), nowMs);
    this.announce(pool.worktrees);
  }

  /** Clears the document from the page: its folder changed or none is open (R9). */
  private clear(): void {
    this.rows.forEach((row) => row.element.remove());
    this.rows.clear();
    this.heading.element.hidden = true;
    this.order = [];
    this.detail.remove();
    this.waiting = null;
    this.note.hidden = true;
    this.header.count();
  }

  /** Moves the clock: ages and the stale or paused header, nothing else (U7, B7). */
  tick(nowMs: number): void {
    this.rows.forEach((row) => row.tick(nowMs));
    if (!this.view || this.view.no_folder) return;
    this.header.tick(this.view, nowMs);
  }

  /** Shows the pool's rows in U2 order after its heading, kept and moved, never remade (X15). */
  private renderRows(ordered: Worktree[], nowMs: number): void {
    const ids = new Set(ordered.map((w) => w.path));
    for (const [id, row] of this.rows) {
      if (!ids.has(id)) {
        row.element.remove();
        this.rows.delete(id);
      }
    }
    let previous: Element = this.heading.element;
    this.order = [];
    for (const worktree of ordered) {
      let row = this.rows.get(worktree.path);
      if (row) {
        row.update(worktree, nowMs);
      } else {
        row = new RowView(worktree, nowMs);
        this.rows.set(worktree.path, row);
      }
      this.order.push(row);
      if (previous.nextElementSibling !== row.element) previous.after(row.element);
      previous = row.element;
      if (worktree.path === this.selected) {
        if (previous.nextElementSibling !== this.detail) previous.after(this.detail);
        previous = this.detail;
      }
    }
    this.select(this.selected !== null && this.rows.has(this.selected) ? this.selected : null);
  }

  /** Announces the worktrees that started to wait since the last document (X14). */
  private announce(worktrees: Worktree[]): void {
    const waiting = worktrees.filter((w) => w.activity === 'waiting');
    const fresh = waiting.filter((w) => this.waiting !== null && !this.waiting.has(w.path));
    this.waiting = new Set(waiting.map((w) => w.path));
    if (fresh.length) this.announcer.textContent = waitAnnouncement(rowOrder(fresh));
  }

  /** Selects a row, or none, and keeps the detail strip under it and the state in step. */
  private select(id: string | null): void {
    for (const row of this.rows.values()) {
      const value = String(row.id === id);
      if (row.element.getAttribute('aria-selected') !== value) row.element.setAttribute('aria-selected', value);
    }
    const row = id === null ? undefined : this.rows.get(id);
    if (row) {
      if (row.element.nextElementSibling !== this.detail) row.element.after(this.detail);
      this.strip.update(row.worktree);
    } else {
      this.detail.remove();
    }
    if (id !== this.selected) {
      this.selected = id;
      this.host.setState({ selected: id });
    }
    if (id !== this.told) {
      this.told = id;
      this.host.postMessage({ type: 'select', id: id ?? '' });
    }
  }

  /** Shows the tails the extension read for the selected row's outputs (R12, U9). */
  showOutput(message: OutputMessage): void {
    if (message.row === this.selected) this.strip.showTails(message.row, message.tails);
  }

  /** Opens the detail strip under `row`, or closes it when it is open there. */
  private toggle(row: Element): void {
    const id = row.getAttribute('data-row-id');
    this.select(id === this.selected ? null : id);
  }

  /**
   * Handles a click, and Enter or Space on a focused row, the same way (X14): a row action posts
   * `{type, id}`, a row toggles its detail strip. A key on a button is left to the button,
   * whose own click follows.
   */
  private activate(event: MouseEvent | KeyboardEvent): void {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const follow = target.closest('button[data-follow]');
    if (follow) {
      const output = follow.getAttribute('data-follow');
      if (event.type === 'click' && output !== null) this.host.postMessage({ type: 'follow', id: output });
      return;
    }
    const key = event instanceof KeyboardEvent ? event.key : undefined;
    if (key !== undefined && !(target.matches('tr[data-row-id]') && ['Enter', ' '].includes(key))) return;
    if (key !== undefined) event.preventDefault();
    const button = target.closest('button[data-action]');
    const row = target.closest('tr[data-row-id]');
    if (button && row) {
      const type = button.getAttribute('data-action');
      const id = row.getAttribute('data-row-id');
      if (isActionType(type) && id !== null) this.host.postMessage({ type, id });
    }
    if (!button && row) this.toggle(row);
  }

  /**
   * Scrolls the element keyboard focus reaches into view in a tab narrower than the table: a
   * row's start, so its activity and name show, or the whole button with its focus ring, as the
   * browser's own focus scroll leaves it cut when the button was partly in view. A click scrolls
   * nothing, so the row under the pointer stays put.
   */
  private reveal(target: Element): void {
    if (!target.matches(':focus-visible')) return;
    if (target.matches('tr[data-row-id]')) target.scrollIntoView({ block: 'nearest', inline: 'start' });
    if (target.matches('button[data-action]')) target.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }

  /** Focuses the first waiting row and scrolls it into view (U3). */
  private focusFirstWaiting(): void {
    const first = this.order.find((row) => row.worktree.activity === 'waiting');
    first?.element.focus();
    first?.element.scrollIntoView({ block: 'nearest', inline: 'start' });
  }
}
