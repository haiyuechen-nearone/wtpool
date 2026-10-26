import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { ago, actor, document, mustValidate, worktree } from '../support/docs.mjs';
import { boundaries, clipping, ring, textContrast } from '../support/measure.mjs';
import { THEME_NAMES } from '../support/page.mjs';
import { clickRow, settle, visualPanel } from '../support/states.mjs';

test.describe.configure({ mode: 'parallel' });

const TASKS = '/private/tmp/claude-501/-Users-verify-pool/s1/tasks';
const WT2 = '/Users/verify/pool/wt2';

/** A build log of `count` lines, some far longer than the tab. */
export function buildLog(count) {
  const lines = [];
  for (let n = 1; n <= count; n += 1) {
    if (n % 4 === 0) {
      lines.push(`warning: unused variable: \`frobnicated_widget_cache_entry_${n}\` in crates/widget-cache/src/health/region_${n}.rs:${n}:17 ${'help: if this is intentional, prefix it with an underscore '.repeat(3)}`);
    } else lines.push(`   Compiling widget-cache-part-${n} v0.${n}.0 (/Users/verify/pool/wt2/crates/part-${n})`);
  }
  return lines;
}

function output(task, extra = {}) {
  return { task, path: `${TASKS}/${task}.output`, summary: 'cargo build', running: true, started: ago(200), ended: null, ...extra };
}

export function outputDocument() {
  const builder = actor({
    key: 's1',
    label: 'release build',
    state: 'working',
    action: { tool: 'Bash', target: 'cargo build' },
    outputs: [output('b2'), output('b1', { summary: 'cargo nextest', running: false, ended: ago(30) })],
  });
  return mustValidate(
    document([
      worktree('fg'),
      worktree('wt1'),
      worktree('wt2', { branch: 'release-candidate-widget-cache', free: false, reasons: ['branch'], activity: 'working', actors: [builder], last_event: builder.updated }),
      worktree('wt3'),
    ]),
  );
}

export async function showOutput(page, panel, { lines = 15 } = {}) {
  await panel.show(outputDocument());
  await clickRow(page, WT2);
  await page.evaluate(
    ({ row, tails }) => window.dispatchEvent(new MessageEvent('message', { data: { type: 'output', version: 1, row, tails } })),
    {
      row: WT2,
      tails: [
        { path: `${TASKS}/b2.output`, lines: buildLog(lines) },
        { path: `${TASKS}/b1.output`, lines: ['    Finished `test` profile [unoptimized] target(s) in 41.20s', '     Summary [  12.204s] 1047 tests run: 1047 passed, 0 skipped', '[exited with code 0]'] },
      ],
    },
  );
  await page.mouse.move(1, page.viewportSize().height - 1);
  await settle(page);
}

for (const theme of THEME_NAMES) {
  for (const width of [1100, 600]) {
    test(`output__should_clip_nothing_in_every_theme ${theme} ${width}`, async ({ page }) => {
      // Given
      const panel = await visualPanel(page, { theme, width, extraBodyClass: 'vscode-reduce-motion' });

      // When
      await showOutput(page, panel);
      const shown = await page.evaluate(() => {
        const strip = document.querySelector('tr.detail');
        const blocks = [...document.querySelectorAll('pre.output-tail')];
        return {
          tasks: document.querySelectorAll('.output[data-output-path]').length,
          follows: document.querySelectorAll('button.follow').length,
          wider: [...strip.querySelectorAll('*')].filter((el) => el.clientWidth > 0 && el.scrollWidth > el.clientWidth + 1).map((el) => `${el.tagName.toLowerCase()}.${el.className}`),
          blocks: blocks.map((b) => {
            const s = getComputedStyle(b);
            return { scrolls: b.scrollHeight > b.clientHeight + 1, overflowY: s.overflowY, lines: b.textContent.split('\n').length, height: b.getBoundingClientRect().height };
          }),
          inView: (() => {
            const r = strip.querySelector('.strip').getBoundingClientRect();
            return r.left >= -0.5 && r.right <= document.documentElement.clientWidth + 0.5;
          })(),
        };
      });
      const { issues } = await page.evaluate(clipping, false);
      // A tail block scrolls its own lines out of view, so text it clips is no issue here. `wider`
      // shows that nothing in it is cut sideways.
      const outsideBlocks = issues.filter((issue) => !(issue.kind === 'text-clipped' && String(issue.el).startsWith('pre.output-tail')));
      const low = await page.evaluate(textContrast);
      const borders = await page.evaluate(boundaries, 'pre.output-tail, button.follow');

      // Then
      assert.equal(shown.tasks, 2, 'the strip does not show the two tasks');
      assert.equal(shown.follows, 2, 'not one Follow button per task');
      assert.deepEqual(shown.wider, [], `parts of the strip are wider than their box in ${theme} at ${width} px`);
      assert.ok(shown.inView, 'the strip does not keep to the visible width');
      for (const block of shown.blocks) {
        assert.ok(!block.scrolls || ['auto', 'scroll'].includes(block.overflowY), 'a tail block hides lines it cannot scroll to');
      }
      assert.deepEqual(outsideBlocks.map((i) => `${i.kind} ${i.el} "${i.text}" by ${i.by ?? ''}`), [], `text clipped or cut in ${theme} at ${width} px`);
      assert.deepEqual(low.map((f) => `${f.key}: ${f.ratio}:1 under ${f.need}:1, "${f.text}"`), [], `text under WCAG AA contrast in ${theme}`);
      const weak = borders.filter((b) => b.best < 3).map((b) => `${b.el}: ${b.best}:1`);
      assert.deepEqual(weak, [], `tail blocks or Follow buttons under 3:1 against what is around them in ${theme}`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });
  }
}

for (const theme of THEME_NAMES) {
  for (const width of [1100, 600]) {
    test(`output__should_show_the_follow_ring_whole ${theme} ${width}`, async ({ page }) => {
      // Given
      const panel = await visualPanel(page, { theme, width, extraBodyClass: 'vscode-reduce-motion' });
      await showOutput(page, panel);
      await page.locator(`tr[data-row-id="${WT2}"]`).focus();
      const rings = [];

      // When
      for (let step = 0; step < 40; step += 1) {
        await page.keyboard.press('Tab');
        await page.clock.runFor(20);
        const at = await page.evaluate(() => {
          const el = document.activeElement;
          if (el?.matches('button.follow')) return `follow ${el.getAttribute('data-follow').split('/').pop()}`;
          if (el?.matches('pre.output-tail')) return `tail ${el.closest('[data-output-path]').getAttribute('data-output-path').split('/').pop()}`;
          if (el?.matches('tr[data-row-id]')) return 'row';
          return null;
        });
        if (at === 'row') break;
        if (!at) continue;
        rings.push({ at, ring: await page.evaluate(ring) });
      }

      // Then
      assert.deepEqual(rings.map((r) => r.at), ['follow b2.output', 'tail b2.output', 'follow b1.output', 'tail b1.output'], 'Tab does not reach each Follow button and tail block in order');
      const problems = rings.flatMap(({ at, ring: r }) => {
        if (r.missing) return [`${at}: no focus ring`];
        const out = [];
        if (Object.keys(r.lost).length) out.push(`${at}: ring cut by ${r.clippedBy.join(', ')} ${JSON.stringify(r.lost)}`);
        if (!r.contrast || r.contrast.ratio < 3) out.push(`${at}: ring ${r.contrast?.ring} on ${r.contrast?.under} is ${r.contrast?.ratio}:1, under 3:1`);
        return out;
      });
      assert.deepEqual(problems, [], `focus rings in the Output section in ${theme} at ${width} px`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });
  }
}

