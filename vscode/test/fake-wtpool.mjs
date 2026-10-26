#!/usr/bin/env node
// The fake `wtpool` of the host layer (verification.md, "Layers"). It replays documents, injects
// stream faults, and logs its argv, working directory, WTPOOL_FOREGROUND and WTPOOL_SLOT_DIR, parent,
// stdin close and exit as JSON lines.
//
// `WTPOOL_FAKE_CONFIG` names a JSON file read at each start:
//   log        file the JSON lines go to (required)
//   documents  status documents to print, in order; else `scenario`
//   scenario   a scenario name of verify/scenarios/, its expected documents replayed
//   scenarioDir   the folder of the scenario files, by default verify/scenarios/
//   faults     one fault per start, the last repeating: none, partial, oversize, garbage,
//              unknown_version, early_exit (exit 1 before any line), error_exit (write
//              `stderr` to stderr, then exit `exitCode` before any line), stall, slow_start,
//              ignore_eof
//   stderr     what error_exit writes to stderr (default one line)
//   exitCode   the exit code of error_exit (default 1)
//   lead       true: a start with a line fault (partial, oversize, garbage, unknown_version)
//              prints the first document whole before the faulty line, so the extension holds
//              a document of its own run when the fault comes
//   slowStartMs   the delay of slow_start (default 5000)
//   gapMs      a pause before each document after the first, as `wtpool` prints documents at
//              least 200 ms apart (W5)
//   aliveMs    the period of alive lines after the documents (default 1000, 0 for none)
//   count      print this many distinct documents as fast as the pipe takes them
//   control    a file whose appended lines steer a running fake: `emit <document JSON>`,
//              `alive`, `exit <code>`
//   actionExit the exit code of `free` and `fg` (phase 3, X12), which only log (default 0)
//   actionStderr  the line `free` and `fg` write to stderr when actionExit is not 0
//   jobExit    the exit code of `job limit`, `job enforce` and `job heavy` (phase 4, X21), which
//              only log and never print a document: a number, or a list taken per `job` call in
//              order, the last repeating (default 0)
//   jobStderr  what a `job` call whose code is not 0 writes to stderr, as given (default one line)
//   jobHangMs  a `job` call waits this long before it answers, so a kill can be seen (default 0)
import { appendFileSync, existsSync, readFileSync, statSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const config = JSON.parse(readFileSync(process.env.WTPOOL_FAKE_CONFIG, 'utf8'));
const args = process.argv.slice(2);

function log(event, extra = {}) {
  appendFileSync(config.log, `${JSON.stringify({ event, pid: process.pid, time: Date.now(), ...extra })}\n`);
}

function parentName() {
  try {
    return execFileSync('ps', ['-o', 'comm=', '-p', String(process.ppid)], { encoding: 'utf8' }).trim();
  } catch {
    return '';
  }
}

function startsSoFar() {
  if (!existsSync(config.log)) return 0;
  return readFileSync(config.log, 'utf8')
    .split('\n')
    .filter((line) => line.includes('"event":"start"')).length;
}

const index = startsSoFar();
// The variables that name a pool, so a check sees whether the extension set them (R9).
const pool = { foreground: process.env.WTPOOL_FOREGROUND ?? null, slotDir: process.env.WTPOOL_SLOT_DIR ?? null };
log('start', { argv: args, script: process.argv[1], cwd: process.cwd(), ppid: process.ppid, parent: parentName(), index, ...pool });
process.on('exit', (code) => log('exit', { code }));
for (const signal of ['SIGTERM', 'SIGINT', 'SIGHUP']) {
  process.on(signal, () => {
    log('signal', { signal });
    process.exit(0);
  });
}

function documents() {
  if (config.documents) return config.documents;
  const dir = config.scenarioDir ?? path.join(here, '..', '..', 'verify', 'scenarios');
  const scenario = JSON.parse(readFileSync(path.join(dir, `${config.scenario}.json`), 'utf8'));
  const docs = scenario.steps.filter((step) => step.expect).map((step) => step.expect);
  docs.push(scenario.expect);
  return JSON.parse(
    JSON.stringify(docs)
      .replaceAll('{common}', '/Users/verify/pool/fg/.git')
      .replaceAll('{pool}', '/Users/verify/pool'),
  );
}

const faults = config.faults ?? ['none'];
const fault = faults[Math.min(index, faults.length - 1)];
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let alive = 0;

function write(text) {
  return new Promise((resolve) => {
    if (!process.stdout.write(text)) process.stdout.once('drain', resolve);
    else resolve();
  });
}

function line(value) {
  return write(`${JSON.stringify(value)}\n`);
}

let ignoreEof = fault === 'ignore_eof';
process.stdin.on('data', () => {});
process.stdin.on('end', () => {
  log('stdin-end');
  if (!ignoreEof) process.exit(0);
});
process.stdin.resume();

async function follow() {
  if (!config.control) return;
  let offset = existsSync(config.control) ? statSync(config.control).size : 0;
  for (;;) {
    await sleep(50);
    if (!existsSync(config.control)) continue;
    const text = readFileSync(config.control, 'utf8');
    if (text.length <= offset) continue;
    const added = text.slice(offset);
    offset = text.length;
    for (const command of added.split('\n').filter(Boolean)) {
      const [verb, ...rest] = command.split(' ');
      if (verb === 'emit') await write(`${rest.join(' ')}\n`);
      if (verb === 'alive') await line({ type: 'alive', seq: (alive += 1) });
      if (verb === 'exit') process.exit(Number(rest[0] ?? 0));
    }
  }
}

async function watch() {
  const docs = documents();
  if (fault === 'slow_start') await sleep(config.slowStartMs ?? 5000);
  if (fault === 'early_exit') {
    await sleep(100);
    process.exit(1);
  }
  if (fault === 'error_exit') {
    await sleep(100);
    await new Promise((resolve) => process.stderr.write(config.stderr ?? 'wtpool: fake error\n', resolve));
    process.exit(config.exitCode ?? 1);
  }
  const [first, ...rest] = docs;
  if (config.lead && ['partial', 'oversize', 'garbage', 'unknown_version'].includes(fault)) await line(first);
  switch (fault) {
    case 'partial': {
      const text = `${JSON.stringify(first)}\n`;
      const third = Math.ceil(text.length / 3);
      for (const part of [text.slice(0, third), text.slice(third, 2 * third), text.slice(2 * third)]) {
        await write(part);
        await sleep(80);
      }
      break;
    }
    case 'oversize': {
      const huge = structuredClone(first);
      huge.machine.name = 'x'.repeat(1024 * 1024 + 16);
      await line(huge);
      break;
    }
    case 'garbage':
      await write('{"type":"status", this is not JSON\n');
      break;
    case 'unknown_version':
      await line({ ...first, version: 2 });
      break;
    default:
      await line(first);
  }
  if (fault === 'stall') {
    await follow();
    return;
  }
  for (const doc of rest) {
    if (config.gapMs) await sleep(config.gapMs);
    await line(doc);
  }
  if (config.count) {
    const base = docs.at(-1);
    for (let number = 1; number <= config.count; number += 1) {
      const next = structuredClone(base);
      next.generated = new Date(Date.parse(base.generated) + number * 1000).toISOString().replace('.000Z', 'Z');
      next.pools[0].skipped_records = number;
      await line(next);
    }
  }
  void follow();
  const period = config.aliveMs ?? 1000;
  while (period > 0) {
    await sleep(period);
    await line({ type: 'alive', seq: (alive += 1) });
  }
}

/** The `job` calls of this fake's log before this one, so `jobExit` can give each its own code. */
function jobCallsBefore() {
  return readFileSync(config.log, 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((text) => JSON.parse(text))
    .filter((entry) => entry.event === 'start' && entry.argv?.[0] === 'job' && entry.pid !== process.pid).length;
}

async function answerJob() {
  // The answer comes after the hang even when the caller closes stdin at once.
  ignoreEof = true;
  const codes = [config.jobExit ?? 0].flat();
  const code = codes[Math.min(jobCallsBefore(), codes.length - 1)];
  if (config.jobHangMs) await sleep(config.jobHangMs);
  if (code === 0) process.exit(0);
  const text = config.jobStderr ?? `wtpool: ${args.join(' ')} failed (fake)\n`;
  process.stderr.write(text, () => process.exit(code));
}

if (args[0] === '--version') {
  process.stdout.write('wtpool 0.0.0-fake\ncommit fake\nfeatures: none\n');
  process.exit(0);
} else if (args[0] === 'status' && args.includes('--watch')) {
  watch();
} else if (args[0] === 'free' || args[0] === 'fg') {
  const code = config.actionExit ?? 0;
  if (code === 0) process.exit(0);
  const text = config.actionStderr ?? `wtpool: ${args.join(' ')} failed (fake)`;
  process.stderr.write(`${text}\n`, () => process.exit(code));
} else if (args[0] === 'job' && ['limit', 'enforce', 'heavy'].includes(args[1])) {
  answerJob();
} else if (args[0] === 'status' && args.includes('--json')) {
  process.stdout.write(`${JSON.stringify(documents().at(-1))}\n`, () => process.exit(0));
} else {
  process.stderr.write(`fake wtpool: unsupported arguments ${JSON.stringify(args)}\n`);
  process.exit(2);
}
