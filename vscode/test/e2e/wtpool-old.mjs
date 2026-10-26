#!/usr/bin/env node
// A wtpool from before `wtpool job`: it runs the wtpool that argv[2] names with the rest of argv,
// stdin and stderr passed through, and prints each line of its stdout with `machine.jobs` dropped.
// `wtpool job …` fails as an unknown subcommand. The exit code is wtpool's.
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

const [binary, ...args] = process.argv.slice(2);
if (args[0] === 'job') {
  process.stderr.write("error: unrecognized subcommand 'job'\n");
  process.exit(2);
}
const child = spawn(binary, args, { stdio: ['pipe', 'pipe', 'inherit'] });
process.stdin.pipe(child.stdin);
child.stdin.on('error', () => {});
createInterface({ input: child.stdout }).on('line', (line) => {
  let out = line;
  try {
    const document = JSON.parse(line);
    if (document && typeof document.machine === 'object' && document.machine !== null) {
      delete document.machine.jobs;
      out = JSON.stringify(document);
    }
  } catch {
    // Not JSON: passed as it is.
  }
  process.stdout.write(`${out}\n`);
});
child.on('close', (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  else process.exit(code ?? 1);
});
