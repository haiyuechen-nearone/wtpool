import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { scenarioDocuments } from '../support/docs.mjs';
import { clipping, tags } from '../support/measure.mjs';
import { THEME_NAMES } from '../support/page.mjs';
import { clickRow, hoverRow, rowIds, short, showPlain, visualPanel } from '../support/states.mjs';
import { expectedTags, stateDocuments, visualDocuments } from '../support/visual-docs.mjs';

const WIDTHS = [1100, 800, 600];

/** The first row with an actor line, else the first row: the one whose strip a check opens. */
async function stripRow(page) {
  return page.evaluate(() => {
    const rows = [...document.querySelectorAll('tr[data-row-id]')];
    const busy = rows.find((r) => r.querySelectorAll('td .line').length > 1 || /·/.test(r.textContent));
    return (busy ?? rows[0])?.dataset.rowId ?? null;
  });
}

function describe(issue) {
  const { kind, place, at, count = 1, ...rest } = issue;
  return `${kind}${place ? ` in the ${place}` : ''} x${count}, first at ${at}: ${JSON.stringify(rest)}`;
}

test.describe.configure({ mode: 'parallel' });

for (const theme of THEME_NAMES) {
  for (const width of WIDTHS) {
    test(`layout__should_clip_no_text ${theme} ${width}`, async ({ page }) => {
      // Given
      const panel = await visualPanel(page, { theme, width });
      const documents = visualDocuments();
      const found = new Map();
      const facts = {};
      const measure = async (at, hasEllipsis) => {
        const { issues, facts: f } = await page.evaluate(clipping, hasEllipsis);
        // One entry per kind of issue and element class, with a count and its first instance.
        for (const issue of issues) {
          const key = `${issue.kind} ${issue.place ?? issue.side ?? ''}`;
          const seen = found.get(key);
          if (seen) seen.count += 1;
          else found.set(key, { ...issue, at, count: 1 });
        }
        return f;
      };

      // When
      for (const item of documents) {
        const hasEllipsis = JSON.stringify(item.doc).includes('…');
        await showPlain(page, panel, item.doc, { options: item.options, ms: 0 });
        facts[item.name] = await measure(`${item.name} plain`, hasEllipsis);
        if (item.options) continue;
        const pick = await stripRow(page);
        if (pick) {
          await clickRow(page, pick, 0);
          await measure(`${item.name} ${short(pick)} selected`, hasEllipsis);
          await clickRow(page, pick, 0);
        }
        if (item.states) {
          for (const id of await rowIds(page)) {
            await hoverRow(page, id, 0);
            await measure(`${item.name} ${short(id)} hovered`, hasEllipsis);
          }
        }
      }

      // Then
      assert.equal(Object.keys(facts).length, new Set(documents.map((d) => d.name)).size, 'setup: a document was not measured');
      // Long values need more than 600 px: a table that fits was squeezed instead of scrolled.
      const long = facts.long_values;
      if (width === 600 && long.table <= long.tab) {
        found.set('squeezed', { kind: 'table-squeezed-into-the-tab', at: 'long_values plain', table: long.table, tab: long.tab });
      }
      const issues = [...found.values()].map(describe);
      assert.deepEqual(issues, [], `text clipped, cut with …, squeezed or out of reach at ${width} px in ${theme}`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });
  }
}

for (const width of WIDTHS) {
  test(`tags__should_show_whole_after_the_name ${width}`, async ({ browser }) => {
    // Given
    const documents = [
      ...stateDocuments(),
      { name: 'unmanaged_worktree', doc: scenarioDocuments('unmanaged_worktree').documents.at(-1) },
    ];
    const found = new Map();
    let measured = 0;

    // When
    for (const theme of THEME_NAMES) {
      const page = await browser.newPage();
      const panel = await visualPanel(page, { theme, width });
      for (const { name, doc } of documents) {
        const expected = expectedTags(doc);
        await showPlain(page, panel, doc, { ms: 0 });
        measured += Object.keys(expected).length;
        for (const { kind, row, tag, ...rest } of await page.evaluate(tags, expected)) {
          const key = `${name} ${row} "${tag}": ${kind} ${JSON.stringify(rest)}`;
          found.set(key, [...(found.get(key) ?? []), theme]);
        }
      }
      await page.close();
      assert.deepEqual(panel.errors, [], `page errors in ${theme}`);
    }

    // Then
    assert.ok(measured >= 4 * 6, `setup: only ${measured} tagged rows measured`);
    const issues = [...found].map(([key, themes]) => `${key} in ${themes.length === THEME_NAMES.length ? 'every theme' : themes.join(', ')}`);
    assert.deepEqual(issues, [], `worktree tags not whole, outside their cell or not after the name at ${width} px`);
  });
}
