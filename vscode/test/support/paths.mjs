import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const TEST = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
export const VSCODE = path.dirname(TEST);
export const REPO = path.dirname(VSCODE);
export const MEDIA = path.join(VSCODE, 'media');
export const OUT_HTML = path.join(VSCODE, 'out', 'html.js');
export const OUT_EXTENSION = path.join(VSCODE, 'out', 'extension.js');
export const WEBVIEW_JS = path.join(MEDIA, 'webview.js');
export const VSIX = path.join(VSCODE, 'wtpool.vsix');
export const SCENARIOS = path.join(REPO, 'verify', 'scenarios');
export const THEMES = path.join(REPO, 'verify', 'themes');
export const SCHEMA = path.join(REPO, 'verify', 'schema', 'status.schema.json');
/** {@link SCHEMA} with `machine.jobs` and `build.queued` added. */
export const SCHEMA_PHASE_4 = path.join(REPO, 'verify', 'schema', 'status.phase-4.schema.json');
export const FAKE_WTPOOL = path.join(TEST, 'fake-wtpool.mjs');

/** The run folder `cargo xtask verify` gives this layer. */
export function runDir() {
  const dir = process.env.WTPOOL_VERIFY_RUN_DIR;
  if (!dir) {
    throw new Error('setup: WTPOOL_VERIFY_RUN_DIR is not set; run the layer through cargo xtask verify');
  }
  return dir;
}

/** WTPOOL_VERIFY_PHASE, 1 by default. */
export const PHASE = Number(process.env.WTPOOL_VERIFY_PHASE ?? 1);
