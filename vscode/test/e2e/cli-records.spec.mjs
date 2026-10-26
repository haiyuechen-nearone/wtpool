// H25, H26 and S11 with the release wtpool, no VS Code window: records name their harness and each
// call a neutral kind, `wtpool status` reads the kinds and never the tool names, and every actor
// names its harness, also from a record an older wtpool wrote (status-panel.md revision 16).
import assert from 'node:assert/strict';
import path from 'node:path';
import { test } from '@playwright/test';

import { poll } from './flows.mjs';
import {
  claude,
  cliSandbox,
  payload,
  readRecord,
  recordFiles,
  secondsAgo,
  statusDocument,
  worktreeOf,
  writeRecord,
} from './cli.mjs';
import { pool, startProcess } from './vscode.mjs';

test('hook__should_tag_records_with_the_harness', async () => {
  // Given
  const dirs = cliSandbox('harness');
  const poolInfo = pool('idle_pool', 'harness');
  const session = claude(dirs, poolInfo);

  // When
  try {
    await session.run(payload('UserPromptSubmit', poolInfo));
    await session.run(payload('PreToolUse-Read', poolInfo));
    await session.run(payload('SubagentStart', poolInfo));
    await session.run(payload('PreToolUse-Read-subagent', poolInfo));
  } finally {
    await session.stop();
  }

  // Then
  const files = recordFiles(poolInfo);
  assert.deepEqual(files, ['s1.a1.json', 's1.json'], 'Claude Code records keep the stems <session_id>.json and <session_id>.<agent_id>.json');
  const harnesses = Object.fromEntries(files.map((file) => [file, readRecord(poolInfo, file.slice(0, -'.json'.length)).harness]));
  assert.deepEqual(harnesses, { 's1.a1.json': 'claude-code', 's1.json': 'claude-code' }, 'the harness each record names');
});

/** Each PreToolUse fixture with the kind the Claude Code adapter gives its tool (H26). */
const KINDS = [
  ['PreToolUse-Bash', 'Bash', 'shell'],
  ['PreToolUse-Edit', 'Edit', 'edit'],
  ['PreToolUse-Write', 'Write', 'edit'],
  ['PreToolUse-NotebookEdit', 'NotebookEdit', 'edit'],
  ['PreToolUse-Agent', 'Agent', 'agent'],
  ['PreToolUse-Read', 'Read', 'other'],
  ['PreToolUse-Grep', 'Grep', 'other'],
  ['PreToolUse-Glob', 'Glob', 'other'],
  ['PreToolUse-EnterWorktree', 'EnterWorktree', 'other'],
  ['PreToolUse-WebSearch', 'WebSearch', 'other'],
  ['PreToolUse-PowerShell', 'PowerShell', 'other'],
  ['PreToolUse-Monitor', 'Monitor', 'other'],
  ['PreToolUse-WebSearch', 'mcp__acme__lookup', 'other'],
];

test('hook__should_give_each_call_its_kind', async () => {
  // Given
  const dirs = cliSandbox('kinds');
  const poolInfo = pool('idle_pool', 'kinds');
  const session = claude(dirs, poolInfo);
  const id = (index) => `toolu_kind_${index}`;
  const ended = [];
  let flying;

  // When
  try {
    await session.run(payload('UserPromptSubmit', poolInfo));
    for (const [index, [fixture, tool]] of KINDS.entries()) {
      await session.run(payload(fixture, poolInfo, { tool_name: tool, tool_use_id: id(index) }));
    }
    flying = readRecord(poolInfo, 's1').in_flight;
    for (const [fixture, index] of [['PostToolUse-Bash', 0], ['PostToolUse-Edit', 1], ['PostToolUse-Agent', 4], ['PostToolUse-Read', 5]]) {
      await session.run(payload(fixture, poolInfo, { tool_use_id: id(index) }));
      const record = readRecord(poolInfo, 's1');
      ended.push(record.ended && { id: record.ended.id, tool: record.ended.tool, kind: record.ended.kind });
    }
  } finally {
    await session.stop();
  }

  // Then
  const byId = Object.fromEntries((flying ?? []).map((call) => [call.id, { tool: call.tool, kind: call.kind }]));
  const expected = Object.fromEntries(KINDS.map(([, tool, kind], index) => [id(index), { tool, kind }]));
  assert.deepEqual(byId, expected, 'the tool and kind of each call in flight');
  assert.deepEqual(
    ended,
    [0, 1, 4, 5].map((index) => ({ id: id(index), tool: KINDS[index][1], kind: KINDS[index][2] })),
    'the id, tool and kind of the call each PostToolUse ended',
  );
});

test('status__should_give_each_actor_its_harness', async () => {
  // Given: a session and its subagent from one claude, a second session from another whose
  // record an older wtpool wrote, without harness and kinds.
  const dirs = cliSandbox('actor-harness');
  const poolInfo = pool('idle_pool', 'actor-harness');
  const first = claude(dirs, poolInfo);
  const second = claude(dirs, poolInfo);
  let status;
  try {
    await first.run(payload('UserPromptSubmit', poolInfo));
    await first.run(payload('SubagentStart', poolInfo));
    await first.run(payload('PreToolUse-Read-subagent', poolInfo));
    const wt3 = path.join(poolInfo.dir, 'wt3');
    await second.run(payload('UserPromptSubmit', poolInfo, { session_id: 's2', cwd: wt3 }));
    await second.run(payload('PreToolUse-Bash', poolInfo, { session_id: 's2', cwd: wt3, tool_input: { command: 'ls', description: 'List' } }));
    const older = readRecord(poolInfo, 's2');
    delete older.harness;
    for (const call of older.in_flight ?? []) delete call.kind;
    if (older.ended) delete older.ended.kind;
    writeRecord(dirs, poolInfo, 's2', older);

    // When
    status = statusDocument(dirs, poolInfo);
  } finally {
    await first.stop();
    await second.stop();
  }

  // Then
  const actors = Object.fromEntries(status.pools[0].worktrees.flatMap((w) => w.actors).map((a) => [a.key, a.harness]));
  assert.deepEqual(actors, { s1: 'claude-code', 's1.a1': 'claude-code', s2: 'claude-code' }, 'the harness of each actor');
});

/** A call in flight, as a record holds it: `kind` left out as an older wtpool wrote it. */
function call(id, tool, kind) {
  return { id, tool, ...(kind ? { kind } : {}), target: 'x', since: secondsAgo(600) };
}

test('status__should_read_kinds_only', async () => {
  // Given: session s1 in wt1, its record 10 minutes old so only a call in flight keeps it
  // working (A1), first with a build in wt1, then with a working subagent.
  const dirs = cliSandbox('read-kinds');
  const poolInfo = pool('idle_pool', 'read-kinds');
  const wt1 = path.join(poolInfo.dir, 'wt1');
  const session = claude(dirs, poolInfo);
  const seen = [];
  let build;
  const show = (label, calls, base) => {
    writeRecord(dirs, poolInfo, 's1', {
      ...base,
      state: 'working',
      waiting: null,
      since: secondsAgo(900),
      updated: secondsAgo(600),
      in_flight: calls,
    });
    const row = worktreeOf(statusDocument(dirs, poolInfo), 'wt1');
    seen.push([label, row.actors.find((a) => a.key === 's1')?.state ?? 'not shown', row.activity]);
  };
  try {
    await session.run(payload('UserPromptSubmit', poolInfo, { cwd: wt1 }));
    await session.run(payload('PreToolUse-Read', poolInfo, { cwd: wt1 }));
    const base = readRecord(poolInfo, 's1');
    build = startProcess(dirs, 'cargo', wt1);
    const building = await poll(() => worktreeOf(statusDocument(dirs, poolInfo), 'wt1').builds.length > 0, 10_000);
    assert.ok(building, 'setup: wtpool status shows no build in wt1');

    // When
    show('unknown tool, kind shell, a build', [call('c1', 'Frobnicate', 'shell')], base);
    show('Bash, kind other, a build', [call('c2', 'Bash', 'other')], base);
    show('Bash without kind, a build', [call('c3', 'Bash')], base);
    build.kill();
    build = undefined;
    const stopped = await poll(() => worktreeOf(statusDocument(dirs, poolInfo), 'wt1').builds.length === 0, 10_000);
    assert.ok(stopped, 'setup: the build in wt1 still shows');
    await session.run(payload('SubagentStart', poolInfo, { cwd: wt1 }));
    show('unknown tool, kind agent, a working subagent', [call('c4', 'Spawn', 'agent')], base);
    show('Agent, kind other, a working subagent', [call('c5', 'Agent', 'other')], base);
    show('Agent without kind, a working subagent', [call('c6', 'Agent')], base);
  } finally {
    build?.kill();
    await session.stop();
  }

  // Then: [case, s1's state, wt1's activity] (A1, A4).
  assert.deepEqual(
    seen,
    [
      ['unknown tool, kind shell, a build', 'working', 'working'],
      ['Bash, kind other, a build', 'quiet', 'building'],
      ['Bash without kind, a build', 'working', 'working'],
      ['unknown tool, kind agent, a working subagent', 'working', 'working'],
      ['Agent, kind other, a working subagent', 'quiet', 'working'],
      ['Agent without kind, a working subagent', 'working', 'working'],
    ],
    "s1's state and wt1's activity by the kind of the call in flight, never its tool",
  );
});
