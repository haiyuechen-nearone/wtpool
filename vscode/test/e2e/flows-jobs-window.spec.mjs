// The heavy job flows of jobs.md, "User flows", about the wtpool and the window the tab has: an
// older wtpool without `wtpool job`, a wtpool that left, and a window whose folder is not on this
// computer (S14, U10, U11, R9, R10, X3, X21).
import assert from 'node:assert/strict';
import { chmodSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from '@playwright/test';

import { runDir } from '../support/paths.mjs';
import { openStatus, poll, withFlow } from './flows.mjs';
import { headingText, machineButton, machineTomlPath, names, openStrip, readText, stripText, writeMachineToml } from './jobs.mjs';
import { close, launch, pool, releaseWtpool, sandbox, sleep, watchers } from './vscode.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const OLD_NOTE = 'This wtpool has no heavy job limit. Update wtpool to a version with wtpool job to set one.';

/** A folder outside the sandbox, which `sandbox` makes afresh, for a `wtpool.path` of the flow `key`. */
function wrapperDir(key) {
  const dir = path.join(runDir(), 'e2e', 'wrappers', key);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  return dir;
}

test('flow__jobs_old_wtpool', async () => {
  // Given: wtpool.path names a wtpool whose documents have no machine.jobs (wtpool-old.mjs).
  const file = path.join(wrapperDir('flow__jobs_old_wtpool'), 'wtpool');
  writeFileSync(file, ['#!/bin/sh', `exec '${process.execPath}' '${path.join(here, 'wtpool-old.mjs')}' '${releaseWtpool()}' "$@"`, ''].join('\n'));
  chmodSync(file, 0o755);
  await withFlow('flow__jobs_old_wtpool', { wtpoolPath: file }, async (session) => {
    const frame = await openStatus(session);
    const { machine, repository, status } = names(session);
    assert.equal(status.machine.jobs, undefined, 'setup: the old wtpool\'s document holds machine.jobs');

    // When
    const button = await machineButton(frame);
    const heading = await headingText(frame);
    const buttonText = (await button.innerText()).replace(/\s+/g, ' ').trim();
    await openStrip(frame);
    const strip = await stripText(frame);

    // Then
    assert.equal(heading, `${repository} · ${machine}`, 'without machine.jobs the heading does not read the repository and the machine alone (U10)');
    assert.equal(buttonText, machine, 'without machine.jobs the machine button does not hold the name alone, and the chevron (U10)');
    assert.equal(strip, OLD_NOTE, 'the strip of a wtpool without wtpool job does not hold only its note (U11)');
  });
});

test('flow__jobs_wtpool_missing', async () => {
  // Given: wtpool.path names a link to the release wtpool, which the flow removes, as wtpool leaving PATH does.
  const link = path.join(wrapperDir('flow__jobs_wtpool_missing'), 'wtpool');
  symlinkSync(releaseWtpool(), link);
  await withFlow('flow__jobs_wtpool_missing', { wtpoolPath: link }, async (session) => {
    writeMachineToml(session.dirs, { limit: 4 });
    const frame = await openStatus(session);
    await machineButton(frame);
    const strip = await openStrip(frame);
    const rowsBefore = await frame.locator('tr[data-row-id]').count();
    const configBefore = readText(machineTomlPath(session.dirs));

    // When
    rmSync(link);
    for (const watcher of watchers(link)) process.kill(watcher.pid, 'SIGTERM');
    const expected = `wtpool did not start: spawn ${link} ENOENT`;
    const header = await poll(async () => {
      const error = ((await frame.locator('.wtpool-error').first().textContent().catch(() => '')) ?? '').trim();
      const connection = ((await frame.locator('.connection').first().textContent().catch(() => '')) ?? '').trim();
      return error === expected && /updates paused/.test(connection) ? { error, connection } : undefined;
    }, 60_000, 250);
    const rowsWhilePaused = await frame.locator('tr[data-row-id]').count();
    const limit = strip.locator('input.job-limit');
    await limit.fill('3');
    await strip.locator('button.job-save').click();
    const answer = await poll(async () => {
      const text = ((await strip.locator('.limit-status').textContent()) ?? '').trim();
      return text === expected ? text : undefined;
    }, 15_000, 200) ?? ((await strip.locator('.limit-status').textContent()) ?? '').trim();
    await sleep(500);

    // Then
    assert.ok(header, `the header does not show ${expected} next to updates paused (R10)`);
    assert.equal(rowsWhilePaused, rowsBefore, 'the last document does not stay while wtpool is missing (R10)');
    assert.equal(answer, expected, 'Save does not show that wtpool did not start (X21, R10 wording)');
    assert.equal(await frame.locator('tr.machine-detail').count(), 1, 'the strip closed (X21: nothing else changes)');
    assert.equal(await limit.inputValue(), '3', 'the field lost the value Save could not send');
    assert.equal(readText(machineTomlPath(session.dirs)), configBefore, 'machine.toml changed though wtpool did not start');
  });
});

test('flow__jobs_remote_window', async () => {
  // Given: a workspace whose one folder has a scheme no file system provides here, as a
  // Remote-SSH, WSL or virtual folder is not a local `file:` folder (R9). wtpool then runs in the
  // home folder, where WTPOOL_FOREGROUND names the local pool.
  const key = 'flow__jobs_remote_window';
  const poolInfo = pool('idle_pool', key);
  const dirs = sandbox(key);
  const workspace = path.join(dirs.root, 'remote.code-workspace');
  writeFileSync(workspace, JSON.stringify({ folders: [{ uri: 'wtpool-verify-remote://verify.invalid/home/project', name: 'project' }], settings: {} }, null, 2));
  const session = { ...(await launch(dirs, poolInfo, [], { folder: workspace })), pool: poolInfo, scenario: 'idle_pool' };
  try {
    const title = await session.window.title();
    assert.ok(/remote|project/i.test(title), `setup: VS Code did not open the remote workspace: ${title}`);
    const frame = await openStatus(session);
    const { machine } = names(session);

    // When
    await machineButton(frame);
    const heading = await headingText(frame);
    await openStrip(frame);
    const strip = await stripText(frame);

    // Then
    assert.ok(heading.includes(` · ${machine}`), `the heading of a window on a remote folder does not name this computer: ${heading} (R9, X3)`);
    assert.ok(
      strip?.includes(`This window's folder is not on this computer: the settings are for this computer, ${machine}.`),
      `the strip does not say the settings are for this computer: ${strip} (U11)`,
    );
  } finally {
    await close(session);
  }
});
