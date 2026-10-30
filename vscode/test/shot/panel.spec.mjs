import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { expect, test } from '@playwright/test';

import { REPOSITORY, document, everyActivity, mustValidate, scenarioDocuments, withActor, worktree } from '../support/docs.mjs';
import { GROUP_TOKENS, GROUPS, WORDS, rowOrder } from '../support/grammar.mjs';
import { REPO, SCENARIOS, runDir } from '../support/paths.mjs';
import { axeRun } from '../support/measure.mjs';
import { THEME_NAMES, computedThemeColours, openPanel } from '../support/page.mjs';
import { sliceOf } from '../support/visual-docs.mjs';

const APPROVED = path.join(REPO, 'verify', 'shots', 'approved');
/** The other shots taken for review in every theme: a scenario and the index of its document. */
const REVIEW_SHOTS = [
  ['permission_wait_and_answer', 0],
  ['several_agents_one_worktree', -1],
];

/** Every colour on the page that is neither a value of the theme nor transparent. */
async function foreignColours(page, allowed) {
  return page.evaluate((allowedList) => {
    const ok = new Set([...allowedList, 'rgba(0, 0, 0, 0)']);
    const describe = (el) =>
      `${el.tagName.toLowerCase()}${el.className && typeof el.className === 'string' ? `.${el.className.trim().split(/\s+/).join('.')}` : ''}${el.dataset.rowId ? `[${el.dataset.rowId}]` : ''}`;
    const bad = [];
    for (const el of document.querySelectorAll('body, body *')) {
      if (!el.checkVisibility({ opacityProperty: true, visibilityProperty: true })) continue;
      for (const pseudo of [null, '::before', '::after']) {
        const style = getComputedStyle(el, pseudo);
        if (pseudo && (style.content === 'none' || style.content === 'normal')) continue;
        const properties = ['color', 'background-color'];
        for (const side of ['top', 'right', 'bottom', 'left']) {
          const width = parseFloat(style.getPropertyValue(`border-${side}-width`));
          if (width > 0 && style.getPropertyValue(`border-${side}-style`) !== 'none') {
            properties.push(`border-${side}-color`);
          }
        }
        if (style.outlineStyle !== 'none' && parseFloat(style.outlineWidth) > 0) properties.push('outline-color');
        if (style.textDecorationLine !== 'none') properties.push('text-decoration-color');
        for (const property of properties) {
          const value = style.getPropertyValue(property);
          if (!ok.has(value)) bad.push(`${describe(el)}${pseudo ?? ''} ${property}: ${value}`);
        }
        if (style.boxShadow !== 'none') {
          for (const match of style.boxShadow.matchAll(/rgba?\([^)]*\)/g)) {
            if (!ok.has(match[0])) bad.push(`${describe(el)}${pseudo ?? ''} box-shadow: ${match[0]}`);
          }
        }
      }
    }
    return [...new Set(bad)];
  }, allowed);
}

/**
 * A scenario document on GitHub, so its heading reads `acme/app · verify-mac`. The idle_pool shots
 * keep the folder name `fg`, for the heading without GitHub.
 */
function onGithub(doc) {
  const shown = structuredClone(doc);
  for (const pool of shown.pools) pool.repository = { ...REPOSITORY };
  return mustValidate(shown);
}

async function screenshot(page, name) {
  const dir = path.join(runDir(), 'shots');
  mkdirSync(dir, { recursive: true });
  const file = path.join(dir, `${name}.png`);
  const shot = await page.screenshot({ path: file, fullPage: true });
  const approved = path.join(APPROVED, `${name}.png`);
  if (existsSync(approved)) {
    // An array names the file as is: a plain string name would have its `_` turned into `-`, so
    // it would never find the approved dark-idle_pool-1100.png.
    expect(shot).toMatchSnapshot([`${name}.png`], { maxDiffPixelRatio: 0.001 });
  }
  return file;
}

test.describe('themes', () => {
  for (const theme of THEME_NAMES) {
    test(`shot__should_render_every_theme ${theme}`, async ({ page }) => {
      // Given
      const idle = mustValidate(scenarioDocuments('idle_pool').documents.at(-1));
      const all = mustValidate(everyActivity());
      const panel = await openPanel(page, { theme });
      const allowed = Object.values(await computedThemeColours(page, panel.theme));

      // When
      await panel.show(idle);
      await screenshot(page, `${theme}-idle_pool-1100`);
      for (const [name, index] of REVIEW_SHOTS) {
        await panel.show(onGithub(scenarioDocuments(name).documents.at(index)));
        await screenshot(page, `${theme}-${name}-1100`);
      }
      await panel.show(all);
      const plain = await foreignColours(page, allowed);
      const busyRow = page.locator('tr[data-row-id$="/wt1"]');
      await busyRow.click();
      await busyRow.hover();
      await page.clock.runFor(50);
      const selected = await foreignColours(page, allowed);
      await page.keyboard.press('Tab');
      await page.clock.runFor(50);
      const focused = await foreignColours(page, allowed);
      await screenshot(page, `${theme}-every_activity-1100`);

      // Then
      assert.deepEqual(plain, [], `colours outside the ${theme} theme`);
      assert.deepEqual(selected, [], `colours outside the ${theme} theme with a selected, hovered row`);
      assert.deepEqual(focused, [], `colours outside the ${theme} theme with focus`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });
  }

  test('shot__should_render_every_theme narrow', async ({ page }) => {
    // Given
    await page.setViewportSize({ width: 600, height: 800 });
    const idle = mustValidate(scenarioDocuments('idle_pool').documents.at(-1));
    const long = structuredClone(idle);
    long.pools[0].worktrees[2].branch = '1234-check-widget-cache-health-before-each-deploy-and-more';
    const panel = await openPanel(page, { theme: 'dark' });

    // When
    await panel.show(long);
    await screenshot(page, 'dark-idle_pool-600');
    // The first document of permission_wait_and_answer is its wait.
    await panel.show(onGithub(scenarioDocuments('permission_wait_and_answer').documents[0]));
    await screenshot(page, 'dark-permission_wait_and_answer-600');
    await panel.show(long);
    const layout = await page.evaluate(() => {
      const row = document.querySelectorAll('tr[data-row-id]')[2];
      const cell = row.querySelectorAll(':scope > td')[2];
      const clipped = [cell, ...cell.querySelectorAll('*')].find((el) => el.scrollWidth > el.clientWidth + 1);
      const style = clipped ? getComputedStyle(clipped) : null;
      return {
        clipped: Boolean(clipped),
        ellipsis: style?.textOverflow,
        text: cell.textContent.trim(),
      };
    });

    // Then
    // Nothing is cut, as a table wider than the tab scrolls sideways inside it.
    // layout__should_clip_no_text checks that in full.
    assert.equal(layout.text, long.pools[0].worktrees[2].branch, 'the branch text is cut in the DOM');
    assert.ok(!layout.clipped, `the long branch is cut at 600 px (text-overflow ${layout.ellipsis})`);
    assert.deepEqual(panel.errors, [], 'page errors');
  });
});

test.describe('legend', () => {
  for (const theme of THEME_NAMES) {
    test(`legend__should_cover_every_activity_value ${theme}`, async ({ page }) => {
      // Given
      const all = mustValidate(everyActivity());
      const panel = await openPanel(page, { theme });
      const colours = await computedThemeColours(page, panel.theme);

      // When
      await panel.show(all);
      const found = await page.evaluate(() =>
        [...document.querySelectorAll('tr[data-row-id]')].map((row) => {
          const cell = row.querySelector(':scope > td');
          const glyph = [cell, ...cell.querySelectorAll('*')].find((el) => {
            const content = getComputedStyle(el, '::before').content;
            return content && content !== 'none' && content !== 'normal';
          });
          const before = glyph ? getComputedStyle(glyph, '::before') : null;
          // An inline SVG icon (the working robot): its shapes are its signature, the fill of
          // every shape its colour.
          const svg = glyph ? null : cell.querySelector('svg');
          const shapes = svg ? [...svg.querySelectorAll('path, circle, rect, ellipse, polygon, line, polyline')] : [];
          const fills = [...new Set(shapes.map((shape) => getComputedStyle(shape).fill))];
          const icon = glyph ?? svg;
          const walker = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT);
          let wordElement = null;
          for (let node = walker.nextNode(); node; node = walker.nextNode()) {
            if (node.textContent.trim()) {
              wordElement = node.parentElement;
              break;
            }
          }
          const masked = before && before.maskImage && before.maskImage !== 'none';
          const bars = [row, cell].flatMap((el) => {
            const style = getComputedStyle(el);
            const found = [];
            if (parseFloat(style.borderLeftWidth) >= 2 && style.borderLeftStyle !== 'none') found.push(style.borderLeftColor);
            for (const match of style.boxShadow.matchAll(/rgba?\([^)]*\)/g)) found.push(match[0]);
            for (const pseudo of ['::before', '::after']) {
              const p = getComputedStyle(el, pseudo);
              if (p.content !== 'none' && p.content !== 'normal' && parseFloat(p.width) <= 6 && parseFloat(p.width) >= 2) {
                found.push(p.backgroundColor);
              }
            }
            return found;
          });
          return {
            id: row.dataset.rowId,
            word: cell.textContent.replace(/\s+/g, ' ').trim(),
            signature: before
              ? [before.content, before.fontFamily, before.maskImage, before.backgroundImage].join(' | ')
              : shapes.length
                ? `svg | ${shapes.map((shape) => shape.outerHTML).join('')}`
                : null,
            iconText: icon && icon !== cell ? icon.textContent.trim() : '',
            iconColour: before ? (masked ? before.backgroundColor : before.color) : fills.length ? fills.join(' and ') : null,
            wordColour: wordElement ? getComputedStyle(wordElement).color : null,
            bars,
          };
        }),
      );

      // Then
      const byPath = new Map(all.pools[0].worktrees.map((w) => [w.path, w]));
      const signatures = new Map();
      for (const row of found) {
        const worktree_ = byPath.get(row.id);
        assert.ok(worktree_, `a row for an unknown worktree ${row.id}`);
        const { activity } = worktree_;
        const groupColour = colours[GROUP_TOKENS[GROUPS[activity]]];
        assert.equal(row.word, WORDS[activity], `${activity}: the word`);
        assert.ok(row.signature, `${activity}: no icon, neither a ::before glyph nor an inline SVG`);
        assert.equal(row.iconText, '', `${activity}: the icon element holds text`);
        assert.equal(row.wordColour, groupColour, `${activity}: the word's colour in ${theme}`);
        assert.equal(row.iconColour, groupColour, `${activity}: the icon's colour in ${theme}`);
        const attention = colours[GROUP_TOKENS.attention];
        if (activity === 'waiting') {
          assert.ok(row.bars.includes(attention), `${activity}: no highlight bar in ${GROUP_TOKENS.attention}: ${row.bars}`);
        } else {
          assert.ok(!row.bars.includes(attention), `${activity}: a highlight bar on a row that does not wait`);
        }
        signatures.set(activity, row.signature);
      }
      assert.equal(signatures.size, Object.keys(WORDS).length, 'a row for every activity value');
      assert.equal(new Set(signatures.values()).size, signatures.size, `activities share an icon: ${JSON.stringify(Object.fromEntries(signatures))}`);
      assert.deepEqual(panel.errors, [], 'page errors');
    });
  }
});

test.describe('axe', () => {
  for (const theme of THEME_NAMES) {
    test(`axe__should_report_no_violation ${theme}`, async ({ browser }) => {
      // Given
      const context = await browser.newContext({ bypassCSP: true });
      const page = await context.newPage();
      const panel = await openPanel(page, { theme });
      await panel.show(mustValidate(everyActivity()));
      await page.locator('tr[data-row-id$="/wt1"]').click();
      await page.clock.runFor(50);

      // When
      const { violations, checked } = await axeRun(page);
      await context.close();

      // Then
      assert.deepEqual(violations, [], `axe in ${theme}`);
      assert.ok(checked > 0, 'setup: axe checked nothing');
    });
  }

  test('axe__should_report_no_violation table_headers', async ({ page }) => {
    // Given
    const panel = await openPanel(page);

    // When
    await panel.show(mustValidate(everyActivity()));
    const headers = await page.evaluate(() => ({
      tables: document.querySelectorAll('table').length,
      columns: [...document.querySelectorAll('table th')].map((th) => [th.getAttribute('scope'), th.textContent.replace(/\s+/g, ' ').trim()]),
    }));

    // Then
    assert.equal(headers.tables, 1, 'the rows sit in one <table>');
    const columns = headers.columns.filter(([scope]) => scope === 'col').map(([, label]) => label.toLowerCase());
    assert.deepEqual(columns.slice(0, 6), ['activity', 'worktree', 'branch', 'git', 'agent · doing', 'last']);
  });
});

test('keyboard__should_reach_every_row_and_action', async ({ page }) => {
  // Given
  const all = mustValidate(everyActivity());
  const panel = await openPanel(page);
  await panel.show(all);
  await page.mouse.move(1, 790);
  const hiddenWhenIdle = await page.evaluate(() =>
    [...document.querySelectorAll('tr[data-row-id] button[data-action]')].filter((b) =>
      b.checkVisibility({ opacityProperty: true, visibilityProperty: true }),
    ).length,
  );

  // When
  const reached = [];
  for (let step = 0; step < 120; step += 1) {
    await page.keyboard.press('Tab');
    await page.clock.runFor(20);
    const focus = await page.evaluate(() => {
      const el = document.activeElement;
      if (!el || el === document.body) return null;
      const style = getComputedStyle(el);
      const visibleFocus =
        (style.outlineStyle !== 'none' && parseFloat(style.outlineWidth) > 0) || style.boxShadow !== 'none';
      return {
        key: el.matches('tr[data-row-id]')
          ? `row ${el.dataset.rowId}`
          : el.matches('button[data-action]')
            ? `${el.dataset.action} ${el.closest('tr[data-row-id]')?.dataset.rowId}`
            : `${el.tagName} ${el.className}`,
        visibleFocus,
        shown: el.checkVisibility({ opacityProperty: true, visibilityProperty: true }),
      };
    });
    if (focus) reached.push(focus);
  }
  const firstRow = page.locator('tr[data-row-id]').first();
  await firstRow.focus();
  await page.keyboard.press('Enter');
  await page.clock.runFor(50);
  const detailAfterEnter = await page.locator('tr.detail').count();
  await page.mouse.move(1, 790);
  await page.locator('tr[data-row-id]').nth(2).hover();
  await page.clock.runFor(50);
  const shownOnHover = await page.locator('tr[data-row-id]').nth(2).locator('button[data-action]').evaluateAll((buttons) =>
    buttons.map((b) => b.checkVisibility({ opacityProperty: true, visibilityProperty: true })),
  );

  // Then
  const keys = new Set(reached.map((r) => r.key));
  const expected = rowOrder(all.pools[0].worktrees).flatMap((w) => [
    `row ${w.path}`,
    ...(w.missing ? ['copyPath'] : ['open', 'terminal', 'copyPath']).map((a) => `${a} ${w.path}`),
    ...(w.role === 'slot' && !w.missing && w.branch !== null ? ['free', 'fg'] : []).map((a) => `${a} ${w.path}`),
  ]);
  assert.deepEqual(expected.filter((key) => !keys.has(key)), [], 'rows and actions Tab does not reach');
  const unseen = reached.filter((r) => !r.visibleFocus).map((r) => r.key);
  assert.deepEqual([...new Set(unseen)], [], 'focus without a visible outline');
  const invisible = reached.filter((r) => !r.shown).map((r) => r.key);
  assert.deepEqual([...new Set(invisible)], [], 'focused elements that are not shown');
  assert.equal(hiddenWhenIdle, 0, 'row actions show without hover or focus');
  assert.ok(shownOnHover.length > 0 && shownOnHover.every(Boolean), 'row actions do not show on hover');
  assert.equal(detailAfterEnter, 1, 'Enter on a focused row shows its detail strip');
  assert.deepEqual(panel.errors, [], 'page errors');
});

test('motion__should_stop_the_spinner', async ({ browser }) => {
  // Given
  // The building dial spins and the working robot swings both arms until motion is reduced. Only
  // the Activity cell is judged, as an agent line's icon never moves, and no animation at all runs
  // under reduce motion.
  const busy = mustValidate(
    document([
      worktree('fg'),
      withActor('wt1', { state: 'working' }),
      worktree('wt2', {
        branch: 'feature-x',
        reasons: ['branch'],
        free: false,
        activity: 'building',
        builds: [{ pid: 4242, name: 'cargo-nextest' }],
      }),
    ]),
  );
  const running = async (extraBodyClass) => {
    const page = await browser.newPage({ viewport: { width: 1100, height: 800 } });
    const panel = await openPanel(page, { extraBodyClass });
    await panel.show(busy);
    await page.clock.runFor(500);
    const animations = await page.evaluate(() =>
      document.getAnimations().map((a) => ({
        state: a.playState,
        row: a.effect?.target?.closest?.('tr')?.dataset?.rowId?.split('/').at(-1) ?? null,
        activity: Boolean(a.effect?.target?.closest?.('tr[data-row-id] > td:first-child')),
        target: `${a.effect?.target?.getAttribute?.('class') ?? a.effect?.target?.tagName}${a.effect?.pseudoElement ?? ''}`,
        duration: a.effect?.getComputedTiming().duration,
      })),
    );
    const arms = await page.evaluate(() =>
      [...document.querySelectorAll('tr[data-row-id] > td:first-child .robot-arm')].map((arm) => getComputedStyle(arm).rotate),
    );
    await page.close();
    return { running: animations.filter((a) => a.state === 'running' && a.duration > 0), arms };
  };

  // When
  const normal = await running('');
  const reduced = await running('vscode-reduce-motion');

  // Then
  const on = (row, target) => normal.running.filter((a) => a.activity && a.row === row && a.target.includes(target));
  assert.equal(on('wt2', '::before').length, 1, `no spinner animates for a building row: ${JSON.stringify(normal.running)}`);
  assert.equal(on('wt1', 'robot-arm-left').length, 1, `the working robot's left arm does not swing: ${JSON.stringify(normal.running)}`);
  assert.equal(on('wt1', 'robot-arm-right').length, 1, `the working robot's right arm does not swing: ${JSON.stringify(normal.running)}`);
  assert.deepEqual(reduced.running, [], 'animations run under vscode-reduce-motion');
  assert.equal(reduced.arms.length, 2, `setup: the working row's Activity cell shows ${reduced.arms.length} robot arms`);
  assert.deepEqual(reduced.arms, ['30deg', '-30deg'], 'the robot arms do not rest at 30 degrees out under vscode-reduce-motion');
});

/** The slices of the CSP check's documents, so each test clicks a slice's rows. */
const CSP_SLICES = 12;

/** Every document the CSP check shows: every scenario document, then every_activity. */
function cspDocuments() {
  const names = readdirSync(SCENARIOS).filter((f) => f.endsWith('.json')).map((f) => f.slice(0, -5)).sort();
  return [...names.flatMap((name) => scenarioDocuments(name).documents), everyActivity()].map(mustValidate);
}

for (let k = 1; k <= CSP_SLICES; k += 1) {
  test(`csp__should_report_no_violation ${k}/${CSP_SLICES}`, async ({ page }) => {
    // Given
    const documents = sliceOf(cspDocuments(), k, CSP_SLICES);
    const panel = await openPanel(page);

    // When
    for (const status of documents) {
      await panel.show(status);
      for (const row of await page.locator('tr[data-row-id]').all()) await row.click();
      await page.clock.runFor(1500);
    }
    const chips = page.locator('.chip:visible');
    if (await chips.count()) await chips.first().click({ timeout: 5000 });
    for (const button of await page.locator('button[data-action]').all()) {
      await button.focus();
      await page.keyboard.press('Enter');
    }
    await page.clock.runFor(5000);
    const violations = await panel.violations();
    const styled = await page.evaluate(() =>
      [...document.querySelectorAll('body [style], body[style], head [style]')].map((el) => el.outerHTML.slice(0, 120)),
    );

    // Then
    assert.ok(documents.length > 0, `setup: slice ${k}/${CSP_SLICES} holds no document`);
    assert.deepEqual(violations, [], 'CSP or Trusted Types violations');
    const consoleCsp = panel.console.filter((line) => /Content Security Policy|Trusted Type|Refused to/i.test(line));
    assert.deepEqual(consoleCsp, [], 'CSP messages on the console');
    assert.deepEqual(styled, [], 'elements with an inline style attribute');
    assert.deepEqual(panel.errors, [], 'page errors');
    console.log(`csp__should_report_no_violation ${k}/${CSP_SLICES}: clicked ${documents.length} documents`);
  });
}

test('csp__should_report_no_violation slices cover every document exactly once', async () => {
  // Given
  const documents = cspDocuments();
  const index = new Map(documents.map((doc, i) => [doc, i]));

  // When
  const measured = Array.from({ length: CSP_SLICES }, (_, i) => sliceOf(documents, i + 1, CSP_SLICES)).flat();

  // Then
  assert.equal(measured.length, documents.length, `setup: the slices hold ${measured.length} documents, not ${documents.length}`);
  assert.equal(new Set(measured.map((doc) => index.get(doc))).size, documents.length, 'setup: a document is in two slices');
});
