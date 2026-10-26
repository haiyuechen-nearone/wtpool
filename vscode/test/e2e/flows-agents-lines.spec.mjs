import assert from 'node:assert/strict';
import path from 'node:path';
import { test } from '@playwright/test';

import { withFlow, poll, worktreePath } from './flows.mjs';
import { WITHIN_A_SECOND_MS, agent, stopJobs } from './jobs-process.mjs';
import { openTab } from './jobs-dom.mjs';
import { lineFacts } from './rows.mjs';
import { sleep } from './vscode.mjs';

/** A line icon's box and the gap after it, in px. */
const ICON_BOX = 14;
const ICON_GAP = 5;
/** Agents of different records write a little over a second apart, so `updated` orders them. */
const ACTOR_GAP_MS = 1100;

/** The running animations whose target lies in the Agent cell of a row. */
async function lineAnimations(frame) {
  return frame.evaluate(() =>
    document
      .getAnimations()
      .filter((a) => a.playState === 'running' && a.effect?.getComputedTiming().duration > 0)
      .map((a) => a.effect?.target)
      .filter((target) => target?.closest?.('tr[data-row-id] > td:nth-child(5)'))
      .map((target) => `${target.getAttribute('class') ?? target.tagName} in ${target.closest('tr').dataset.rowId}`),
  );
}

test('flow__several_agents_in_one_row', async () => {
  await withFlow('flow__several_agents_in_one_row', { scenario: 'several_agents_one_worktree' }, async (session) => {
    try {
      // Given
      const fg = session.pool.fg;
      const file = path.join(worktreePath(session, 'wt2'), 'f.txt');
      const frame = await openTab(session);
      const sessions = [];
      const parent = agent(session, { id: 'lead', title: 'Review the TS host', cwd: fg });
      await parent.hook('UserPromptSubmit');
      await parent.hook('SubagentStart', { agent_id: 'a1', agent_type: 'Explore' });
      await parent.hook('PreToolUse-Read-subagent', { agent_id: 'a1', agent_type: 'Explore', tool_input: { file_path: file }, tool_use_id: 'toolu_sub_read' });
      await parent.hook('PostToolUse-Read', { agent_id: 'a1', agent_type: 'Explore', tool_input: { file_path: file }, tool_use_id: 'toolu_sub_read' });
      await parent.hook('SubagentStop', { agent_id: 'a1', agent_type: 'Explore' });
      await sleep(ACTOR_GAP_MS);
      await parent.hook('PreToolUse-Read', { tool_input: { file_path: file }, tool_use_id: 'toolu_lead_read' });
      sessions.push(parent);
      for (const [id, title] of [['design', 'Design jobs'], ['panel', 'WT panel build'], ['harness', 'Fix the harness']]) {
        await sleep(ACTOR_GAP_MS);
        const one = agent(session, { id, title, cwd: fg });
        await one.hook('UserPromptSubmit');
        await one.hook('PreToolUse-Read', { tool_input: { file_path: file }, tool_use_id: `toolu_${id}_read` });
        sessions.push(one);
      }
      const wt2 = worktreePath(session, 'wt2');
      const factsOf = async () => (await lineFacts(frame)).find((row) => row.id === wt2);

      // When
      const five = await poll(async () => {
        const facts = await factsOf();
        return facts?.lines.length === 5 ? facts : undefined;
      }, WITHIN_A_SECOND_MS * 2, 100) ?? (await factsOf());
      const others = (await lineFacts(frame)).filter((row) => row.id !== wt2);
      const moving = await lineAnimations(frame);
      await frame.evaluate(() => document.body.classList.add('vscode-reduce-motion'));
      await sleep(300);
      const movingReduced = await lineAnimations(frame);
      await frame.evaluate(() => document.body.classList.remove('vscode-reduce-motion'));
      await sessions.at(-1).hook('SessionEnd', { reason: 'other' });
      const four = await poll(async () => {
        const facts = await factsOf();
        return facts?.count === '×4' ? facts : undefined;
      }, WITHIN_A_SECOND_MS * 2, 100) ?? (await factsOf());

      // Then
      assert.equal(five?.lines.length, 5, `wt2 does not list the five agents, one line each (R4): ${JSON.stringify(five)}`);
      assert.equal(five.count, '×5', 'wt2 with five agent lines does not show ×5 after its Activity word (U12)');
      assert.equal(five.countSkipped, true, 'screen readers read the ×5 (U12)');
      assert.equal(five.countColour, five.wordColour, 'the ×5 is not in the Activity word\'s colour (U12)');
      assert.deepEqual(five.countBorder, Array(4).fill(`1px solid ${five.wordColour}`), 'the ×5 is not inside a 1 px border of the word\'s colour (U12)');
      assert.equal(five.sameLine, true, 'the ×5 is not on the word\'s line, after it (U12)');
      assert.equal(five.activityHidden.trim(), ', 5 agents', `the cell does not read "<word>, 5 agents" to screen readers: ${JSON.stringify(five.activityHidden)} (U12)`);
      assert.deepEqual(others.filter((row) => row.count !== null).map((row) => row.id), [], 'a row with one agent line or none shows a count (U12)');
      const prefixes = five.lines.map((line) => line.hidden.trimEnd());
      assert.deepEqual([...prefixes].sort(), ['done:', 'working:', 'working:', 'working:', 'working:'], `the agent lines do not start with their state for screen readers: ${JSON.stringify(prefixes)} (U13)`);
      for (const [index, line] of five.lines.entries()) {
        assert.ok(line.icon, `agent line ${index + 1} has no .line-icon (U13)`);
        assert.ok(Math.abs(line.icon.width - ICON_BOX) <= 0.5 && Math.abs(line.icon.height - ICON_BOX) <= 0.5, `agent line ${index + 1}'s icon box is ${line.icon.width}×${line.icon.height}, not 14 px (U13)`);
        assert.ok(line.textLeft !== null && Math.abs(line.textLeft - line.icon.right - ICON_GAP) <= 1, `agent line ${index + 1}'s text starts ${line.textLeft - line.icon.right} px after its icon, not 5 (U13)`);
        assert.equal(line.icon.skipped, true, `screen readers read agent line ${index + 1}'s icon (U13)`);
      }
      const done = five.lines.find((line) => line.hidden.startsWith('done'));
      const working = five.lines.find((line) => line.hidden.startsWith('working'));
      assert.ok(working?.icon.signature.startsWith('svg'), `a working line's icon is not the robot, an inline SVG: ${working?.icon.signature} (U13)`);
      assert.notEqual(done?.icon.signature, working?.icon.signature, 'the done line shows the working icon (U13)');
      assert.deepEqual(moving, [], 'an agent line icon moves (U13)');
      assert.deepEqual(movingReduced, [], 'an agent line icon moves under reduce motion (U13)');
      assert.equal(four?.count, '×4', 'once one agent leaves, wt2 does not show ×4 (U12)');
      assert.equal(four.activityHidden.trim(), ', 4 agents', 'once one agent leaves, the cell does not read ", 4 agents" (U12)');
    } finally {
      await stopJobs(session);
    }
  });
});
