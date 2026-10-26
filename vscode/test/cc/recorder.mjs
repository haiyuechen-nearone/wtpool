#!/usr/bin/env node
// The recorder hook of the cc layer (verification.md, "Contract test"): it appends each hook
// payload Claude Code sends, as read, to WTPOOL_VERIFY_CC_RECORDING, and the event with this hook's
// parent pid and name to WTPOOL_VERIFY_CC_PARENTS. It prints nothing and exits 0.
import { execFileSync } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';

function parentName(pid) {
  try {
    return execFileSync('ps', ['-o', 'comm=', '-p', String(pid)], { encoding: 'utf8' }).trim();
  } catch {
    return '';
  }
}

try {
  const text = readFileSync(0, 'utf8');
  const payload = JSON.parse(text);
  if (process.env.WTPOOL_VERIFY_CC_RECORDING) {
    appendFileSync(process.env.WTPOOL_VERIFY_CC_RECORDING, `${JSON.stringify(payload)}\n`);
  }
  if (process.env.WTPOOL_VERIFY_CC_PARENTS) {
    const line = {
      event: payload.hook_event_name,
      session_id: payload.session_id,
      ppid: process.ppid,
      parent: parentName(process.ppid),
      at: Date.now(),
    };
    appendFileSync(process.env.WTPOOL_VERIFY_CC_PARENTS, `${JSON.stringify(line)}\n`);
  }
} catch {
  // A recorder never fails a session.
}
process.exit(0);
