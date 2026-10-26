// What runs in the main process of every VS Code the oracle starts on macOS, before its first line
// (support/background.mjs), so that a run leaves the owner's session as it was (verification.md,
// "No side effects"):
// 1. the app is never active, so no window of it takes focus;
// 2. Electron's clipboard is private to this VS Code: every read and write of
//    `require('electron').clipboard`, the module VS Code's main process copies and pastes
//    through, goes to an in-memory store, so no check reads or changes the system pasteboard;
// 3. VS Code's macOS recent documents, which it clears and refills on every start for the bundle
//    it shares with the owner's VS Code (the Dock menu's list), are left alone, and no power save
//    blocker keeps the owner's display or Mac awake;
// 4. every window is fully transparent and lets the mouse through, so nothing of the run shows
//    on screen and no pointer or scroll wheel of the owner reaches it. An app that is never
//    active still orders a window it shows just behind the frontmost window, in front of every
//    other app's windows; transparent, it shows nowhere. The checks reach the windows through
//    CDP alone, which neither affects, and a transparent window still paints;
// 5. a native dialog, an OS notification or a beep never shows: each is refused and noted in
//    the log file, which fails the layer (`<layer>:side_effects`, layers/focus.rs).
// With `back` (background.mjs's keepAtBack, for the demo recording) or
// WTPOOL_VERIFY_VISIBLE_WINDOWS=1, for a person watching a run, the windows are opaque instead and
// kept at the back of the window order; nothing else changes.

/**
 * Installs the hooks. Runs inside VS Code's main process through the inspector, while `require`
 * of the console API is there, so it takes every module it needs at once.
 */
function install({ back, log }) {
  const electron = require('electron');
  const fs = require('fs');
  const effects = [];
  const note = (what) => {
    effects.push(what);
    if (!log) return;
    try {
      fs.appendFileSync(log, `${new Date().toISOString()} pid ${process.pid}: ${what}\n`);
    } catch {
      // The layer still fails on the effects VS Code's own output shows.
    }
  };
  electron.app.setActivationPolicy('prohibited');

  // The private clipboard: one store per clipboard type, a write replacing its whole content as
  // a write of the system pasteboard does.
  const boards = new Map();
  const board = (type) => {
    const key = type === 'selection' ? 'selection' : 'clipboard';
    if (!boards.has(key)) boards.set(key, new Map());
    return boards.get(key);
  };
  const replace = (type, entries) => {
    const target = board(type);
    target.clear();
    for (const [format, value] of entries) if (value !== undefined) target.set(format, value);
  };
  let findText = '';
  const writes = [];
  const text = (format) => (type) => {
    const value = board(type).get(format);
    return typeof value === 'string' ? value : '';
  };
  const fake = {
    availableFormats: (type) => [...board(type).keys()],
    has: (format, type) => board(type).has(format),
    clear: (type) => board(type).clear(),
    read: (format) => text(format)(),
    readText: text('text/plain'),
    readHTML: text('text/html'),
    readRTF: text('text/rtf'),
    readBuffer: (format) => {
      const value = board().get(format);
      return Buffer.isBuffer(value) ? value : Buffer.alloc(0);
    },
    readImage: (type) => board(type).get('image/png') ?? electron.nativeImage.createEmpty(),
    readBookmark: () => board().get('bookmark') ?? { title: '', url: '' },
    readFindText: () => findText,
    writeText: (value, type) => {
      writes.push(String(value));
      replace(type, [['text/plain', String(value)]]);
    },
    writeHTML: (markup, type) => replace(type, [['text/html', String(markup)]]),
    writeRTF: (value, type) => replace(type, [['text/rtf', String(value)]]),
    writeBuffer: (format, buffer, type) => replace(type, [[format, Buffer.from(buffer)]]),
    writeImage: (image, type) => replace(type, [['image/png', image]]),
    writeBookmark: (title, url, type) => replace(type, [['bookmark', { title, url }]]),
    writeFindText: (value) => {
      findText = String(value);
    },
    write: (data, type) => {
      if (data.text !== undefined) writes.push(String(data.text));
      replace(type, [
        ['text/plain', data.text],
        ['text/html', data.html],
        ['text/rtf', data.rtf],
        ['image/png', data.image],
        ['bookmark', data.bookmark === undefined ? undefined : { title: data.bookmark, url: data.text ?? '' }],
      ]);
    },
  };
  const { clipboard } = electron;
  for (const name of Object.getOwnPropertyNames(clipboard)) {
    // A call this store does not know never reaches the system pasteboard either.
    if (typeof clipboard[name] === 'function' && !(name in fake)) {
      fake[name] = () => note(`clipboard.${name}: refused, the private clipboard has no such call`);
    }
  }
  for (const [name, call] of Object.entries(fake)) {
    clipboard[name] = call;
    if (clipboard[name] !== call) throw new Error(`clipboard.${name} cannot be replaced`);
  }

  // The owner's recent documents of the VS Code bundle.
  const recent = [];
  electron.app.addRecentDocument = (file) => recent.push(file);
  electron.app.clearRecentDocuments = () => recent.push(null);

  // Native dialogs, OS notifications and beeps: refused, each one noted.
  const dialogs = ['showMessageBox', 'showMessageBoxSync', 'showOpenDialog', 'showOpenDialogSync', 'showSaveDialog', 'showSaveDialogSync', 'showErrorBox', 'showCertificateTrustDialog'];
  for (const name of dialogs) {
    electron.dialog[name] = (...args) => {
      const options = args.find((arg) => arg && typeof arg === 'object' && !('webContents' in arg)) ?? {};
      const words = typeof args[0] === 'string' ? args.slice(0, 2).join(': ') : [options.title, options.message].filter(Boolean).join(': ');
      note(`dialog.${name}: refused a native dialog: ${String(words).slice(0, 200)}`);
      const response = options.cancelId ?? Math.max(0, (options.buttons?.length ?? 1) - 1);
      const cancel = { response, checkboxChecked: false, canceled: true, filePaths: [], filePath: '' };
      if (name.endsWith('Sync')) return name === 'showMessageBoxSync' ? cancel.response : undefined;
      return name === 'showErrorBox' ? undefined : Promise.resolve(cancel);
    };
  }
  const notification = electron.Notification?.prototype;
  if (notification) {
    notification.show = function show() {
      note(`Notification.show: refused an OS notification: ${String(this.title ?? '').slice(0, 200)}`);
    };
  }
  electron.shell.beep = () => note('shell.beep: refused the system alert sound');
  // A power save blocker would keep the owner's display or Mac awake while the run goes on.
  const blockers = [];
  electron.powerSaveBlocker.start = (type) => blockers.push(type) - 1;
  electron.powerSaveBlocker.stop = () => {};
  electron.powerSaveBlocker.isStarted = () => false;

  // Every window, from its creation, before it first shows.
  electron.app.on('browser-window-created', (_event, win) => (back ? keepBack(win) : hide(win)));
  globalThis.__wtpoolVerify = { clipboard: { boards, writes }, recent, blockers, effects, back };
  return 'installed';
}

/** Makes a window transparent and lets the mouse through it, for good. */
function hide(win) {
  win.setOpacity(0);
  win.setIgnoreMouseEvents(true);
}

/**
 * Keeps an opaque window at the back of the window order. BrowserWindow.blur on macOS orders a
 * window out and back in at the back of its level. The window is see-through from its creation
 * until it is first painted and sent to the back, as it may order in before its first paint, and
 * it goes to the back again after each call that orders it front.
 */
function keepBack(win) {
  const toBack = () => {
    if (!win.isDestroyed()) win.blur();
  };
  let settled = false;
  const settle = () => {
    if (settled || win.isDestroyed()) return;
    settled = true;
    toBack();
    win.setOpacity(1);
  };
  win.setOpacity(0);
  win.once('ready-to-show', settle);
  setTimeout(settle, 5000);
  for (const name of ['show', 'showInactive', 'focus', 'moveTop']) {
    const original = win[name].bind(win);
    win[name] = (...args) => {
      const result = original(...args);
      toBack();
      return result;
    };
  }
}

/**
 * The expression the inspector evaluates in VS Code's main process: it installs the hooks and
 * yields `installed`. `log` names the file each refused effect is appended to; `back` keeps
 * opaque windows at the back instead of transparent ones.
 */
export function mainHooks({ log, back = false } = {}) {
  const options = { back: back || process.env.WTPOOL_VERIFY_VISIBLE_WINDOWS === '1', log: log ?? null };
  return `(() => { ${hide.toString()}\n${keepBack.toString()}\nreturn (${install.toString()})(${JSON.stringify(options)}); })()`;
}
