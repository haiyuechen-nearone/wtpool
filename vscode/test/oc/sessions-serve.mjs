// The oc layer's sessions on an `opencode serve` of their own, driven over its HTTP API: waits,
// interrupts, several sessions in one process, deletion, titles and the shell's environment.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { sleep } from '../support/processes.mjs';
import { Session, expectOpenCode, stemOf } from './opencode.mjs';
import { call, say, slow } from './stub-model.mjs';

const ASK = { permission: { bash: 'ask' } };

/** The record of session `id` of OpenCode, or undefined. */
const recordOf = (s, id) => s.records().find((r) => r.file === `${stemOf(id)}.json`);

/** The actor of session `id` in each sample from `from` to `to`, where it shows. */
function shown(s, id, from = 0, to = Infinity) {
  const stem = stemOf(id);
  return s.samples.filter((x) => x.at >= from && x.at <= to).flatMap((x) => x.actors.filter((a) => a.key === stem));
}

/** Waits until the actor of session `id` shows with `state`, at most `limitMs`. */
async function until(s, id, test, limitMs = 10_000) {
  const deadline = Date.now() + limitMs;
  while (Date.now() < deadline) {
    const actor = s.actors().find((a) => a.key === stemOf(id));
    if (actor && test(actor)) return actor;
    await sleep(250);
  }
  return s.actors().find((a) => a.key === stemOf(id)) ?? null;
}

/** A shell call held at its permission `holdMs`, then answered `decision`. */
async function heldPermission(context, name, decision) {
  const s = new Session(context, name);
  s.config(ASK);
  await s.serve();
  const wt3 = s.wt('wt3');
  context.stub.scripts[name] = [call('shell', { command: `cd ${wt3} && git status && sleep 3` }), slow(10, 100)];
  const id = await s.create(s.pool.fg);
  const from = s.mark();
  const stop = s.sampleActors(400);
  await s.prompt(id, `[[scenario:${name}]] Run the status command with the shell tool.`);
  const asked = await s.waitEvent((e) => e.type === 'permission.asked' && e.data?.sessionID === id, 60_000, from);
  expectOpenCode(asked, 'no permission.asked');
  const holdStart = Date.now();
  await sleep(5000);
  const holdEnd = Date.now();
  const recorded = s.recordedEvents('permission.asked').some((l) => l.payload.data?.id === asked.data.id);
  expectOpenCode(recorded, 'the recorder did not see permission.asked during the hold');
  const replied = await s.api('POST', `/api/session/${id}/permission/${asked.data.id}/reply`, { decision });
  expectOpenCode(replied.status < 300, `the reply ${decision} answered ${replied.status}`);
  const afterReply = Date.now();
  const end = await s.turnEnd(id, from);
  await sleep(1500);
  await stop();
  const during = shown(s, id, holdStart + 1500, holdEnd - 500);
  assert.ok(during.length >= 3, `the session showed in ${during.length} samples during the hold`);
  assert.ok(
    during.every((a) => a.state === 'waiting' && a.reason === 'permission'),
    `during the hold: ${JSON.stringify(during.map((a) => [a.state, a.reason]))}`,
  );
  return { s, id, end, afterReply };
}

async function permission(context) {
  const { s, id, end, afterReply } = await heldPermission(context, 'oc_permission', 'once');
  expectOpenCode(end?.type === 'session.execution.succeeded', `the turn ended ${end?.type}`);
  const success = s.recordedEvents('session.tool.success')[0];
  const working = shown(s, id, afterReply + 300, (success?.t ?? afterReply + 3000) - 200);
  assert.ok(working.some((a) => a.state === 'working'), `not working after the reply: ${JSON.stringify(working.map((a) => a.state))}`);
  const final = await until(s, id, (a) => a.state === 'idle');
  assert.equal(final?.state, 'idle', 'the session is not idle at the end');
  return 'waiting during the hold, working after the reply, idle at the end';
}

async function permissionReject(context) {
  const { s, id, end } = await heldPermission(context, 'oc_permission_reject', 'reject');
  const lines = s.recorded();
  const failed = lines.findIndex((l) => l.kind === 'event' && l.name === 'session.tool.failed');
  const interrupted = lines.findIndex((l) => l.kind === 'event' && l.name === 'session.execution.interrupted');
  expectOpenCode(failed >= 0 && interrupted > failed, `session.tool.failed at ${failed}, then interrupted at ${interrupted}`);
  expectOpenCode(end?.type === 'session.execution.interrupted', `the turn ended ${end?.type}`);
  const final = await until(s, id, (a) => a.state === 'idle');
  assert.equal(final?.state, 'idle', 'the session is not idle after the reject');
  assert.deepEqual(recordOf(s, id)?.in_flight ?? [], [], 'a call is still in flight');
  return 'waiting during the hold, idle after the reject, nothing in flight';
}

async function question(context) {
  const s = new Session(context, 'oc_question');
  s.config();
  await s.serve();
  context.stub.scripts.oc_question = [
    call('question', {
      questions: [
        {
          question: 'Which colour should the notes use?',
          header: 'Colour',
          options: [
            { label: 'Teal', description: 'Keep the current colour' },
            { label: 'Orange', description: 'Switch to orange' },
          ],
        },
      ],
    }),
    slow(40, 100),
  ];
  const id = await s.create(s.pool.fg);
  const from = s.mark();
  const stop = s.sampleActors(400);
  await s.prompt(id, '[[scenario:oc_question]] Ask me which colour the notes should use.');
  const created = await s.waitEvent((e) => e.type === 'form.created' && e.data?.form?.sessionID === id, 60_000, from);
  expectOpenCode(created, 'no form.created');
  const holdStart = Date.now();
  await sleep(3000);
  const holdEnd = Date.now();
  const field = created.data.form.fields?.[0];
  const value = field?.options?.[1]?.value ?? 'Orange';
  const answer = { [field?.key ?? 'q0']: field?.type === 'multiselect' ? [value] : value };
  const replied = await s.api('POST', `/api/session/${id}/form/${created.data.form.id}/reply`, { answer });
  expectOpenCode(replied.status < 300, `the answer got ${replied.status}`);
  const afterReply = Date.now();
  const end = await s.turnEnd(id, from);
  await stop();
  expectOpenCode(end?.type === 'session.execution.succeeded', `the turn ended ${end?.type}`);
  const during = shown(s, id, holdStart + 800, holdEnd - 300);
  assert.ok(during.length >= 2 && during.every((a) => a.state === 'waiting' && a.reason === 'question'), `during the question: ${JSON.stringify(during.map((a) => [a.state, a.reason]))}`);
  const after = shown(s, id, afterReply + 300, end.receivedAt - 300);
  assert.ok(after.some((a) => a.state === 'working'), `not working after the answer: ${JSON.stringify(after.map((a) => a.state))}`);
  const final = await until(s, id, (a) => a.state === 'idle');
  assert.equal(final?.state, 'idle', 'the session is not idle at the end');
  return 'waiting for the question, working after the answer, idle at the end';
}

async function interrupt(context) {
  const s = new Session(context, 'oc_interrupt');
  s.config(ASK);
  await s.serve();
  context.stub.scripts.oc_interrupt_stream = [slow(400, 250)];
  context.stub.scripts.oc_interrupt_wait = [call('shell', { command: 'sleep 30; echo slept' }), say('Slept.')];
  const stop = s.sampleActors(400);
  const outcomes = [];
  for (const [script, waitFor] of [
    ['oc_interrupt_stream', (e, id) => e.type === 'session.text.delta' && e.data?.sessionID === id],
    ['oc_interrupt_wait', (e, id) => e.type === 'permission.asked' && e.data?.sessionID === id],
  ]) {
    const id = await s.create(s.pool.fg);
    const from = s.mark();
    await s.prompt(id, `[[scenario:${script}]] Go on slowly.`);
    const started = await s.waitEvent((e) => waitFor(e, id), 60_000, from);
    expectOpenCode(started, `${script}: the turn did not reach its interrupt point`);
    await sleep(3000);
    const sent = await s.api('POST', `/api/session/${id}/interrupt`);
    expectOpenCode(sent.status < 300, `${script}: the interrupt got ${sent.status}`);
    const end = await s.turnEnd(id, from);
    expectOpenCode(end?.type === 'session.execution.interrupted' && end.data?.reason === 'user', `${script}: the turn ended ${end?.type} ${end?.data?.reason}`);
    const final = await until(s, id, (a) => a.state === 'idle');
    assert.equal(final?.state, 'idle', `${script}: not idle after the interrupt`);
    assert.deepEqual(recordOf(s, id)?.in_flight ?? [], [], `${script}: a call is still in flight`);
    outcomes.push(id);
  }
  await sleep(2000);
  await stop();
  for (const id of outcomes) {
    const states = shown(s, id).map((a) => a.state);
    assert.ok(!states.includes('quiet'), `${id} showed quiet: ${JSON.stringify(states)}`);
  }
  return 'idle after each interrupt, never quiet';
}

async function twoSessions(context) {
  const s = new Session(context, 'oc_two_sessions');
  s.config();
  await s.serve();
  context.stub.scripts.oc_two_a = [slow(60, 100)];
  context.stub.scripts.oc_two_b = [slow(60, 100)];
  context.stub.scripts.oc_two_c = [slow(120, 100)];
  const [a, b] = [await s.create(s.wt('wt1')), await s.create(s.wt('wt2'))];
  const from = s.mark();
  await Promise.all([s.prompt(a, '[[scenario:oc_two_a]] Count slowly in wt1.'), s.prompt(b, '[[scenario:oc_two_b]] Count slowly in wt2.')]);
  await Promise.all([s.turnEnd(a, from), s.turnEnd(b, from)]);
  await sleep(1500);
  const c = await s.create(s.wt('wt1'));
  const fromC = s.mark();
  await s.prompt(c, '[[scenario:oc_two_c]] Count slowly again in wt1.');
  await s.waitEvent((e) => e.type === 'session.text.delta' && e.data?.sessionID === c, 60_000, fromC);
  await sleep(2000);
  const whileC = s.actors();
  await s.turnEnd(c, fromC);
  await sleep(1500);
  const setups = s.recorded().filter((l) => l.kind === 'setup');
  const pids = new Set(setups.map((l) => l.pid));
  expectOpenCode(pids.size === 1, `the instances ran in ${pids.size} processes`);
  const folders = setups.map((l) => l.payload.location?.directory);
  expectOpenCode(folders.includes(s.wt('wt1')) && folders.includes(s.wt('wt2')), `the instances' folders: ${JSON.stringify(folders)}`);
  expectOpenCode(new Set(setups.map((l) => l.instance)).size === setups.length, 'two folders shared an instance');
  const records = [a, b, c].map((id) => recordOf(s, id));
  assert.ok(records.every(Boolean), `records: ${s.records().map((r) => r.file).join(', ')}`);
  assert.equal(new Set(records.map((r) => r.process?.pid)).size, 1, 'the three records name different processes');
  assert.ok(records.every((r) => r.process?.shared === true), 'a record is not shared');
  assert.ok(records.every((r) => r.state !== 'ended'), `a session ended another: ${JSON.stringify(records.map((r) => r.state))}`);
  const inWt1 = whileC.filter((x) => x.worktree === 'wt1' && x.harness === 'opencode').map((x) => x.key);
  assert.deepEqual(inWt1, [stemOf(c)], 'while the newer session works, wt1 shows');
  assert.ok(whileC.some((x) => x.key === stemOf(b) && x.worktree === 'wt2'), 'the wt2 session is hidden');
  const calls = s.recordCalls();
  for (const id of [a, b, c]) {
    for (const name of ['session.execution.started', 'session.execution.succeeded']) {
      const count = calls.filter((x) => x.payload?.name === name && x.payload?.data?.sessionID === id).length;
      assert.equal(count, 1, `${name} of ${id} spawned ${count} times`);
    }
  }
  return `one process ${[...pids][0]}, instances for ${folders.length} folders`;
}

async function sessionDelete(context) {
  const s = new Session(context, 'oc_session_delete');
  s.config();
  await s.serve();
  context.stub.scripts.oc_session_delete = [say('A short exchange before the delete.')];
  const id = await s.create(s.pool.fg);
  const from = s.mark();
  await s.prompt(id, '[[scenario:oc_session_delete]] Say something short.');
  await s.turnEnd(id, from);
  await sleep(1000);
  const removed = await s.api('DELETE', `/api/session/${id}`);
  expectOpenCode(removed.status < 300, `the delete got ${removed.status}`);
  const deleted = await s.waitEvent((e) => e.type === 'session.deleted' && e.data?.sessionID === id, 15_000, from);
  expectOpenCode(deleted, 'no session.deleted');
  await sleep(1500);
  assert.equal(recordOf(s, id)?.state, 'ended', 'the record did not end');
  return 'ended';
}

async function title(context) {
  const s = new Session(context, 'oc_title');
  s.config();
  await s.serve();
  context.stub.scripts.oc_title = [say('A short exchange.')];
  // A folder's instance starts at the first prompt there, after that session was created.
  const first = await s.create(s.pool.fg);
  const fromFirst = s.mark();
  await s.prompt(first, '[[scenario:oc_title]] Say something short first.');
  await s.turnEnd(first, fromFirst);
  const id = await s.create(s.pool.fg, 'Given title via API');
  const from = s.mark();
  await s.prompt(id, '[[scenario:oc_title]] Say something short.');
  await s.turnEnd(id, from);
  await sleep(1000);
  const given = (await until(s, id, (a) => a.label === 'Given title via API'))?.label;
  const renamed = await s.api('PATCH', `/api/session/${id}`, { title: 'Renamed via API' });
  expectOpenCode(renamed.status < 300, `the rename got ${renamed.status}`);
  const event = await s.waitEvent((e) => e.type === 'session.renamed' && e.data?.sessionID === id && e.data?.title === 'Renamed via API', 15_000, from);
  expectOpenCode(event, 'no session.renamed');
  const after = (await until(s, id, (a) => a.label === 'Renamed via API'))?.label;
  assert.equal(given, 'Given title via API', 'the label is not the given title');
  assert.equal(after, 'Renamed via API', 'the label did not follow the rename');
  return 'the label followed the given title and the rename';
}

/** Every file below `dir` that holds `text`. */
function filesHolding(dir, text) {
  const found = [];
  const stack = [dir];
  while (stack.length) {
    const current = stack.pop();
    let names = [];
    try {
      names = readdirSync(current, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of names) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (entry.isFile()) {
        try {
          if (readFileSync(full, 'utf8').includes(text)) found.push(full);
        } catch {
          // Unreadable.
        }
      }
    }
  }
  return found;
}

async function shellEnv(context) {
  const s = new Session(context, 'oc_shell_env');
  s.config();
  await s.serve();
  context.stub.scripts.oc_shell_env = [call('shell', { command: 'printenv WTPOOL_SESSION' }), say('Printed.')];
  const id = await s.create(s.pool.fg);
  const from = s.mark();
  await s.prompt(id, '[[scenario:oc_shell_env]] Print the variable.');
  await s.turnEnd(id, from);
  await sleep(1000);
  const after = s.recorded().find((l) => l.kind === 'hook' && l.name === 'tool.execute.after' && l.payload.tool === 'shell');
  expectOpenCode(after, 'no tool.execute.after for the shell call');
  const output = String(after.payload.result?.output?.output ?? after.payload.result?.metadata?.output ?? JSON.stringify(after.payload.result?.content ?? '')).trim();
  assert.equal(output, stemOf(id), "the shell's WTPOOL_SESSION");
  const holding = [
    ...filesHolding(s.calls, s.password),
    ...filesHolding(path.join(s.pool.common, 'wtpool-activity'), s.password),
    ...filesHolding(path.join(context.dirs.state, 'wtpool'), s.password),
  ];
  assert.deepEqual(holding, [], 'files that hold the server password');
  return `WTPOOL_SESSION=${output}`;
}

export const SERVE_SESSIONS = [
  { name: 'oc_permission', run: permission },
  { name: 'oc_permission_reject', run: permissionReject },
  { name: 'oc_question', run: question },
  { name: 'oc_interrupt', run: interrupt },
  { name: 'oc_two_sessions', run: twoSessions },
  { name: 'oc_session_delete', run: sessionDelete },
  { name: 'oc_title', run: title },
  { name: 'oc_shell_env', run: shellEnv },
];
