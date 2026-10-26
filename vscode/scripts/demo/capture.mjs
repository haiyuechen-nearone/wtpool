import { writeFile } from 'node:fs/promises';
import path from 'node:path';

import { sleep } from '../../test/e2e/vscode.mjs';

/**
 * Draws a pointer, an arrow with a dark rim, and the ring a click shows, at `start`, over the
 * workbench, as screenshots show no OS cursor.
 */
export async function installPointer(window, start) {
  await window.evaluate(({ x, y }) => {
    const layer = document.createElement('div');
    layer.id = 'wtpool-demo-pointer';
    layer.style.cssText = `position:fixed;left:0;top:0;z-index:2147483647;pointer-events:none;transform:translate(${x}px,${y}px)`;
    const ring = document.createElement('div');
    ring.id = 'wtpool-demo-ring';
    ring.style.cssText =
      'position:absolute;left:-18px;top:-18px;width:36px;height:36px;border-radius:50%;' +
      'border:3px solid gold;box-sizing:border-box;opacity:0';
    const rim = document.createElement('div');
    rim.style.cssText =
      'position:absolute;left:0;top:0;filter:drop-shadow(0 0 1px black) drop-shadow(0 1px 1.5px black)';
    const arrow = document.createElement('div');
    arrow.style.cssText =
      'width:15px;height:23px;background:white;' +
      'clip-path:polygon(0 0,0 19px,4.6px 14.8px,8.2px 22.5px,11px 21.2px,7.5px 13.8px,13.6px 13.8px)';
    rim.append(arrow);
    layer.append(ring, rim);
    document.body.append(layer);
  }, start);
  await window.mouse.move(start.x, start.y);
}

async function placePointer(window, x, y) {
  await window.evaluate(
    ({ x: left, y: top }) => {
      const layer = document.getElementById('wtpool-demo-pointer');
      if (layer) {
        layer.style.transform = `translate(${left}px,${top}px)`;
      }
    },
    { x, y },
  );
  await window.mouse.move(x, y);
}

function easeInOutQuad(t) {
  return t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
}

/** Moves the pointer and the mouse from `from` to `to` over `ms`, and returns `to`. */
export async function movePointer(window, from, to, ms) {
  const begin = Date.now();
  for (;;) {
    const t = ms > 0 ? Math.min(1, (Date.now() - begin) / ms) : 1;
    const k = easeInOutQuad(t);
    await placePointer(window, from.x + (to.x - from.x) * k, from.y + (to.y - from.y) * k);
    if (t >= 1) {
      return to;
    }
    await sleep(12);
  }
}

/** Clicks where the pointer is, with the ring growing and fading around the tip. */
export async function clickPointer(window, at) {
  await window.evaluate(() => {
    document.getElementById('wtpool-demo-ring')?.animate(
      [
        { opacity: 0.95, transform: 'scale(0.35)' },
        { opacity: 0, transform: 'scale(1.25)' },
      ],
      { duration: 520, easing: 'ease-out' },
    );
  });
  await window.mouse.move(at.x, at.y);
  await window.mouse.down();
  await sleep(60);
  await window.mouse.up();
}

/**
 * Shifts the webview's clock so that the real time `realMs` reads as `storyMs`, and the tab's ages
 * and the detail strip's times follow the scripted times. The webview reads the time only through
 * `Date.now()`.
 */
export async function shiftClock(frame, realMs, storyMs) {
  await frame.evaluate((shift) => {
    const real = Date.now.bind(Date);
    Date.now = () => real() + shift;
  }, storyMs - realMs);
}

/**
 * Takes screenshots of the window back to back, at most `fps` a second, into `dir`, until
 * `stop`. Each frame keeps the time it shows, in ms from `origin`: the middle of its capture.
 */
export function record(window, dir, { fps, origin }) {
  const frames = [];
  const writes = [];
  let running = true;
  const loop = (async () => {
    const gap = 1000 / fps;
    while (running) {
      const begin = Date.now();
      const png = await window.screenshot({ scale: 'device', caret: 'hide' });
      const shown = (begin + Date.now()) / 2 - origin;
      const file = path.join(dir, `frame-${String(frames.length).padStart(5, '0')}.png`);
      frames.push({ file, ms: shown });
      writes.push(writeFile(file, png));
      await sleep(Math.max(0, gap - (Date.now() - begin)));
    }
  })();
  return {
    frames,
    async stop() {
      running = false;
      await loop;
      await Promise.all(writes);
    },
  };
}

/** Picks the frame on screen at each tick of `fps` from 0 to `lengthMs`: the last one shown by then. */
export function timeline(frames, fps, lengthMs) {
  const tickMs = 1000 / fps;
  const picked = [];
  let index = 0;
  for (let tick = 0; tick * tickMs <= lengthMs; tick += 1) {
    const ms = tick * tickMs;
    while (index + 1 < frames.length && frames[index + 1].ms <= ms) {
      index += 1;
    }
    picked.push(frames[index]);
  }
  return picked;
}
