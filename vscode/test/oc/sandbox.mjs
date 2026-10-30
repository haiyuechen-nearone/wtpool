// The sandbox of the oc layer. OpenCode runs with a home, XDG folders and a temporary folder of
// its own, a fixed allowlist of variables, its background service turned off and the proxy of
// network.mjs for every host but loopback, so it never reaches the owner's OpenCode: its config,
// data, state and cache folders, its background service, or launchd. The sandbox lives in a short
// folder with no user name in its path, as a real model's prompt names the working directory.
import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
export const RECORDER = path.join(here, 'recorder.ts');

/** The folders of one oc run: {root, home, config, data, state, cache, tmp, bin, pools, logs}. */
export function makeDirs(runDir) {
  const root = mkdtempSync('/private/tmp/wtpool-oc-');
  const dirs = {
    root,
    home: path.join(root, 'home'),
    config: path.join(root, 'config'),
    data: path.join(root, 'data'),
    state: path.join(root, 'state'),
    cache: path.join(root, 'cache'),
    tmp: path.join(root, 'tmp'),
    bin: path.join(root, 'bin'),
    pools: path.join(root, 'pools'),
    logs: path.join(runDir, 'oc', 'logs'),
  };
  for (const dir of Object.values(dirs)) mkdirSync(dir, { recursive: true });
  return dirs;
}

/** The program folders OpenCode's shell finds `git`, `ls` and `printenv` in, the opencode one first. */
function programPath(opencode) {
  return [path.dirname(opencode), '/opt/homebrew/bin', '/usr/local/bin', '/usr/bin', '/bin', '/usr/sbin', '/sbin'].join(':');
}

/**
 * The whole environment of an `opencode` command: no variable of the runner's reaches it but
 * these. `proxy` is the port of the recording proxy; `extra` adds the session's variables.
 */
export function openCodeEnv(context, pool, extra = {}) {
  const { dirs } = context;
  const proxy = `http://127.0.0.1:${context.proxy.port}`;
  return {
    PATH: programPath(context.opencode),
    HOME: dirs.home,
    USER: 'me',
    LOGNAME: 'me',
    SHELL: '/bin/zsh',
    LANG: 'en_US.UTF-8',
    TERM: 'dumb',
    TZ: 'UTC',
    TMPDIR: `${dirs.tmp}/`,
    XDG_CONFIG_HOME: dirs.config,
    XDG_DATA_HOME: dirs.data,
    XDG_STATE_HOME: dirs.state,
    XDG_CACHE_HOME: dirs.cache,
    OPENCODE_DISABLE_AUTOUPDATE: '1',
    OPENCODE_DISABLE_MODELS_FETCH: '1',
    OPENCODE_DISABLE_CLAUDE_CODE: '1',
    OPENCODE_DISABLE_CLAUDE_CODE_SKILLS: '1',
    HTTPS_PROXY: proxy,
    HTTP_PROXY: proxy,
    https_proxy: proxy,
    http_proxy: proxy,
    NO_PROXY: '127.0.0.1,localhost,::1',
    no_proxy: '127.0.0.1,localhost,::1',
    GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_CONFIG_NOSYSTEM: '1',
    WTPOOL_VERIFY_SANDBOX: '1',
    ...(pool ? { WTPOOL_FOREGROUND: pool.fg, WTPOOL_SLOT_DIR: pool.dir } : {}),
    ...extra,
  };
}

/** The environment of the runner's own `wtpool` commands on `pool`. */
export function wtpoolEnv(context, pool, extra = {}) {
  const { dirs } = context;
  return {
    PATH: programPath(context.opencode),
    HOME: dirs.home,
    TMPDIR: `${dirs.tmp}/`,
    TZ: 'UTC',
    XDG_CONFIG_HOME: dirs.config,
    XDG_DATA_HOME: dirs.data,
    XDG_STATE_HOME: dirs.state,
    XDG_CACHE_HOME: dirs.cache,
    GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_CONFIG_NOSYSTEM: '1',
    WTPOOL_VERIFY_SANDBOX: '1',
    WTPOOL_FOREGROUND: pool.fg,
    WTPOOL_SLOT_DIR: pool.dir,
    ...extra,
  };
}

/** A fresh pool of the scenario catalog for one session: {scenario, dir, fg, common}. */
export function makePool(context, name) {
  const text = execFileSync(process.env.WTPOOL_VERIFY_XTASK, ['pool', '--scenario', 'agent_working_in_slot', '--dir', path.join(context.dirs.pools, name)], {
    encoding: 'utf8',
    timeout: 120_000,
  });
  return JSON.parse(text);
}

/**
 * The wrapper the plugin runs in place of `wtpool`: it keeps each run's argv, parent pid and stdin
 * in `$WTPOOL_VERIFY_OC_CALLS`, then becomes the build by `exec`, so the build's parent is still
 * the OpenCode server.
 */
export function writeWrapper(dirs, wtpool) {
  const file = path.join(dirs.bin, 'wtpool');
  const calls = path.join(dirs.root, 'calls');
  mkdirSync(calls, { recursive: true });
  const quote = (text) => `'${text.replaceAll("'", "'\\''")}'`;
  writeFileSync(
    file,
    [
      '#!/bin/sh',
      `dir="\${WTPOOL_VERIFY_OC_CALLS:-${calls}}"`,
      'f="$dir/$(/bin/date +%s)-$$"',
      '/bin/cat > "$f.in"',
      '{ printf \'%s\\n\' "$PPID"; for a in "$@"; do printf \'%s\\n\' "$a"; done; } > "$f.argv"',
      'exec 3< "$f.in"',
      `exec ${quote(wtpool)} "$@" <&3 3<&-`,
      '',
    ].join('\n'),
  );
  chmodSync(file, 0o755);
  return file;
}

/** The calls the wrapper kept in `dir`: {at, ppid, argv, stdin, payload}, oldest first. */
export function readCalls(dir) {
  let names = [];
  try {
    names = readdirSync(dir).filter((name) => name.endsWith('.argv'));
  } catch {
    return [];
  }
  const calls = names.map((name) => {
    const base = path.join(dir, name.slice(0, -'.argv'.length));
    const lines = readFileSync(`${base}.argv`, 'utf8').split('\n').slice(0, -1);
    let stdin = '';
    try {
      stdin = readFileSync(`${base}.in`, 'utf8');
    } catch {
      // A call cut short before its stdin was kept.
    }
    let payload = null;
    try {
      payload = JSON.parse(stdin);
    } catch {
      // A policy or garbage stdin.
    }
    return { at: lstatSync(`${base}.argv`).mtimeMs, ppid: Number(lines[0]), argv: lines.slice(1), stdin, payload };
  });
  return calls.sort((a, b) => a.at - b.at);
}

/** The sandbox's OpenCode configuration: the scripted model, then `override` merged in. */
export function writeConfig(context, override = {}) {
  const folder = path.join(context.dirs.config, 'opencode');
  mkdirSync(folder, { recursive: true });
  const config = {
    $schema: 'https://opencode.ai/config.json',
    autoupdate: false,
    share: 'disabled',
    model: 'stub/scripted',
    small_model: 'stub/scripted',
    provider: {
      stub: {
        npm: '@ai-sdk/openai-compatible',
        name: 'Scripted stub',
        options: { baseURL: `http://127.0.0.1:${context.stubPort}/v1` },
        models: { scripted: { name: 'Scripted', tool_call: true, limit: { context: 200000, output: 8192 } } },
      },
    },
    permission: { external_directory: 'allow' },
  };
  const merge = (a, b) => {
    for (const [key, value] of Object.entries(b)) {
      a[key] = value && typeof value === 'object' && !Array.isArray(value) && a[key] && typeof a[key] === 'object' ? merge(a[key], value) : value;
    }
    return a;
  };
  writeFileSync(path.join(folder, 'opencode.json'), JSON.stringify(merge(config, override), null, 2));
  // No command of the run may start or reach the background service.
  writeFileSync(path.join(folder, 'service.json'), '{"disabled": true}\n');
}

/**
 * Installs the plugins: wtpool's as `wtpool hook install --harness opencode --policy` printed it,
 * its binary replaced by `bin`, and the recorder beside it.
 */
export function installPlugins(context, bin) {
  const folder = path.join(context.dirs.config, 'opencode', 'plugins');
  mkdirSync(path.join(folder, 'wtpool'), { recursive: true });
  mkdirSync(path.join(folder, 'wtpool-verify-recorder'), { recursive: true });
  writeFileSync(path.join(folder, 'wtpool', 'index.ts'), withBinary(context.printed, context.wtpool, bin));
  copyFileSync(RECORDER, path.join(folder, 'wtpool-verify-recorder', 'index.ts'));
}

/** The printed plugin with its binary path `from` replaced by `to`, in its CONFIG line only. */
export function withBinary(printed, from, to) {
  const lines = printed.split('\n');
  const at = lines.findIndex((line) => /^\s*const\s+CONFIG\b/.test(line));
  if (at < 0) throw new Error('the printed plugin has no CONFIG line');
  const quoted = JSON.stringify(from);
  if (!lines[at].includes(quoted)) throw new Error(`the printed plugin's CONFIG does not name the build ${from}`);
  lines[at] = lines[at].replace(quoted, JSON.stringify(to));
  return lines.join('\n');
}

/** The owner's OpenCode folders, by the XDG defaults OpenCode falls back to. */
export function ownerFolders() {
  const home = os.homedir();
  return {
    config: path.join(home, '.config', 'opencode'),
    data: path.join(home, '.local', 'share', 'opencode'),
    state: path.join(home, '.local', 'state', 'opencode'),
    cache: path.join(home, '.cache', 'opencode'),
  };
}

/** Each entry below `dir` by name, size and modification time, at most `limit` of them. */
function listing(dir, limit = 5000) {
  const found = {};
  const stack = [dir];
  while (stack.length && Object.keys(found).length < limit) {
    const current = stack.pop();
    let names = [];
    try {
      names = readdirSync(current);
    } catch {
      continue;
    }
    for (const name of names) {
      const full = path.join(current, name);
      try {
        const stat = lstatSync(full);
        found[path.relative(dir, full)] = `${stat.size} ${stat.mtimeMs}`;
        if (stat.isDirectory()) stack.push(full);
      } catch {
        // Gone while listed.
      }
    }
  }
  return found;
}

/**
 * The owner's OpenCode as the run must leave it, read by names, sizes and times, never contents:
 * its config folder, its background service's pid and start, and its launchd entries.
 */
export function ownerSnapshot() {
  const folders = ownerFolders();
  let services = [];
  try {
    services = execFileSync('/bin/ps', ['-axo', 'pid=,lstart=,command='], { encoding: 'utf8' })
      .split('\n')
      .filter((line) => /opencode/.test(line) && /\bserve\b/.test(line) && /--service\b/.test(line))
      .map((line) => line.trim().replace(/\s+/g, ' '));
  } catch {
    // No process table: the comparison then fails on its own.
  }
  let launchd = [];
  try {
    launchd = execFileSync('/bin/launchctl', ['list'], { encoding: 'utf8' })
      .split('\n')
      .filter((line) => /opencode/i.test(line))
      .map((line) => line.trim());
  } catch {
    // No launchctl.
  }
  return { config: existsSync(folders.config) ? listing(folders.config) : null, services, launchd };
}

/** Lists the differences between two {@link ownerSnapshot}s, one phrase each. */
export function ownerChanges(before, after) {
  const changes = [];
  const configBefore = before.config ?? {};
  const configAfter = after.config ?? {};
  for (const name of [...new Set([...Object.keys(configBefore), ...Object.keys(configAfter)])].sort()) {
    if (configBefore[name] !== configAfter[name]) changes.push(`~/.config/opencode/${name} changed`);
  }
  if (JSON.stringify(before.services) !== JSON.stringify(after.services)) {
    changes.push(`the background service changed: ${JSON.stringify(before.services)} then ${JSON.stringify(after.services)}`);
  }
  if (JSON.stringify(before.launchd) !== JSON.stringify(after.launchd)) {
    changes.push(`launchd's OpenCode entries changed: ${JSON.stringify(before.launchd)} then ${JSON.stringify(after.launchd)}`);
  }
  return changes;
}

/** The sandbox's credentials: its auth.json, and the rows of its database's credential table. */
export function sandboxCredentials(dirs) {
  const found = [];
  const auth = path.join(dirs.data, 'opencode', 'auth.json');
  if (existsSync(auth)) found.push(`${auth} exists`);
  const db = path.join(dirs.data, 'opencode', 'opencode.db');
  if (existsSync(db)) {
    try {
      const count = execFileSync('/usr/bin/sqlite3', ['-readonly', db, 'select count(*) from credential;'], { encoding: 'utf8', timeout: 10_000 }).trim();
      if (count !== '0') found.push(`the sandbox database holds ${count} credentials`);
    } catch (error) {
      found.push(`the sandbox database's credential table could not be counted: ${String(error.message).split('\n')[0]}`);
    }
  }
  return found;
}
