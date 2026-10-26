import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { POOL, actor, ago, document, mustValidate, worktree } from '../support/docs.mjs';
import { DOT, WORDS } from '../support/grammar.mjs';
import { CONFIG, buttonColours, buttonName, buttonText, busyDocument, firstOpenDocument, job, jobs, withJobs } from '../support/jobs.mjs';
import { accessibleName, mustButton, press, strip } from '../support/jobs-render.mjs';
import { mount, rowById, seen, seenLines, text } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

const plain = () => document([worktree('fg'), worktree('wt1')]);
const busy = busyDocument();
const first = firstOpenDocument();

/** The longest accessible name the machine button may have: its text, then every count by name. */
function fullName(value, machine) {
  const counts = [`${value.running.length} of ${value.limit} running`, `${value.waiting.length} queued`];
  counts.push(`${value.outside.length} not queued`, `${value.suggested.length} suggested`);
  return `${buttonText(value, machine)}, heavy jobs on ${machine}: ${counts.join(', ')}`;
}

/** An element of the button that holds `part` apart from the machine name and `jobs R/L`. */
function ownElement(found, part) {
  return [...found.querySelectorAll('*')].some((el) => {
    const shown = text(el);
    return shown.includes(part) && !shown.includes('jobs') && !shown.includes('verify-mac');
  });
}

const countCases = [
  ['no_job', withJobs(plain(), jobs())],
  ['busy', busy],
  ['off_hides_not_queued', withJobs(busy, { ...busy.machine.jobs, enforce: 'off' })],
  ['require_counts_not_queued', withJobs(busy, { ...busy.machine.jobs, enforce: 'require' })],
  ['suggested_until_a_repository_pattern', first],
  ['machine_pattern_keeps_suggested', withJobs(first, { ...first.machine.jobs, heavy: [{ pattern: 'just', source: 'machine' }] })],
  [
    'repository_pattern_hides_suggested',
    withJobs(first, { ...first.machine.jobs, heavy: [{ pattern: 'make', source: 'repository' }], suggested: first.machine.jobs.suggested.slice(1) }),
  ],
  ['build_processes_hides_suggested', withJobs(first, { ...first.machine.jobs, heavy: [{ pattern: 'protoc', source: 'build_processes' }] })],
  ['over_the_limit', withJobs(plain(), jobs({ limit: 2, running: [job(), job(), job()], waiting: [job()] }))],
];

for (const [name, status] of countCases) {
  test(`heading__should_count_jobs ${name}`, async () => {
    // Given
    mustValidate(status);
    const value = status.machine.jobs;
    webview = await mount();

    // When
    webview.show(status);

    // Then
    const found = mustButton(webview);
    assert.equal(text(found), buttonText(value, status.machine.name), 'the machine button text (U10)');
    const repository = status.pools[0].repository.github ?? status.pools[0].repository.name;
    assert.equal(text(webview.document.querySelector('tr.pool-heading')), `${repository}${DOT}${buttonText(value, status.machine.name)}`, 'the heading text (U8, U10)');
    for (const part of buttonColours(value).attention) {
      assert.ok(ownElement(found, part), `"${part}" is in no element of its own, for the attention colour (U10)`);
    }
  });
}

test('heading__should_count_jobs follows_each_document', async () => {
  // Given
  const idle = mustValidate(withJobs(busy, jobs({ heavy: busy.machine.jobs.heavy })));
  webview = await mount();
  webview.show(mustValidate(busy));
  const before = mustButton(webview);

  // When
  webview.show(idle);
  const after = button();
  const afterText = text(after);
  webview.show(mustValidate(busy));

  // Then
  assert.equal(after, before, 'the machine button was replaced, not updated in place (X15)');
  assert.equal(afterText, buttonText(idle.machine.jobs, 'verify-mac'), 'the button after the jobs ended');
  assert.equal(text(button()), buttonText(busy.machine.jobs, 'verify-mac'), 'the button after the jobs came back');
});

function button() {
  return webview.document.querySelector('tr.pool-heading button.machine-button');
}

test('heading__should_offer_the_machine_button without_jobs', async () => {
  // Given
  const status = mustValidate(plain());
  webview = await mount();

  // When
  webview.show(status);

  // Then
  const found = mustButton(webview);
  assert.equal(text(found), 'verify-mac', 'a wtpool without wtpool job: the name and the chevron (U10)');
  assert.equal(text(webview.document.querySelector('tr.pool-heading')), `acme/app${DOT}verify-mac`, 'the heading text (U8)');
  assert.equal(found.getAttribute('aria-expanded'), 'false', 'aria-expanded of a closed strip');
});

test('heading__should_offer_the_machine_button busy', async () => {
  // Given
  const status = mustValidate(busy);
  webview = await mount();

  // When
  webview.show(status);

  // Then
  const heading = webview.document.querySelector('tr.pool-heading');
  const found = mustButton(webview);
  const repository = heading.querySelector('.repository');
  assert.ok(repository, 'the heading holds no .repository (U8)');
  assert.equal(text(repository), 'acme/app', 'the repository part');
  assert.ok(repository.compareDocumentPosition(found) & webview.window.Node.DOCUMENT_POSITION_FOLLOWING, 'the button comes after the repository');
  assert.equal(found.tagName, 'BUTTON', 'the machine button is no <button>, so Enter and Space would not activate it');
  assert.equal(found.getAttribute('aria-expanded'), 'false', 'aria-expanded of a closed strip');
  assert.equal(strip(webview), null, 'the strip shows before the button is pressed');
  const name = accessibleName(found);
  const value = status.machine.jobs;
  assert.ok(
    [buttonName(value, 'verify-mac'), fullName(value, 'verify-mac')].includes(name),
    `the accessible name "${name}" is not "${buttonName(value, 'verify-mac')}" (U10)`,
  );
});

test('heading__should_offer_the_machine_button config_error', async () => {
  // Given
  const status = mustValidate(withJobs(busy, jobs({ limit: null, error: `${CONFIG}: unknown key limits`, running: [], waiting: [] })));
  webview = await mount();

  // When
  webview.show(status);

  // Then
  const found = mustButton(webview);
  assert.equal(text(found), `verify-mac${DOT}jobs`, 'an unreadable config: the warning glyph and jobs (U10)');
});

for (const [name, act] of [
  ['click', (target) => target.click()],
  ['enter', (target) => press(webview, target, 'Enter')],
  ['space', (target) => press(webview, target, ' ')],
]) {
  test(`heading__should_offer_the_machine_button toggles_by_${name}`, async () => {
    // Given
    webview = await mount();
    webview.show(mustValidate(busy));
    const found = mustButton(webview);

    // When
    act(found);
    webview.tick(20);
    const opened = { strip: Boolean(strip(webview)), expanded: found.getAttribute('aria-expanded') };
    found.focus();
    act(found);
    webview.tick(20);
    const closed = { strip: Boolean(strip(webview)), expanded: found.getAttribute('aria-expanded') };

    // Then
    assert.deepEqual(opened, { strip: true, expanded: 'true' }, `${name} opens the strip (U10, U11)`);
    assert.deepEqual(closed, { strip: false, expanded: 'false' }, `${name} again closes it`);
  });
}

/**
 * An agent's build and a row's own build outside a job, and a queued build. Without `enforce` the
 * document has no `machine.jobs`, as an older wtpool prints it.
 */
function buildsDocument(enforce) {
  const wt1 = worktree('wt1', {
    branch: 'design-jobs',
    free: false,
    reasons: ['branch'],
    activity: 'working',
    actors: [actor({ key: 's1', label: 'session 10:02', action: { tool: 'build:', target: 'cargo test' }, updated: ago(120) })],
    builds: [{ pid: 5101, name: 'cargo test', queued: false }],
    last_event: ago(120),
  });
  const wt2 = worktree('wt2', { branch: 'fix-tail', free: false, reasons: ['branch'], activity: 'building', builds: [{ pid: 5202, name: 'make', queued: false }] });
  const wt3 = worktree('wt3', { branch: 'review-ts', free: false, reasons: ['branch'], activity: 'building', builds: [{ pid: 5303, name: 'cargo nextest', queued: true }] });
  const status = document([worktree('fg'), wt1, wt2, wt3, worktree('wt4')]);
  if (enforce === undefined) return status;
  return withJobs(
    status,
    jobs({
      enforce,
      running: [job({ summary: 'cargo nextest', worktree: `${POOL}/wt3` })],
      outside: [job({ summary: 'cargo test', worktree: `${POOL}/wt1`, actor: 's1' }), job({ summary: 'make', worktree: `${POOL}/wt2` })],
    }),
  );
}

/** The seen agent line of the row of `name`, and its `.not-queued` texts. */
function buildLine(name) {
  const row = rowById(webview.document, `${POOL}/${name}`);
  const cell = row.querySelectorAll(':scope > td')[4];
  return { line: seenLines(webview.window, cell)[0], marks: [...cell.querySelectorAll('.not-queued')].map(text) };
}

for (const enforce of ['warn', 'require']) {
  test(`not_queued__should_mark_builds_outside_jobs ${enforce}`, async () => {
    // Given
    const status = mustValidate(buildsDocument(enforce));
    webview = await mount();

    // When
    webview.show(status);

    // Then
    assert.deepEqual(
      ['wt1', 'wt2', 'wt3'].map(buildLine),
      [
        { line: `session 10:02${DOT}build: cargo test (not queued)`, marks: ['(not queued)'] },
        { line: 'build: make (not queued)', marks: ['(not queued)'] },
        { line: 'build: cargo nextest', marks: [] },
      ],
      `the build lines under ${enforce} (A8, U14): an actor's build and a row's own build outside a job, a queued build`,
    );
  });
}

for (const [name, status] of [
  ['off', buildsDocument('off')],
  ['old_wtpool', buildsDocument(undefined)],
]) {
  test(`not_queued__should_mark_builds_outside_jobs ${name}`, async () => {
    // Given
    mustValidate(status);
    webview = await mount();

    // When
    webview.show(status);

    // Then
    assert.deepEqual(
      ['wt1', 'wt2', 'wt3'].map(buildLine),
      [
        { line: `session 10:02${DOT}build: cargo test`, marks: [] },
        { line: 'build: make', marks: [] },
        { line: 'build: cargo nextest', marks: [] },
      ],
      `the build lines ${name === 'off' ? 'under off' : 'without machine.jobs'}: nothing is flagged (A8)`,
    );
    const marked = buildsDocument('warn');
    webview.show(mustValidate(marked));
    assert.equal(buildLine('wt2').line, 'build: make (not queued)', 'the same build outside a job under warn (A8, U14)');
  });
}

test('not_queued__should_mark_builds_outside_jobs queued_actor', async () => {
  // Given
  const status = mustValidate(busy);
  webview = await mount();

  // When
  webview.show(status);

  // Then
  const row = rowById(webview.document, `${POOL}/wt1`);
  const [activity, , , , agents] = row.querySelectorAll(':scope > td');
  assert.equal(seen(webview.window, activity), WORDS.working, 'the worktree of a queued agent');
  assert.ok(
    seenLines(webview.window, agents).includes(`Design jobs${DOT}queued: cargo test`),
    `no line "Design jobs · queued: cargo test" (A8): ${seenLines(webview.window, agents)}`,
  );
  assert.equal(agents.querySelectorAll('.not-queued').length, 0, 'a queued job reads (not queued)');
});
