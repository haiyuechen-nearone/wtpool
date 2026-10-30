// Loads the OpenCode plugin of crates/wtpool-opencode as Node runs it, its types stripped, with a
// test CONFIG line, and replays the recorded OpenCode 2.2.0 streams through it.
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { FakeBun, FakeClock, FakeContext, flush } from './fakes.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
export const ROOT = path.resolve(here, '../../..');
export const PLUGIN_REL = 'crates/wtpool-opencode/plugin/wtpool.ts';
export const PLUGIN = path.join(ROOT, PLUGIN_REL);
export const FIXTURES = path.join(ROOT, 'verify/fixtures/opencode-2.2.0');
/** The binary the test CONFIG names: no such file exists, and the fake `Bun` runs nothing. */
export const BIN = '/wtpool-verify/bin/wtpool';
export const RECORD_ARGS = ['hook', '--harness', 'opencode'];
export const POLICY_ARGS = ['hook', 'policy', '--harness', 'opencode'];

export const bun = new FakeBun();
globalThis.Bun = bun;

/** The CONFIG line: `<declaration> = <JSON>;` on one line. */
const CONFIG_LINE = /^(\s*const\s+CONFIG\b[^=]*=\s*)(.*);\s*$/;

/** The plugin's source, or an assertion failure when the crate ships none. */
export function pluginSource() {
  if (!existsSync(PLUGIN)) assert.fail(`the plugin ${PLUGIN_REL} does not exist`);
  return readFileSync(PLUGIN, 'utf8');
}

/** The lines that declare CONFIG. */
export function configLines(source) {
  return source.split('\n').filter((line) => /^\s*(export\s+)?(const|let|var)\s+CONFIG\b/.test(line));
}

/** The committed CONFIG: its line and the JSON value it holds. */
export function committedConfig(source = pluginSource()) {
  const lines = configLines(source);
  assert.equal(lines.length, 1, `the plugin declares CONFIG on ${lines.length} lines, not exactly one`);
  const match = CONFIG_LINE.exec(lines[0]);
  assert.ok(match, `the CONFIG line is no \`const CONFIG... = <value>;\` line: ${lines[0].slice(0, 200)}`);
  let value;
  try {
    value = JSON.parse(match[2]);
  } catch (error) {
    assert.fail(`the CONFIG line holds no JSON: ${error.message}`);
  }
  assert.ok(value && typeof value === 'object' && !Array.isArray(value), 'the CONFIG line holds no JSON object');
  return { line: lines[0], declaration: match[1], value };
}

let folder;
let loads = 0;

function scratch() {
  if (!folder) {
    const base = process.env.WTPOOL_VERIFY_RUN_DIR ? path.join(process.env.WTPOOL_VERIFY_RUN_DIR, 'js') : os.tmpdir();
    mkdirSync(base, { recursive: true });
    folder = mkdtempSync(path.join(base, 'plugin-'));
    process.on('exit', () => rmSync(folder, { recursive: true, force: true }));
  }
  return folder;
}

/**
 * Imports a fresh evaluation of the plugin, its CONFIG line replaced by the committed value with
 * `bin` and `policy` set, as `wtpool hook install` would print it. Each call is a module of its own,
 * as OpenCode evaluates the plugin once per folder.
 */
export async function loadPlugin({ policy = false, bin = BIN, limits } = {}) {
  const source = pluginSource();
  const { line, declaration, value } = committedConfig(source);
  const config = { ...value, bin, policy };
  if (limits) config.limits = { ...value.limits, ...limits };
  const text = source.replace(line, `${declaration}${JSON.stringify(config)};`);
  loads += 1;
  const file = path.join(scratch(), `wtpool-${loads}.mts`);
  writeFileSync(file, text);
  const module = await import(`${pathToFileURL(file).href}?instance=${loads}`);
  return module.default;
}

/** `value` as the plugin gets it: the recorder's `"<undefined>"` fields and `$` markers dropped. */
export function clean(value) {
  if (Array.isArray(value)) return value.filter((item) => item !== '<undefined>').map(clean);
  if (!value || typeof value !== 'object') return value;
  const out = {};
  for (const [key, item] of Object.entries(value)) {
    if (key.startsWith('$') || item === '<undefined>') continue;
    out[key] = clean(item);
  }
  return out;
}

function readLines(file) {
  return readFileSync(file, 'utf8')
    .split('\n')
    .filter((line) => line.trim())
    .map((line) => JSON.parse(line));
}

/** The names of the recorded streams, `NN-<name>`, or a setup failure without the fixtures. */
export function streams() {
  const dir = path.join(FIXTURES, 'raw');
  if (!existsSync(dir)) throw new Error(`setup: the recordings ${path.relative(ROOT, dir)} do not exist`);
  return readdirSync(dir)
    .filter((name) => name.endsWith('.hooks.jsonl'))
    .map((name) => name.slice(0, -'.hooks.jsonl'.length))
    .sort();
}

export function rawLines(name) {
  const file = path.join(FIXTURES, 'raw', `${name}.hooks.jsonl`);
  if (!existsSync(file)) throw new Error(`setup: the recording ${path.relative(ROOT, file)} does not exist`);
  return readLines(file);
}

/** What `ctx.session.get` answers in stream `name`, by session id, from `lookups.json`. */
export function lookups(name) {
  const file = path.join(FIXTURES, 'lookups.json');
  if (!existsSync(file)) return {};
  return JSON.parse(readFileSync(file, 'utf8'))[name] ?? {};
}

/** The `forwarded/` fixture of a stream: `{line, t, payload}` per spawn. */
export function forwarded(name) {
  const file = path.join(FIXTURES, 'forwarded', `${name}.jsonl`);
  if (!existsSync(file)) throw new Error(`setup: the frozen fixture ${path.relative(ROOT, file)} does not exist`);
  return readLines(file);
}

let folders;

/** Each recorded instance's folder: its `setup` line's, else `instances.json`'s. */
export function instanceFolders() {
  if (folders) return folders;
  folders = new Map();
  const map = path.join(FIXTURES, 'instances.json');
  if (existsSync(map)) for (const [id, dir] of Object.entries(JSON.parse(readFileSync(map, 'utf8')))) folders.set(id, dir);
  for (const name of streams()) {
    for (const line of rawLines(name)) {
      if (line.kind === 'setup' && line.name === 'context' && line.payload?.location?.directory) {
        folders.set(line.instance, line.payload.location.directory);
      }
    }
  }
  return folders;
}

/**
 * One OpenCode server of the replay: an instance per folder, each a fresh plugin evaluation.
 * `sessions` is what `ctx.session.get` answers, by session id.
 */
export class Server {
  constructor({ policy = false, clock, sessions = {} } = {}) {
    this.policy = policy;
    this.clock = clock;
    this.sessions = sessions;
    this.instances = new Map();
  }

  /** Starts an instance of the plugin for `directory`: its context and the cleanup setup gave. */
  async start(key, directory, options = {}) {
    const plugin = await loadPlugin({ policy: this.policy, ...options });
    const context = new FakeContext(directory, { sessions: this.sessions, ...options });
    const cleanup = await plugin.setup(context.ctx);
    await flush();
    const instance = { key, directory, context, cleanup, plugin };
    this.instances.set(key, instance);
    return instance;
  }

  async instance(key) {
    const known = this.instances.get(key);
    if (known) return known;
    const directory = instanceFolders().get(key);
    if (!directory) throw new Error(`setup: the folder of recorded instance ${key} is unknown`);
    return this.start(key, directory);
  }

  end() {
    for (const instance of this.instances.values()) instance.context.end();
  }
}

/** The record spawns since `from`: `wtpool hook --harness opencode` runs, with their payloads. */
export function recordSpawns(from = 0) {
  return bun.spawns.slice(from).filter((spawn) => sameArgs(spawn.argv.slice(1), RECORD_ARGS));
}

export function policySpawns(from = 0) {
  return bun.spawns.slice(from).filter((spawn) => sameArgs(spawn.argv.slice(1), POLICY_ARGS));
}

function sameArgs(a, b) {
  return a.length === b.length && a.every((item, i) => item === b[i]);
}

/** A spawn's stdin as the payload it holds. */
export function payloadOf(spawn) {
  assert.equal(typeof spawn.stdin, 'string', `a record spawn got no stdin text: ${JSON.stringify(spawn.options?.stdin)?.slice(0, 200)}`);
  try {
    return JSON.parse(spawn.stdin);
  } catch (error) {
    assert.fail(`a record spawn's stdin is no JSON (${error.message}): ${spawn.stdin.slice(0, 200)}`);
  }
}

/**
 * Replays the recorded stream `name`: each line to its instance at its own time, hooks awaited
 * as OpenCode awaits them. Gives every record spawn as `{line, t, instance, payload}`, `line`
 * being the 1-based raw line during which it happened, and each hook that threw.
 */
export async function replay(name, { policy = false } = {}) {
  const lines = rawLines(name);
  const clock = new FakeClock(lines[0]?.t);
  clock.install();
  bun.reset();
  bun.clock = clock;
  const server = new Server({ policy, clock, sessions: lookups(name) });
  const spawns = [];
  const thrown = [];
  try {
    for (const [index, line] of lines.entries()) {
      await clock.advanceTo(line.t);
      const from = bun.spawns.length;
      const instance = await server.instance(line.instance);
      if (line.kind === 'event') {
        await instance.context.feed(clean(line.payload));
      } else if (line.kind === 'hook') {
        try {
          await instance.context.call(line.name, clean(line.payload));
        } catch (error) {
          thrown.push({ line: index + 1, name: line.name, message: error?.message });
        }
        await flush();
      } else if (line.kind === 'cleanup') {
        await instance.cleanup?.();
        await flush();
      }
      for (const spawn of recordSpawns(from)) spawns.push({ line: index + 1, t: line.t, instance: line.instance, spawn });
    }
    await bun.settle();
  } finally {
    server.end();
    clock.restore();
    bun.clock = null;
  }
  return { lines, thrown, spawns: spawns.map((s) => ({ line: s.line, t: s.t, instance: s.instance, payload: payloadOf(s.spawn) })) };
}

/** The kept fields of each forwarded name: the union of its rows in the event table. */
export const KEPT = {
  'session.created': ['sessionID', 'title', 'parentID', 'agent'],
  'session.execution.started': ['sessionID'],
  'session.execution.succeeded': ['sessionID', 'reason'],
  'session.execution.interrupted': ['sessionID', 'reason'],
  'session.execution.failed': ['sessionID', 'error.type'],
  'tool.execute.before': ['sessionID', 'id', 'tool', 'input.path', 'input.command', 'input.agent'],
  'session.tool.success': ['sessionID', 'id'],
  'session.tool.failed': ['sessionID', 'id'],
  'permission.asked': ['sessionID', 'id', 'source.id'],
  'permission.replied': ['sessionID', 'requestID', 'reply'],
  'form.created': ['form.id', 'form.sessionID', 'form.metadata.kind', 'form.metadata.tool.id'],
  'form.replied': ['id', 'sessionID'],
  'session.renamed': ['sessionID', 'title'],
  'session.step.started': ['sessionID', 'model.id', 'model.providerID'],
  'session.text.delta': ['sessionID'],
  'session.reasoning.delta': ['sessionID'],
  'session.tool.input.delta': ['sessionID'],
  'session.deleted': ['sessionID'],
  'session.get': ['id', 'parentID', 'title'],
};
/** The payload kind of each forwarded name that is no event. */
export const KINDS = { 'tool.execute.before': 'hook', 'session.get': 'lookup' };
export const DELTAS = ['session.text.delta', 'session.reasoning.delta', 'session.tool.input.delta'];
export const PAYLOAD_KEYS = ['v', 'kind', 'name', 'directory', 'parent', 'call', 'data'];

/** The dotted paths of an object's leaves. */
export function leaves(value, prefix = '') {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return prefix ? [prefix] : [];
  const out = [];
  for (const [key, item] of Object.entries(value)) {
    const at = prefix ? `${prefix}.${key}` : key;
    if (item && typeof item === 'object' && !Array.isArray(item)) out.push(...leaves(item, at));
    else out.push(at);
  }
  return out;
}

let eventIds = 0;

/** An event envelope as 2.2.0 sends it, located in `directory` unless it is undefined. */
export function event(type, data, directory) {
  eventIds += 1;
  const envelope = { id: `evt_1892${String(eventIds).padStart(22, '0')}`, created: Date.now(), type, data };
  if (directory !== undefined) envelope.location = { directory };
  return envelope;
}

/** A `tool.execute.before` input. */
export function toolCall(sessionID, id, tool, input = {}) {
  return { tool, sessionID, agent: 'build', messageID: 'msg_189208632001XhxMZnlknvxaud', id, input };
}

/** A `shell.create.before` input with `env` as the server's environment. */
export function shellCreate(command, cwd, env = {}) {
  return { command, cwd, timeout: 120_000, shell: '/bin/zsh', env: { PATH: '/usr/bin:/bin', HOME: '/sandbox/home', ...env } };
}

/** A session id of OpenCode's own form, `ses_` and 26 letters and digits. */
export function sessionId(n) {
  return `ses_e76d${String(n).padStart(8, '0')}AbCdEfGhIjKlMn`;
}

/** Collects unhandled rejections and uncaught exceptions while `fn` runs. */
export async function withoutStrayErrors(fn) {
  const stray = [];
  const onRejection = (reason) => stray.push(`unhandled rejection: ${reason?.stack ?? reason}`);
  const onException = (error) => stray.push(`uncaught exception: ${error?.stack ?? error}`);
  process.on('unhandledRejection', onRejection);
  process.on('uncaughtException', onException);
  try {
    await fn();
    await flush(5);
  } finally {
    process.off('unhandledRejection', onRejection);
    process.off('uncaughtException', onException);
  }
  return stray;
}
