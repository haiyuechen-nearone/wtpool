#!/usr/bin/env node
// Renders the Marketplace icon, media/marketplace-icon.png, from its source
// media/marketplace-icon.svg, with the Chromium of Playwright that the oracle in vscode/test
// installs (its dev dependency @playwright/test). Run it after an edit of the SVG and commit both.
//
//   node scripts/icon.mjs [--size <px>] [--out <png>]
//
// The default is the 128 x 128 PNG that package.json names as `icon`. `--size 32 --out <png>`
// shows how the icon reads small. Without Playwright's Chromium, run `npx playwright install
// chromium` in vscode/test first; a verify run of the shot layer also installs it.
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const VSCODE = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const SOURCE = path.join(VSCODE, 'media', 'marketplace-icon.svg');

const { values: args } = parseArgs({
  options: {
    size: { type: 'string', default: '128' },
    out: { type: 'string', default: path.join(VSCODE, 'media', 'marketplace-icon.png') },
  },
});
const size = Number(args.size);
if (!Number.isInteger(size) || size < 16 || size > 1024) {
  process.stderr.write(`icon: --size takes a whole number of pixels from 16 to 1024, not ${args.size}\n`);
  process.exit(2);
}

const { chromium } = createRequire(path.join(VSCODE, 'test', 'package.json'))('@playwright/test');
const svg = readFileSync(SOURCE, 'utf8');
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: size, height: size }, deviceScaleFactor: 1 });
  await page.setContent(
    `<!DOCTYPE html><style>html, body { margin: 0; background: transparent; } ` +
      `svg { display: block; width: ${size}px; height: ${size}px; }</style>${svg}`,
  );
  await page.screenshot({ path: args.out, omitBackground: true, clip: { x: 0, y: 0, width: size, height: size } });
  process.stdout.write(`icon: wrote ${args.out} (${size} x ${size})\n`);
} finally {
  await browser.close();
}
