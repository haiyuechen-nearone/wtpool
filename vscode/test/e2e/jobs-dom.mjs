// The DOM probes of the heavy job flows: the heading, the machine strip, the job lists and the focus.
import assert from 'node:assert/strict';

import { poll, worktreePath } from './flows.mjs';
import { shownRows } from './rows.mjs';
import { panelFrame, runCommand } from './vscode.mjs';

/** Opens the tab and returns its frame, without comparing rows: jobs change them. */
export async function openTab(session) {
  await runCommand(session.window, 'Worktrees: Open Status');
  const frame = await panelFrame(session.window);
  assert.ok(frame, 'Worktrees: Open Status showed no tab with rows');
  return frame;
}

const squash = (text) => (text ?? '').replace(/\s+/g, ' ').trim();

/** The heading's machine button, asserted present. */
export async function machineButton(frame) {
  const button = frame.locator('tr.pool-heading button.machine-button');
  assert.equal(await button.count(), 1, 'the heading holds no button.machine-button (U10)');
  return button;
}

/** The heading row's text as shown, spaces collapsed. */
export async function headingText(frame) {
  return squash(await frame.locator('tr.pool-heading').first().innerText());
}

/** The machine strip's text as shown, or null when it is closed. */
export async function stripText(frame) {
  const strip = frame.locator('tr.machine-detail');
  if (!(await strip.count())) return null;
  return squash(await strip.first().innerText());
}

/** Opens the strip by a click on the machine button, asserting it opened. */
export async function openStrip(frame) {
  const button = await machineButton(frame);
  await button.click();
  const opened = await poll(async () => (await frame.locator('tr.machine-detail').count()) === 1, 3_000, 100);
  assert.ok(opened, 'a click on the machine button opens no tr.machine-detail (U10, U11)');
  return frame.locator('tr.machine-detail');
}

/** Each job list of the strip: its items' text, in order. */
export async function jobLists(frame) {
  return frame.evaluate(() => {
    const items = (selector) =>
      [...document.querySelectorAll(`tr.machine-detail ${selector} li[data-job-id]`)].map((li) => li.innerText.replace(/\s+/g, ' ').trim());
    return { running: items('ol.jobs-running'), waiting: items('ol.jobs-waiting'), outside: items('ol.jobs-outside') };
  });
}

/** The texts of the polite live regions. */
export async function liveText(frame) {
  return frame.evaluate(() =>
    [...document.querySelectorAll('[aria-live="polite"], [role="status"]')].map((r) => r.textContent.replace(/\s+/g, ' ').trim()).join(' | '),
  );
}

/** The focused element: a short key per element of the jobs DOM, else its tag and class. */
export async function focusKey(frame) {
  return frame.evaluate(() => {
    const el = document.activeElement;
    if (!el || el === document.body) return null;
    // An input or Add outside its list's `[data-scope]` takes its scope by order: This machine
    // comes first, then This repository.
    const byOrder = (selector) => ['machine', 'repository'][[...document.querySelectorAll(`tr.machine-detail ${selector}`)].indexOf(el)];
    const scope =
      el.closest('[data-scope]')?.dataset.scope ??
      (el.matches('input.heavy-input') ? byOrder('input.heavy-input') : el.matches('button.heavy-add') ? byOrder('button.heavy-add') : undefined);
    const chip = el.closest('li.heavy-chip');
    const chipText = chip ? chip.textContent.replace(/\s+/g, ' ').trim() : '';
    for (const [selector, key] of [
      ['button.machine-button', 'machine-button'],
      ['input.job-limit', 'limit'],
      ['select.job-enforce', 'enforce'],
      ['button.job-save', 'save'],
      ['button.job-default', 'defaults'],
      ['button.chip-remove', `remove ${scope} ${chipText}`],
      ['input.heavy-input', `input ${scope}`],
      ['button.heavy-add', `add ${scope}`],
      ['button.suggest-add', `suggest ${chipText}`],
      ['button.suggest-all', 'add-all'],
      ['tr[data-row-id]', `row ${el.dataset.rowId}`],
    ]) {
      if (el.matches(selector)) return key;
    }
    return `${el.tagName.toLowerCase()}.${String(el.className).trim().split(/\s+/).join('.')}`;
  });
}

/** Tells if the focused element shows a ring: an outline or a box shadow. */
export async function focusRing(frame) {
  return frame.evaluate(() => {
    const el = document.activeElement;
    if (!el || el === document.body) return false;
    const style = getComputedStyle(el);
    return (style.outlineStyle !== 'none' && parseFloat(style.outlineWidth) > 0) || style.boxShadow !== 'none';
  });
}

/** Moves the window's focus into the webview, as the exclusive flows do before keys. */
export async function focusWebview(session, frame) {
  for (let step = 0; step < 3 && !(await frame.evaluate(() => document.hasFocus())); step += 1) {
    await runCommand(session.window, 'View: Focus Next Editor Group', { keyboardOnly: true });
  }
}

const AGE = /(now|\d+ s|\d+ m)$/;
const squashAll = (text) => text.replace(/\s+/g, '');

/** Tells a job line that reads `parts` in order, with only spaces between them, then an age. */
export function reads(line, parts) {
  const want = squashAll(parts.join(' '));
  return squashAll(line).startsWith(want) && AGE.test(line.trim());
}

/** The problems of a job list against the lines it must hold, in any order. */
export function listProblems(name, lines, wanted) {
  const problems = [];
  if (lines.length !== wanted.length) problems.push(`${name} lists ${lines.length} jobs, not ${wanted.length}: ${JSON.stringify(lines)}`);
  for (const parts of wanted) {
    if (!lines.some((line) => reads(line, parts))) problems.push(`${name} has no line ${JSON.stringify(parts.join(' '))} then an age: ${JSON.stringify(lines)}`);
  }
  return problems;
}

/** The Activity word and agent lines of the row of worktree `name`, read as rows.mjs reads them. */
export async function agentLines(frame, session, name) {
  const row = (await shownRows(frame)).find((r) => r.id === worktreePath(session, name));
  return row ? { activity: row.activity, agent: row.agent } : null;
}

/** The colour a token resolves to in the frame. */
export async function tokenColour(frame, token) {
  return frame.evaluate((name) => {
    const probe = document.createElement('span');
    probe.style.color = `var(${name})`;
    document.body.append(probe);
    const value = getComputedStyle(probe).color;
    probe.remove();
    return value;
  }, token);
}
