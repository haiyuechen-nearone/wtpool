// Every scenario of the render layer: each expected document renders as support/grammar.mjs
// says.
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { afterEach, test } from 'vitest';

import { NOW, mustValidate, scenarioDocuments } from '../support/docs.mjs';
import { WORDS, chipCounts, expectedRow, normalizeEscapes, rowOrder } from '../support/grammar.mjs';
import { agentProblems } from '../support/jobs-render.mjs';
import { SCENARIOS } from '../support/paths.mjs';
import { cells, isShown, mount, rows, seen, seenLines, text } from '../support/webview.mjs';

const names = readdirSync(SCENARIOS)
  .filter((file) => file.endsWith('.json'))
  .map((file) => file.slice(0, -'.json'.length))
  .filter((name) => {
    const scenario = JSON.parse(readFileSync(path.join(SCENARIOS, `${name}.json`), 'utf8'));
    return scenario.layers.includes('render');
  });

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

function shownRows(document) {
  const window = document.defaultView;
  return rows(document).map((row) => {
    const { data } = cells(row);
    return {
      id: row.getAttribute('data-row-id'),
      activity: normalizeEscapes(seen(window, data[0])),
      worktree: normalizeEscapes(seen(window, data[1])),
      branch: normalizeEscapes(seen(window, data[2])),
      git: normalizeEscapes(seen(window, data[3])),
      agent: data[4] ? seenLines(window, data[4]).map(normalizeEscapes) : [],
      last: data[5] ? seenLines(window, data[5]) : [],
    };
  });
}

function chips(document, window) {
  const found = {};
  for (const word of ['free', 'working', 'waiting']) {
    const chip = [...document.querySelectorAll('.chip')].find((c) => new RegExp(`\\b${word}\\b`).test(text(c)));
    found[word] = chip && isShown(window, chip) ? text(chip) : null;
  }
  return found;
}

test.each(names)('%s', async (name) => {
  // Given
  const { scenario, documents } = scenarioDocuments(name);
  documents.forEach(mustValidate);
  webview = await mount({ now: NOW });

  // When
  const observed = documents.map((status) => {
    webview.show(status);
    const note = webview.document.querySelector('.agents-off');
    // A scenario of phase 4 or later also judges the agent count and the start of each agent line.
    const byPath = new Map(status.pools[0].worktrees.map((w) => [w.path, w]));
    const agents =
      scenario.phase >= 4
        ? rows(webview.document).flatMap((row) => {
            const worktree = byPath.get(row.getAttribute('data-row-id'));
            return worktree ? agentProblems(webview.window, row, worktree, WORDS[worktree.activity]).map((p) => `${worktree.name}: ${p}`) : [];
          })
        : [];
    return {
      agents,
      rows: shownRows(webview.document),
      chips: chips(webview.document, webview.window),
      note: note && isShown(webview.window, note) ? text(note) : null,
    };
  });

  // Then
  documents.forEach((status, index) => {
    const pool = status.pools[0];
    const expectedRows = rowOrder(pool.worktrees).map((w) => ({ id: w.path, ...expectedRow(w, NOW, status.machine.jobs) }));
    assert.deepEqual(observed[index].rows, expectedRows, `${name}, document ${index + 1}: rows`);
    assert.deepEqual(observed[index].agents, [], `${name}, document ${index + 1}: the count and line icons (U12, U13)`);
    const counts = chipCounts(pool.worktrees);
    const expectedChips = Object.fromEntries(
      Object.entries(counts).map(([word, count]) => [word, count ? `${count} ${word}` : null]),
    );
    assert.deepEqual(observed[index].chips, expectedChips, `${name}, document ${index + 1}: chips`);
    if (pool.agents === 'off') {
      assert.ok(observed[index].note?.includes('agent activity is off'), `${name}: the agents off note: ${observed[index].note}`);
      const note = observed[index].note ?? '';
      const setup = note.indexOf('wtpool setup');
      const hooks = ['/plugin marketplace add haiyuechen-nearone/wtpool', '/plugin install wtpool@wtpool', 'wtpool hook install'];
      for (const step of hooks) {
        assert.ok(setup >= 0 && note.indexOf(step) > setup, `${name}: the note says how to turn agents on, wtpool setup then ${step}: ${note}`);
      }
    } else {
      assert.equal(observed[index].note, null, `${name}: an agents off note while agents are on`);
    }
  });
});
