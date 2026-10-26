// Loads the product's webview in jsdom the way VS Code would: `buildHtml(webview, nonce)` from
// vscode/out/html.js, its scripts and styles served from vscode/, `acquireVsCodeApi` defined,
// and a fake clock in the webview's window.
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import FakeTimers from '@sinonjs/fake-timers';
import { JSDOM, VirtualConsole, requestInterceptor } from 'jsdom';

import { NOW, view } from './docs.mjs';
import { OUT_HTML, VSCODE, WEBVIEW_JS } from './paths.mjs';

export const CSP_SOURCE = 'https://wtpool-verify.test';

/** Enough of `vscode.Uri` for `buildHtml`: `scheme`, `path`, `fsPath`, `with` and `toString`. */
export class FakeUri {
  constructor(scheme, filePath) {
    this.scheme = scheme;
    this.authority = '';
    this.path = filePath;
    this.query = '';
    this.fragment = '';
  }

  get fsPath() {
    return this.path;
  }

  with(change) {
    return new FakeUri(change.scheme ?? this.scheme, change.path ?? this.path);
  }

  toString() {
    return this.scheme === 'file' ? pathToFileURL(this.path).href : `${this.scheme}:${this.path}`;
  }

  toJSON() {
    return { scheme: this.scheme, path: this.path };
  }
}

/** The URL the fake webview gives a file below vscode/. */
function servedUrl(filePath) {
  const relative = path.relative(VSCODE, filePath).split(path.sep).join('/');
  return `${CSP_SOURCE}/${relative}`;
}

/** A webview as `buildHtml` sees it, with vscode/media as `options.localResourceRoots[0]`. */
export function fakeWebview(cspSource = CSP_SOURCE) {
  const media = new FakeUri('file', path.join(VSCODE, 'media'));
  return {
    cspSource,
    options: { enableScripts: true, localResourceRoots: [media] },
    asWebviewUri(uri) {
      const filePath =
        typeof uri === 'string'
          ? path.resolve(VSCODE, uri.replace(/^\/+/, ''))
          : (uri.fsPath ?? uri.path);
      const served = servedUrl(filePath).replace(CSP_SOURCE, cspSource);
      return {
        scheme: 'https',
        path: served,
        fsPath: filePath,
        toString: () => served,
        toJSON: () => served,
      };
    },
  };
}

export async function mustLoadBuildHtml() {
  assert.ok(existsSync(OUT_HTML), 'npm run build wrote no vscode/out/html.js');
  assert.ok(existsSync(WEBVIEW_JS), 'npm run build wrote no vscode/media/webview.js');
  const loaded = await import(`${pathToFileURL(OUT_HTML).href}?t=${Date.now()}`);
  const buildHtml = loaded.buildHtml ?? loaded.default?.buildHtml;
  assert.equal(typeof buildHtml, 'function', 'vscode/out/html.js exports no buildHtml');
  return buildHtml;
}

const TYPES = { '.js': 'text/javascript', '.css': 'text/css', '.map': 'application/json' };

/** Serves files below vscode/ for URLs of the cspSource origin and refuses every other URL. */
const serveVscodeFiles = requestInterceptor((request) => {
  const url = request.url;
  if (!url.startsWith(`${CSP_SOURCE}/`)) {
    return new Response('', { status: 404 });
  }
  const file = path.join(VSCODE, decodeURIComponent(new URL(url).pathname.slice(1)));
  if (!file.startsWith(`${VSCODE}${path.sep}`) || !existsSync(file)) {
    return new Response('', { status: 404 });
  }
  return new Response(readFileSync(file), {
    status: 200,
    headers: { 'content-type': TYPES[path.extname(file)] ?? 'application/octet-stream' },
  });
});

/**
 * Mounts the webview. Returns its window and document, the messages it posted, its saved state,
 * the fake clock, the errors it raised, the elements it scrolled to and, in `scrolls`, each
 * scroll's element with its options.
 */
export async function mount({ state, now = NOW, nonce = 'wtverifynonce0123456789', bodyClass = 'vscode-dark' } = {}) {
  const buildHtml = await mustLoadBuildHtml();
  const html = buildHtml(fakeWebview(), nonce);
  assert.equal(typeof html, 'string', 'buildHtml returned no string');
  const posted = [];
  const errors = [];
  const scrolled = [];
  const scrolls = [];
  const saved = { state };
  let clock;
  const virtualConsole = new VirtualConsole();
  virtualConsole.on('jsdomError', (error) => errors.push(error));
  const dom = new JSDOM(html, {
    url: `${CSP_SOURCE}/index.html`,
    runScripts: 'dangerously',
    resources: { interceptors: [serveVscodeFiles] },
    pretendToBeVisual: true,
    virtualConsole,
    beforeParse(window) {
      clock = FakeTimers.withGlobal(window).install({
        now,
        toFake: [
          'setTimeout',
          'clearTimeout',
          'setInterval',
          'clearInterval',
          'Date',
          'requestAnimationFrame',
          'cancelAnimationFrame',
          'performance',
        ],
      });
      let acquired = false;
      const api = {
        postMessage: (message) => posted.push(JSON.parse(JSON.stringify(message))),
        getState: () => saved.state,
        setState: (value) => {
          saved.state = value;
          return value;
        },
      };
      window.acquireVsCodeApi = () => {
        if (acquired) throw new Error('acquireVsCodeApi was called twice');
        acquired = true;
        return api;
      };
      window.Element.prototype.scrollIntoView = function scrollIntoView(options) {
        scrolled.push(this);
        scrolls.push({ element: this, options });
      };
      window.addEventListener('error', (event) => errors.push(event.error ?? event.message));
    },
  });
  const { window } = dom;
  if (window.document.readyState !== 'complete') {
    await new Promise((resolve) => window.addEventListener('load', resolve, { once: true }));
  }
  await new Promise((resolve) => setImmediate(resolve));
  for (const name of bodyClass.split(' ').filter(Boolean)) {
    window.document.body.classList.add(name);
  }
  clock.tick(20);
  const webview = {
    window,
    document: window.document,
    posted,
    errors,
    scrolled,
    scrolls,
    clock,
    state: () => saved.state,
    /** Posts a message from the extension and lets the webview render it. */
    post(message) {
      window.dispatchEvent(new window.MessageEvent('message', { data: message }));
      clock.tick(20);
    },
    /** Posts `status` as a live view message. */
    show(status, options) {
      this.post(view(status, options));
    },
    tick(ms) {
      clock.tick(ms);
    },
    close() {
      clock.uninstall();
      window.close();
    },
  };
  return webview;
}

/** Text of an element, whitespace collapsed. */
export function text(element) {
  return (element?.textContent ?? '').replace(/\s+/g, ' ').trim();
}

export function rows(document) {
  return [...document.querySelectorAll('tr[data-row-id]')];
}

/** The six data cells of a row, then its actions cell. */
export function cells(row) {
  const all = [...row.children].filter((child) => child.tagName === 'TD');
  return { data: all.filter((cell) => !cell.classList.contains('actions')), actions: row.querySelector('td.actions') };
}

/** The `.line` texts of a cell, or its whole text as one line. */
export function lines(cell) {
  const found = [...cell.querySelectorAll('.line')];
  return found.length ? found.map(text) : [text(cell)];
}

export function rowById(document, id) {
  return rows(document).find((row) => row.getAttribute('data-row-id') === id);
}

/**
 * Tells an element on screen: in the document, not `hidden`, and not `display: none`, read by
 * screen readers or not.
 */
export function isDisplayed(window, element) {
  for (let node = element; node && node.nodeType === 1; node = node.parentElement) {
    if (node.hidden || window.getComputedStyle(node).display === 'none') return false;
  }
  return Boolean(element?.isConnected);
}

/** Tells an element shown: in the document, not `hidden`, `aria-hidden` or `display: none`. */
export function isShown(window, element) {
  for (let node = element; node && node.nodeType === 1; node = node.parentElement) {
    if (node.hidden || node.getAttribute('aria-hidden') === 'true') return false;
    if (window.getComputedStyle(node).display === 'none') return false;
  }
  return Boolean(element?.isConnected);
}

/**
 * The declared value of `property` for `element`: its matching style rules in order, then its
 * style attribute.
 */
function declared(window, element, property) {
  let value = '';
  const visit = (rules) => {
    for (const rule of rules) {
      if (!rule.selectorText || !rule.style) continue;
      let matches = false;
      try {
        matches = element.matches(rule.selectorText);
      } catch {
        matches = false;
      }
      const found = matches ? rule.style.getPropertyValue(property) : '';
      if (found) value = found;
    }
  };
  for (const sheet of window.document.styleSheets) {
    try {
      visit(sheet.cssRules);
    } catch {
      // A sheet whose rules jsdom cannot list declares nothing here.
    }
  }
  return element.style?.getPropertyValue(property) || value;
}

const px = (value) => (/^-?[\d.]+(px)?$/.test(value.trim()) ? parseFloat(value) : Number.NaN);

/**
 * Tells an element the stylesheet hides from sight only, as text for screen readers is hidden: a
 * box of at most 1×1 px that clips, a zero `clip` or `clip-path`, a place far off screen, no
 * opacity, or a zero font size.
 */
export function isVisuallyHidden(window, element) {
  const get = (property) => declared(window, element, property).trim();
  const positioned = ['absolute', 'fixed'].includes(get('position'));
  const clip = get('clip');
  const clipValues = [...clip.matchAll(/-?[\d.]+/g)].map((m) => parseFloat(m[0]));
  if (positioned && /^rect\(/.test(clip) && clipValues.every((v) => v <= 1)) return true;
  if (/inset\(\s*(50|100)%/.test(get('clip-path')) || /circle\(\s*0/.test(get('clip-path'))) return true;
  const tiny = px(get('width')) <= 1 && px(get('height')) <= 1;
  if (tiny && (positioned || get('overflow') === 'hidden')) return true;
  if (positioned && (px(get('left')) <= -999 || px(get('top')) <= -999)) return true;
  if (get('opacity') === '0' || px(get('font-size')) === 0) return true;
  return false;
}

/**
 * The text a sighted user reads in `element`: {@link text} without visually hidden text, and
 * without the agent count `.count` unless `keepCount`.
 */
export function seen(window, element, { keepCount = false } = {}) {
  const parts = [];
  const walk = (node) => {
    if (node.nodeType === 3) {
      parts.push(node.textContent);
      return;
    }
    if (node.nodeType !== 1) return;
    if (!keepCount && node.classList.contains('count')) return;
    if (isVisuallyHidden(window, node)) return;
    for (const child of node.childNodes) walk(child);
  };
  if (element) walk(element);
  return parts.join('').replace(/\s+/g, ' ').trim();
}

/** The `.line` texts of a cell as {@link seen} reads them, or its whole such text as one line. */
export function seenLines(window, cell) {
  const found = [...cell.querySelectorAll('.line')];
  return found.length ? found.map((line) => seen(window, line)) : [seen(window, cell)];
}

/** The text a screen reader reads in `element`: every text but that of `aria-hidden` subtrees. */
export function spoken(element) {
  const parts = [];
  const walk = (node) => {
    if (node.nodeType === 3) {
      parts.push(node.textContent);
      return;
    }
    if (node.nodeType !== 1 || node.getAttribute('aria-hidden') === 'true' || node.hidden) return;
    for (const child of node.childNodes) walk(child);
  };
  if (element) walk(element);
  return parts.join('').replace(/\s+/g, ' ').replace(/\s+([,:])/g, '$1').trim();
}
