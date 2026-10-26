import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { test } from '@playwright/test';

import { scenarioDocuments } from '../support/docs.mjs';
import { age, rowOrder, shownActors } from '../support/grammar.mjs';
import { SCENARIOS, runDir } from '../support/paths.mjs';
import { AgentSteps } from './agents.mjs';
import { flowSession } from './flows.mjs';
import { expectedLineFacts, expectedRows, lineFacts, lineProblems, shownRows, waitForRows } from './rows.mjs';
import { close, panelFrame, runCommand, statusJson } from './vscode.mjs';

const PHASE = Number(process.env.WTPOOL_VERIFY_PHASE ?? 1);
/**
 * An agent event shows within 1 s. The wait adds the watch's 200 ms merge and a second of polling
 * and rendering, and fails at that deadline, not the 300 s test timeout.
 */
const AGENT_EVENT_MS = 3_000;
const SHOTS = new Set(['permission_wait_and_answer', 'several_agents_one_worktree']);

const names = readdirSync(SCENARIOS)
  .filter((file) => file.endsWith('.json'))
  .map((file) => file.slice(0, -'.json'.length))
  .filter((name) => {
    const scenario = JSON.parse(readFileSync(path.join(SCENARIOS, `${name}.json`), 'utf8'));
    return scenario.phase >= 2 && scenario.phase <= PHASE && scenario.layers.includes('e2e');
  });

/** Tells a row whose Last lines read their times' ages at one instant from `fromMs` to `toMs`. */
function lastOk(row, worktree, fromMs, toMs) {
  const times = worktree.actors.length
    ? shownActors(worktree).map((actor) => actor.updated)
    : [worktree.last_event ?? worktree.last_change];
  if (row.last.length !== times.length) return false;
  for (let at = fromMs; at <= toMs + 999; at += 500) {
    if (times.every((time, index) => age(time, at) === row.last[index])) return true;
  }
  return false;
}

for (const name of names) {
  test(name, async () => {
    // Given
    const session = await flowSession(`scenario/${name}`, { scenario: name });
    const { scenario, documents } = scenarioDocuments(name);
    const steps = new AgentSteps(session, name, scenario);
    const seen = [];
    try {
      await runCommand(session.window, 'Worktrees: Open Status');
      const frame = await panelFrame(session.window);
      assert.ok(frame, `${name}: the Worktrees tab shows no rows`);

      // When
      const observe = async (index) => {
        // A hook that failed is the product's failure, and the rows cannot follow it.
        assert.deepEqual(steps.problems, [], `${name}: hook or files steps before document ${index + 1}`);
        const expected = expectedRows(documents[index], session.pool);
        const rows = await waitForRows(frame, expected, AGENT_EVENT_MS);
        const from = Date.now();
        const shown = await shownRows(frame);
        const status = statusJson(session.dirs, session.pool);
        const to = Date.now();
        // Only a scenario of phase 4 or later checks the agent count and the screen reader text of
        // the agent lines.
        const lines = scenario.phase >= 4 ? lineProblems(await lineFacts(frame), expectedLineFacts(documents[index], session.pool)) : [];
        seen.push({ index, expected, rows, shown, status, from, to, lines });
        if (SHOTS.has(name)) {
          mkdirSync(path.join(runDir(), 'shots'), { recursive: true });
          await session.window.screenshot({ path: path.join(runDir(), 'shots', `e2e-${name}-${index + 1}.png`) });
        }
      };
      await steps.run(0, scenario.steps.length, observe);
      await observe(documents.length - 1);
    } finally {
      await steps.stop();
      await close(session);
    }

    // Then
    assert.deepEqual(steps.problems, [], `${name}: hook or files steps`);
    for (const shot of seen) {
      const where = `${name}, document ${shot.index + 1}`;
      assert.deepEqual(shot.rows, shot.expected, `${where}: the icon word, worktree, branch, git or agent text`);
      assert.deepEqual(shot.lines, [], `${where}: the count and the lines' screen reader text`);
      const byPath = new Map(rowOrder(shot.status.pools[0].worktrees).map((w) => [w.path, w]));
      for (const row of shot.shown) {
        const worktree = byPath.get(row.id);
        assert.ok(worktree, `${where}: wtpool status --json has no worktree ${row.id}`);
        assert.ok(lastOk(row, worktree, shot.from - 2000, shot.to), `${where}: ${row.id} Last reads ${JSON.stringify(row.last)}`);
      }
    }
  });
}
