/**
 * @fileoverview The wtpool plugin for OpenCode 2.2. It forwards the hooks and events of the
 * sessions of its folder to `wtpool hook --harness opencode`, trimmed to the fields wtpool reads,
 * and with `policy` set it refuses the shell calls `wtpool hook policy` denies. OpenCode runs one
 * instance per folder and sends every event to each, so an instance forwards only the events of
 * its own sessions. A folder's first events can go out before its instance reads any, so a
 * session first seen without its title is looked up once and its title forwarded.
 * `wtpool hook install --harness opencode` prints this file with {@link CONFIG}
 * filled in; it imports nothing, so it runs as is in OpenCode and in Node's tests.
 */

/** A field of a forwarded hook or event, as dotted paths name them. */
type Fields = Record<string, unknown>;

/** A request a later reply answers, kept by its id with the call it belongs to. */
interface Ask {
  map: string;
  id: string;
  call: string;
}

/** The answer to the request of `map` named by the field `id`. */
interface Reply {
  map: string;
  id: string;
}

/** One forwarded hook, event or lookup: its kept fields, its session field and its role. */
interface Entry {
  keep: string[];
  session: string;
  parent?: string;
  ask?: Ask;
  reply?: Reply;
  throttle?: boolean;
  changes?: string[];
  /**
   * The fields a lookup is made for: a session first seen in an entry that keeps one of them is
   * not looked up, and an answer without them is not sent.
   */
  needs?: string[];
}

interface Limits {
  sessions: number;
  asks: number;
  shells: number;
  shellMs: number;
  progressMs: number;
  policyMs: number;
  bytes: number;
}

/** OpenCode's own id form, `<letters>_<letters and digits>`, and the prefix of its stems. */
interface IdForm {
  prefix: string;
  letters: number;
  max: number;
}

interface ShellCall {
  tool: string;
  command: string;
}

interface Config {
  bin: string;
  policy: boolean;
  shell: ShellCall;
  ids: IdForm;
  hooks: Record<string, Entry>;
  events: Record<string, Entry>;
  lookups: Record<string, Entry>;
  limits: Limits;
}

type Hook = (input: Fields) => Promise<void>;

interface Domain {
  hook(name: string, callback: Hook): unknown;
}

interface Location {
  directory: string;
}

interface Events {
  subscribe(): AsyncIterable<unknown>;
}

interface Sessions {
  get(input: { sessionID: string }): Promise<unknown>;
}

/** The part of OpenCode's plugin context the plugin uses. */
interface Context {
  location: Location;
  event: Events;
  tool: Domain;
  shell: Domain;
  session: Sessions;
}

interface Child {
  stdout: unknown;
  exited: Promise<number>;
  kill(): void;
}

interface SpawnOptions {
  stdin: Uint8Array;
  stdout: 'ignore' | 'pipe';
  stderr: 'ignore';
}

interface Runtime {
  spawn(argv: string[], options: SpawnOptions): Child;
}

interface Encoder {
  encode(text: string): Uint8Array;
}

interface Body {
  text(): Promise<string>;
}

declare const Bun: Runtime;
declare const TextEncoder: new () => Encoder;
declare const Response: new (body: unknown) => Body;
declare function setTimeout(callback: () => void, ms: number): unknown;
declare function clearTimeout(timer: unknown): void;

// prettier-ignore
const CONFIG: Config = {"bin":"/usr/local/bin/wtpool","policy":false,"shell":{"tool":"shell","command":"input.command"},"ids":{"prefix":"opencode-","letters":8,"max":64},"hooks":{"tool.execute.before":{"keep":["sessionID","id","tool","input.path","input.command","input.agent"],"session":"sessionID"}},"events":{"session.created":{"keep":["sessionID","parentID","title","agent"],"session":"sessionID","parent":"parentID"},"session.execution.started":{"keep":["sessionID"],"session":"sessionID"},"session.execution.succeeded":{"keep":["sessionID","reason"],"session":"sessionID"},"session.execution.interrupted":{"keep":["sessionID","reason"],"session":"sessionID"},"session.execution.failed":{"keep":["sessionID","error.type"],"session":"sessionID"},"session.tool.success":{"keep":["sessionID","id"],"session":"sessionID"},"session.tool.failed":{"keep":["sessionID","id"],"session":"sessionID"},"permission.asked":{"keep":["sessionID","id","source.id"],"session":"sessionID","ask":{"map":"permission","id":"id","call":"source.id"}},"permission.replied":{"keep":["sessionID","requestID","reply"],"session":"sessionID","reply":{"map":"permission","id":"requestID"}},"form.created":{"keep":["form.id","form.sessionID","form.metadata.kind","form.metadata.tool.id"],"session":"form.sessionID","ask":{"map":"form","id":"form.id","call":"form.metadata.tool.id"}},"form.replied":{"keep":["id","sessionID"],"session":"sessionID","reply":{"map":"form","id":"id"}},"session.renamed":{"keep":["sessionID","title"],"session":"sessionID"},"session.step.started":{"keep":["sessionID","model.id","model.providerID"],"session":"sessionID","changes":["model.id","model.providerID"]},"session.text.delta":{"keep":["sessionID"],"session":"sessionID","throttle":true},"session.reasoning.delta":{"keep":["sessionID"],"session":"sessionID","throttle":true},"session.tool.input.delta":{"keep":["sessionID"],"session":"sessionID","throttle":true},"session.deleted":{"keep":["sessionID"],"session":"sessionID"}},"lookups":{"session.get":{"keep":["id","parentID","title"],"session":"id","parent":"parentID","needs":["title"]}},"limits":{"sessions":1024,"asks":256,"shells":64,"shellMs":10000,"progressMs":10000,"policyMs":2000,"bytes":1048576}};

/** A session an instance has seen, with the root session it belongs to. */
interface Session {
  root: string;
  progressAt?: number;
  model?: string;
  deleted?: boolean;
}

/** A shell call whose shell has not started yet. */
interface PendingShell {
  root: string;
  command: string;
  at: number;
}

interface Instance {
  directory: string;
  sessions: Map<string, Session>;
  asks: Map<string, Map<string, string>>;
  shells: PendingShell[];
  open: boolean;
  /** OpenCode's sessions, to look one up. */
  store: Sessions;
}

/** The output of `wtpool hook policy`. */
interface Answer {
  stdout: string;
  code: number;
}

/** The lookup of a session's stored fields, `ctx.session.get`. */
const LOOKUP = 'session.get';
/** The event that ends a session: a lookup's answer after it would bring the session back. */
const DELETED = 'session.deleted';

export default { id: 'wtpool', setup };

async function setup(ctx: Context): Promise<() => void> {
  const directory = ctx.location?.directory;
  const instance: Instance = {
    directory: typeof directory === 'string' ? directory : '',
    sessions: new Map(),
    asks: new Map(),
    shells: [],
    open: true,
    store: ctx.session,
  };
  void forwardEvents(ctx, instance);
  await register(ctx.tool, 'execute.before', (input) => beforeTool(instance, input));
  await register(ctx.shell, 'create.before', async (input) => {
    beforeShell(instance, input);
  });
  return () => {
    instance.open = false;
    instance.sessions.clear();
    instance.asks.clear();
    instance.shells = [];
  };
}

async function register(domain: Domain, name: string, callback: Hook): Promise<void> {
  try {
    await domain.hook(name, callback);
  } catch {
    // A hook that fails to register leaves the others working.
    return;
  }
}

async function forwardEvents(ctx: Context, instance: Instance): Promise<void> {
  try {
    for await (const event of ctx.event.subscribe()) {
      if (!instance.open) {
        return;
      }
      try {
        onEvent(instance, event);
      } catch {
        // An event that cannot be forwarded drops alone.
        continue;
      }
    }
  } catch {
    // The stream ends with the instance.
    return;
  }
}

/**
 * Forwards `event` when it is one of the table's and of a session of this instance: located in
 * its folder, or without a location for a session it has seen.
 */
function onEvent(instance: Instance, event: unknown): void {
  if (!isFields(event) || typeof event.type !== 'string' || !isFields(event.data)) {
    return;
  }
  const entry = entryOf(CONFIG.events, event.type);
  const session = text(at(event.data, entry?.session ?? 'sessionID'));
  if (session === undefined) {
    return;
  }
  const location = event.location;
  if (isFields(location) && typeof location.directory === 'string') {
    if (location.directory !== instance.directory) {
      return;
    }
  } else if (!instance.sessions.has(session)) {
    return;
  }
  const state = learn(instance, session, entry);
  if (entry !== undefined) {
    forwardEvent(instance, event.type, entry, session, state, event.data);
  }
}

function forwardEvent(
  instance: Instance,
  name: string,
  entry: Entry,
  session: string,
  state: Session,
  data: Fields,
): void {
  adoptRoot(instance, entry, session, state, data);
  if (entry.throttle === true) {
    const now = Date.now();
    if (state.progressAt !== undefined && now - state.progressAt < CONFIG.limits.progressMs) {
      return;
    }
    state.progressAt = now;
  }
  if (entry.changes !== undefined) {
    const model = JSON.stringify(entry.changes.map((field) => at(data, field) ?? null));
    if (state.model === model) {
      return;
    }
    state.model = model;
  }
  if (entry.ask !== undefined) {
    remember(instance, entry.ask, data);
  }
  const call = entry.reply === undefined ? undefined : answered(instance, entry.reply, data);
  send(instance, 'event', name, session, call, keep(data, entry.keep));
}

/** Gives a child session its parent's root, from the entry's parent field. */
function adoptRoot(
  instance: Instance,
  entry: Entry,
  session: string,
  state: Session,
  data: Fields,
): void {
  if (entry.parent === undefined) {
    return;
  }
  const parent = text(at(data, entry.parent));
  if (parent !== undefined && parent !== session) {
    state.root = instance.sessions.get(parent)?.root ?? parent;
  }
}

/**
 * Marks `id` as seen, or as deleted by its deletion, and looks up a session first seen in another
 * entry that keeps none of the lookup's fields: OpenCode may have sent the events that named it
 * before this instance read any.
 */
function learn(instance: Instance, id: string, entry: Entry | undefined): Session {
  const known = instance.sessions.has(id);
  const state = seen(instance, id);
  if (entry !== undefined && entry === entryOf(CONFIG.events, DELETED)) {
    state.deleted = true;
    return state;
  }
  const lookup = entryOf(CONFIG.lookups, LOOKUP);
  const needs = lookup?.needs ?? [];
  if (!known && lookup !== undefined && !entry?.keep.some((field) => needs.includes(field))) {
    void lookUp(instance, id, lookup, state);
  }
  return state;
}

/**
 * Sends the lookup of session `id`, whose record is `asked`, when OpenCode's answer is that
 * session's, with its fields, and the session was not deleted meanwhile.
 */
async function lookUp(instance: Instance, id: string, entry: Entry, asked: Session): Promise<void> {
  try {
    const found = await instance.store.get({ sessionID: id });
    if (
      !instance.open ||
      asked.deleted === true ||
      !isFields(found) ||
      text(at(found, entry.session)) !== id
    ) {
      return;
    }
    if (!(entry.needs ?? []).every((field) => text(at(found, field)) !== undefined)) {
      return;
    }
    const state = seen(instance, id);
    adoptRoot(instance, entry, id, state, found);
    send(instance, 'lookup', LOOKUP, id, undefined, keep(found, entry.keep));
  } catch {
    // The session goes on without the lookup.
    return;
  }
}

/** Keeps the call of an open request, the oldest dropped past the limit. */
function remember(instance: Instance, ask: Ask, data: Fields): void {
  const id = text(at(data, ask.id));
  const call = text(at(data, ask.call));
  if (id === undefined || call === undefined) {
    return;
  }
  const calls = instance.asks.get(ask.map) ?? new Map<string, string>();
  instance.asks.set(ask.map, calls);
  calls.delete(id);
  calls.set(id, call);
  dropOldest(calls, CONFIG.limits.asks);
}

/** The call of the request a reply answers; the reply closes the request. */
function answered(instance: Instance, reply: Reply, data: Fields): string | undefined {
  const id = text(at(data, reply.id));
  const calls = instance.asks.get(reply.map);
  if (id === undefined || calls === undefined) {
    return undefined;
  }
  const call = calls.get(id);
  calls.delete(id);
  return call;
}

/** Records the call, and under `policy` throws the reason of a shell call's denial. */
async function beforeTool(instance: Instance, input: Fields): Promise<void> {
  let denial: string | undefined;
  try {
    denial = await recordCall(instance, input);
  } catch {
    denial = undefined;
  }
  if (denial !== undefined) {
    throw new Error(denial);
  }
}

async function recordCall(instance: Instance, input: Fields): Promise<string | undefined> {
  const entry = entryOf(CONFIG.hooks, 'tool.execute.before');
  const session = text(at(input, entry?.session ?? 'sessionID'));
  if (entry === undefined || session === undefined) {
    return undefined;
  }
  const state = learn(instance, session, entry);
  send(instance, 'hook', 'tool.execute.before', session, undefined, keep(input, entry.keep));
  const command = at(input, CONFIG.shell.command);
  if (input.tool !== CONFIG.shell.tool || typeof command !== 'string') {
    return undefined;
  }
  const pending: PendingShell = { root: state.root, command, at: Date.now() };
  instance.shells.push(pending);
  if (instance.shells.length > CONFIG.limits.shells) {
    instance.shells.shift();
  }
  if (!CONFIG.policy) {
    return undefined;
  }
  const reason = await judge(instance, input, session, command);
  if (reason !== undefined) {
    instance.shells = instance.shells.filter((shell) => shell !== pending);
  }
  return reason;
}

/**
 * Names the root session of the shell call in `WTPOOL_SESSION`, and removes the variable when
 * the call is unknown or ambiguous, so an inherited value never names another session.
 */
function beforeShell(instance: Instance, input: Fields): void {
  try {
    const env = input.env;
    if (!isFields(env)) {
      return;
    }
    const now = Date.now();
    instance.shells = instance.shells.filter((shell) => now - shell.at < CONFIG.limits.shellMs);
    const matching = instance.shells.filter((shell) => shell.command === input.command);
    const first = matching[0];
    instance.shells = instance.shells.filter((shell) => shell !== first);
    const roots = new Set(matching.map((shell) => shell.root));
    const stem = first !== undefined && roots.size === 1 ? stemOf(first.root) : undefined;
    if (stem === undefined) {
      delete env.WTPOOL_SESSION;
    } else {
      env.WTPOOL_SESSION = stem;
    }
  } catch {
    // The shell starts as OpenCode made it.
    return;
  }
}

/** The record stem of the session `id`, when the id has OpenCode's own form. */
function stemOf(id: string): string | undefined {
  const form = new RegExp(`^[a-z]{1,${CONFIG.ids.letters}}_[A-Za-z0-9]+$`);
  return id.length <= CONFIG.ids.max && form.test(id)
    ? CONFIG.ids.prefix + id.replace('_', '-')
    : undefined;
}

/** Runs `wtpool hook policy` on a shell call, and returns the reason of its denial. */
async function judge(
  instance: Instance,
  input: Fields,
  session: string,
  command: string,
): Promise<string | undefined> {
  const request = JSON.stringify({
    tool: input.tool,
    sessionID: session,
    id: input.id,
    input: { command },
    directory: instance.directory,
  });
  let child: Child;
  try {
    child = Bun.spawn([CONFIG.bin, 'hook', 'policy', '--harness', 'opencode'], {
      stdin: new TextEncoder().encode(request),
      stdout: 'pipe',
      stderr: 'ignore',
    });
  } catch {
    return undefined;
  }
  let timer: unknown;
  const late = new Promise<undefined>((resolve) => {
    timer = setTimeout(() => resolve(undefined), CONFIG.limits.policyMs);
  });
  const reading = read(child).catch(() => undefined);
  try {
    const answer = await Promise.race([reading, late]);
    if (answer === undefined) {
      stop(child);
      return undefined;
    }
    return denial(answer);
  } finally {
    clearTimeout(timer);
  }
}

async function read(child: Child): Promise<Answer> {
  const [stdout, code] = await Promise.all([new Response(child.stdout).text(), child.exited]);
  return { stdout, code };
}

function stop(child: Child): void {
  try {
    child.kill();
  } catch {
    // The child has ended.
    return;
  }
}

/** The reason of `{"decision":"deny","reason":<text>}`, when it is all the policy printed. */
function denial(answer: Answer): string | undefined {
  if (answer.code !== 0) {
    return undefined;
  }
  const line = answer.stdout.endsWith('\n') ? answer.stdout.slice(0, -1) : answer.stdout;
  if (line.includes('\n')) {
    return undefined;
  }
  let decision: unknown;
  try {
    decision = JSON.parse(line);
  } catch {
    return undefined;
  }
  if (
    !isFields(decision) ||
    Object.keys(decision).join() !== 'decision,reason' ||
    decision.decision !== 'deny' ||
    typeof decision.reason !== 'string'
  ) {
    return undefined;
  }
  return decision.reason;
}

/** Starts `wtpool hook` on one payload and returns at once. */
function send(
  instance: Instance,
  kind: string,
  name: string,
  session: string,
  call: string | undefined,
  data: Fields,
): void {
  try {
    const root = instance.sessions.get(session)?.root;
    const payload = {
      v: 1,
      kind,
      name,
      directory: instance.directory,
      ...(root === undefined || root === session ? {} : { parent: root }),
      ...(call === undefined ? {} : { call }),
      data,
    };
    const stdin = new TextEncoder().encode(JSON.stringify(payload));
    if (stdin.length > CONFIG.limits.bytes) {
      return;
    }
    Bun.spawn([CONFIG.bin, 'hook', '--harness', 'opencode'], {
      stdin,
      stdout: 'ignore',
      stderr: 'ignore',
    });
  } catch {
    // The session goes on without its record.
    return;
  }
}

/** Marks `id` as the most recently seen session, the least recently seen dropped past the limit. */
function seen(instance: Instance, id: string): Session {
  const state = instance.sessions.get(id) ?? { root: id };
  instance.sessions.delete(id);
  instance.sessions.set(id, state);
  dropOldest(instance.sessions, CONFIG.limits.sessions);
  return state;
}

function dropOldest<V>(map: Map<string, V>, limit: number): void {
  for (const key of map.keys()) {
    if (map.size <= limit) {
      return;
    }
    map.delete(key);
  }
}

/** The scalars of `source` at `paths`, nested as in `source`. */
function keep(source: Fields, paths: string[]): Fields {
  const kept: Fields = {};
  for (const path of paths) {
    const value = at(source, path);
    if (!isScalar(value)) {
      continue;
    }
    const keys = path.split('.');
    const last = keys.pop() ?? path;
    let into = kept;
    for (const key of keys) {
      const next = into[key];
      if (isFields(next)) {
        into = next;
      } else {
        const made: Fields = {};
        into[key] = made;
        into = made;
      }
    }
    into[last] = value;
  }
  return kept;
}

function at(source: Fields, path: string): unknown {
  let value: unknown = source;
  for (const key of path.split('.')) {
    if (!isFields(value) || !Object.prototype.hasOwnProperty.call(value, key)) {
      return undefined;
    }
    value = value[key];
  }
  return value;
}

function entryOf(table: Record<string, Entry>, name: string): Entry | undefined {
  return Object.prototype.hasOwnProperty.call(table, name) ? table[name] : undefined;
}

function text(value: unknown): string | undefined {
  return typeof value === 'string' && value !== '' ? value : undefined;
}

function isScalar(value: unknown): boolean {
  return (
    value === null ||
    typeof value === 'string' ||
    typeof value === 'number' ||
    typeof value === 'boolean'
  );
}

function isFields(value: unknown): value is Fields {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
