// Keeps each VS Code a run launches on macOS from taking focus. Electron activates its app each
// time a window shows, and VS Code shows every window it opens and steals focus for a window
// another instance asks for. LaunchServices' `open -g` keeps only the launch itself in the
// background. So VS Code starts paused before its first line (`--inspect-brk` on a free port), and
// `release` installs support/main-hooks.mjs to make it an app that is never active, then lets it
// run. The checks reach its windows through CDP and Electron's API, and neither needs an active
// window.
//
// An app that is never active still orders a window it shows in front of every other app's
// windows. `keepAtBack`, for the demo recording, keeps each window at the back of the window
// order, where it still paints. Without it each window is transparent and lets the mouse through.
import { execFileSync } from 'node:child_process';
import net from 'node:net';

import { mainHooks } from './main-hooks.mjs';

/** The time VS Code may take to open its inspector and stop at its first line. */
const LIMIT_MS = 60_000;

let atBack = false;

/** Keeps the windows of every VS Code launched from now on at the back of the window order. */
export function keepAtBack() {
  atBack = true;
}

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

/** Kills the VS Code that `flag` started, so one that cannot be kept behind never opens a window. */
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
 * Gives `args` for the command line of one VS Code launch, and `release` to call while it starts.
 * `release` resolves once VS Code runs as an app that is never active with the hooks of
 * support/main-hooks.mjs, or stops it and rejects. `log` names the file the hooks note each
 * refused side effect in. Outside macOS there are no arguments and nothing to release.
 */
export async function inBackground({ log } = {}) {
  if (process.platform !== 'darwin') return { args: [], release: async () => {} };
  const port = await freePort();
  const flag = `--inspect-brk=127.0.0.1:${port}`;
  const back = atBack;
  return { args: [flag], release: () => release(port, flag, back, log) };
}
