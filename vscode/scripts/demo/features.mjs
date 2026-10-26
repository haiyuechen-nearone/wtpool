// Which later product features this checkout has, so the story reads as this version shows it
// (story.mjs). Each is read from the product's own contract, never from a branch name.
import { readFileSync } from 'node:fs';
import path from 'node:path';

import { REPO, VSCODE } from '../../test/support/paths.mjs';

/** The names `--features` takes. */
export const NAMES = ['labels', 'outputs', 'background'];

/**
 * `labels`: the spec's H16 names the session's title as its label (and A1 lets a session work
 * on a build it started), the live-labels work. `outputs`: the extension's generated schema
 * gives an actor `outputs`, the live-output work. `background`: A1 keeps an actor whose turn
 * ended working while its own background task runs, its action `background: <summary>`.
 */
export function detect() {
  const spec = readFileSync(path.join(REPO, 'docs', 'design', 'status-panel', 'status-panel.md'), 'utf8');
  const row = (id) => spec.split('\n').find((line) => line.startsWith(`| ${id} |`)) ?? '';
  const schema = JSON.parse(readFileSync(path.join(VSCODE, 'src', 'generated', 'status.schema.json'), 'utf8'));
  return {
    labels: /the session's title/.test(row('H16')),
    outputs: Boolean(schema.$defs?.actor?.properties?.outputs),
    background: row('A1').includes('the tool `background:`'),
  };
}

/** `--features=all`, `none` or a comma list of NAMES, over what `detect` found. */
export function parse(value, detected) {
  if (value === undefined) return detected;
  if (value === 'all') return Object.fromEntries(NAMES.map((name) => [name, true]));
  const named = value === 'none' ? [] : value.split(',').filter(Boolean);
  const unknown = named.filter((name) => !NAMES.includes(name));
  if (unknown.length) throw new Error(`demo: unknown feature ${unknown.join(', ')}; known: ${NAMES.join(', ')}, all, none`);
  return Object.fromEntries(NAMES.map((name) => [name, named.includes(name)]));
}
