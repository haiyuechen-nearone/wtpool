import { readFileSync } from 'node:fs';
import path from 'node:path';

import { REPO, VSCODE } from '../../test/support/paths.mjs';

const FEATURE_NAMES = ['labels', 'outputs', 'background'];

/** Detects the later features of this checkout that the story can show. */
export function detectFeatures() {
  const spec = readFileSync(path.join(REPO, 'docs', 'design', 'status-panel', 'status-panel.md'), 'utf8');
  const specRow = (id) => spec.split('\n').find((line) => line.startsWith(`| ${id} |`)) ?? '';
  const schema = JSON.parse(readFileSync(path.join(VSCODE, 'src', 'generated', 'status.schema.json'), 'utf8'));
  return {
    labels: /the session's title/.test(specRow('H16')),
    outputs: Boolean(schema.$defs?.actor?.properties?.outputs),
    background: specRow('A1').includes('the tool `background:`'),
  };
}

/** Parses `--features`: `all`, `none` or a comma list of feature names, `detected` when absent. */
export function parseFeatures(value, detected) {
  if (value === undefined) {
    return detected;
  }
  if (value === 'all') {
    return Object.fromEntries(FEATURE_NAMES.map((name) => [name, true]));
  }
  const named = value === 'none' ? [] : value.split(',').filter(Boolean);
  const unknown = named.filter((name) => !FEATURE_NAMES.includes(name));
  if (unknown.length > 0) {
    throw new Error(`demo: unknown feature ${unknown.join(', ')}; known: ${FEATURE_NAMES.join(', ')}, all, none`);
  }
  return Object.fromEntries(FEATURE_NAMES.map((name) => [name, named.includes(name)]));
}
