import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { scenarioDocuments } from '../support/docs.mjs';
import { AgentSteps } from './agents.mjs';
import { poll, rowOf, withFlow } from './flows.mjs';
import { expectedRows, waitForRows } from './rows.mjs';
import { editorGroups, isStatusTab, panelFrame, runCommand } from './vscode.mjs';

const SCENARIO = 'permission_wait_and_answer';
/**
 * An agent event shows within 1 s. The watch merges events for up to 200 ms, and polling and
 * rendering take up to a second more.
 */
const AGENT_EVENT_MS = 3_000;

/** The label of the Worktrees tab, or undefined. */
async function statusTitle(window) {
  const groups = await editorGroups(window);
  return groups.flatMap((g) => g.tabs).find((t) => isStatusTab(t.label))?.label;
}

/** VS Code toasts and dialogs on screen. */
async function popups(window) {
  return window.locator('.notifications-toasts .notification-toast, .monaco-dialog-box, .quick-input-widget:visible').count();
}

/** The text of the panel's polite live regions. */
async function announced(frame) {
  const texts = await frame.locator('[aria-live="polite"], [role="status"]').allTextContents();
  return texts.join(' ').replace(/\s+/g, ' ').trim();
}

/**
 * Opens the tab on the scenario's pool and runs its steps up to the first expected document, wt2
 * waiting. The returned `steps` runs the rest.
 */
async function untilWaiting(session) {
  const { scenario, documents } = scenarioDocuments(SCENARIO);
  await runCommand(session.window, 'Worktrees: Open Status');
  const frame = await panelFrame(session.window);
  assert.ok(frame, 'Worktrees: Open Status showed no tab with rows');
  const steps = new AgentSteps(session, SCENARIO, scenario);
  const firstExpect = scenario.steps.findIndex((step) => step.expect);
  await steps.run(0, firstExpect, async () => {});
  const waiting = expectedRows(documents[0], session.pool);
  const answered = expectedRows(documents.at(-1), session.pool);
  return { frame, steps, firstExpect, scenario, waiting, answered };
}

test('flow__wait_appears', async () => {
  await withFlow('flow__wait_appears', { scenario: SCENARIO }, async (session) => {
    // Given
    const { window } = session;
    const { frame, steps, waiting } = await untilWaiting(session);
    try {
      // When
      const rows = await waitForRows(frame, waiting, AGENT_EVENT_MS);
      const chip = frame.locator('.chip').filter({ hasText: /waiting/ });
      const chipText = (await chip.count()) ? (await chip.first().textContent()).replace(/\s+/g, ' ').trim() : null;
      const title = await poll(async () => ((await statusTitle(window)) === 'Worktrees (1)' ? 'Worktrees (1)' : undefined), 5_000);
      const said = await announced(frame);
      const shown = await popups(window);

      // Then
      assert.deepEqual(steps.problems, [], 'the hooks of the wait');
      assert.deepEqual(rows, waiting, 'wt2 does not show waiting');
      assert.equal(chipText, '1 waiting', 'the waiting chip');
      assert.equal(title, 'Worktrees (1)', `the tab title reads ${await statusTitle(window)}`);
      assert.match(said, /wt2/, `the live region did not announce wt2: "${said}"`);
      assert.equal(shown, 0, 'a wait raised a popup');
    } finally {
      await steps.stop();
    }
  });
});

test('flow__waiting_chip_focuses', async () => {
  await withFlow('flow__waiting_chip_focuses', { scenario: SCENARIO }, async (session) => {
    // Given
    const { frame, steps, waiting } = await untilWaiting(session);
    try {
      await waitForRows(frame, waiting, AGENT_EVENT_MS);
      const chip = frame.locator('.chip').filter({ hasText: /waiting/ });
      const visible = (await chip.count()) === 1 && (await chip.first().isVisible());
      assert.ok(visible, 'no waiting chip shows while wt2 waits');

      // When
      await chip.first().click();
      const focused = await poll(
        () => frame.evaluate(() => document.activeElement?.closest('tr[data-row-id]')?.dataset.rowId ?? null),
        3_000,
      );

      // Then
      const wt2 = await rowOf(frame, session, 'wt2').getAttribute('data-row-id');
      assert.equal(focused, wt2, 'the waiting chip did not move focus to wt2, the first waiting row');
    } finally {
      await steps.stop();
    }
  });
});

test('flow__wait_answered', async () => {
  await withFlow('flow__wait_answered', { scenario: SCENARIO }, async (session) => {
    // Given
    const { window } = session;
    const { frame, steps, firstExpect, scenario, waiting, answered } = await untilWaiting(session);
    try {
      const before = await waitForRows(frame, waiting, AGENT_EVENT_MS);
      assert.deepEqual(before, waiting, 'wt2 never showed waiting');

      // When
      await steps.run(firstExpect + 1, scenario.steps.length, async () => {});
      const rows = await waitForRows(frame, answered, AGENT_EVENT_MS);
      const title = await poll(async () => ((await statusTitle(window)) === 'Worktrees' ? 'Worktrees' : undefined), 5_000);
      const chips = await frame.locator('.chip').filter({ hasText: /waiting/ }).evaluateAll((found) =>
        found.filter((el) => el.checkVisibility()).length,
      );

      // Then
      assert.deepEqual(steps.problems, [], 'the hooks of the answer');
      assert.deepEqual(rows, answered, 'wt2 does not show working after the answer');
      assert.equal(title, 'Worktrees', `the tab title reads ${await statusTitle(window)}`);
      assert.equal(chips, 0, 'the waiting chip still shows');
    } finally {
      await steps.stop();
    }
  });
});
