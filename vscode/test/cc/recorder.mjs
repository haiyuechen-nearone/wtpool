#!/usr/bin/env node
// The Claude Code hook of the cc layer: it records each payload and its own parent process, prints
// nothing and exits 0.
import { appendFileSync, readFileSync } from 'node:fs';

import { commandName } from '../support/processes.mjs';

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
      parent: commandName(process.ppid),
      at: Date.now(),
    };
    appendFileSync(process.env.WTPOOL_VERIFY_CC_PARENTS, `${JSON.stringify(line)}\n`);
  }
} catch {
  // A recorder never fails a session.
}
process.exit(0);
