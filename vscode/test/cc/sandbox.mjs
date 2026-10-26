// The sandbox of the cc layer. `claude` keeps the real home and the owner's login, but loads only
// the run's settings file and keeps its configuration in the run folder, so transcripts, project
// entries, file history and session environments never reach the owner's ~/.claude/ or
// ~/.claude.json.
import { execFileSync } from 'node:child_process';
import { existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, readlinkSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
export const RECORDER = path.join(here, 'recorder.mjs');
export const MODEL = 'claude-haiku-5-5';

/** The hook events wtpool hooks into. */
export const EVENTS = [
  'UserPromptSubmit',
  'PreToolUse',
  'PermissionRequest',
  'PostToolUse',
  'PostToolUseFailure',
  'PermissionDenied',
  'PostToolBatch',
  'Notification',
  'Stop',
  'StopFailure',
  'SubagentStart',
  'SubagentStop',
  'CwdChanged',
  'SessionEnd',
];
const ASYNC = new Set(['PreToolUse', 'PostToolUse', 'PostToolUseFailure', 'PermissionDenied', 'PostToolBatch']);

/**
 * Parses the block `wtpool hook install` printed, or gives undefined when it printed nothing.
 * Anything but one JSON object throws, as text after the block makes a malformed settings file,
 * and claude skips such a file without a word and so runs no hook.
 */
function wholeObject(text) {
  if (!text.trim()) return undefined;
  let value;
  try {
    value = JSON.parse(text);
  } catch (error) {
    throw new Error(`wtpool hook install stdout is not one JSON value (H22): ${error.message}: ${JSON.stringify(text.slice(0, 200))}`);
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`wtpool hook install stdout is not a JSON object (H22): ${JSON.stringify(text.slice(0, 200))}`);
  }
  return value;
}

/**
 * The run's cc folders: {root, logs, state, data, cache, config}, `config` being claude's
 * configuration folder and the three before it the sessions' XDG folders.
 */
export function makeRunDirs(runDir) {
  const root = path.join(runDir, 'cc');
  const dirs = {
    root,
    logs: path.join(root, 'logs'),
    state: path.join(root, 'state'),
    data: path.join(root, 'data'),
    cache: path.join(root, 'cache'),
    config: path.join(root, 'claude-config'),
    // wtpool's machine config folder: the run's, never the owner's ~/.config.
    xdgConfig: path.join(root, 'xdg-config'),
  };
  for (const dir of Object.values(dirs)) mkdirSync(dir, { recursive: true });
  return dirs;
}

/** A fresh pool for one session in the run folder: {scenario, dir, fg, common}. */
export function makePool(dirs, name) {
  const text = execFileSync(process.env.WTPOOL_VERIFY_XTASK, ['pool', '--scenario', 'agent_working_in_slot', '--dir', path.join(dirs.root, 'pools', name)], {
    encoding: 'utf8',
    timeout: 120_000,
  });
  return JSON.parse(text);
}

/**
 * The runner's variables that reach `claude`, and so its hooks and commands: its programs, home,
 * temporary folder, user, shell and locale, a login by token, git's settings of
 * `.cargo/config.toml`, and the harness's own. No other does, such as a Claude Code session's
 * messaging socket, VS Code's IPC handles, an SSH agent, or a GitHub or cloud token. HOME stays
 * the real one, as macOS finds the login keychain with the owner's login through it.
 */
const KEEP = new Set([
  'PATH',
  'HOME',
  'TMPDIR',
  'USER',
  'LOGNAME',
  'SHELL',
  'LANG',
  'LANGUAGE',
  'ANTHROPIC_API_KEY',
  'ANTHROPIC_AUTH_TOKEN',
  'CLAUDE_CODE_OAUTH_TOKEN',
  'GIT_CONFIG_GLOBAL',
  'GIT_CONFIG_NOSYSTEM',
]);
const KEEP_PREFIX = /^(LC_|WTPOOL_)/;

/**
 * The environment of `claude` and so of its hooks: the variables {@link KEEP} names, the run's
 * configuration folder with the keychain entry of the owner's login, the sandbox pool for wtpool.
 */
export function sessionEnv(dirs, pool, extra = {}) {
  const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => KEEP.has(name) || KEEP_PREFIX.test(name)));
  delete env.WTPOOL_BIN;
  mkdirSync(dirs.config, { recursive: true });
  return {
    ...env,
    CLAUDE_CONFIG_DIR: dirs.config,
    // Empty, not absent, so claude reads the keychain entry of the default configuration, the
    // owner's login, rather than an empty one named after CLAUDE_CONFIG_DIR.
    CLAUDE_SECURESTORAGE_CONFIG_DIR: '',
    // The run's configuration lacks the owner's `autoUpdates: false`, and an update replaces the
    // machine's shared `claude` outside the run folder.
    DISABLE_AUTOUPDATER: '1',
    // The claude.ai connectors of the owner's account (Drive, Gmail and the like) stay out.
    ENABLE_CLAUDEAI_MCP_SERVERS: 'false',
    // Claude Code keeps its installed versions in the data folder, their locks in the state
    // folder and its update staging in the cache. Its start-up cleanup prunes the versions it
    // finds unlocked, so with only the state folder here it would prune the owner's versions.
    XDG_STATE_HOME: dirs.state,
    XDG_DATA_HOME: dirs.data,
    XDG_CACHE_HOME: dirs.cache,
    XDG_CONFIG_HOME: dirs.xdgConfig,
    WTPOOL_FOREGROUND: pool.fg,
    WTPOOL_SLOT_DIR: pool.dir,
    WTPOOL_VERIFY_SANDBOX: '1',
    TZ: 'UTC',
    ...extra,
  };
}

/** The env of `wtpool` runs of the runner itself: a sandbox home besides the pool. */
export function wtpoolEnv(dirs, pool) {
  const home = path.join(dirs.root, 'wtpool-home');
  mkdirSync(home, { recursive: true });
  return {
    PATH: process.env.PATH,
    HOME: home,
    TMPDIR: os.tmpdir(),
    XDG_STATE_HOME: dirs.state,
    XDG_CONFIG_HOME: dirs.xdgConfig,
    WTPOOL_FOREGROUND: pool.fg,
    WTPOOL_SLOT_DIR: pool.dir,
    WTPOOL_VERIFY_SANDBOX: '1',
    TZ: 'UTC',
    GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_CONFIG_NOSYSTEM: '1',
  };
}

/** The hook block for a `wtpool` whose `hook install` prints none yet. */
function fallbackHooks(wtpool) {
  const hooks = {};
  for (const event of EVENTS) {
    const entry = { type: 'command', command: wtpool, args: ['hook'], onFailure: 'continue' };
    if (ASYNC.has(event)) entry.async = true;
    else if (event !== 'SessionEnd') entry.timeout = 2;
    hooks[event] = [{ hooks: [entry] }];
  }
  return hooks;
}

/**
 * Writes the run's settings file: the hook block of the build copy (from `wtpool hook install`,
 * or {@link fallbackHooks} while the build prints none), a recorder hook per event,
 * WTPOOL_VERIFY_SANDBOX=1, defaultMode `default`, the ask rule `Bash(git status)`, no WebFetch or
 * WebSearch, and Claude Code's sandbox on. Returns {file, printed}, `printed` telling if the block
 * came from `wtpool hook install`.
 */
export function writeSettings(dirs, wtpool, pool) {
  let text = '';
  try {
    text = execFileSync(wtpool, ['hook', 'install'], { encoding: 'utf8', env: wtpoolEnv(dirs, pool), timeout: 30_000, stdio: ['ignore', 'pipe', 'pipe'] });
  } catch {
    // A build that cannot run `hook install` yet gets the fallback block.
  }
  const block = wholeObject(text);
  const printed = Boolean(block);
  const hooks = structuredClone(block?.hooks ?? block ?? fallbackHooks(wtpool));
  for (const event of EVENTS) {
    hooks[event] ??= [];
    hooks[event].push({ hooks: [{ type: 'command', command: `"${process.execPath}" "${RECORDER}"`, timeout: 10 }] });
  }
  const settings = {
    hooks,
    env: { WTPOOL_VERIFY_SANDBOX: '1' },
    // No web tools: Claude Code fetches from some hosts without asking, past canUseTool.
    permissions: { defaultMode: 'default', ask: ['Bash(git status)'], deny: ['WebFetch', 'WebSearch'] },
    // Bash writes only in the session's working directories, all in the run folder, and Claude
    // Code's temporary folder, and reaches no host but localhost. Each command still asks as it
    // would unsandboxed.
    sandbox: {
      enabled: true,
      failIfUnavailable: true,
      autoAllowBashIfSandboxed: false,
      allowUnsandboxedCommands: false,
      network: { strictAllowlist: true },
    },
  };
  const file = path.join(dirs.root, 'settings.json');
  writeFileSync(file, JSON.stringify(settings, null, 2));
  return { file, printed };
}

/** Plants project and local settings in fg with a hook that writes a canary file. */
export function plantCanaries(dirs, pool) {
  const canaries = [];
  for (const [file, name] of [
    [path.join(pool.fg, '.claude', 'settings.json'), 'project'],
    [path.join(pool.fg, '.claude', 'settings.local.json'), 'local'],
  ]) {
    const canary = path.join(dirs.logs, `canary-${name}.txt`);
    mkdirSync(path.dirname(file), { recursive: true });
    const hook = { hooks: [{ type: 'command', command: `/usr/bin/touch "${canary}"` }] };
    writeFileSync(file, JSON.stringify({ hooks: { UserPromptSubmit: [hook], Stop: [hook] } }, null, 2));
    canaries.push(canary);
  }
  return canaries;
}

/** `~/.claude/projects/`, where claude keeps transcripts by default. */
function projectsDir() {
  return path.join(os.homedir(), '.claude', 'projects');
}

/**
 * Claude Code's temporary folders, `claude-<uid>` in /tmp and in `CLAUDE_CODE_TMPDIR`, where it
 * keeps a folder per working directory: cwd files and task outputs.
 */
function tempDirs() {
  const name = `claude-${process.getuid?.() ?? 0}`;
  return [...new Set(['/tmp', process.env.CLAUDE_CODE_TMPDIR].filter(Boolean).map((dir) => path.join(dir, name)))];
}

/** The name prefix claude gives the folders of a session with its cwd below `runDir`. */
function runMarker(runDir) {
  return path.resolve(runDir).replace(/[^A-Za-z0-9]/g, '-');
}

/** The run's folders in Claude Code's temporary folders now. */
export function tempFolders(runDir) {
  const marker = runMarker(runDir);
  return tempDirs().flatMap((dir) => {
    try {
      return readdirSync(dir).filter((name) => name.startsWith(marker)).map((name) => path.join(dir, name));
    } catch {
      return [];
    }
  });
}

export function removeTempFolders(runDir) {
  const found = tempFolders(runDir);
  for (const folder of found) rmSync(folder, { recursive: true, force: true });
  return found;
}

/**
 * Lists the traces of the session `id`, run with cwd below `runDir`, in the owner's Claude Code
 * configuration: ~/.claude/ entries named after it or after the run folder, and project entries
 * of ~/.claude.json below the run folder. Reads names and keys only, never values.
 */
export function ownerTraces(id, runDir) {
  const home = path.join(os.homedir(), '.claude');
  const marker = runMarker(runDir);
  const found = [];
  for (const sub of ['projects', 'session-env', 'file-history', 'todos', 'tasks', 'debug']) {
    let names = [];
    try {
      names = readdirSync(path.join(home, sub));
    } catch {
      continue;
    }
    for (const name of names) if (name.includes(id) || name.startsWith(marker)) found.push(`~/.claude/${sub}/${name}`);
  }
  try {
    const projects = JSON.parse(readFileSync(path.join(os.homedir(), '.claude.json'), 'utf8')).projects ?? {};
    const run = path.resolve(runDir);
    for (const key of Object.keys(projects)) if (key.startsWith(run)) found.push(`~/.claude.json projects["${key}"]`);
  } catch {
    // No ~/.claude.json, or one being written: its entries count as none.
  }
  return found;
}

/**
 * The owner's Claude Code install, by names and times alone, never contents: each entry of its
 * versions folder, found as Claude Code finds it, with its modification time, and the target of
 * `~/.local/bin/claude`.
 */
export function installSnapshot() {
  const data = process.env.XDG_DATA_HOME ?? path.join(os.homedir(), '.local', 'share');
  const versions = {};
  try {
    for (const name of readdirSync(path.join(data, 'claude', 'versions'))) {
      versions[name] = lstatSync(path.join(data, 'claude', 'versions', name)).mtimeMs;
    }
  } catch {
    // No native install: nothing a run could change.
  }
  let launcher = null;
  try {
    launcher = readlinkSync(path.join(os.homedir(), '.local', 'bin', 'claude'));
  } catch {
    // No launcher link.
  }
  return { versions, launcher };
}

/** Lists the differences between two {@link installSnapshot}s, one phrase each. */
export function installChanges(before, after) {
  const changes = [];
  for (const name of [...new Set([...Object.keys(before.versions), ...Object.keys(after.versions)])].sort()) {
    if (!Object.hasOwn(after.versions, name)) changes.push(`versions/${name} removed`);
    else if (!Object.hasOwn(before.versions, name)) changes.push(`versions/${name} added`);
    else if (before.versions[name] !== after.versions[name]) changes.push(`versions/${name} modified`);
  }
  if (before.launcher !== after.launcher) changes.push(`~/.local/bin/claude now points to ${after.launcher}`);
  return changes;
}

/** The names of the transcript folders in {@link projectsDir} now. */
export function transcriptFolders() {
  try {
    return new Set(readdirSync(projectsDir()));
  } catch {
    return new Set();
  }
}

/**
 * Deletes the transcript folders this run's sessions created: new since `before` and named
 * after a folder below the run folder, as claude names them from the session's cwd.
 */
export function removeOwnTranscripts(before, runDir) {
  const marker = runMarker(runDir);
  const removed = [];
  for (const name of transcriptFolders()) {
    if (before.has(name) || !name.startsWith(marker)) continue;
    const folder = path.join(projectsDir(), name);
    if (existsSync(folder)) {
      rmSync(folder, { recursive: true, force: true });
      removed.push(name);
    }
  }
  return removed;
}
