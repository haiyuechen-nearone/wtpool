// Shared steps of the flow checks (verification.md, "User flows"): each flow starts a VS Code
// with the .vsix on its own sandbox pool and drives it through the UI.
import assert from 'node:assert/strict';
import { existsSync, readFileSync, rmSync } from 'node:fs';
import path from 'node:path';

import { scenarioDocuments } from '../support/docs.mjs';
import { expectedRows, waitForRows } from './rows.mjs';
import { close, launch, panelFrame, pasteboardCount, pool, runCommand, sandbox, sleep, writeClipboard } from './vscode.mjs';

/**
 * Starts VS Code for a flow on a fresh pool of `options.scenario` (default idle_pool) and
 * returns the session plus `pool` and `scenario`. `options.prepare(pool)` runs before VS Code.
 */
export async function flowSession(key, options = {}) {
  const poolInfo = pool(options.scenario ?? 'idle_pool', key);
  await options.prepare?.(poolInfo);
  const dirs = sandbox(key.replaceAll('/', '-'), options);
  const session = await launch(dirs, poolInfo, options.args ?? []);
  return { ...session, pool: poolInfo, scenario: poolInfo.scenario };
}

/** Runs `body(session)` and closes the session, whatever happens. */
export async function withFlow(key, options, body) {
  const session = await flowSession(key, options);
  try {
    return await body(session);
  } finally {
    await close(session);
  }
}

/** The path of a worktree of the session's pool, as `wtpool` reports it. */
export function worktreePath(session, name) {
  return path.join(session.pool.dir, name);
}

/** The row of a worktree in the panel frame. */
export function rowOf(frame, session, name) {
  return frame.locator(`tr[data-row-id="${worktreePath(session, name)}"]`);
}

/** The expected rows of the session's scenario, its final document. */
export function scenarioRows(session, scenario = session.scenario) {
  return expectedRows(scenarioDocuments(scenario).documents.at(-1), session.pool);
}

/**
 * Opens the tab through the Command Palette and waits until it shows the scenario's rows.
 * Returns the frame of the webview.
 */
export async function openStatus(session, window = session.window) {
  await runCommand(window, 'Worktrees: Open Status');
  const frame = await panelFrame(window);
  assert.ok(frame, 'Worktrees: Open Status showed no tab with rows');
  const expected = scenarioRows(session);
  const seen = await waitForRows(frame, expected, 15_000);
  assert.deepEqual(seen, expected, `the tab does not show ${session.scenario}`);
  return frame;
}

/** Which rows the detail strips follow, in DOM order. */
export async function detailOwners(frame) {
  return frame.locator('tr.detail').evaluateAll((strips) =>
    strips.map((strip) => strip.previousElementSibling?.dataset?.rowId ?? null),
  );
}

/** The detail strip's text, spaces collapsed, or null without one. */
export async function detailText(frame) {
  const strips = frame.locator('tr.detail');
  if (!(await strips.count())) return null;
  return (await strips.first().textContent()).replace(/\s+/g, ' ').trim();
}

/** Waits in real time until `check` returns a truthy value; returns it, or the last value. */
export async function poll(check, timeoutMs, stepMs = 200) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    let value;
    try {
      value = await check();
    } catch {
      value = undefined;
    }
    if (value || Date.now() >= deadline) return value;
    await new Promise((resolve) => setTimeout(resolve, stepMs));
  }
}

/**
 * Runs `body` with VS Code's private clipboard holding a marker, so only a copy made in `body`
 * replaces it, and returns what `body` returns. The system pasteboard must not change meanwhile
 * (verification.md, "No side effects"): a change fails the check as a setup error, since either
 * a copy bypassed the private clipboard or the owner copied in these few seconds.
 */
export async function withClipboard(app, body) {
  const before = pasteboardCount();
  await writeClipboard(app, 'wtpool-verify clipboard marker');
  const result = await body();
  const after = pasteboardCount();
  if (before !== after) {
    throw new Error(`setup: the system pasteboard changed while the check copied (change count ${before} to ${after}): a copy reached it past VS Code's private clipboard, or you copied meanwhile`);
  }
  return result;
}

/** Hovers a row and clicks one of its row actions. */
export async function clickAction(frame, session, name, action) {
  const row = rowOf(frame, session, name);
  await row.hover();
  const button = row.locator(`button[data-action="${action}"]`);
  assert.equal(await button.count(), 1, `${name} has no ${action} button`);
  const clicked = await button.click({ timeout: 5_000 }).then(() => true, () => false);
  assert.ok(clicked, `${name}'s ${action} button cannot be clicked on hover`);
}

/**
 * Types `pwd -P` into the active terminal and returns the folder it printed, or undefined. The
 * terminal gets focus by a click, or with `keyboard` through the Command Palette.
 */
export async function terminalFolder(window, dirs, { keyboard = false } = {}) {
  const out = path.join(dirs.root, 'tmp', `pwd-${Date.now()}.txt`);
  rmSync(out, { force: true });
  const screen = window.locator('.terminal-wrapper.active .xterm-screen, .terminal-wrapper .xterm-screen').first();
  // No terminal is the product's failure, which the caller asserts.
  if (!(await screen.waitFor({ timeout: 15_000 }).then(() => true, () => false))) return undefined;
  await sleep(1500);
  if (keyboard) await runCommand(window, 'Terminal: Focus Terminal', { keyboardOnly: true });
  else await screen.click();
  await window.keyboard.type(`pwd -P > '${out}'`);
  await window.keyboard.press('Enter');
  const text = await poll(() => (existsSync(out) ? readFileSync(out, 'utf8').trim() : undefined), 10_000);
  return text || undefined;
}
