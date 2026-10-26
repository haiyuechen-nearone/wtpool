// The extension's posts to the webview, and no wtpool outliving the tab, in the extension host.
const assert = require('node:assert/strict');
const vscode = require('vscode');

const { advance, control, ended, openStatus, posted, resetFake, scenarioDocuments, sleep, starts, waitFor } = require('./support.cjs');

describe('budgets', () => {
  it('budget__b7', async () => {
    // Given
    await resetFake({ scenario: 'idle_pool', aliveMs: 0 });
    const from = (await posted()).length;
    const opened = await openStatus();
    // The fake reads control lines only once it has printed its documents, and earlier checks
    // posted idle_pool too, so only a view of this tab counts.
    const final = JSON.stringify(scenarioDocuments('idle_pool').at(-1));
    await waitFor(async () => (await posted()).slice(from).some((m) => m.type === 'view' && JSON.stringify(m.status) === final), 10_000);
    await sleep(500);
    const settled = (await posted()).length;

    // When
    for (let beat = 0; beat < 5; beat += 1) control('alive');
    await sleep(1000);
    await advance(5000, { step: 1000, realMs: 100 });
    const afterAliveAndTicks = (await posted()).slice(settled);
    const document = await vscode.workspace.openTextDocument({ content: 'hide' });
    await vscode.window.showTextDocument(document, { viewColumn: opened.group.viewColumn, preview: false });
    await sleep(500);
    const hidden = (await posted()).length;
    const docs = scenarioDocuments('idle_pool');
    for (let number = 1; number <= 3; number += 1) {
      const next = structuredClone(docs.at(-1));
      next.generated = `2026-10-20T12:0${number}:00Z`;
      control(`emit ${JSON.stringify(next)}`);
    }
    control('alive');
    await sleep(1500);
    const whileHidden = (await posted()).slice(hidden);
    await vscode.commands.executeCommand('wtpool.openStatus');
    await waitFor(async () => (await posted()).length > hidden + whileHidden.length, 10_000);
    await sleep(1500);
    const onShow = (await posted()).slice(hidden + whileHidden.length);

    // Then
    assert.deepEqual(afterAliveAndTicks, [], 'alive lines that leave the connection live, or ticks, posted');
    assert.deepEqual(whileHidden, [], 'the extension posted to a hidden tab');
    assert.equal(onShow.length, 1, `showing the tab posted ${onShow.length} messages instead of the last document`);
    assert.equal(onShow[0]?.status?.generated, '2026-10-20T12:03:00Z', 'showing the tab posted another document');
  });

  for (const [title, fault] of [
    ['budget__b10', 'none'],
    ['budget__b10 ignoring_eof', 'ignore_eof'],
  ]) {
    it(title, async () => {
      // Given
      await resetFake({ scenario: 'idle_pool', faults: [fault], aliveMs: 200 });
      await openStatus();
      const started = await waitFor(() => starts()[0], 10_000);
      assert.ok(started, 'wtpool did not start');
      await sleep(500);

      // When
      await vscode.commands.executeCommand('workbench.action.closeAllEditors');
      const gone = await waitFor(() => ended(started.pid), 1_000);

      // Then
      assert.ok(gone, `wtpool ${started.pid} still runs 1 s after the tab closed (${fault})`);
    });
  }
});
