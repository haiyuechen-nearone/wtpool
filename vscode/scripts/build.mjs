import { execFileSync } from 'node:child_process';
import { copyFileSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import * as esbuild from 'esbuild';

const VSCODE = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

function packageFile(specifier) {
  return createRequire(import.meta.url).resolve(specifier);
}

function typeCheck() {
  const tsc = packageFile('typescript/bin/tsc');
  for (const project of ['tsconfig.json', 'tsconfig.webview.json']) {
    try {
      execFileSync(process.execPath, [tsc, '--project', project], { cwd: VSCODE, stdio: 'inherit' });
    } catch {
      process.stderr.write(`build: ${project} does not type check\n`);
      process.exit(1);
    }
  }
}

async function bundle() {
  const common = { absWorkingDir: VSCODE, bundle: true, sourcemap: true, logLevel: 'warning', legalComments: 'eof' };
  const host = { ...common, platform: 'node', format: 'cjs', target: 'node20', external: ['vscode'] };
  const webview = { ...common, platform: 'browser', target: 'chrome120' };
  await Promise.all([
    esbuild.build({ ...host, entryPoints: ['src/extension.ts'], outfile: 'out/extension.js' }),
    // test/support/webview.mjs loads the page builder on its own, in plain Node.
    esbuild.build({ ...host, entryPoints: ['src/html.ts'], outfile: 'out/html.js' }),
    esbuild.build({ ...webview, format: 'iife', entryPoints: ['src/webview/main.ts'], outfile: 'media/webview.js' }),
  ]);
}

/** The sheets style.css imports, in its order. It holds only imports, so a page can link it unbuilt. */
function styleParts(dir) {
  return readFileSync(path.join(dir, 'style.css'), 'utf8')
    .trimEnd()
    .split('\n')
    .map((line) => {
      const part = /^@import url\("(.+)"\);$/.exec(line)?.[1];
      if (part === undefined) {
        process.stderr.write(`build: style.css holds more than imports: ${line}\n`);
        process.exit(1);
      }
      return part;
    });
}

function writeStyleSheet() {
  // The webview page links a single stylesheet.
  const dir = path.join(VSCODE, 'src/webview');
  const sheets = ['icons.css', ...styleParts(dir)].map((file) => readFileSync(path.join(dir, file), 'utf8'));
  writeFileSync(path.join(VSCODE, 'media/webview.css'), sheets.join('\n'));
}

function copyIconFont() {
  // icons.css declares the font and only the glyphs the webview uses, so codicon.css stays out.
  copyFileSync(packageFile('@vscode/codicons/dist/codicon.ttf'), path.join(VSCODE, 'media/codicon.ttf'));
}

typeCheck();
rmSync(path.join(VSCODE, 'out'), { recursive: true, force: true });
await bundle();
writeStyleSheet();
copyIconFont();
