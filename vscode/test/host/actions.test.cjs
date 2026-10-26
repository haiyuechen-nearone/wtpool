// The webview's messages come in through the verify API's fromWebview, and the dialogs through
// wrapped `vscode.window.show*Message` calls, so no dialog reaches the screen.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vscode = require('vscode');

const {
  control,
  openStatus,
  resetFake,
  scenarioDocuments,
  sleep,
  starts,
  verifyApi,
  views,
  waitFor,
  wrap,
} = require('./support.cjs');

const MESSAGE_KINDS = ['showInformationMessage', 'showWarningMessage', 'showErrorMessage'];
const WT2 = '/Users/verify/pool/wt2';
const BRANCH = 'feature-a';
/** The time a check waits for a `wtpool` the extension must not start. */
const QUIET_MS = 1500;

let generation = 0;

/** idle_pool's last document, wt2 (a slot on feature-a) holding `actors`, with a new time. */
function poolWith(actors, activity) {
  const doc = structuredClone(scenarioDocuments('idle_pool').at(-1));
  generation += 1;
  doc.generated = new Date(Date.parse(doc.generated) + generation * 1000).toISOString().replace('.000Z', 'Z');
  const wt2 = doc.pools[0].worktrees.find((w) => w.path === WT2);
  assert.equal(wt2.branch, BRANCH, 'setup: idle_pool\'s wt2 holds feature-a');
  wt2.actors = actors;
  if (activity) wt2.activity = activity;
  if (actors.length) wt2.last_event = actors.map((a) => a.updated).sort().at(-1);
  return doc;
}

function actor(key, state, extra = {}) {
  return {
    key,
    kind: key.includes('.') ? 'subagent' : 'session',
    harness: 'claude-code',
    label: key.includes('.') ? 'Explore' : `session ${key}`,
    state,
    reason: state === 'waiting' ? 'permission' : null,
    action: { tool: 'Bash', target: 'cargo test' },
    placed_by: 'path',
    error: null,
    since: '2026-10-20T11:59:00Z',
    updated: '2026-10-20T11:59:30Z',
    ...extra,
  };
}

/** Opens the tab on idle_pool, then shows `doc` and waits until the webview got it. */
async function showing(doc, config = {}) {
  await resetFake({ scenario: 'idle_pool', aliveMs: 0, ...config });
  // Earlier checks posted idle_pool too, so only views of this tab count.
  const from = (await views()).length;
  await openStatus();
  const last = scenarioDocuments('idle_pool').at(-1);
  const first = await waitFor(async () => (await views()).slice(from).some((v) => JSON.stringify(v.status) === JSON.stringify(last)), 10_000);
  assert.ok(first, 'setup: idle_pool never reached the webview');
  // The fake follows its control file once it printed the scenario.
  await sleep(300);
  await show(doc);
}

async function show(doc) {
  const from = (await views()).length;
  control(`emit ${JSON.stringify(doc)}`);
  const shown = await waitFor(async () => (await views()).slice(from).some((v) => JSON.stringify(v.status) === JSON.stringify(doc)), 10_000);
  assert.ok(shown, 'setup: the document never reached the webview');
}

/** The items a dialog call offers, after its message and options. */
function itemsOf(args) {
  return args.slice(1).filter((a) => typeof a === 'string' || (a && typeof a === 'object' && 'title' in a));
}

function optionsOf(args) {
  return args.slice(1).find((a) => a && typeof a === 'object' && !('title' in a)) ?? {};
}

const titleOf = (item) => (typeof item === 'string' ? item : item.title);

/** The item that goes ahead: the first that is not a close affordance or "Cancel". */
function confirmItem(args) {
  return itemsOf(args).find((item) => !(typeof item === 'object' && item.isCloseAffordance) && !/^cancel$/i.test(titleOf(item)));
}

/** A dialog's whole text: message, detail and items. */
function textOf(args) {
  return [String(args[0] ?? ''), String(optionsOf(args).detail ?? ''), ...itemsOf(args).map(titleOf)].join('\n');
}

/**
 * Wraps every show*Message call until `restore`. `answer(call)` gives each call's result: a
 * value or a promise. `calls` holds {kind, args, modal}.
 */
function dialogs(answer) {
  const calls = [];
  const wrappers = MESSAGE_KINDS.map((kind) =>
    wrap(
      vscode.window,
      kind,
      () => false,
      (...args) => {
        const call = { kind, args, modal: optionsOf(args).modal === true };
        calls.push(call);
        return Promise.resolve(answer(call));
      },
    ),
  );
  return {
    calls,
    modals: () => calls.filter((c) => c.modal),
    restore: () => wrappers.forEach((w) => w.restore()),
  };
}

const actionStarts = (verb) => starts().filter((s) => s.argv?.[0] === verb);

/** Sends `{type, id}` from the webview and gives the extension time to ask and run. */
async function send(type, id = WT2) {
  const verify = await verifyApi();
  await verify.fromWebview({ type, id });
}

describe('actions', () => {
  for (const [verb, argv] of [
    ['free', ['free', 'wt2', '--force']],
    ['fg', ['fg', BRANCH, '--force']],
  ]) {
    it(`actions__should_confirm_naming_the_worktree ${verb}_confirmed`, async () => {
      // Given
      await showing(poolWith([]));
      const shown = dialogs((call) => (call.modal ? confirmItem(call.args) : undefined));

      // When
      try {
        await send(verb);
        await waitFor(() => actionStarts(verb).length > 0, 5_000);
        await sleep(500);
      } finally {
        shown.restore();
      }

      // Then
      const modals = shown.modals();
      assert.equal(modals.length, 1, `${verb} on wt2 showed ${modals.length} modal dialogs: ${JSON.stringify(shown.calls.map((c) => c.args))}`);
      const text = textOf(modals[0].args);
      assert.match(text, /\bwt2\b/, `the ${verb} dialog does not name the worktree: ${text}`);
      assert.ok(text.includes(BRANCH), `the ${verb} dialog does not name the branch: ${text}`);
      assert.ok(confirmItem(modals[0].args), `the ${verb} dialog offers nothing to confirm: ${text}`);
      assert.deepEqual(actionStarts(verb).map((s) => s.argv), [argv], `confirming ${verb} did not run wtpool ${argv.join(' ')} once`);
    });

    it(`actions__should_confirm_naming_the_worktree ${verb}_cancelled`, async () => {
      // Given
      await showing(poolWith([]));
      const shown = dialogs(() => undefined);

      // When
      try {
        await send(verb);
        await waitFor(() => shown.modals().length > 0, 5_000);
        await sleep(QUIET_MS);
      } finally {
        shown.restore();
      }

      // Then
      assert.equal(shown.modals().length, 1, `${verb} on wt2 showed no modal dialog to cancel`);
      assert.deepEqual(actionStarts(verb), [], `a cancelled ${verb} ran wtpool`);
    });
  }

  const STDERR = 'wtpool: wt2 has 2 uncommitted file(s) (wtpool-verify fake)';
  for (const [verb, argv] of [
    ['free', ['free', 'wt2', '--force']],
    ['fg', ['fg', BRANCH, '--force']],
  ]) {
    it(`actions__should_confirm_naming_the_worktree ${verb}_fails`, async () => {
      // Given
      await showing(poolWith([]), { actionExit: 1, actionStderr: STDERR });
      const shown = dialogs((call) => (call.modal ? confirmItem(call.args) : undefined));

      // When
      try {
        await send(verb);
        await waitFor(() => actionStarts(verb).length > 0, 5_000);
        await waitFor(() => shown.calls.some((c) => c.kind === 'showErrorMessage'), 5_000);
        await sleep(QUIET_MS);
      } finally {
        shown.restore();
      }

      // Then
      assert.deepEqual(actionStarts(verb).map((s) => s.argv), [argv], `a failing wtpool ${verb} did not run once, or ran again`);
      const errors = shown.calls.filter((c) => c.kind === 'showErrorMessage').map((c) => textOf(c.args));
      assert.equal(errors.length, 1, `a failing wtpool ${argv.join(' ')} showed ${errors.length} error messages: ${JSON.stringify(errors)}`);
      assert.ok(errors[0].includes(`wtpool ${argv.join(' ')}`), `the error does not name the command wtpool ${argv.join(' ')}: ${errors[0]}`);
      assert.ok(errors[0].includes(STDERR), `the error does not say what wtpool printed: ${errors[0]}`);
      assert.equal(shown.modals().length, 1, 'a failing wtpool asked again');
    });
  }

  // A NUL in the binary path makes Node's spawn throw at once (ERR_INVALID_ARG_VALUE), so `wtpool`
  // cannot even start.
  for (const [verb, argv] of [
    ['free', ['free', 'wt2', '--force']],
    ['fg', ['fg', BRANCH, '--force']],
  ]) {
    it(`actions__should_confirm_naming_the_worktree ${verb}_spawn_throws`, async () => {
      // Given
      await showing(poolWith([]));
      const configuration = vscode.workspace.getConfiguration('wtpool');
      const previous = configuration.inspect('path').globalValue;
      const broken = `${process.env.WTPOOL_VERIFY_HOST_FAKES ?? '/nonexistent'}/wtpool\u0000nul`;
      const shown = dialogs((call) => (call.modal ? confirmItem(call.args) : undefined));
      const from = (await views()).length;

      // When
      try {
        // The sandbox's user settings only: the host runs with its own --user-data-dir.
        await configuration.update('path', broken, vscode.ConfigurationTarget.Global);
        await send(verb);
        await waitFor(() => shown.calls.some((c) => c.kind === 'showErrorMessage'), 5_000);
        await sleep(QUIET_MS);
      } finally {
        shown.restore();
        await vscode.workspace.getConfiguration('wtpool').update('path', previous, vscode.ConfigurationTarget.Global);
      }

      // Then
      assert.deepEqual([...actionStarts('free'), ...actionStarts('fg')], [], `a ${verb} whose wtpool cannot start ran wtpool`);
      assert.equal(shown.modals().length, 1, `a ${verb} whose wtpool cannot start asked ${shown.modals().length} times`);
      const errors = shown.calls.filter((c) => c.kind === 'showErrorMessage').map((c) => textOf(c.args));
      assert.equal(errors.length, 1, `a wtpool ${argv.join(' ')} that cannot start showed ${errors.length} error messages: ${JSON.stringify(errors)}`);
      assert.ok(errors[0].includes(`wtpool ${argv.join(' ')}`), `the error does not name the command wtpool ${argv.join(' ')}: ${errors[0]}`);
      const later = (await views()).slice(from);
      const cleared = later.filter((v) => !v.status?.pools?.[0]?.worktrees?.some((w) => w.path === WT2));
      assert.deepEqual(cleared.map((v) => v.connection), [], 'the tab stopped showing wt2 after the failed start');
    });
  }

  const changes = {
    branch_changed: (doc) => {
      doc.pools[0].worktrees.find((w) => w.path === WT2).branch = 'feature-b';
    },
    row_vanished: (doc) => {
      doc.pools[0].worktrees = doc.pools[0].worktrees.filter((w) => w.path !== WT2);
    },
  };
  for (const verb of ['free', 'fg']) {
    for (const [change, apply] of Object.entries(changes)) {
      it(`actions__should_confirm_naming_the_worktree ${verb}_${change}_while_asked`, async () => {
        // Given
        await showing(poolWith([]));
        let confirm;
        const held = new Promise((resolve) => {
          confirm = resolve;
        });
        // The first modal stays open until the row changed. Any later dialog goes ahead at once.
        let modals = 0;
        let changedAt;
        const shown = dialogs((call) => {
          if (call.modal) modals += 1;
          return call.modal && modals === 1 ? held.then(() => confirmItem(call.args)) : confirmItem(call.args);
        });

        // When
        try {
          await send(verb);
          const asked = await waitFor(() => shown.modals().length > 0, 5_000);
          assert.ok(asked, `${verb} on wt2 showed no modal dialog`);
          const later = poolWith([]);
          apply(later);
          await show(later);
          changedAt = shown.calls.length;
          confirm();
          await waitFor(() => shown.calls.length > changedAt, 5_000);
          await sleep(QUIET_MS);
        } finally {
          confirm();
          shown.restore();
        }

        // Then
        assert.deepEqual(actionStarts(verb), [], `wtpool ${verb} ran although wt2 changed (${change}) while the dialog was open`);
        const told = shown.calls.slice(changedAt).filter((c) => /\bwt2\b/.test(textOf(c.args)));
        assert.ok(told.length > 0, `confirming after wt2 changed (${change}) shows no message naming wt2`);
      });
    }
  }

  // `fg` of wt2's branch guards wt2, the slot that holds it, as `free` of wt2 does. Each case's
  // title, by verb.
  const guarded = {
    free: {
      refuse: (name) => `actions__should_refuse_free_with_a_working_actor ${name}`,
      warn: (name) => `actions__should_warn_free_with_an_idle_actor ${name}`,
      argv: ['free', 'wt2', '--force'],
    },
    fg: {
      refuse: (name) => `actions__should_guard_fg_by_the_holding_slot refuses_${name}`,
      warn: (name) => `actions__should_guard_fg_by_the_holding_slot warns_${name}`,
      argv: ['fg', BRANCH, '--force'],
    },
  };
  /** Every free and fg the extension started, not only the one the user asked for. */
  const anyAction = () => [...actionStarts('free'), ...actionStarts('fg')];

  const refusals = {
    working: () => [actor('s1', 'working')],
    waiting: () => [actor('s1', 'waiting')],
    idle_session_with_a_working_subagent: () => [actor('s1', 'idle'), actor('s1.a1', 'working')],
  };
  for (const [verb, titles] of Object.entries(guarded)) {
    for (const [name, actors] of Object.entries(refusals)) {
      it(titles.refuse(name), async () => {
        // Given
        const doc = poolWith(actors(), actors().some((a) => a.state === 'waiting') ? 'waiting' : 'working');
        await showing(doc);
        // The user takes the first way ahead in every dialog.
        const shown = dialogs((call) => confirmItem(call.args));

        // When
        try {
          await send(verb);
          await waitFor(() => shown.calls.length > 0, 5_000);
          await sleep(QUIET_MS);
        } finally {
          shown.restore();
        }

        // Then
        assert.deepEqual(anyAction(), [], `${verb} ran wtpool while an actor in wt2 is ${name}`);
        const named = shown.calls.filter((c) => /\bwt2\b/.test(textOf(c.args)));
        assert.ok(named.length > 0, `the refusal shows the user no message naming wt2: ${JSON.stringify(shown.calls.map((c) => c.args))}`);
      });
    }

    it(titles.refuse('working_by_the_confirm'), async () => {
      // Given
      await showing(poolWith([actor('s1', 'idle')], 'idle'));
      let confirm;
      const held = new Promise((resolve) => {
        confirm = resolve;
      });
      // The first modal stays open until the actor works. Any later dialog is confirmed at once.
      let modals = 0;
      const shown = dialogs((call) => {
        if (call.modal) modals += 1;
        return call.modal && modals === 1 ? held.then(() => confirmItem(call.args)) : confirmItem(call.args);
      });

      // When
      try {
        await send(verb);
        const asked = await waitFor(() => shown.modals().length > 0, 5_000);
        assert.ok(asked, `${verb} on a wt2 with an idle actor showed no modal dialog`);
        await show(poolWith([actor('s1', 'working', { updated: '2026-10-20T12:00:30Z' })], 'working'));
        confirm();
        await sleep(QUIET_MS);
      } finally {
        confirm();
        shown.restore();
      }

      // Then
      assert.deepEqual(anyAction(), [], `${verb} ran wtpool although the actor in wt2 worked by the time of the confirm`);
    });
  }

  const warnings = {
    idle: { actors: () => [actor('s1', 'idle', { action: null })], activity: 'idle', word: /\bidle\b/i },
    quiet: { actors: () => [actor('s1', 'quiet')], activity: 'idle', word: /\b(idle|quiet)\b/i },
  };
  for (const [verb, titles] of Object.entries(guarded)) {
    for (const [name, { actors, activity, word }] of Object.entries(warnings)) {
      it(titles.warn(name), async () => {
        // Given
        await showing(poolWith(actors(), activity));
        const shown = dialogs((call) => (call.modal ? confirmItem(call.args) : undefined));

        // When
        try {
          await send(verb);
          await waitFor(() => actionStarts(verb).length > 0, 5_000);
          await sleep(500);
        } finally {
          shown.restore();
        }

        // Then
        const modals = shown.modals();
        assert.equal(modals.length, 1, `${verb} with a ${name} actor in wt2 showed ${modals.length} modal dialogs`);
        const text = textOf(modals[0].args);
        assert.equal(modals[0].kind, 'showWarningMessage', `${verb} with a ${name} actor in wt2 does not warn: ${modals[0].kind}`);
        assert.match(text, word, `the ${verb} dialog does not warn that an actor in wt2 is ${name}: ${text}`);
        assert.match(text, /\bwt2\b/, `the ${verb} dialog does not name the worktree: ${text}`);
        assert.ok(text.includes(BRANCH), `the ${verb} dialog does not name the branch: ${text}`);
        assert.deepEqual(actionStarts(verb).map((s) => s.argv), [titles.argv], `confirming the warning did not run wtpool ${titles.argv.join(' ')}`);
      });
    }

    it(titles.warn('no_actor_no_idle_warning'), async () => {
      // Given
      await showing(poolWith([]));
      const shown = dialogs(() => undefined);

      // When
      try {
        await send(verb);
        await waitFor(() => shown.modals().length > 0, 5_000);
      } finally {
        shown.restore();
      }

      // Then
      const modals = shown.modals();
      assert.equal(modals.length, 1, `${verb} on wt2 without actors showed no modal dialog`);
      assert.doesNotMatch(textOf(modals[0].args), /\bidle\b/i, `the ${verb} dialog warns for idle where no actor is idle`);
    });
  }
});

// The extension sets no WTPOOL_FOREGROUND or WTPOOL_SLOT_DIR of its own, so each wtpool it runs
// gets the ones the extension host inherited.
describe('workspace folder', () => {
  it('spawn__should_run_in_the_workspace_folder', async () => {
    // Given
    const folder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    assert.ok(folder, 'setup: the host window has no workspace folder');
    const real = fs.realpathSync(folder);
    const inherited = { foreground: process.env.WTPOOL_FOREGROUND ?? null, slotDir: process.env.WTPOOL_SLOT_DIR ?? null };
    const wrong = (start) => {
      const found = [];
      if (start.cwd !== real) found.push(`wtpool ${start.argv.join(' ')} ran in ${start.cwd}, not the workspace folder ${real}`);
      if (start.foreground !== inherited.foreground || start.slotDir !== inherited.slotDir) {
        found.push(`wtpool ${start.argv.join(' ')} got WTPOOL_FOREGROUND ${start.foreground} and WTPOOL_SLOT_DIR ${start.slotDir}, not the inherited ${JSON.stringify(inherited)}`);
      }
      return found;
    };

    // When: the watch, then free and fg confirmed.
    await resetFake({ scenario: 'idle_pool', aliveMs: 0 });
    await openStatus();
    const watch = await waitFor(() => starts()[0], 10_000);
    assert.ok(watch, 'wtpool status --watch did not start');
    assert.deepEqual(wrong(watch), [], 'the watch');
    await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    await showing(poolWith([]));
    const shown = dialogs((call) => (call.modal ? confirmItem(call.args) : undefined));
    try {
      for (const verb of ['free', 'fg']) {
        await send(verb);
        await waitFor(() => actionStarts(verb).length > 0, 5_000);
      }
      await sleep(500);
    } finally {
      shown.restore();
    }

    // Then
    const actions = starts().filter((s) => s.argv[0] !== 'status');
    assert.deepEqual(actions.map((s) => s.argv.join(' ')), ['free wt2 --force', `fg ${BRANCH} --force`], 'setup: free and fg did not each run once');
    assert.deepEqual(actions.flatMap(wrong), [], 'the actions');
  });
});
