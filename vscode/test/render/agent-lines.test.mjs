import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { POOL, actor, ago, document, mustValidate, withActor, worktree } from '../support/docs.mjs';
import { WORDS } from '../support/grammar.mjs';
import { agentFacts, agentProblems } from '../support/jobs-render.mjs';
import { mount, rowById, seen } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

/** wt2 with `actors`, newest first, its activity `activity`. */
function crowded(actors, activity = 'working') {
  return worktree('wt2', {
    branch: 'design-jobs',
    free: false,
    reasons: ['branch'],
    activity,
    actors,
    last_event: actors[0]?.updated ?? null,
  });
}

const five = () => [
  actor({ key: 'a', label: 'Review the TS host', updated: ago(1), action: { tool: 'Read', target: 'vscode/src/host/panel.ts' } }),
  actor({ key: 'b', label: 'Design jobs', updated: ago(8) }),
  actor({ key: 'c', label: 'WT panel build', updated: ago(14), action: { tool: 'Agent', target: 'Fix the harness' } }),
  actor({ key: 'd', label: 'Fix the harness', updated: ago(40), action: { tool: 'Edit', target: 'xtask/src/schedule.rs' } }),
  actor({ key: 'e', kind: 'subagent', label: 'Explore icons', state: 'done', updated: ago(50) }),
];

const row = (name) => rowById(webview.document, `${POOL}/${name}`);

const countCases = [
  ['five_with_a_done_one', crowded(five())],
  ['two', crowded(five().slice(3))],
  ['idle_three', crowded([actor({ key: 'q', state: 'quiet', updated: ago(180) }), actor({ key: 'i', state: 'idle', action: null, updated: ago(900) }), actor({ key: 'r', state: 'idle', action: null, error: 'rate_limit', updated: ago(2400) })], 'idle')],
  ['one', crowded(five().slice(0, 1))],
  ['none', worktree('wt2', { branch: 'b', free: false, reasons: ['branch'], activity: 'unused' })],
  ['build_line', worktree('wt2', { branch: 'b', free: false, reasons: ['branch'], activity: 'building', builds: [{ pid: 4242, name: 'cargo-nextest' }] })],
];

for (const [name, subject] of countCases) {
  test(`count__should_follow_the_agent_lines ${name}`, async () => {
    // Given
    const status = mustValidate(document([worktree('fg'), worktree('wt1'), subject]));
    webview = await mount();

    // When
    webview.show(status);

    // Then
    const facts = agentFacts(webview.window, row('wt2'));
    const n = subject.actors.length;
    const word = WORDS[subject.activity];
    if (n >= 2) {
      assert.equal(facts.count, `×${n}`, `no .count ×${n} after the Activity word (U12)`);
      assert.equal(facts.countHidden, true, 'the count is not hidden from screen readers (U12)');
      assert.equal(facts.activitySpoken, `${word}, ${n} agents`, 'the Activity cell as a screen reader reads it (U12)');
      const cell = row('wt2').querySelector(':scope > td');
      const count = cell.querySelector('.count');
      const words = [...cell.querySelectorAll('*')].filter((el) => !el.contains(count) && seen(webview.window, el) === word);
      assert.ok(words.some((el) => el.compareDocumentPosition(count) & webview.window.Node.DOCUMENT_POSITION_FOLLOWING), 'the count does not follow the word (U12)');
      assert.equal(seen(webview.window, cell, { keepCount: true }).replace(/\s+/g, ''), `${word}×${n}`, 'the Activity cell reads the word and the count, nothing else');
    } else {
      assert.equal(facts.count, null, `a row with ${n} agent lines shows a count (U12)`);
      assert.equal(facts.activitySpoken, word, 'the Activity cell of a row without a count (U12)');
    }
  });
}

test('count__should_follow_the_agent_lines follows_each_document', async () => {
  // Given
  const all = five();
  const docs = [all, all.slice(0, 4), all.slice(0, 1)].map((actors) => mustValidate(document([worktree('fg'), crowded(actors)])));
  webview = await mount();

  // When
  const counts = docs.map((status) => {
    webview.show(status);
    return agentFacts(webview.window, row('wt2')).count;
  });

  // Then
  assert.deepEqual(counts, ['×5', '×4', null], 'the count after each document (U12)');
});

const STATES = [
  ['working', { state: 'working' }],
  ['quiet', { state: 'quiet' }],
  ['waiting', { state: 'waiting', reason: 'permission', action: { tool: 'Bash', target: 'git push' } }],
  ['idle', { state: 'idle', action: null }],
  ['idle_error', { state: 'idle', action: null, error: 'rate_limit' }],
  ['done', { state: 'done', kind: 'subagent', label: 'Explore' }],
  ['queued', { state: 'working', label: 'Design jobs', action: { tool: 'queued:', target: 'cargo test' } }],
];

/** One worktree per case of {@link STATES}, so each line is alone in its row. */
function iconsDocument() {
  const worktrees = [worktree('fg')];
  STATES.forEach(([, overrides], index) => {
    const name = `wt${index + 1}`;
    const one = withActor(name, overrides);
    worktrees.push(overrides.state === 'done' ? { ...one, activity: 'free' } : one);
  });
  worktrees.push(worktree('wt8', { branch: 'b', free: false, reasons: ['branch'], activity: 'building', builds: [{ pid: 4242, name: 'cargo-nextest' }] }));
  worktrees.push(worktree('wt9'));
  return document(worktrees);
}

function firstLine(name) {
  const line = row(name).querySelectorAll(':scope > td')[4]?.querySelector('.line');
  return { line, icon: line?.firstElementChild?.classList.contains('line-icon') ? line.firstElementChild : null };
}

/** An icon's classes, attributes and inner markup without ids: all jsdom can tell of its look. */
function signature(icon) {
  const clone = icon.cloneNode(true);
  for (const el of [clone, ...clone.querySelectorAll('*')]) el.removeAttribute('id');
  return clone.outerHTML;
}

test('line_icons__should_match_each_state', async () => {
  // Given
  const status = mustValidate(iconsDocument());
  webview = await mount();

  // When
  webview.show(status);

  // Then
  const signatures = new Map();
  STATES.forEach(([name], index) => {
    const worktreeName = `wt${index + 1}`;
    const subject = status.pools[0].worktrees.find((w) => w.name === worktreeName);
    const problems = agentProblems(webview.window, row(worktreeName), subject, WORDS[subject.activity]);
    assert.deepEqual(problems, [], `${name}: the line's icon and hidden state (U13)`);
    const { icon } = firstLine(worktreeName);
    assert.equal(seen(webview.window, icon), '', `${name}: the icon holds text`);
    signatures.set(name, signature(icon));
  });
  const states = ['working', 'quiet', 'waiting', 'idle', 'idle_error', 'done'];
  assert.equal(new Set(states.map((s) => signatures.get(s))).size, states.length, `states share a line icon (U13): ${JSON.stringify(Object.fromEntries(signatures))}`);
  assert.equal(signatures.get('queued'), signatures.get('working'), "a queued line's icon is not working's (jobs.md Q8)");
  for (const name of ['wt8', 'wt9']) {
    const { line, icon } = firstLine(name);
    assert.ok(line, `${name}: no .line`);
    assert.ok(icon, `${name}: the build or no agent line keeps no empty .line-icon box first (U13)`);
    assert.equal(icon.textContent, '', `${name}: the empty box holds text`);
    assert.equal(icon.querySelectorAll('*').length, 0, `${name}: the empty box holds an icon`);
    assert.ok(![...signatures.values()].includes(signature(icon)), `${name}: the empty box looks like a state icon`);
  }
});

test('line_icons__should_match_each_state several_lines', async () => {
  // Given
  const status = mustValidate(document([worktree('fg'), crowded(five())]));
  webview = await mount();

  // When
  webview.show(status);

  // Then
  const subject = status.pools[0].worktrees[1];
  assert.deepEqual(agentProblems(webview.window, row('wt2'), subject, WORDS.working), [], 'the count and each line of five agents (U12, U13)');
});
