import assert from 'node:assert/strict';
import { afterEach, test } from 'vitest';

import { ago, actor, document, mustValidate, worktree } from '../support/docs.mjs';
import { clock, visible, withoutRunTime } from '../support/grammar.mjs';
import { mount, rowById, text } from '../support/webview.mjs';

let webview;
afterEach(() => {
  webview?.close();
  webview = undefined;
});

const TASKS = '/private/tmp/claude-501/-Users-verify-pool/s1/tasks';

function output(task, extra = {}) {
  return {
    task,
    path: `${TASKS}/${task}.output`,
    summary: 'cargo build',
    running: true,
    started: ago(300),
    ended: null,
    ...extra,
  };
}

function poolWithOutputs(outputs, extraActors = []) {
  const one = actor({ key: 's1', label: 'build agent', ...(outputs.length ? { outputs } : {}) });
  return mustValidate(
    document([
      worktree('fg'),
      worktree('wt2', {
        branch: 'wt2-branch',
        free: false,
        reasons: ['branch'],
        activity: 'working',
        actors: [one, ...extraActors],
        last_event: one.updated,
      }),
    ]),
  );
}

function outputMessage(row, tails) {
  return { type: 'output', version: 1, row, tails };
}

const LONG = `error[E0599]: ${'no method named `frobnicate` found for struct `Widget` in the current scope '.repeat(6)}`;

test('output__should_show_each_task_tail', async () => {
  // Given
  const ended = ago(40);
  const status = poolWithOutputs(
    [output('b3', { summary: 'cargo nextest' }), output('b2'), output('b1', { summary: 'npm run', running: false, ended })],
    [actor({ key: 's2', label: 'quiet one', state: 'idle', action: null, updated: ago(400) })],
  );
  const wt2 = status.pools[0].worktrees[1].path;
  webview = await mount();
  webview.show(status);
  const before = webview.posted.length;

  // When
  rowById(webview.document, wt2).click();
  const selected = webview.posted.slice(before);
  const hostile = ['plain line', LONG, 'bidi ‮txt.exe and zero​width', 'bell \u0007 here', '<img src=x onerror=alert(1)>'];
  webview.post(
    outputMessage(wt2, [
      { path: `${TASKS}/b3.output`, lines: hostile },
      { path: `${TASKS}/b2.output`, lines: [] },
      { path: `${TASKS}/b1.output`, lines: [], error: 'the output file is gone' },
      { path: '/etc/passwd', lines: ['root:x:0:0'] },
    ]),
  );
  webview.post(outputMessage(`${status.pools[0].worktrees[0].path}`, [{ path: `${TASKS}/b2.output`, lines: ['for another row'] }]));
  const strip = webview.document.querySelector('tr.detail');
  const sections = [...strip.querySelectorAll('section.outputs')];
  const tasks = [...strip.querySelectorAll('section.outputs .output[data-output-path]')];
  const follow = tasks[0]?.querySelector('button.follow');
  const beforeFollow = webview.posted.length;
  follow?.click();
  const followed = webview.posted.slice(beforeFollow);

  // Then
  assert.deepEqual(selected, [{ type: 'select', id: wt2 }], 'selecting wt2 did not tell the extension once');
  assert.equal(sections.length, 1, 'not one Output section for the one agent with outputs');
  assert.ok(text(sections[0]).startsWith('Output · build agent'), `the section does not name its agent: ${text(sections[0])}`);
  assert.deepEqual(
    tasks.map((task) => task.getAttribute('data-output-path')),
    [`${TASKS}/b3.output`, `${TASKS}/b2.output`, `${TASKS}/b1.output`],
    'the tasks do not follow the document',
  );
  assert.deepEqual(
    tasks.map((task) => withoutRunTime(text(task.querySelector('.output-heading')))),
    ['cargo nextest · running', 'cargo build · running', `npm run · ended ${clock(ended)}`],
  );
  for (const task of tasks) {
    const button = task.querySelector('button.follow');
    assert.ok(button && button.type === 'button' && text(button) === 'Follow', 'a task has no Follow button');
    assert.ok(task.querySelector('pre.output-tail'), 'a task has no monospace tail block');
  }
  const blocks = tasks.map((task) => task.querySelector('pre.output-tail'));
  assert.equal(blocks[0].textContent, hostile.map(visible).join('\n'), 'the tail is not its lines as text, controls shown as X10 shows them');
  assert.equal(blocks[0].querySelector('*'), null, 'the tail block holds markup');
  assert.equal(text(blocks[1]), 'no output yet');
  assert.equal(text(blocks[2]), 'the output file is gone');
  assert.ok(!text(strip).includes('root:x') && !text(strip).includes('for another row'), 'a tail the strip does not list, or of another row, shows');
  const style = webview.window.getComputedStyle(blocks[0]);
  assert.equal(style.whiteSpace, 'pre-wrap', 'long lines do not wrap');
  assert.equal(style.overflowWrap, 'anywhere', 'long words do not wrap anywhere');
  assert.deepEqual(followed, [{ type: 'follow', id: `${TASKS}/b3.output` }], 'Follow did not ask for its output');
  assert.deepEqual(webview.errors, []);
});

/** Gives a tail block the layout jsdom lacks: 20 px per line, 100 px tall, a settable scrollTop. */
function withLayout(block) {
  let top = 0;
  Object.defineProperty(block, 'clientHeight', { configurable: true, get: () => 100 });
  Object.defineProperty(block, 'scrollHeight', { configurable: true, get: () => 20 * block.textContent.split('\n').length });
  Object.defineProperty(block, 'scrollTop', {
    configurable: true,
    get: () => top,
    set: (value) => {
      top = Math.max(0, Math.min(value, 20 * block.textContent.split('\n').length - 100));
    },
  });
}

test('output__should_follow_the_end_unless_scrolled_up', async () => {
  // Given
  const status = poolWithOutputs([output('b1')]);
  const wt2 = status.pools[0].worktrees[1].path;
  const path = `${TASKS}/b1.output`;
  const tail = (from, to) => outputMessage(wt2, [{ path, lines: Array.from({ length: to - from + 1 }, (_, n) => `line ${from + n}`) }]);
  webview = await mount();
  webview.show(status);
  rowById(webview.document, wt2).click();
  const block = webview.document.querySelector(`.output[data-output-path="${path}"] pre.output-tail`);
  assert.ok(block, 'the strip shows no tail block for the task');
  withLayout(block);
  const atEnd = () => block.scrollTop === block.scrollHeight - block.clientHeight;

  // When
  webview.post(tail(1, 15));
  const first = atEnd();
  webview.post(tail(5, 19));
  const followed = atEnd();
  block.scrollTop = 40;
  block.dispatchEvent(new webview.window.Event('scroll'));
  webview.post(tail(9, 23));
  const keptUp = block.scrollTop;
  webview.show(structuredClone(status));
  const keptAfterDocument = block.scrollTop;
  const sameBlock = webview.document.querySelector(`.output[data-output-path="${path}"] pre.output-tail`) === block;
  block.scrollTop = 1e9;
  block.dispatchEvent(new webview.window.Event('scroll'));
  webview.post(tail(12, 26));
  const followsAgain = atEnd();

  // Then
  assert.ok(first, 'the first tail is not scrolled to its end');
  assert.ok(followed, 'new lines did not keep the block at its end');
  assert.equal(keptUp, 40, 'new lines moved a block the user scrolled up');
  assert.ok(sameBlock, 'a new document replaced the tail block');
  assert.equal(keptAfterDocument, 40, 'a new document moved a block the user scrolled up');
  assert.ok(followsAgain, 'a block scrolled back to its end does not follow new lines');
  assert.equal(block.textContent.split('\n').at(-1), 'line 26');
});

test('output__should_drop_what_the_document_no_longer_lists', async () => {
  // Given
  const status = poolWithOutputs(
    [output('b1'), output('b2')],
    [actor({ key: 's2', label: 'second agent', outputs: [output('b3', { summary: 'npm test' })] })],
  );
  const wt2 = status.pools[0].worktrees[1].path;
  webview = await mount();
  webview.show(status);
  rowById(webview.document, wt2).click();
  webview.post(outputMessage(wt2, [{ path: `${TASKS}/b2.output`, lines: ['old line'] }]));
  const strip = () => webview.document.querySelector('tr.detail');
  const task = (id) => strip().querySelector(`.output[data-output-path="${TASKS}/${id}.output"]`);
  const kept = task('b1');

  // When
  const fewer = structuredClone(status);
  const actors = fewer.pools[0].worktrees[1].actors;
  actors[0].outputs = [actors[0].outputs[0]];
  delete actors[1].outputs;
  webview.show(fewer);
  const listed = [...strip().querySelectorAll('.output[data-output-path]')].map((t) => t.getAttribute('data-output-path'));
  const sections = [...strip().querySelectorAll('section.outputs')].map((s) => text(s.querySelector('.outputs-heading')));
  const sameTask = task('b1') === kept;
  webview.show(structuredClone(status));
  const back = text(task('b2').querySelector('pre.output-tail'));
  const none = structuredClone(status);
  for (const one of none.pools[0].worktrees[1].actors) delete one.outputs;
  webview.show(none);

  // Then
  assert.deepEqual(listed, [`${TASKS}/b1.output`], 'a task the document no longer lists still shows');
  assert.deepEqual(sections, ['Output · build agent'], 'the section of an agent without tasks still shows');
  assert.ok(sameTask, 'a task the document still lists was made anew');
  assert.equal(back, 'reading the output', 'a task that came back shows its tail from before');
  assert.equal(strip().querySelectorAll('.output').length, 0, 'tasks show with none listed');
  assert.ok(strip().querySelector('.output-area').hidden, 'the empty Output area shows');
  assert.deepEqual(webview.errors, []);
});
