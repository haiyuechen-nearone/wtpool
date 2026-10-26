// The e2e layer: real VS Codes with the packaged extension and the release wtpool, in parallel.
// A check may take 300 s and is retried twice (verification.md, "Outcomes and limits").
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from '@playwright/test';

const here = path.dirname(fileURLToPath(import.meta.url));
export const runDir = process.env.WTPOOL_VERIFY_RUN_DIR ?? path.join(here, '..', '..', '..', 'verify', 'out', 'local');

export default defineConfig({
  testDir: here,
  testMatch: '*.spec.mjs',
  // playwright.exclusive.config.mjs runs these alone, after this run.
  testIgnore: 'flows-exclusive*.spec.mjs',
  // Finds or downloads VS Code once, before the checks (verification.md, "Portability").
  globalSetup: path.join(here, 'global-setup.mjs'),
  timeout: 300_000,
  // WTPOOL_VERIFY_RETRIES=0 when freeze only needs each new check to fail once; verify runs retry.
  retries: Number(process.env.WTPOOL_VERIFY_RETRIES ?? 2),
  // Each check has its own sandbox and pool, so checks run in parallel.
  workers: Number(process.env.WTPOOL_VERIFY_E2E_WORKERS ?? 4),
  fullyParallel: true,
  outputDir: path.join(runDir, 'e2e', 'artifacts'),
  reporter: [['json', { outputFile: path.join(runDir, 'e2e', 'results.json') }], ['list']],
});
