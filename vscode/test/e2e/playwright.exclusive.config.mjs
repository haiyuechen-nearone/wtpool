// The e2e checks that need the machine to themselves (flows-exclusive*.spec.mjs): one worker,
// one check at a time, run by layers/e2e.rs after the parallel checks of playwright.config.mjs.
import path from 'node:path';
import { defineConfig } from '@playwright/test';

import base, { runDir } from './playwright.config.mjs';

export default defineConfig({
  ...base,
  testMatch: 'flows-exclusive*.spec.mjs',
  testIgnore: [],
  workers: 1,
  fullyParallel: false,
  outputDir: path.join(runDir, 'e2e', 'artifacts-exclusive'),
  reporter: [['json', { outputFile: path.join(runDir, 'e2e', 'results-exclusive.json') }], ['list']],
});
