// The render layer: Vitest in Node, with the webview in jsdom (support/webview.mjs).
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

const here = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  root: here,
  test: {
    include: ['*.test.mjs'],
    setupFiles: ['coverage.setup.mjs'],
    environment: 'node',
    testTimeout: 60_000,
    hookTimeout: 60_000,
    retry: 0,
    pool: 'forks',
    fileParallelism: true,
    // The product's build loads as Node loads it, untransformed, so its V8 coverage offsets
    // match the file and its source map (support/v8-lcov.mjs).
    server: { deps: { external: [/\/vscode\/out\//] } },
  },
});
