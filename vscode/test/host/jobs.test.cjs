const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vscode = require('vscode');

const {
  CONFIG,
  ended,
  expectedDocuments,
  fakeLog,
  posted,
  show,
  showingAlone,
  sleep,
  starts,
  verifyApi,
  waitFor,
} = require('./support.cjs');

const FAKE = path.join(__dirname, '..', 'fake-wtpool.mjs');
const SHELLS = ['sh', 'bash', 'zsh', 'dash', 'fish', 'ksh', 'tcsh', 'csh'];
/** The time a check waits for a `wtpool` the extension must not start, or start again. */
const QUIET_MS = 1500;
/** The extension kills a `wtpool job` call after this long. */
const KILL_MS = 10_000;

let generation = 0;

/** idle_pool's last document with a new time and, unless `jobs` is null, a `machine.jobs`. */
function documentWith(jobs) {
  const doc = structuredClone(expectedDocuments('idle_pool').at(-1));
  generation += 1;
  doc.generated = new Date(Date.parse(doc.generated) + generation * 1000).toISOString().replace('.000Z', 'Z');
  if (jobs !== null) {
    doc.machine.jobs = {
      limit: 4,
      default: 4,
      min: 1,
      max: 64,
      enforce: 'warn',
      source: { limit: 'machine', enforce: 'machine' },
      config: CONFIG,
      error: null,
      heavy: [
        { pattern: 'just', source: 'machine' },
        { pattern: 'make', source: 'repository' },
        { pattern: 'protoc', source: 'build_processes' },
      ],
      suggested: [
        { pattern: 'cargo nextest', from: 'Cargo.toml' },
        { pattern: 'cargo clippy', from: 'Cargo.toml' },
      ],
      running: [],
      waiting: [],
      outside: [],
      ...jobs,
    };
  }
  return doc;
}

const jobStarts = () => starts().filter((s) => s.argv?.[0] === 'job');

/** The jobSettings answers the extension posted after message number `from`. */
async function answersSince(from) {
  return (await posted()).slice(from).filter((m) => m && m.type === 'jobSettings');
}

/** When the fake of `pid` ended, by its log, or Infinity while it runs. */
function endTime(pid) {
  const end = fakeLog().find((e) => e.pid === pid && (e.event === 'exit' || e.event === 'signal'));
  return end ? end.time : Infinity;
}

/**
 * Lists the problems of a `job` start: a folder other than the first workspace folder, pool
 * variables other than the inherited ones, a binary other than wtpool.path, or a shell.
 */
function placeProblems(start) {
  const folder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const real = fs.realpathSync(folder);
  const inherited = { foreground: process.env.WTPOOL_FOREGROUND ?? null, slotDir: process.env.WTPOOL_SLOT_DIR ?? null };
  const found = [];
  const what = `wtpool ${start.argv.join(' ')}`;
  if (start.cwd !== real) found.push(`${what} ran in ${start.cwd}, not the first workspace folder ${real}`);
  if (start.foreground !== inherited.foreground || start.slotDir !== inherited.slotDir) {
    found.push(`${what} got WTPOOL_FOREGROUND ${start.foreground} and WTPOOL_SLOT_DIR ${start.slotDir}, not the inherited ${JSON.stringify(inherited)}`);
  }
  if (fs.realpathSync(start.script) !== fs.realpathSync(FAKE)) found.push(`${what} ran ${start.script}, not the user's wtpool.path`);
  if (SHELLS.includes(path.basename(start.parent ?? '').replace(/^-/, ''))) found.push(`${what} ran under the shell ${start.parent}`);
  return found;
}

/** The starts that began before the one ahead of them ended. */
function overlaps(list) {
  return list.slice(1).filter((start, index) => start.time < endTime(list[index].pid)).map((s) => s.argv.join(' '));
}

/** Files named machine.toml or wtpool.toml below `dir`. */
function settingsFiles(dir) {
  if (!dir || !fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir, { recursive: true })
    .map(String)
    .filter((name) => ['machine.toml', 'wtpool.toml'].includes(path.basename(name)))
    .map((name) => path.join(dir, name));
}

/** The text of each settings file of the sandbox home and the workspace folder, by path. */
function settingsTexts() {
  const files = [...settingsFiles(process.env.HOME), ...settingsFiles(vscode.workspace.workspaceFolders?.[0]?.uri.fsPath)];
  return new Map(files.map((file) => [file, fs.readFileSync(file, 'utf8')]));
}

const settings = (fields) => ({ type: 'jobSettings', ...fields });
const heavy = (op, scope, pattern) => settings({ heavy: { op, scope, pattern } });

/**
 * Sends `messages` once the tab shows `doc`, then waits until `count` job calls started and
 * ended, and `settleMs` more for any call that must not come.
 */
async function run(doc, messages, { count, config = {}, settleMs = QUIET_MS } = {}) {
  await showingAlone(doc, config);
  const verify = await verifyApi();
  const from = (await posted()).length;
  const textsBefore = settingsTexts();
  for (const message of messages) await verify.fromWebview(message);
  await waitFor(() => jobStarts().length >= count && jobStarts().every((s) => endTime(s.pid) !== Infinity), 15_000);
  await waitFor(async () => (await answersSince(from)).length >= messages.length, 5_000);
  await sleep(settleMs);
  const textsAfter = settingsTexts();
  return {
    calls: jobStarts(),
    answers: await answersSince(from),
    written: [...textsAfter].filter(([file, text]) => textsBefore.get(file) !== text).map(([file]) => file),
  };
}

const OK = { type: 'jobSettings', version: 1, ok: true };

describe('job settings', () => {
  const ran = [
    {
      name: 'limit_then_enforce',
      messages: [settings({ limit: 6, enforce: 'require' })],
      argv: [['job', 'limit', '6'], ['job', 'enforce', 'require']],
      answers: [OK],
    },
    {
      name: 'both_defaults',
      messages: [settings({ limit: null, enforce: null })],
      argv: [['job', 'limit', 'default'], ['job', 'enforce', 'default']],
      answers: [OK],
    },
    {
      name: 'limit_alone_at_the_bounds',
      messages: [settings({ limit: 1 }), settings({ limit: 64 })],
      argv: [['job', 'limit', '1'], ['job', 'limit', '64']],
      answers: [OK, OK],
    },
    {
      name: 'enforce_alone',
      messages: [settings({ enforce: 'off' }), settings({ enforce: 'warn' })],
      argv: [['job', 'enforce', 'off'], ['job', 'enforce', 'warn']],
      answers: [OK, OK],
    },
    {
      name: 'heavy_add_to_the_repository',
      messages: [heavy('add', 'repository', 'cargo xtask verify')],
      argv: [['job', 'heavy', 'add', '--repo', '--', 'cargo xtask verify']],
      answers: [{ ...OK, scope: 'repository' }],
    },
    {
      name: 'heavy_add_to_the_machine',
      messages: [heavy('add', 'machine', 'cargo-*')],
      argv: [['job', 'heavy', 'add', '--', 'cargo-*']],
      answers: [{ ...OK, scope: 'machine' }],
    },
    {
      name: 'heavy_rm_from_each_list',
      messages: [heavy('rm', 'machine', 'just'), heavy('rm', 'repository', 'protoc')],
      argv: [['job', 'heavy', 'rm', '--', 'just'], ['job', 'heavy', 'rm', '--repo', '--', 'protoc']],
      answers: [{ ...OK, scope: 'machine' }, { ...OK, scope: 'repository' }],
    },
    {
      // The suggestions as the last document holds them, in its order, in one call. A pattern the
      // CLI refuses is left out, as it would refuse the whole call for it.
      name: 'add_suggested',
      jobs: {
        suggested: [
          { pattern: 'make', from: 'Makefile' },
          { pattern: 'pnpm run test:e2e', from: 'package.json' },
          { pattern: '-rf', from: 'package.json' },
          { pattern: 'pnpm test:e2e', from: 'package.json' },
        ],
      },
      messages: [heavy('add-suggested', 'repository', null)],
      argv: [['job', 'heavy', 'add', '--repo', '--', 'make', 'pnpm run test:e2e', 'pnpm test:e2e']],
      answers: [{ ...OK, scope: 'repository' }],
    },
  ];
  for (const { name, jobs = {}, messages, argv, answers } of ran) {
    it(`job_settings__should_run_wtpool_once_each_and_report ${name}`, async () => {
      // Given
      const doc = documentWith(jobs);

      // When
      const seen = await run(doc, messages, { count: argv.length });

      // Then
      assert.deepEqual(seen.calls.map((s) => s.argv), argv, 'the wtpool job calls, in order, each once');
      assert.deepEqual(seen.calls.flatMap(placeProblems), [], 'where and how the calls ran');
      assert.deepEqual(overlaps(seen.calls), [], 'calls that started before the one ahead of them ended');
      assert.deepEqual(seen.answers, answers, 'the answers posted to the webview');
      assert.deepEqual(seen.written, [], 'the extension wrote a settings file itself');
    });
  }

  it('job_settings__should_run_wtpool_once_each_and_report one_message_at_a_time', async () => {
    // Given
    // Each call takes 1.5 s, and two messages come at once.
    const doc = documentWith({});

    // When
    const seen = await run(doc, [settings({ limit: 6, enforce: 'off' }), heavy('add', 'machine', 'go test')], {
      count: 3,
      config: { jobHangMs: 1500 },
    });

    // Then
    assert.deepEqual(
      seen.calls.map((s) => s.argv),
      [['job', 'limit', '6'], ['job', 'enforce', 'off'], ['job', 'heavy', 'add', '--', 'go test']],
      'the wtpool job calls of two messages, in order',
    );
    assert.deepEqual(overlaps(seen.calls), [], 'calls that started before the one ahead of them ended');
    assert.deepEqual(seen.answers, [OK, { ...OK, scope: 'machine' }], 'one answer per message, in order');
  });

  const STDERR = `wtpool: ${CONFIG} is a symlink to /Users/verify/dotfiles/machine.toml: edit it there`;
  const failed = [
    {
      name: 'refused_limit_stops_enforce',
      messages: [settings({ limit: 6, enforce: 'require' })],
      argv: [['job', 'limit', '6']],
      config: { jobExit: 125, jobStderr: `wtpool: reading the config\n${STDERR}\n\n` },
      answers: [{ type: 'jobSettings', version: 1, ok: false, error: STDERR }],
    },
    {
      name: 'refused_enforce_after_a_limit',
      messages: [settings({ limit: 6, enforce: 'require' })],
      argv: [['job', 'limit', '6'], ['job', 'enforce', 'require']],
      config: { jobExit: [0, 125], jobStderr: `${STDERR}\n` },
      answers: [{ type: 'jobSettings', version: 1, ok: false, error: STDERR }],
    },
    {
      name: 'refused_pattern_edit',
      messages: [heavy('add', 'repository', 'cargo xtask verify')],
      argv: [['job', 'heavy', 'add', '--repo', '--', 'cargo xtask verify']],
      config: { jobExit: 125, jobStderr: 'wtpool: /Users/verify/pool/fg/.git/wtpool.toml: a comment inside heavy at line 3; edit it by hand\n' },
      answers: [
        {
          type: 'jobSettings',
          version: 1,
          ok: false,
          error: 'wtpool: /Users/verify/pool/fg/.git/wtpool.toml: a comment inside heavy at line 3; edit it by hand',
          scope: 'repository',
        },
      ],
    },
  ];
  for (const { name, messages, argv, config, answers } of failed) {
    it(`job_settings__should_run_wtpool_once_each_and_report ${name}`, async () => {
      // Given
      const doc = documentWith({});

      // When
      const seen = await run(doc, messages, { count: argv.length, config });

      // Then
      assert.deepEqual(seen.calls.map((s) => s.argv), argv, 'the wtpool job calls up to the one that failed, and none after');
      assert.deepEqual(seen.answers, answers, 'the answer names wtpool\'s last non-empty stderr line (R10)');
    });
  }

  it('job_settings__should_run_wtpool_once_each_and_report killed_after_10_s', async () => {
    // Given
    // The first call never answers in time.
    const doc = documentWith({});
    await showingAlone(doc, { jobHangMs: 60_000 });
    const verify = await verifyApi();
    const from = (await posted()).length;

    // When
    await verify.fromWebview(settings({ limit: 6, enforce: 'require' }));
    const first = await waitFor(() => jobStarts()[0], 5_000);
    assert.ok(first, 'the message ran no wtpool job limit');
    // The kill is due 10 s after the start, on the extension's clock. Real time and the verify
    // API's fake clock both move past it, so the kill comes on either clock.
    for (let waited = 0; waited < KILL_MS + 2000 && !ended(first.pid); waited += 500) {
      await verify.advance(500);
      await sleep(500);
    }
    await waitFor(async () => (await answersSince(from)).length > 0, 3_000);
    await sleep(QUIET_MS);

    // Then
    assert.ok(ended(first.pid), 'wtpool job limit still runs 12 s after it started');
    assert.deepEqual(jobStarts().map((s) => s.argv), [['job', 'limit', '6']], 'a call ran after the one that timed out');
    assert.deepEqual(
      await answersSince(from),
      [{ type: 'jobSettings', version: 1, ok: false, error: 'wtpool job limit timed out' }],
      'the answer of a call killed after 10 s',
    );
  });

  it('job_settings__should_run_wtpool_once_each_and_report did_not_start', async () => {
    // Given
    // The setting `wtpool.path` names a missing file, once the tab shows the document.
    const doc = documentWith({});
    await showingAlone(doc);
    const verify = await verifyApi();
    const configuration = vscode.workspace.getConfiguration('wtpool');
    const previous = configuration.inspect('path').globalValue;
    const missing = path.join(process.env.WTPOOL_VERIFY_HOST_FAKES ?? '/nonexistent', 'wtpool-missing');
    let answers;

    // When
    try {
      // The sandbox's user settings only: the host runs with its own --user-data-dir.
      await configuration.update('path', missing, vscode.ConfigurationTarget.Global);
      await sleep(500);
      const from = (await posted()).length;
      await verify.fromWebview(settings({ limit: 6 }));
      await waitFor(async () => (await answersSince(from)).length > 0, 5_000);
      await sleep(QUIET_MS);
      answers = await answersSince(from);
    } finally {
      await vscode.workspace.getConfiguration('wtpool').update('path', previous, vscode.ConfigurationTarget.Global);
    }

    // Then
    assert.deepEqual(
      answers,
      [{ type: 'jobSettings', version: 1, ok: false, error: `wtpool did not start: spawn ${missing} ENOENT` }],
      'the answer of a wtpool that cannot start (R10)',
    );
    assert.deepEqual(jobStarts(), [], 'a wtpool job ran from another binary');
  });
});

describe('job settings outside the set', () => {
  it('messages__should_ignore_everything_outside_the_set jobsettings', async () => {
    // Given
    // Limits 1 to 64, `just` on the machine's list, `make` on this repository's, and
    // `cargo nextest` suggested.
    const doc = documentWith({});
    await showingAlone(doc);
    const verify = await verifyApi();
    const wt1 = doc.pools[0].worktrees.find((w) => w.name === 'wt1').path;
    const outside = [
      settings({ limit: 6, extra: 1 }),
      settings({ limit: 6, version: 1 }),
      settings({ id: wt1, limit: 6 }),
      settings({ limit: '6' }),
      settings({ limit: 2.5 }),
      settings({ limit: 0 }),
      settings({ limit: 65 }),
      settings({ limit: -1 }),
      settings({ limit: true }),
      settings({ limit: Number.NaN }),
      settings({ enforce: 'strict' }),
      settings({ enforce: 'WARN' }),
      settings({ enforce: 1 }),
      settings({}),
      settings({ limit: 6, heavy: { op: 'add', scope: 'machine', pattern: 'go test' } }),
      settings({ enforce: 'off', heavy: { op: 'add', scope: 'machine', pattern: 'go test' } }),
      settings({ heavy: null }),
      settings({ heavy: 'go test' }),
      heavy('replace', 'machine', 'go test'),
      heavy('add', 'global', 'go test'),
      heavy('add', 'machine', 'make *'),
      heavy('add', 'machine', '-rf'),
      heavy('add', 'machine', '+nightly'),
      heavy('add', 'machine', 'go  test'),
      heavy('add', 'machine', 'go test unit fast'),
      heavy('add', 'machine', 'x'.repeat(33)),
      heavy('add', 'machine', 'go\ttest'),
      heavy('add', 'machine', 'gö test'),
      heavy('add', 'machine', ''),
      heavy('add', 'machine', null),
      heavy('rm', 'machine', 'go test'),
      heavy('rm', 'machine', 'make'),
      heavy('rm', 'repository', 'just'),
      heavy('add', 'machine', 'just'),
      heavy('add', 'repository', 'make'),
      heavy('add', 'repository', 'protoc'),
      heavy('add-suggested', 'repository', 'cargo nextest'),
      heavy('add-suggested', 'machine', null),
      settings({ heavy: { op: 'add', scope: 'machine', pattern: 'go test', extra: 1 } }),
      settings({ heavy: { op: 'add', scope: 'machine' } }),
      JSON.parse('{"type": "jobSettings", "__proto__": {"limit": 6}}'),
      JSON.parse('{"type": "jobSettings", "limit": 6, "__proto__": {"x": 1}}'),
      JSON.parse('{"type": "jobSettings", "heavy": {"op": "add", "scope": "machine", "pattern": "go test", "__proto__": {"x": 1}}}'),
    ];
    // Without machine.jobs a jobSettings counts for nothing, valid or not.
    const withoutJobs = [
      settings({ limit: 6 }),
      settings({ enforce: 'require' }),
      settings({ limit: null, enforce: null }),
      heavy('add', 'machine', 'go test'),
      heavy('rm', 'machine', 'just'),
      heavy('add-suggested', 'repository', null),
      settings({ limit: 0 }),
    ];
    const from = (await posted()).length;

    // When
    let threw = 0;
    const send = async (message) => {
      try {
        await verify.fromWebview(message);
      } catch {
        threw += 1;
      }
    };
    for (const message of outside) await send(message);
    await sleep(QUIET_MS);
    const ranWithJobs = jobStarts().map((s) => s.argv);
    await show(documentWith(null));
    for (const message of withoutJobs) await send(message);
    await sleep(QUIET_MS);

    // Then
    assert.deepEqual(ranWithJobs, [], 'a jobSettings outside the set ran wtpool');
    assert.deepEqual(jobStarts().map((s) => s.argv), [], 'a jobSettings ran wtpool while the document has no machine.jobs');
    assert.deepEqual(await answersSince(from), [], 'a jobSettings outside the set was answered');
    assert.equal(threw, 0, 'fromWebview threw on a jobSettings outside the set');
  });
});
