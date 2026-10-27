/**
 * @fileoverview The demo's story: the placeholders, the status documents the fake wtpool prints,
 * the test log that grows in the sandbox, and the pointer's moves. Times are seconds from the
 * start of the recording. Every time a document holds derives from them, so ages and clock times
 * are the same on every run.
 */
import path from 'node:path';

/** The names the demo shows, and the only place that names a repository or a machine. */
export const PLACEHOLDERS = {
  /** The GitHub repository, `<owner>/<repo>`. Its repo part names the main worktree's folder. */
  repository: 'acme/app',
  machine: 'my-mac',
  /** The folder the worktrees sit in, as their paths show it. */
  parent: '/Users/you/code',
};

/** The local wall time the recording starts at, in the time zone VS Code runs in. */
const START = { date: '2026-03-10', time: '09:41:00' };

export const LENGTH_SECONDS = 25;

/**
 * The lines the background `cargo test` of slot 1 prints, one every {@link LOG_GAP} seconds from
 * {@link LOG_AT}. They are few enough that the strip's output block, under slot 1's row, keeps its
 * newest line inside the window.
 */
const TEST_LINES = [
  '   Compiling tower-http v0.6.6',
  '   Compiling app v0.4.0 ({path})',
  '    Finished `test` profile [unoptimized + debuginfo] target(s) in 7.84s',
  '     Running tests/login.rs (target/debug/deps/login-8f3c1a2e9b7d4c60)',
  '',
  'running 4 tests',
  'test login_rejects_a_wrong_password ... ok',
  'test login_sets_the_session_cookie ... ok',
  'test login_retries_after_a_timeout ... ok',
];
const LOG_AT = 7.2;
const LOG_GAP = 0.45;

/** Returns the story's clock in `timeZone`: {@link START} in epoch ms, and story seconds as times. */
export function clockOf(timeZone) {
  const guess = Date.parse(`${START.date}T${START.time}Z`);
  const parts = new Intl.DateTimeFormat('en-US', {
    timeZone,
    hourCycle: 'h23',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).formatToParts(new Date(guess));
  const part = (type) => Number(parts.find((p) => p.type === type).value);
  const shown = Date.UTC(
    part('year'),
    part('month') - 1,
    part('day'),
    part('hour'),
    part('minute'),
    part('second'),
  );
  const startMs = guess - (shown - guess);
  const hhmm = new Intl.DateTimeFormat('en-GB', {
    timeZone,
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  });
  return {
    startMs,
    /** The RFC 3339 time `seconds` after the start, whole seconds, as wtpool prints times. */
    at: (seconds) =>
      new Date(startMs + Math.round(seconds) * 1000).toISOString().replace('.000Z', 'Z'),
    /** The local `HH:MM` `seconds` after the start, as a session without a title shows it. */
    hhmm: (seconds) => hhmm.format(new Date(startMs + seconds * 1000)),
  };
}

const CLEAN_GIT = {
  dirty: 0,
  ahead: 0,
  behind: 0,
  unreferenced: 0,
  operation: null,
  unknown: false,
};

/**
 * Returns the story for `features` in `timeZone`, with slot 1's test log at `logPath`, as
 * `{first, steps, pathOf}`. `first` is the document the fake prints at start. A step at `at`
 * seconds does one of these: `emit` a document, `log` a line, move the pointer `to` a target
 * (`row:<name>`, `machine`, `action:<name>:<action>`, `dialog:<button>` or `rest`) over `ms` and maybe
 * `click`, or `waitFor` the fake's start with that argv before the next step.
 *
 * The documents list `outputs` whatever `features.outputs` says: an extension without that
 * feature ignores the property.
 */
export function story({ features, timeZone, logPath }) {
  const { at, hhmm } = clockOf(timeZone);
  const repo = PLACEHOLDERS.repository.split('/').at(-1);
  const slot = (n) => `${repo}-wt${n}`;
  const pathOf = (name) => `${PLACEHOLDERS.parent}/${name}`;

  const session = (key, title, firstEventAt, fields) => ({
    key,
    kind: 'session',
    label: features.labels ? title : `session ${hhmm(firstEventAt)}`,
    harness: 'claude-code',
    placed_by: 'cwd',
    error: null,
    reason: null,
    action: null,
    ...fields,
  });

  const backgroundTask = (
    id,
    summary,
    started,
    file = path.join(path.dirname(logPath), `${id}.output`),
  ) => ({
    task: id,
    path: file,
    summary,
    running: true,
    started: at(started),
    ended: null,
  });

  const worktree = (name, fields) => ({
    path: pathOf(name),
    name,
    role: 'slot',
    missing: false,
    branch: null,
    upstream: null,
    git: { ...CLEAN_GIT, ahead: null, behind: null },
    free: true,
    reasons: [],
    activity: 'free',
    actors: [],
    builds: [],
    last_event: null,
    last_change: at(-3 * 3600),
    ...fields,
  });

  const onBranch = (branch, fields) => ({
    branch,
    upstream: `origin/${branch}`,
    free: false,
    reasons: ['branch'],
    ...fields,
  });

  const rows = new Map();
  const set = (name, fields) => rows.set(name, { ...rows.get(name), ...fields });
  rows.set(
    repo,
    worktree(repo, {
      role: 'foreground',
      ...onBranch('main', { reasons: ['foreground'] }),
      git: CLEAN_GIT,
      activity: 'foreground',
      last_change: at(-190),
    }),
  );
  rows.set(slot(1), worktree(slot(1), { last_change: at(-75 * 60) }));
  const oauth = (fields) => session('5c1d8e20-oauth', 'Add OAuth callback route', -14 * 60, fields);
  rows.set(
    slot(2),
    worktree(slot(2), {
      ...onBranch('feat/oauth-callback'),
      git: { ...CLEAN_GIT, dirty: 2 },
      activity: 'idle',
      actors: [oauth({ state: 'idle', since: at(-380), updated: at(-380) })],
      last_event: at(-380),
      last_change: at(-400),
    }),
  );
  const deps = (fields) => session('9e4b7a31-deps', 'Bump dependencies', -29 * 60, fields);
  // A second agent in slot 3 has run `cargo test` in the background for a while: its run time
  // is 3 m 12 s at CARGO_READ seconds, when the machine strip is open (the tab shows `3 m`).
  const CARGO_READ = 19;
  const cargoTest = backgroundTask('c5m1x7', 'cargo test', CARGO_READ - (3 * 60 + 12));
  const retry = () =>
    session('d2a6f815-retry', 'Add retry', -9 * 60, {
      state: features.background ? 'working' : 'idle',
      action: features.background ? { tool: 'background:', target: 'cargo test' } : null,
      since: at(-190),
      updated: at(-12),
      outputs: [cargoTest],
    });
  rows.set(
    slot(3),
    worktree(slot(3), {
      ...onBranch('chore/bump-deps'),
      git: { ...CLEAN_GIT, ahead: 1 },
      activity: features.background ? 'working' : 'idle',
      actors: [deps({ state: 'idle', since: at(-130), updated: at(-130) }), retry()],
      last_event: at(-130),
      last_change: at(-150),
    }),
  );
  rows.set(
    slot(4),
    worktree(slot(4), {
      ...onBranch('feat/dark-mode'),
      git: CLEAN_GIT,
      activity: 'unused',
      // 25 m 20 s at the start, so its age reads `25 m` from the first frame to the last.
      last_event: at(-25 * 60 - 20),
      last_change: at(-24 * 60),
    }),
  );

  // The machine's heavy jobs: slot 3's `cargo test` runs, and from 9.4 s slot 1's agent waits.
  const heavyJob = (id, summary, worktreeName, actor, since) => ({
    id,
    summary,
    label: null,
    worktree: pathOf(worktreeName),
    actor,
    since: at(since),
  });
  const jobs = {
    limit: 1,
    default: 4,
    min: 1,
    max: 64,
    enforce: 'warn',
    source: { limit: 'machine', enforce: 'machine' },
    config: `${PLACEHOLDERS.parent.replace(/\/code$/, '')}/.config/wtpool/machine.toml`,
    error: null,
    heavy: [
      { pattern: 'cargo build', source: 'repository' },
      { pattern: 'cargo test', source: 'repository' },
      { pattern: 'npm test', source: 'repository' },
    ],
    suggested: [],
    running: [
      heavyJob('4121a1b2c3', 'cargo test', slot(3), retry().key, CARGO_READ - (3 * 60 + 12)),
    ],
    waiting: [],
    outside: [],
  };

  const document = (seconds) => ({
    type: 'status',
    version: 1,
    generated: at(seconds),
    machine: {
      name: PLACEHOLDERS.machine,
      ...(features.jobs ? { jobs: structuredClone(jobs) } : {}),
    },
    pools: [
      {
        common_dir: `${pathOf(repo)}/.git`,
        agents: 'on',
        skipped_records: 0,
        repository: { name: repo, github: PLACEHOLDERS.repository },
        worktrees: [...rows.values()].map((row) => structuredClone(row)),
      },
    ],
  });

  const steps = [];
  // Row edits by story time. They apply in time order at the end, each followed by its document.
  const changes = [];
  const change = (seconds, edit) => changes.push({ seconds, edit });
  const first = document(0);

  // 1. A quiet pool. The pointer rests.
  steps.push({ at: 0, to: 'rest', ms: 0 });

  // 2. An agent starts in slot 1: it takes a branch and works, its action changing. At 5.3 s it
  // runs `cargo test` in the background and works on, while the task's cargo builds in slot 1.
  const tests = backgroundTask('b7k2q9', 'cargo test', 5.3, logPath);
  const login = (seconds, action, dirty, running = false) => {
    change(seconds, () =>
      set(slot(1), {
        ...onBranch('fix/flaky-login'),
        upstream: null,
        git: { ...CLEAN_GIT, ahead: null, behind: null, dirty },
        activity: 'working',
        builds: running ? [{ name: 'cargo', pid: 48213 }] : [],
        actors: [
          session('a3f09c4e-login', 'Fix flaky login test', 1, {
            state: 'working',
            action,
            since: at(1.5),
            updated: at(seconds),
            ...(running ? { outputs: [tests] } : {}),
          }),
        ],
        last_event: at(seconds),
        last_change: at(1.5),
      }),
    );
  };
  login(1.5, { tool: 'Read', target: 'tests/login.rs' }, 0);
  login(2.7, { tool: 'Edit', target: 'tests/login.rs' }, 1);

  // 3. The agent in slot 2 works again and stops at a permission prompt: the `1 waiting` chip,
  // and the tab's title `Worktrees (1)`.
  change(3.6, () =>
    set(slot(2), {
      activity: 'working',
      actors: [
        oauth({
          state: 'working',
          action: { tool: 'Edit', target: 'src/routes/oauth.rs' },
          since: at(3.6),
          updated: at(3.6),
        }),
      ],
      git: { ...CLEAN_GIT, dirty: 3 },
      last_event: at(3.6),
    }),
  );
  change(4.5, () =>
    set(slot(2), {
      activity: 'waiting',
      actors: [
        oauth({
          state: 'waiting',
          reason: 'permission',
          action: { tool: 'Bash', target: 'git push' },
          since: at(4.5),
          updated: at(4.5),
        }),
      ],
      last_event: at(4.5),
    }),
  );
  login(5.3, { tool: 'Bash', target: 'cargo test' }, 1);
  login(5.8, { tool: 'Read', target: 'src/auth/session.rs' }, 1, true);

  // 4. The user clicks slot 1's row: the detail strip opens under it, and its Output section
  // shows the test log as cargo writes it.
  steps.push({ at: 5.9, to: `row:${slot(1)}`, ms: 900, click: true });
  const logLines = TEST_LINES.map((line) => line.replace('{path}', pathOf(slot(1))));
  logLines.forEach((line, index) => steps.push({ at: LOG_AT + index * LOG_GAP, log: line }));

  // 5. The user builds in the foreground: a build with no agent, so the row is `building`.
  change(7.8, () =>
    set(repo, {
      activity: 'building',
      builds: [{ name: 'cargo', pid: 51877 }],
      last_event: at(7.8),
    }),
  );
  login(9.4, { tool: 'Edit', target: 'src/auth/session.rs' }, 2, true);
  // Slot 1's agent asks for a place to run `cargo build`: the machine button reads `1 queued`.
  change(9.4, () => {
    jobs.waiting = [heavyJob('4122d4e5f6', 'cargo build', slot(1), 'a3f09c4e-login', 9.4)];
  });

  // 6. Below the strip, slot 3's agent runs its tests in the background and ends its turn: it
  // still works, `background: npm test`, while they run.
  const npmTest = backgroundTask('n4w8r2', 'npm test', 8.4);
  change(8.4, () =>
    set(slot(3), {
      activity: 'working',
      actors: [
        deps({
          state: 'working',
          action: { tool: 'Bash', target: 'npm test' },
          since: at(8.4),
          updated: at(8.4),
        }),
        retry(),
      ],
      last_event: at(8.4),
    }),
  );
  change(9.0, () =>
    set(slot(3), {
      activity: features.background ? 'working' : 'idle',
      actors: [
        deps({
          state: features.background ? 'working' : 'idle',
          action: features.background ? { tool: 'background:', target: 'npm test' } : null,
          since: at(9.0),
          updated: at(9.0),
          outputs: [npmTest],
        }),
        retry(),
      ],
      last_event: at(9.0),
    }),
  );

  // 7. The user clicks slot 1's row again to close the strip, so slot 4 is in view. Then the user
  // hovers slot 4, the finished slot, clicks free, and confirms in VS Code's dialog.
  steps.push({ at: 11.8, to: `row:${slot(1)}`, ms: 200, click: true });
  steps.push({ at: 12.5, to: `row:${slot(4)}`, ms: 700 });
  steps.push({ at: 13.6, to: `action:${slot(4)}:free`, ms: 500, click: true });
  steps.push({ at: 14.8, to: 'dialog:Free', ms: 700, click: true });
  steps.push({ at: 15.6, waitFor: ['free', slot(4)] });
  change(15.8, () =>
    set(slot(4), {
      branch: null,
      upstream: null,
      git: { ...CLEAN_GIT, ahead: null, behind: null },
      free: true,
      reasons: [],
      activity: 'free',
      last_change: at(15.8),
    }),
  );
  steps.push({ at: 16.1, to: 'rest', ms: 900 });

  // 8. The user opens the machine strip from the heading's machine button: the limit and the
  // policy on the left, the running and the queued job on the right. It closes again.
  if (features.jobs) {
    steps.push({ at: 17.4, to: 'machine', ms: 700, click: true });
    steps.push({ at: 22.4, to: 'machine', ms: 200, click: true });
    steps.push({ at: 23.2, to: 'rest', ms: 900 });
  }

  // 9. A pause until LENGTH_SECONDS, then the GIF loops.
  changes.sort((a, b) => a.seconds - b.seconds);
  for (const { seconds, edit } of changes) {
    edit();
    steps.push({ at: seconds, emit: document(seconds) });
  }
  steps.sort((a, b) => a.at - b.at);
  return { first, steps, pathOf };
}

/** The frames `--keyframes` copies for review, by story time. */
export const KEYFRAMES = [
  { at: 1.2, name: 'quiet-pool' },
  { at: 3.4, name: 'agent-working' },
  { at: 5.6, name: 'waiting-for-permission' },
  { at: 8.6, name: 'output-streaming' },
  { at: 11.6, name: 'output-and-building' },
  { at: 13.4, name: 'background-and-hover' },
  { at: 15.3, name: 'confirm-free' },
  { at: 17.6, name: 'slot-freed' },
  { at: 19.6, name: 'machine-strip' },
  { at: 24.4, name: 'strip-closed' },
];
