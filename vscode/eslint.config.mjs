// It applies the recommended rules and every group of the oracle's rules in
// vscode/test/lint/rules.mjs. Generated files (`*.generated.*` or a `generated/` folder) are
// left to the named lint checks, which lint every source file.
//
// On top, the rules of the Google TypeScript Style Guide that ESLint can check. naming-convention
// stays off, as the wire names are snake_case; so do no-unnecessary-condition and
// switch-exhaustiveness-check.
import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import globals from 'globals';
import tseslint from 'typescript-eslint';

import { rulesOf } from './test/lint/rules.mjs';

const GOOGLE = 'Google TypeScript Style Guide';
const NAMED_FUNCTION = `${GOOGLE}: a named function is a function declaration`;
const lintRules = rulesOf();

/** The guide's rules that no ESLint rule checks, as no-restricted-syntax entries. */
const googleSyntax = [
  // A type annotation is the guide's one reason for an arrow function.
  ...['Program', 'Program > ExportNamedDeclaration'].map((parent) => ({
    selector: `${parent} > VariableDeclaration > VariableDeclarator[init.type=/^(ArrowFunctionExpression|FunctionExpression)$/]:not([id.typeAnnotation])`,
    message: NAMED_FUNCTION,
  })),
  {
    selector: ':matches(TSPropertySignature, PropertyDefinition) > TSTypeAnnotation > TSUnionType > TSUndefinedKeyword',
    message: `${GOOGLE}: an optional field, not one typed \`| undefined\``,
  },
  { selector: 'PrivateIdentifier', message: `${GOOGLE}: the private modifier, not a #private field` },
  { selector: 'TSEnumDeclaration[const=true]', message: `${GOOGLE}: an enum, not a const enum` },
  // curly is off: Prettier puts a long `if` body on its own line, which `multi-line` rejects. A loop
  // body still takes a block.
  {
    selector:
      ':matches(ForStatement, ForInStatement, ForOfStatement, WhileStatement, DoWhileStatement)[body.type!="BlockStatement"]',
    message: `${GOOGLE}: a loop body is a block`,
  },
];

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
      ...lintRules,
      '@typescript-eslint/ban-ts-comment': ['error', { 'ts-expect-error': true, 'ts-ignore': true, 'ts-nocheck': true }],
      // A rule set twice keeps only its last options, so the guide's entries join the oracle's.
      'no-restricted-syntax': [...lintRules['no-restricted-syntax'], ...googleSyntax],
      // The guide lets an `if` elide its block, and Prettier moves a long body to its own line,
      // which `multi-line` rejects.
      curly: 'off',
      eqeqeq: ['error', 'always', { null: 'ignore' }],
      'no-duplicate-imports': ['error', { allowSeparateTypeImports: true }],
      'prefer-const': 'error',
    },
  },
  {
    files: ['**/*.{ts,mts,cts,tsx}'],
    rules: {
      '@typescript-eslint/array-type': ['error', { default: 'array-simple' }],
      '@typescript-eslint/consistent-type-assertions': [
        'error',
        { assertionStyle: 'as', objectLiteralTypeAssertions: 'never' },
      ],
      '@typescript-eslint/consistent-type-definitions': ['error', 'interface'],
      '@typescript-eslint/consistent-type-imports': 'error',
      '@typescript-eslint/explicit-member-accessibility': ['error', { accessibility: 'no-public' }],
      '@typescript-eslint/no-non-null-assertion': 'error',
      '@typescript-eslint/parameter-properties': ['error', { prefer: 'parameter-property' }],
    },
  },
  {
    // The type-aware rules, through the two programs scripts/build.mjs type checks.
    files: ['src/**/*.ts'],
    languageOptions: {
      parserOptions: {
        project: ['./tsconfig.json', './tsconfig.webview.json'],
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // The guide's own way to drop an arrow shorthand's value is `void`.
      '@typescript-eslint/no-confusing-void-expression': ['error', { ignoreVoidOperator: true }],
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
      '@typescript-eslint/only-throw-error': 'error',
      '@typescript-eslint/prefer-readonly': 'error',
      '@typescript-eslint/unbound-method': 'error',
    },
  },
);
