#!/usr/bin/env node
// Records the README's animated GIF of the Worktrees tab, docs/images/demo.gif, from the
// wtpool.vsix that `npm run package` wrote.
// It drives a real VS Code with the packaged extension, as the e2e layer does, in a sandbox: a
// temporary HOME, user data and extensions folder, and the harness's fake wtpool printing the
// documents of demo/story.mjs, never a real pool. It takes screenshots at a steady rate, draws a
// pointer into the window, and encodes the frames with gifski. On macOS the window stays behind
// all other windows and never takes focus (test/support/background.mjs).
//
//   node scripts/demo.mjs [--out <gif>] [--features all|none|<names>] [--width <px>]
//                         [--fps <n>] [--quality <1-100>] [--frames <dir>] [--keyframes <dir>]
//                         [--keep]
//
// `--features` names, comma separated, the later features the story shows: labels, outputs,
// background (demo/features.mjs); by default the ones this checkout has.
//
// `--frames` keeps the screenshots in a new or empty folder and refuses any other, `--keyframes`
// writes a PNG of each step of the story, and `--keep` leaves the sandbox in place. The
// placeholders (repository, machine, paths) and every scripted time are in demo/story.mjs;
// docs/development.md, "Recording the demo GIF", has more.
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';

import { keepAtBack } from '../test/support/background.mjs';
import { REPO } from '../test/support/paths.mjs';
import { installedVSCode } from '../test/support/vscode-app.mjs';
import { close, env, launch, panelFrame, runCommand, sleep } from '../test/e2e/vscode.mjs';
import { clickPointer, installPointer, movePointer, record, shiftClock, timeline } from './demo/capture.mjs';
import { detect, parse } from './demo/features.mjs';
import { configureFake, create, emit, fakeStarts, remove } from './demo/sandbox.mjs';
import { KEYFRAMES, LENGTH, PLACEHOLDERS, clockOf, story } from './demo/story.mjs';

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
    fail('demo: gifski is not installed. Install it with `cargo install --locked gifski` (https://gif.ski), then run again.');
  }
  if (!installedVSCode()) {
    fail('demo: no VS Code found. Install VS Code, or name its binary in WTPOOL_VERIFY_VSCODE (and its CLI in WTPOOL_VERIFY_VSCODE_CLI).');
  }
}

/** Refuses a `--frames` folder that holds anything, so the frames never mix with other files. */
function checkFrames(dir) {
  let names;
  try {
    names = readdirSync(dir);
  } catch (error) {
    if (error.code === 'ENOENT') return;
    fail(`demo: --frames ${dir}: ${error.message}`);
  }
  if (names.length) fail(`demo: --frames ${dir} is not empty: name a new or empty folder`);
}

/** The page point a story target names, in the window's CSS pixels. */
async function pointOf(target, { window, frame, pathOf }) {
  if (target === 'rest') return { x: WINDOW.width * 0.72, y: WINDOW.height * 0.86 };
  const [kind, name, action] = target.split(':');
  if (kind === 'dialog') {
    const button = window.locator('.monaco-dialog-box .monaco-button').filter({ hasText: new RegExp(`^${name}$`) });
    await button.first().waitFor({ state: 'visible', timeout: 5_000 });
    return centre(await button.first().boundingBox(), 0.5);
  }
  const row = frame.locator(`tr[data-row-id="${pathOf(name)}"]`);
  if (kind === 'action') {
    const button = row.locator(`button[data-action="${action}"]`);
    await button.waitFor({ state: 'visible', timeout: 5_000 });
    return centre(await button.boundingBox(), 0.5);
  }
  // A row: the start of its Branch cell, clear of the row actions.
  const cell = row.locator('td.branch');
  return centre(await cell.boundingBox(), 0.3);
}

const centre = (box, across) => ({ x: box.x + box.width * across, y: box.y + box.height / 2 });

/** Runs the story's steps in real time from `origin`, the recording's start. */
async function play(steps, context) {
  const { window, dirs, origin } = context;
  let pointer = await pointOf('rest', context);
  for (const step of steps) {
    const wait = origin + step.at * 1000 - Date.now();
    if (wait > 0) await sleep(wait);
    if (step.emit) emit(dirs, step.emit);
    if (step.log !== undefined) writeFileSync(context.logPath, `${step.log}\n`, { flag: 'a' });
    if (step.to) {
      pointer = await movePointer(window, pointer, await pointOf(step.to, context), step.ms);
      if (step.click) await clickPointer(window, pointer);
    }
    if (step.waitFor) {
      const deadline = Date.now() + 5_000;
      const wanted = JSON.stringify(step.waitFor);
      while (!fakeStarts(dirs).some((argv) => JSON.stringify(argv) === wanted)) {
        if (Date.now() > deadline) throw new Error(`demo: the extension never ran wtpool ${step.waitFor.join(' ')}`);
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
  // The time zone the harness runs VS Code in, so `session HH:MM` and the strip's times agree.
  const timeZone = env(dirs, undefined).TZ;
  const scripted = story({ features, timeZone, logPath });
  configureFake(dirs, scripted.first);
  // On macOS the window never takes focus (launch) and, from the moment it opens, sits behind
  // every other window, so it never shows on top of the owner's windows while it records.
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
    if (!frame) throw new Error('demo: Worktrees: Open Status showed no tab with rows');
    // Let the window settle; the clock is shifted before the first frame and a tick after it.
    await sleep(1500);
    const origin = Date.now() + 1500;
    await shiftClock(frame, origin, clockOf(timeZone).startMs);
    const context = { window, frame, dirs, origin, logPath, pathOf: scripted.pathOf };
    await installPointer(window, await pointOf('rest', context));
    await sleep(origin - Date.now());
    const recorder = record(window, framesDir, { fps: Number(args.fps) * 1.5, origin });
    try {
      await play(scripted.steps, context);
      await sleep(Math.max(0, origin + LENGTH * 1000 - Date.now()));
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
  const picked = timeline(frames, fps, LENGTH * 1000);
  process.stdout.write(`demo: ${frames.length} screenshots, ${picked.length} GIF frames at ${fps} fps\n`);
  if (args.keyframes) {
    mkdirSync(args.keyframes, { recursive: true });
    KEYFRAMES.forEach(({ at, name }, index) => {
      const frame = picked[Math.min(picked.length - 1, Math.round(at * fps))];
      copyFileSync(frame.file, path.join(args.keyframes, `${index + 1}-${name}.png`));
    });
  }
  mkdirSync(path.dirname(args.out), { recursive: true });
  const options = ['--quiet', '--no-sort', '--fps', String(fps), '--width', args.width, '--quality', args.quality];
  const encoded = spawnSync('gifski', [...options, '--output', args.out, ...picked.map((f) => f.file)], { stdio: 'inherit' });
  if (encoded.status !== 0) throw new Error('demo: gifski failed');
  const { size } = statSync(args.out);
  process.stdout.write(`demo: wrote ${args.out}, ${(size / 1024 / 1024).toFixed(2)} MB\n`);
}

async function main() {
  if (args.frames) checkFrames(args.frames);
  checkTools();
  const features = parse(args.features, detect());
  process.stdout.write(`demo: features ${JSON.stringify(features)}\n`);
  const dirs = create(PLACEHOLDERS.repository.split('/').at(-1));
  try {
    const framesDir = args.frames ?? path.join(dirs.root, 'frames');
    mkdirSync(framesDir, { recursive: true });
    encode(await recordStory(dirs, features, framesDir));
  } finally {
    if (args.keep) process.stdout.write(`demo: sandbox kept in ${dirs.root}\n`);
    else remove(dirs);
  }
}

try {
  await main();
} catch (error) {
  fail(error instanceof Error ? error.message : String(error));
}
