import assert from 'node:assert/strict';
import { test } from '@playwright/test';

import { ago, document, everyActivity, git, mustValidate, withActor, worktree } from '../support/docs.mjs';
import { openPanel } from '../support/page.mjs';

const MB = 1024 * 1024;

/** The `number`th document of a stream: 20 worktrees whose facts and actors change each time. */
function nth(number) {
  const worktrees = [worktree('fg', { last_change: ago(number % 600) })];
  for (let slot = 1; slot <= 18; slot += 1) {
    const name = `wt${slot}`;
    const phase = (number + slot) % 4;
    if (phase === 0) worktrees.push(worktree(name, { last_change: ago((number * 7) % 3600) }));
    if (phase === 1) worktrees.push(withActor(name, { state: 'working', label: `session ${number % 60}`, updated: ago(number % 100) }));
    if (phase === 2) worktrees.push(withActor(name, { state: 'waiting', reason: 'permission', updated: ago(number % 50) }));
    if (phase === 3) {
      worktrees.push(worktree(name, {
        free: false,
        reasons: ['dirty'],
        git: git({ dirty: number % 9 + 1 }),
        activity: 'not_reusable',
      }));
    }
  }
  worktrees.push(worktree('x', { branch: `side-${number}` }));
  return document(worktrees);
}

async function heap(client) {
  await client.send('HeapProfiler.collectGarbage');
  await client.send('HeapProfiler.collectGarbage');
  const { usedSize } = await client.send('Runtime.getHeapUsage');
  return usedSize;
}

test('budget__b8', async ({ page }) => {
  // Given
  mustValidate(nth(1));
  mustValidate(everyActivity());
  const panel = await openPanel(page);
  const client = await page.context().newCDPSession(page);
  await client.send('HeapProfiler.enable');

  // When
  for (let number = 1; number <= 1000; number += 1) {
    await panel.show(nth(number));
    if (number % 100 === 0) await page.clock.runFor(1000);
  }
  const after1000 = await heap(client);
  for (let number = 1001; number <= 1500; number += 1) {
    await panel.show(nth(number));
    if (number % 100 === 0) await page.clock.runFor(1000);
  }
  const after1500 = await heap(client);

  // Then
  const rows = await page.locator('tr[data-row-id]').count();
  assert.equal(rows, 20, 'setup: the panel did not render the documents');
  assert.ok(after1000 <= 15 * MB, `heap ${(after1000 / MB).toFixed(2)} MB after 1000 documents, more than 15 MB`);
  assert.ok(
    after1500 - after1000 <= 1 * MB,
    `heap grew ${((after1500 - after1000) / MB).toFixed(2)} MB over 500 documents, more than 1 MB`,
  );
  assert.deepEqual(panel.errors, [], 'page errors');
});
