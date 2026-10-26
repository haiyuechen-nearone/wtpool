import type { StatusDocument } from '../generated/status';
import type { Connection } from '../shared/protocol';
import type { Cancel, Clock } from './clock';
import { LineSplitter, MAX_LINE_BYTES, readLine } from './lines';
import { lastErrorLine, spawnWatch, stopChild, type WatchProcess } from './spawn';

/** The silence that turns a live connection stale. */
const STALE_MS = 15_000;
/** The silence that makes the stream start `wtpool` again. */
const RESTART_MS = 30_000;
/** The wait before the first restart after a fault. Each further fault doubles it. */
const FIRST_BACKOFF_MS = 1_000;
/** The longest wait between two starts. */
const MAX_BACKOFF_MS = 30_000;

const WATCH_ARGS = ['status', '--watch'];
const MIB = 1024 * 1024;

/** Carries a status document that arrived. {@link WatchStream.lastLine} gives its time. */
export interface DocumentUpdate {
  document: StatusDocument;
}

/** Receives the reports of a {@link WatchStream} to its tab. */
interface StreamEvents {
  /**
   * Hears that a document arrived (`update`), that a line made the connection live again, that
   * the connection went stale or paused, or that a restart began.
   */
  changed(update?: DocumentUpdate): void;
  /** Writes a diagnostic to the tab's output channel. */
  log(message: string): void;
}

/** Runs one tab's `wtpool status --watch` and restarts it, with backoff, after each fault. */
export class WatchStream {
  private state: Connection = 'starting';
  private lastLineAt?: number;
  private nextStartAt = 0;
  private failure?: string;
  private child?: WatchProcess;
  /** The current start. The events of an earlier `wtpool` no longer count. */
  private attempt?: object;
  private backoff = FIRST_BACKOFF_MS;
  private silentSince = 0;
  private cancelTimer?: Cancel;

  constructor(
    private readonly clock: Clock,
    private readonly binary: () => string,
    private readonly cwd: string,
    private readonly events: StreamEvents,
  ) {}

  /**
   * Gives the connection: `starting` from each start until wtpool's first line. With no line
   * to count from it never turns stale: a fault, or {@link RESTART_MS} without a line, pauses it as
   * it would any start.
   */
  get connection(): Connection {
    return this.state;
  }

  /** Gives the time the last line arrived, in clock milliseconds, if one did. */
  get lastLine(): number | undefined {
    return this.lastLineAt;
  }

  /** Gives the time the next start is due. */
  get retryAt(): number {
    return this.nextStartAt;
  }

  /**
   * Gives, while paused after a start that failed, wtpool's last non-empty stderr line, or the
   * reason it did not start. Any line from `wtpool` clears it.
   */
  get error(): string | undefined {
    return this.failure;
  }

  /** Starts `wtpool`, now and after each fault. */
  start(): void {
    // A restart turns the watch back to starting, so the header never counts down past the time
    // the restart was due.
    const restarting = this.state === 'paused';
    this.state = 'starting';
    const splitter = new LineSplitter();
    const attempt = {};
    this.attempt = attempt;
    const child = spawnWatch(this.binary(), WATCH_ARGS, this.cwd, (ended) => {
      if (this.attempt === attempt) {
        this.fail('wtpool ended or did not start', lastErrorLine(ended));
      }
    });
    this.child = child;
    if (child) {
      // Ending the stdin of a `wtpool` that has gone fails with EPIPE, harmless here.
      child.stdin.on('error', () => undefined);
      child.stdout.on('data', (chunk: Buffer) => {
        if (this.attempt === attempt) {
          this.read(splitter.push(chunk));
        }
      });
    }
    this.silentSince = this.clock.now();
    this.arm();
    if (restarting) {
      this.events.changed();
    }
  }

  /** Stops for good: the tab closed. */
  stop(): void {
    this.cancelTimer?.();
    this.kill();
  }

  /** Ends the current `wtpool`, whose events then no longer count. */
  private kill(): void {
    const child = this.child;
    this.attempt = undefined;
    this.child = undefined;
    if (child) {
      stopChild(child, { endStdin: true });
    }
  }

  /** Takes the lines of a chunk: each document, alive line or fault. */
  private read(lines: string[] | 'oversize'): void {
    if (lines === 'oversize') {
      this.fail(`a line over ${MAX_LINE_BYTES / MIB} MiB`);
      return;
    }
    for (const text of lines) {
      const line = readLine(text);
      if (line.kind === 'fault') {
        this.fail(line.reason);
        return;
      }
      const wasLive = this.state === 'live';
      const now = this.clock.now();
      this.state = 'live';
      this.failure = undefined;
      this.lastLineAt = now;
      this.silentSince = now;
      const update = line.kind === 'status' ? { document: line.document } : undefined;
      if (update) {
        this.backoff = FIRST_BACKOFF_MS;
      }
      if (update || !wasLive) {
        this.events.changed(update);
      }
    }
    this.arm();
  }

  /**
   * Sets the next silence deadline: stale at {@link STALE_MS} while live, else a restart at
   * {@link RESTART_MS}.
   */
  private arm(): void {
    this.cancelTimer?.();
    const silent = this.clock.now() - this.silentSince;
    if (this.state === 'live' && silent < STALE_MS) {
      this.cancelTimer = this.clock.after(STALE_MS - silent, () => {
        this.state = 'stale';
        this.events.changed();
        this.arm();
      });
    } else {
      // Also the next restart a paused stream shows, until the restarted `wtpool` prints a line.
      this.nextStartAt = this.clock.now() + RESTART_MS - silent;
      this.cancelTimer = this.clock.after(RESTART_MS - silent, () => {
        this.fail(`${RESTART_MS / 1000} s without a line`);
      });
    }
  }

  /** Pauses after a fault, starts again after the backoff, and doubles the backoff. */
  private fail(reason: string, error?: string): void {
    this.cancelTimer?.();
    this.kill();
    this.state = 'paused';
    this.failure = error;
    this.nextStartAt = this.clock.now() + this.backoff;
    const detail = error === undefined ? reason : `${reason}: ${error}`;
    this.events.log(`restarting wtpool status --watch in ${this.backoff / 1000} s: ${detail}`);
    this.cancelTimer = this.clock.after(this.backoff, () => void this.start());
    this.backoff = Math.min(this.backoff * 2, MAX_BACKOFF_MS);
    this.events.changed();
  }
}
