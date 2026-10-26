// The TOML of the heavy job flows: the machine's config and a repository's patterns are written by
// hand, so a Given never depends on the CLI under test.
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { machineConfigFile, writeMachineConfig } from '../support/machine-config.mjs';

/** The sandbox's machine.toml. */
export function machineTomlPath(dirs) {
  return machineConfigFile(dirs.home);
}

/** Writes machine.toml by hand, `[jobs]` with the keys given, or `text` as the whole file. */
export function writeMachineToml(dirs, config = {}) {
  return writeMachineConfig(dirs.home, config);
}

/**
 * Writes a machine.toml that makes `cargo` a heavy pattern and flags nothing, so a
 * `cargo` outside a job reads as it does in a document without `machine.jobs`: `build: cargo`.
 */
export function unflaggedCargo(dirs) {
  writeMachineToml(dirs, { enforce: 'off', heavy: ['cargo'] });
}

export function readText(file) {
  return existsSync(file) ? readFileSync(file, 'utf8') : null;
}

/** `<common>/wtpool.toml`, written by the pool builder with the slot keys. */
export function repoTomlPath(poolInfo) {
  return path.join(poolInfo.common, 'wtpool.toml');
}

/**
 * Appends top level keys to the repository's wtpool.toml: `heavy`, `build_processes`, and
 * `comment` as a comment line of its own.
 */
export function addRepoKeys(poolInfo, { heavy, buildProcesses, comment } = {}) {
  const file = repoTomlPath(poolInfo);
  const lines = [readFileSync(file, 'utf8').trimEnd()];
  if (comment) lines.push(`# ${comment}`);
  if (heavy) lines.push(`heavy = [${heavy.map((p) => JSON.stringify(p)).join(', ')}]`);
  if (buildProcesses) lines.push(`build_processes = [${buildProcesses.map((p) => JSON.stringify(p)).join(', ')}]`);
  writeFileSync(file, `${lines.join('\n')}\n`);
  return file;
}

/** The strings of a TOML array key `key`, by a plain reading of one `key = [...]` line, or null. */
export function tomlList(text, key) {
  const match = (text ?? '').match(new RegExp(`^\\s*${key}\\s*=\\s*\\[([^\\]]*)\\]`, 'm'));
  if (!match) return null;
  return [...match[1].matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => JSON.parse(`"${m[1]}"`));
}

/** The value of a TOML scalar key, as written, or null. */
export function tomlValue(text, key) {
  const match = (text ?? '').match(new RegExp(`^\\s*${key}\\s*=\\s*([^#\\n]+)`, 'm'));
  return match ? match[1].trim() : null;
}
