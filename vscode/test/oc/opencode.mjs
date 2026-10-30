// One session of the oc layer: its pool, its OpenCode commands in the sandbox, the recorder's
// lines, the wrapper's calls, and wtpool's records and documents as it runs.
import { execFile, execFileSync, spawn } from 'node:child_process';
import { cpSync, createWriteStream, existsSync, mkdirSync, readFileSync, readdirSync } from 'node:fs';
import net from 'node:net';
import path from 'node:path';

import { sleep } from '../support/processes.mjs';
import { installPlugins, makePool, openCodeEnv, readCalls, writeConfig, wtpoolEnv } from './sandbox.mjs';

export class Inconclusive extends Error {}
export class AssumptionBroken extends Error {}

/** An OpenCode assert: its failure is assumption_broken. */
export function expectOpenCode(condition, message) {
  if (!condition) throw new AssumptionBroken(message);
}

function readLines(file) {
  if (!existsSync(file)) return [];
  return readFileSync(file, 'utf8')
    .split('\n')
    .filter(Boolean)
    .flatMap((line) => {
      try {
        return [JSON.parse(line)];
      } catch {
        return [];
      }
    });
}

/** A free loopback port. */
export function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

/** A process the session started: its exit, and a stop that ends it and every child. */
class Started {
  constructor(child, limitMs) {
    this.child = child;
    this.exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
    this.timer = setTimeout(() => this.stop(), limitMs);
    this.exited.then(() => clearTimeout(this.timer));
  }

  async stop() {
    if (this.child.exitCode !== null || this.child.signalCode !== null) return this.exited;
    try {
      process.kill(this.child.pid, 'SIGTERM');
    } catch {
      // Gone already.
    }
    const ended = await Promise.race([this.exited, sleep(5000).then(() => null)]);
    if (!ended) {
      try {
        process.kill(this.child.pid, 'SIGKILL');
      } catch {
        // Gone already.
      }
    }
    return this.exited;
  }
}

export class Session {
  constructor(context, name, { plugin = context.wrapper } = {}) {
    this.context = context;
    this.name = name;
    this.pool = makePool(context, `${name}-${Date.now()}`);
    this.dir = path.join(context.dirs.logs, name);
    mkdirSync(this.dir, { recursive: true });
    this.recording = path.join(this.dir, 'recorder.jsonl');
    this.calls = path.join(this.dir, 'calls');
    mkdirSync(this.calls, { recursive: true });
    this.started = [];
    this.samples = [];
    context.open.push(this);
    installPlugins(context, plugin);
  }

  wt(name) {
    return path.join(this.pool.dir, name);
  }

  env(extra = {}) {
    return openCodeEnv(this.context, this.pool, {
      WTPOOL_VERIFY_OC_RECORDING: this.recording,
      WTPOOL_VERIFY_OC_CALLS: this.calls,
      ...extra,
    });
  }

  config(override = {}) {
    writeConfig(this.context, override);
  }

  /** Starts `opencode <args>` in `cwd` with an empty stdin, its output in the session's folder. */
  spawnOpenCode(args, { cwd = this.pool.fg, env = this.env(), label = 'opencode', limitMs = 240_000 } = {}) {
    const child = spawn(this.context.opencode, args, { cwd, env, stdio: ['ignore', 'pipe', 'pipe'] });
    child.stdout.pipe(createWriteStream(path.join(this.dir, `${label}.stdout`)));
    child.stderr.pipe(createWriteStream(path.join(this.dir, `${label}.stderr`)));
    const started = new Started(child, limitMs);
    this.started.push(started);
    return started;
  }

  /** `opencode run --standalone --format json <prompt>` in fg, by default, to its end. */
  async run(prompt, { flags = [], cwd, limitMs = 240_000, label = 'run' } = {}) {
    const started = this.spawnOpenCode(['run', '--standalone', '--format', 'json', ...flags, prompt], { cwd, limitMs, label });
    const ended = await started.exited;
    return { ...ended, stream: readLines(path.join(this.dir, `${label}.stdout`)) };
  }

  /** An `opencode serve` on loopback with a password, ready for {@link Session.api}. */
  async serve({ cwd = this.pool.fg, label = 'serve' } = {}) {
    const port = await freePort();
    this.password = `pw-${Math.random().toString(36).slice(2)}${Math.random().toString(36).slice(2)}`;
    this.base = `http://127.0.0.1:${port}`;
    this.server = this.spawnOpenCode(['serve', '--hostname', '127.0.0.1', '--port', String(port)], {
      cwd,
      env: this.env({ OPENCODE_SERVER_PASSWORD: this.password }),
      label,
      limitMs: 600_000,
    });
    for (let i = 0; i < 150; i += 1) {
      try {
        const info = await this.api('GET', '/api/info');
        if (info.status === 200) {
          this.serverPid = info.body?.pid ?? info.body?.data?.pid ?? this.server.child.pid;
          this.events = this.subscribe();
          await this.waitEvent((e) => e.type === 'server.connected', 10_000);
          return;
        }
      } catch {
        // Not listening yet.
      }
      await sleep(200);
    }
    throw new Error(`setup: opencode serve did not answer on ${this.base}`);
  }

  async api(method, route, body) {
    const response = await fetch(this.base + route, {
      method,
      headers: { authorization: `Basic ${Buffer.from(`opencode:${this.password}`).toString('base64')}`, 'content-type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(30_000),
    });
    const text = await response.text();
    let parsed = text;
    try {
      parsed = text ? JSON.parse(text) : null;
    } catch {
      // Not JSON.
    }
    return { status: response.status, body: parsed };
  }

  /** Reads the server's event stream into `items`, for {@link Session.waitEvent}. */
  subscribe() {
    const events = { items: [], waiters: [] };
    const controller = new AbortController();
    events.close = () => controller.abort();
    (async () => {
      try {
        const response = await fetch(`${this.base}/api/event`, {
          headers: { authorization: `Basic ${Buffer.from(`opencode:${this.password}`).toString('base64')}`, accept: 'text/event-stream' },
          signal: controller.signal,
        });
        const decoder = new TextDecoder();
        let rest = '';
        for await (const chunk of response.body) {
          rest += decoder.decode(chunk, { stream: true });
          let at;
          while ((at = rest.indexOf('\n')) >= 0) {
            const line = rest.slice(0, at).trimEnd();
            rest = rest.slice(at + 1);
            if (!line.startsWith('data: ')) continue;
            try {
              events.items.push({ ...JSON.parse(line.slice(6)), receivedAt: Date.now() });
              for (const wake of events.waiters.splice(0)) wake();
            } catch {
              // Not an event.
            }
          }
        }
      } catch {
        // The server ended or the stream closed.
      }
    })();
    return events;
  }

  /** The first event from `from` on that `test` passes, or null after `limitMs`. */
  async waitEvent(test, limitMs = 60_000, from = 0) {
    const deadline = Date.now() + limitMs;
    let index = from;
    for (;;) {
      for (; index < this.events.items.length; index += 1) if (test(this.events.items[index])) return this.events.items[index];
      const left = deadline - Date.now();
      if (left <= 0) return null;
      await Promise.race([new Promise((resolve) => this.events.waiters.push(resolve)), sleep(Math.min(left, 500))]);
    }
  }

  mark() {
    return this.events?.items.length ?? 0;
  }

  async create(directory, title) {
    const body = { location: { directory } };
    if (title) body.title = title;
    const created = await this.api('POST', '/api/session', body);
    const id = created.body?.data?.id ?? created.body?.id;
    if (!id) throw new Error(`setup: the session was not created: ${created.status} ${JSON.stringify(created.body).slice(0, 300)}`);
    return id;
  }

  prompt(id, text) {
    return this.api('POST', `/api/session/${id}/prompt`, { text });
  }

  /** Waits for the end of a turn of session `id` after event `from`. */
  turnEnd(id, from = 0, limitMs = 120_000) {
    return this.waitEvent((e) => /^session\.execution\.(succeeded|failed|interrupted)$/.test(e.type) && e.data?.sessionID === id, limitMs, from);
  }

  recorded() {
    return readLines(this.recording);
  }

  /** The recorder's events of `type`, once per event id. */
  recordedEvents(type) {
    const seen = new Set();
    return this.recorded().filter((line) => {
      if (line.kind !== 'event' || (type && line.name !== type)) return false;
      const id = line.payload?.id ?? `${line.instance}:${line.t}`;
      if (seen.has(id)) return false;
      seen.add(id);
      return true;
    });
  }

  wrapperCalls() {
    return readCalls(this.calls);
  }

  /** The record spawns the wrapper kept: `wtpool hook --harness opencode` runs with payloads. */
  recordCalls() {
    return this.wrapperCalls().filter((call) => call.argv.join(' ') === 'hook --harness opencode');
  }

  status() {
    const text = execFileSync(this.context.wtpool, ['status', '--json'], {
      cwd: this.pool.fg,
      encoding: 'utf8',
      env: wtpoolEnv(this.context, this.pool),
      timeout: 30_000,
    });
    return JSON.parse(text);
  }

  /** Every actor of the document, each with its worktree's name, or none when wtpool fails. */
  actors() {
    try {
      return this.status().pools[0].worktrees.flatMap((w) => w.actors.map((a) => ({ ...a, worktree: w.name })));
    } catch {
      return [];
    }
  }

  /** Records `actors()` every `everyMs` until the returned stop is called. */
  sampleActors(everyMs = 500) {
    let stopped = false;
    const loop = (async () => {
      while (!stopped && !this.closed) {
        const actors = await new Promise((resolve) => {
          execFile(this.context.wtpool, ['status', '--json'], { cwd: this.pool.fg, encoding: 'utf8', env: wtpoolEnv(this.context, this.pool), timeout: 30_000 }, (error, stdout) => {
            try {
              resolve(JSON.parse(stdout).pools[0].worktrees.flatMap((w) => w.actors.map((a) => ({ ...a, worktree: w.name }))));
            } catch {
              resolve(null);
            }
          });
        });
        if (actors) this.samples.push({ at: Date.now(), actors });
        await sleep(everyMs);
      }
    })();
    return async () => {
      stopped = true;
      await loop;
    };
  }

  /** The OpenCode records of the pool: `opencode-*.json` of its wtpool-activity folder. */
  records() {
    const dir = path.join(this.pool.common, 'wtpool-activity');
    try {
      return readdirSync(dir)
        .filter((name) => name.startsWith('opencode-') && name.endsWith('.json'))
        .map((name) => ({ file: name, ...JSON.parse(readFileSync(path.join(dir, name), 'utf8')) }));
    } catch {
      return [];
    }
  }

  async end() {
    if (this.closed) return;
    this.closed = true;
    this.events?.close();
    for (const started of this.started.splice(0)) await started.stop();
    const activity = path.join(this.pool.common, 'wtpool-activity');
    if (existsSync(activity)) cpSync(activity, path.join(this.dir, 'wtpool-activity'), { recursive: true });
  }
}

/** The OpenCode actors among `actors`. */
export const openCodeActors = (actors) => actors.filter((a) => a.harness === 'opencode');

/** The stem a session id of OpenCode's own form takes. */
export const stemOf = (id) => `opencode-${id.replace('_', '-')}`;
