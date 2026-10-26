// The demo's story, all its scripted data in one place: the placeholders, the status documents
// the fake wtpool prints, the test log that grows in the sandbox, and the pointer's moves. Times
// are seconds from the start of the recording; every time a document holds comes from them, so
// ages and clock times are the same on every run.
//
// The story, at 1000 x 600: a quiet pool; an agent starts in slot 1 and runs its tests in the
// background while it works on; the agent in slot 2 stops at a permission prompt; the user opens
// slot 1's detail strip and watches the test log stream in; the user's own build in the
// foreground spins the building dial; slot 3's agent ends its turn with its tests still running;
// the user closes the strip, so the whole table fits again, and frees slot 4 through VS Code's
// confirm dialog.
//
// Three parts follow features of later product versions, and `features` (features.mjs) picks how
// they read, so one story serves all:
// - `labels` (status-panel.md H16): a session's label is its title. Without it a session reads
//   `session HH:MM`.
// - `outputs` (the live-output work): an agent's background task is listed in `outputs`, and the
//   tab tails that file in the detail strip. The documents always carry `outputs`; an extension
//   without the feature ignores the property (R11), so the strip shows no output.
// - `background` (A1, revision 23): an agent whose turn ended works while its own background task
//   runs, its action `background: <summary>`. Without it that agent reads `idle`.

import path from 'node:path';

/** The names the demo shows. Change them here; nothing else names a repository or machine. */
export const PLACEHOLDERS = {
  /** The GitHub repository, `<owner>/<repo>`; its repo part names the main worktree's folder. */
  repository: 'acme/app',
  machine: 'my-mac',
  /** The folder the worktrees sit in, as their paths show it. */
  parent: '/Users/you/code',
};

/** The local wall time the recording starts at, in the time zone VS Code runs in. */
export const START = { date: '2026-03-10', time: '09:41:00' };

/** How long the recording runs, in seconds; the last steps leave a pause before the loop. */
export const LENGTH = 18;

/**
 * The lines the background `cargo test` of slot 1 prints, one every `LOG_GAP` seconds from
 * `LOG_AT`. They are few enough that the strip's output block, under slot 1's row, keeps its
 * newest line inside the 600 px window.
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

/** Seconds and the time zone to RFC 3339 times and local `HH:MM`, from START. */
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
  const shown = Date.UTC(part('year'), part('month') - 1, part('day'), part('hour'), part('minute'), part('second'));
  const startMs = guess - (shown - guess);
  const hhmm = new Intl.DateTimeFormat('en-GB', { timeZone, hour: '2-digit', minute: '2-digit', hourCycle: 'h23' });
  return {
    startMs,
    /** The RFC 3339 time `seconds` after the start, whole seconds, as wtpool prints times. */
    at: (seconds) => new Date(startMs + Math.round(seconds) * 1000).toISOString().replace('.000Z', 'Z'),
    /** The local `HH:MM` `seconds` after the start, as `session HH:MM` reads (H16). */
    hhmm: (seconds) => hhmm.format(new Date(startMs + seconds * 1000)),
  };
}

const GIT = { dirty: 0, ahead: 0, behind: 0, unreferenced: 0, operation: null, unknown: false };

/**
 * The story for `features` in `timeZone`, with slot 1's test log at `logPath`:
 * {first: the document the fake prints at start, steps: [{at, ...}], pathOf}. A step does one of:
 * `emit` a document, `log` a line, move the pointer `to` a target (`row:<name>`,
 * `action:<name>:<action>`, `dialog:<button>`, or `rest`) over `ms` and maybe `click`, or
 * `waitFor` the fake's start with that argv before the next step.
 */
export function story({ features, timeZone, logPath }) {
  const { at, hhmm } = clockOf(timeZone);
  const repo = PLACEHOLDERS.repository.split('/').at(-1);
  const slot = (n) => `${repo}-wt${n}`;
  const pathOf = (name) => `${PLACEHOLDERS.parent}/${name}`;

  /** A session actor; `first` is its first event, which names it without a title (H16). */
  const session = (key, title, first, fields) => ({
    key,
    kind: 'session',
    label: features.labels ? title : `session ${hhmm(first)}`,
    harness: 'claude-code',
    placed_by: 'cwd',
    error: null,
    reason: null,
    action: null,
    ...fields,
  });

  /** A background task (S13), its output file in the sandbox, beside slot 1's log. */
  const task = (id, summary, started, file = path.join(path.dirname(logPath), `${id}.output`)) => ({
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
    git: { ...GIT, ahead: null, behind: null },
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
  rows.set(repo, worktree(repo, {
    role: 'foreground',
    ...onBranch('main', { reasons: ['foreground'] }),
    git: GIT,
    activity: 'foreground',
    last_change: at(-190),
  }));
  rows.set(slot(1), worktree(slot(1), { last_change: at(-75 * 60) }));
  const oauth = (fields) => session('5c1d8e20-oauth', 'Add OAuth callback route', -14 * 60, fields);
  rows.set(slot(2), worktree(slot(2), {
    ...onBranch('feat/oauth-callback'),
    git: { ...GIT, dirty: 2 },
    activity: 'idle',
    actors: [oauth({ state: 'idle', since: at(-380), updated: at(-380) })],
    last_event: at(-380),
    last_change: at(-400),
  }));
  const deps = (fields) => session('9e4b7a31-deps', 'Bump dependencies', -29 * 60, fields);
  rows.set(slot(3), worktree(slot(3), {
    ...onBranch('chore/bump-deps'),
    git: { ...GIT, ahead: 1 },
    activity: 'idle',
    actors: [deps({ state: 'idle', since: at(-130), updated: at(-130) })],
    last_event: at(-130),
    last_change: at(-150),
  }));
  rows.set(slot(4), worktree(slot(4), {
    ...onBranch('feat/dark-mode'),
    git: GIT,
    activity: 'unused',
    // 25 m 20 s at the start, so its age reads `25 m` from the first frame to the last.
    last_event: at(-25 * 60 - 20),
    last_change: at(-24 * 60),
  }));

  const document = (seconds) => ({
    type: 'status',
    version: 1,
    generated: at(seconds),
    machine: { name: PLACEHOLDERS.machine },
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
  /** Changes rows at `seconds`, then emits the document; the changes apply in time order below. */
  const changes = [];
  const change = (seconds, edit) => changes.push({ seconds, edit });
  const first = document(0);

  // 1. A quiet pool; the pointer rests.
  steps.push({ at: 0, to: 'rest', ms: 0 });

  // 2. An agent starts in slot 1: it takes a branch and works, its action changing. At 5.3 s it
  // runs `cargo test` in the background and works on; the task's cargo builds in slot 1 (S4).
  const tests = task('b7k2q9', 'cargo test', 5.3, logPath);
  const login = (seconds, action, dirty, running = false) => {
    change(seconds, () =>
      set(slot(1), {
        ...onBranch('fix/flaky-login'),
        upstream: null,
        git: { ...GIT, ahead: null, behind: null, dirty },
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
      actors: [oauth({ state: 'working', action: { tool: 'Edit', target: 'src/routes/oauth.rs' }, since: at(3.6), updated: at(3.6) })],
      git: { ...GIT, dirty: 3 },
      last_event: at(3.6),
    }),
  );
  change(4.5, () =>
    set(slot(2), {
      activity: 'waiting',
      actors: [
        oauth({ state: 'waiting', reason: 'permission', action: { tool: 'Bash', target: 'git push' }, since: at(4.5), updated: at(4.5) }),
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
    set(repo, { activity: 'building', builds: [{ name: 'cargo', pid: 51877 }], last_event: at(7.8) }),
  );
  login(9.4, { tool: 'Edit', target: 'src/auth/session.rs' }, 2, true);

  // 6. Below the strip, slot 3's agent runs its tests in the background and ends its turn: it
  // still works, `background: npm test`, while they run (A1).
  const npmTest = task('n4w8r2', 'npm test', 8.4);
  change(8.4, () =>
    set(slot(3), {
      activity: 'working',
      actors: [deps({ state: 'working', action: { tool: 'Bash', target: 'npm test' }, since: at(8.4), updated: at(8.4) })],
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
      ],
      last_event: at(9.0),
    }),
  );

  // 7. The user clicks slot 1's row again, which closes the strip, so slot 4 is in view; then
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
      git: { ...GIT, ahead: null, behind: null },
      free: true,
      reasons: [],
      activity: 'free',
      last_change: at(15.8),
    }),
  );
  steps.push({ at: 16.1, to: 'rest', ms: 900 });

  // 8. A pause until LENGTH, then the GIF loops.
  changes.sort((a, b) => a.seconds - b.seconds);
  for (const { seconds, edit } of changes) {
    edit();
    steps.push({ at: seconds, emit: document(seconds) });
  }
  steps.sort((a, b) => a.at - b.at);
  return { first, steps, pathOf };
}

/** Frames worth a look, by story time: the review copies `--keyframes` writes. */
export const KEYFRAMES = [
  { at: 1.2, name: 'quiet-pool' },
  { at: 3.4, name: 'agent-working' },
  { at: 5.6, name: 'waiting-for-permission' },
  { at: 8.6, name: 'output-streaming' },
  { at: 11.6, name: 'output-and-building' },
  { at: 13.4, name: 'background-and-hover' },
  { at: 15.3, name: 'confirm-free' },
  { at: 17.6, name: 'slot-freed' },
];
