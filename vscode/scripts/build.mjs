// `npm run build`: type checks both sides of the extension, then bundles them with esbuild,
// with source maps (verification.md, "Test interfaces", item 4):
//
//   src/extension.ts       → out/extension.js   the extension host, CommonJS, `vscode` external
//   src/html.ts            → out/html.js        buildHtml(webview, nonce), loadable in plain Node
//   src/webview/main.ts    → media/webview.js   the webview script
//   src/webview/icons.css  → media/webview.css  the webview style sheet: the icon glyphs, then
//   src/webview/style.css                       the layout and colours, joined by a newline,
//                                               as stylelint lints media/ after the build
//   @vscode/codicons       → media/codicon.ttf  the icon font only; icons.css declares it (U6)
import { execFileSync } from 'node:child_process';
import { copyFileSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import * as esbuild from 'esbuild';

const VSCODE = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const tsc = createRequire(import.meta.url).resolve('typescript/bin/tsc');

for (const project of ['tsconfig.json', 'tsconfig.webview.json']) {
  try {
    execFileSync(process.execPath, [tsc, '--project', project], { cwd: VSCODE, stdio: 'inherit' });
  } catch {
    process.stderr.write(`build: ${project} does not type check\n`);
    process.exit(1);
  }
}

rmSync(path.join(VSCODE, 'out'), { recursive: true, force: true });

const common = {
  absWorkingDir: VSCODE,
  bundle: true,
  sourcemap: true,
  logLevel: 'warning',
  // Keep the licence comments of bundled packages, at the end of each bundle.
  legalComments: 'eof',
};
const host = { ...common, platform: 'node', format: 'cjs', target: 'node20', external: ['vscode'] };
const webview = { ...common, platform: 'browser', target: 'chrome120' };

await Promise.all([
  esbuild.build({ ...host, entryPoints: ['src/extension.ts'], outfile: 'out/extension.js' }),
  esbuild.build({ ...host, entryPoints: ['src/html.ts'], outfile: 'out/html.js' }),
  esbuild.build({ ...webview, format: 'iife', entryPoints: ['src/webview/main.ts'], outfile: 'media/webview.js' }),
]);
// One sheet, so the page still links one stylesheet (X8): the glyphs, then the layout and colours.
const sheets = ['icons.css', 'style.css'].map((file) => readFileSync(path.join(VSCODE, 'src/webview', file), 'utf8'));
writeFileSync(path.join(VSCODE, 'media/webview.css'), sheets.join('\n'));
// The icon font only (U6). codicon.css stays out: icons.css declares the font and the glyphs it uses.
copyFileSync(
  createRequire(import.meta.url).resolve('@vscode/codicons/dist/codicon.ttf'),
  path.join(VSCODE, 'media/codicon.ttf'),
);
