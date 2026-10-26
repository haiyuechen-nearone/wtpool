// The webview's entry: one Page fed by the extension's view messages (verification.md, "Test
// interfaces", item 5), and a one second tick for the ages (U7).
import { isRecord, type OutputMessage, type ViewMessage } from '../shared/protocol';
import { Page, type Host } from './page';

declare function acquireVsCodeApi(): Host;

/** Holds the hand over the page's inline first script leaves on `window` (html.ts). */
interface Early {
  /** Passes the kept messages to `consumer`, in the order they came, then every later one. */
  attach(consumer: (data: unknown) => void): void;
}

declare global {
  interface Window {
    /** The inline first script's hand over of the messages it kept (html.ts). */
    wtpoolEarly: Early;
  }
}

/** Tells a view message of version 1. The extension validated its document. */
function isView(data: unknown): data is ViewMessage {
  return isRecord(data) && data.type === 'view' && data.version === 1;
}

/** Tells one tail of an output message: a path and lines of text. */
function isTail(tail: unknown): boolean {
  return isRecord(tail) && typeof tail.path === 'string' && Array.isArray(tail.lines) && tail.lines.every((line) => typeof line === 'string');
}

/** Tells an output message of version 1: the selected row's tails (R12). */
function isOutput(data: unknown): data is OutputMessage {
  return (
    isRecord(data) &&
    data.type === 'output' &&
    data.version === 1 &&
    typeof data.row === 'string' &&
    Array.isArray(data.tails) &&
    data.tails.every((tail) => isTail(tail))
  );
}

const page = new Page(acquireVsCodeApi());

// The inline first script listens from the top of <head> and hands over the messages it kept,
// then every message.
window.wtpoolEarly.attach((data) => {
  if (isView(data)) {
    page.render(data, Date.now());
  } else if (isOutput(data)) {
    page.showOutput(data);
  }
});

setInterval(() => page.tick(Date.now()), 1000);
