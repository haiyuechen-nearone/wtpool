#!/usr/bin/env node
/**
 * @fileoverview Renders media/marketplace-icon.svg to the Marketplace icon PNG with the Chromium
 * of Playwright, a dev dependency of vscode/test. Run it after an edit of the SVG and commit both.
 *
 *     node scripts/icon.mjs [--size <px>] [--out <png>]
 *
 * The default is the 128 x 128 media/marketplace-icon.png that package.json names. Without
 * Playwright's Chromium, run `npx playwright install chromium` in vscode/test first.
 */
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const VSCODE = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const SOURCE = path.join(VSCODE, 'media', 'marketplace-icon.svg');
const MIN_SIZE = 16;
const MAX_SIZE = 1024;

const { values: args } = parseArgs({
  options: {
    size: { type: 'string', default: '128' },
    out: { type: 'string', default: path.join(VSCODE, 'media', 'marketplace-icon.png') },
  },
});

function sizeOrExit(text) {
  const size = Number(text);
  if (!Number.isInteger(size) || size < MIN_SIZE || size > MAX_SIZE) {
    process.stderr.write(
      `icon: --size takes a whole number of pixels from ${MIN_SIZE} to ${MAX_SIZE}, not ${text}\n`,
    );
    process.exit(2);
  }
  return size;
}

async function writeIcon(size, out) {
  const { chromium } = createRequire(path.join(VSCODE, 'test', 'package.json'))('@playwright/test');
  const svg = readFileSync(SOURCE, 'utf8');
  const browser = await chromium.launch();
  try {
    const page = await browser.newPage({
      viewport: { width: size, height: size },
      deviceScaleFactor: 1,
    });
    await page.setContent(
      `<!DOCTYPE html><style>html, body { margin: 0; background: transparent; } ` +
        `svg { display: block; width: ${size}px; height: ${size}px; }</style>${svg}`,
    );
    await page.screenshot({
      path: out,
      omitBackground: true,
      clip: { x: 0, y: 0, width: size, height: size },
    });
    process.stdout.write(`icon: wrote ${out} (${size} x ${size})\n`);
  } finally {
    await browser.close();
  }
}

await writeIcon(sizeOrExit(args.size), args.out);
