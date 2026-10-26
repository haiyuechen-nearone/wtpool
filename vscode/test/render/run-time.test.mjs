// The run time of a background task in jsdom, on the fake clock of support/webview.mjs: the agent
// line whose action is `background:` ends in ` · <run time>` of the actor's first listed task,
// and the Output section's heading of a running task reads `running · <run time>`. The time ticks
// each second; the Follow message, the accessible names and the live region never hold it.
import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { actor, ago, document, mustValidate, POOL, worktree } from '../support/docs.mjs';
import { cells, mount, rowById, seenLines, spoken, text } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

const TASKS = '/private/tmp/claude-501/-Users-verify-pool/s1/tasks';
const LABEL = 'release agent';
/** Any run time, so a name or an announcement that holds one shows. */
const ANY_RUN_TIME = /\b\d+ [smhd]\b/;

function task(id, extra = {}) {
  return { task: id, path: `${TASKS}/${id}.output`, summary: 'until', running: true, started: ago(300), ended: null, ...extra };
}

/** An agent whose turn ended while its first task, `outputs[0]`, still runs. */
function waiting(outputs, overrides = {}) {
  return actor({ label: LABEL, action: { tool: 'background:', target: outputs[0].summary }, outputs, updated: ago(20), ...overrides });
}

function busy(name, actors) {
  return worktree(name, { branch: `${name}-branch`, free: false, reasons: ['branch'], activity: 'working', actors, last_event: actors[0].updated });
}

const row = (name) => rowById(webview.document, `${POOL}/${name}`);
const agentLines = (name) => seenLines(webview.window, cells(row(name)).data[4]);
const heading = (id) => text(webview.document.querySelector(`tr.detail .output[data-output-path="${TASKS}/${id}.output"] .output-heading`));

/** Mounts the webview on `status` with the row `name` selected, so its Output section shows. */
async function showSelected(status, name) {
  webview = await mount();
  webview.show(status);
  row(name).click();
}

const formatCases = [
  ['0_s', 0, '0 s'],
  ['4_s', 4, '4 s'],
  ['59_s', 59, '59 s'],
  ['60_s', 60, '1 m'],
  ['59_m', 3599, '59 m'],
  ['1_h', 3600, '1 h'],
  ['1_h_0_m', 3659, '1 h'],
  ['1_h_1_m', 3660, '1 h 1 m'],
  ['3_h_12_m', 3 * 3600 + 12 * 60, '3 h 12 m'],
  ['47_h_59_m', 48 * 3600 - 1, '47 h 59 m'],
  ['48_h', 48 * 3600, '2 d'],
  ['2_d_0_h', 2 * 86400 + 3599, '2 d'],
  ['2_d_5_h', 2 * 86400 + 5 * 3600, '2 d 5 h'],
  ['started_in_the_future', -90, '0 s'],
];

for (const [name, seconds, expected] of formatCases) {
  test(`run_time__should_use_the_format ${name}`, async () => {
    // Given
    const status = mustValidate(document([worktree('fg'), busy('wt2', [waiting([task('b1', { started: ago(seconds) })])])]));

    // When
    await showSelected(status, 'wt2');

    // Then
    const [line] = agentLines('wt2');
    assert.equal(line, `${LABEL} · background: until · ${expected}`, `the agent line after ${seconds} s`);
    const cell = cells(row('wt2')).data[4];
    assert.ok(spoken(cell.querySelector('.line')).endsWith(` · ${expected}`), `a screen reader does not read the run time as shown: ${spoken(cell)}`);
    assert.equal(heading('b1'), `until · running · ${expected}`, `the Output heading after ${seconds} s`);
    assert.deepEqual(webview.errors, []);
  });
}

test('run_time__should_tick_each_second', async () => {
  // Given
  const status = mustValidate(
    document([
      worktree('fg'),
      busy('wt1', [waiting([task('b1', { started: ago(58) })])]),
      busy('wt2', [waiting([task('b2', { started: ago(3599) })])]),
      busy('wt3', [waiting([task('b3', { started: ago(-2) })])]),
    ]),
  );
  await showSelected(status, 'wt1');
  const postedBefore = webview.posted.length;
  const read = () => ({ wt1: agentLines('wt1')[0], heading: heading('b1'), wt2: agentLines('wt2')[0], wt3: agentLines('wt3')[0] });
  const shown = [read()];

  // When
  for (let second = 0; second < 3; second += 1) {
    webview.tick(1000);
    shown.push(read());
  }

  // Then
  const at = (wt1, wt2, wt3) => ({
    wt1: `${LABEL} · background: until · ${wt1}`,
    heading: `until · running · ${wt1}`,
    wt2: `${LABEL} · background: until · ${wt2}`,
    wt3: `${LABEL} · background: until · ${wt3}`,
  });
  assert.deepEqual(shown, [at('58 s', '59 m', '0 s'), at('59 s', '1 h', '0 s'), at('1 m', '1 h', '0 s'), at('1 m', '1 h', '1 s')], 'the run times over three seconds of ticks');
  assert.equal(webview.posted.length, postedBefore, 'ticking posted messages');
});

test('run_time__should_follow_the_named_task', async () => {
  // Given
  const ended = ago(60);
  const background = waiting([
    task('b3', { summary: 'npm run dev', started: ago(30) }),
    task('b2', { summary: 'cargo build', started: ago(7200) }),
    task('b1', { summary: 'cargo test', running: false, started: ago(4000), ended }),
  ]);
  const foreground = actor({ label: 'busy agent', action: { tool: 'Bash', target: 'cargo nextest' }, outputs: [task('b4', { summary: 'npm ci', started: ago(90) })], updated: ago(10) });
  const status = mustValidate(document([worktree('fg'), busy('wt1', [foreground]), busy('wt2', [background])]));
  const endedAt = new Date(Date.parse(ended));
  const clock = `${String(endedAt.getHours()).padStart(2, '0')}:${String(endedAt.getMinutes()).padStart(2, '0')}`;

  // When
  await showSelected(status, 'wt2');
  const backgroundLine = agentLines('wt2')[0];
  const headings = ['b3', 'b2', 'b1'].map(heading);
  row('wt1').click();
  const foregroundLine = agentLines('wt1')[0];
  const foregroundHeading = heading('b4');

  // Then
  assert.equal(backgroundLine, `${LABEL} · background: npm run dev · 30 s`, 'the line does not give the run time of its first listed task');
  assert.deepEqual(headings, ['npm run dev · running · 30 s', 'cargo build · running · 2 h', `cargo test · ended ${clock}`], 'each running task gives its own run time, an ended one none');
  assert.equal(foregroundLine, 'busy agent · Bash cargo nextest', 'a line whose action is not background: shows a run time');
  assert.equal(foregroundHeading, 'npm ci · running · 1 m', 'a running task of an agent at work does not give its run time');
  assert.deepEqual(webview.errors, []);
});

test('run_time__should_leave_names_and_announcements_unchanged', async () => {
  // Given
  const quiet = mustValidate(document([worktree('fg'), busy('wt2', [waiting([task('b1', { started: ago(125) })])]), worktree('wt3')]));
  const wt3Waits = structuredClone(quiet);
  wt3Waits.pools[0].worktrees[2] = worktree('wt3', {
    branch: 'wt3-branch',
    free: false,
    reasons: ['branch'],
    activity: 'waiting',
    actors: [actor({ label: 'asking agent', state: 'waiting', reason: 'permission' })],
  });
  mustValidate(wt3Waits);
  webview = await mount();
  webview.show(quiet);
  webview.show(wt3Waits);
  row('wt2').click();
  const output = webview.document.querySelector(`tr.detail .output[data-output-path="${TASKS}/b1.output"]`);
  const follow = output?.querySelector('button.follow');
  const tail = output?.querySelector('pre.output-tail');
  const announcer = () => [...webview.document.querySelectorAll('[aria-live="polite"], [role="status"]')].map(text).join(' ');
  const names = () => ({ follow: follow?.getAttribute('aria-label'), tail: tail?.getAttribute('aria-label'), live: announcer() });
  const before = { heading: heading('b1'), ...names() };

  // When
  webview.tick(60_000);
  const after = { heading: heading('b1'), ...names() };
  const posted = webview.posted.length;
  follow?.click();
  const followed = webview.posted.slice(posted);

  // Then
  assert.equal(before.heading, 'until · running · 2 m', 'the heading shows no run time to keep out of the names');
  assert.equal(after.heading, 'until · running · 3 m', 'the heading did not tick');
  assert.ok(follow && tail, 'the task has no Follow button or tail block');
  assert.match(before.live, /wt3/, 'the wait in wt3 is not announced');
  for (const [what, value] of Object.entries(names())) {
    assert.ok(!ANY_RUN_TIME.test(value ?? ''), `the ${what} name holds a run time: ${value}`);
  }
  assert.deepEqual({ follow: after.follow, tail: after.tail, live: after.live }, { follow: before.follow, tail: before.tail, live: before.live }, 'a name or the live region changed as the run time ticked');
  assert.deepEqual(followed, [{ type: 'follow', id: `${TASKS}/b1.output` }], 'Follow asked for more than its output');
});
