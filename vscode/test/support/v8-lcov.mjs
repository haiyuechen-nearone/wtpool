// Turns a NODE_V8_COVERAGE folder of the render or host layer into an lcov file over vscode/src,
// through the source maps of the extension's build.
// Usage: node support/v8-lcov.mjs <coverage folder> <lcov file>
//
// Each script of vscode/ (not node_modules/ or test/) goes through v8-to-istanbul with the
// source map its build wrote, so a line counts the V8 hits of the innermost range spanning it:
// a function or branch no check reaches is 0. c8's Report is not used: it hands v8-to-istanbul
// an exclude filter that sees the source map's unresolved `../src/...` paths, excludes every
// range, and so leaves every line at its initial count of 1.
//
// Scripts the render layer runs in jsdom carry their served URL (support/webview.mjs), so they
// are mapped back to the file below vscode/ first. A script whose length as V8 saw it differs from
// its file's size on disk stops the conversion, as offsets into it would land on wrong lines.
// `c8 ignore` comments count for nothing. Without any script of vscode/ it writes no lcov, and
// `cover` fails. Files no script maps to drop out of the lcov, and `cover` lists their changed
// lines as not measured.
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { VSCODE } from './paths.mjs';
import { CSP_SOURCE } from './webview.mjs';

// c8, a locked dependency, brings both libraries.
const fromC8 = createRequire(createRequire(import.meta.url).resolve('c8/package.json'));
const v8toIstanbul = fromC8('v8-to-istanbul');
const { createCoverageMap } = fromC8('istanbul-lib-coverage');

const SRC = `${path.join(VSCODE, 'src')}${path.sep}`;
const PRODUCT = `${pathToFileURL(VSCODE).href}/`;
const OWN = [`${PRODUCT}node_modules/`, `${PRODUCT}test/`];

/**
 * The file URL of a script of vscode/, or undefined for any other script. The page's inline
 * script carries the page's URL, and no file below vscode/ holds it: it is a string of html.ts,
 * counted when Node loads out/html.js.
 */
export function productUrl(url) {
  if (url.split(/[?#]/)[0] === `${CSP_SOURCE}/index.html`) return undefined;
  const local = url.startsWith(`${CSP_SOURCE}/`)
    ? `${PRODUCT}${url.slice(CSP_SOURCE.length + 1).split(/[?#]/)[0]}`
    : url;
  if (!local.startsWith(PRODUCT) || OWN.some((prefix) => local.startsWith(prefix))) return undefined;
  return local;
}

/** The scripts of vscode/ in every coverage file of `folder`, as `{ file, functions }`. */
export function productScripts(folder) {
  const scripts = [];
  for (const name of readdirSync(folder).filter((entry) => entry.endsWith('.json')).sort()) {
    const data = JSON.parse(readFileSync(path.join(folder, name), 'utf8'));
    for (const script of data.result ?? []) {
      const url = productUrl(script.url);
      if (url) scripts.push({ file: fileURLToPath(url), functions: script.functions });
    }
  }
  return scripts;
}

/** The length V8 saw of a script: the end of its top level range. */
function scriptLength(functions) {
  return Math.max(0, ...functions.flatMap((fn) => fn.ranges.map((range) => range.endOffset)));
}

/** The line hits of each file below vscode/src/ that `scripts` map to, merged over scripts. */
export async function lineHits(scripts) {
  const map = createCoverageMap({});
  for (const { file, functions } of scripts) {
    const size = statSync(file).size;
    if (scriptLength(functions) !== size) {
      throw new Error(`setup: ${file} has ${size} bytes, but V8 covered ${scriptLength(functions)}`);
    }
    // No exclude filter: v8-to-istanbul calls it with unresolved source map paths.
    const converter = v8toIstanbul(file, 0);
    await converter.load();
    for (const { source } of converter.covSources) {
      for (const line of source.lines) line.ignore = false;
    }
    converter.applyCoverage(functions);
    const istanbul = converter.toIstanbul();
    for (const key of Object.keys(istanbul)) {
      if (!path.resolve(key).startsWith(SRC)) delete istanbul[key];
    }
    map.merge(istanbul);
  }
  const hits = new Map();
  for (const file of map.files().sort()) {
    const lines = map.fileCoverageFor(file).getLineCoverage();
    hits.set(
      path.resolve(file),
      Object.entries(lines)
        .map(([line, count]) => [Number(line), count])
        .sort((a, b) => a[0] - b[0]),
    );
  }
  return hits;
}

/** lcov text with one `DA:<line>,<hits>` per line of each file. */
export function lcov(hits) {
  let text = '';
  for (const [file, lines] of hits) {
    text += `SF:${file}\n`;
    for (const [line, count] of lines) text += `DA:${line},${count}\n`;
    text += `LF:${lines.length}\nLH:${lines.filter(([, count]) => count > 0).length}\nend_of_record\n`;
  }
  return text;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [coverage, out] = process.argv.slice(2);
  if (!coverage || !out) throw new Error('setup: usage: v8-lcov.mjs <coverage folder> <lcov file>');
  const scripts = existsSync(coverage) ? productScripts(coverage) : [];
  // A layer that ran the extension always loads a script of it, so none means the data is broken.
  if (!scripts.length) throw new Error(`setup: no script of vscode/ in the V8 coverage of ${coverage}`);
  const hits = await lineHits(scripts);
  mkdirSync(path.dirname(out), { recursive: true });
  writeFileSync(out, lcov(hits));
  let lines = 0;
  let covered = 0;
  for (const entries of hits.values()) {
    lines += entries.length;
    covered += entries.filter(([, count]) => count > 0).length;
  }
  process.stdout.write(
    `${scripts.length} scripts of vscode/ covered, ${covered} of ${lines} lines of ${hits.size} files in src/, lcov at ${out}\n`,
  );
}
