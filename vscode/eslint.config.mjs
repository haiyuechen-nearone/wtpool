// ESLint for the extension (verification.md, "Layers", static). An oracle file: the builder
// never changes it. It applies the recommended rules and every group of the oracle's rules in
// vscode/test/lint/rules.mjs. Generated files (`*.generated.*` or a `generated/` folder) are
// left to the named lint checks, which lint every source file.
import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import globals from 'globals';
import tseslint from 'typescript-eslint';

import { rulesOf } from './test/lint/rules.mjs';

export default defineConfig(
  {
    ignores: [
      'node_modules/',
      'out/',
      'media/',
      'test/',
      '*.vsix',
      '**/*.generated.*',
      '**/generated/**',
    ],
  },
  js.configs.recommended,
  tseslint.configs.recommended,
  {
    files: ['**/*.{ts,mts,cts,tsx,js,mjs,cjs,jsx}'],
    languageOptions: {
      ecmaVersion: 'latest',
      sourceType: 'module',
      globals: { ...globals.browser, ...globals.node },
    },
    linterOptions: { noInlineConfig: true, reportUnusedDisableDirectives: 'error' },
    rules: {
      ...rulesOf(),
      '@typescript-eslint/ban-ts-comment': ['error', { 'ts-expect-error': true, 'ts-ignore': true, 'ts-nocheck': true }],
    },
  },
);
