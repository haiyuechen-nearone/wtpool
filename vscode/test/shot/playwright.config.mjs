// The shot layer: Playwright's Chromium, headless, on buildHtml's page (support/page.mjs). A
// check may take 180 s and is retried twice (verification.md, "Outcomes and limits").
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from '@playwright/test';

const here = path.dirname(fileURLToPath(import.meta.url));
const runDir = process.env.WTPOOL_VERIFY_RUN_DIR ?? path.join(here, '..', '..', '..', 'verify', 'out', 'local');

export default defineConfig({
  testDir: here,
  testMatch: '*.spec.mjs',
  timeout: 180_000,
  // WTPOOL_VERIFY_RETRIES=0 when freeze only needs each new check to fail once; verify runs retry.
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
