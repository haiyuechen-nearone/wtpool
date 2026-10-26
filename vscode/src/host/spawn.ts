import { spawn, type ChildProcess, type ChildProcessByStdio } from 'node:child_process';
import type { Readable, Writable } from 'node:stream';

import { resolveBinary } from './binary';
import { assertIsError } from './errors';

/** The number of characters kept from the end of a run's stderr. */
const STDERR_TAIL = 4096;

/** Holds a `wtpool` that did not start: spawn threw or reported an error. */
interface NotStarted {
  started: false;
  error: Error;
}

/** Holds a `wtpool` that exited, with the last {@link STDERR_TAIL} characters of its stderr. */
interface Exited {
  started: true;
  code: number | null;
  signal: NodeJS.Signals | null;
  stderr: string;
}

/** The end of one run of `wtpool`. */
export type RunEnd = NotStarted | Exited;

/** A running watch. Its stdin stays open, as `wtpool status --watch` exits at EOF. */
export type WatchProcess = ChildProcessByStdio<Writable, Readable, Readable>;

/** Describes an exit that left no error text. */
export function describeExit(code: number | null, signal: NodeJS.Signals | null): string {
  return code === null ? `wtpool was stopped by ${signal}` : `wtpool exited with code ${code}`;
}

/**
 * Starts `start()`'s child and calls `ended` once, when it did not start or once it exited.
 * spawn reports some failures (ENOENT, EACCES) through its 'error' event, a tick later, and
 * throws others (ENOTDIR, ELOOP, a NUL in the path, a bare name {@link resolveBinary} does not
 * find). A throw also reaches `ended` a tick later, so a caller handles one asynchronous end.
 */
function track<T extends ChildProcess & { stderr: Readable }>(
  start: () => T,
  ended: (end: RunEnd) => void,
): T | undefined {
  let child: T;
  try {
    child = start();
  } catch (error: unknown) {
    assertIsError(error);
    queueMicrotask(() => ended({ started: false, error }));
    return undefined;
  }
  let stderr = '';
  let done = false;
  const end = (result: RunEnd): void => {
    if (!done) {
      done = true;
      ended(result);
    }
  };
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', (chunk: string) => {
    stderr = (stderr + chunk).slice(-STDERR_TAIL);
  });
  child.on('error', (error) => end({ started: false, error }));
  // 'close' comes once stderr has ended, so the tail holds all that wtpool printed.
  child.on('close', (code, signal) => end({ started: true, code, signal, stderr }));
  return child;
}

/**
 * Starts `binary` with `args` as a watch, or gives undefined when spawn threw. `ended` runs in
 * both cases.
 */
export function spawnWatch(
  binary: string,
  args: string[],
  cwd: string,
  ended: (end: RunEnd) => void,
): WatchProcess | undefined {
  return track(
    () =>
      spawn(resolveBinary(binary), args, {
        cwd,
        stdio: ['pipe', 'pipe', 'pipe'],
        shell: false,
      }),
    ended,
  );
}

/** Runs `binary` with `args` to its end, with no stdin or stdout. */
export function runToEnd(binary: string, args: string[], cwd: string | undefined): Promise<RunEnd> {
  return new Promise((resolve) => {
    track(
      () =>
        spawn(resolveBinary(binary), args, {
          cwd,
          stdio: ['ignore', 'ignore', 'pipe'],
          shell: false,
        }),
      resolve,
    );
  });
}
