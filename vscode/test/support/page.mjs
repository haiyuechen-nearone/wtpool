// Loads `buildHtml`'s page in Playwright's Chromium as VS Code shows a webview: VS Code's default
// webview styles first, a theme's `--vscode-*` values on <html> and its body classes, the files
// of vscode/ served from the cspSource origin, `acquireVsCodeApi` and a fake clock. No socket is
// opened: Playwright fulfils every request itself.
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';

import { NOW, view } from './docs.mjs';
import { THEMES, VSCODE } from './paths.mjs';
import { CSP_SOURCE, fakeWebview, mustLoadBuildHtml } from './webview.mjs';

export const THEME_NAMES = ['dark', 'light', 'high-contrast', 'high-contrast-light'];

export function loadTheme(name) {
  return JSON.parse(readFileSync(path.join(THEMES, `${name}.json`), 'utf8'));
}

const TYPES = {
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.ttf': 'font/ttf',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.json': 'application/json',
  '.map': 'application/json',
};

/** Puts VS Code's default styles right after the CSP meta element, governed by the CSP. */
function withDefaultStyles(html) {
  const link = `<link rel="stylesheet" href="${CSP_SOURCE}/__verify/webview-default.css">`;
  const meta = /<meta[^>]+http-equiv=["']?Content-Security-Policy["']?[^>]*>/i;
  if (meta.test(html)) return html.replace(meta, (found) => `${found}${link}`);
  return html.replace(/<head([^>]*)>/i, (found) => `${found}${link}`);
}

function initScript({ vars, bodyClass, state }) {
  window.__verify = { posted: [], violations: [], state, saved: [] };
  document.addEventListener('securitypolicyviolation', (event) => {
    window.__verify.violations.push(`${event.violatedDirective} ${event.blockedURI} ${event.sample}`);
  });
  let acquired = false;
  window.acquireVsCodeApi = () => {
    if (acquired) throw new Error('acquireVsCodeApi was called twice');
    acquired = true;
    return {
      postMessage: (message) => window.__verify.posted.push(JSON.parse(JSON.stringify(message))),
      getState: () => window.__verify.state,
      setState: (value) => {
        window.__verify.state = value;
        window.__verify.saved.push(value);
        return value;
      },
    };
  };
  const apply = () => {
    const root = document.documentElement;
    if (root && !root.dataset.verifyThemed) {
      root.dataset.verifyThemed = '1';
      for (const [name, value] of Object.entries(vars)) root.style.setProperty(name, value);
    }
    if (document.body && !document.body.dataset.verifyThemed) {
      document.body.dataset.verifyThemed = '1';
      for (const name of bodyClass.split(' ').filter(Boolean)) document.body.classList.add(name);
      document.body.dataset.vscodeThemeKind = bodyClass.split(' ').pop();
      return true;
    }
    return Boolean(document.body);
  };
  if (!apply()) {
    const observer = new MutationObserver(() => {
      if (apply()) observer.disconnect();
    });
    observer.observe(document, { childList: true, subtree: true });
  }
}

/**
 * Opens the panel in `page`. `theme` names a file of verify/themes/; `extraBodyClass` adds body
 * classes such as `vscode-reduce-motion`.
 */
export async function openPanel(page, { theme = 'dark', extraBodyClass = '', state, now = NOW } = {}) {
  const buildHtml = await mustLoadBuildHtml();
  const nonce = 'shotNonce0123456789abcdefABCDEF';
  const html = withDefaultStyles(buildHtml(fakeWebview(CSP_SOURCE), nonce));
  const data = loadTheme(theme);
  const errors = [];
  const console_ = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => console_.push(`${message.type()}: ${message.text()}`));
  await page.route(`${CSP_SOURCE}/**`, async (route) => {
    const url = new URL(route.request().url());
    if (url.pathname === '/index.html') {
      await route.fulfill({ status: 200, contentType: 'text/html', body: html });
      return;
    }
    const relative = decodeURIComponent(url.pathname.slice(1));
    const file = relative.startsWith('__verify/')
      ? path.join(THEMES, relative.slice('__verify/'.length))
      : path.join(VSCODE, relative);
    const inside = file.startsWith(`${VSCODE}${path.sep}`) || file.startsWith(`${THEMES}${path.sep}`);
    if (!inside || !existsSync(file)) {
      await route.fulfill({ status: 404, body: '' });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: TYPES[path.extname(file)] ?? 'application/octet-stream',
      body: readFileSync(file),
    });
  });
  await page.route((url) => !url.href.startsWith(CSP_SOURCE), (route) => route.abort());
  await page.clock.install({ time: now });
  await page.addInitScript(initScript, {
    vars: data.vars,
    bodyClass: `${data.bodyClass} ${extraBodyClass}`.trim(),
    state,
  });
  await page.goto(`${CSP_SOURCE}/index.html`);
  await page.waitForLoadState('load');
  await page.clock.runFor(50);
  return {
    errors,
    console: console_,
    theme: data,
    async show(status, options) {
      await page.evaluate((message) => {
        window.dispatchEvent(new MessageEvent('message', { data: message }));
      }, view(status, options));
      await page.clock.runFor(50);
    },
    async violations() {
      return page.evaluate(() => window.__verify.violations);
    },
    async posted() {
      return page.evaluate(() => window.__verify.posted);
    },
  };
}

/** Each `--vscode-*` value of a theme as the browser computes it, for comparing colours. */
export async function computedThemeColours(page, theme) {
  return page.evaluate((vars) => {
    const probe = document.createElement('span');
    document.body.append(probe);
    const found = {};
    for (const [name, value] of Object.entries(vars)) {
      if (!name.startsWith('--vscode-') || /font|size|weight/.test(name)) continue;
      probe.style.color = '';
      probe.style.color = value;
      if (probe.style.color) found[name] = getComputedStyle(probe).color;
    }
    probe.remove();
    return found;
  }, theme.vars);
}
