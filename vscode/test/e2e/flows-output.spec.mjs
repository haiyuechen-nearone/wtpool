// A `claude` stub's hooks report a background Bash task in wt2, whose output file in the sandbox's
// Claude Code temp folder grows. The release wtpool records only the file's path, and the extension
// reads the file.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { appendFileSync, mkdirSync, realpathSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { test } from '@playwright/test';

import { startStub } from './agents.mjs';
import { openStatus, poll, rowOf, withFlow, worktreePath } from './flows.mjs';
import { env, releaseWtpool } from './vscode.mjs';

const SESSION = 'e2e-live-output';
const TASK = 'b1live01';
const SLUG = '-verify-pool';
const CALL = 'toolu_e2e_live_output_1';

/** The text of an element, spaces collapsed. */
const textOf = async (locator) => ((await locator.textContent({ timeout: 1_000 }).catch(() => '')) ?? '').replace(/\s+/g, ' ').trim();

test('flow__live_output', async () => {
  // The terminal draws its rows as DOM text only without GPU acceleration.
  const settings = { 'terminal.integrated.gpuAcceleration': 'off' };
  await withFlow('flow__live_output', { settings }, async (session) => {
    // Given
    const frame = await openStatus(session);
    const { dirs, pool: poolInfo } = session;
    const wt2 = worktreePath(session, 'wt2');
    const base = path.join(dirs.root, 'claude-tmp');
    mkdirSync(base, { recursive: true });
    const tasks = path.join(realpathSync(base), `claude-${process.getuid?.() ?? 0}`, SLUG, SESSION, 'tasks');
    mkdirSync(tasks, { recursive: true });
    const file = path.join(tasks, `${TASK}.output`);
    writeFileSync(file, '   Compiling app v0.1.0 (wt2)\n');
    const stub = startStub();
    const variables = { ...env(dirs, poolInfo), CLAUDE_CODE_TMPDIR: base };
    const common = { session_id: SESSION, cwd: wt2, transcript_path: `/Users/verify/.claude/projects/${SLUG}/${SESSION}.jsonl`, permission_mode: 'default' };
    const input = { command: 'cargo build --release', description: 'Build the release', run_in_background: true };
    const hooks = [
      { ...common, hook_event_name: 'UserPromptSubmit', prompt: 'build it' },
      { ...common, hook_event_name: 'PreToolUse', tool_name: 'Bash', tool_input: input, tool_use_id: CALL },
      {
        ...common,
        hook_event_name: 'PostToolUse',
        tool_name: 'Bash',
        tool_input: input,
        tool_use_id: CALL,
        tool_response: { stdout: '', stderr: '', interrupted: false, isImage: false, noOutputExpected: false, backgroundTaskId: TASK },
      },
    ];
    try {
      for (const payload of hooks) {
        const reply = await stub.send({ binary: releaseWtpool(), payload, cwd: wt2, env: variables });
        assert.equal(reply.code, 0, `setup: wtpool hook exited ${reply.code ?? reply.signal}: ${reply.stderr}`);
      }

      // When: wt2 is selected and the file grows.
      const row = rowOf(frame, session, 'wt2');
      const working = await poll(async () => (await textOf(row)).includes('cargo build'), 10_000);
      await row.click();
      const tail = frame.locator(`tr.detail .output[data-output-path="${file}"] pre.output-tail`);
      const first = await poll(async () => (await textOf(tail)).includes('Compiling app v0.1.0 (wt2)'), 5_000);
      const heading = await textOf(frame.locator(`tr.detail .output[data-output-path="${file}"] .output-heading`));
      // The open strip follows a commit in wt2 (git facts) and the agent's next call (its action).
      const strip = frame.locator('tr.detail');
      const before = await textOf(strip);
      execFileSync('git', ['-C', wt2, 'commit', '--allow-empty', '-q', '-m', 'live strip'], { env: variables, timeout: 30_000 });
      const ahead = await poll(async () => (await textOf(strip)).includes('2 ahead'), 5_000, 100);
      const edit = { ...common, hook_event_name: 'PreToolUse', tool_name: 'Edit', tool_input: { file_path: `${wt2}/src/lib.rs`, old_string: 'a', new_string: 'b' }, tool_use_id: `${CALL}_edit` };
      const editReply = await stub.send({ binary: releaseWtpool(), payload: edit, cwd: wt2, env: variables });
      const action = await poll(async () => (await textOf(strip)).includes('Edit src/lib.rs'), 5_000, 100);
      const delays = [];
      for (const line of ['   Compiling widget v0.2.0', '    Finished `release` profile [optimized] target(s) in 4.20s']) {
        const wrote = Date.now();
        appendFileSync(file, `${line}\n`);
        const shown = await poll(async () => (await textOf(tail)).includes(line.trim()), 5_000, 50);
        delays.push(shown ? Date.now() - wrote : Infinity);
      }

      // And: Follow opens a terminal that shows the file, then its new lines.
      const follow = frame.locator(`tr.detail .output[data-output-path="${file}"] button.follow`);
      const followed = (await follow.count()) === 1 && (await follow.click({ timeout: 5_000 }).then(() => true, () => false));
      const terminal = session.window.locator('.terminal-wrapper .xterm-rows').first();
      const showsFile = followed && (await poll(async () => (await textOf(terminal)).includes('Compiling widget v0.2.0'), 15_000));
      appendFileSync(file, 'test result: ok. 12 passed\n');
      const followsFile = showsFile && (await poll(async () => (await textOf(terminal)).includes('test result: ok. 12 passed'), 5_000));
      const tabs = await textOf(session.window.locator('.integrated-terminal, .terminal-tabs-entry, .single-terminal-tab').first());

      // Then
      assert.ok(working, `wt2 does not show the agent's background build: ${await textOf(row)}`);
      assert.ok(before.includes('1 ahead'), `setup: the strip does not show wt2 one commit ahead: ${before}`);
      assert.ok(ahead, `the open strip did not follow a commit in wt2: ${await textOf(strip)}`);
      assert.equal(editReply.code, 0, `setup: wtpool hook exited ${editReply.code ?? editReply.signal}`);
      assert.ok(action, `the open strip did not follow the agent's next call: ${await textOf(strip)}`);
      assert.ok(first, `the strip's Output section does not show the tail: ${await textOf(tail)}`);
      assert.equal(heading, 'cargo build · running', 'the task heading does not read the summary and running');
      assert.ok(delays.every((ms) => ms <= 1000), `the tail took ${delays.join(', ')} ms to show a new line, over 1 s`);
      assert.ok(followed, 'the task has no Follow button that can be clicked');
      assert.ok(showsFile, `Follow opened no terminal showing the file: ${await textOf(terminal)} ${tabs}`);
      assert.ok(followsFile, 'the Follow terminal did not show a new line');
    } finally {
      stub.child.kill('SIGTERM');
      await stub.exited;
    }
  });
});
