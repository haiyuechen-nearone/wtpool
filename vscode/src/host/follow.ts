// Follow (X19): a terminal that shows an output file live through a Pseudoterminal that reads the
// file itself, with no shell and no `tail`, so it works on every OS.
import { watch, type FSWatcher } from 'node:fs';
import * as vscode from 'vscode';

import { plain } from './actions';
import { systemClock, type Cancel, type Clock } from './clock';
import { POLL_MS } from './outputs';
import { expandTabs, keepColours, openOutput, readError, readLastBytes, tailLines, type LastBytes } from './tail';

/** The number of lines a Follow terminal starts with, and shows again after it skips ahead. */
const FOLLOW_LINES = 40;
/**
 * The most a Follow terminal reads per tick of {@link POLL_MS}. It skips ahead of a writer that
 * outruns it, so the extension host never holds or sends more for the terminal to draw.
 */
const TICK_BYTES = 1024 * 1024;

/** Names a Follow terminal after the agent's label and the task's summary, as plain text. */
function followName(label: string, summary: string): string {
  return `${plain(label)} · ${plain(summary) || 'background task'}`;
}

/** Streams the output file at `path` into a terminal, from its last {@link FOLLOW_LINES} lines on. */
class FollowTerminal implements vscode.Pseudoterminal {
  private readonly written = new vscode.EventEmitter<string>();
  readonly onDidWrite = this.written.event;
  /** The file offset the next read starts at. */
  private offset = 0;
  /** The column of the terminal's cursor, where the next tab starts counting. */
  private column = 0;
  private decoder = new TextDecoder('utf-8');
  private watcher?: FSWatcher;
  private cancelPoll?: Cancel;
  /** The clock time the current tick ends at. */
  private tickEnd = -Infinity;
  /** The bytes the current tick may still read. */
  private tickLeft = 0;
  private closed = false;
  private reading = false;
  private again = false;

  constructor(
    private readonly path: string,
    private readonly clock: Clock = systemClock,
  ) {}

  open(): void {
    void this.start();
  }

  close(): void {
    this.closed = true;
    this.watcher?.close();
    this.watcher = undefined;
    this.cancelPoll?.();
    this.written.dispose();
  }

  private write(text: string): void {
    if (!this.closed && text) this.written.fire(text);
  }

  /** Converts `text` for the terminal: tabs as spaces, each line end a carriage return and a line feed. */
  private terminalText(text: string): string {
    const expanded = expandTabs(keepColours(text), this.column);
    this.column = expanded.column;
    return expanded.text.replace(/\r?\n/g, '\r\n');
  }

  /** Moves the next read to `offset`, with the cursor at a line start. */
  private moveTo(offset: number): void {
    this.offset = offset;
    this.column = 0;
    this.decoder = new TextDecoder('utf-8');
  }

  /** Writes the last {@link FOLLOW_LINES} lines of `last` and goes on after them. */
  private writeLast(last: LastBytes): void {
    this.moveTo(last.end);
    const lines = tailLines(last, FOLLOW_LINES, keepColours);
    this.write(lines.map((line) => `${line}\r\n`).join(''));
  }

  /** Writes the file's last lines, then watches and polls it for growth. */
  private async start(): Promise<void> {
    let last: LastBytes;
    try {
      last = await readLastBytes(this.path);
    } catch (cause) {
      const line = plain(`wtpool: ${readError(cause)}: ${this.path}`);
      this.write(`${line}\r\n`);
      return;
    }
    this.writeLast(last);
    if (this.closed) return;
    try {
      this.watcher = watch(this.path, { persistent: false }, () => void this.grow());
      this.watcher.on('error', () => undefined);
    } catch {
      // The poll stands in for a watch the system does not give.
    }
    this.poll();
  }

  private poll(): void {
    if (this.closed) return;
    this.cancelPoll = this.clock.after(POLL_MS, () => {
      void this.grow();
      this.poll();
    });
  }

  /** Writes the file's gain since the last read, one read at a time. */
  private async grow(): Promise<void> {
    if (this.closed) return;
    if (this.reading) {
      this.again = true;
      return;
    }
    this.reading = true;
    try {
      await this.readGain();
    } catch {
      // A file that went away or turned into something else shows nothing more.
    } finally {
      this.reading = false;
    }
    if (this.again) {
      this.again = false;
      await this.grow();
    }
  }

  /** Starts a tick of {@link TICK_BYTES} once the last one has ended. */
  private startTick(): void {
    const now = this.clock.now();
    if (now < this.tickEnd) return;
    this.tickEnd = now + POLL_MS;
    this.tickLeft = TICK_BYTES;
  }

  /**
   * Writes the file's gain if the tick can take it. A file that shrank starts over. A gain that
   * the rest of a tick cannot take waits for the next tick's poll, and one that a whole tick
   * cannot take skips ahead.
   */
  private async readGain(): Promise<void> {
    const file = await openOutput(this.path);
    try {
      const { size } = await file.stat();
      if (size < this.offset) {
        this.moveTo(0);
        this.write('\r\nwtpool: the output file was truncated; following it from its start\r\n');
      }
      const gain = size - this.offset;
      if (gain === 0) return;
      this.startTick();
      if (gain > this.tickLeft) {
        if (this.tickLeft === TICK_BYTES) await this.skip();
        return;
      }
      const buffer = Buffer.alloc(gain);
      const { bytesRead } = await file.read(buffer, 0, gain, this.offset);
      this.offset += bytesRead;
      this.tickLeft -= bytesRead;
      this.write(this.terminalText(this.decoder.decode(buffer.subarray(0, bytesRead), { stream: true })));
    } finally {
      await file.close();
    }
  }

  /** Skips to the file's last lines, after one line that says so. */
  private async skip(): Promise<void> {
    const last = await readLastBytes(this.path);
    this.tickLeft -= last.end - last.start;
    this.write(`\r\nwtpool: the output grows faster than Follow shows it; skipped to its last ${FOLLOW_LINES} lines\r\n`);
    this.writeLast(last);
  }
}

/** Opens and shows a Follow terminal for the output at `path`. */
export function follow(path: string, label: string, summary: string): vscode.Terminal {
  const terminal = vscode.window.createTerminal({ name: followName(label, summary), pty: new FollowTerminal(path) });
  terminal.show();
  return terminal;
}
