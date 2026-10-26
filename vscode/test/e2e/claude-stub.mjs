#!/usr/bin/env node
// A stand in for one `claude` process of the e2e layer: it stays alive and runs `wtpool hook` as
// its child for each request, so the hook's parent is a live process that a step can stop.
//
// stdin: one JSON request per line, {id, binary, args?, payload, cwd, env, detach?}. stdout: one
// JSON reply per request, {id, code, signal, stdout, stderr}, after that child exited. `args`, by
// default `hook`, lets `wtpool hook policy` run from the same parent. With `detach` the child is a
// command the agent starts and leaves running, such as `wtpool job -- cargo test`, and the reply,
// {id, pid}, comes at once.
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

const lines = createInterface({ input: process.stdin });

lines.on('line', (line) => {
  let request;
  try {
    request = JSON.parse(line);
  } catch {
    return;
  }
  if (request.detach) {
    const started = spawn(request.binary, request.args, { cwd: request.cwd, env: request.env, stdio: 'ignore' });
    started.on('error', (error) => {
      process.stdout.write(`${JSON.stringify({ id: request.id, pid: null, stderr: String(error) })}\n`);
    });
    started.on('spawn', () => process.stdout.write(`${JSON.stringify({ id: request.id, pid: started.pid })}\n`));
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
    process.stdout.write(`${JSON.stringify({ id: request.id, code: null, signal: null, stdout: '', stderr: String(error) })}\n`);
  });
  child.on('close', (code, signal) => {
    const reply = {
      id: request.id,
      code,
      signal,
      stdout: Buffer.concat(out).toString('utf8'),
      stderr: Buffer.concat(err).toString('utf8').slice(0, 2000),
    };
    process.stdout.write(`${JSON.stringify(reply)}\n`);
  });
  child.stdin.on('error', () => {});
  child.stdin.end(JSON.stringify(request.payload));
});

lines.on('close', () => process.exit(0));
