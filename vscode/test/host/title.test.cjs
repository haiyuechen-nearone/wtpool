const assert = require('node:assert/strict');
const vscode = require('vscode');

const { created, expectedDocuments, openStatus, resetFake, sleep, waitFor } = require('./support.cjs');

/** The panel the last Worktrees: Open Status created. */
function statusPanel() {
  return created.filter((c) => c.viewType === 'wtpool.status').at(-1)?.panel;
}

/** `doc` with `generated` moved on by `seconds`, so every document differs. */
function later(doc, seconds) {
  const copy = structuredClone(doc);
  copy.generated = new Date(Date.parse(doc.generated) + seconds * 1000).toISOString().replace('.000Z', 'Z');
  return copy;
}

/** The waiting document of permission_wait_and_answer with wt3 waiting too. */
function twoWaiting(waiting) {
  const copy = structuredClone(waiting);
  const wt2 = copy.pools[0].worktrees.find((w) => w.name === 'wt2');
  const wt3 = copy.pools[0].worktrees.find((w) => w.name === 'wt3');
  Object.assign(wt3, {
    activity: 'waiting',
    actors: wt2.actors.map((a) => ({ ...a, key: 's9', label: 'second wait' })),
    last_event: wt2.last_event,
  });
  return copy;
}

async function titleBecomes(panel, title, timeoutMs = 10_000) {
  return waitFor(() => panel?.title === title, timeoutMs);
}

describe('title', () => {
  it('title__should_read_worktrees_without_waits', async () => {
    // Given
    const idle = expectedDocuments('idle_pool');
    await resetFake({ documents: idle, aliveMs: 200 });

    // When
    await openStatus();
    const panel = statusPanel();
    await sleep(1500);

    // Then
    assert.ok(panel, 'no wtpool.status panel was created');
    assert.equal(panel.title, 'Worktrees', 'the title of a pool without waits');
  });

  it('title__should_count_waits_while_hidden', async () => {
    // Given
    const [waiting, answered] = expectedDocuments('permission_wait_and_answer');
    const documents = [later(answered, 0), later(waiting, 1), later(twoWaiting(waiting), 2), later(answered, 3)];
    await resetFake({ documents, aliveMs: 200, gapMs: 4000 });
    await openStatus();
    const panel = statusPanel();
    const first = await titleBecomes(panel, 'Worktrees', 3_000);

    // When
    const editor = await vscode.workspace.openTextDocument({ content: 'wtpool-verify' });
    await vscode.window.showTextDocument(editor, { viewColumn: panel?.viewColumn, preview: false });
    const hidden = await waitFor(() => panel && !panel.visible, 3_000);
    const one = await titleBecomes(panel, 'Worktrees (1)');
    const visibleAtOne = panel?.visible;
    const two = await titleBecomes(panel, 'Worktrees (2)');
    const visibleAtTwo = panel?.visible;
    const none = await titleBecomes(panel, 'Worktrees');

    // Then
    assert.ok(panel, 'no wtpool.status panel was created');
    assert.ok(first, `the first document, without waits, gave the title ${panel?.title}`);
    assert.ok(hidden, 'setup: the tab did not hide behind an editor');
    assert.ok(one, `one waiting worktree while hidden gave the title ${panel?.title}`);
    assert.equal(visibleAtOne, false, 'setup: the tab showed again before the count');
    assert.ok(two, `two waiting worktrees while hidden gave the title ${panel?.title}`);
    assert.equal(visibleAtTwo, false, 'setup: the tab showed again before the count');
    assert.ok(none, `the wait ending while hidden left the title ${panel?.title}`);
  });

  it('title__should_drop_the_count_when_a_wait_ends', async () => {
    // Given
    const [waiting, answered] = expectedDocuments('permission_wait_and_answer');
    const documents = [later(waiting, 0), later(answered, 1)];
    await resetFake({ documents, aliveMs: 200, gapMs: 3000 });

    // When
    await openStatus();
    const panel = statusPanel();
    const counted = await titleBecomes(panel, 'Worktrees (1)');
    const dropped = await titleBecomes(panel, 'Worktrees', 8_000);

    // Then
    assert.ok(panel, 'no wtpool.status panel was created');
    assert.ok(counted, `wt2 waiting gave the title ${panel?.title}`);
    assert.ok(dropped, `wt2 answered left the title ${panel?.title}`);
    assert.ok(panel.visible, 'setup: the tab did not stay visible');
  });
});
