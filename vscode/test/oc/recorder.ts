// The oc layer's recorder plugin for OpenCode 2.2.0, beside the wtpool plugin: one JSON line per
// event and per hook of its own to $WTPOOL_VERIFY_OC_RECORDING, for the OpenCode asserts. It keeps
// ids, names and short fields, a shell's environment by its names only, and never throws.

interface Proc {
  pid: number;
  ppid: number;
  argv: string[];
  env: Record<string, string | undefined>;
  getBuiltinModule(name: string): { appendFileSync(file: string, text: string): void };
}

const proc = (globalThis as unknown as { process: Proc }).process;
const fs = proc.getBuiltinModule('node:fs');
const LOG = proc.env.WTPOOL_VERIFY_OC_RECORDING ?? '';
const INSTANCE = Math.random().toString(36).slice(2, 8);
const MAX = 2048;

function shorten(value: unknown, depth = 0): unknown {
  if (typeof value === 'string') return value.length > MAX ? `${value.slice(0, MAX)}...` : value;
  if (value === null || typeof value !== 'object' || depth > 8) return value;
  if (Array.isArray(value)) return value.slice(0, 50).map((item) => shorten(item, depth + 1));
  const out: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(value)) {
    if (key === 'env' && item && typeof item === 'object') out[key] = { names: Object.keys(item).sort() };
    else if (/^(headers|authorization|password|apiKey|key|token)$/i.test(key)) out[key] = '<redacted>';
    else out[key] = shorten(item, depth + 1);
  }
  return out;
}

function write(kind: string, name: string, payload: unknown): void {
  if (!LOG) return;
  try {
    const line = { t: Date.now(), pid: proc.pid, ppid: proc.ppid, instance: INSTANCE, kind, name, payload: shorten(payload) };
    fs.appendFileSync(LOG, `${JSON.stringify(line)}\n`);
  } catch {
    // A recorder never fails a session.
  }
}

interface Context {
  location: { directory: string };
  event: { subscribe(): AsyncIterable<{ type?: string }> };
  tool: { hook(name: string, cb: (input: unknown) => void): Promise<unknown> };
  shell: { hook(name: string, cb: (input: unknown) => void): Promise<unknown> };
}

async function setup(ctx: Context): Promise<() => void> {
  write('setup', 'context', { location: ctx.location, argv: proc.argv.slice(1) });
  void (async () => {
    try {
      for await (const event of ctx.event.subscribe()) {
        const type = String(event?.type);
        // A delta's text is no assert's business: its count and session are.
        if (type.endsWith('.delta')) write('event', type, { type, data: { sessionID: (event as { data?: { sessionID?: string } }).data?.sessionID } });
        else write('event', type, event);
      }
    } catch (error) {
      write('event.error', 'subscribe', { message: String(error) });
    }
  })();
  for (const [domain, name] of [
    ['tool', 'execute.before'],
    ['tool', 'execute.after'],
    ['shell', 'create.before'],
  ] as const) {
    try {
      await ctx[domain].hook(name, (input: unknown) => write('hook', `${domain}.${name}`, input));
    } catch (error) {
      write('register.error', `${domain}.${name}`, { message: String(error) });
    }
  }
  return () => write('cleanup', 'dispose', {});
}

export default { id: 'wtpool-verify-recorder', setup };
