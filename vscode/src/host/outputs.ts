// The live tails of one tab (R12): for the actors of the selected row only, the tail of each
// output file the last document lists there (S13), read again when the file changes, through
// fs.watch with a {@link POLL_MS} poll for changes the watch misses, and posted at most once per
// {@link POST_EVERY_MS}, only while the tab is visible. It reads no path the document does not
// list, and of those only a task's output file (H27), and stops watching when the row is
// deselected, a task leaves the document, or the tab is hidden or closed.
import { promises as fs, watch, type FSWatcher } from 'node:fs';

import type { Output, StatusDocument } from '../generated/status';
import type { OutputMessage, Tail } from '../shared/protocol';
import { systemClock, type Cancel, type Clock } from './clock';
import { readTail } from './tail';

/** The fewest milliseconds between two posts of one tab's tails: at most 4 a second. */
const POST_EVERY_MS = 250;
/** The interval between two checks of a watched file for a change the watch missed. */
export const POLL_MS = 1000;

/** The path separators of this system: Windows takes `\` and `/` alike. */
const SEPARATOR = process.platform === 'win32' ? '[\\\\/]' : '/';
/** The end of a task's output path (H27): `tasks/<id>.output`, the id as the hook accepts it. */
const TASK_OUTPUT = new RegExp(`${SEPARATOR}tasks${SEPARATOR}[A-Za-z0-9_-]{1,64}\\.output$`);
/** The tail of a listed path that is not a task's output file: the tab never reads it (R12). */
const NOT_A_TASK_OUTPUT =
  'the output path does not end in tasks/<id>.output, so the panel does not read it';

/** Tells a path of the shape of a task's output file, the only paths the tab reads (R12, X19). */
export function isTaskOutputPath(path: string): boolean {
  return TASK_OUTPUT.test(path);
}

/**
 * Lists the outputs that the actors of row `id` list in `document`, each path once, in the
 * document's order.
 */
function listedOutputs(document: StatusDocument | null, id: string | null): Output[] {
  const row = document?.pools[0].worktrees.find((worktree) => worktree.path === id);
  const seen = new Set<string>();
  const outputs: Output[] = [];
  for (const actor of row?.actors ?? []) {
    for (const output of actor.outputs ?? []) {
      if (!seen.has(output.path)) {
        seen.add(output.path);
        outputs.push(output);
      }
    }
  }
  return outputs;
}

/** One watched file: its watch, its poll, the state it was last read at and its last tail. */
interface Watched {
  /** False for a path {@link isTaskOutputPath} refuses, whose tail says so and is never read. */
  readable: boolean;
  watcher?: FSWatcher;
  size: number;
  modified: number;
  tail?: Tail;
  reading: boolean;
  again: boolean;
}

/** Reads and posts the tails of the selected row's outputs while the tab is visible (R12). */
export class OutputWatch {
  private row: string | null = null;
  /** The watched paths in the document's order, the order of the tails. */
  private paths: string[] = [];
  private readonly files = new Map<string, Watched>();
  private cancelPoll?: Cancel;
  private cancelPost?: Cancel;
  private lastPost = -Infinity;
  private pending = false;

  constructor(
    private readonly post: (message: OutputMessage) => void,
    private readonly clock: Clock = systemClock,
  ) {}

  /**
   * Watches the outputs that `document` lists for row `id`, or nothing when `id` is null or the
   * tab is not `visible`. `fresh` posts the tails again even when they did not change, as a
   * webview that just loaded needs them.
   */
  set(document: StatusDocument | null, id: string | null, visible: boolean, fresh = false): void {
    const paths = visible && id !== null ? listedOutputs(document, id).map((output) => output.path) : [];
    const rowChanged = id !== this.row;
    this.row = paths.length ? id : null;
    this.paths = paths;
    for (const [path, file] of this.files) {
      if (!paths.includes(path) || rowChanged) {
        this.unwatch(path, file);
      }
    }
    if (!paths.length) {
      this.stop();
      return;
    }
    const added = paths.filter((path) => !this.files.has(path));
    for (const path of added) {
      this.watch(path);
    }
    // New files post once read; a webview that asks again gets the tails it has not seen.
    if (fresh && !added.length) this.schedule();
    this.cancelPoll?.();
    this.cancelPoll = this.clock.after(POLL_MS, () => this.poll());
  }

  /** Stops every watch and timer: no row, a hidden tab, or a closed one. */
  stop(): void {
    for (const [path, file] of this.files) {
      this.unwatch(path, file);
    }
    this.row = null;
    this.paths = [];
    this.pending = false;
    this.cancelPoll?.();
    this.cancelPost?.();
  }

  /** Starts on a newly listed path: reads it, or gives a path of another shape its refusal. */
  private watch(path: string): void {
    const readable = isTaskOutputPath(path);
    const file: Watched = { readable, size: -1, modified: -1, reading: false, again: false };
    this.files.set(path, file);
    if (readable) {
      this.read(path);
      return;
    }
    file.tail = { path, lines: [], error: NOT_A_TASK_OUTPUT };
    this.schedule();
  }

  /** Watches a file once a read found it a regular file; the poll covers the rest. */
  private watchFile(path: string, file: Watched): void {
    try {
      file.watcher = watch(path, { persistent: false }, () => this.read(path));
      file.watcher.on('error', () => {
        file.watcher?.close();
        file.watcher = undefined;
      });
    } catch {
      // No watch for this file: the poll stands in.
    }
  }

  /** Stops watching `path` and forgets it. */
  private unwatch(path: string, file: Watched): void {
    file.watcher?.close();
    this.files.delete(path);
  }

  /** Checks each file for a change in size or time that its watch did not report. */
  private poll(): void {
    for (const [path, file] of this.files) {
      if (!file.readable) {
        continue;
      }
      void fs.lstat(path).then(
        (stat) => {
          if (stat.size !== file.size || stat.mtimeMs !== file.modified) {
            this.read(path);
          }
        },
        () => {
          if (file.size !== -1) this.read(path);
        },
      );
    }
    this.cancelPoll = this.clock.after(POLL_MS, () => this.poll());
  }

  /** Reads the tail of `path` again, one read at a time per file, and posts it when it changed. */
  private read(path: string): void {
    const file = this.files.get(path);
    if (!file) return;
    if (file.reading) {
      file.again = true;
      return;
    }
    file.reading = true;
    // readTail gives an `error` tail for every failure; it never rejects.
    void readTail(path).then((read) => {
      file.reading = false;
      if (this.files.get(path) !== file) return;
      file.size = read.size;
      file.modified = read.modified;
      if (!file.watcher && read.size >= 0) {
        this.watchFile(path, file);
      }
      const changed = JSON.stringify(read.tail) !== JSON.stringify(file.tail);
      file.tail = read.tail;
      if (changed) {
        this.schedule();
      }
      if (file.again) {
        file.again = false;
        this.read(path);
      }
    });
  }

  /** Posts the row's tails now, or once POST_EVERY_MS has passed since the last post. */
  private schedule(): void {
    if (this.pending) return;
    this.pending = true;
    const wait = Math.max(0, this.lastPost + POST_EVERY_MS - this.clock.now());
    this.cancelPost = this.clock.after(wait, () => {
      this.pending = false;
      this.flush();
    });
  }

  /** Posts the row's tails once every listed file has one. */
  private flush(): void {
    const row = this.row;
    if (row === null) return;
    const tails: Tail[] = [];
    for (const path of this.paths) {
      const tail = this.files.get(path)?.tail;
      // Each listed file posts once it has been read; until then the others wait for it.
      if (tail === undefined) return;
      tails.push(tail);
    }
    this.lastPost = this.clock.now();
    this.post({ type: 'output', version: 1, row, tails });
  }
}
