// The scenario catalog in the host layer: the fake wtpool replays each scenario's documents and the
// extension posts every one of them to the webview, in order.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const vscode = require('vscode');

const {
  REPO,
  control,
  ended,
  openStatus,
  posted,
  resetFake,
  scenarioDocuments,
  sleep,
  starts,
  waitFor,
  wrap,
} = require('./support.cjs');

const dir = path.join(REPO, 'verify', 'scenarios');
const names = fs
  .readdirSync(dir)
  .filter((file) => file.endsWith('.json'))
  .map((file) => file.slice(0, -'.json'.length))
  .filter((name) => JSON.parse(fs.readFileSync(path.join(dir, `${name}.json`), 'utf8')).layers.includes('host'));

describe('scenarios', () => {
  for (const name of names) {
    it(name, async () => {
      // Given
      await resetFake({ scenario: name, aliveMs: 200, gapMs: 500 });
      const expected = scenarioDocuments(name).map((doc) => JSON.stringify(doc));
      const from = (await posted()).length;

      // When
      await openStatus();
      const statuses = await waitFor(async () => {
        const seen = (await posted())
          .slice(from)
          .filter((m) => m && m.type === 'view')
          .map((m) => JSON.stringify(m.status));
        return seen.at(-1) === expected.at(-1) ? seen : undefined;
        // The fake prints a document each 500 ms: a long scenario needs more than 10 s.
      }, Math.max(10_000, expected.length * 500 + 5_000));

      // Then
      assert.ok(statuses, `${name}: the last document never reached the webview`);
      const order = expected.map((doc) => statuses.indexOf(doc));
      assert.ok(order.every((index) => index >= 0), `${name}: documents missing from the webview`);
      assert.deepEqual([...order].sort((a, b) => a - b), order, `${name}: documents out of order`);
    });
  }
});

// S7 in the extension: the validator generated from the schema accepts every scenario document,
// so none restarts the stream, and rejects one that breaks the schema, which does.
describe('validator', () => {
  it('validator__should_accept_every_scenario_document', async () => {
    // Given
    // Each document distinct, so a product that skips a repeated document still posts each.
    const documents = names.flatMap((name) => scenarioDocuments(name)).map((doc, index) => ({
      ...doc,
      generated: new Date(Date.parse(doc.generated) + index * 1000).toISOString().replace('.000Z', 'Z'),
    }));
    const broken = structuredClone(documents.at(-1));
    broken.pools[0].worktrees[0].activity = 'bogus';
    await resetFake({ documents, aliveMs: 200, gapMs: 300 });
    const from = (await posted()).length;

    // When
    await openStatus();
    const last = JSON.stringify(documents.at(-1));
    const reached = await waitFor(async () => {
      const seen = (await posted()).slice(from).filter((m) => m && m.type === 'view');
      return seen.some((m) => JSON.stringify(m.status) === last) ? seen : undefined;
    }, 60_000);
    const startsBefore = starts().length;
    control(`emit ${JSON.stringify(broken)}`);
    await sleep(1500);
    const afterBroken = (await posted()).slice(from).filter((m) => m && m.type === 'view');

    // Then
    assert.ok(reached, 'the last scenario document never reached the webview');
    const shown = new Set(reached.map((m) => JSON.stringify(m.status)));
    const rejected = documents.filter((doc) => !shown.has(JSON.stringify(doc))).map((doc) => doc.generated);
    assert.deepEqual(rejected, [], 'scenario documents the extension did not post');
    assert.equal(startsBefore, 1, 'a scenario document restarted the stream');
    assert.ok(
      !afterBroken.some((m) => m.status?.pools?.[0]?.worktrees?.[0]?.activity === 'bogus'),
      'a document that breaks the schema reached the webview',
    );
    assert.ok(
      afterBroken.some((m) => m.connection === 'paused') || ended(starts()[0].pid),
      'a document that breaks the schema did not restart the stream',
    );
  });
});

// D3: waits raise no popup and the extension shows no status bar item.
describe('popups', () => {
  it('manifest__should_contribute_no_status_bar_item waits_raise_no_popup', async () => {
    // Given
    const waiting = structuredClone(scenarioDocuments('idle_pool').at(-1));
    const slot = waiting.pools[0].worktrees.find((w) => w.name === 'wt2');
    Object.assign(slot, {
      activity: 'waiting',
      actors: [
        {
          key: 's1',
          kind: 'session',
          harness: 'claude-code',
          label: 'session 13:41',
          state: 'waiting',
          reason: 'permission',
          action: { tool: 'Bash', target: 'git push' },
          placed_by: 'cwd',
          error: null,
          since: '2026-10-20T11:59:00Z',
          updated: '2026-10-20T11:59:30Z',
        },
      ],
      last_event: '2026-10-20T11:59:30Z',
    });
    const popups = ['showInformationMessage', 'showWarningMessage', 'showErrorMessage', 'createStatusBarItem', 'setStatusBarMessage'].map(
      (name) => [name, wrap(vscode.window, name, () => true)],
    );

    // When
    try {
      await resetFake({ documents: [scenarioDocuments('idle_pool').at(-1), waiting], aliveMs: 200 });
      await openStatus();
      await waitFor(async () => (await posted()).some((m) => m?.type === 'view' && m.status?.generated === waiting.generated && m.status.pools[0].worktrees.some((w) => w.activity === 'waiting')), 10_000);
      await vscode.commands.executeCommand('workbench.action.closeAllEditors');
      await resetFake({ documents: [waiting], aliveMs: 200 });
      await openStatus();
      await sleep(1500);
    } finally {
      for (const [, wrapped] of popups) wrapped.restore();
    }

    // Then
    const called = popups.filter(([, wrapped]) => wrapped.calls.length).map(([name]) => name);
    assert.deepEqual(called, [], 'a wait raised a popup or a status bar item');
  });
});
