// Runs one group of vscode/test/lint/rules.mjs over the extension's sources with ESLint (and,
// for `colour`, stylelint over its CSS), inline disable comments ignored, and prints
// `{"findings": ["file:line: message", ...]}`, or `{"error": "..."}` when a linter cannot run.
// ESLint, typescript-eslint and stylelint come from the product's locked vscode/node_modules.
import { createRequire } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

import { VSCODE } from '../support/paths.mjs';
import { SOURCES, STYLES, colourStylelintRules, groups, rulesOf } from './rules.mjs';

const group = process.argv[2];
const requireFromProduct = createRequire(path.join(VSCODE, 'package.json'));
const load = async (name) => import(pathToFileURL(requireFromProduct.resolve(name)).href);

async function eslintFindings() {
  const { ESLint } = await load('eslint');
  const tseslint = (await load('typescript-eslint')).default;
  const eslint = new ESLint({
    cwd: VSCODE,
    overrideConfigFile: true,
    allowInlineConfig: false,
    errorOnUnmatchedPattern: false,
    overrideConfig: [
      {
        files: ['**/*.{ts,mts,cts,tsx,js,mjs,cjs,jsx}'],
        languageOptions: { parser: tseslint.parser, ecmaVersion: 'latest', sourceType: 'module' },
        linterOptions: { reportUnusedDisableDirectives: 'off' },
        rules: rulesOf([group]),
      },
    ],
  });
  const results = await eslint.lintFiles(SOURCES);
  return results.flatMap((result) =>
    result.messages.map(
      (message) =>
        `${path.relative(VSCODE, result.filePath)}:${message.line ?? 0}: ${message.ruleId ?? 'parse'}: ${message.message}`,
    ),
  );
}

async function stylelintFindings() {
  const stylelint = (await load('stylelint')).default;
  const { results } = await stylelint.lint({
    files: STYLES,
    cwd: VSCODE,
    config: { rules: colourStylelintRules },
    configBasedir: VSCODE,
    allowEmptyInput: true,
    ignoreDisables: true,
  });
  return results.flatMap((result) => [
    ...result.warnings.map(
      (warning) => `${path.relative(VSCODE, result.source)}:${warning.line}: ${warning.rule}: ${warning.text}`,
    ),
    ...(result.parseErrors ?? []).map((error) => `${path.relative(VSCODE, result.source)}: parse: ${error.text}`),
  ]);
}

try {
  if (!groups[group]) throw new Error(`setup: unknown group ${group}`);
  const findings = await eslintFindings();
  if (group === 'colour') findings.push(...(await stylelintFindings()));
  process.stdout.write(`${JSON.stringify({ findings })}\n`);
} catch (error) {
  process.stdout.write(`${JSON.stringify({ error: String(error?.stack ?? error) })}\n`);
}
