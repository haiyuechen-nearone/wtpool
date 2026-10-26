// Process helpers of the harness: the process table, a process's descendants, its command name,
// and the sleep every layer waits with.
import { execFileSync } from 'node:child_process';

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Every process now: [{pid, ppid, args}]. */
export function psTable() {
  return execFileSync('ps', ['-axo', 'pid=,ppid=,args='], { encoding: 'utf8' })
    .split('\n')
    .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s*(.*)$/))
    .filter(Boolean)
    .map((match) => ({ pid: Number(match[1]), ppid: Number(match[2]), args: match[3] }));
}

/** The pids of every process below `pid`, children first found first. */
export function descendants(pid) {
  const children = new Map();
  for (const { pid: child, ppid } of psTable()) {
    if (!children.has(ppid)) children.set(ppid, []);
    children.get(ppid).push(child);
  }
  const found = [];
  const stack = [pid];
  while (stack.length) {
    for (const child of children.get(stack.pop()) ?? []) {
      found.push(child);
      stack.push(child);
    }
  }
  return found;
}

/** The command name of process `pid`, or an empty string when it is gone. */
export function commandName(pid) {
  try {
    return execFileSync('ps', ['-o', 'comm=', '-p', String(pid)], { encoding: 'utf8' }).trim();
  } catch {
    return '';
  }
}
