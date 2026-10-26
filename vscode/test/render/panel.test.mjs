// U3, U4, U5, X2, X6, X10, X14 and X15: the panel's behaviour in jsdom.
import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import {
  ago,
  actor,
  document,
  everyActivity,
  git,
  mustValidate,
  view,
  withActor,
  worktree,
} from '../support/docs.mjs';
import { chipCounts, normalizeEscapes, rowOrder, visible } from '../support/grammar.mjs';
import { cells, isShown, mount, rowById, rows, seen, text } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

/**
 * Whether a row action is offered: present, not hidden and not disabled. Row actions show on
 * hover and focus only (U4), which jsdom does not compute; shot checks that part.
 */
function isOffered(button) {
  for (let node = button; node && node.nodeType === 1; node = node.parentElement) {
    if (node.hidden || node.getAttribute('aria-hidden') === 'true' || node.hasAttribute('inert')) return false;
  }
  return !button.disabled && button.getAttribute('aria-disabled') !== 'true';
}

function chip(document_, word) {
  return [...document_.querySelectorAll('.chip')].find((c) => new RegExp(`\\b${word}\\b`).test(text(c)));
}

function shownChipText(word) {
  const found = chip(webview.document, word);
  return found && isShown(webview.window, found) ? text(found) : null;
}

test('chips__should_count_and_scroll_to_waiting', async () => {
  // Given
  const all = mustValidate(everyActivity());
  const twoWaiting = mustValidate(
    document([
      worktree('fg'),
      withActor('wt3', { state: 'waiting', reason: 'question', action: null }),
      withActor('wt1', { state: 'waiting', reason: 'permission' }),
      worktree('wt2', { branch: 'b', free: false, reasons: ['branch'], activity: 'unused' }),
    ]),
  );
  const noneBusy = mustValidate(document([worktree('fg'), worktree('wt1', { branch: 'b', free: false, reasons: ['branch'], activity: 'unused' })]));
  webview = await mount();

  // When
  webview.show(all);
  const counted = {
    free: shownChipText('free'),
    working: shownChipText('working'),
    waiting: shownChipText('waiting'),
  };
  webview.show(twoWaiting);
  const waitingChip = chip(webview.document, 'waiting');
  const twoText = shownChipText('waiting');
  const hiddenWhenTwo = { free: shownChipText('free'), working: shownChipText('working') };
  waitingChip?.click();
  webview.tick(20);
  const focused = webview.document.activeElement;
  webview.show(noneBusy);
  const hiddenWhenNone = shownChipText('waiting');

  // Then
  const expected = chipCounts(all.pools[0].worktrees);
  assert.deepEqual(counted, {
    free: `${expected.free} free`,
    working: `${expected.working} working`,
    waiting: `${expected.waiting} waiting`,
  });
  assert.equal(twoText, '2 waiting');
  assert.deepEqual(hiddenWhenTwo, { free: null, working: null }, 'chips hide at 0');
  assert.equal(hiddenWhenNone, null, 'the waiting chip hides at 0');
  assert.equal(waitingChip?.tagName, 'BUTTON', 'the waiting chip is a button');
  const firstWaiting = rowOrder(twoWaiting.pools[0].worktrees).find((w) => w.activity === 'waiting');
  assert.equal(
    focused?.getAttribute?.('data-row-id'),
    firstWaiting.path,
    'the waiting chip focuses the first waiting row',
  );
});

test('actions__should_match_the_row_kind', async () => {
  // Given
  const status = mustValidate(everyActivity());
  webview = await mount();
  webview.show(status);
  const before = webview.posted.length;

  // When
  const found = rows(webview.document).map((row) => {
    const { actions } = cells(row);
    const buttons = actions ? [...actions.querySelectorAll('button[data-action]')].filter(isOffered) : [];
    for (const button of buttons) button.click();
    return {
      id: row.getAttribute('data-row-id'),
      actions: buttons.map((b) => b.getAttribute('data-action')).sort(),
    };
  });
  webview.tick(20);
  const messages = webview.posted.slice(before);

  // Then
  // U4's three actions, copy path only when missing; from phase 3 (X11, X12) free and fg on
  // each slot that holds a branch, and nowhere outside the slots that are there.
  const expected = status.pools[0].worktrees.map((w) => {
    const required = w.missing ? ['copyPath'] : ['copyPath', 'open', 'terminal'];
    const slotWithBranch = w.role === 'slot' && !w.missing && w.branch !== null;
    if (slotWithBranch) required.push('fg', 'free');
    const allowed = w.role === 'slot' && !w.missing ? ['fg', 'free'] : [];
    const shown = found.find((row) => row.id === w.path)?.actions ?? [];
    const extra = shown.filter((a) => allowed.includes(a) && !required.includes(a));
    return { id: w.path, actions: [...required, ...extra].sort() };
  });
  const byId = (a, b) => (a.id < b.id ? -1 : 1);
  assert.deepEqual([...found].sort(byId), [...expected].sort(byId));
  const expectedMessages = found.flatMap((row) =>
    row.actions.map((action) => ({ type: action, id: row.id })),
  );
  const key = (m) => JSON.stringify(m);
  assert.deepEqual(
    messages.map(key).sort(),
    expectedMessages.map(key).sort(),
    'each button posts exactly {type, id}',
  );
});

test('detail__should_show_every_fact_and_actor', async () => {
  // Given
  const previousTz = process.env.TZ;
  process.env.TZ = 'Asia/Kolkata';
  const waiting = actor({
    label: 'session 13:41',
    state: 'waiting',
    reason: 'permission',
    action: { tool: 'Bash', target: 'git push' },
    placed_by: 'path',
    since: '2026-10-20T08:11:00Z',
    updated: ago(2),
  });
  const done = actor({
    kind: 'subagent',
    label: 'test',
    state: 'done',
    action: { tool: 'Edit', target: 'src/resolve.rs' },
    placed_by: 'cwd',
    since: '2026-10-20T08:17:00Z',
    updated: ago(720),
  });
  const subject = worktree('wt2', {
    branch: '5678-signed-release-notes',
    upstream: 'origin/5678-signed-release-notes',
    free: false,
    reasons: ['branch', 'dirty'],
    git: git({ dirty: 3, ahead: 2, behind: 0, unreferenced: 1, operation: 'merge' }),
    activity: 'waiting',
    actors: [waiting, done],
    last_event: ago(2),
  });
  const status = mustValidate(document([worktree('fg'), worktree('wt1'), subject]));
  try {
    webview = await mount();
    webview.show(status);

    // When
    rowById(webview.document, subject.path).click();
    webview.tick(20);

    // Then
    const row = rowById(webview.document, subject.path);
    const detail = webview.document.querySelector('tr.detail');
    assert.ok(detail, 'no tr.detail after selecting a row');
    assert.equal(row.nextElementSibling, detail, 'the detail strip follows the selected row');
    const shown = text(detail);
    const wanted = [
      subject.path,
      subject.upstream,
      '3 dirty',
      '2 ahead',
      '0 behind',
      '1 unreferenced',
      'merge',
      'branch',
      'dirty',
      'session 13:41',
      'waiting',
      'permission',
      'Bash git push',
      'path',
      '13:41',
      'test',
      'done',
      'Edit src/resolve.rs',
      'cwd',
      '13:47',
    ];
    const absent = wanted.filter((fact) => !shown.includes(fact));
    assert.deepEqual(absent, [], `facts missing from the detail strip: ${shown}`);
    assert.ok(!shown.includes('08:11'), 'the detail strip shows since in UTC, not local time');
  } finally {
    process.env.TZ = previousTz;
  }
});

/**
 * The paused headers that count down to `retryAt` correctly at the clock's time: N is the
 * whole seconds left, rounded down or up.
 */
function pausedHeaders(window, retryAt) {
  const left = (Date.parse(retryAt) - window.Date.now()) / 1000;
  return [...new Set([Math.floor(left), Math.ceil(left)])].map((n) => `updates paused, retrying in ${n} s`);
}

test('header__should_show_live_updated_and_paused', async () => {
  // Given
  const status = mustValidate(document([worktree('fg'), worktree('wt1')]));
  webview = await mount();
  const header = () => text(webview.document.querySelector('.connection'));

  // When
  webview.post(view(status, { connection: 'live', lastLine: ago(1) }));
  const live = header();
  webview.post(view(status, { connection: 'stale', lastLine: ago(17) }));
  const stale = header();
  webview.tick(3000);
  const later = header();
  webview.post(view(status, { connection: 'paused', lastLine: ago(40), retryAt: ago(-8) }));
  const paused = header();
  const pausedWanted = pausedHeaders(webview.window, ago(-8));
  webview.tick(3000);
  const pausedLater = header();
  const pausedLaterWanted = pausedHeaders(webview.window, ago(-8));

  // Then
  assert.equal(live, 'live');
  assert.equal(stale, 'updated 17 s ago');
  assert.equal(later, 'updated 20 s ago', 'the stale header counts from the last line');
  assert.ok(pausedWanted.includes(paused), `N is not the time to retry_at, 8 s: ${paused}`);
  assert.ok(pausedLaterWanted.includes(pausedLater), `N does not count down to retry_at, 5 s: ${pausedLater}`);
});

test('header__should_show_paused_before_the_first_document', async () => {
  // Given
  webview = await mount();
  const header = () => text(webview.document.querySelector('.connection'));

  // When
  webview.post(view(null, { connection: 'paused', lastLine: ago(2), retryAt: ago(-4) }));
  const paused = header();
  const pausedWanted = pausedHeaders(webview.window, ago(-4));
  const tables = webview.document.querySelectorAll('table').length;
  const shownRows = rows(webview.document).length;
  webview.tick(2000);
  const later = header();
  const laterWanted = pausedHeaders(webview.window, ago(-4));
  const status = mustValidate(document([worktree('fg'), worktree('wt1')]));
  webview.post(view(status, { connection: 'live', lastLine: ago(0) }));
  const recovered = { header: header(), rows: rows(webview.document).length };

  // Then
  assert.deepEqual(webview.errors.map(String), [], 'a paused view without a document raised errors');
  assert.ok(pausedWanted.includes(paused), `no paused header counting to retry_at before the first document: ${paused}`);
  assert.ok(laterWanted.includes(later), `the paused header does not count down: ${later}`);
  assert.equal(tables, 1, 'the paused header does not sit over the table');
  assert.equal(shownRows, 0, 'rows show before the first document');
  assert.deepEqual(recovered, { header: 'live', rows: 2 }, 'the first document after the fault does not show');
});

test('header__should_show_starting_before_the_first_line', async () => {
  // Given
  webview = await mount();
  const header = () => text(webview.document.querySelector('.connection'));
  const page = () => ({
    header: header(),
    headerShown: isShown(webview.window, webview.document.querySelector('.connection')),
    chips: [...webview.document.querySelectorAll('.chip')].filter((c) => isShown(webview.window, c)).length,
    error: isShown(webview.window, webview.document.querySelector('.wtpool-error')),
    tables: webview.document.querySelectorAll('table').length,
    rows: rows(webview.document).length,
  });
  const empty = { header: 'starting…', headerShown: true, chips: 0, error: false, tables: 1, rows: 0 };

  // When
  const beforeAnyView = page();
  webview.tick(5000);
  const stillNoView = page();
  webview.post(view(null, { connection: 'starting', lastLine: ago(0) }));
  const starting = page();
  webview.tick(20_000);
  const later = page();
  const status = mustValidate(document([worktree('fg'), worktree('wt1')]));
  webview.post(view(status, { connection: 'live', lastLine: ago(0) }));
  const live = { header: header(), rows: rows(webview.document).length };

  // Then
  assert.deepEqual(webview.errors.map(String), [], 'the page raised errors before its first document');
  assert.deepEqual(beforeAnyView, empty, 'the page shows no starting header before any view');
  assert.deepEqual(stillNoView, empty, 'the starting header changed while no view came');
  assert.deepEqual(starting, empty, 'the starting view shows no starting header over the empty table');
  assert.deepEqual(later, empty, 'the starting header turned into an age or a countdown');
  assert.deepEqual(live, { header: 'live', rows: 2 }, 'the first document after the starting view does not show');
});

test('live_region__should_announce_only_waiting', async () => {
  // Given
  const quiet = mustValidate(document([worktree('fg'), worktree('wt1'), worktree('wt2'), worktree('wt3')]));
  const wt2Waits = mustValidate(
    document([worktree('fg'), worktree('wt1'), withActor('wt2', { state: 'waiting', reason: 'permission' }), worktree('wt3')]),
  );
  const wt3Works = mustValidate(
    document([
      worktree('fg'),
      worktree('wt1'),
      withActor('wt2', { state: 'waiting', reason: 'permission' }),
      withActor('wt3', { state: 'working' }),
    ]),
  );
  const wt3Waits = mustValidate(
    document([
      worktree('fg'),
      worktree('wt1'),
      withActor('wt2', { state: 'waiting', reason: 'permission' }),
      withActor('wt3', { state: 'waiting', reason: 'question', action: null }),
    ]),
  );
  webview = await mount();
  const regions = () =>
    [...webview.document.querySelectorAll('[aria-live="polite"], [role="status"]')];
  const announced = () => regions().map(text).join(' ');

  // When
  webview.show(quiet);
  const atStart = announced();
  webview.show(wt2Waits);
  const afterWait = announced();
  webview.show(wt3Works);
  const afterWork = announced();
  webview.show(wt3Waits);
  const afterSecondWait = announced();

  // Then
  assert.ok(regions().length >= 1, 'no polite live region');
  assert.ok(
    regions().every((r) => r.getAttribute('aria-live') !== 'assertive'),
    'the live region is polite',
  );
  assert.equal(atStart, '', 'nothing to announce without waits');
  assert.match(afterWait, /wt2/, 'a new wait in wt2 is announced');
  assert.ok(!/wt3/.test(afterWork), `working is announced: ${afterWork}`);
  assert.ok(afterWork === afterWait || afterWork === '', `a working change changed the announcement: ${afterWork}`);
  assert.match(afterSecondWait, /wt3/, 'a new wait in wt3 is announced');
});

test('update__should_keep_focus_selection_and_detail', async () => {
  // Given
  const first = mustValidate(document([worktree('fg'), worktree('wt1'), worktree('wt2', { last_change: ago(100) }), worktree('wt3')]));
  const second = mustValidate(
    document([
      worktree('fg'),
      withActor('wt1', { state: 'working' }),
      worktree('wt2', {
        free: false,
        reasons: ['dirty'],
        git: git({ dirty: 5 }),
        activity: 'not_reusable',
        last_change: ago(1),
      }),
      worktree('wt3'),
    ]),
  );
  webview = await mount();
  webview.show(first);
  const wt2 = rowById(webview.document, first.pools[0].worktrees[2].path);
  const wt3 = rowById(webview.document, first.pools[0].worktrees[3].path);
  wt2.click();
  webview.tick(20);
  wt3.focus();
  const rowsBefore = rows(webview.document);
  const focusBefore = webview.document.activeElement;

  // When
  webview.show(second);

  // Then
  const rowsAfter = rows(webview.document);
  assert.equal(focusBefore, wt3, 'setup: wt3 has focus');
  assert.ok(rowsBefore.every((row, index) => row === rowsAfter[index]), 'rows were replaced');
  assert.equal(webview.document.activeElement, wt3, 'focus left wt3');
  const detail = webview.document.querySelector('tr.detail');
  assert.ok(detail, 'the detail strip closed');
  assert.equal(rowById(webview.document, second.pools[0].worktrees[2].path).nextElementSibling, detail);
  assert.ok(text(detail).includes('5 dirty'), `the detail strip did not update: ${text(detail)}`);
  assert.equal(
    normalizeEscapes(text(cells(rowsAfter[2]).data[3])),
    '5 dirty',
    'the selected row did not update in place',
  );
});

const HOSTILE = {
  markup: '<img src=x onerror=alert(1)><script>alert(2)</script>',
  link: 'command:workbench.action.reloadWindow',
  bidi: 'fix‮txt.exe',
  zeroWidth: 'ma​in',
  controls: 'a\u0007b\u001b[31mred\u001b[0m\nnext',
  ampersand: '&amp;&lt;b&gt;',
};

test('render__should_escape_and_isolate_controls', async () => {
  // Given
  const hostileActor = actor({
    label: HOSTILE.markup,
    state: 'idle',
    action: null,
    error: HOSTILE.controls,
  });
  const linkActor = actor({ label: HOSTILE.link, action: { tool: 'Read', target: HOSTILE.bidi } });
  const status = mustValidate(
    document([
      worktree('fg', { branch: HOSTILE.markup, upstream: HOSTILE.ampersand }),
      worktree('wt1', { branch: HOSTILE.bidi, free: false, reasons: ['branch'], activity: 'unused' }),
      worktree('wt2', { branch: HOSTILE.zeroWidth, free: false, reasons: ['branch'], activity: 'idle', actors: [hostileActor] }),
      worktree('wt3', { branch: HOSTILE.link, free: false, reasons: ['branch'], activity: 'working', actors: [linkActor] }),
      worktree('x', { branch: HOSTILE.controls }),
    ]),
  );
  webview = await mount();

  // When
  webview.show(status);
  for (const row of rows(webview.document)) row.click();
  webview.tick(20);

  // Then
  const doc = webview.document;
  assert.equal(doc.querySelectorAll('img, iframe, object, embed, a, form').length, 0, 'markup or links from strings');
  assert.equal(doc.querySelectorAll('body script:not([src])').length, 0, 'an inline script appeared');
  const handlers = [...doc.querySelectorAll('*')].flatMap((el) =>
    [...el.attributes].filter((a) => a.name.startsWith('on')).map((a) => `${el.tagName} ${a.name}`),
  );
  assert.deepEqual(handlers, [], 'event handler attributes appeared');
  const branches = rows(doc).map((row) => normalizeEscapes(text(cells(row).data[2])));
  assert.deepEqual(
    branches,
    rowOrder(status.pools[0].worktrees).map((w) => normalizeEscapes(visible(w.branch).replace(/\s+/g, ' ').trim())),
    'branches show literally, controls, bidi and zero width as \\u{XXXX}',
  );
  const all = normalizeEscapes(doc.body.textContent);
  assert.ok(!/[\u0000-\u0008\u000b-\u001f\u007f‪-‮⁦-⁩​-‏⁠﻿]/u.test(all), 'a raw control, bidi or zero width character reached the page');
  assert.ok(all.includes(normalizeEscapes(visible(HOSTILE.markup))), 'the markup label is not shown literally');
  assert.ok(all.includes('API error: '), 'the error line is missing');
  const isolating = (element) => {
    if (element.tagName === 'BDI' || element.hasAttribute('dir')) return true;
    const bidi = webview.window.getComputedStyle(element).unicodeBidi;
    return ['isolate', 'isolate-override', 'plaintext'].includes(bidi);
  };
  const isolated = (element) =>
    isolating(element) ||
    [...element.querySelectorAll('*')].some(isolating) ||
    (element.parentElement !== null && element.parentElement.tagName !== 'TR' && isolating(element.parentElement));
  const textCells = rows(doc).flatMap((row) => cells(row).data.slice(1, 5));
  const notIsolated = textCells
    .flatMap((cell) => {
      const parts = [...cell.querySelectorAll('.line')];
      return parts.length ? parts : [cell];
    })
    .filter((part) => !isolated(part))
    .map((part) => part.outerHTML.slice(0, 120));
  assert.deepEqual(notIsolated, [], 'text cells are not bidi isolated');
});

/**
 * The line and paragraph separators and format characters beyond the bidi and zero width ones:
 * Arabic number signs and the end of ayah, the Syriac abbreviation mark, the deprecated format
 * controls, the interlinear annotation anchor, the Kaithi number sign, a musical symbol format
 * character and the language tag.
 */
const SEPARATORS_AND_FORMAT = [' ', ' ', '؀', '۝', '܏', '࣢', '⁪', '￹', '\u{110bd}', '\u{1d173}', '\u{e0001}'];

/** `text` with each character of SEPARATORS_AND_FORMAT as X10 writes it, `\u{XXXX}`. */
function escapedFormat(text) {
  return [...text]
    .map((c) => (SEPARATORS_AND_FORMAT.includes(c) ? `\\u{${c.codePointAt(0).toString(16).toUpperCase().padStart(4, '0')}}` : c))
    .join('');
}

test('render__should_escape_and_isolate_controls separators_and_format', async () => {
  // Given
  const hidden = `main${SEPARATORS_AND_FORMAT.join('x')}end`;
  const status = mustValidate(
    document([
      worktree('fg'),
      worktree('wt1', { branch: hidden, free: false, reasons: ['branch'], activity: 'working', actors: [actor({ label: hidden })] }),
    ]),
  );
  webview = await mount();

  // When
  webview.show(status);
  const wt1 = rows(webview.document).find((row) => row.getAttribute('data-row-id') === status.pools[0].worktrees[1].path);
  wt1?.click();
  webview.tick(20);

  // Then
  const { data } = cells(wt1);
  assert.equal(normalizeEscapes(text(data[2])), normalizeEscapes(escapedFormat(hidden)), 'separators or format characters in a branch do not show as \\u{XXXX}');
  assert.ok(normalizeEscapes(seen(webview.window, data[4])).startsWith(normalizeEscapes(escapedFormat(hidden))), 'separators or format characters in an agent label do not show as \\u{XXXX}');
  const page = webview.document.body.textContent;
  const raw = SEPARATORS_AND_FORMAT.filter((c) => page.includes(c)).map((c) => c.codePointAt(0).toString(16));
  assert.deepEqual(raw, [], 'a raw separator or format character reached the page');
});

const hostileStates = [
  ['markup', { selected: '<img src=x onerror=alert(1)>' }],
  ['proto', JSON.parse('{"__proto__": {"polluted": true}, "selected": "__proto__"}')],
  ['unknown_row', { selected: '/no/such/worktree' }],
  ['wrong_type', { selected: { path: '/Users/verify/pool/wt1' } }],
  ['number', 42],
  ['array', ['/Users/verify/pool/wt1']],
  ['huge', { selected: 'x'.repeat(5_000_000) }],
  ['null', null],
];

test.each(hostileStates)('restore__should_ignore_hostile_state %s', async (_, state) => {
  // Given
  const status = mustValidate(document([worktree('fg'), worktree('wt1'), worktree('wt2')]));
  webview = await mount({ state });

  // When
  webview.show(status);

  // Then
  assert.equal(rows(webview.document).length, 3, 'the panel did not render under a hostile state');
  assert.equal(webview.document.querySelector('tr.detail'), null, 'a hostile state selected a row');
  assert.equal(webview.document.querySelectorAll('img, body script:not([src])').length, 0, 'markup from the state');
  assert.equal({}.polluted, undefined, 'the state polluted Object.prototype');
  assert.equal(webview.window.Object.prototype.polluted, undefined, 'the state polluted the webview');
  assert.deepEqual(webview.errors.map(String), [], 'the webview threw');
});

test('restore__should_ignore_hostile_state keeps_only_the_selected_row', async () => {
  // Given
  const status = mustValidate(document([worktree('fg'), worktree('wt1'), worktree('wt2')]));
  const target = status.pools[0].worktrees[1].path;
  webview = await mount();
  webview.show(status);

  // When
  rowById(webview.document, target).click();
  webview.tick(20);
  const saved = webview.state();
  webview.close();
  webview = await mount({ state: saved });
  webview.show(status);

  // Then
  const values = saved !== null && typeof saved === 'object' ? Object.values(saved) : [saved];
  assert.deepEqual(values, [target], `the saved state holds more than the selected row: ${JSON.stringify(saved)}`);
  assert.equal(
    rowById(webview.document, target)?.nextElementSibling,
    webview.document.querySelector('tr.detail'),
    'the restored state did not select the row',
  );
});
