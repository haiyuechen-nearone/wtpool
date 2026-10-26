// The machine config of a sandbox home, written by hand so no check depends on the CLI under test.
import { chmodSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

/** The `machine.toml` of `home`, as the sandbox's XDG_CONFIG_HOME is `<home>/.config`. */
export function machineConfigFile(home) {
  return path.join(home, '.config', 'wtpool', 'machine.toml');
}

/**
 * Writes the `machine.toml` of `home`, folder 0700 and file 0600, and returns its path. The file is
 * `text` when given, else a `[jobs]` table with the keys given.
 */
export function writeMachineConfig(home, { limit, enforce, heavy, text } = {}) {
  const file = machineConfigFile(home);
  mkdirSync(path.dirname(file), { recursive: true });
  chmodSync(path.dirname(file), 0o700);
  const lines = ['# wtpool-verify: the machine config of this sandbox', '[jobs]'];
  if (limit !== undefined) lines.push(`limit = ${limit}`);
  if (enforce !== undefined) lines.push(`enforce = "${enforce}"`);
  if (heavy !== undefined) lines.push(`heavy = [${heavy.map((p) => JSON.stringify(p)).join(', ')}]`);
  writeFileSync(file, text ?? `${lines.join('\n')}\n`, { mode: 0o600 });
  chmodSync(file, 0o600);
  return file;
}
