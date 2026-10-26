import { mkdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { test } from '@playwright/test';

import { git as gitFacts, worktree } from '../support/docs.mjs';
import { runDir } from '../support/paths.mjs';
import { git, sleep } from './vscode.mjs';

/**
 * A repository whose main worktree is `<pool>/<name>`, with slots `<name>-wt1` to
 * `<name>-wt<slots>` beside it, detached and clean, as wtpool's defaults find them from the
 * repository alone: no wtpool.toml, no remote, `wtpool-activity/` with the sandbox marker. Returns
 * {scenario: null, dir, fg, common}, as `pool` does.
 */
export function repositoryPool(key, { name = 'app', slots = 3 } = {}) {
  const repeat = test.info().repeatEachIndex;
  const dir = path.join(runDir(), 'e2e', 'pools', `${key.replaceAll('/', '-')}${repeat ? `-repeat${repeat}` : ''}`);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const real = realpathSync(dir);
  const fg = path.join(real, name);
  git(real, ['init', '--quiet', '-b', 'main', fg]);
  for (const [setting, value] of [['commit.gpgsign', 'false'], ['advice.detachedHead', 'false']]) git(fg, ['config', setting, value]);
  writeFileSync(path.join(fg, 'f.txt'), 'base\n');
  git(fg, ['add', 'f.txt']);
  git(fg, ['commit', '--quiet', '-m', 'base']);
  for (let slot = 1; slot <= slots; slot += 1) {
    git(fg, ['worktree', 'add', '--quiet', '--detach', path.join(real, `${name}-wt${slot}`), 'main']);
  }
  const common = path.join(fg, '.git');
  mkdirSync(path.join(common, 'wtpool-activity'), { recursive: true });
  writeFileSync(path.join(common, 'wtpool-activity', '.verify-sandbox'), '');
  return { scenario: null, dir: real, fg, common };
}

/** The document `wtpool status` must print for a {@link repositoryPool}: its foreground and free slots. */
export function repositoryDocument(poolInfo, { name = 'app', slots = 3 } = {}) {
  const clean = gitFacts();
  const worktrees = [{ ...worktree('fg'), path: poolInfo.fg, name, upstream: null, git: clean }];
  for (let slot = 1; slot <= slots; slot += 1) {
    worktrees.push({ ...worktree('wt1'), path: path.join(poolInfo.dir, `${name}-wt${slot}`), name: `${name}-wt${slot}` });
  }
  return { pools: [{ repository: { name, github: null }, worktrees }] };
}

const collapse = (text) => text.replace(/\s+/g, ' ').trim();

/**
 * Waits for a frame of `window` whose text `matches` accepts: {frame, text}, or {texts} of every
 * frame last seen when none does within `timeoutMs`.
 */
export async function frameWithText(window, matches, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  let texts = [];
  for (;;) {
    texts = [];
    for (const frame of window.frames()) {
      const text = await frame.evaluate(() => document.body?.innerText ?? '').catch(() => '');
      if (matches(collapse(text))) return { frame, text: collapse(text) };
      if (text.trim()) texts.push(collapse(text).slice(0, 300));
    }
    if (Date.now() >= deadline) return { texts };
    await sleep(250);
  }
}

/**
 * Makes the webview of `frame` `width` px wide, or as narrow as the window allows, by resizing
 * the window, and returns the width it got.
 */
export async function sizeWebview(session, frame, width) {
  let got = await frame.evaluate(() => window.innerWidth);
  for (let attempt = 0; attempt < 4 && Math.abs(got - width) > 2; attempt += 1) {
    await session.app.evaluate(({ BrowserWindow }, extra) => {
      const [win] = BrowserWindow.getAllWindows();
      const [w, h] = win.getContentSize();
      win.setContentSize(Math.max(1, w + extra), h);
    }, width - got);
    await sleep(1000);
    const next = await frame.evaluate(() => window.innerWidth);
    if (next === got) break;
    got = next;
  }
  return got;
}

/**
 * The layout of `needle` in the frame: the smallest element whose text holds it, its line boxes,
 * the boxes that cut it, its width on one line, and the frame's width. Null when no element
 * holds it.
 */
export async function textLayout(frame, needle) {
  return frame.evaluate((wanted) => {
    const squash = (text) => text.replace(/\s+/g, ' ').trim();
    let found = null;
    for (const el of document.body.querySelectorAll('*')) {
      if (squash(el.textContent ?? '').includes(wanted) && (!found || found.contains(el))) found = el;
    }
    if (!found) return null;
    const range = document.createRange();
    range.selectNodeContents(found);
    const boxes = [...range.getClientRects()].filter((r) => r.width > 0.5 && r.height > 0.5);
    const cuts = [];
    for (let el = found; el && el !== document.documentElement; el = el.parentElement) {
      const style = getComputedStyle(el);
      const name = `${el.tagName.toLowerCase()}${el.className ? `.${String(el.className).trim().split(/\s+/).join('.')}` : ''}`;
      if (style.overflowX !== 'visible' && el.scrollWidth > el.clientWidth + 1) cuts.push(`${name} is ${el.scrollWidth - el.clientWidth} px too narrow`);
      if (style.textOverflow === 'ellipsis' && style.overflowX !== 'visible') cuts.push(`${name} cuts with an ellipsis`);
    }
    const canvas = document.createElement('canvas').getContext('2d');
    canvas.font = getComputedStyle(found).font;
    return {
      shown: found.checkVisibility({ opacityProperty: true, visibilityProperty: true }),
      lines: new Set(boxes.map((r) => Math.round(r.top))).size,
      outside: boxes.filter((r) => r.left < -1 || r.right > window.innerWidth + 1).map((r) => [Math.round(r.left), Math.round(r.right)]),
      cuts,
      oneLine: Math.ceil(canvas.measureText(wanted).width),
      width: window.innerWidth,
      pageScroll: (document.scrollingElement ?? document.documentElement).scrollWidth - window.innerWidth,
    };
  }, needle);
}
