// The scenario catalog in the render layer: every expected document of a scenario renders as
// the oracle's text model says.
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { afterEach, test } from 'vitest';

import { NOW, mustValidate, scenarioDocuments } from '../support/docs.mjs';
import { chipCounts, expectedRow, normalizeEscapes, rowOrder } from '../support/grammar.mjs';
import { SCENARIOS } from '../support/paths.mjs';
import { cells, isShown, lines, mount, rows, text } from '../support/webview.mjs';

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
  return rows(document).map((row) => {
    const { data } = cells(row);
    return {
      id: row.getAttribute('data-row-id'),
      activity: normalizeEscapes(text(data[0])),
      worktree: normalizeEscapes(text(data[1])),
      branch: normalizeEscapes(text(data[2])),
      git: normalizeEscapes(text(data[3])),
      agent: data[4] ? lines(data[4]).map(normalizeEscapes) : [],
      last: data[5] ? lines(data[5]) : [],
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
  const { documents } = scenarioDocuments(name);
  documents.forEach(mustValidate);
  webview = await mount({ now: NOW });

  // When
  const seen = documents.map((status) => {
    webview.show(status);
    const note = webview.document.querySelector('.agents-off');
    return {
      rows: shownRows(webview.document),
      chips: chips(webview.document, webview.window),
      note: note && isShown(webview.window, note) ? text(note) : null,
    };
  });

  // Then
  documents.forEach((status, index) => {
    const pool = status.pools[0];
    const expectedRows = rowOrder(pool.worktrees).map((w) => ({ id: w.path, ...expectedRow(w, NOW) }));
    assert.deepEqual(seen[index].rows, expectedRows, `${name}, document ${index + 1}: rows`);
    const counts = chipCounts(pool.worktrees);
    const expectedChips = Object.fromEntries(
      Object.entries(counts).map(([word, count]) => [word, count ? `${count} ${word}` : null]),
    );
    assert.deepEqual(seen[index].chips, expectedChips, `${name}, document ${index + 1}: chips`);
    if (pool.agents === 'off') {
      assert.ok(seen[index].note?.includes('agent activity is off'), `${name}: the agents off note: ${seen[index].note}`);
      // S8 (round publish-fixups): `wtpool setup`, then the plugin's commands (H28) and `wtpool hook install` (H22).
      const note = seen[index].note ?? '';
      const setup = note.indexOf('wtpool setup');
      const hooks = ['/plugin marketplace add haiyuechen-nearone/wtpool', '/plugin install wtpool@wtpool', 'wtpool hook install'];
      for (const step of hooks) {
        assert.ok(setup >= 0 && note.indexOf(step) > setup, `${name}: the note says how to turn agents on, wtpool setup then ${step}: ${note}`);
      }
    } else {
      assert.equal(seen[index].note, null, `${name}: an agents off note while agents are on`);
    }
  });
});
