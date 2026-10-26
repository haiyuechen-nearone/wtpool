import assert from 'node:assert/strict';
import path from 'node:path';
import { test } from '@playwright/test';

import { scenarioDocuments } from '../support/docs.mjs';
import { GROUP_TOKENS, GROUPS } from '../support/grammar.mjs';
import { expectedRows, resolved, waitForRows } from './rows.mjs';
import { PHASE } from '../support/paths.mjs';
import { openStatus, poll, withFlow, worktreePath } from './flows.mjs';
import { agent } from './jobs-process.mjs';
import { unflaggedCargo } from './jobs-config.mjs';
import { panelFrame, runCommand, sleep, startProcess, writeSettings } from './vscode.mjs';

/** Each theme with the test of its webview body classes. VS Code sets them per theme kind. */
const THEMES = [
  ['Default Light Modern', 'vscode-light', (c) => c.includes('vscode-light') && !c.some((k) => k.startsWith('vscode-high-contrast'))],
  ['Default Dark Modern', 'vscode-dark', (c) => c.includes('vscode-dark') && !c.some((k) => k.startsWith('vscode-high-contrast'))],
  ['Default High Contrast Light', 'vscode-high-contrast-light', (c) => c.includes('vscode-high-contrast-light')],
  ['Default High Contrast', 'vscode-high-contrast', (c) => c.includes('vscode-high-contrast') && !c.includes('vscode-high-contrast-light')],
];

/**
 * Each row's word and icon colour, and the colour of each group token, in the page. A page that
 * reloaded detaches its frame, and that reads as no marker and no rows.
 */
async function colours(frame) {
  return frame.evaluate((tokens) => {
    const resolve = (token) => {
      const probe = document.createElement('span');
      probe.style.color = `var(${token})`;
      document.body.append(probe);
      const value = getComputedStyle(probe).color;
      probe.remove();
      return value;
    };
    const groups = Object.fromEntries(Object.entries(tokens).map(([group, token]) => [group, resolve(token)]));
    const rows = [...document.querySelectorAll('tr[data-row-id]')].map((row) => {
      const cell = row.querySelector(':scope > td');
      const walker = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT);
      let word = null;
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        if (node.textContent.trim()) {
          word = node.parentElement;
          break;
        }
      }
      const glyph = [cell, ...cell.querySelectorAll('*')].find((el) => {
        const content = getComputedStyle(el, '::before').content;
        return content && content !== 'none' && content !== 'normal';
      });
      const before = glyph ? getComputedStyle(glyph, '::before') : null;
      const masked = before && before.maskImage && before.maskImage !== 'none';
      // An inline SVG icon (the working robot) takes the fill of its shapes.
      const svg = glyph ? null : cell.querySelector('svg');
      const fills = svg ? [...new Set([...svg.querySelectorAll('path, circle, rect, ellipse, polygon, line, polyline')].map((shape) => getComputedStyle(shape).fill))] : [];
      return {
        id: row.dataset.rowId,
        word: word ? getComputedStyle(word).color : null,
        icon: before ? (masked ? before.backgroundColor : before.color) : fills.length ? fills.join(' and ') : null,
      };
    });
    return { groups, rows, body: document.body.className, marker: window.__wtpoolVerifyMarker ?? null };
  }, GROUP_TOKENS).catch((error) => ({ groups: {}, rows: [], body: '', marker: null, error: String(error) }));
}

test('flow__theme_switch', async () => {
  await withFlow('flow__theme_switch', {}, async (session) => {
    // Given
    const frame = await openStatus(session);
    await frame.evaluate(() => {
      window.__wtpoolVerifyMarker = 'not reloaded';
    });
    const document = resolved(scenarioDocuments(session.scenario).documents.at(-1), session.pool);
    const activity = new Map(document.pools[0].worktrees.map((w) => [w.path, w.activity]));

    // When
    const seen = [];
    for (const [theme, bodyClass, matches] of THEMES) {
      const write = () => writeSettings(session.dirs, { 'workbench.colorTheme': theme });
      write();
      // A change that VS Code's file watcher missed is written again.
      let waited = 0;
      const applied = await poll(async () => {
        if (waited++ % 50 === 49) write();
        return matches((await colours(frame)).body.split(/\s+/));
      }, 40_000);
      // The theme's colours settle some time after its body class: two reads in a row agree.
      let last;
      await poll(async () => {
        const now = JSON.stringify(await colours(frame));
        const settled = now === last;
        last = now;
        return settled;
      }, 5_000, 100);
      seen.push({ theme, bodyClass, applied, ...(await colours(frame)) });
    }

    // Then
    for (const shot of seen) {
      assert.equal(shot.marker, 'not reloaded', `the webview reloaded when the theme became ${shot.theme}${shot.error ? `: ${shot.error}` : ''}`);
      assert.ok(shot.applied, `setup: the webview body never got ${shot.bodyClass} for ${shot.theme}: ${shot.body}`);
      assert.equal(shot.rows.length, activity.size, `${shot.theme}: the rows are gone`);
      for (const row of shot.rows) {
        const group = GROUPS[activity.get(row.id)];
        assert.equal(row.word, shot.groups[group], `${shot.theme}: ${row.id}'s word is not ${GROUP_TOKENS[group]}`);
        assert.equal(row.icon, shot.groups[group], `${shot.theme}: ${row.id}'s icon is not ${GROUP_TOKENS[group]}`);
      }
    }
    const blues = new Set(seen.map((shot) => shot.groups.active));
    assert.ok(blues.size >= 2, `setup: ${GROUP_TOKENS.active} is the same in every theme`);
  });
});

test('flow__reduced_motion', async () => {
  await withFlow('flow__reduced_motion', { scenario: 'building_without_agent', settings: { 'workbench.reduceMotion': 'on' }, beforeLaunch: unflaggedCargo }, async (session) => {
    // Given
    const { window, dirs, pool: poolInfo } = session;
    const { scenario, documents } = scenarioDocuments(session.scenario);
    const start = scenario.steps.find((step) => step.process?.start).process.start;
    const build = startProcess(dirs, start.name, path.join(poolInfo.dir, start.in));
    let reader = null;
    try {
      await runCommand(window, 'Worktrees: Open Status');
      const frame = await panelFrame(window);
      assert.ok(frame, 'Worktrees: Open Status showed no tab with rows');
      const building = expectedRows(documents[0], poolInfo);
      const rows = await waitForRows(frame, building, 15_000);
      // From phase 4 an agent line starts with a state icon, the working robot among them, and it
      // never moves, with or without reduce motion.
      reader = PHASE >= 4 ? agent(session, { id: 'reader', title: 'Read in wt2', cwd: poolInfo.fg }) : null;
      let lineIcons = 0;
      if (reader) {
        await reader.hook('UserPromptSubmit');
        await reader.hook('PreToolUse-Read', { tool_input: { file_path: path.join(worktreePath(session, 'wt2'), 'f.txt') }, tool_use_id: 'toolu_reader_read' });
        lineIcons = (await poll(() => frame.locator('tr[data-row-id] > td:nth-child(5) .line-icon').count(), 5_000)) ?? 0;
      }
      const lineMoves = () =>
        frame.evaluate(() =>
          document.getAnimations()
            .filter((a) => a.playState === 'running' && a.effect?.getComputedTiming().duration > 0 && a.effect?.target?.closest?.('tr[data-row-id] > td:nth-child(5)'))
            .map((a) => `${a.effect.target.getAttribute('class') ?? a.effect.target.tagName} in ${a.effect.target.closest('tr').dataset.rowId}`),
        );

      // When
      await sleep(1000);
      const running = () =>
        frame.evaluate(() =>
          document.getAnimations()
            .filter((a) => a.playState === 'running' && a.effect?.getComputedTiming().duration > 0)
            .map((a) => `${a.animationName ?? a.constructor.name} on ${a.effect?.target?.closest?.('tr')?.dataset?.rowId ?? a.effect?.target?.tagName}${a.effect?.pseudoElement ?? ''}`),
        );
      const state = { body: await frame.evaluate(() => document.body.className), running: await running() };
      // The control: without the class the building row's dial spins, so the check above
      // is not empty by construction. The class goes back at once.
      await frame.evaluate(() => document.body.classList.remove('vscode-reduce-motion'));
      const spinning = await running();
      const linesMoving = await lineMoves();
      await frame.evaluate(() => document.body.classList.add('vscode-reduce-motion'));

      // Then
      assert.ok(state.body.split(/\s+/).includes('vscode-reduce-motion'), `setup: the webview body lacks vscode-reduce-motion: ${state.body}`);
      assert.deepEqual(rows, building, 'the building row does not show');
      assert.ok(spinning.some((a) => a.endsWith('wt1::before')), `setup: the building dial does not spin without reduce motion: ${JSON.stringify(spinning)}`);
      assert.deepEqual(state.running, [], 'animations run with reduce motion on');
      if (PHASE >= 4) {
        assert.ok(lineIcons > 0, 'from phase 4 the agent line in wt2 shows no .line-icon (U13)');
        assert.deepEqual(linesMoving, [], 'an agent line icon moves without reduce motion (U13)');
      }
    } finally {
      await reader?.stop();
      build.kill();
    }
  });
});
