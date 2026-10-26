// The shot layer: headless Chromium on the page buildHtml writes (support/page.mjs).
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from '@playwright/test';

const here = path.dirname(fileURLToPath(import.meta.url));
const runDir = process.env.WTPOOL_VERIFY_RUN_DIR ?? path.join(here, '..', '..', '..', 'verify', 'out', 'local');

export default defineConfig({
  testDir: here,
  testMatch: '*.spec.mjs',
  timeout: 180_000,
  // freeze sets WTPOOL_VERIFY_RETRIES=0, as it only needs each new check to fail once.
  retries: Number(process.env.WTPOOL_VERIFY_RETRIES ?? 2),
  workers: 4,
  fullyParallel: true,
  snapshotPathTemplate: path.join(here, '..', '..', '..', 'verify', 'shots', 'approved', '{arg}{ext}'),
  outputDir: path.join(runDir, 'shot', 'artifacts'),
  reporter: [['json', { outputFile: path.join(runDir, 'shot', 'results.json') }], ['list']],
  use: {
    browserName: 'chromium',
    headless: true,
    viewport: { width: 1100, height: 800 },
    deviceScaleFactor: 1,
    colorScheme: 'dark',
    timezoneId: 'UTC',
    locale: 'en-US',
  },
});
