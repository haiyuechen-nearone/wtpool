#!/usr/bin/env node
// A stand in for one `claude` process of the e2e layer: it stays alive and runs `wtpool hook` as
// its child for each request, so the hook's parent is a live process that a step can stop.
//
// stdin: one JSON request per line, {id, binary, args?, payload, cwd, env, detach?}. stdout: one
// JSON reply per request, {id, code, signal, stdout, stderr}, after that child exited. `args`, by
// default `hook`, lets `wtpool hook policy` run from the same parent. With `detach` the child is a
// command the agent starts and leaves running, such as `wtpool job -- cargo test`, and the reply,
// {id, pid}, comes at once. With `spawn` the child is a stub of this same kind, such as the
// OpenCode server a coordinator starts, and the reply, {id, pid}, comes once it runs. A later
// request with `to` set to that request's id goes to the child, which answers with the request's
// own `id`.
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

const lines = createInterface({ input: process.stdin });
const children = new Map();

function reply(body) {
  process.stdout.write(`${JSON.stringify(body)}\n`);
}

function startChild(request) {
  const child = spawn(process.execPath, [fileURLToPath(import.meta.url)], {
    cwd: request.cwd,
    env: request.env,
    stdio: ['pipe', 'pipe', 'ignore'],
  });
  children.set(request.id, child);
  createInterface({ input: child.stdout }).on('line', (line) => process.stdout.write(`${line}\n`));
  child.stdin.on('error', () => {});
  child.on('error', (error) => reply({ id: request.id, pid: null, stderr: String(error) }));
  child.on('spawn', () => reply({ id: request.id, pid: child.pid }));
  child.on('exit', () => children.delete(request.id));
}

function stopChildren() {
  for (const child of children.values()) child.kill('SIGTERM');
  process.exit(0);
}

process.on('SIGTERM', stopChildren);

lines.on('line', (line) => {
  let request;
  try {
    request = JSON.parse(line);
  } catch {
    return;
  }
  if (request.spawn) {
    startChild(request);
    return;
  }
  if (request.to !== undefined) {
    const { to, ...forwarded } = request;
    const child = children.get(to);
    if (child) child.stdin.write(`${JSON.stringify(forwarded)}\n`);
    else reply({ id: request.id, code: null, signal: null, stdout: '', stderr: `no child ${to}` });
    return;
  }
  if (request.detach) {
    const started = spawn(request.binary, request.args, { cwd: request.cwd, env: request.env, stdio: 'ignore' });
    started.on('error', (error) => reply({ id: request.id, pid: null, stderr: String(error) }));
    started.on('spawn', () => reply({ id: request.id, pid: started.pid }));
    return;
  }
  const child = spawn(request.binary, request.args ?? ['hook'], {
    cwd: request.cwd,
    env: request.env,
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  const out = [];
  const err = [];
  child.stdout.on('data', (chunk) => out.push(chunk));
  child.stderr.on('data', (chunk) => err.push(chunk));
  child.on('error', (error) => {
    reply({ id: request.id, code: null, signal: null, stdout: '', stderr: String(error) });
  });
  child.on('close', (code, signal) => {
    reply({
      id: request.id,
      code,
      signal,
      stdout: Buffer.concat(out).toString('utf8'),
      stderr: Buffer.concat(err).toString('utf8').slice(0, 2000),
    });
  });
  child.stdin.on('error', () => {});
  child.stdin.end(JSON.stringify(request.payload));
});

lines.on('close', stopChildren);
