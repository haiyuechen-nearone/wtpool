#!/usr/bin/env node
// A stand in for one `claude` process of the e2e layer: it stays alive and runs `wtpool hook` as its
// child for each request, so the hook's parent is a live process that a step can stop.
//
// stdin: one JSON request per line, {id, binary, payload, cwd, env}. stdout: one JSON reply per
// request, {id, code, signal, stdout, stderr}, after that hook exited.
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
  const child = spawn(request.binary, ['hook'], {
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

// The stub lives until its stdin closes or a step stops it.
lines.on('close', () => process.exit(0));
