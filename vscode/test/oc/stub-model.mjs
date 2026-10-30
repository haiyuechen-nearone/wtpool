// A scripted OpenAI compatible chat model on loopback, so OpenCode runs with no credential: a
// Node port of the recording's stub model. The first user messages name a script as
// `[[scenario:<name>]]`; the number of assistant messages in the request picks its step. A
// request without tools, a title request, gets a short title.
import { appendFileSync } from 'node:fs';
import http from 'node:http';

const ESSAY_WORDS = 800;

function essay() {
  const base = 'Worktrees let one repository hold several checkouts at once, each on its own branch, so a build in one folder never disturbs the edits in another.'.split(' ');
  return Array.from({ length: ESSAY_WORDS }, (_, i) => base[i % base.length]);
}

/** A step that calls `tool` with `args`. */
export const call = (tool, args) => ({ calls: [{ tool, args }] });
/** A step that answers `text`. */
export const say = (text) => ({ text });
/** A step that streams `words` words, `gapMs` apart. */
export const slow = (words, gapMs, { essay: isEssay = false } = {}) => ({ slowText: words, gapMs, essay: isEssay });

function textOf(content) {
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) return content.map((part) => (part && typeof part === 'object' ? (part.text ?? '') : '')).join(' ');
  return '';
}

/**
 * Starts the stub on a free loopback port. `scripts` maps a scenario name to its steps; the
 * caller may add scripts while it runs. `log` names a JSON lines file of every request.
 */
export function startStubModel({ scripts = {}, log } = {}) {
  const record = (entry) => {
    if (log) appendFileSync(log, `${JSON.stringify({ t: Date.now(), ...entry })}\n`);
  };
  const pick = (body) => {
    const messages = body.messages ?? [];
    const users = messages.filter((m) => m.role === 'user').map((m) => textOf(m.content));
    const name = /\[\[scenario:([a-z0-9_]+)\]\]/.exec(users.join(' '))?.[1] ?? 'smoke';
    if (!body.tools?.length) return { name, step: -1, plan: say(`Stub title ${name}`) };
    const steps = scripts[name] ?? [say('ready')];
    const done = messages.filter((m) => m.role === 'assistant').length;
    return { name, step: done, plan: steps[Math.min(done, steps.length - 1)] };
  };
  const server = http.createServer((req, res) => {
    const chunks = [];
    req.on('data', (chunk) => chunks.push(chunk));
    req.on('end', () => {
      if (req.method === 'GET') {
        reply(res, 200, { object: 'list', data: [{ id: 'scripted', object: 'model' }] });
        return;
      }
      let body = {};
      try {
        body = JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}');
      } catch {
        // An unreadable request gets the smoke answer.
      }
      const { name, step, plan } = pick(body);
      const toolResults = (body.messages ?? []).filter((m) => m.role === 'tool').map((m) => ({ id: m.tool_call_id, content: textOf(m.content).slice(0, 2048) }));
      record({ path: req.url, scenario: name, step, stream: body.stream, tools: (body.tools ?? []).map((t) => t.function?.name), roles: (body.messages ?? []).map((m) => m.role), toolResults });
      if (plan.status) {
        reply(res, plan.status, { error: { message: plan.message ?? 'stub error', type: 'stub_error', param: null, code: null } });
      } else if (body.stream) {
        stream(res, plan, name, step, record);
      } else {
        reply(res, 200, complete(plan, name, step));
      }
    });
  });
  const ready = new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server.address().port));
  });
  return {
    scripts,
    ready,
    close: () => new Promise((resolve) => {
      server.closeAllConnections?.();
      server.close(() => resolve());
    }),
  };
}

function reply(res, status, object) {
  const data = Buffer.from(JSON.stringify(object));
  res.writeHead(status, { 'content-type': 'application/json', 'content-length': data.length });
  res.end(data);
}

const toolCall = (name, step, i, call) => ({ id: `call_${name}_${step}_${i}`, type: 'function', function: { name: call.tool, arguments: JSON.stringify(call.args) } });

async function stream(res, plan, name, step, record) {
  res.writeHead(200, { 'content-type': 'text/event-stream', 'cache-control': 'no-cache' });
  const usage = { prompt_tokens: 1000, completion_tokens: 50, total_tokens: 1050 };
  const chunk = (delta, finish = null, extra) => {
    const object = { id: 'chatcmpl-stub', object: 'chat.completion.chunk', created: Math.floor(Date.now() / 1000), model: 'scripted', choices: [{ index: 0, delta, finish_reason: finish }] };
    if (extra) object.usage = extra;
    return res.write(`data: ${JSON.stringify(object)}\n\n`);
  };
  let closed = false;
  res.on('close', () => {
    closed = true;
  });
  try {
    chunk({ role: 'assistant', content: '' });
    if (plan.calls) {
      plan.calls.forEach((call, i) => chunk({ tool_calls: [{ index: i, ...toolCall(name, step, i, call) }] }));
      chunk({}, 'tool_calls', usage);
    } else if (plan.slowText) {
      const words = plan.essay ? essay() : Array.from({ length: plan.slowText }, (_, i) => `word${i}`);
      for (const word of words.slice(0, plan.slowText)) {
        if (closed) break;
        chunk({ content: `${word} ` });
        await new Promise((resolve) => setTimeout(resolve, plan.gapMs));
      }
      chunk({}, 'stop', { ...usage, completion_tokens: plan.slowText });
    } else {
      chunk({ content: plan.text });
      chunk({}, 'stop', usage);
    }
    res.end('data: [DONE]\n\n');
  } catch {
    record({ scenario: name, step, event: 'client closed the stream' });
  }
  if (closed) record({ scenario: name, step, event: 'client closed the stream' });
}

function complete(plan, name, step) {
  const message = plan.calls
    ? { role: 'assistant', content: null, tool_calls: plan.calls.map((call, i) => toolCall(name, step, i, call)) }
    : { role: 'assistant', content: plan.text ?? 'ok' };
  return {
    id: 'chatcmpl-stub',
    object: 'chat.completion',
    created: Math.floor(Date.now() / 1000),
    model: 'scripted',
    choices: [{ index: 0, message, finish_reason: plan.calls ? 'tool_calls' : 'stop' }],
    usage: { prompt_tokens: 1000, completion_tokens: 20, total_tokens: 1020 },
  };
}
