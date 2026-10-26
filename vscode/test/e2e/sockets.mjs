import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, readlinkSync } from 'node:fs';

/**
 * The listening TCP and the bound UDP sockets of `pids`, one line each: `<pid> <protocol>
 * <local address>`. Node has no API that lists another process's sockets, and each OS exposes
 * them through its own interface, so this reads them per platform with the tools the OS ships:
 * Linux's /proc, macOS's lsof, Windows's netstat. Another platform is a setup error, never an empty
 * list.
 */
export function listening(pids) {
  if (!pids.length) return [];
  if (process.platform === 'linux') return procSockets(pids);
  if (process.platform === 'darwin') return lsofSockets(pids);
  if (process.platform === 'win32') return netstatSockets(pids);
  throw new Error(`setup: no way to list the sockets of a process on ${process.platform}`);
}

/** Linux: the socket inodes of each process's open files, matched in /proc/net/{tcp,udp}{,6}. */
function procSockets(pids) {
  const owner = new Map();
  for (const pid of pids) {
    let fds = [];
    try {
      fds = readdirSync(`/proc/${pid}/fd`);
    } catch {
      continue; // The process ended.
    }
    for (const fd of fds) {
      try {
        const inode = /^socket:\[(\d+)\]$/.exec(readlinkSync(`/proc/${pid}/fd/${fd}`))?.[1];
        if (inode) owner.set(inode, pid);
      } catch {
        // The file closed meanwhile.
      }
    }
  }
  const found = [];
  for (const [table, protocol] of [['tcp', 'TCP'], ['tcp6', 'TCP'], ['udp', 'UDP'], ['udp6', 'UDP']]) {
    let text = '';
    try {
      text = readFileSync(`/proc/net/${table}`, 'utf8');
    } catch {
      continue; // No IPv6, say.
    }
    for (const line of text.split('\n').slice(1)) {
      const fields = line.trim().split(/\s+/);
      // Local address, state and inode. TCP state 0A is LISTEN, and a bound UDP socket counts in
      // any state.
      const [local, state, inode] = [fields[1], fields[3], fields[9]];
      if (!inode || !owner.has(inode) || (protocol === 'TCP' && state !== '0A')) continue;
      found.push(`${owner.get(inode)} ${protocol} ${local}`);
    }
  }
  return found;
}

/** macOS: lsof, shipped with the OS. It exits 1 when it finds nothing. */
function lsofSockets(pids) {
  const run = (args) => {
    try {
      return execFileSync('lsof', ['-nP', '-a', '-p', pids.join(','), ...args], { encoding: 'utf8' });
    } catch (error) {
      if (error.code === 'ENOENT') throw new Error('setup: macOS ships lsof, but it is not on PATH');
      return String(error.stdout ?? '');
    }
  };
  const lines = (text) => text.split('\n').slice(1).filter(Boolean);
  // COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME: the PID, the NODE (TCP or UDP), the NAME.
  return [...lines(run(['-iTCP', '-sTCP:LISTEN'])), ...lines(run(['-iUDP']))].map((line) => {
    const fields = line.split(/\s+/);
    return `${fields[1]} ${fields[7]} ${fields.slice(8).join(' ')}`;
  });
}

/** Windows: `netstat -ano`, shipped with the OS, in columns protocol, local address, state, PID. */
function netstatSockets(pids) {
  const wanted = new Set(pids.map(String));
  const text = execFileSync('netstat', ['-ano'], { encoding: 'utf8' });
  const found = [];
  for (const line of text.split(/\r?\n/)) {
    const fields = line.trim().split(/\s+/);
    const protocol = fields[0];
    if (protocol === 'TCP' && fields[3] === 'LISTENING' && wanted.has(fields[4])) found.push(`${fields[4]} TCP ${fields[1]}`);
    if (protocol === 'UDP' && wanted.has(fields[3])) found.push(`${fields[3]} UDP ${fields[1]}`);
  }
  return found;
}
