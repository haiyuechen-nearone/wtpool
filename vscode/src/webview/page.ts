import type { StatusDocument, Worktree } from '../generated/status';
import { flagsUnqueued, jobsOf, type JobSettingsAnswer } from '../shared/jobs';
import {
  isActionType,
  isRecord,
  type OutputMessage,
  type ViewMessage,
  type WebviewMessage,
} from '../shared/protocol';
import { DetailStrip } from './detail';
import { cell, div, paragraph, textSpan } from './dom';
import { Header } from './header';
import { PoolHeading } from './heading';
import { MachineStrip } from './machine_strip';
import { KeyedCache, placeInOrder } from './order';
import { RowView } from './row';
import { rowOrder, waitAnnouncement } from './text';

/** The columns of the table: the header text and the key of the `col-` class. */
const COLUMNS = [
  { label: 'Activity', key: 'activity' },
  { label: 'Worktree', key: 'worktree' },
  { label: 'Branch', key: 'branch' },
  { label: 'Git', key: 'git' },
  { label: 'Agent · doing', key: 'agent' },
  { label: 'Last', key: 'last' },
  { label: '', key: 'actions' },
] as const;
const NO_FOLDER = 'Open a folder inside a git repository to see its worktrees.';
const AGENTS_OFF =
  'No agents shown: agent activity is off for this pool. ' +
  'To turn it on, run wtpool setup in the pool, ' +
  'then add the hooks: in Claude Code, run /plugin marketplace add haiyuechen-nearone/wtpool and ' +
  '/plugin install wtpool@wtpool, ' +
  'or add the block that wtpool hook install prints to your Claude Code settings.';
/** The longest selected row id a saved state may hold. Anything longer is not a path. */
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

function hiddenNote(className: string, text: string): HTMLParagraphElement {
  const note = paragraph(className);
  note.textContent = text;
  note.hidden = true;
  return note;
}

/** Shows the tab's page: made once, then updated by each view, output message and tick. */
export class Page {
  private readonly header = new Header(() => void this.focusFirstWaiting());
  private readonly noFolder = hiddenNote('no-folder', NO_FOLDER);
  private readonly agentsOff = hiddenNote('agents-off', AGENTS_OFF);
  private readonly scroller = div('scroller');
  private readonly tbody = document.createElement('tbody');
  /** The pool's heading row, hidden until the first document. */
  private readonly heading = new PoolHeading(COLUMNS.length, () => void this.toggleMachine());
  /** The machine strip under the heading, in the table only while it is open. */
  private readonly machine: MachineStrip;
  private readonly detail = document.createElement('tr');
  private readonly detailCell = cell('detail-cell');
  private readonly strip = new DetailStrip();
  private readonly announcer = div('visually-hidden');
  private readonly rows = new KeyedCache<RowView>();
  /** The rows in their shown order. */
  private order: RowView[] = [];
  private view: ViewMessage | null = null;
  private waiting: Set<string> | null = null;
  private selected: string | null;
  /** The selection the extension last heard of: none at first, as a new tab's host has none. */
  private told: string | null = null;
  /** The time of the last view or tick, so a strip opened between them agrees with its row. */
  private nowMs = 0;

  constructor(private readonly host: Host) {
    this.selected = savedSelection(host.getState());
    this.detail.className = 'detail';
    this.detailCell.colSpan = COLUMNS.length;
    this.detailCell.append(this.strip.element);
    this.detail.append(this.detailCell);
    this.announcer.setAttribute('role', 'status');
    this.announcer.setAttribute('aria-live', 'polite');
    this.machine = new MachineStrip(COLUMNS.length, {
      send: (message) => void host.postMessage(message),
      announce: (text) => {
        this.announcer.textContent = text;
      },
      close: () => {
        this.toggleMachine();
        this.heading.button.focus();
      },
    });
    document.body.append(
      this.header.element,
      this.noFolder,
      this.agentsOff,
      this.table(),
      this.announcer,
    );
  }

  /** Makes the table with its columns, header row and the pool's heading row, in its scroller. */
  private table(): HTMLElement {
    const table = document.createElement('table');
    table.className = 'grid';
    const colgroup = document.createElement('colgroup');
    const head = document.createElement('tr');
    for (const { label, key } of COLUMNS) {
      const col = document.createElement('col');
      col.className = `col-${key}`;
      colgroup.append(col);
      const th = document.createElement('th');
      th.setAttribute('scope', 'col');
      th.textContent = label;
      head.append(th);
    }
    head.lastElementChild?.append(textSpan('visually-hidden', 'Actions'));
    const thead = document.createElement('thead');
    thead.append(head);
    this.heading.element.hidden = true;
    this.tbody.append(this.heading.element);
    this.tbody.addEventListener('click', (event) => void this.activate(event));
    this.tbody.addEventListener('keydown', (event) => void this.activate(event));
    this.tbody.addEventListener('focusin', (event) => {
      if (event.target instanceof Element) this.reveal(event.target);
    });
    table.append(colgroup, thead, this.tbody);
    // A tab narrower than the table scrolls it sideways here, so no cell is cut.
    this.scroller.append(table);
    return this.scroller;
  }

  /**
   * Shows a view message. Without a document only the header shows, over the empty table, and with
   * no folder open the no folder note replaces the table.
   */
  render(view: ViewMessage, nowMs: number): void {
    this.view = view;
    this.nowMs = nowMs;
    const noFolder = view.no_folder === true;
    this.noFolder.hidden = !noFolder;
    this.scroller.hidden = noFolder;
    this.header.show(view);
    if (view.status) this.renderStatus(view.status, view.folder_remote === true, nowMs);
    else this.clear();
    this.tick(nowMs);
  }

  /** Shows the document's first pool, the one pool wtpool sends, and this machine's heavy jobs. */
  private renderStatus(status: StatusDocument, remote: boolean, nowMs: number): void {
    const [pool] = status.pools;
    const jobs = jobsOf(status);
    this.heading.update(pool, status.machine.name, jobs);
    this.heading.element.hidden = false;
    this.machine.show(
      { name: status.machine.name, jobs, worktrees: pool.worktrees, remote },
      nowMs,
    );
    this.agentsOff.hidden = pool.agents !== 'off';
    this.header.count(pool);
    this.renderRows(rowOrder(pool.worktrees), flagsUnqueued(jobs), nowMs);
    this.announce(pool.worktrees);
  }

  /** Clears the document from the page. */
  private clear(): void {
    this.rows.forEach((row) => void row.element.remove());
    this.rows.clear();
    this.heading.element.hidden = true;
    if (this.machine.element.isConnected) this.toggleMachine();
    this.order = [];
    this.detail.remove();
    this.waiting = null;
    this.agentsOff.hidden = true;
    this.header.count();
  }

  /** Moves the clock: ages, run times and the stale or paused header, nothing else. */
  tick(nowMs: number): void {
    this.nowMs = nowMs;
    this.rows.forEach((row) => void row.tick(nowMs));
    this.strip.tick(nowMs);
    this.machine.tick(nowMs);
    if (!this.view || this.view.no_folder) return;
    this.header.tick(this.view, nowMs);
  }

  /**
   * Shows the pool's rows after its heading and the machine strip, kept and moved, never remade, so
   * focus survives.
   */
  private renderRows(ordered: Worktree[], flagUnqueued: boolean, nowMs: number): void {
    this.rows.retain(new Set(ordered.map((w) => w.path)), (row) => void row.element.remove());
    this.order = ordered.map((worktree) => {
      const row = this.rows.get(worktree.path);
      if (row) {
        row.update(worktree, nowMs, flagUnqueued);
        return row;
      }
      const made = new RowView(worktree, nowMs, flagUnqueued);
      this.rows.set(worktree.path, made);
      return made;
    });
    const anchor = this.machine.element.isConnected ? this.machine.element : this.heading.element;
    placeInOrder(
      anchor,
      this.order.flatMap((row) =>
        row.id === this.selected ? [row.element, this.detail] : [row.element],
      ),
    );
    this.select(this.selected !== null && this.rows.has(this.selected) ? this.selected : null);
  }

  /** Announces the worktrees that started to wait since the last document. */
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
      if (row.element.getAttribute('aria-selected') !== value) {
        row.element.setAttribute('aria-selected', value);
      }
    }
    const row = id === null ? undefined : this.rows.get(id);
    if (row) {
      if (row.element.nextElementSibling !== this.detail) row.element.after(this.detail);
      this.strip.update(row.worktree, this.nowMs);
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

  /** Shows the tails the extension read for the selected row's outputs. */
  showOutput(message: OutputMessage): void {
    if (message.row === this.selected) this.strip.showTails(message.row, message.tails);
  }

  /** Shows the extension's answer to the settings the machine strip sent. */
  showJobSettings(answer: JobSettingsAnswer): void {
    this.machine.answer(answer);
  }

  /** Opens the machine strip under the heading, focus on its first field, or closes it. */
  private toggleMachine(): void {
    const open = !this.machine.element.isConnected;
    if (open) {
      this.heading.element.after(this.machine.element);
      this.machine.focus();
    } else {
      this.machine.element.remove();
    }
    this.heading.expand(open);
  }

  /** Opens the detail strip under `row`, or closes it when it is open there. */
  private toggle(row: Element): void {
    const id = row.getAttribute('data-row-id');
    this.select(id === this.selected ? null : id);
  }

  /**
   * Handles a click, and Enter or Space on a focused row, the same way: a row action posts
   * `{type, id}`, a row toggles its detail strip. A key on a button is left to the button,
   * whose own click follows.
   */
  private activate(event: MouseEvent | KeyboardEvent): void {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const follow = target.closest('button[data-follow]');
    if (follow) {
      const output = follow.getAttribute('data-follow');
      if (event.type === 'click' && output !== null) {
        this.host.postMessage({ type: 'follow', id: output });
      }
      return;
    }
    const key = event instanceof KeyboardEvent ? event.key : undefined;
    if (key !== undefined && !(target.matches('tr[data-row-id]') && ['Enter', ' '].includes(key))) {
      return;
    }
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
    if (target.matches('tr[data-row-id]')) {
      target.scrollIntoView({ block: 'nearest', inline: 'start' });
    }
    if (target.matches('button[data-action]')) {
      target.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    }
  }

  private focusFirstWaiting(): void {
    const first = this.order.find((row) => row.worktree.activity === 'waiting');
    first?.element.focus();
    first?.element.scrollIntoView({ block: 'nearest', inline: 'start' });
  }
}
