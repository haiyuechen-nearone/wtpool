// The fakes the plugin runs against in Node: a clock, `Bun` and an OpenCode plugin context.
// Node gives the plugin no `Bun`, so one fake stands in for the whole test process and each test
// resets it; a module keeps the `Bun` it first saw.

const real = {
  setTimeout: globalThis.setTimeout,
  clearTimeout: globalThis.clearTimeout,
  setInterval: globalThis.setInterval,
  clearInterval: globalThis.clearInterval,
  setImmediate: globalThis.setImmediate,
  dateNow: Date.now,
  performanceNow: performance.now.bind(performance),
};

/** Resolves once the tasks queued so far, and the promises they settle, have run. */
export async function flush(rounds = 3) {
  for (let i = 0; i < rounds; i += 1) await new Promise((resolve) => real.setImmediate(resolve));
}

/** The real time in milliseconds, whatever clock is installed. */
export const realNow = () => real.dateNow();

/** Resolves after `ms` of real time, whatever clock is installed. */
export function realSleep(ms) {
  return new Promise((resolve) => real.setTimeout(resolve, ms));
}

/**
 * A clock the test moves: `Date.now`, `performance.now` and the timer functions read it. Timers
 * fire only when {@link FakeClock.advanceTo} passes their time.
 */
export class FakeClock {
  constructor(start = 1_793_300_000_000) {
    this.now = start;
    this.origin = start - 1000;
    this.timers = [];
    this.nextId = 1;
  }

  install() {
    const clock = this;
    Date.now = () => clock.now;
    performance.now = () => clock.now - clock.origin;
    globalThis.setTimeout = (fn, ms = 0, ...args) => clock.schedule(fn, ms, args, 0);
    globalThis.setInterval = (fn, ms = 0, ...args) => clock.schedule(fn, ms, args, Math.max(1, ms));
    globalThis.clearTimeout = (handle) => clock.cancel(handle);
    globalThis.clearInterval = (handle) => clock.cancel(handle);
    return this;
  }

  restore() {
    Date.now = real.dateNow;
    performance.now = real.performanceNow;
    globalThis.setTimeout = real.setTimeout;
    globalThis.setInterval = real.setInterval;
    globalThis.clearTimeout = real.clearTimeout;
    globalThis.clearInterval = real.clearInterval;
  }

  schedule(fn, ms, args, every) {
    const timer = { id: this.nextId++, at: this.now + Math.max(0, Number(ms) || 0), fn, args, every };
    this.timers.push(timer);
    const handle = { id: timer.id, ref: () => handle, unref: () => handle, hasRef: () => true, [Symbol.toPrimitive]: () => timer.id };
    return handle;
  }

  cancel(handle) {
    const id = typeof handle === 'object' && handle ? handle.id : handle;
    this.timers = this.timers.filter((timer) => timer.id !== id);
  }

  /** Moves the clock to `at`, firing every timer due by then in order. */
  async advanceTo(at) {
    for (;;) {
      const due = this.timers.filter((timer) => timer.at <= at).sort((a, b) => a.at - b.at || a.id - b.id)[0];
      if (!due) break;
      this.now = Math.max(this.now, due.at);
      if (due.every) due.at += due.every;
      else this.timers = this.timers.filter((timer) => timer !== due);
      if (typeof due.fn === 'function') due.fn(...due.args);
      await flush();
    }
    this.now = Math.max(this.now, at);
    await flush();
  }

  advance(ms) {
    return this.advanceTo(this.now + ms);
  }
}

/** The text a spawn's stdin option holds, or a promise of it. */
function stdinText(stdin) {
  if (stdin == null || stdin === 'ignore' || stdin === 'inherit' || stdin === 'pipe') return null;
  if (typeof stdin === 'string') return stdin;
  if (stdin instanceof ArrayBuffer) return new TextDecoder().decode(stdin);
  if (ArrayBuffer.isView(stdin)) return new TextDecoder().decode(stdin);
  if (typeof stdin.text === 'function') return stdin.text();
  if (typeof stdin.getReader === 'function') return new Response(stdin).text();
  return Promise.resolve(`<unreadable stdin ${Object.prototype.toString.call(stdin)}>`);
}

/**
 * What a fake child does: `stdout` it writes, `stderr`, the `code` it exits with or `signal` it
 * dies of, after `delayMs` on the installed clock; `hang` never exits until killed; `throws` makes
 * the spawn itself throw; `refuseStdin` makes writes to a piped stdin fail.
 */
const QUICK = { stdout: '', stderr: '', code: 0, delayMs: 0 };

/** The fake `Bun`: it records every spawn and file access, and runs no process. */
export class FakeBun {
  constructor() {
    this.reset();
  }

  reset() {
    this.spawns = [];
    this.files = [];
    this.clock = null;
    this.behave = () => QUICK;
    this.version = '1.4.4';
  }

  spawn(cmd, options = {}) {
    const argv = Array.isArray(cmd) ? [...cmd] : [...(cmd?.cmd ?? [])];
    const opts = Array.isArray(cmd) ? options : cmd;
    const behaviour = { ...QUICK, ...this.behave(argv, opts) };
    const record = { argv, options: opts, behaviour, at: this.clock?.now ?? Date.now(), killed: false, stdin: null };
    this.spawns.push(record);
    if (behaviour.throws) throw Object.assign(new Error(`ENOENT: no such file or directory, posix_spawn '${argv[0]}'`), { code: 'ENOENT' });
    const writes = [];
    const piped = opts?.stdin === 'pipe';
    record.stdin = piped ? null : stdinText(opts?.stdin);
    let exit;
    const exited = new Promise((resolve) => {
      exit = resolve;
    });
    const stream = (text, open) => {
      let control;
      const readable = new ReadableStream({
        start(c) {
          control = c;
          if (!open) {
            if (text) c.enqueue(new TextEncoder().encode(text));
            c.close();
          }
        },
      });
      readable.text = () => new Response(readable).text();
      return { readable, finish: (rest) => {
        try {
          if (rest) control.enqueue(new TextEncoder().encode(rest));
          control.close();
        } catch {
          // Closed already.
        }
      } };
    };
    const pipeOut = (opts?.stdout ?? 'pipe') === 'pipe';
    const out = pipeOut ? stream('', true) : null;
    const err = opts?.stderr === 'pipe' ? stream('', true) : null;
    const child = {
      pid: 90_000 + this.spawns.length,
      stdout: out?.readable ?? null,
      stderr: err?.readable ?? null,
      stdin: piped
        ? {
            write(chunk) {
              if (behaviour.refuseStdin) throw Object.assign(new Error('EPIPE: broken pipe, write'), { code: 'EPIPE' });
              writes.push(typeof chunk === 'string' ? chunk : new TextDecoder().decode(chunk));
              return chunk?.length ?? 0;
            },
            flush() {
              if (behaviour.refuseStdin) throw Object.assign(new Error('EPIPE: broken pipe, flush'), { code: 'EPIPE' });
            },
            end() {
              record.stdin = writes.join('');
              if (behaviour.refuseStdin) throw Object.assign(new Error('EPIPE: broken pipe, end'), { code: 'EPIPE' });
            },
            close() {},
          }
        : null,
      exited,
      exitCode: null,
      signalCode: null,
      killed: false,
      kill(signal = 'SIGTERM') {
        if (child.exitCode !== null || child.signalCode !== null) return;
        record.killed = true;
        child.killed = true;
        child.signalCode = signal;
        out?.finish('');
        err?.finish('');
        exit(143);
      },
      unref() {},
      ref() {},
    };
    record.child = child;
    const finish = () => {
      if (child.killed) return;
      out?.finish(behaviour.stdout);
      err?.finish(behaviour.stderr);
      if (behaviour.signal) child.signalCode = behaviour.signal;
      else child.exitCode = behaviour.code;
      exit(behaviour.signal ? 134 : behaviour.code);
    };
    if (!behaviour.hang) {
      if (behaviour.delayMs && this.clock) this.clock.schedule(finish, behaviour.delayMs, [], 0);
      else if (behaviour.delayMs) real.setTimeout(finish, behaviour.delayMs);
      else queueMicrotask(finish);
    }
    if (typeof opts?.timeout === 'number' && opts.timeout > 0) {
      const kill = () => child.kill(opts.killSignal ?? 'SIGTERM');
      if (this.clock) this.clock.schedule(kill, opts.timeout, [], 0);
      else real.setTimeout(kill, opts.timeout);
    }
    opts?.signal?.addEventListener?.('abort', () => child.kill());
    return child;
  }

  file(...args) {
    this.files.push(['Bun.file', ...args]);
    throw new Error('wtpool-verify: the plugin opened a file through Bun.file');
  }

  write(...args) {
    this.files.push(['Bun.write', ...args.slice(0, 1)]);
    throw new Error('wtpool-verify: the plugin wrote a file through Bun.write');
  }

  readableStreamToText(stream) {
    return new Response(stream).text();
  }

  /** Every spawn's stdin as text, once the ones given as a Blob or stream are read. */
  async settle() {
    for (const spawn of this.spawns) spawn.stdin = await spawn.stdin;
    return this.spawns;
  }
}

/** The hooks of OpenCode 2.2.0, by domain, and the domains with a transform. */
const HOOK_DOMAINS = ['tool', 'session', 'permission', 'shell', 'aisdk'];
const DOMAINS = [
  'agent', 'aisdk', 'command', 'credential', 'event', 'experimental', 'generate', 'integration', 'mcp', 'model',
  'permission', 'plugin', 'provider', 'reference', 'rpc', 'session', 'shell', 'skill', 'storage', 'tool', 'vcs',
  'websearch', 'worktree',
];

/**
 * One plugin instance's context: `location.directory`, an event stream the test feeds, a `hook`
 * and `transform` on every domain that record what the plugin registers, and `session.get`.
 * `refuse` names domains whose `hook` throws (`'throw'`) or rejects (`'reject'`). `session.get`
 * answers a session of `sessions`, by id, else that id alone, as OpenCode answers for a session
 * with no title yet; `lookup` replaces it.
 */
export class FakeContext {
  constructor(directory, { refuse = {}, sessions = {}, lookup } = {}) {
    this.directory = directory;
    this.registered = [];
    this.lookups = [];
    this.transforms = [];
    this.subscriptions = 0;
    this.queue = [];
    this.waiting = [];
    this.pulls = 0;
    this.ended = false;
    this.pullWaiters = [];
    const ctx = {
      app: { name: 'cli', version: '2.2.0', channel: 'latest' },
      location: { directory, project: { id: '3d7fadac9684354168a1bd2b0daa4d54c10c0a11', directory, canonical: directory } },
      options: {},
    };
    for (const domain of DOMAINS) ctx[domain] = {};
    for (const domain of HOOK_DOMAINS) {
      ctx[domain].hook = (name, callback) => {
        const how = refuse[domain];
        if (how === 'throw') throw new Error(`wtpool-verify: ${domain}.hook refuses ${name}`);
        if (how === 'reject') return Promise.reject(new Error(`wtpool-verify: ${domain}.hook rejects ${name}`));
        this.registered.push({ domain, name, callback });
        return Promise.resolve({ dispose: async () => {} });
      };
    }
    for (const domain of DOMAINS) {
      ctx[domain].transform ??= (callback) => {
        this.transforms.push({ domain, callback });
        return Promise.resolve({ dispose: async () => {} });
      };
    }
    ctx.session.get = (input) => {
      this.lookups.push(structuredClone(input));
      if (lookup) return lookup(input);
      const id = input?.sessionID;
      return Promise.resolve(Object.hasOwn(sessions, id) ? structuredClone(sessions[id]) : { id });
    };
    ctx.event.subscribe = () => {
      this.subscriptions += 1;
      return { [Symbol.asyncIterator]: () => this.iterator() };
    };
    this.ctx = ctx;
  }

  iterator() {
    return {
      next: () => {
        this.pulls += 1;
        for (const resolve of this.pullWaiters.splice(0)) resolve();
        if (this.queue.length) return Promise.resolve({ value: this.queue.shift(), done: false });
        if (this.ended) return Promise.resolve({ value: undefined, done: true });
        return new Promise((resolve) => this.waiting.push(resolve));
      },
      return: () => {
        this.ended = true;
        return Promise.resolve({ value: undefined, done: true });
      },
      [Symbol.asyncIterator]() {
        return this;
      },
    };
  }

  /** The callback the plugin registered as `<domain>.<name>`, such as `tool.execute.before`. */
  hook(full) {
    const at = full.indexOf('.');
    const domain = full.slice(0, at);
    const name = full.slice(at + 1);
    return this.registered.find((r) => r.domain === domain && r.name === name)?.callback;
  }

  /** Calls a registered hook as OpenCode does, awaiting it. Gives undefined for an unregistered one. */
  async call(full, input) {
    const callback = this.hook(full);
    if (!callback) return undefined;
    return callback(input);
  }

  /**
   * Gives the plugin one event and resolves once it asks for the next, or after 1 s of real
   * time when it no longer reads.
   */
  async feed(event) {
    const resolve = this.waiting.shift();
    if (resolve) resolve({ value: event, done: false });
    else this.queue.push(event);
    const limit = realSleep(this.stalled ? 0 : 1000).then(() => 'late');
    while (this.queue.length || !this.waiting.length) {
      const pulled = await Promise.race([new Promise((done) => this.pullWaiters.push(done)), limit]);
      if (pulled === 'late') {
        // A plugin that stopped reading costs the replay one wait, not one per event.
        this.stalled = true;
        break;
      }
    }
    await flush();
  }

  end() {
    this.ended = true;
    for (const resolve of this.waiting.splice(0)) resolve({ value: undefined, done: true });
  }
}
