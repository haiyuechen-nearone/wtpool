import assert from 'node:assert/strict';
import path from 'node:path';
import { test } from '@playwright/test';

import { GROUP_TOKENS } from '../support/grammar.mjs';
import { poll, withFlow, worktreePath } from './flows.mjs';
import {
  addRepoKeys,
  machineTomlPath,
  readText,
  tomlValue,
  writeMachineToml,
} from './jobs-config.mjs';
import {
  agentLines,
  focusKey,
  headingText,
  jobLists,
  listProblems,
  liveText,
  machineButton,
  openStrip,
  openTab,
  reads,
  stripText,
  tokenColour,
} from './jobs-dom.mjs';
import {
  agent,
  assertRunning,
  defaultLimit,
  endCommand,
  fourRunTwoWait,
  heavyCommand,
  loggedRuns,
  loggingWtpool,
  runningCommands,
  startedCount,
  startJob,
  stopJobs,
  WITHIN_A_SECOND_MS,
} from './jobs-process.mjs';
import { sleep, startProcess } from './vscode.mjs';

test('flow__jobs_running_and_queued', async () => {
  await withFlow('flow__jobs_running_and_queued', { beforeLaunch: (dirs) => writeMachineToml(dirs, { limit: 4, enforce: 'warn' }) }, async (session) => {
    try {
      // Given
      const frame = await openTab(session);
      await machineButton(frame);
      await fourRunTwoWait(session, { withAgent: true });

      // When
      await openStrip(frame);
      await sleep(1500);
      const before = await jobLists(frame);
      const rowBefore = await agentLines(frame, session, 'wt1');
      const focusBefore = await focusKey(frame);
      const oldest = runningCommands(session.dirs)[0];
      await endCommand(session.dirs, oldest.pid);
      const moved = await poll(async () => {
        const lists = await jobLists(frame);
        return lists.waiting.length === 1 && lists.running.some((line) => reads(line, ['cargo test', 'wt1', '·', 'Design jobs'])) ? lists : undefined;
      }, WITHIN_A_SECOND_MS, 100);
      const after = moved ?? (await jobLists(frame));
      const rowAfter = await poll(async () => {
        const row = await agentLines(frame, session, 'wt1');
        return row?.agent.includes('Design jobs · Bash cargo test') ? row : undefined;
      }, WITHIN_A_SECOND_MS, 100) ?? (await agentLines(frame, session, 'wt1'));

      // Then
      assert.deepEqual(
        [
          ...listProblems('Running', before.running, [['cargo nextest', 'wt1'], ['npm run', 'wt3'], ['verify', 'outside this pool'], ['cargo build', 'outside this pool']]),
          ...listProblems('Queued', before.waiting, [['1', 'cargo test', 'wt1', '·', 'Design jobs'], ['2', 'cargo clippy', 'fg']]),
        ],
        [],
        'the strip\'s Running and Queued lists with 4 jobs running and 2 waiting (U11, S14, S15)',
      );
      assert.ok(reads(before.waiting[0] ?? '', ['1', 'cargo test']) && reads(before.waiting[1] ?? '', ['2', 'cargo clippy']), `Queued is not in queue order with positions: ${JSON.stringify(before.waiting)}`);
      assert.equal(rowBefore?.activity, 'working', 'wt1 does not show working while its agent\'s job waits (A8)');
      assert.ok(rowBefore?.agent.includes('Design jobs · queued: cargo test'), `the waiting agent does not read "Design jobs · queued: cargo test" (A8): ${JSON.stringify(rowBefore)}`);
      assert.ok(moved, `within 1 s of a running job's end queued job 1 does not move to Running: ${JSON.stringify(after)}`);
      assert.ok(reads(after.waiting[0] ?? '', ['1', 'cargo clippy', 'fg']), `cargo clippy is not queued job 1 now: ${JSON.stringify(after.waiting)}`);
      assert.ok(rowAfter?.agent.includes('Design jobs · Bash cargo test'), `once its job runs, the agent line does not show its own action (A8): ${JSON.stringify(rowAfter)}`);
      assert.equal(await focusKey(frame), focusBefore, 'focus moved when the jobs changed (X15)');
    } finally {
      await stopJobs(session);
    }
  });
});

test('flow__jobs_change_settings', async () => {
  const logging = loggingWtpool('flow__jobs_change_settings');
  await withFlow('flow__jobs_change_settings', { wtpoolPath: logging.file, beforeLaunch: (dirs) => writeMachineToml(dirs, { limit: 4, enforce: 'warn' }) }, async (session) => {
    try {
      // Given
      const frame = await openTab(session);
      await machineButton(frame);
      await fourRunTwoWait(session);
      const strip = await openStrip(frame);
      const limit = strip.locator('input.job-limit');
      const policy = strip.locator('select.job-enforce');
      assert.equal(await limit.count(), 1, 'the strip has no input.job-limit (U11)');
      assert.equal(await policy.count(), 1, 'the strip has no select.job-enforce (U11)');

      // When
      await limit.fill('6');
      await policy.selectOption('require');
      await limit.press('Enter');
      const saved = await poll(async () => (await strip.locator('.limit-status').textContent())?.includes('Saved.'), WITHIN_A_SECOND_MS * 3, 100);
      const runsAfterSave = loggedRuns(logging.log);
      const tomlAfterSave = readText(machineTomlPath(session.dirs));
      const heading = await poll(async () => ((await headingText(frame)).includes('jobs 6/6') ? headingText(frame) : undefined), WITHIN_A_SECOND_MS, 100);
      const runningAfterSave = runningCommands(session.dirs).length;
      const announced = await liveText(frame);
      await strip.locator('button.job-default').click();
      await poll(() => loggedRuns(logging.log).length >= 4, WITHIN_A_SECOND_MS * 3, 100);
      await sleep(1000);
      const runsAfterDefaults = loggedRuns(logging.log);
      const tomlAfterDefaults = readText(machineTomlPath(session.dirs));
      const fieldAfterDefaults = await limit.inputValue();
      const policyAfterDefaults = await policy.inputValue();

      // Then
      assert.deepEqual(runsAfterSave, [['job', 'limit', '6'], ['job', 'enforce', 'require']], 'Save does not run wtpool job limit 6, then wtpool job enforce require, once each (X21)');
      assert.ok(saved, 'the strip does not say Saved. (X20)');
      assert.ok(announced.includes('Saved.'), `the live region does not announce Saved.: ${announced} (X20)`);
      assert.equal(tomlValue(tomlAfterSave, 'limit'), '6', `machine.toml does not hold limit 6: ${tomlAfterSave} (J6)`);
      assert.equal(tomlValue(tomlAfterSave, 'enforce'), '"require"', `machine.toml does not hold enforce require: ${tomlAfterSave} (J6)`);
      assert.ok(heading, `within 1 s the heading does not read jobs 6/6: ${await headingText(frame)} (W10, U10)`);
      assert.equal(runningAfterSave, 6, 'the 2 queued jobs do not run under limit 6 (J11)');
      assert.deepEqual(runsAfterDefaults.slice(2), [['job', 'limit', 'default'], ['job', 'enforce', 'default']], 'Use defaults does not run wtpool job limit default, then wtpool job enforce default (X20, X21)');
      assert.equal(tomlValue(tomlAfterDefaults, 'limit'), null, `machine.toml keeps a limit after Use defaults: ${tomlAfterDefaults} (J3)`);
      assert.equal(tomlValue(tomlAfterDefaults, 'enforce'), null, `machine.toml keeps an enforce after Use defaults: ${tomlAfterDefaults} (J3)`);
      assert.equal(fieldAfterDefaults, String(defaultLimit()), 'the Limit field does not show the default limit after Use defaults (J5)');
      assert.equal(policyAfterDefaults, 'warn', 'the Policy menu does not show warn after Use defaults (J5)');
    } finally {
      await stopJobs(session);
    }
  });
});

test('flow__jobs_lower_limit', async () => {
  await withFlow('flow__jobs_lower_limit', { beforeLaunch: (dirs) => writeMachineToml(dirs, { limit: 4, enforce: 'warn' }) }, async (session) => {
    try {
      // Given
      const frame = await openTab(session);
      await machineButton(frame);
      await fourRunTwoWait(session);
      const strip = await openStrip(frame);
      const limit = strip.locator('input.job-limit');
      assert.equal(await limit.count(), 1, 'the strip has no input.job-limit (U11)');
      const first = runningCommands(session.dirs).map((c) => c.pid);

      // When
      await limit.fill('2');
      await limit.press('Enter');
      const lowered = await poll(() => tomlValue(readText(machineTomlPath(session.dirs)), 'limit') === '2', WITHIN_A_SECOND_MS * 3, 100);
      await sleep(2000);
      const stillRunning = runningCommands(session.dirs).map((c) => c.pid);
      const over = await stripText(frame);
      await endCommand(session.dirs, first[0]);
      await endCommand(session.dirs, first[1]);
      await sleep(2500);
      const startedAfterTwo = startedCount(session.dirs);
      await endCommand(session.dirs, first[2]);
      const startedAfterThree = await poll(() => (startedCount(session.dirs) === 5 ? 5 : undefined), WITHIN_A_SECOND_MS, 100) ?? startedCount(session.dirs);
      const newest = runningCommands(session.dirs).filter((c) => !first.includes(c.pid));

      // Then
      assert.ok(lowered, 'setting 2 does not write limit 2 to machine.toml (X21, J6)');
      assert.deepEqual(stillRunning.sort(), [...first].sort(), 'a running job stopped when the limit dropped to 2 (J11)');
      assert.ok(over?.includes('4 running, over the limit of 2: they finish, and queued jobs start once fewer than 2 run.'), `the strip does not explain the lowered limit (U11): ${over}`);
      assert.equal(startedAfterTwo, 4, 'a queued job started while 2 ran under limit 2 (J11)');
      assert.equal(startedAfterThree, 5, 'queued job 1 does not start once fewer than 2 run (J11)');
      assert.ok(newest.length === 1 && newest[0].line.includes('cargo test'), `the job that started is not queued job 1, cargo test: ${JSON.stringify(newest)} (J10)`);
    } finally {
      await stopJobs(session);
    }
  });
});

test('flow__jobs_not_queued', async () => {
  await withFlow('flow__jobs_not_queued', { scenario: 'building_without_agent', prepare: (poolInfo) => addRepoKeys(poolInfo, { heavy: ['make'] }), beforeLaunch: (dirs) => writeMachineToml(dirs, { enforce: 'warn' }) }, async (session) => {
    let build;
    try {
      // Given
      const frame = await openTab(session);
      await machineButton(frame);
      const wt2 = worktreePath(session, 'wt2');
      const attention = await tokenColour(frame, GROUP_TOKENS.attention);

      // When
      build = startProcess(session.dirs, 'make', wt2);
      const outside = await poll(async () => {
        const row = await agentLines(frame, session, 'wt2');
        return row?.agent.includes('build: make (not queued)') ? row : undefined;
      }, WITHIN_A_SECOND_MS, 100) ?? (await agentLines(frame, session, 'wt2'));
      const mark = await frame.evaluate((id) => {
        const el = document.querySelector(`tr[data-row-id="${CSS.escape(id)}"] .not-queued`);
        return el ? { text: el.textContent.replace(/\s+/g, ' ').trim(), colour: getComputedStyle(el).color } : null;
      }, wt2);
      const headingOutside = await headingText(frame);
      await openStrip(frame);
      const listsOutside = await jobLists(frame);
      build.kill();
      build = startJob(session, ['make', '600'], { cwd: wt2 });
      const inJob = await poll(async () => {
        const row = await agentLines(frame, session, 'wt2');
        return row?.agent.includes('build: make') ? row : undefined;
      }, WITHIN_A_SECOND_MS * 2, 100) ?? (await agentLines(frame, session, 'wt2'));
      await sleep(500);
      const headingInJob = await headingText(frame);
      const listsInJob = await jobLists(frame);
      const stripInJob = await stripText(frame);

      // Then
      assert.equal(outside?.activity, 'building', `within 1 s of make outside a job wt2 does not read building (S4): ${JSON.stringify(outside)}`);
      assert.ok(outside?.agent.includes('build: make (not queued)'), `wt2 does not read build: make (not queued) under warn (A8): ${JSON.stringify(outside)}`);
      assert.equal(mark?.text, '(not queued)', `the row holds no .not-queued "(not queued)": ${JSON.stringify(mark)} (U14)`);
      assert.equal(mark.colour, attention, `(not queued) is not in ${GROUP_TOKENS.attention} (U14)`);
      assert.ok(headingOutside.includes('· 1 not queued'), `the heading does not add · 1 not queued: ${headingOutside} (U10)`);
      assert.deepEqual(listProblems('Not queued', listsOutside.outside, [['make', 'wt2']]), [], 'the strip does not list make under Not queued (U11)');
      assert.ok(inJob?.agent.includes('build: make') && !inJob.agent.some((line) => line.includes('not queued')), `as wtpool job -- make the line does not read build: make (A8, S4): ${JSON.stringify(inJob)}`);
      assert.ok(!headingInJob.includes('not queued'), `the heading still counts a build not queued: ${headingInJob} (U10)`);
      assert.deepEqual(listsInJob.outside, [], 'Not queued still lists a build that a job holds (U11)');
      assert.ok(/Not queued 0 none/i.test(stripInJob ?? ''), `the strip does not read Not queued 0, none: ${stripInJob} (U11)`);
    } finally {
      build?.kill();
      await stopJobs(session);
    }
  });
});

test('flow__jobs_require_refuses', async () => {
  await withFlow('flow__jobs_require_refuses', { prepare: (poolInfo) => addRepoKeys(poolInfo, { heavy: ['cargo test'] }), beforeLaunch: (dirs) => writeMachineToml(dirs, { limit: 1, enforce: 'require' }) }, async (session) => {
    try {
      // Given
      heavyCommand(session.dirs, 'cargo');
      const frame = await openTab(session);
      const wt1 = worktreePath(session, 'wt1');
      startJob(session, ['cargo', 'build'], { cwd: path.join(session.dirs.root) });
      await assertRunning(session, 1, 'a job that holds the one place');
      const coder = agent(session, { id: 'coder', title: 'Refuse check', cwd: wt1 });
      await coder.hook('UserPromptSubmit');

      // When
      const refused = await coder.bash(`cd ${wt1} && cargo test`);
      await sleep(500);
      const startedAfterRefusal = startedCount(session.dirs);
      const wrapped = await coder.bash(`cd ${wt1} && wtpool job -- cargo test`);
      await coder.job(['cargo', 'test'], { cwd: wt1 });
      const line = await poll(async () => {
        const row = await agentLines(frame, session, 'wt1');
        return row?.agent.includes('Refuse check · queued: cargo test') ? row : undefined;
      }, WITHIN_A_SECOND_MS * 2, 100) ?? (await agentLines(frame, session, 'wt1'));

      // Then
      const reason = `wtpool: cargo test is heavy on this machine (enforce = require). Run it as: cd ${wt1} && wtpool job -- cargo test`;
      assert.equal(refused.policy.code, 0, `wtpool hook policy exited ${refused.policy.code}: ${refused.policy.stderr} (H29)`);
      let decision = null;
      try {
        decision = JSON.parse(refused.policy.stdout.trim());
      } catch {
        // No decision: the assertion below shows the output.
      }
      assert.deepEqual(
        decision,
        { hookSpecificOutput: { hookEventName: 'PreToolUse', permissionDecision: 'deny', permissionDecisionReason: reason } },
        `under require wtpool hook policy does not refuse cd wt1 && cargo test with the wrapped command: ${JSON.stringify(refused.policy.stdout)} (J21, H29)`,
      );
      assert.equal(refused.policy.stdout.trim().split('\n').length, 1, 'the refusal is not one line (H29)');
      assert.equal(startedAfterRefusal, 1, 'cargo test ran though the call was refused');
      assert.equal(wrapped.policy.stdout, '', `the wrapped command is refused: ${wrapped.policy.stdout} (J21)`);
      assert.ok(line?.agent.includes('Refuse check · queued: cargo test'), `the agent's job does not queue with its line reading queued: cargo test: ${JSON.stringify(line)} (A8)`);
    } finally {
      await stopJobs(session);
    }
  });
});

