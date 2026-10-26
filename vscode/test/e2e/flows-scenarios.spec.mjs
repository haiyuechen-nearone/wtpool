// flow__every_activity_shows (verification.md, "User flows"): every phase 1 scenario that runs
// in e2e, each in its own VS Code on its own pool (a pool is VS Code's
// environment, WTPOOL_FOREGROUND and WTPOOL_SLOT_DIR, so one session cannot show two). It also gives
// each scenario check its e2e result: <run>/e2e/scenarios.json, which layers/e2e.rs reads.
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, readdirSync, utimesSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { test } from '@playwright/test';

import { scenarioDocuments } from '../support/docs.mjs';
import { WORDS, rowOrder } from '../support/grammar.mjs';
import { SCENARIOS, runDir } from '../support/paths.mjs';
import { expectedLast, expectedRows, lastMatches, resolved, shownRows, waitForRows } from './rows.mjs';
import { flowSession } from './flows.mjs';
import { close, git, panelFrame, runCommand, startProcess, statusJson } from './vscode.mjs';

/** The phase the harness runs (WTPOOL_VERIFY_PHASE), 1 by default. */
const PHASE = Number(process.env.WTPOOL_VERIFY_PHASE ?? 1);

/**
 * Each row's activity icon: the ::before of the activity cell or an element in it, or an inline
 * SVG in the cell, which its shapes identify.
 */
async function icons(frame) {
  return frame.locator('tr[data-row-id]').evaluateAll((rows) =>
    rows.map((row) => {
      const cell = row.querySelector(':scope > td');
      const glyph = [cell, ...cell.querySelectorAll('*')].find((el) => {
        const content = getComputedStyle(el, '::before').content;
        return content && content !== 'none' && content !== 'normal';
      });
      const before = glyph ? getComputedStyle(glyph, '::before') : null;
      const svg = glyph ? null : cell.querySelector('svg');
      const shapes = svg ? [...svg.querySelectorAll('path, circle, rect, ellipse, polygon, line, polyline')] : [];
      const icon = glyph ?? svg;
      return {
        id: row.dataset.rowId,
        signature: before
          ? [before.content, before.fontFamily, before.maskImage, before.backgroundImage].join(' | ')
          : shapes.length
            ? `svg | ${shapes.map((shape) => shape.outerHTML).join('')}`
            : null,
        iconText: icon && icon !== cell ? icon.textContent.trim() : '',
      };
    }),
  );
}

/** How long ago each worktree's HEAD and index last changed, so Last shows every unit. */
const AGES = { fg: 130, wt1: 40, wt2: 3 * 3600 + 60, wt3: 3 * 86400 + 60, x: 70 * 60 };

/** Sets the times of each worktree's HEAD and index to {@link AGES} ago. */
function backdate(poolInfo) {
  for (const [name, seconds] of Object.entries(AGES)) {
    const gitDir = name === 'fg' ? poolInfo.common : path.join(poolInfo.common, 'worktrees', name);
    const time = new Date(Date.now() - seconds * 1000);
    for (const file of ['HEAD', 'index']) {
      if (existsSync(path.join(gitDir, file))) utimesSync(path.join(gitDir, file), time, time);
    }
  }
}

/** Runs one scenario's steps in its own VS Code and returns what each document showed. */
async function showScenario(name) {
  const session = await flowSession(`flow__every_activity_shows/${name}`, { scenario: name, prepare: backdate });
  const processes = new Map();
  const seen = [];
  try {
    const { window, dirs, pool: poolInfo } = session;
    const { scenario, documents } = scenarioDocuments(name);
    await runCommand(window, 'Worktrees: Open Status');
    const frame = await panelFrame(window);
    assert.ok(frame, `${name}: the Worktrees tab shows no rows`);
    let index = 0;
    const observe = async (document) => {
      const expected = expectedRows(document, poolInfo);
      const rows = await waitForRows(frame, expected, 15_000);
      const from = Date.now();
      const shown = await shownRows(frame);
      const status = statusJson(dirs, poolInfo);
      const to = Date.now();
      seen.push({ name, index, expected, rows, shown, status, from, to, icons: await icons(frame), document: resolved(document, poolInfo) });
      index += 1;
    };
    for (const step of scenario.steps) {
      if (step.git) git(path.join(poolInfo.dir, step.git.in), step.git.args);
      if (step.process?.start) {
        const { pid, name: program, in: where } = step.process.start;
        processes.set(pid, startProcess(dirs, program, path.join(poolInfo.dir, where)));
      }
      if (step.process?.stop !== undefined) processes.get(step.process.stop)?.kill();
      if (step.expect) await observe(documents[index]);
    }
    await observe(documents.at(-1));
    if (name === 'idle_pool') {
      // The e2e shot of the visual review (verification.md, "Visual review").
      mkdirSync(path.join(runDir(), 'shots'), { recursive: true });
      await window.screenshot({ path: path.join(runDir(), 'shots', 'e2e-idle_pool.png') });
    }
    return seen;
  } finally {
    for (const child of processes.values()) child.kill();
    await close(session);
  }
}

/**
 * Asserts what one scenario showed: each document's rows, each row's Last against the document
 * `wtpool status --json` printed then, and an icon without text per row. Adds each activity's icon
 * to `signatures`.
 */
function judgeScenario(shots, signatures) {
  for (const shot of shots) {
    const where = `${shot.name}, document ${shot.index + 1}`;
    assert.deepEqual(shot.rows, shot.expected, `${where}: the icon word, worktree, branch, git or agent text`);
    const last = expectedLast(shot.status, shot.from);
    for (const row of shot.shown) {
      const status = shot.status.pools[0].worktrees.find((w) => w.path === row.id);
      assert.ok(status, `${where}: wtpool status --json has no worktree ${row.id}`);
      const time = status.actors.length ? null : (status.last_event ?? status.last_change);
      const ok = row.last.length === 1 && lastMatches(row.last[0], time, shot.from - 2000, shot.to);
      assert.ok(ok, `${where}: ${row.id} Last reads ${JSON.stringify(row.last)}, not ${JSON.stringify(last[row.id])}`);
    }
    const byPath = new Map(rowOrder(shot.document.pools[0].worktrees).map((w) => [w.path, w]));
    for (const icon of shot.icons) {
      const { activity } = byPath.get(icon.id);
      assert.ok(icon.signature, `${where}: ${icon.id} (${WORDS[activity]}) has no icon, neither a ::before glyph nor an inline SVG`);
      assert.equal(icon.iconText, '', `${where}: ${icon.id}'s icon element holds text`);
      if (!signatures.has(activity)) signatures.set(activity, new Set());
      signatures.get(activity).add(icon.signature);
    }
  }
}

/** A scenario's e2e result as layers/e2e.rs reads it. */
function result(name, error) {
  if (!error) return { title: name, outcome: 'pass', kind: null, message: '' };
  const message = String(error.message ?? error).split('\n').slice(0, 6).join('\n');
  const assertion = error.code === 'ERR_ASSERTION' && !message.startsWith('setup:');
  return { title: name, outcome: 'fail', kind: assertion ? 'assertion' : 'error', message };
}

test('flow__every_activity_shows', async () => {
  // Given
  const names = readdirSync(SCENARIOS)
    .filter((file) => file.endsWith('.json'))
    .map((file) => file.slice(0, -'.json'.length))
    .filter((name) => {
      const scenario = JSON.parse(readFileSync(path.join(SCENARIOS, `${name}.json`), 'utf8'));
      // The phase 1 scenarios; scenarios-agents.spec.mjs runs the later ones, which have hook steps.
      return scenario.phase === 1 && scenario.phase <= PHASE && scenario.layers.includes('e2e');
    });
  const file = path.join(runDir(), 'e2e', 'scenarios.json');
  mkdirSync(path.dirname(file), { recursive: true });

  // When
  const signatures = new Map();
  const results = [];
  for (const name of names) {
    await test.step(name, async () => {
      try {
        judgeScenario(await showScenario(name), signatures);
        results.push(result(name));
      } catch (error) {
        results.push(result(name, error));
      }
    });
    writeFileSync(file, JSON.stringify(results, null, 2));
  }

  // Then
  assert.ok(names.length >= 8, `setup: ${names.length} scenarios of phase ${PHASE} run in e2e`);
  const failed = results.filter((r) => r.outcome !== 'pass').map((r) => `${r.title}: ${r.message.split('\n')[0]}`);
  assert.deepEqual(failed, [], 'scenarios whose rows, Last or icons the panel does not show');
  for (const [activity, found] of signatures) {
    assert.equal(found.size, 1, `${WORDS[activity]} shows ${found.size} different icons`);
  }
  const each = [...signatures].map(([activity, found]) => [activity, [...found][0]]);
  assert.equal(new Set(each.map(([, signature]) => signature)).size, each.length, `activities share an icon: ${JSON.stringify(each)}`);
});
