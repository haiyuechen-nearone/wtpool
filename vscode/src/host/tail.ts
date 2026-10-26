import { constants, promises as fs, type Stats } from 'node:fs';

import type { Tail } from '../shared/protocol';
import { assertIsError } from './errors';

/** The most the extension reads of an output file at once from its end. */
const TAIL_BYTES = 64 * 1024;
/** The number of lines a tail keeps. */
const TAIL_LINES = 15;
/** The columns from one tab stop to the next. */
const TAB_COLUMNS = 8;

/** A tail and the file state it was read at, for a later read to compare with. */
interface TailRead {
  tail: Tail;
  size: number;
  modified: number;
}

/** The end of an output file as {@link readLastBytes} reads it. */
export interface LastBytes {
  /** The bytes read, decoded as UTF-8 with replacement characters. */
  text: string;
  /** The offset of the first byte read. Above 0, the read cut the first line of `text`. */
  start: number;
  /** The offset after the last byte read. */
  end: number;
  /** The size of the file at the read. */
  size: number;
  /** The modification time of the file at the read, in milliseconds. */
  modified: number;
}

// Regex escapes of the control characters the patterns name: a control character written in a
// pattern literal reads as a mistake to the linter.
const ESC = '\\u001b';
const BEL = '\\u0007';
/** The 8-bit CSI, the C1 form of `ESC [`. */
const CSI = '\\u009b';
/** The 8-bit string terminator, the C1 form of `ESC \`. */
const ST = '\\u009c';
/** DCS, SOS, OSC, PM and APC, the C1 controls that open a control string. */
const STRING_OPENERS = '\\u0090\\u0098\\u009d\\u009e\\u009f';
/** Every C1 control. */
const C1 = '\\u0080-\\u009f';
/** The C0 controls and DEL that only Follow drops: all but tab, line feed, carriage return and ESC. */
const FOLLOW_C0 = '\\u0000-\\u0008\\u000b\\u000c\\u000e-\\u001a\\u001c-\\u001f\\u007f';

/**
 * Every escape sequence and C1 control, in one pass so that a control string goes with its
 * payload. xterm acts on the 8-bit forms as on the 7-bit ones, and an OSC 52 there writes the
 * clipboard. The alternatives: a C1 control string up to BEL, ST or the next ESC, an 8-bit CSI,
 * a 7-bit CSI, OSC, DCS, SOS, PM or APC, a two character escape, then any C1 or ESC left over.
 */
const ESCAPES = [
  `[${STRING_OPENERS}][^${BEL}${ESC}${ST}]*(?:${BEL}|${ST}|${ESC}\\\\)?`,
  `${CSI}[0-?]*[ -/]*[@-~]`,
  `${ESC}\\[[0-?]*[ -/]*[@-~]`,
  `${ESC}\\][^${BEL}${ESC}]*(?:${BEL}|${ESC}\\\\)?`,
  `${ESC}[PX^_][^${ESC}]*(?:${ESC}\\\\)?`,
  `${ESC}[ -/]*[0-~]`,
  `[${C1}]`,
  ESC,
].join('|');
const TAIL_DROPS = new RegExp(ESCAPES, 'g');
const FOLLOW_DROPS = new RegExp(`${ESCAPES}|[${FOLLOW_C0}]`, 'g');

/** Removes every escape sequence and C1 control from `text`. */
function stripEscapes(text: string): string {
  return text.replace(TAIL_DROPS, '');
}

/** An SGR (colour) sequence in its 7-bit form, the only escape Follow keeps. */
const SGR = new RegExp(`^${ESC}\\[[0-9;:]*m$`);

/**
 * Removes from `text` every escape sequence but a 7-bit SGR and every control character but tab,
 * line feed and carriage return, so a terminal gets nothing it acts on but colours.
 */
export function keepColours(text: string): string {
  return text.replace(FOLLOW_DROPS, (found) => (SGR.test(found) ? found : ''));
}

/** Keeps the text of `line` after its last carriage return, the last draw of a progress line. */
function lastDraw(line: string): string {
  const trimmed = line.endsWith('\r') ? line.slice(0, -1) : line;
  const at = trimmed.lastIndexOf('\r');
  return at < 0 ? trimmed : trimmed.slice(at + 1);
}

/** The text that counts other than one column per character: an SGR sequence, a tab, a line start. */
const COLUMN_MARKS = new RegExp(`${ESC}\\[[0-9;:]*m|[\\t\\r\\n]`, 'g');

function codePointCount(text: string): number {
  return [...text].length;
}

/**
 * Turns each tab of `text` into spaces up to the next multiple of {@link TAB_COLUMNS} columns,
 * counted one per character from `column`, and gives the column the text ends at. A carriage
 * return or a line feed starts the count again. An SGR sequence, the one escape Follow keeps,
 * takes no column.
 */
export function expandTabs(text: string, column = 0): { text: string; column: number } {
  let at = column;
  let from = 0;
  const expanded = text.replace(COLUMN_MARKS, (found: string, offset: number) => {
    at += codePointCount(text.slice(from, offset));
    from = offset + found.length;
    if (found === '\t') {
      const spaces = TAB_COLUMNS - (at % TAB_COLUMNS);
      at += spaces;
      return ' '.repeat(spaces);
    }
    if (found === '\r' || found === '\n') at = 0;
    return found;
  });
  return { text: expanded, column: at + codePointCount(text.slice(from)) };
}

/**
 * Gives the last `count` lines of `last` after `clean`, each from its last carriage return. A read
 * that starts inside the file drops its first line, cut by the read, unless it is the only one.
 * Tabs turn into spaces last, so their columns count the line as shown.
 */
export function tailLines(last: LastBytes, count = TAIL_LINES, clean = stripEscapes): string[] {
  let lines = clean(last.text).split('\n');
  if (last.start > 0 && lines.length > 1) lines = lines.slice(1);
  if (lines.at(-1) === '') lines.pop();
  return lines.slice(-count).map((line) => expandTabs(lastDraw(line)).text);
}

const SYMLINK_REFUSED = 'the output is a symlink, which the panel does not follow';

/** A refusal of a file that is not a regular one, with the reason as its message. */
class OutputRefusedError extends Error {}

function requireRegularFile(stat: Stats): void {
  if (stat.isSymbolicLink()) throw new OutputRefusedError(SYMLINK_REFUSED);
  if (!stat.isFile()) throw new OutputRefusedError('the output is not a regular file');
}

/** Opens `path` for reading without following a symlink and without blocking on a FIFO. */
export async function openOutput(path: string): Promise<fs.FileHandle> {
  requireRegularFile(await fs.lstat(path));
  // Windows has neither flag, though the types give both.
  const flags = constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0) | (constants.O_NONBLOCK ?? 0);
  const file = await fs.open(path, flags);
  let regular = false;
  try {
    // The path may name another file than the lstat saw.
    requireRegularFile(await file.stat());
    regular = true;
  } finally {
    if (!regular) await file.close();
  }
  return file;
}

/** The reasons of the read errors that have a wording of their own, by error code. */
const CODE_LINES = new Map<unknown, string>([
  ['ENOENT', 'the output file is gone'],
  ['ELOOP', SYMLINK_REFUSED],
]);

/** Describes in one line the reason `cause` stopped a read. */
export function readError(cause: unknown): string {
  assertIsError(cause);
  if (cause instanceof OutputRefusedError) return cause.message;
  const code = 'code' in cause ? cause.code : undefined;
  return CODE_LINES.get(code) ?? `the output cannot be read: ${cause.message}`;
}

/** Opens `path` with {@link openOutput} and reads at most its last {@link TAIL_BYTES}. */
export async function readLastBytes(path: string): Promise<LastBytes> {
  const file = await openOutput(path);
  try {
    const stat = await file.stat();
    const start = Math.max(0, stat.size - TAIL_BYTES);
    const buffer = Buffer.alloc(stat.size - start);
    const { bytesRead } = await file.read(buffer, 0, buffer.length, start);
    const text = new TextDecoder('utf-8').decode(buffer.subarray(0, bytesRead));
    return { text, start, end: start + bytesRead, size: stat.size, modified: stat.mtimeMs };
  } finally {
    await file.close();
  }
}

/** Reads the tail of `path`. A file it refuses or cannot read gives a tail with `error`. */
export async function readTail(path: string): Promise<TailRead> {
  try {
    const last = await readLastBytes(path);
    return { tail: { path, lines: tailLines(last) }, size: last.size, modified: last.modified };
  } catch (cause) {
    return { tail: { path, lines: [], error: readError(cause) }, size: -1, modified: -1 };
  }
}
