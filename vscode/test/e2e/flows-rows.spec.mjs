// The flows of the rows and the header (verification.md, "User flows"): selection, the connection
// states and the agents off note (X6, U5, S8). Live updates are in flows-exclusive.spec.mjs.
import assert from 'node:assert/strict';
import { chmodSync, copyFileSync, mkdirSync, renameSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { test } from '@playwright/test';

import { runDir } from '../support/paths.mjs';
import { waitForRows } from './rows.mjs';
import { detailOwners, detailText, openStatus, poll, rowOf, scenarioRows, withFlow, worktreePath } from './flows.mjs';
import { releaseWtpool, sleep, wtpoolProcesses } from './vscode.mjs';

test('flow__select_and_deselect', async () => {
  await withFlow('flow__select_and_deselect', {}, async (session) => {
    // Given
    const frame = await openStatus(session);
    const [wt1, wt2] = ['wt1', 'wt2'].map((name) => worktreePath(session, name));
    const owners = async (want) => (await poll(async () => {
      const found = await detailOwners(frame);
      return JSON.stringify(found) === JSON.stringify(want) && found;
    }, 3_000)) || detailOwners(frame);

    // When
    await rowOf(frame, session, 'wt1').click();
    const first = await owners([wt1]);
    const firstText = await detailText(frame);
    await rowOf(frame, session, 'wt2').click();
    const second = await owners([wt2]);
    const secondText = await detailText(frame);
    await rowOf(frame, session, 'wt2').click();
    const third = await owners([]);

    // Then
    assert.deepEqual(first, [wt1], 'clicking wt1 did not show one detail strip under wt1');
    assert.ok(firstText?.includes(wt1), `wt1's strip lacks its path: ${firstText}`);
    assert.deepEqual(second, [wt2], 'clicking wt2 did not move the strip under wt2');
    assert.ok(secondText?.includes(wt2), `wt2's strip lacks its path: ${secondText}`);
    assert.deepEqual(third, [], 'clicking wt2 again did not close the strip');
  });
});

test('flow__connection_states', async () => {
  // Given: wtpool.path names a copy of the release wtpool, which the check swaps for a failing start.
  const key = 'flow__connection_states';
  const { repeatEachIndex, retry } = test.info();
  const binary = path.join(runDir(), 'e2e', 'bins', `${key}-${repeatEachIndex}-${retry}`, 'wtpool');
  mkdirSync(path.dirname(binary), { recursive: true });
  copyFileSync(releaseWtpool(), binary);
  chmodSync(binary, 0o755);
  await withFlow(key, { wtpoolPath: binary }, async (session) => {
    const frame = await openStatus(session);
    const header = async () => ((await frame.locator('.connection').textContent()) ?? '').replace(/\s+/g, ' ').trim();
    const live = await poll(async () => (await header()) === 'live', 10_000);
    const watch = wtpoolProcesses(session.main, binary).find((p) => p.args.includes('--watch'));
    assert.ok(live, 'the header does not read live');
    assert.ok(watch, 'no wtpool status --watch runs');
    const real = `${binary}.real`;
    try {
      // When
      process.kill(watch.pid, 'SIGSTOP');
      const stale = await poll(async () => (await header()).match(/^updated (\d+) s ago$/), 30_000);
      const counting = await poll(async () => {
        const found = (await header()).match(/^updated (\d+) s ago$/);
        return found && Number(found[1]) > Number(stale[1]) && found;
      }, 5_000);
      renameSync(binary, real);
      writeFileSync(binary, '#!/bin/sh\nexit 1\n');
      chmodSync(binary, 0o755);
      const pausedN = async () => {
        const found = (await header()).match(/^updates paused, retrying in (\d+) s$/);
        return found ? Number(found[1]) : undefined;
      };
      const paused = await poll(pausedN, 40_000);
      const seen = [];
      // The failing restarts back off 1, 2, 4, 8 s: wait for a pause with N of 4 or more, then
      // read it again 2 s later, still inside the same pause. The header may lag the host's
      // retry_at by a fraction of a second, so N must drop by 1 to N - 1.
      const long = await poll(async () => {
        const n = await pausedN();
        if (n !== undefined) seen.push(n);
        return n >= 4 && n;
      }, 40_000, 100);
      await sleep(2000);
      const later = await pausedN();
      if (later !== undefined) seen.push(later);
      renameSync(real, binary);
      const restoredAt = Date.now();
      const restoredN = await pausedN();
      const resumed = await poll(async () => (await header()) === 'live', 40_000, 100);
      const resumedMs = Date.now() - restoredAt;
      const rows = await waitForRows(frame, scenarioRows(session), 10_000);

      // Then
      assert.ok(stale, 'the header never read "updated N s ago" after the stream stopped');
      assert.ok(Number(stale[1]) >= 15, `"updated N s ago" showed after ${stale[1]} s without a line`);
      assert.ok(counting, `"updated N s ago" does not count up from ${stale[0]} within 5 s`);
      assert.ok(paused, 'the header never read "updates paused, retrying in N s" while the restart failed');
      assert.deepEqual(seen.filter((n) => n < 1 || n > 30), [], `N outside the 1 to 30 s backoff: ${seen}`);
      assert.ok(long, `no pause read "retrying in N s" with N of 4 or more while the backoff grew: ${seen}`);
      assert.ok(later >= 1 && later <= long - 1, `N does not count down: ${long} s, then ${later} s 2 s later`);
      assert.ok(resumed, 'the header did not return to live after the stream resumed');
      if (restoredN !== undefined) {
        assert.ok(resumedMs <= (restoredN + 3) * 1000, `"retrying in ${restoredN} s", but live came ${resumedMs} ms later`);
      }
      assert.deepEqual(rows, scenarioRows(session), 'the resumed stream does not show the pool');
    } finally {
      try {
        renameSync(real, binary);
      } catch {
        // Already back.
      }
      for (const signal of ['SIGCONT', 'SIGKILL']) {
        try {
          if (watch) process.kill(watch.pid, signal);
        } catch {
          // Gone.
        }
      }
    }
  });
});

test('flow__agents_off_note', async () => {
  await withFlow('flow__agents_off_note', { scenario: 'agents_off' }, async (session) => {
    // Given
    const frame = await openStatus(session);

    // When
    const note = frame.locator('.agents-off');
    const count = await note.count();
    const text = count ? ((await note.first().textContent()) ?? '').replace(/\s+/g, ' ').trim() : '';
    const shown = count ? await note.first().evaluate((el) => el.checkVisibility({ opacityProperty: true, visibilityProperty: true })) : false;

    // Then
    assert.equal(count, 1, 'not one agents off note for a pool without wtpool-activity/');
    assert.ok(shown, 'the agents off note is not shown');
    assert.ok(text.includes('agent activity is off'), `the note does not say agent activity is off: ${text}`);
    assert.ok(text.includes('wtpool setup'), `the note does not name wtpool setup: ${text}`);
    // S8 (round publish-fixups): then the plugin's commands (H28) and `wtpool hook install` (H22).
    for (const step of ['/plugin marketplace add haiyuechen-nearone/wtpool', '/plugin install wtpool@wtpool', 'wtpool hook install']) {
      assert.ok(text.indexOf(step) > text.indexOf('wtpool setup'), `the note does not name ${step} after wtpool setup: ${text}`);
    }
  });
});
