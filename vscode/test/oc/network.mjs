// What the oc layer's OpenCode processes may reach: loopback, and for the real model OpenCode's
// own provider. A proxy every sandbox process is pointed at keeps the host of each connection it is
// asked for and opens only the allowed ones; a sampler of `lsof` sees any connection that went
// around it, and any file the processes held open in the owner's OpenCode folders.
import { execFile } from 'node:child_process';
import http from 'node:http';
import net from 'node:net';

import { descendants, psTable } from '../support/processes.mjs';

/** True for OpenCode's own provider, the one host `oc_real_model` may reach. */
export const providerHost = (host) => host === 'opencode.ai' || host.endsWith('.opencode.ai');

/**
 * Starts the proxy on a free loopback port. `allow(host)` decides each connection; every request
 * lands in `asked` as {at, host, port, allowed}.
 */
export function startProxy() {
  const proxy = { asked: [], allow: () => false, port: 0, session: null };
  const server = http.createServer((req, res) => {
    let host = '';
    try {
      host = new URL(req.url).hostname;
    } catch {
      // A request with no absolute URL.
    }
    proxy.asked.push({ at: Date.now(), session: proxy.session, host, port: 80, allowed: false, method: req.method });
    res.writeHead(403, { 'content-type': 'text/plain' });
    res.end('wtpool-verify: plain HTTP is not allowed\n');
  });
  server.on('connect', (req, socket, head) => {
    const [host, portText] = String(req.url).split(':');
    const port = Number(portText) || 443;
    const allowed = Boolean(proxy.allow(host, port)) && port === 443;
    proxy.asked.push({ at: Date.now(), session: proxy.session, host, port, allowed });
    if (!allowed) {
      socket.end('HTTP/1.1 403 Forbidden\r\n\r\n');
      return;
    }
    const upstream = net.connect(port, host, () => {
      socket.write('HTTP/1.1 200 Connection Established\r\n\r\n');
      if (head?.length) upstream.write(head);
      upstream.pipe(socket);
      socket.pipe(upstream);
    });
    const close = () => {
      upstream.destroy();
      socket.destroy();
    };
    upstream.on('error', close);
    socket.on('error', close);
  });
  proxy.ready = new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      proxy.port = server.address().port;
      resolve(proxy.port);
    });
  });
  proxy.close = () =>
    new Promise((resolve) => {
      server.closeAllConnections?.();
      server.close(() => resolve());
    });
  return proxy;
}

const LOOPBACK = /^(127\.\d+\.\d+\.\d+|\[?::1\]?|localhost)$/;

/** The remote end of an lsof `NAME`, such as `127.0.0.1:5000->1.2.3.4:443 (ESTABLISHED)`. */
export function remoteOf(name) {
  const arrow = name.indexOf('->');
  if (arrow < 0) return null;
  const remote = name.slice(arrow + 2).split(' ')[0];
  const at = remote.lastIndexOf(':');
  return { host: remote.slice(0, at), port: Number(remote.slice(at + 1)) };
}

/**
 * Samples, every second, the network connections and open files of the processes below this
 * runner: `remote` holds each remote end that is not loopback, `owner` each open file below one
 * of `folders`.
 */
export function startSampler(folders) {
  const sample = { remote: [], owner: [], samples: 0, stopped: false };
  const once = () =>
    new Promise((resolve) => {
      const own = new Set(descendants(process.pid));
      const table = psTable().filter((row) => own.has(row.pid) && !/\blsof\b/.test(row.args));
      if (!table.length) {
        resolve();
        return;
      }
      execFile('/usr/sbin/lsof', ['-nP', '-w', '-p', table.map((row) => row.pid).join(',')], { maxBuffer: 32 * 1024 * 1024, timeout: 10_000 }, (error, stdout) => {
        sample.samples += 1;
        for (const line of String(stdout ?? '').split('\n').slice(1)) {
          const columns = line.trim().split(/\s+/);
          if (columns.length < 9) continue;
          const name = columns.slice(8).join(' ');
          const type = columns[4];
          if (type === 'IPv4' || type === 'IPv6') {
            const remote = remoteOf(name);
            if (remote && !LOOPBACK.test(remote.host)) sample.remote.push({ command: columns[0], pid: Number(columns[1]), ...remote });
          } else if (folders.some((folder) => name.startsWith(`${folder}/`) || name === folder)) {
            sample.owner.push({ command: columns[0], pid: Number(columns[1]), name });
          }
        }
        resolve();
      });
    });
  const loop = (async () => {
    while (!sample.stopped) {
      await once();
      await new Promise((resolve) => setTimeout(resolve, 1000));
    }
  })();
  sample.stop = async () => {
    sample.stopped = true;
    await loop;
    await once();
  };
  return sample;
}
