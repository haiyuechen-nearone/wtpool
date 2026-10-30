#!/usr/bin/env node
/**
 * @fileoverview Records docs/images/demo.gif, the README's animation of the Worktrees tab, from
 * the wtpool.vsix of `npm run package`. A real VS Code plays demo/story.mjs in a sandbox, with
 * the fake wtpool and never a real pool, and gifski encodes its screenshots.
 *
 *     node scripts/demo.mjs [--out <gif>] [--features all|none|<names>] [--width <px>]
 *                           [--fps <n>] [--quality <1-100>] [--frames <dir>] [--keyframes <dir>]
 *                           [--keep]
 *
 * docs/development.md, "Recording the demo GIF", describes the flags.
 */
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';

import { keepAtBack } from '../test/support/background.mjs';
import { REPO } from '../test/support/paths.mjs';
import { installedVSCode } from '../test/support/vscode-app.mjs';
import { close, env, launch, panelFrame, runCommand, sleep } from '../test/e2e/vscode.mjs';
import {
  clickPointer,
  focusPointer,
  installPointer,
  movePointer,
  record,
  shiftClock,
  timeline,
} from './demo/capture.mjs';
import { detectFeatures, parseFeatures } from './demo/features.mjs';
import { configureFake, createSandbox, emit, fakeStarts, removeSandbox } from './demo/sandbox.mjs';
import { KEYFRAMES, LENGTH_SECONDS, PLACEHOLDERS, clockOf, story } from './demo/story.mjs';

/** The window's content size in CSS pixels, and the pixel ratio the frames are taken at. */
const WINDOW = { width: 1000, height: 600, scale: 2 };

const { values: args } = parseArgs({
  options: {
    out: { type: 'string', default: path.join(REPO, 'docs', 'images', 'demo.gif') },
    features: { type: 'string' },
    width: { type: 'string', default: String(WINDOW.width) },
    fps: { type: 'string', default: '10' },
    quality: { type: 'string', default: '90' },
    frames: { type: 'string' },
    keyframes: { type: 'string' },
    keep: { type: 'boolean', default: false },
  },
});

function fail(message) {
  process.stderr.write(`${message}\n`);
  process.exit(1);
}

function checkTools() {
  const gifski = spawnSync('gifski', ['--version'], { encoding: 'utf8' });
  if (gifski.error) {
    fail(
      'demo: gifski is not installed. Install it with `cargo install --locked gifski` (https://gif.ski), then run again.',
    );
  }
  if (!installedVSCode()) {
    fail(
      'demo: no VS Code found. Install VS Code, or name its binary in WTPOOL_VERIFY_VSCODE (and its CLI in WTPOOL_VERIFY_VSCODE_CLI).',
    );
  }
}

/** Refuses a `--frames` folder that holds anything, so the frames never mix with other files. */
function checkFrames(dir) {
  let names = [];
  try {
    names = readdirSync(dir);
  } catch (error) {
    if (error.code !== 'ENOENT') {
      fail(`demo: --frames ${dir}: ${error.message}`);
    }
  }
  if (names.length > 0) {
    fail(`demo: --frames ${dir} is not empty: name a new or empty folder`);
  }
}

/** Returns the page point a story target names, in the window's CSS pixels. */
async function pointOf(target, { window, frame, pathOf }) {
  if (target === 'rest') {
    return { x: WINDOW.width * 0.72, y: WINDOW.height * 0.86 };
  }
  if (target === 'machine') {
    const button = frame.locator('button.machine-button');
    await button.waitFor({ state: 'visible', timeout: 5_000 });
    return pointAcross(await button.boundingBox(), 0.5);
  }
  const [kind, name, action] = target.split(':');
  if (kind === 'mark') {
    const mark = frame.locator(`tr[data-row-id="${pathOf(name)}"] .line-mark`).first();
    await mark.waitFor({ state: 'visible', timeout: 5_000 });
    return pointAcross(await mark.boundingBox(), 0.5);
  }
  if (kind === 'dialog') {
    const button = window
      .locator('.monaco-dialog-box .monaco-button')
      .filter({ hasText: new RegExp(`^${name}$`) });
    await button.first().waitFor({ state: 'visible', timeout: 5_000 });
    return pointAcross(await button.first().boundingBox(), 0.5);
  }
  const row = frame.locator(`tr[data-row-id="${pathOf(name)}"]`);
  if (kind === 'action') {
    const button = row.locator(`button[data-action="${action}"]`);
    await button.waitFor({ state: 'visible', timeout: 5_000 });
    return pointAcross(await button.boundingBox(), 0.5);
  }
  // A row: the start of its Branch cell, clear of the row actions.
  const cell = row.locator('td.branch');
  return pointAcross(await cell.boundingBox(), 0.3);
}

/** Returns the point `across` the width of `box`, halfway down. */
function pointAcross(box, across) {
  return { x: box.x + box.width * across, y: box.y + box.height / 2 };
}

/** Runs the story's steps in real time from `origin`, the recording's start. */
async function play(steps, context) {
  const { window, dirs, origin } = context;
  let pointer = await pointOf('rest', context);
  for (const step of steps) {
    const wait = origin + step.at * 1000 - Date.now();
    if (wait > 0) {
      await sleep(wait);
    }
    if (step.emit) {
      emit(dirs, step.emit);
    }
    if (step.log !== undefined) {
      writeFileSync(context.logPath, `${step.log}\n`, { flag: 'a' });
    }
    if (step.to) {
      await focusPointer(window, false);
      pointer = await movePointer(window, pointer, await pointOf(step.to, context), step.ms);
      if (step.focus) {
        await focusPointer(window, true);
      }
      if (step.click) {
        await clickPointer(window, pointer);
      }
    }
    if (step.waitFor) {
      const deadline = Date.now() + 5_000;
      // The extension may add flags, such as `--force`, after the words the story names.
      const wanted = step.waitFor;
      while (
        !fakeStarts(dirs).some((argv) => wanted.every((word, index) => argv[index] === word))
      ) {
        if (Date.now() > deadline) {
          throw new Error(`demo: the extension never ran wtpool ${step.waitFor.join(' ')}`);
        }
        await sleep(50);
      }
    }
  }
}

/**
 * Starts VS Code in the sandbox, opens the tab, plays the story and returns the screenshots:
 * [{file, ms}], `ms` from the recording's start.
 */
async function recordStory(dirs, features, framesDir) {
  const logPath = path.join(dirs.root, 'tasks', 'b7k2q9.output');
  mkdirSync(path.dirname(logPath), { recursive: true });
  writeFileSync(logPath, '');
  // The time zone VS Code runs in, so the story's clock times match the tab's.
  const timeZone = env(dirs, undefined).TZ;
  const scripted = story({ features, timeZone, logPath });
  configureFake(dirs, scripted.first);
  keepAtBack();
  const session = await launch(dirs, undefined, [`--force-device-scale-factor=${WINDOW.scale}`]);
  try {
    const { app, window } = session;
    await app.evaluate(({ BrowserWindow }, size) => {
      const [win] = BrowserWindow.getAllWindows();
      win.setContentSize(size.width, size.height);
      win.center();
    }, WINDOW);
    await runCommand(window, 'View: Close Primary Side Bar');
    await runCommand(window, 'Worktrees: Open Status');
    const frame = await panelFrame(window);
    if (!frame) {
      throw new Error('demo: Worktrees: Open Status showed no tab with rows');
    }
    await sleep(1500);
    // The first frame comes after the clock shift and the webview's next tick.
    const origin = Date.now() + 1500;
    await shiftClock(frame, origin, clockOf(timeZone).startMs);
    const context = { window, frame, dirs, origin, logPath, pathOf: scripted.pathOf };
    await installPointer(window, await pointOf('rest', context));
    await sleep(origin - Date.now());
    const recorder = record(window, framesDir, { fps: Number(args.fps) * 1.5, origin });
    try {
      await play(scripted.steps, context);
      await sleep(Math.max(0, origin + LENGTH_SECONDS * 1000 - Date.now()));
    } finally {
      await recorder.stop();
    }
    return recorder.frames;
  } finally {
    await close(session);
  }
}

/** Encodes the frame on screen at each tick into the GIF, and copies the key frames. */
function encode(frames) {
  const fps = Number(args.fps);
  const picked = timeline(frames, fps, LENGTH_SECONDS * 1000);
  process.stdout.write(
    `demo: ${frames.length} screenshots, ${picked.length} GIF frames at ${fps} fps\n`,
  );
  if (args.keyframes) {
    mkdirSync(args.keyframes, { recursive: true });
    KEYFRAMES.forEach(({ at, name }, index) => {
      const frame = picked[Math.min(picked.length - 1, Math.round(at * fps))];
      copyFileSync(frame.file, path.join(args.keyframes, `${index + 1}-${name}.png`));
    });
  }
  mkdirSync(path.dirname(args.out), { recursive: true });
  const options = [
    '--quiet',
    '--no-sort',
    '--fps',
    String(fps),
    '--width',
    args.width,
    '--quality',
    args.quality,
  ];
  const files = picked.map((frame) => frame.file);
  const encoded = spawnSync('gifski', [...options, '--output', args.out, ...files], {
    stdio: 'inherit',
  });
  if (encoded.status !== 0) {
    throw new Error('demo: gifski failed');
  }
  const { size } = statSync(args.out);
  process.stdout.write(`demo: wrote ${args.out}, ${(size / 1024 / 1024).toFixed(2)} MB\n`);
}

async function main() {
  if (args.frames) {
    checkFrames(args.frames);
  }
  checkTools();
  const features = parseFeatures(args.features, detectFeatures());
  process.stdout.write(`demo: features ${JSON.stringify(features)}\n`);
  const dirs = createSandbox(PLACEHOLDERS.repository.split('/').at(-1));
  try {
    const framesDir = args.frames ?? path.join(dirs.root, 'frames');
    mkdirSync(framesDir, { recursive: true });
    encode(await recordStory(dirs, features, framesDir));
  } finally {
    if (args.keep) {
      process.stdout.write(`demo: sandbox kept in ${dirs.root}\n`);
    } else {
      removeSandbox(dirs);
    }
  }
}

try {
  await main();
} catch (error) {
  fail(error.message);
}
