// The run time on an agent line in Chromium, in Dark Modern, Light Modern, High Contrast and High
// Contrast Light at 1100 and 600 px: each line that ends in ` · <run time>` keeps to one line, no
// box cuts its text, and all of it meets WCAG AA on plain, hovered and selected rows
// (support/measure.mjs, clipping and textContrast).
import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { actor, ago, document, mustValidate, worktree } from '../support/docs.mjs';
import { clipping, textContrast } from '../support/measure.mjs';
import { THEME_NAMES } from '../support/page.mjs';
import { away, clickRow, hoverRow, settle, showPlain, visualPanel } from '../support/states.mjs';

test.describe.configure({ mode: 'parallel' });

const TASKS = '/private/tmp/claude-501/-Users-verify-pool/s1/tasks';

/** The slot `name` whose agent `label` waits for its background task `summary`, `seconds` old. */
function waitingSlot(name, label, summary, seconds) {
  const output = { task: name, path: `${TASKS}/${name}.output`, summary, running: true, started: ago(seconds), ended: null };
  const one = actor({ key: `k-${name}`, label, action: { tool: 'background:', target: summary }, outputs: [output], updated: ago(20) });
  return worktree(name, { branch: `${name}-release-candidate`, free: false, reasons: ['branch'], activity: 'working', actors: [one], last_event: one.updated });
}

function runTimeDocument() {
  return mustValidate(
    document([
      worktree('fg'),
      waitingSlot('wt1', 'release agent', 'until', 3 * 3600 + 12 * 60),
      waitingSlot('wt2', 'Review the TS host and fix the harness', 'cargo nextest', 2 * 86400 + 5 * 3600),
      worktree('wt3'),
    ]),
  );
}

/** How each slot's agent line ends. */
const ENDINGS = { wt1: ' · background: until · 3 h 12 m', wt2: ' · background: cargo nextest · 2 d 5 h' };

for (const theme of THEME_NAMES) {
  for (const width of [1100, 600]) {
    test(`run_time__should_clip_nothing_in_every_theme ${theme} ${width}`, async ({ page }) => {
      // Given
      const panel = await visualPanel(page, { theme, width, extraBodyClass: 'vscode-reduce-motion' });
      const doc = runTimeDocument();
      const wt1 = doc.pools[0].worktrees[1].path;
      await showPlain(page, panel, doc);
      const lines = () =>
        page.evaluate(() =>
          Object.fromEntries(
            [...document.querySelectorAll('tr[data-row-id]')].map((row) => [
              row.dataset.rowId.split('/').pop(),
              row.querySelector(':scope > td:nth-child(5) .line')?.textContent.replace(/\s+/g, ' ').trim() ?? '',
            ]),
          ),
        );
      const measure = async (at) => {
        const { issues } = await page.evaluate(clipping, false);
        const low = await page.evaluate(textContrast);
        return [
          ...issues.map((i) => `${at}: ${i.kind} ${i.el ?? i.side ?? ''} "${i.text ?? ''}" ${i.by ?? ''}`),
          ...low.map((f) => `${at}: ${f.key}: ${f.ratio}:1 under ${f.need}:1, "${f.text}"`),
        ];
      };
      const problems = [];

      // When
      const shown = await lines();
      problems.push(...(await measure('plain')));
      await hoverRow(page, wt1);
      problems.push(...(await measure('wt1 hovered')));
      await clickRow(page, wt1);
      problems.push(...(await measure('wt1 selected and hovered')));
      await away(page);
      await settle(page);
      problems.push(...(await measure('wt1 selected')));

      // Then
      for (const [name, ending] of Object.entries(ENDINGS)) {
        assert.ok(shown[name]?.endsWith(ending), `${name}'s agent line does not end in "${ending}": ${shown[name]}`);
      }
      assert.deepEqual(problems, [], `text clipped, wrapped or under AA in ${theme} at ${width} px`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });
  }
}
