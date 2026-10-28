# Verifying the status panel

**Status:** approved, revision 27 (2026-10-26: the `cc` fixtures come from the recordings of Claude Code 2.1.310, on which every `cc` check passed live; the trace list gives X12 the five `int` checks of the CLI's agent guard of status-panel.md revision 28, and names the checks `verify/trace.toml` already held under H13, H16, H22, A1, W4, X6 and X12, so the trace list matches it; revision 26: the trace list gives R2 the four `int` checks of the CLI's escapes of status-panel.md revision 27, from phase 3, H18 `hook__should_keep_a_record_it_can_read` and H26 `status__should_read_the_kind_of_an_older_call`, and the harness checks name `harness:fmt`, `harness:clippy` and `harness:rustdoc` beside `harness:self_test`; Contract test: `canUseTool` allows only each session's scripted calls, the contract turn runs in auto mode with no tools, the run's settings deny WebFetch and WebSearch and turn Claude Code's sandbox on, and the harness supports the latest installed Claude Code alone, a `--record` run in which every `cc` check passes moving the recordings of other versions into `recordings-backup/`; No side effects: only an allowlist of variables reaches `claude`, `DISABLE_AUTOUPDATER=1` keeps your installed `claude`, the sessions' XDG data and cache folders are in the run folder, and `contract__should_run_in_the_sandbox` also fails when your Claude Code install changed during the run; revision 25: the checks of status-panel.md revision 24: test interface 5 lists the `starting` connection; the trace list gives X6 the two checks of the starting header and H21 the check of the hook's daily gc, both from phase 3, and H27 `hook__should_end_a_task_through_its_session`; What needs you no longer asks for a daily gc job; Oracle files: `oracle_unchanged` compares the tree with the lock alone, which holds a hash per file and key, the ids of the spent entries and a seal, a change counts only through a live entry whose `files` table pins its hash, and an accept line names the entry's hash; Canaries: a canary is also caught when its check ends with the needs you code its `Expect:` line names; No side effects: the int checks run at the normal priority once built; revision 24: a run has no side effects on your use of the Mac (new section No side effects): on macOS every window of `host` and `e2e` is transparent and lets the mouse through; Electron's clipboard is private to each VS Code, the copy checks read it there and fail when the system pasteboard's change count moves; native dialogs, OS notifications and beeps are refused and fail the layer with `<layer>:side_effects`; power save blockers and VS Code's macOS recent documents are refused; the profile silences accessibility signals; `cc` keeps Claude Code's configuration in the run folder with your keychain login, passes no variable of the starting session, loads no claude.ai connectors, deletes its temporary folders, and `contract__should_run_in_the_sandbox` fails on a trace in `~/.claude/`; every job but bench's runs at utility QoS on macOS (`nice` elsewhere, `WTPOOL_VERIFY_PRIORITY=normal` to turn off); the harness also calls `osascript` and `taskpolicy` on macOS; revision 23: no window of a run takes focus: on macOS each VS Code of `host` and `e2e` starts paused and runs as an app that is never active, so its windows open behind the others; a focus guard samples the frontmost app with `lsappinfo` and fails the layer with `<layer>:focus` when a VS Code of the run becomes the frontmost app; `lsappinfo` is the harness's one OS-specific tool, on macOS only; What needs you no longer asks you to accept windows taking focus; H28 and the new H22 check are phase 3, and the trace list gives D5 H22 and H28; a check skipped for want of the recordings folder counts as passing for reuse while the folder is missing; the R12 and X19 checks cover tabs; revision 22: editorial: the tool is renamed wtpool (command, crates, settings file, activity folder, variables, extension id); no behaviour change; revision 21: the live output of background tasks of status-panel.md revision 20: the webview input gains the `output` message and the webview sends `select` and `follow`, the extension's tests also wrap the `pty` of `createTerminal`, the DOM names the Output section, a `live_output` flow, and the trace list names the checks of H27, S13, R12, U9 and X19, which are phase 3; revision 20: the bench pool's foreground gets a `bench` branch when the source's HEAD is detached, a p95 budget stops once it can no longer pass, and the budgets wait for other verify parts and run with none under way; revision 19: the webview input gains `error` and `no_folder`, the DOM names the single pool heading, the error line and the no folder note; revision 18: verify runs from a fresh clone on any machine: coverage and repository context measure from a base with a stated source, a `skipped` outcome, the `cc` checks by replay alone without a recordings folder, the real pool from `WTPOOL_FOREGROUND` and the bench source from `WTPOOL_BENCH_SOURCE`, and a portable harness (the `sysinfo` crate, VS Code found or downloaded, `WTPOOL_VERIFY_WRAPPER`, XDG folders, Playwright's Chromium on first use) tested on macOS only; H25 and H26 of status-panel.md revision 16 are phase 2; revision 17: editorial, the real pool and the owner named without machine paths, no process change; revision 16: the narrow window scrolls sideways, a `narrow_scroll` flow, the visual checks of status-panel.md revision 14 in the trace list; revision 15: a focused slot row in the visual review; revision 14: stress sized to one wtpool instance and fewer than 10 worktrees)

How an unattended build proves that it meets [status-panel.md](status-panel.md).

## Threat model

The builder is an honest agent with full file access, under pressure to finish. It risks drift,
shortcuts (a weaker check, a raised limit, a rerun until green), grading its own work, and tests
that mirror the code. Forgery is out of scope. One rule answers all four: **the builder never
writes what decides PASS.**

## Orchestration

A lead Claude Code session runs each phase through fresh `opus` agents, with these prompts:

1. The oracle author (prompt A) writes the checks.
2. The lead runs `cargo xtask freeze --phase N`, which commits exactly the author's files.
3. The builder (prompt C) writes the product, and the lead commits it.
4. The lead clones that commit into `~/.cache/wtpool-verify/clones/` and runs there every verify part
   whose inputs changed since its last passing result (see The report), with
   `--contract` only when the installed Claude Code has no live result. On a machine without a
   recordings folder, the `cc` checks count by replay alone, and the report says so (Contract
   test).
5. When that run says PASS, one verifier (prompt B) reviews the clone once.
6. The lead commits the review to `verify/reviews/phase-N/`.

After step 4 or 6 the lead takes the first route that applies:

1. A needs you code: stop, but first rerun a `machine_busy` part, at most twice, an hour apart.
2. A FAIL, or a blocking review finding against the product: step 3 with the findings. After the
   third builder round of a phase, stop.
3. A blocking review finding that names a wrong check: add an entry to
   `verify/oracle-changes.toml`, accept it under your standing rule for blocking fixes, run prompt
   A with the finding, freeze, and return to step 3. One such round per phase; a second stops.
4. Done: the next phase.

The verifier classes each finding blocking or non blocking. A non blocking finding goes on a list in
the review and starts no round. Each stop reports the open findings and waits for you.

`cargo` runs directly, with no wrapper shell: the repository needs none. A heavy job runs under a
wrapper only when `WTPOOL_VERIFY_WRAPPER` names one (Portability).

```
Every prompt starts: Never write auto memory or commit. Run a built wtpool only with WTPOOL_FOREGROUND,
WTPOOL_SLOT_DIR and HOME set to temporary folders, never on the real pool. Run each verify part in the
background, and end your run only after its exit notice.

A. Write the oracle for phase {phase} of docs/design/status-panel/ at {commit}. First fix:
{findings}. Read both docs and the mockups. Write each missing check that the trace list and
catalog name for phase {phase} or earlier, under that name, and the harness parts Layers gives this
phase. Test only through the test interfaces. Do not read crates/ or vscode/src. Write only oracle
files outside docs/. Check the stricter reading of anything ambiguous, and report it. Run each
verify part your new checks are in, once: each new check must fail on an assertion, with its
canary applied if it has one, or test an absence listed in verify/absent.toml.

C. Build phase {phase} of docs/design/status-panel/ from {commit}. Never edit oracle files,
CLAUDE.md, AGENTS.md, .claude/, ~/.local/bin/wtpool, ~/.claude/settings.json, VS Code settings or
skills. Follow the test standards. First fix: {findings}. Run the verify parts your change touches until
they pass, each at most 5 times, and none whose inputs are unchanged since it passed. Never pass
--contract. Report the verdict, checks you think wrong, and the install commands.

B. Verify phase {phase}. Read docs/design/ at {freeze}. Clone {clone} holds {commit}, and {run} is
its verify run, which says PASS. 1. Does each check in the author's diff from {previous_freeze} to
{freeze} test what the spec says, and does every user flow of phase {phase} or earlier have an e2e
check that exercises it? 2. Review the code against each ID of phase {phase} or earlier, each line
no oracle layer covers, and the tests against
the test standards. 3. Compare the fixed screenshots with the normative mockup parts. List blocking
and non blocking findings with file and line, naming each check you think wrong. Blocking means a
spec ID is broken or untested, or a risk to the owner's real setup. End with exactly "verdict: clean"
when nothing blocks, else "verdict: not clean".
```

## Oracle files

The builder never changes these:

1. `xtask/`, `vscode/test/`, and `verify/` except `out/`, `reviews/`, `accepted.txt` and
   `oracle-changes.toml`.
2. The `members` list of the root `Cargo.toml`, the `[alias]` table of `.cargo/config.toml`.
3. `clippy.toml`, `rustfmt.toml`, `[workspace.lints]`, `vscode/eslint.config.mjs`,
   `vscode/.stylelintrc.json`.
4. `vscode/package.json` and its lockfile, from phase 1. The builder asks for dependencies.
5. `docs/design/`, with the mockups. Only you change it.

`freeze` checks that each new check fails, a check written for a canary with that canary applied,
except those in `verify/absent.toml`. A check is new when the tree implements it and the commit
under the author's changes does not, as every check in that commit came in through an accepted
entry. A changed check need not fail. `freeze` writes
`verify/oracle.lock` and commits. The lock holds a SHA 256 per file and key, the id of every entry
of `verify/oracle-changes.toml` at the freeze, which are spent, and a seal: the SHA 256 of the rest
of the lock. A phase N author may change earlier phases' files. Later an oracle file changes only
through a new entry, an `oracle_change` until you accept it.

`oracle_unchanged` compares the tree with the lock alone, so every copy of the history, a squashed
one included, judges alike. Each file or key that differs from the lock, and the lock itself when
its seal no longer matches, needs a live entry: one the lock does not list as spent, whose `files`
table maps the path, or `file:key` for a key, to its hash after the change, or to `missing` for a
deletion. A later edit changes that hash, so the entry no longer covers it. The entry counts once
`verify/accepted.txt` holds `accept <id> <entry hash>`, the hash of the entry's content, so an
entry widened or edited after it was accepted needs a new line. Any other line but a comment fails
the check. A change no live entry pins fails, and the message gives the `files` lines to add; an
entry not accepted at its content gives `oracle_change`, and the message gives its accept line.

## Test interfaces

1. **CLI:** `wtpool hook`, `wtpool hook gc`, `wtpool hook install`, `wtpool status --json`, `--watch` and
   `--schema`, `wtpool setup` and the pool commands, `wtpool --version`, records and documents. `WTPOOL_BIN`
   names the binary.
2. **Cargo feature `verify`:** `WTPOOL_VERIFY_CLOCK=<file>` holds an RFC 3339 time that every clock
   read returns, and a change fires timers. `WTPOOL_VERIFY_PANIC=<event>` makes `wtpool hook` panic.
   `WTPOOL_VERIFY_PROCS=<file>` replaces the process table with JSON lines of `pid`, `ppid`,
   `started`, `name` and `cwd`.
3. **Extension:** with `WTPOOL_VERIFY=1`, `activate` returns `{ fromWebview(message), toWebview(),
   advance(ms) }`, `advance` moving a fake clock. Tests wrap `executeCommand`, `createTerminal`
   (with the `pty` of a Follow terminal, X19) and `clipboard.writeText`.
4. **Builds:** `npm run build` writes `vscode/media/webview.js` and `vscode/out/html.js` with
   `buildHtml(webview, nonce)`. `npm run package` writes `vscode/wtpool.vsix`.
5. **Webview input:** `{ "type": "view", "version": 1, "connection": "starting" | "live" | "stale" |
   "paused", "last_line": <time>, "retry_at": <time>, "status": <document> | null, "error"?: <string>,
   "no_folder"?: true }`. `starting` is a new watch before wtpool's first line (X6), with `status`
   null. `retry_at` is the next restart while `paused`, else absent. `error` is
   wtpool's error (R10), present only while `paused`. With `no_folder` (R9), `connection` is `paused`,
   there is no `retry_at`, and `status` is null. Otherwise `status` is null only before the first
   document of the folder wtpool runs in. The extension also posts `{ "type": "output", "version": 1,
   "row": <row id>, "tails": [{ "path": <string>, "lines": [<string>], "error"?: <string> }] }`
   (R12): the tails of the selected row's `outputs`, only while the tab is visible and only for
   paths the last document lists for that row, `error` saying why a file shows nothing. The
   webview sends `{type, id}` messages (X11), `select` among them, so the extension knows the
   selected row.
6. **DOM:** the single heading `tr.pool-heading[data-machine][data-pool]` (U8), with `data-pool`
   the pool's `common_dir`, holding `.repository` and `.heading-machine`; rows
   `tr[data-row-id]` with six `td` then `td.actions` holding `button[data-action]` (`open`,
   `terminal`, `copyPath`), lines `.line`, chips `.chip`, the detail strip `tr.detail`, the header
   text `.connection` and wtpool's error `.wtpool-error` in the header, the agents off note `.agents-off`,
   the no folder note `.no-folder`, and in the strip the Output section `section.outputs`, holding
   per task `.output[data-output-path]` with `.output-heading`, `button.follow` and
   `pre.output-tail` (U9). `tr[data-machine]` matches that heading; `.machine` and
   `.machine-icon` are gone. Icons are CSS `::before` without text.

## Isolation

Sandboxes live in `verify/out/<run>/`. `wtpool` and VS Code get `HOME`, `TMPDIR`, `XDG_*`,
`--user-data-dir` and `--extensions-dir` there. Cargo, rustup, npm, Playwright and `claude` keep
the real home and caches. All get sandbox `WTPOOL_FOREGROUND` and `WTPOOL_SLOT_DIR`, `WTPOOL_VERIFY_SANDBOX=1`
and `TZ=UTC`, but `e2e` runs in `Asia/Kolkata`. `.cargo/config.toml` forces
`GIT_CONFIG_GLOBAL=/dev/null`, so git settings go in each sandbox repository, including the
committer `wtpool-verify <verify@wtpool.invalid>`. Sandbox repositories have no remote outside the run
folder: `origin` is a bare repository inside it. Other outside paths:
`~/.cache/wtpool-verify/`, `~/.local/share/wtpool-verify/`. Sandbox pools hold `wtpool-activity/.verify-sandbox`.

`isolation__left_no_trace` compares snapshots taken before and after each part and at `--report`.
It looks for no processes and kills none. It fails when the run left the run folder or a contract session in
the real pool, the pool `WTPOOL_FOREGROUND` names (its `.git/wtpool-activity/`, its `git worktree list`,
its `.git/wtpool.toml`), `wtpool-verify` in `~/.claude/settings.json`, or a folder the run created in
`~/.vscode/extensions/`. It also fails when a repository in the run folder has a remote outside
it, or when a branch or tag in the real pool points at a commit by `wtpool-verify`. The pool rules are
skipped, with a message, when `WTPOOL_FOREGROUND` is unset or no worktree is there. Your own
work on the pool during a run moves refs and must not fail the run.

No window of a run takes focus or input from the apps in use. On macOS each VS Code of `host`
and `e2e` starts paused (`--inspect-brk`) and, before its first line runs, becomes an app that is
never active (`app.setActivationPolicy('prohibited')`), since Electron activates its app whenever
a window shows and VS Code steals focus for a window a second instance asks for. Its windows are
transparent (No side effects), and the checks reach them through CDP and Electron's API alone: no
check sends an OS key or mouse event, and none minimizes a window, which hides its page. While `host` or
`e2e` runs, the focus guard asks `lsappinfo` for the frontmost app every 250 ms, lists what it saw
in `<layer>/front.txt`, and fails the layer with `<layer>:focus` when that app is a process whose
command line names the run folder. On other platforms VS Code starts as before, under xvfb on
Linux.

## No side effects

You keep using the machine while a run goes on, and the run changes nothing of that use. On macOS
the stop before VS Code's first line (Isolation) also installs, in each VS Code of `host` and
`e2e`:

1. **Windows.** Every window is transparent from its creation and lets the mouse through. macOS
   orders a window of an app that is never active just behind the frontmost window, so above
   other apps' windows, but a transparent one shows nowhere, and no pointer, click or scroll of
   yours reaches it. It still paints, its page stays visible and its timers run, so screenshots
   and every check that needs a visible tab are as before. The app has no Dock icon or menu bar.
2. **Clipboard.** Electron's clipboard, through which VS Code's main process copies, is a store
   private to that VS Code: the copy checks read the copied text there, which shows the
   extension's copy reached VS Code's clipboard API, and no check reads or writes the system
   pasteboard. Each copy check reads the pasteboard's change count before and after, without its
   content and so without permission, and fails with a setup error when it moved.
3. **Dialogs, notifications and sound.** A native dialog, an OS notification or `shell.beep` is
   refused and noted in `<layer>/side-effects.txt`, which fails the layer with
   `<layer>:side_effects`; `window.dialogStyle` `custom` keeps VS Code's modals in the window.
   The profile turns every accessibility signal's sound off and their volume to 0.
4. **Recent documents and sleep.** VS Code clears and refills the macOS recent documents of its
   bundle, the list of the Dock menu your VS Code shares, on each start; there it does nothing,
   and no power save blocker keeps your display or Mac awake.

`cc` keeps Claude Code's configuration in `<run>/cc/claude-config/` (`CLAUDE_CONFIG_DIR`), with
`CLAUDE_SECURESTORAGE_CONFIG_DIR` empty so the keychain entry of your login is the one read:
transcripts, the session list, project entries, file history and session environments stay in
the run folder, so no session shows in your session list, `/resume` or VS Code's Claude panel.
Only an allowlist of variables reaches `claude`, and so its hooks and commands: `PATH`, `HOME`,
`TMPDIR`, `USER`, `LOGNAME`, `SHELL`, `LANG`, `LANGUAGE`, `LC_*`, a login by token
(`ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`), the two git variables
of `.cargo/config.toml` and `WTPOOL_*`. So no variable of the session or editor that started the
run does, nor an SSH agent or a GitHub or cloud token. `HOME` stays yours, as macOS finds your
login keychain through it. `DISABLE_AUTOUPDATER=1` keeps a session from replacing your installed
`claude`, since the run's configuration lacks your `autoUpdates` setting. `XDG_STATE_HOME`,
`XDG_DATA_HOME` and `XDG_CACHE_HOME` name folders in `<run>/cc/`, so Claude Code's start-up
cleanup of old versions reads an empty versions folder of the run and never prunes yours. The
claude.ai connectors of your account stay out (`ENABLE_CLAUDEAI_MCP_SERVERS=false`), and your user
settings and so your hooks never load (`settingSources: []`). The runner deletes the folders its
sessions leave in Claude Code's temporary folder, `/tmp/claude-<uid>/`, and
`contract__should_run_in_the_sandbox` also fails when its session left a file in `~/.claude/` or
an entry in `~/.claude.json`, or when your Claude Code install changed during the run: an entry of
its versions folder, by name and modification time, or the target of `~/.local/bin/claude`.

Every job of a part but `bench`, and the shared builds, starts at a priority that yields the
processor to your work: the `utility` QoS clamp (`taskpolicy -c utility`) on macOS, where `nice`
changes nothing measurable, `nice -n 10` elsewhere. On an idle machine it runs as fast;
`WTPOOL_VERIFY_PRIORITY=normal` turns it off. The budgets measure at the normal priority, and the
int checks run at it once built, as the clamp's timer coalescing stretches the sleeps they time.

What remains: Mission Control lists the transparent windows as empty places; a run folder below a
folder Spotlight indexes, such as a checkout on the Desktop, is indexed (the lead's clones in
`~/.cache/` are not); the cc part talks to the Claude API, and a first run downloads VS Code, npm
packages, crates or Playwright's Chromium.

## Portability

The harness is tested on macOS only, but assumes no machine of its own. It calls no OS-specific
tool but, on macOS, `lsappinfo` for the focus guard (Isolation), `osascript` for the pasteboard's
change count and `taskpolicy` for the priority (No side effects): process and load information, as
for `machine_busy`, comes through the `sysinfo` crate.
These variables adapt it to a machine:

| Variable | Without it | What it names |
|---|---|---|
| `WTPOOL_VERIFY_VSCODE`, `WTPOOL_VERIFY_VSCODE_CLI` | the platform's default install location, else a download of the pinned version through `@vscode/test-electron` | the VS Code binary and its `code` CLI |
| `WTPOOL_VERIFY_WRAPPER` | heavy jobs run directly | a program each heavy job, such as a verify part, runs under, given the job's command line |
| `XDG_CACHE_HOME`, `XDG_DATA_HOME` | `~/.cache`, `~/.local/share` | where the harness keeps `wtpool-verify/`, its cache and data folders |
| `WTPOOL_VERIFY_BASE` | the `spec-base` tag, the merge base, the root commit (Test standards) | the base commit |
| `WTPOOL_FOREGROUND` | the pool rules of isolation are skipped | the real pool (Isolation) |
| `WTPOOL_BENCH_SOURCE` | this repository | the repository the bench pool clones (Budgets) |

`~/.cache/wtpool-verify/` and `~/.local/share/wtpool-verify/` in this document stand for those two
folders. The `shot` layer and the bench install Playwright's Chromium on first use.

## Layers

`static`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, ESLint, stylelint,
manifest and doc checks. `unit`: the builder's tests, which must pass and decide nothing else.
`int`: nextest on the `verify` build of `wtpool`. `render`: Vitest and jsdom. `host`:
`@vscode/test-electron` with the fake `wtpool`. `shot`: Playwright on `buildHtml`, four themes,
`axe-core`. `e2e`: a real VS Code with the `.vsix` and the release `wtpool`, one check per user flow. `cc`: below.
`bench`: `cargo xtask bench`. `cover`: line coverage of `unit` and `int` (`cargo llvm-cov`) and of
`render` and `host` (V8).

Phase 0 adds `static`, `unit`, `int`, `cover` and the harness checks. Phase 1 adds `render`,
`host`, `shot`, `e2e`, `bench`, the scenario files, the fake `wtpool` and `verify/themes/` (each
theme's `--vscode-*` values). Phase 2
adds `cc`, the scrubber, the fixtures and `bash-commands.toml`.

The fake `wtpool` (`vscode/test/fake-wtpool.mjs`) replays a scenario, injects stream faults (partial,
oversize and garbage lines, unknown `version`, stall, early exit, slow start, ignored EOF), and logs
argv and stdin close. A check that cannot run here counts as not run.

## Test standards

Unit tests and Rust checks use `rstest` tables of exhaustive cases with no overlap, names
`<sut>__should_<assertion>` (trace list names win), and bare `// Given`, `// When`, `// Then`. Code
follows google.github.io/styleguide/rustguide.html and an existing team's
`engineering-standards.md`, both of which the phase 0 author copies into `verify/standards/`
(sources in `THIRD-PARTY-NOTICES.md`). The coverage target is 100 % of lines. The report lists
every line changed since the base that no oracle layer covers, marking those that only `unit`
covers. The verifier marks each one justified, with the reason, or blocking. The base is the commit
`WTPOOL_VERIFY_BASE` names, else the `spec-base` tag, else the merge base with origin's default branch,
else the root commit. `repo_context__should_be_unchanged` compares with the same base.
`lint__no_suppressions` fails on `eslint-disable`, `stylelint-disable` and any `allow` or `expect`
of an oracle lint, except the `#[expect(clippy::panic)]` for `WTPOOL_VERIFY_PANIC`.

## Scenario catalog

A scenario `verify/scenarios/<name>.json` has `phase`, `covers`, `layers`, `pool`, `steps`
(`hook` with a fixture payload, `git`, `process`, `clock`, `expect`, and from phase 4 `machine`,
`wtpool_toml` and `job`, which runs a real `wtpool job` in a worktree) and `expect`. `int` and `e2e`
run the steps, `render` and `host` replay the expected documents. `clock` steps limit a scenario to
`int` and `render`, and the phase 4 steps to `int`, `render` and `host`.

`catalog__should_match_the_files` fails when a file is missing or extra, its phase or covers
differ, or a worktree and activity below match no row. Every scenario also covers R3 and U1. Pools
hold fg, slots wt1 to wt3 and unmanaged x.

1. `idle_pool` (1: R1, A5): fg `foreground`, wt1 `free`, wt2 `unused`, wt3 `not reusable`.
2. `building_without_agent` (1: S4): wt1 `building`, then wt1 `free`.
3. `git_operations` (1: R2, A5): bisect, rebase, merge → wt1 `not reusable`, wt2 `not reusable`, wt3 `not reusable`.
4. `unknown_git` (1: R2, A5): unreadable index in wt2 and wt3 → wt2 `unknown`, wt3 `not reusable`.
5. `folder_missing` (1: S3): wt3 `missing`.
6. `unmanaged_worktree` (1: R1, S3): x `unused`.
7. `hostile_git_strings` (1: X10): markup, bidi and zero width names → literal.
8. `agents_off` (1: S8): note "agent activity is off".
9. `agent_working_in_slot` (2: H10): Read in wt1 from fg → wt1 `working`, fg `foreground`.
10. `pathless_events_keep_place` (2: H10): pathless events → wt1 `idle`, wt1 `working`.
11. `cwd_changed_moves` (2: H10): CwdChanged to wt3, then back to fg → wt3 `idle`.
12. `foreground_with_agent` (2: H10): fg `working`, x `working`.
13. `permission_wait_and_answer` (2: H13, U3): wt2 `waiting` with chip, then wt2 `working` without.
14. `permission_request_before_pre` (2: H13): PermissionRequest before its Pre, then a Post whose input another hook changed → wt2 `waiting`, then wt2 `working`.
15. `permission_denied` (2: H12, H13): denial in a PostToolBatch → wt2 `waiting`, then wt2 `working`.
16. `permission_denied_then_interrupt` (2: H13): clock +30 s, `idle_prompt` → wt2 `waiting`, then wt2 `idle`. Variant with a prompt instead: wt2 `waiting` until it, then wt2 `working`.
17. `auto_mode_denied` (2: H12): PermissionDenied, a build, clock +180 s → nothing in flight, actor `quiet`, wt1 `building`.
18. `parallel_batch_keeps_waiting` (2: H13): another call ends → wt2 `waiting`.
19. `question_and_plan` (2: H13): wt1 `waiting`, then wt1 `working`, twice.
20. `idle_prompt_is_idle` (2: H13): wt1 `idle`, no chip.
21. `interrupt_mid_tool` (2: H11, H12, A1, A4): wt1 `working`, wt1 `idle` (quiet), and with a build wt1 `working`.
22. `main_waits_for_subagent` (2: A1): 5 minutes → wt1 `working`.
23. `stop_failure` (2: H11): wt1 `idle` with `rate_limit`.
24. `post_before_pre` (2: H12, H24): wt1 `working`, nothing in flight.
25. `several_agents_one_worktree` (2: R4): s1 working, s2 idle, subagent a1 of s1 working → wt2 `working`, three lines, newest first.
26. `subagent_elsewhere` (2: H10, A2): wt2 `working`, then hidden after 16 minutes.
27. `subagent_permission` (2: H13): wt2 `waiting` for the subagent only.
28. `dead_process` (2: H19, A3): process gone or pid reused → wt3 `free`.
29. `new_session_same_process` (2: H19): only the newer session.
30. `session_end_cleanup` (2: H20, H21): Last keeps the time, records and `.lock` files gone after gc.
31. `outside_pool` (2: H6): no file written.
32. `bash_attribution` (2: H9): each command of `verify/fixtures/bash-commands.toml`.
33. `hostile_agent_strings` (2: H15, X10): bidi, zero width, `command:` links, ANSI, 10 KB, newlines → literal.
34. `jobs_queue` (4: S14, S15, A8, W10): limit 1, a session's job runs in wt1, a subagent's `cargo test` waits → wt1 `working`, the subagent `queued: cargo test`, then its job runs.
35. `build_not_queued` (4: S4, A8, U14): `warn`, `make` listed and run in wt2 outside a job → wt2 `building`, `build: make (not queued)`; in a job → `build: make`.
36. `five_agents_one_worktree` (4: R4, U12, U13): four working and one done in wt2 → wt2 `working` with `×5`, then one session ends and wt2 `working` with `×4`.

`bash-commands.toml` lists commands with their worktree and summary, covering every form H9 and
H14 name, heredocs and secrets in arguments.

## User flows

Every flow a user can take in the panel, each with its phase and the IDs it exercises. `e2e` runs
each flow in a real VS Code with the `.vsix` and the release `wtpool`, on a sandbox pool, through the
UI: commands from the palette, clicks and keys on the webview, settings through the settings file.
A check named `flow__<name>` asserts what the user sees and what the action did.
`flows__should_each_have_an_e2e_check` fails when a flow below has no passing `flow__` check of its
phase or earlier, or a `flow__` check names no flow below.

| Flow | Phase | Steps and expected result | IDs |
|---|---|---|---|
| `open_from_palette` | 1 | No tab open, run "Worktrees: Open Status": a tab beside the active group shows `idle_pool`. | X1, R1 |
| `open_again_reveals` | 1 | Tab open in another group, run the command again: that tab is revealed, no second tab. | X1 |
| `reload_restores` | 1 | Select wt2, reload the window: the tab returns with wt2 selected and its detail strip. | X2, U5 |
| `hide_and_show` | 1 | Switch to another tab, change wt1's git state, switch back: wt1 is current at once. A page reloaded while the tab stays visible, as when the tab moves to another editor group, asks for the view with a `ready` message and gets it at once (`move__should_fill_the_table_in_the_new_group`, `ready__should_answer_with_the_view`). | X7 |
| `close_stops_watch` | 1 | Close the tab: no `wtpool` process 1 s later. | X7, B10 |
| `two_windows` | 1 | Open the tab in two windows, each on a folder (R9): each runs one watcher and shows the pool. | X16 |
| `live_update_keeps_place` | 1 | Focus wt3's copy button and select wt2, commit in wt1: wt1 updates, focus and selection stay. | X15, R6 |
| `select_and_deselect` | 1 | Click wt1, then wt2, then wt2 again: the strip follows, then closes. | U5 |
| `every_activity_shows` | 1 | The phase 1 scenarios: each row's icon, word, branch, git text and Last match. | U1, U6, A4, A5 |
| `connection_states` | 1 | Stop the fake stream: "updated N s ago", then "updates paused, retrying in N s", then "live" after it resumes. | X6 |
| `agents_off_note` | 1 | Pool without `wtpool-activity/`: the note names `wtpool setup`, then the plugin's `/plugin marketplace add haiyuechen-nearone/wtpool` and `/plugin install wtpool@wtpool`, and `wtpool hook install`. | S8 |
| `narrow_window` | 1 | 600 px wide: a long branch shows whole, the table scrolls sideways, each action scrolled into view can be hovered and clicked. | U1 |
| `narrow_scroll` | 3 | 600 px wide while wt2 waits: the table, not the page, scrolls sideways; at its end the Last and actions cells are in view and the header has not moved; Tab brings each action wholly into view and each row's start; scrolled to the end, the waiting chip focuses wt2 with its start in view. | U1, U3 |
| `open_in_new_window` | 1 | Click wt1's open action: a new VS Code window opens on wt1. | U4 |
| `open_terminal` | 1 | Click wt1's terminal action: a terminal opens with wt1 as its folder. | U4 |
| `copy_path` | 1 | Click wt1's copy action: the clipboard holds wt1's path. | U4 |
| `missing_worktree_actions` | 1 | `folder_missing`: wt3 offers copy path only. | U4, S3 |
| `keyboard_only` | 1 | Without a mouse: Tab reaches each row and action, Enter runs it, focus stays visible. From phase 4 Tab reaches the machine button, Enter opens the strip with focus on the Limit field, Tab goes through the field, the Policy menu, Save, Use defaults and the pattern editor (one tab stop per list of chips, then its input and Add, the suggestions, then Add all) to the rows, and Escape closes the strip and refocuses the button. | X14, X20, X22 |
| `theme_switch` | 1 | Switch light, dark, high contrast light and dark: colours follow each theme, no reload. | X13, U6 |
| `reduced_motion` | 1 | With reduce motion on: no animation runs. From phase 4 a line icon never moves, with reduce motion or without (U13). | X14, U13 |
| `wait_appears` | 2 | An agent asks permission in wt2: the row shows `waiting`, the chip counts it, the title reads `Worktrees (1)`, the live region announces it, no popup. | H13, U3, X18, D3 |
| `waiting_chip_focuses` | 2 | Click the waiting chip: focus moves to the first waiting row. | U3 |
| `wait_answered` | 2 | Answer the permission: the row returns to `working` and the title to `Worktrees`. | H13, X18 |
| `free_confirm` | 3 | Free an idle slot: a modal names worktree and branch, warns for `idle`, confirm frees it. | X12 |
| `free_cancel` | 3 | Cancel the same dialog: nothing changes. | X12 |
| `free_refused` | 3 | Free a slot with a working agent: refused, nothing changes. | X12 |
| `fg_confirm` | 3 | Bring a slot's branch to the foreground: a modal names it, confirm runs `wtpool fg`. | X12 |
| `workspace_folder_pool` | 1 | Without `WTPOOL_FOREGROUND` or `WTPOOL_SLOT_*`, a sandbox `HOME` only, open VS Code on the main worktree `app` of a pool with slots `app-wt1` to `app-wt3` and no `wtpool.toml`, then on `app-wt2`: each tab shows that pool, `app` its foreground. | R9 |
| `no_folder_open` | 1 | A window with no folder: the tab says "Open a folder inside a git repository to see its worktrees." | R9 |
| `wtpool_error_shown` | 1 | VS Code on a folder in no git repository: the tab shows the last stderr line of wtpool next to "updates paused, retrying in N s", wrapped and whole in a tab as narrow as the window allows. | R10, U1 |
| `live_output` | 3 | An agent's background task writes to its output file in wt2: select wt2, the strip's Output section shows the tail and new lines within 1 s; Follow opens a terminal that shows new lines. | H27, S13, R12, U9, X19 |
| `jobs_open_from_heading` | 4 | The heading reads `<repository> · <machine> · jobs 4/4 · 2 queued · 1 not queued` with a chevron; click the machine part, or Tab to it and press Enter or Space: the strip opens under the heading, focus moves to the Limit field, the chevron points up; Escape in the strip, or the button again, closes it and focus returns to the button. | U10, U11, X20 |
| `jobs_running_and_queued` | 4 | Open the strip while 4 jobs run and 2 wait: Running lists 4, Queued 2 with their positions, each with its summary or label, worktree and agent or `outside this pool`, and age; the waiting agent reads `<label> · queued: cargo test` and its worktree `working`; a running job ends: within 1 s queued job 1 moves to Running, its agent line shows its own action, focus stays. | S14, S15, A8, W10, U11, J10 |
| `jobs_change_settings` | 4 | Type 6 in the Limit field, choose `require`, press Enter or Save: `wtpool job limit 6`, then `wtpool job enforce require`, run once each, the strip says `Saved.` and the live region announces it; within 1 s the heading reads `jobs 6/6` and the 2 queued jobs run; Use defaults runs `wtpool job limit default` and `wtpool job enforce default`: the defaults and `warn` again. | J3, J6, J11, X11, X21, W10 |
| `jobs_lower_limit` | 4 | 4 jobs run and 2 wait, set 2: no job stops, the strip says `4 running, over the limit of 2: they finish, and queued jobs start once fewer than 2 run.`; two jobs end and none starts; a third ends and queued job 1 starts. | J11, U11 |
| `jobs_not_queued` | 4 | Under `warn`, with `make` in this repository's list, run `make` in wt2 outside a job: within 1 s wt2 reads `building` with `build: make (not queued)`, ` (not queued)` in the attention colour, the heading adds `· 1 not queued`, the strip lists it under Not queued; run it as `wtpool job -- make`: the line reads `build: make` and the counts drop. | S4, A8, U14, J21 |
| `jobs_require_refuses` | 4 | Under `require`, with `cargo test` in this repository's list, an agent runs `cd <wt1> && cargo test` through Bash: `wtpool hook policy` denies it with `wtpool: cargo test is heavy on this machine (enforce = require). Run it as: cd <wt1> && wtpool job -- cargo test`, nothing runs; the agent runs that command: no refusal, the job queues, its line reads `queued: cargo test`. | J21, H29 |
| `jobs_first_open` | 4 | A repository with no list, holding `Makefile`, `package.json` with scripts `build`, `test`, `test:e2e` and `start`, and `pnpm-lock.yaml`: the heading reads `<repository> · <machine> · jobs 0/<limit> · 7 suggested`, the last part muted; both lists read `none`; the suggestions list `make` from `Makefile` and the six pnpm commands from `package.json` scripts, no `start`; under `warn` nothing is flagged; Enter on `make` runs `wtpool job heavy add --repo -- make`, the chip moves to This repository, `· 7 suggested` goes; `Add all` adds the six shown, the live region says `Added 6 suggestions.`, the group goes. | J3, J20, J23, J24, S14, U15, X11, X21, X22 |
| `jobs_edit_patterns` | 4 | Tab to This repository's chips, arrow to `protoc` (tagged `build_processes`), Delete: `wtpool job heavy rm --repo -- protoc` runs, the chip goes within 1 s, `Removed protoc.` is announced, focus moves to the next chip; type `cargo xtask verify` and Enter: it is added, `.git/wtpool.toml` keeps its other keys and comments; `make *` and a listed pattern say why under the input and send nothing; `just` in This machine's input runs `wtpool job heavy add -- just`. | J3, J6, J18, U15, X11, X21, X22 |
| `jobs_invalid_value` | 4 | Type 0, 65, 2.5, `four` or nothing and press Enter: the field is marked invalid, `Enter a whole number from 1 to 64.` shows under it and is announced, nothing is sent, focus stays; type 6 and Enter while wtpool refuses (a symlinked `machine.toml`): the strip shows its message and 6 stays in the field. | X20, X21, J6 |
| `jobs_old_wtpool` | 4 | The tab runs a wtpool without `wtpool job`, so the document has no `machine.jobs`: the heading reads the machine name with the chevron; opening it shows `This wtpool has no heavy job limit. Update wtpool to a version with wtpool job to set one.` | S14, U10, U11 |
| `jobs_wtpool_missing` | 4 | With the strip open, `wtpool` leaves `PATH`: the header shows R10's error and `updates paused`, the last document stays; Save shows `wtpool did not start: spawn wtpool ENOENT` and nothing else changes. | R10, X21 |
| `jobs_remote_window` | 4 | A window whose first folder is not local: wtpool runs on this computer and the heading names it; the strip adds `This window's folder is not on this computer: the settings are for this computer, <machine>.` | R9, X3, U11 |
| `several_agents_in_one_row` | 4 | Five agents in wt2, four working and one done: the row shows `×5` after `working`, on the word's line, in its colour inside a 1 px border, read as `working, 5 agents`; each agent line starts with its state's static icon and a hidden `<state>: `; one agent leaves: `×4`; a row with one agent line shows no count. | R4, U12, U13 |

## Trace list

`trace_complete --phase N` fails when an ID of phase N or earlier has a check that did not pass, or
none, and when a harness check of phase N or earlier did not pass. An ID's checks are the scenarios
covering it plus its entry below, where a named ID stands for its own checks, from its own phase.
IDs are phase 1, except phase 2 for R4, D5, H1 to H26, A1 to A4, A7, S5, S6, W6, X18, and phase 3
for X12, H27, H28, S13, R12, U9 and X19, and phase 4 for H31 and U17, as for the IDs of [jobs.md](../jobs/jobs.md). A scenario or budget check runs from its own phase if later.
Without `--contract`, a `cc` check counts by the live result for the installed Claude Code version,
which lies beside the recordings and is hashed into the lock (Contract test). On a machine without
the recordings folder, it counts by replay alone, and the report says so. The visual review counts
only for done.

R1 `rows__should_show_name_role_and_branch`. R2 `status__should_report_the_facts_wtpool_ls_reports`, and from phase 3 the `int` checks `ls__should_show_controls_bidi_and_zero_width_as_escapes`, `bg__should_show_controls_in_the_fetching_line_as_escapes`, `fg__should_show_controls_in_the_commit_subject_as_escapes` and `config__should_show_controls_in_paths_as_escapes` (a branch in the `ls` table and the warning that names a gone worktree, the fetching line of `wtpool bg`, the commit subject `wtpool fg` prints and the `slot_dir` that `wtpool config` prints, each showing X10's characters as `\u{XXXX}`, with none raw on stdout or stderr). R3 `legend__should_cover_every_activity_value`. R6 `budget__b4`. R7 `budget__b1`, `budget__b2`, `budget__b5`, `budget__b10`. R8 S2, X4, X10, X11, X12, X17. R9 `flow__workspace_folder_pool`, `flow__no_folder_open`, `spawn__should_run_in_the_workspace_folder` (from phase 3: the watch, `free` and `fg`, with only the inherited `WTPOOL_FOREGROUND` and `WTPOOL_SLOT_DIR`). R10 `flow__wtpool_error_shown`. R11 `stream__should_accept_unknown_properties` (an unknown property on every object of a document with actors, actions and a build). R12 `tail__should_read_only_the_selected_row`, `tail__should_update_a_growing_file_within_a_second` (new lines within 1 s, the last 64 KiB and 15 lines, ANSI removed, invalid UTF-8 replaced, tabs as spaces to the next multiple of 8 columns), `tail__should_refuse_symlinks_and_other_files`, `tail__should_never_read_an_unlisted_path` (`select` and `follow` with ids the document does not list), `tail__should_stop_on_deselect_and_hide`, `tail__should_poll_a_file_it_cannot_watch` (a watch that cannot be made or that fails), `tail__should_read_a_change_made_during_a_read` (one read of a file at a time), `flow__live_output`.

D1 `manifest__should_contribute_no_views`. D2 `stream__should_spawn_one_watch_and_never_poll` (60 s, exactly one `--watch` spawn, no `--json`). D3 `manifest__should_contribute_no_status_bar_item`. D5 H22, H28.

H1 `install__should_match_the_event_table`, `install__should_refuse_a_binary_under_target`. H2 `hook__should_print_nothing_for_every_fixture`, `cc_no_hook_output`. H3 `hook__should_exit_zero_for_every_fixture`, `hook__should_exit_zero_after_a_panic`. H4 `hook__should_exit_within_a_second_when_the_record_is_a_fifo`, `hook__should_exit_within_a_second_when_stdin_stays_open`. H5 `hook__should_ignore_input_past_32_mib`, `hook__should_write_nothing_for_malformed_json`. H6 `hook__should_not_create_wtpool_activity`, `hook__should_refuse_a_symlinked_wtpool_activity`, `hook__should_skip_a_sandbox_pool_without_the_variable`, `hook__should_write_only_inside_wtpool_activity` (tree snapshot around the fixture corpus), `setup__should_create_wtpool_activity` (`wtpool setup` and each pool changing command). H7 `placement__should_compare_paths_by_component`, `hook__should_refuse_a_forged_dot_git_file`, `placement__should_use_the_nearest_existing_ancestor`, `hook__should_run_no_git` (PATH without git). H8 `placement__should_read_the_path_field_of_each_tool`. H9 `bash_paths__should_match_the_table`. H10 `cc_read_edit_bash`, `cc_subagent_elsewhere`, `cc_cd_into_slot`. H11 `transitions__should_match_the_event_table`, `cc_stop_failure`. H12 `in_flight__should_clear_on_turn_end`, `in_flight__should_keep_at_most_32`, `auto_mode_denied`. H13 `wait__should_hash_input_with_sorted_keys`, `wait__should_wait_on_a_question_still_in_flight`, `cc_permission`, `cc_permission_deny`, `cc_permission_deny_interrupt`, `cc_subagent_permission`, `cc_idle_prompt`. H14 `summary__should_match_the_table` (`export GH_TOKEN=…`, `TOKEN=x cargo`, `curl https://user:pass@host`, `timeout 600 cargo test`). H15 `sanitize__should_strip_controls_bidi_and_zero_width`, `hook__should_store_no_prompt_text`. H16 `labels__should_prefer_session_title`, `labels__should_use_agent_type`, `labels__should_follow_the_transcript_title`, `labels__should_use_the_description_of_the_agent_call`. H17 `hook__should_refuse_ids_outside_the_pattern` (`../x`, 65 characters, Unicode, empty). H18 `hook__should_not_follow_a_symlinked_record`, `hook__should_keep_records_whole_under_parallel_writers` (16 writers), `hook__should_keep_a_record_it_can_read` (a call that brings the record past 16 KiB is dropped from `in_flight`, and an event whose record cannot fit leaves it as it was and says so in the hook log). H19 `liveness__should_record_the_parent_process`, `liveness__should_keep_the_record_when_reparented`, `cc_records_claude_pid`. H20 `cc_session_end`. H21 `gc__should_delete_only_old_dead_or_ended_records`, `gc__should_delete_done_subagents_but_the_newest`, `gc__should_delete_the_lock_with_the_record`, and from phase 3 `gc__should_run_once_a_day_on_session_end` (a session's end without a stamp and 2 days later, with a stamp 6 days old that deletes the records over a week old, with a stamp an hour old that deletes nothing, then `wtpool hook gc` on that fresh stamp, each gc stamping its time). H22 `install__should_never_write_settings`, `install__should_report_presence`, `install__should_print_only_the_block_on_stdout`, and from phase 3 `install__should_name_what_is_missing` (no file, not valid JSON, no hook, two events missing, another `wtpool` for one event and for every event). H23 `hook__should_cap_its_log`, `hook__should_not_log_into_watched_folders`. H24 `merge__should_refuse_an_older_state`, `hook__should_lose_no_update_under_parallel_writers` (8 writers, 10 rounds), `hook__should_restart_when_gc_deleted_the_lock`. H25 `hook__should_tag_records_with_the_harness`. H26 `hook__should_give_each_call_its_kind` (in flight and ended, an MCP tool among them), `status__should_read_kinds_only` (an unknown tool with kind `shell` or `agent`, `Bash` and `Agent` with `other`, and calls an older wtpool wrote without a kind), `status__should_read_the_kind_of_an_older_call` (`int`: a `Bash` call without a kind during a build, and the same call of a harness without an adapter). H27 `hook__should_record_background_tasks` (the path with and without `CLAUDE_CODE_TMPDIR`, an id, folder name or transcript name outside its pattern, `..`, at most 5 tasks, the ends by Stop, TaskStop and SessionEnd, an output file that is a FIFO never opened), `hook__should_record_a_resumed_sessions_task_where_claude_code_writes_it` (`int`: the file under another session folder of the project, the path the record keeps), `hook__should_end_a_task_through_its_session` (`int`: the session's next Stop ends a subagent's task that its last SubagentStop listed, ends a task it lists as completed and keeps one it lists as running), `hook__should_end_no_task_of_a_record_it_cannot_lock` (`int`). H28 `plugin__should_hold_the_printed_block`, `install__should_report_the_plugin` (enabled with and without `wtpool` on `PATH`, with the whole block and with part of it, disabled, and a plugin of another marketplace). H31, from phase 4, `hook_policy__should_refuse_unbounded_background_waits` (`int`: the cases of the verification plan of [jobs.md](../jobs/jobs.md) at each `enforce`, each refusal with its fixed reason, a wait that H29 also refuses with H31's reason alone, and no decision for a command it cannot read or a panic) and `hook_policy__should_read_bounded_waits_from_the_repository` (`int`: `false` allows every refused wait, also from a linked worktree, `true` and no key refuse them, as does another repository without the key, and a string value, a `wtpool.toml` that is no TOML and a `cwd` outside any repository give no decision).

A1 `decay__should_match_the_table`, `cc_interrupt`, `builds__should_keep_the_session_that_started_them_working`, `builds__should_keep_no_session_working_through_other_children`, `background__should_keep_a_session_working_until_its_task_ends`, `background__should_keep_a_subagent_working_until_its_task_ends`. A2 `stop_failure__should_finish_a_subagent_and_keep_a_session_idle`, `agent_line__should_read_done_with_its_api_error`. A4 `activity__should_follow_the_precedence`. A6 `last__should_use_the_newest_known_time`. A7 `status__should_ignore_a_record_outside_the_pool`.

S1 `status__should_match_the_schema`, `status__should_print_one_pool`. S2 `status__should_leave_every_file_unchanged` (mtime and size snapshot), `status__should_not_start_fsmonitor` (`core.fsmonitor=true` in the sandbox), `status__should_never_create_index_lock`, `lint__no_socket_apis` (clippy `disallowed_types` for `std::net`, no networking crate in `cargo tree`). S4 `builds__should_match_cargo_star_and_rustc`, `builds__should_read_the_setting`. S5 `status__should_skip_fifos_symlinks_locks_and_large_records`, `status__should_keep_the_newest_512_records`. S6 `status__should_skip_an_unknown_record_version`. S7 `schema__generated_files_current`, `validator__should_accept_every_scenario_document`. S9 `free__should_agree_with_wtpool_bg`. S10 `status__should_name_the_repository` (each URL form with and without `.git`, other hosts, `WTPOOL_REMOTE`, from a linked worktree, no remote), and from phase 3 the `int` checks `bg__should_take_the_pr_head_from_gh`, `bg__should_park_the_head_branch_of_the_pr_gh_names`, `bg__should_name_the_failure_of_gh`, `bg__should_say_gh_is_missing` and `bg__should_refuse_a_pr_without_a_head_to_use` (a pull request through `gh` on the configured remote's repository, and each refusal). S11 `status__should_give_each_actor_its_harness` (from phase 2, also for a record without one). S12 `schema__should_be_published_by_wtpool_status` (`verify/schema/` but for annotations, a one-line description per property, no closed object, the example's schema), `status__should_emit_only_listed_properties` (from phase 2: `verify/schema/` with every object closed). S13 `status__should_list_outputs` (running tasks newest first, then the one that ended last, omitted when empty, a path with `..` left out, against `verify/schema/`).

W1 `watch__should_print_a_document_at_once`, `watch__should_print_alive_lines`. W2 `watch__should_see_head_and_index_after_many_renames` (100 commits), `watch__should_see_nested_branch_refs`, `watch__should_see_packed_refs`, `watch__should_see_a_fetch`. W3 `watch__should_ignore_lock_and_temp_files`. W4 `watch__should_recompute_git_after_a_change`, `watch__should_read_only_the_changed_record`, and from phase 2 `watch__should_refresh_git_after_an_edit_or_bash_call`. W5 `watch__should_merge_a_burst_into_one_document`, `watch__should_print_only_changes`. W6 `watch__should_turn_quiet_on_time`. W7 `watch__should_show_a_missed_change_within_one_heartbeat`, `watch__should_recompute_on_rescan`. W8 `watch__should_exit_on_stdin_eof`, `watch__should_exit_zero_on_a_closed_pipe`. W9 `watch__should_stay_quiet_on_an_idle_pool`.

X1 `open__should_reveal_or_create_beside`, `tab__should_show_the_extension_icon`. X2 `restore__should_rebuild_after_reload`, `restore__should_ignore_hostile_state`, `tab__should_show_the_extension_icon`. X3 `manifest__should_be_a_ui_extension`. X4 `manifest__should_scope_wtpool_path_to_application`, `spawn__should_use_the_setting_and_no_shell`. X5 `partial_lines`, `oversize_line`, `garbage_line`, `unknown_version`, `early_exit`, `slow_start`, `stream__should_survive_2000_documents`. X6 `stall_30s`, `header__should_show_live_updated_and_paused`, `header__should_show_paused_before_the_first_document`, `stream__should_post_paused_before_the_first_document`, and from phase 3 `header__should_show_starting_before_the_first_line` and `stream__should_post_starting_before_the_first_line` (a tab whose wtpool waits before its first line). X7 `hidden_then_shown`. X8 `csp__should_report_no_violation`, `csp__should_match_the_template`, `e2e_csp_clean`. X9 `options__should_limit_roots_and_disable_commands_and_forms`. X10 `lint__no_html_sinks`, `render__should_escape_and_isolate_controls`. X11 `messages__should_ignore_everything_outside_the_set` (unknown type, extra field, unknown id, a `path` field, `__proto__`, oversized), and from phase 3 `tail__should_never_read_an_unlisted_path`. X12 `actions__should_confirm_naming_the_worktree`, `actions__should_refuse_free_with_a_working_actor`, `actions__should_warn_free_with_an_idle_actor`, `actions__should_guard_fg_by_the_holding_slot`, and the `int` checks of the CLI's own guard `free__should_refuse_while_an_agent_works_or_waits`, `fg__should_refuse_while_an_agent_works_or_waits`, `force__should_skip_the_agent_guard`, `guard__should_never_refuse_for_idle_done_or_elsewhere_agents`, `free__should_name_an_agent_by_its_escaped_label_else_its_key` and `guard__should_skip_only_the_agents_of_the_calling_session`. X13 `lint__no_colour_literals`, `shot__should_render_every_theme`. X14 `axe__should_report_no_violation`, `keyboard__should_reach_every_row_and_action`, `live_region__should_announce_only_waiting`, `motion__should_stop_the_spinner`, `focus__should_show_every_ring_whole_in_contrast`. X15 `update__should_keep_focus_selection_and_detail`, and from phase 3 `detail__should_update_while_open` and `detail__should_post_each_document_while_open` (the open strip renders its facts, actors and Output section again for each new document). X16 `windows__should_run_one_watcher_each`. X17 `lint__no_network_modules`, `bundle__should_contain_no_listen_call`, `e2e_no_listening_sockets`. X18 in `host`: `title__should_read_worktrees_without_waits` (`idle_pool`), `title__should_count_waits_while_hidden`, `title__should_drop_the_count_when_a_wait_ends` (both `permission_wait_and_answer`). X19 `follow__should_stream_the_file_into_a_terminal` (the terminal's name, the last 40 lines, new lines, tabs as spaces in both, also across reads, a refused file, no read after close), `follow__should_poll_a_file_it_cannot_watch`, `follow__should_read_one_gain_at_a_time`, `follow__should_stop_at_a_file_replaced_by_a_symlink`, `flow__live_output`.

U1 `rows__should_follow_the_grammar`, `ages__should_use_the_format`, `layout__should_clip_no_text`, `tags__should_show_whole_after_the_name`, `scroll__should_scroll_the_table_inside_the_tab`, `scroll__should_bring_focus_and_waits_into_view`. U2 `order__should_not_change_when_states_change`. U3 `chips__should_count_and_scroll_to_waiting`, `scroll__should_bring_focus_and_waits_into_view`. U4 `actions__should_match_the_row_kind`, `actions__should_not_cover_another_column`, `actions__should_centre_their_glyphs`. U5 `detail__should_show_every_fact_and_actor`, and from phase 3 `detail__should_update_while_open` and `detail__should_post_each_document_while_open`. U6 `legend__should_cover_every_activity_value`, `icons__should_share_one_size`, `contrast__should_meet_aa_in_every_theme`, `focus__should_show_every_ring_whole_in_contrast`, `hover__should_show_in_every_theme`, `chips__should_keep_their_outline_in_every_theme`, `detail__should_stand_out_from_the_page`. U7 `ages__should_tick_without_messages`. U8 `heading__should_name_the_repository_then_the_machine` (`render`, a second pool in the document ignored; `e2e` with a GitHub and a local `origin`). U9 `output__should_show_each_task_tail`, `output__should_follow_the_end_unless_scrolled_up` and `output__should_drop_what_the_document_no_longer_lists` (`render`), `output__should_clip_nothing_in_every_theme` and `output__should_show_the_follow_ring_whole` (`shot`), `flow__live_output`. U17, from phase 4, `run_time__should_use_the_format` (each unit boundary: 59 s, 1 m, 59 m, 1 h, `1 h` for 1 h 0 m, 1 h 1 m, 47 h 59 m, `2 d` for 48 h, 2 d 5 h, and `0 s` for a `started` ahead of the clock, on the agent line as a screen reader reads it and in the Output heading), `run_time__should_tick_each_second` (on the fake clock, with no message), `run_time__should_follow_the_named_task` (the first listed task's, each running task's own, none for an ended task or a line whose action is not `background:`) and `run_time__should_leave_names_and_announcements_unchanged` (the Follow message, the accessible names of the Follow button and the tail, the live region) (`render`), `run_time__should_clip_nothing_in_every_theme` (`shot`), and `follow__should_stream_the_file_into_a_terminal` (`host`: the terminal's name). The checks of status-panel.md revision 14 (layout, tags, scroll, glyphs, icons, contrast, focus, hover, chips, detail, the tab icon) and `actions__should_not_cover_another_column` run from phase 3.

B1 to B11 `budget__b1` to `budget__b11`.

Harness checks carry no ID and run from phase 0, except where noted: `oracle_unchanged`, `repo_context__should_be_unchanged` (the repository's `CLAUDE.md`, `AGENTS.md` and `.claude/` equal the base), `lint__no_suppressions`, `catalog__should_match_the_files` (phase 1), `flows__should_each_have_an_e2e_check` (phase 1),
`trace_complete`, `isolation__left_no_trace`, `release__should_have_no_verify_feature`, `version__should_list_version_commit_and_features`, `fixtures__should_match_a_fresh_scrub` (phase 2, `skipped` where it has no recordings to scrub), `contract__should_run_in_the_sandbox` (phase 2: one auto mode turn that loads only the run's settings, `skipped` on a machine without the recordings folder),
and the harness's own `harness:self_test` (its unit tests), `harness:fmt`, `harness:clippy` and `harness:rustdoc` (`cargo fmt --check`, clippy and rustdoc with `-D warnings` over `xtask/`).

## Contract test

`cc` drives the installed `claude` through the Agent SDK, model `claude-haiku-5-5`, on your
existing login: no user, project or local settings, only the run's `--settings` file, and its
configuration in the run folder (No side effects). Its settings hold the block of a build copy in `~/.local/share/wtpool-verify/bin/`, a recorder hook,
`WTPOOL_VERIFY_SANDBOX=1`, `defaultMode` `default`, the ask rule `Bash(git status)`, deny rules
for WebFetch and WebSearch, and Claude Code's sandbox on: Bash writes only in the session's
working directories, all in the run folder, and Claude Code's temporary folder, reaches no host
but localhost (`network.strictAllowlist`), never runs outside the sandbox, asks as it would unsandboxed
(`autoAllowBashIfSandboxed` off), and a sandbox that cannot start stops the session
(`failIfUnavailable`). Sessions get the slots as `additionalDirectories`. `canUseTool` allows
only the session's scripted calls, each tool with its command or path, and denies any other with
a message that names it. Of those, it holds the dialog of a permission session, then allows or
denies, and allows at once elsewhere. Calls Claude Code allows without asking, reads and
read-only commands inside the pool and the Agent tool, never reach it. The contract turn runs in
auto mode with no tools.

| Session | Script | Claude Code asserts | wtpool asserts | Recorded |
|---|---|---|---|---|
| `cc_read_edit_bash` | Read, Edit, `ls` in wt1 from fg | each event has its H8 path field | on wt1, `idle` after Stop | |
| `cc_subagent_elsewhere` | a subagent reads in wt2 | `agent_id`, `agent_type` | a1 on wt2 | the Agent `description` |
| `cc_subagent_permission` | a subagent runs `cd {wt3} && git status`, held 10 s | its PermissionRequest has `agent_id` | only a1 waits | `permission_prompt` |
| `cc_cd_into_slot` | `cd {wt2} && ls`, then `pwd` | | s1 on wt2 | CwdChanged, later `cwd` |
| `cc_permission` | `cd {wt3} && git status`, held 70 s, allowed | PermissionRequest during the hold | `waiting` all along | event order |
| `cc_permission_deny` | the same, denied after 5 s | a PostToolBatch lists the call | the wait ends there | PostToolUseFailure |
| `cc_permission_deny_interrupt` | the same, denied with `interrupt: true`, 70 s idle | no Stop in 10 s | as `permission_denied_then_interrupt` | `idle_prompt` |
| `cc_interrupt` | `{wt2}/.verify/block.sh` on a held FIFO, `interrupt()` | no Stop in 10 s | `quiet` after 125 s | PostToolUseFailure, `is_interrupt` |
| `cc_idle_prompt` | one turn, 70 s idle | | `idle`, never `waiting` | `idle_prompt` |
| `cc_stop_failure` | an unknown model | StopFailure or a refusal | `idle` with the error | which one |
| `cc_session_end` | end a session with a subagent | SessionEnd | ended, subagent gone | |
| `cc_no_hook_output`, `cc_records_claude_pid` | all sessions | the hook's parent is `claude` | no hook output, pid recorded | |

Tool calls that differ from the script rerun a session twice at most, then it is `inconclusive`. A
failed Claude Code assert is `assumption_broken`, a failed wtpool assert a fail. Records never fail. No session covers PermissionDenied: Claude Code 2.1.306's auto mode allowed
the scripted force push, so the scenario `auto_mode_denied` with a hand-written payload is its only check.

The oracle step's live run keeps raw recordings in `~/.local/share/wtpool-verify/recordings/`, hashed
into the lock, and its live result, `claude-<version>/cc-results.json`, beside them, hashed too.
The frozen scrubber turns them into `verify/fixtures/claude-<version>/`, and
`fixtures__should_match_a_fresh_scrub` redoes it. Every `int` run replays them as
`cc_replay:<name>`. That live run is the only one per Claude Code version: freeze judges the new
`cc` checks from its results instead of starting sessions, and a clean verify passes `--contract`
only when the installed Claude Code version has no live result. Later live runs record into their
run folder, outside the lock. The harness supports the latest installed Claude Code alone: a
`--record` run in which every `cc` check passes moves the recordings of each other version into
`recordings-backup/` beside them, so the next freeze locks the new version's recordings only.

A machine without that recordings folder, such as a fresh clone elsewhere, has no live result to
count. There `fixtures__should_match_a_fresh_scrub` still fails on a fixture that comes from no
locked recording, and is otherwise `skipped`. The `cc` checks count by replay alone, with or
without the `cc` part: each session by this run's `cc_replay:<name>`, `cc_no_hook_output` by every
session's replay, and `contract__should_run_in_the_sandbox` is `skipped`. The report says the `cc`
checks were judged by replay alone. A machine with recordings still needs the live result of its
Claude Code version.

## Budgets

Load models one wtpool instance with fewer than 10 worktrees.

The bench pool clones `WTPOOL_BENCH_SOURCE`, by default this repository, with 4 worktrees: the
foreground on the source's branch, or on a new `bench` branch when the source's HEAD is detached,
as in a frozen or CI checkout. Phases are in brackets.

1. B1 (2): `wtpool hook` p95 ≤ 5 ms, max ≤ 30 ms, over 1000 fixture runs.
2. B2 (2): the five async events of H1 are async in the printed block.
3. B3 (1): `wtpool status --json` p95 ≤ 200 ms.
4. B4 (1): p95 commit to document ≤ 1 s, missed change ≤ 11 s, and from phase 2 record write ≤ 300 ms.
5. B5 (1): over 60 s with no file or hook event, the watcher starts git only on its 10 s heartbeat,
   at most 6 rounds, each starting no more git processes than the first document did, and prints only
   `alive` lines. A logging `git` first on `PATH` counts the starts. Nothing measures CPU time.
6. B6 (2): while 3 agents write 6 records a second for 30 s, the watcher reads each changed record
   once per change, recomputes git only for worktrees whose git files changed, and prints at most 5
   documents a second (W5). Counted like B5, never by CPU time.
7. B7 (1): no `postMessage` while hidden or for an alive line that leaves the connection
   unchanged, a tick changes only ages.
8. B8 (1): heap ≤ 15 MB after 1000 documents, growing ≤ 1 MB after 500.
9. B9 (2): ≤ 64 KiB per document for 10 worktrees of 5 actors.
10. B10 (1): no `wtpool` process 1 s after close.
11. B11 (2): ≤ 512 records after a week of 200 subagents a day with daily gc.

When the load passes half the cores, a build runs outside the sandbox, or another verify part is
under way, a budget waits up to 10 minutes, then reports `machine_busy`. The budgets hold
`~/.cache/wtpool-verify/verify.lock` alone while they measure, and every other part and a run's shared
builds hold it shared, so no other part starts until the budgets end. A p95 budget stops once
its p95 can no longer pass and reports the samples it took, so it fails within the time a check may
take. No budget is ever raised.

## Outcomes and limits

A check ends `pass`, `fail`, `skipped` or needs you with a code: `logged_out`, `no_network`,
`machine_busy`, `inconclusive`, `assumption_broken`, `oracle_change`. `skipped` means the check's
subject does not exist on this machine, such as the recordings of a live run. It fails no verdict,
and the report lists it with its reason under "Skipped on this machine". The verdict is FAIL on any
fail, else NEEDS YOU on any code, else PASS.

A check may take 60 s in `static`, `unit` and `render`, 120 s in `int` and `bench`, 180 s in
`host` and `shot`, 300 s in `e2e`, 10 minutes per `cc` session, or its own length (fake clock
included) plus 60 s. A part stops after 1 hour. `host`, `shot`, `e2e` and `cc` (for drift) retry
twice. A timeout fails, and so does a check flaky in 3 runs in a row.

## The report

`cargo xtask verify --phase N --part <part> [--only <check>]` runs every check of phase N or
earlier in one part, without stopping at a failure. The parts are `quick` (`static`, `render`,
`host`), `int` (`unit`, `int`), `ui` (`shot`, `e2e`), `bench`, and `cc`, which runs only
with `--contract`. `cargo xtask verify --phase N --report` then computes `cover`, runs `trace_complete`
and `isolation__left_no_trace` over the run's parts, and writes `verify/out/<run>/report.md` and
`report.html`. They open with the verdict, commit, the base and its source (Test standards), the
Claude Code version and how the `cc` checks were judged (by the `cc` part, or by replay alone),
then needs you codes with what to do, the checks skipped on this machine with their
reasons, oracle, `verify/accepted.txt` and spec diffs, failures, flaky checks, budgets, coverage
and each check's result.

Test once. Each part's result records a hash of its inputs: the oracle files, the files the part
reads (`crates/` and the Cargo files for `int` and `bench`; those plus `vscode/` for
`ui`, which runs the release `wtpool`; every tracked file outside `docs/` and `verify/reviews/` for
`quick`, whose `static` lints them all), and the versions of the Rust, Node, VS Code and Claude
Code it ran with. A part whose hash equals that of an earlier passing
result of this phase is not run again: the run copies that result and names its commit. A check
`skipped` for want of the recordings folder counts as passing here while that folder is still
missing, as the folder is no input of the hash; any other skip does not. `--fresh`
runs every part. The report lists each reused part with its source commit and hash, and the
verdict covers the run as assembled.

## Canaries

The author may still ship canaries in `verify/canaries/` for harness checks that cannot fail
otherwise. Nothing requires a count of them, and the verifier writes none. A canary is caught when
its check fails on an assertion, or ends with the needs you code its `Expect:` line names.

## Visual review

From phase 1 the visual review shoots `idle_pool`, and from phase 2 also
`permission_wait_and_answer` and `several_agents_one_worktree`, in `e2e` and in `shot` with four
themes at 1100 px. At 600 px it shoots `permission_wait_and_answer`, or `idle_pool` in phase 1.
From phase 3 it also shoots `every_activity` with the wt2 slot row focused, in Dark Modern at
1100 px and 600 px, so shown row actions stay in view.
Once you approve a set, 0.1 % changed pixels fail.

## Done

Phase N is done at commit C when the verify run of C says PASS, `cover` included,
with any reused part matching C's input hash, the one verifier review of C ends `verdict: clean`,
and in phase 2 the `cc` checks count by a live result for the installed Claude Code version.
Commits after C touch only `verify/reviews/`.

## What needs you

1. Approve both docs and the mockups. That commit is `spec-base`.
2. Run each phase's install commands. Phase 0 replaces the bash `wtpool`.
3. After phase 2, run `wtpool setup` in the foreground worktree to create `wtpool-activity/`, and add
   the block `wtpool hook install` prints to Claude Code's `settings.json` (H22). The hook runs
   gc itself (H21).
4. Leave the VS Code windows of `host` and `e2e` alone: on macOS they are transparent and never
   take focus (Isolation). Name a time window if they may not open at all.
5. Answer each stop the lead reports, for example with `accept <entry>`.
6. Watch one real terminal session against the panel.
7. Install the `.vsix`, read the report, approve the visual baseline.
