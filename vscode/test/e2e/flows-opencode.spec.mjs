import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { detailText, poll, rowOf, withFlow, worktreePath } from './flows.mjs';
import { openTab } from './jobs-dom.mjs';
import { WITHIN_A_SECOND_MS, agent, jobEnv, stopJobs, wtpoolRun } from './jobs-process.mjs';
import { git, releaseWtpool } from './vscode.mjs';

/** A change that has to show within 1 s of the event, as the other flows read it. */
const CHANGE_MS = 10_000;
const COORDINATOR = 'Coordinate the OpenCode task';
const TASK = 'fix the docs';
const SESSION = 'ses_e76df7aa0ffe3vdlGbn0I4xCYq';
const MODEL = { id: 'deepseek-v4.1-flash-free', providerID: 'opencode' };
const LABEL = 'deepseek-v4.1-flash-free · OpenCode';

/** An OC6 payload as OpenCode's plugin forwards it from the instance of `directory`. */
const forwarded = (directory, kind, name, data) => ({ v: 1, kind, name, directory, data });

/**
 * The agent lines of the row `id`: each line's text, its `title` (none is wanted), whether a state icon comes first and
 * the mark box right after it, the mark and badge ids that box names, and whether screen readers
 * skip it.
 */
async function agentLines(frame, id) {
  return frame.evaluate((rowId) => {
    const row = [...document.querySelectorAll('tr[data-row-id]')].find((r) => r.dataset.rowId === rowId);
    const cell = row?.querySelector(':scope > td:nth-child(5)');
    return [...(cell?.querySelectorAll('.line') ?? [])].map((line) => {
      const icon = line.querySelector('.line-icon');
      const mark = line.querySelector('.line-mark');
      return {
        text: line.textContent.replace(/\s+/g, ' ').trim(),
        title: line.getAttribute('title'),
        iconFirst: line.firstElementChild === icon && icon !== null,
        markAfterIcon: mark !== null && icon?.nextElementSibling === mark,
        mark: mark?.getAttribute('data-mark') ?? null,
        badge: mark?.getAttribute('data-badge') ?? null,
        markHidden: mark?.getAttribute('aria-hidden') === 'true',
      };
    });
  }, id);
}

/**
 * Hovers the mark of the first agent line of row `id` and gives the text of the floating label
 * once it shows, or undefined. Then moves the pointer off the mark and tells whether the label went.
 */
async function hoverLabel(frame, id) {
  const label = frame.locator('.float-label:not([hidden])');
  await frame.locator(`tr[data-row-id="${id}"] .line-mark`).first().hover();
  const text = await poll(async () => ((await label.count()) === 1 ? (await label.first().innerText()).replace(/\s+/g, ' ').trim() : undefined), WITHIN_A_SECOND_MS, 100);
  await frame.locator('tr.pool-heading').first().hover();
  const gone = await poll(async () => (await label.count()) === 0, WITHIN_A_SECOND_MS, 100);
  return { text, gone: Boolean(gone) };
}

/** Sends one forwarded payload through `wtpool hook --harness opencode` from the server stub. */
async function send(server, session, directory, payload) {
  const reply = await server.send({ binary: releaseWtpool(), args: ['hook', '--harness', 'opencode'], payload, cwd: directory, env: jobEnv(session) });
  if (reply.code !== 0 || reply.stdout) {
    throw new Error(`setup: wtpool hook --harness opencode for ${payload.name} exited ${reply.code ?? reply.signal}, stdout ${reply.stdout}, stderr ${reply.stderr}`);
  }
}

test('flow__coordinator_launches_opencode', async () => {
  const options = { scenario: 'idle_pool', prepare: (poolInfo) => git(poolInfo.fg, ['branch', 'oc-task']) };
  await withFlow('flow__coordinator_launches_opencode', options, async (session) => {
    try {
      // Given
      const fg = worktreePath(session, 'fg');
      const frame = await openTab(session);
      const coordinator = agent(session, { id: 'coordinator', title: COORDINATOR, cwd: session.pool.fg });
      await coordinator.hook('UserPromptSubmit');
      const bg = wtpoolRun(session, ['bg', 'oc-task']);
      assert.equal(bg.code, 0, `wtpool bg oc-task exited ${bg.code}: ${bg.stderr.trim()}`);
      const slot = bg.stdout.trim();
      assert.equal(slot, worktreePath(session, 'wt1'), `wtpool bg oc-task parked the branch in ${slot}, not in the one free slot`);
      await coordinator.hook('PreToolUse-Bash', { tool_input: { command: 'wtpool bg oc-task', description: 'wtpool-verify' }, tool_use_id: 'toolu_bg' });
      await coordinator.hook('PostToolUse-Bash', { tool_input: { command: 'wtpool bg oc-task', description: 'wtpool-verify' }, tool_use_id: 'toolu_bg' });
      await coordinator.hook('PreToolUse-Bash', {
        tool_input: { command: `cd ${slot} && opencode run --standalone "${TASK}"`, description: 'wtpool-verify' },
        tool_use_id: 'toolu_run',
      });
      const server = await coordinator.stub.spawnChild({ cwd: slot, env: jobEnv(session) });

      // When
      await send(server, session, slot, forwarded(slot, 'event', 'session.created', { sessionID: SESSION, title: TASK }));
      await send(server, session, slot, forwarded(slot, 'event', 'session.step.started', { sessionID: SESSION, model: MODEL }));
      const sentAt = Date.now();
      await send(server, session, slot, forwarded(slot, 'event', 'session.execution.started', { sessionID: SESSION }));
      const shown = await poll(async () => {
        const [line] = await agentLines(frame, slot);
        return line?.mark === 'deepseek' ? line : undefined;
      }, WITHIN_A_SECOND_MS, 100);
      const showedAfter = Date.now() - sentAt;
      const slotLines = await agentLines(frame, slot);
      const fgLines = await agentLines(frame, fg);
      const agentHover = await hoverLabel(frame, slot);
      const coordinatorHover = await hoverLabel(frame, fg);
      const slotRow = rowOf(frame, session, 'wt1');
      const word = (await slotRow.locator(':scope > td').first().innerText()).replace(/\s+/g, ' ').trim();
      await slotRow.locator(':scope > td').first().click({ position: { x: 4, y: 4 } });
      const detail = await poll(() => detailText(frame), WITHIN_A_SECOND_MS, 100);
      server.child.kill('SIGTERM');
      await server.exited;
      const gone = await poll(async () => !(await agentLines(frame, slot)).some((line) => line.text.includes(TASK)), CHANGE_MS, 200);
      const fgAfter = await agentLines(frame, fg);

      // Then
      assert.ok(shown, `the slot's row shows no agent line with the DeepSeek mark within 1 s: ${JSON.stringify(slotLines)} (OC2, U18)`);
      assert.ok(showedAfter <= WITHIN_A_SECOND_MS, `the OpenCode agent showed after ${showedAfter} ms`);
      assert.match(word, /working/i, `the slot's row does not read working: ${word} (OC2)`);
      assert.equal(slotLines.length, 1, `the slot's row lists ${slotLines.length} agent lines, not the one OpenCode agent: ${JSON.stringify(slotLines)}`);
      const [agentLine] = slotLines;
      assert.equal(agentLine.mark, 'deepseek', `the OpenCode agent's mark is ${agentLine.mark}, not DeepSeek's (U18)`);
      assert.equal(agentLine.badge, 'opencode', `the OpenCode agent's badge is ${agentLine.badge}, not OpenCode's (U18)`);
      assert.ok(agentLine.iconFirst && agentLine.markAfterIcon, `the line keeps its state icon first and holds the mark box right after it (U13): ${JSON.stringify(agentLine)}`);
      assert.ok(agentLine.markHidden, 'screen readers read the mark (U18)');
      assert.equal(agentLine.title, null, `the line has the tooltip ${JSON.stringify(agentLine.title)}, which would show a second label (U18)`);
      assert.equal(agentHover.text, LABEL, `hovering the OpenCode agent's mark shows the label ${JSON.stringify(agentHover.text)}, not ${LABEL} (U18)`);
      assert.ok(agentHover.gone, 'the label stays after the pointer left the mark (U18)');
      assert.equal(coordinatorHover.text, 'Claude Code', `hovering the coordinator's mark shows the label ${JSON.stringify(coordinatorHover.text)}, not its harness alone (U18)`);
      assert.ok(coordinatorHover.gone, "the coordinator's label stays after the pointer left the mark (U18)");
      assert.ok(agentLine.text.includes(TASK), `the line does not name the session's title: ${agentLine.text}`);
      assert.ok(detail?.includes(`launched by ${COORDINATOR}`), `the detail strip does not say it was launched by the coordinator's label: ${detail} (S17, U5)`);
      assert.ok(detail.includes('opencode · opencode/deepseek-v4.1-flash-free'), `the detail strip does not read the harness and model: ${detail} (U5)`);
      const coordinatorLine = fgLines.find((line) => line.text.includes(COORDINATOR));
      assert.ok(coordinatorLine, `the foreground row does not list the coordinator, which a shell call that starts an agent moved (H9): ${JSON.stringify(fgLines)}`);
      assert.equal(coordinatorLine.mark, 'anthropic', `the coordinator's mark is ${coordinatorLine.mark}, not Anthropic's (U18)`);
      assert.equal(coordinatorLine.badge, null, 'the coordinator, a Claude Code session, shows a badge (U18)');
      assert.ok(coordinatorLine.iconFirst && coordinatorLine.markAfterIcon, `the coordinator's line lacks the state icon and the mark box after it (U13): ${JSON.stringify(coordinatorLine)}`);
      assert.ok(!slotLines.some((line) => line.text.includes(COORDINATOR)), 'the coordinator shows in the slot (H9)');
      assert.ok(gone, 'once the OpenCode run ended, its line stayed in the slot (A3)');
      assert.ok(fgAfter.some((line) => line.text.includes(COORDINATOR)), 'the coordinator left the foreground when the run ended (H9)');
    } finally {
      await stopJobs(session);
    }
  });
});
