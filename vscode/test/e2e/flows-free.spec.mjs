import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { test } from '@playwright/test';

import { scenarioDocuments } from '../support/docs.mjs';
import { AgentSteps } from './agents.mjs';
import { clickAction, openStatus, poll, withFlow, worktreePath } from './flows.mjs';
import { shownRows } from './rows.mjs';
import { git, panelFrame, runCommand, sleep } from './vscode.mjs';

/** VS Code's modal dialog, drawn in the window as support/code.mjs sets `window.dialogStyle`. */
const DIALOG = '.monaco-dialog-box';
const TOAST = '.notifications-toasts .notification-toast';
/** A git change shows within 1 s, plus the watch's merge, `wtpool` itself and polling. */
const CHANGE_MS = 10_000;
/** The time a flow watches for a change that must not come. */
const STILL_MS = 3_000;

/** The branch a worktree has checked out, or null when detached. */
function branchOf(dir) {
  try {
    return git(dir, ['symbolic-ref', '--quiet', '--short', 'HEAD']).trim();
  } catch {
    return null;
  }
}

function branchExists(dir, branch) {
  try {
    execFileSync('git', ['-C', dir, 'show-ref', '--verify', '--quiet', `refs/heads/${branch}`], {
      env: { PATH: process.env.PATH, GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_NOSYSTEM: '1' },
    });
    return true;
  } catch {
    return false;
  }
}

/** The row of `name` as the user reads it, or undefined. */
async function rowRead(frame, session, name) {
  const id = worktreePath(session, name);
  return (await shownRows(frame).catch(() => [])).find((row) => row.id === id);
}

/** Waits until the row of `name` satisfies `check`, and gives the last row read. */
async function rowUntil(frame, session, name, check, timeoutMs) {
  let last;
  await poll(async () => {
    last = await rowRead(frame, session, name);
    return last && check(last);
  }, timeoutMs);
  return last;
}

/** The modal dialog on screen: its text and button labels, or undefined after `timeoutMs`. */
async function modal(window, timeoutMs = 5_000) {
  const box = window.locator(DIALOG);
  if (!(await box.first().waitFor({ state: 'visible', timeout: timeoutMs }).then(() => true, () => false))) return undefined;
  const text = (await box.first().textContent()).replace(/\s+/g, ' ').trim();
  const buttons = (await box.first().locator('.monaco-button').allTextContents()).map((t) => t.trim()).filter(Boolean);
  return { text, buttons };
}

/** Clicks the dialog's button that `matches` accepts, and tells if there was one. */
async function press(window, matches) {
  const buttons = window.locator(`${DIALOG} .monaco-button`);
  for (let index = 0; index < (await buttons.count()); index += 1) {
    const label = (await buttons.nth(index).textContent()).trim();
    if (label && matches(label)) {
      await buttons.nth(index).click();
      return true;
    }
  }
  return false;
}

const isCancel = (label) => /^cancel$/i.test(label);

/**
 * Opens the tab on a scenario's pool with `branch` checked out in wt1, runs the scenario's
 * steps up to `until` (the index of a step) and waits until wt1 shows `activity`.
 */
async function slotWithAgent(session, scenarioName, until, activity, branch) {
  const { scenario } = scenarioDocuments(scenarioName);
  await runCommand(session.window, 'Worktrees: Open Status');
  const frame = await panelFrame(session.window);
  assert.ok(frame, 'Worktrees: Open Status showed no tab with rows');
  const steps = new AgentSteps(session, scenarioName, scenario);
  await steps.run(0, until, async () => {});
  const row = await rowUntil(frame, session, 'wt1', (r) => r.activity === activity && r.branch === branch, CHANGE_MS);
  return { frame, steps, row };
}

/** A pool option that checks out a new `branch` in the detached wt1 before VS Code starts. */
const checkoutInWt1 = (branch) => (poolInfo) => {
  git(`${poolInfo.dir}/wt1`, ['switch', '--quiet', '-c', branch]);
};

const IDLE_BRANCH = 'idle-work';

async function idleSlot(session) {
  const { scenario } = scenarioDocuments('idle_prompt_is_idle');
  return slotWithAgent(session, 'idle_prompt_is_idle', scenario.steps.length, 'idle', IDLE_BRANCH);
}

test('flow__free_confirm', async () => {
  const options = { scenario: 'idle_prompt_is_idle', prepare: checkoutInWt1(IDLE_BRANCH) };
  await withFlow('flow__free_confirm', options, async (session) => {
    // Given
    const { window } = session;
    const wt1 = worktreePath(session, 'wt1');
    const { frame, steps, row } = await idleSlot(session);
    try {
      assert.deepEqual(steps.problems, [], 'the hooks that leave wt1 idle');
      assert.equal(row?.activity, 'idle', `wt1 does not show idle: ${JSON.stringify(row)}`);

      // When
      await clickAction(frame, session, 'wt1', 'free');
      const asked = await modal(window);
      const confirmed = asked ? await press(window, (label) => !isCancel(label)) : false;
      const freed = await poll(() => branchOf(wt1) === null, CHANGE_MS);
      const after = await rowUntil(frame, session, 'wt1', (r) => r.branch === '(detached)', CHANGE_MS);

      // Then
      assert.ok(asked, 'free on wt1 showed no modal dialog');
      assert.match(asked.text, /\bwt1\b/, `the dialog does not name the worktree: ${asked.text}`);
      assert.ok(asked.text.includes(IDLE_BRANCH), `the dialog does not name the branch: ${asked.text}`);
      assert.match(asked.text, /\bidle\b/i, `the dialog does not warn that an agent in wt1 is idle: ${asked.text}`);
      assert.ok(confirmed, `the dialog offers nothing to confirm: ${JSON.stringify(asked.buttons)}`);
      assert.ok(freed, `confirming did not free wt1: it still holds ${branchOf(wt1)}`);
      assert.ok(branchExists(wt1, IDLE_BRANCH), 'freeing wt1 deleted its branch');
      assert.equal(after?.branch, '(detached)', `the row does not show wt1 freed: ${JSON.stringify(after)}`);
    } finally {
      await steps.stop();
    }
  });
});

test('flow__free_cancel', async () => {
  const options = { scenario: 'idle_prompt_is_idle', prepare: checkoutInWt1(IDLE_BRANCH) };
  await withFlow('flow__free_cancel', options, async (session) => {
    // Given
    const { window } = session;
    const wt1 = worktreePath(session, 'wt1');
    const { frame, steps, row: before } = await idleSlot(session);
    try {
      assert.equal(before?.branch, IDLE_BRANCH, `wt1 does not show its branch: ${JSON.stringify(before)}`);

      // When
      await clickAction(frame, session, 'wt1', 'free');
      const asked = await modal(window);
      const cancelled = asked ? await press(window, isCancel) : false;
      await sleep(STILL_MS);
      const open = await window.locator(DIALOG).count();
      const after = await rowRead(frame, session, 'wt1');

      // Then
      assert.ok(asked, 'free on wt1 showed no modal dialog');
      assert.ok(cancelled, `the dialog has no Cancel button: ${JSON.stringify(asked.buttons)}`);
      assert.equal(open, 0, 'the dialog stayed open after Cancel');
      assert.equal(branchOf(wt1), IDLE_BRANCH, 'a cancelled free changed wt1\'s branch');
      const { last: _a, ...afterText } = after ?? {};
      const { last: _b, ...beforeText } = before ?? {};
      assert.deepEqual(afterText, beforeText, 'a cancelled free changed wt1\'s row (its ages aside)');
    } finally {
      await steps.stop();
    }
  });
});

const BUSY_BRANCH = 'busy-work';

test('flow__free_refused', async () => {
  const options = { scenario: 'agent_working_in_slot', prepare: checkoutInWt1(BUSY_BRANCH) };
  await withFlow('flow__free_refused', options, async (session) => {
    // Given
    const { window } = session;
    const wt1 = worktreePath(session, 'wt1');
    const { scenario } = scenarioDocuments('agent_working_in_slot');
    const working = scenario.steps.findIndex((step) => step.expect);
    const { frame, steps, row: before } = await slotWithAgent(session, 'agent_working_in_slot', working, 'working', BUSY_BRANCH);
    try {
      assert.deepEqual(steps.problems, [], 'the hooks of the working agent');
      assert.equal(before?.activity, 'working', `wt1 does not show working: ${JSON.stringify(before)}`);

      // When
      await clickAction(frame, session, 'wt1', 'free');
      const said = await poll(async () => {
        const texts = await window.locator(`${DIALOG}, ${TOAST}`).allTextContents();
        return texts.map((t) => t.replace(/\s+/g, ' ').trim()).find((t) => /\bwt1\b/.test(t));
      }, 5_000);
      // Whatever a dialog offers, the user takes the first way ahead.
      if (await window.locator(DIALOG).count()) await press(window, (label) => !isCancel(label));
      await sleep(STILL_MS);
      const after = await rowRead(frame, session, 'wt1');

      // Then
      assert.equal(branchOf(wt1), BUSY_BRANCH, 'free released wt1\'s branch while an agent works there');
      assert.equal(after?.branch, BUSY_BRANCH, `the row of wt1 changed: ${JSON.stringify(after)}`);
      assert.ok(said, 'the refusal shows the user no message naming wt1');
    } finally {
      await steps.stop();
    }
  });
});

test('flow__fg_confirm', async () => {
  await withFlow('flow__fg_confirm', {}, async (session) => {
    // Given
    const { window, pool } = session;
    const wt2 = worktreePath(session, 'wt2');
    const frame = await openStatus(session);
    const branch = branchOf(wt2);
    assert.equal(branch, 'feature-a', 'setup: idle_pool\'s wt2 holds feature-a');

    // When
    await clickAction(frame, session, 'wt2', 'fg');
    const asked = await modal(window);
    const confirmed = asked ? await press(window, (label) => !isCancel(label)) : false;
    const moved = await poll(() => branchOf(pool.fg) === branch, CHANGE_MS);
    const fgRow = await rowUntil(frame, session, 'fg', (r) => r.branch === branch, CHANGE_MS);

    // Then
    assert.ok(asked, 'fg on wt2 showed no modal dialog');
    assert.match(asked.text, /\bwt2\b/, `the dialog does not name the worktree: ${asked.text}`);
    assert.ok(asked.text.includes(branch), `the dialog does not name the branch: ${asked.text}`);
    assert.ok(confirmed, `the dialog offers nothing to confirm: ${JSON.stringify(asked.buttons)}`);
    assert.ok(moved, `confirming did not run wtpool fg: the foreground holds ${branchOf(pool.fg)}`);
    assert.equal(fgRow?.branch, branch, `the foreground row does not show ${branch}: ${JSON.stringify(fgRow)}`);
  });
});
