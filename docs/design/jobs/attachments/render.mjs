// Renders the design mockups of docs/design/jobs/ and docs/design/status-panel/ to PNG, through
// the panel's real stylesheet (vscode/src/webview/style.css, which imports style/icons.css) and the
// theme values of verify/themes/, in the Chromium of vscode/test's Playwright, at 1 device pixel
// per CSS pixel as the approved screenshots are. Each frame takes the theme its `?theme=` names, dark by
// default, the way the shot layer sets a theme: the variables on <html>, the body classes.
// Every request is answered from the repository; nothing reaches the network.
//
//   node docs/design/jobs/attachments/render.mjs [copy-folder]   (after npm ci in vscode/ and vscode/test/)
import { copyFileSync, existsSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const { chromium } = createRequire(path.join(REPO, 'vscode/test/package.json'))('playwright');
const ORIGIN = 'https://mockup.test';
const THEMES = ['dark', 'light', 'high-contrast', 'high-contrast-light'];
const CODICON = path.join(REPO, 'vscode/node_modules/@vscode/codicons/dist/codicon.ttf');
const TYPES = { '.html': 'text/html', '.css': 'text/css', '.ttf': 'font/ttf', '.js': 'text/javascript' };

const JOBS = 'docs/design/jobs/attachments';
const PANEL = 'docs/design/status-panel/attachments';
/** Each mockup: its page, its image, and the viewport in CSS pixels. */
const PAGES = [
  { html: `${JOBS}/EditOptions.html`, png: `${JOBS}/EditOptions.png`, width: 2284, height: 764 },
  { html: `${JOBS}/Strip.html`, png: `${JOBS}/Strip.png`, width: 1100, height: 640 },
  { html: `${JOBS}/Strip.html`, png: `${JOBS}/Strip-600.png`, width: 600, height: 1230 },
  { html: `${JOBS}/PatternError.html`, png: `${JOBS}/PatternError.png`, width: 1100, height: 640 },
  { html: `${JOBS}/FirstOpen.html`, png: `${JOBS}/FirstOpen.png`, width: 1100, height: 660 },
  { html: `${JOBS}/ChipFocus.html`, png: `${JOBS}/ChipFocus.png`, width: 1100, height: 620 },
  { html: `${JOBS}/StripStates.html`, png: `${JOBS}/StripStates.png`, width: 1142, height: 3960 },
  { html: `${JOBS}/Heading.html`, png: `${JOBS}/Heading.png`, width: 1142, height: 866 },
  { html: `${JOBS}/NotQueued.html`, png: `${JOBS}/NotQueued.png`, width: 1100, height: 330 },
  { html: `${JOBS}/Measured.html`, png: `${JOBS}/Measured.png`, width: 1100, height: 900 },
  { html: `${JOBS}/Measured.html`, png: `${JOBS}/Measured-600.png`, width: 600, height: 1240 },
  { html: `${JOBS}/MeasuredDismissed.html`, png: `${JOBS}/MeasuredDismissed.png`, width: 1100, height: 980 },
  { html: `${PANEL}/SeveralAgents.html`, png: `${PANEL}/SeveralAgents-1100.png`, width: 1100, height: 440 },
  { html: `${PANEL}/SeveralAgents.html`, png: `${PANEL}/SeveralAgents-600.png`, width: 600, height: 440 },
  { html: `${PANEL}/SeveralAgentsThemes.html`, png: `${PANEL}/SeveralAgentsThemes.png`, width: 1142, height: 1912 },
];

const themes = Object.fromEntries(
  THEMES.map((name) => [name, JSON.parse(readFileSync(path.join(REPO, 'verify/themes', `${name}.json`), 'utf8'))]),
);

/** Runs in every frame before its scripts: the theme of `?theme=` on <html> and <body>. */
function applyTheme(all) {
  const theme = all[new URLSearchParams(location.search).get('theme') ?? 'dark'] ?? all.dark;
  let rooted = false;
  const apply = () => {
    if (!rooted && document.documentElement) {
      rooted = true;
      for (const [name, value] of Object.entries(theme.vars)) document.documentElement.style.setProperty(name, value);
    }
    if (!document.body) return false;
    document.body.classList.add(...theme.bodyClass.split(' '));
    return true;
  };
  if (!apply()) {
    const observer = new MutationObserver(() => {
      if (apply()) observer.disconnect();
    });
    observer.observe(document, { childList: true, subtree: true });
  }
}

async function serve(route) {
  const url = new URL(route.request().url());
  if (url.origin !== ORIGIN) return route.abort();
  const relative = decodeURIComponent(url.pathname.slice(1));
  // icons.css loads codicon.ttf beside itself; the build copies it from @vscode/codicons.
  const file = relative.endsWith('/codicon.ttf') ? CODICON : path.join(REPO, relative);
  if (!file.startsWith(REPO) || !existsSync(file)) return route.fulfill({ status: 404, body: '' });
  return route.fulfill({ status: 200, contentType: TYPES[path.extname(file)] ?? 'application/octet-stream', body: readFileSync(file) });
}

const copyTo = process.argv[2];
const browser = await chromium.launch();
for (const { html, png, width, height } of PAGES) {
  const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 1 });
  await page.route('**/*', serve);
  await page.addInitScript(applyTheme, themes);
  await page.goto(`${ORIGIN}/${html}`);
  await page.waitForLoadState('load');
  await page.evaluate(() => document.fonts.ready);
  for (const frame of page.frames()) await frame.evaluate(() => document.fonts.ready);
  const out = path.join(REPO, png);
  await page.screenshot({ path: out, animations: 'disabled' });
  if (copyTo) copyFileSync(out, path.join(copyTo, path.basename(png)));
  await page.close();
  console.log(png);
}
await browser.close();
