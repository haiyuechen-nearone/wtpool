import type { StatusDocument } from '../generated/status';
import { validate } from '../generated/validate.js';
import { isRecord } from '../shared/protocol';

/** The longest line the extension reads, newline excluded. */
export const MAX_LINE_BYTES = 1024 * 1024;

const NEWLINE = 0x0a;

/** Cuts a byte stream into lines, up to {@link MAX_LINE_BYTES} each. */
export class LineSplitter {
  private pending: Buffer[] = [];
  private pendingBytes = 0;

  /** Adds `chunk` and gives the lines it completed, or 'oversize' once a line is too long. */
  push(chunk: Buffer): string[] | 'oversize' {
    const lines: string[] = [];
    for (let start = 0; ; ) {
      const end = chunk.indexOf(NEWLINE, start);
      const stop = end < 0 ? chunk.length : end;
      this.pending.push(chunk.subarray(start, stop));
      this.pendingBytes += stop - start;
      if (this.pendingBytes > MAX_LINE_BYTES) {
        return 'oversize';
      }
      if (end < 0) {
        return lines;
      }
      lines.push(Buffer.concat(this.pending).toString('utf8'));
      this.pending = [];
      this.pendingBytes = 0;
      start = end + 1;
    }
  }
}

interface StatusLine {
  kind: 'status';
  document: StatusDocument;
}

interface AliveLine {
  kind: 'alive';
}

/** A line that restarts the stream, with the reason for the log. */
interface FaultLine {
  kind: 'fault';
  reason: string;
}

type Line = StatusLine | AliveLine | FaultLine;

function isAliveSeq(seq: unknown): boolean {
  return typeof seq === 'number' && Number.isSafeInteger(seq) && seq >= 0;
}

/** Reads one line of the stream as a status document, an alive line or a fault. */
export function readLine(text: string): Line {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return { kind: 'fault', reason: 'a line that is not JSON' };
  }
  const record: Readonly<Record<string, unknown>> = isRecord(value) ? value : {};
  if (record.type === 'alive' && isAliveSeq(record.seq)) {
    return { kind: 'alive' };
  }
  if (record.type !== 'status' || record.version !== 1) {
    return { kind: 'fault', reason: 'a line of an unknown type or version' };
  }
  if (!validate(record)) {
    return { kind: 'fault', reason: 'a document that breaks the schema' };
  }
  return { kind: 'status', document: record };
}
