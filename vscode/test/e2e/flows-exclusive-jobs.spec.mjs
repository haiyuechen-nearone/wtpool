// Keys need focus in the webview, and another VS Code window would take it, so
// playwright.exclusive.config.mjs runs this file alone.
import assert from 'node:assert/strict';
import { appendFileSync, mkdirSync, renameSync, symlinkSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { expect, test } from '@playwright/test';

import { GROUP_TOKENS } from '../support/grammar.mjs';
import { TEXT } from '../support/jobs.mjs';
import { poll, withFlow, worktreePath } from './flows.mjs';
import {
  addRepoKeys,
  machineTomlPath,
  readText,
  repoTomlPath,
  tomlList,
  writeMachineToml,
} from './jobs-config.mjs';
import {
  focusKey,
  focusRing,
  focusWebview,
  headingText,
  liveText,
  machineButton,
  openStrip,
  openTab,
  stripText,
  tokenColour,
} from './jobs-dom.mjs';
import {
  defaultLimit,
  fourRunTwoWait,
  loggedRuns,
  loggingWtpool,
  names,
  stopJobs,
  WITHIN_A_SECOND_MS,
  wtpoolRun,
} from './jobs-process.mjs';
import { sleep, startProcess } from './vscode.mjs';

/** The content of the codicon glyph the machine button shows, its chevron. */
async function chevron(frame) {
  return frame.locator('tr.pool-heading button.machine-button').evaluate((button) => {
    const glyphs = [];
    for (const el of [button, ...button.querySelectorAll('*')]) {
      for (const pseudo of ['::before', '::after']) {
        const style = getComputedStyle(el, pseudo);
        if (style.content && style.content !== 'none' && style.content !== 'normal' && /codicon/i.test(style.fontFamily)) glyphs.push(style.content);
      }
    }
    return glyphs.at(-1) ?? null;
  });
}

/**
 * Presses Tab, then Shift+Tab, until the focus key passes `matches`, at most `limit` times each
 * way, as a keyboard user looks for a control. Gives the key it reached.
 */
async function pressUntil(session, frame, matches, limit = 30) {
  for (const key of ['Tab', 'Shift+Tab']) {
    for (let step = 0; step < limit; step += 1) {
      const found = await focusKey(frame);
      if (found && matches(found)) return found;
      await session.window.keyboard.press(key);
      await sleep(60);
    }
  }
  return focusKey(frame);
}

test('flow__jobs_open_from_heading', async () => {
  await withFlow('flow__jobs_open_from_heading', { prepare: (poolInfo) => addRepoKeys(poolInfo, { heavy: ['make'] }), beforeLaunch: (dirs) => writeMachineToml(dirs, { limit: 4, enforce: 'warn' }) }, async (session) => {
    let build;
    try {
      // Given
      const frame = await openTab(session);
      const button = await machineButton(frame);
      await fourRunTwoWait(session);
      build = startProcess(session.dirs, 'make', worktreePath(session, 'wt2'));
      const { machine, repository } = names(session);
      const expected = `${repository} · ${machine} · jobs 4/4 · 2 queued · 1 not queued`;
      const heading = await poll(async () => ((await headingText(frame)) === expected ? expected : undefined), WITHIN_A_SECOND_MS * 2, 100) ?? (await headingText(frame));
      const closedGlyph = await chevron(frame);
      const escape = (literal) => literal.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const name = `^${escape(`${machine} · jobs 4/4 · 2 queued · 1 not queued`)}, heavy jobs on ${escape(machine)}: 4 of 4 running, 2 queued, 1 not queued`;
      await expect(button).toHaveAccessibleName(new RegExp(name), { timeout: 2_000 });
      await focusWebview(session, frame);

      // When
      await button.click();
      const byClick = { strip: await stripText(frame), focus: await focusKey(frame), expanded: await button.getAttribute('aria-expanded'), glyph: await chevron(frame) };
      await session.window.keyboard.press('Escape');
      await sleep(200);
      const byEscape = { strip: await stripText(frame), focus: await focusKey(frame), expanded: await button.getAttribute('aria-expanded') };
      await session.window.keyboard.press('Enter');
      await sleep(200);
      const byEnter = { strip: await stripText(frame), focus: await focusKey(frame) };
      await session.window.keyboard.press('Escape');
      await sleep(200);
      await session.window.keyboard.press('Space');
      await sleep(200);
      const bySpace = { strip: await stripText(frame), focus: await focusKey(frame) };
      await button.click();
      await sleep(200);
      const byButton = { strip: await stripText(frame), focus: await focusKey(frame), expanded: await button.getAttribute('aria-expanded') };

      // Then
      assert.equal(heading, expected, 'the heading does not name the jobs, the queued and the not queued (U10)');
      assert.ok(closedGlyph, 'the machine button shows no codicon chevron (U10)');
      assert.ok(byClick.strip !== null, 'a click on the machine button opens no strip (U11)');
      assert.equal(byClick.focus, 'limit', 'opening the strip does not move focus to the Limit field (X20)');
      assert.equal(byClick.expanded, 'true', 'aria-expanded is not true while the strip is open (U10)');
      assert.ok(byClick.glyph && byClick.glyph !== closedGlyph, 'the chevron does not point up while the strip is open (U10)');
      assert.equal(byEscape.strip, null, 'Escape in the strip does not close it (X20)');
      assert.equal(byEscape.focus, 'machine-button', 'focus does not return to the machine button after Escape (X20)');
      assert.equal(byEscape.expanded, 'false', 'aria-expanded is not false once the strip is closed (U10)');
      assert.ok(byEnter.strip !== null && byEnter.focus === 'limit', `Enter on the machine button does not open the strip with focus on Limit: ${JSON.stringify(byEnter)} (U10, X20)`);
      assert.ok(bySpace.strip !== null && bySpace.focus === 'limit', `Space on the machine button does not open the strip with focus on Limit: ${JSON.stringify(bySpace)} (U10, X20)`);
      assert.equal(byButton.strip, null, 'the machine button again does not close the strip (U10)');
      assert.equal(byButton.focus, 'machine-button', 'focus is not on the machine button after it closed the strip (X20)');
    } finally {
      build?.kill();
      await stopJobs(session);
    }
  });
});

/** The chips of a scope's list, each its text, in order. */
async function chips(frame, scope) {
  return frame.locator(`tr.machine-detail ul.heavy-chips[data-scope="${scope}"] li.heavy-chip`).evaluateAll((items) => items.map((li) => li.textContent.replace(/\s+/g, ' ').trim()));
}

/**
 * The text and colour of the `.heavy-error` or `.limit-error` an input's aria-describedby names,
 * with the ids it names.
 */
async function describedText(input) {
  return input.evaluate((el) => {
    const ids = (el.getAttribute('aria-describedby') ?? '').split(/\s+/).filter(Boolean);
    const found = ids.map((id) => document.getElementById(id)).filter(Boolean);
    const error = found.find((e) => e.matches('.heavy-error, .limit-error'));
    return { text: error ? error.textContent.replace(/\s+/g, ' ').trim() : null, colour: error ? getComputedStyle(error).color : null, ids };
  });
}

test('flow__jobs_edit_patterns', async () => {
  const logging = loggingWtpool('flow__jobs_edit_patterns');
  const prepare = (poolInfo) => addRepoKeys(poolInfo, { comment: 'wtpool-verify: kept by every edit (J6)', heavy: ['cargo build', 'cargo test'], buildProcesses: ['protoc'] });
  await withFlow('flow__jobs_edit_patterns', { prepare, wtpoolPath: logging.file, beforeLaunch: (dirs) => writeMachineToml(dirs, { limit: 4 }) }, async (session) => {
    // Given
    const repoToml = repoTomlPath(session.pool);
    const before = readText(repoToml);
    const frame = await openTab(session);
    await machineButton(frame);
    await openStrip(frame);
    await focusWebview(session, frame);
    const inputs = frame.locator('tr.machine-detail input.heavy-input');
    assert.equal(await inputs.count(), 2, 'the strip does not have one input.heavy-input per list (U15)');
    const [machineInput, repoInput] = [inputs.nth(0), inputs.nth(1)];
    const chipsBefore = await chips(frame, 'repository');

    // When
    const entered = await pressUntil(session, frame, (key) => key.startsWith('remove repository'));
    const enteredRing = await focusRing(frame);
    let onProtoc = entered;
    for (let step = 0; step < 6 && !onProtoc?.includes('protoc'); step += 1) {
      await session.window.keyboard.press('ArrowRight');
      onProtoc = await focusKey(frame);
    }
    await session.window.keyboard.press('Delete');
    const removed = await poll(async () => !(await chips(frame, 'repository')).some((c) => c.startsWith('protoc')), WITHIN_A_SECOND_MS * 2, 100);
    const afterRemove = { focus: await focusKey(frame), live: await liveText(frame), runs: loggedRuns(logging.log), toml: readText(repoToml) };
    await pressUntil(session, frame, (key) => key === 'input repository');
    await session.window.keyboard.type('cargo xtask verify');
    await session.window.keyboard.press('Enter');
    const added = await poll(async () => (await chips(frame, 'repository')).some((c) => c.startsWith('cargo xtask verify')), WITHIN_A_SECOND_MS * 2, 100);
    const afterAdd = { runs: loggedRuns(logging.log), toml: readText(repoToml) };
    await repoInput.fill('make *');
    await repoInput.press('Enter');
    await sleep(500);
    const notPattern = await describedText(repoInput);
    await repoInput.fill('cargo build');
    await repoInput.press('Enter');
    await sleep(500);
    const already = await describedText(repoInput);
    const afterRefused = { runs: loggedRuns(logging.log), toml: readText(repoToml) };
    await machineInput.fill('just');
    await machineInput.press('Enter');
    const machineAdded = await poll(async () => (await chips(frame, 'machine')).some((c) => c.startsWith('just')), WITHIN_A_SECOND_MS * 2, 100);
    const machineToml = readText(machineTomlPath(session.dirs));
    const problem = await tokenColour(frame, GROUP_TOKENS.problem);

    // Then
    assert.ok(chipsBefore.some((c) => c.startsWith('protoc') && c.includes('build_processes')), `This repository shows no protoc chip tagged build_processes: ${JSON.stringify(chipsBefore)} (U15)`);
    assert.equal(entered, `remove repository ${chipsBefore[0]}`, `Tab into This repository's chips does not land on the first remove button (X22)`);
    assert.ok(enteredRing, 'the focused remove button shows no ring (X22, X14)');
    assert.ok(onProtoc?.includes('protoc'), `the arrow keys do not reach protoc's remove button: ${onProtoc} (X22)`);
    assert.deepEqual(afterRemove.runs.slice(0, 1), [['job', 'heavy', 'rm', '--repo', '--', 'protoc']], 'Delete does not run wtpool job heavy rm --repo -- protoc (X21)');
    assert.ok(removed, 'within 1 s the protoc chip does not go (W10, U15)');
    assert.ok(afterRemove.live.includes('Removed protoc.'), `the live region does not say Removed protoc.: ${afterRemove.live} (X22)`);
    const index = chipsBefore.findIndex((c) => c.startsWith('protoc'));
    const next = chipsBefore[index + 1] ?? chipsBefore[index - 1];
    assert.equal(afterRemove.focus, `remove repository ${next}`, `focus does not move to the next chip, else the previous, after the removal (X22)`);
    assert.equal(tomlList(afterRemove.toml, 'build_processes')?.includes('protoc') ?? false, false, `.git/wtpool.toml still names protoc: ${afterRemove.toml} (J3)`);
    assert.deepEqual(afterAdd.runs.slice(1), [['job', 'heavy', 'add', '--repo', '--', 'cargo xtask verify']], 'Enter in This repository\'s input does not run wtpool job heavy add --repo -- \'cargo xtask verify\' (X21)');
    assert.ok(added, 'the cargo xtask verify chip does not appear (U15)');
    for (const line of before.split('\n').filter((l) => l && !/^\s*(heavy|build_processes)\s*=/.test(l))) {
      assert.ok(afterAdd.toml.includes(line), `.git/wtpool.toml lost ${JSON.stringify(line)} (J6)`);
    }
    assert.ok(tomlList(afterAdd.toml, 'heavy')?.includes('cargo xtask verify'), `.git/wtpool.toml does not list cargo xtask verify: ${afterAdd.toml} (J6)`);
    assert.equal(notPattern.text, TEXT.patternError, `make * does not show why it is not a pattern under the input: ${JSON.stringify(notPattern)} (U15, J18)`);
    assert.equal(notPattern.colour, problem, `the error under the input is not in ${GROUP_TOKENS.problem} (U15)`);
    assert.equal(already.text, 'Already in this list.', `cargo build does not show Already in this list.: ${JSON.stringify(already)} (U15)`);
    assert.deepEqual(afterRefused.runs, afterAdd.runs, 'a refused pattern ran wtpool (U15)');
    assert.equal(afterRefused.toml, afterAdd.toml, 'a refused pattern changed .git/wtpool.toml');
    assert.deepEqual(loggedRuns(logging.log).slice(2), [['job', 'heavy', 'add', '--', 'just']], 'Enter in This machine\'s input does not run wtpool job heavy add -- just (X21)');
    assert.ok(machineAdded, 'the just chip does not appear in This machine (U15)');
    assert.ok(tomlList(machineToml, 'heavy')?.includes('just'), `machine.toml does not list just: ${machineToml} (J3)`);
  });
});

test('flow__jobs_invalid_value', async () => {
  const logging = loggingWtpool('flow__jobs_invalid_value');
  await withFlow('flow__jobs_invalid_value', { wtpoolPath: logging.file, beforeLaunch: (dirs) => writeMachineToml(dirs, { limit: 4 }) }, async (session) => {
    // Given
    const frame = await openTab(session);
    await machineButton(frame);
    const strip = await openStrip(frame);
    await focusWebview(session, frame);
    const limit = strip.locator('input.job-limit');
    assert.equal(await limit.count(), 1, 'the strip has no input.job-limit (U11)');
    const problem = await tokenColour(frame, GROUP_TOKENS.problem);

    // When
    const tried = [];
    for (const value of ['0', '65', '2.5', 'four', '']) {
      // Typed key by key: a number input would refuse `four` from fill().
      await limit.fill('');
      if (value) await limit.pressSequentially(value);
      await session.window.keyboard.press('Enter');
      await sleep(300);
      tried.push({ value, invalid: await limit.getAttribute('aria-invalid'), error: await describedText(limit), live: await liveText(frame), focus: await focusKey(frame) });
    }
    const runsAfterInvalid = loggedRuns(logging.log);
    // wtpool refuses a write through a symlinked machine.toml, naming its target.
    const file = machineTomlPath(session.dirs);
    const target = path.join(session.dirs.root, 'machine-target.toml');
    renameSync(file, target);
    symlinkSync(target, file);
    await limit.fill('6');
    await session.window.keyboard.press('Enter');
    const status = strip.locator('.limit-status');
    const refusal = await poll(async () => {
      const text = ((await status.textContent()) ?? '').trim();
      return text && text !== 'Saved.' ? text : undefined;
    }, WITHIN_A_SECOND_MS * 3, 100);
    const field = await limit.inputValue();
    const cli = wtpoolRun(session, ['job', 'limit', '6']);
    const lastLine = cli.stderr.split('\n').map((l) => l.trim()).filter(Boolean).at(-1);

    // Then
    for (const t of tried) {
      assert.equal(t.invalid, 'true', `${JSON.stringify(t.value)} does not mark the field invalid (X20)`);
      assert.equal(t.error.text, 'Enter a whole number from 1 to 64.', `${JSON.stringify(t.value)} does not show the bounds under the field, tied by aria-describedby: ${JSON.stringify(t.error)} (X20)`);
      assert.equal(t.error.colour, problem, `the limit's error is not in ${GROUP_TOKENS.problem} (X20)`);
      assert.ok(t.live.includes('Enter a whole number from 1 to 64.'), `${JSON.stringify(t.value)}: the live region does not announce the error: ${t.live} (X20)`);
      assert.equal(t.focus, 'limit', `${JSON.stringify(t.value)}: focus left the Limit field (X20)`);
    }
    assert.deepEqual(runsAfterInvalid, [], 'an invalid limit ran wtpool (X20, X11)');
    assert.notEqual(cli.code, 0, `setup: wtpool job limit 6 through a symlinked machine.toml exited 0: ${cli.stdout}`);
    assert.equal(refusal, lastLine, 'the strip does not show wtpool\'s refusal (X21, R10 wording)');
    assert.equal(field, '6', 'the refused 6 does not stay in the field (U11)');
  });
});

test('flow__jobs_first_open', async () => {
  const logging = loggingWtpool('flow__jobs_first_open');
  const prepare = (poolInfo) => {
    const scripts = { build: 'node build.js', test: 'node test.js', 'test:e2e': 'node e2e.js', start: 'node serve.js' };
    writeFileSync(path.join(poolInfo.fg, 'Makefile'), 'all:\n\ttrue\n');
    writeFileSync(path.join(poolInfo.fg, 'package.json'), JSON.stringify({ name: 'shop', scripts }, null, 2));
    writeFileSync(path.join(poolInfo.fg, 'pnpm-lock.yaml'), "lockfileVersion: '9.0'\n");
    mkdirSync(path.join(poolInfo.common, 'info'), { recursive: true });
    // The markers stay out of git, so the foreground's row reads as the scenario's.
    appendFileSync(path.join(poolInfo.common, 'info', 'exclude'), 'Makefile\npackage.json\npnpm-lock.yaml\n');
  };
  await withFlow('flow__jobs_first_open', { prepare, wtpoolPath: logging.file, earlierBuilds: false }, async (session) => {
    // Given
    const frame = await openTab(session);
    await machineButton(frame);
    const { machine, repository } = names(session);
    const expected = `${repository} · ${machine} · jobs 0/${defaultLimit()} · 7 suggested`;
    const heading = await poll(async () => ((await headingText(frame)) === expected ? expected : undefined), WITHIN_A_SECOND_MS * 2, 100) ?? (await headingText(frame));
    const muted = await frame.evaluate(() => {
      const button = document.querySelector('tr.pool-heading button.machine-button');
      const holder = [...(button?.querySelectorAll('*') ?? [])].filter((el) => /7 suggested/.test(el.textContent)).at(-1);
      return holder ? getComputedStyle(holder).color : null;
    });
    const description = await tokenColour(frame, '--vscode-descriptionForeground');
    await openStrip(frame);
    await focusWebview(session, frame);
    const strip = await stripText(frame);
    const suggested = await frame.locator('tr.machine-detail ul.heavy-suggested li.heavy-chip[data-from]').evaluateAll((items) =>
      items.map((li) => ({ from: li.dataset.from, text: li.textContent.replace(/\s+/g, ' ').trim() })),
    );
    const flagged = await frame.evaluate(() => document.querySelectorAll('.not-queued').length);

    // When
    let onMake = await pressUntil(session, frame, (key) => key.startsWith('suggest'));
    for (let step = 0; step < 8 && !onMake?.startsWith('suggest make'); step += 1) {
      await session.window.keyboard.press('ArrowRight');
      onMake = await focusKey(frame);
    }
    await session.window.keyboard.press('Enter');
    const moved = await poll(async () => (await chips(frame, 'repository')).some((c) => c.startsWith('make')), WITHIN_A_SECOND_MS * 2, 100);
    const headingAfterMake = await headingText(frame);
    const runsAfterMake = loggedRuns(logging.log);
    await pressUntil(session, frame, (key) => key === 'add-all');
    await session.window.keyboard.press('Enter');
    const gone = await poll(async () => (await frame.locator('tr.machine-detail ul.heavy-suggested li.heavy-chip').count()) === 0, WITHIN_A_SECOND_MS * 3, 100);
    const live = await liveText(frame);
    const runs = loggedRuns(logging.log);
    const repoHeavy = tomlList(readText(repoTomlPath(session.pool)), 'heavy');

    // Then
    const pnpm = ['pnpm run build', 'pnpm build', 'pnpm run test', 'pnpm test', 'pnpm run test:e2e', 'pnpm test:e2e'];
    assert.equal(heading, expected, 'the heading of a repository with no list does not count its 7 suggestions (U10, J23, J24)');
    assert.equal(muted, description, 'the · 7 suggested part is not in --vscode-descriptionForeground (U10)');
    assert.ok(/This machine none/i.test(strip ?? '') && /This repository · \.git\/wtpool\.toml none/i.test(strip ?? ''), `This machine and This repository do not read none: ${strip} (U15)`);
    const byFrom = (from) => suggested.filter((s) => s.from === from).map((s) => s.text).sort();
    assert.deepEqual(byFrom('Makefile'), ['make'], `the suggestions from Makefile: ${JSON.stringify(suggested)} (J23)`);
    assert.deepEqual(byFrom('package.json'), [...pnpm].sort(), `the suggestions from package.json scripts under pnpm: ${JSON.stringify(suggested)} (J24)`);
    assert.equal(suggested.length, 7, `suggestions beyond the seven, start among them? ${JSON.stringify(suggested)} (J24)`);
    assert.ok(/from Makefile/i.test(strip ?? '') && /from package\.json scripts/i.test(strip ?? ''), `the suggestions are not grouped by from: ${strip} (U15)`);
    assert.equal(flagged, 0, 'under warn a suggestion flags a process (J20)');
    assert.ok(onMake?.startsWith('suggest make'), `Tab and the arrows do not reach make's add button: ${onMake} (X22)`);
    assert.deepEqual(runsAfterMake, [['job', 'heavy', 'add', '--repo', '--', 'make']], 'Enter on make does not run wtpool job heavy add --repo -- make (X21)');
    assert.ok(moved, 'the make chip does not move to This repository (U15)');
    assert.ok(!headingAfterMake.includes('suggested'), `the heading keeps · N suggested after the first add: ${headingAfterMake} (U10)`);
    assert.equal(runs.length, 2, `Add all does not run wtpool once: ${JSON.stringify(runs)} (X21)`);
    assert.deepEqual(runs[1]?.slice(0, 5), ['job', 'heavy', 'add', '--repo', '--'], `Add all does not run wtpool job heavy add --repo --: ${JSON.stringify(runs[1])} (X21)`);
    assert.deepEqual([...(runs[1]?.slice(5) ?? [])].sort(), [...pnpm].sort(), 'Add all does not add the six shown suggestions (X21)');
    assert.ok(live.includes('Added 6 suggestions.'), `the live region does not say Added 6 suggestions.: ${live} (X22)`);
    assert.ok(gone, 'the suggestions group does not go once every suggestion is added (U15)');
    assert.deepEqual([...(repoHeavy ?? [])].sort(), ['make', ...pnpm].sort(), 'the repository\'s heavy list does not hold the seven (J3)');
  });
});
