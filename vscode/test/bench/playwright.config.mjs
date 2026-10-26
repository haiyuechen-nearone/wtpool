// Budgets measured in Playwright's Chromium (B8). A budget may take 120 s and is not retried.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from '@playwright/test';

const here = path.dirname(fileURLToPath(import.meta.url));
const runDir = process.env.WTPOOL_VERIFY_RUN_DIR ?? path.join(here, '..', '..', '..', 'verify', 'out', 'local');

export default defineConfig({
  testDir: here,
  testMatch: '*.spec.mjs',
  timeout: 120_000,
  retries: 0,
  workers: 1,
  outputDir: path.join(runDir, 'bench', 'artifacts'),
  reporter: [['json', { outputFile: path.join(runDir, 'bench', 'results.json') }], ['list']],
  use: { browserName: 'chromium', headless: true, viewport: { width: 1100, height: 800 } },
});
