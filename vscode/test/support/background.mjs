// Keeps every VS Code the oracle starts from taking focus (verification.md, "Isolation"). On macOS,
// Electron activates its app each time a window shows (BrowserWindow.show calls
// activateIgnoringOtherApps:YES), and VS Code shows every window it opens and steals focus for a
// window another instance asks for (app.focus({ steal: true })). So each test window came to the
// front and took the keyboard from the app in use, however VS Code was started: LaunchServices'
// `open -g` keeps only the launch itself in the background. Instead VS Code starts paused before
// its first line (`--inspect-brk` on a free port), and `release` makes it an app that is never
// active, app.setActivationPolicy('prohibited'), then lets it run. Its windows open behind the
// others and paint as before, and the checks reach them through CDP and Electron's API, which
// need no active window. Elsewhere VS Code starts as before: Linux runs it under xvfb.
//
// An app that is never active still orders a window it shows just behind the frontmost window,
// in front of every other app's windows, so the window shows on top of the owner's windows as
// soon as the owner switches to an app whose windows do not cover it. `keepAtBack` sends every
// window to the back of the window order, behind all windows, when it opens and after each show,
// focus or moveTop VS Code calls; there it still paints, so screenshots stay current. The demo
// recording (scripts/demo.mjs) turns it on, as its window plays for half a minute mid screen.
// Without it, as in the `host` and `e2e` layers, every window is transparent and lets the mouse
// through instead, so it shows nowhere, wherever it is ordered.
//
// The same stop installs the rest of support/main-hooks.mjs before VS Code's first line: a
// private clipboard, no recent documents, and no native dialog, notification or beep
// (verification.md, "No side effects").
import { execFileSync } from 'node:child_process';
import net from 'node:net';

import { mainHooks } from './main-hooks.mjs';

/** How long VS Code may take to open its inspector and stop at its first line. */
const LIMIT_MS = 60_000;

/** Whether launches from now on keep their windows at the back of the window order. */
let atBack = false;

/** Keeps the windows of every VS Code launched from now on at the back of the window order. */
export function keepAtBack() {
  atBack = true;
}

/** A free TCP port of the loopback interface. */
function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

/** `promise`, or an error naming `what` once `deadline` (a Date.now() time) passes. */
function by(deadline, what, promise) {
  let timer;
  const late = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${what} within ${LIMIT_MS / 1000} s`)), Math.max(0, deadline - Date.now()));
  });
  return Promise.race([promise, late]).finally(() => clearTimeout(timer));
}

/** The WebSocket address of the inspector VS Code opens on `port`, once it answers. */
async function inspectorUrl(port, deadline) {
  for (;;) {
    const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((r) => r.json(), () => undefined).catch(() => undefined);
    const url = targets?.find((target) => target.webSocketDebuggerUrl)?.webSocketDebuggerUrl;
    if (url) return url;
    if (Date.now() > deadline) throw new Error(`no inspector on port ${port}`);
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

/** A session on VS Code's main process: {send(method, params), paused, close()}. */
async function session(url) {
  const socket = new WebSocket(url);
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = () => reject(new Error(`the inspector at ${url} refused the connection`));
  });
  const pending = new Map();
  let id = 0;
  let onPaused;
  const paused = new Promise((resolve) => (onPaused = resolve));
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.method === 'Debugger.paused') onPaused();
    const reply = pending.get(message.id);
    if (!reply) return;
    pending.delete(message.id);
    if (message.error) reply.reject(new Error(`${reply.method}: ${message.error.message}`));
    else reply.resolve(message.result);
  };
  socket.onclose = () => {
    for (const reply of pending.values()) reply.reject(new Error(`${reply.method}: the inspector closed`));
    pending.clear();
  };
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      id += 1;
      pending.set(id, { method, resolve, reject });
      socket.send(JSON.stringify({ id, method, params }));
    });
  return { send, paused, close: () => socket.close() };
}

/** Kills the VS Code that `flag` started, so a VS Code that cannot be kept behind never opens a window. */
function kill(flag) {
  const listing = execFileSync('ps', ['-axo', 'pid=,args='], { encoding: 'utf8' });
  for (const line of listing.split('\n')) {
    const match = line.trim().match(/^(\d+)\s+(.*)$/);
    if (!match || !match[2].split(' ').includes(flag)) continue;
    try {
      process.kill(Number(match[1]), 'SIGKILL');
    } catch {
      // Already gone.
    }
  }
}

/**
 * Lets the VS Code paused on `port` run as an app that is never active, with the hooks of
 * support/main-hooks.mjs: its windows opaque at the back with `back`, else transparent, and each
 * refused side effect noted in `log`.
 */
async function release(port, flag, back, log) {
  const deadline = Date.now() + LIMIT_MS;
  let inspector;
  try {
    inspector = await by(deadline, 'VS Code opened no inspector', session(await inspectorUrl(port, deadline)));
    await inspector.send('Debugger.enable');
    await inspector.send('Runtime.runIfWaitingForDebugger');
    // Before its first line, Node cannot yet load a module.
    await by(deadline, 'VS Code did not stop at its first line', inspector.paused);
    const set = await inspector.send('Runtime.evaluate', {
      expression: mainHooks({ back, log }),
      includeCommandLineAPI: true,
      returnByValue: true,
    });
    if (set.result?.value !== 'installed') {
      throw new Error(set.exceptionDetails?.exception?.description ?? JSON.stringify(set));
    }
    await inspector.send('Debugger.resume');
    await inspector.send('Debugger.disable');
  } catch (error) {
    kill(flag);
    throw new Error(`setup: VS Code could not be kept behind other windows and was stopped before its first window: ${error.message}`);
  } finally {
    inspector?.close();
  }
}

/**
 * What keeps one VS Code launch in the background: `args` for its command line, and `release`,
 * to call while it starts, which resolves once VS Code runs as an app that is never active with
 * the hooks of support/main-hooks.mjs, or stops it and rejects. `log` names the file the hooks
 * note each refused side effect in. Outside macOS there are no arguments and nothing to release.
 */
export async function inBackground({ log } = {}) {
  if (process.platform !== 'darwin') return { args: [], release: async () => {} };
  const port = await freePort();
  const flag = `--inspect-brk=127.0.0.1:${port}`;
  const back = atBack;
  return { args: [flag], release: () => release(port, flag, back, log) };
}
