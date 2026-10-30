# How wtpool was built

[README](../README.md) · [Development](development.md) · **How it was built**

wtpool, its Claude Code hook and its VS Code extension were specified, tested and built by Claude Code
agents under one lead Claude Code session. The owner was the human gate for the spec, the
screenshots and each stop the lead reported. This repository keeps the scaffolding of
that build on purpose, as a worked example: the prompts, the frozen checks, every change to them
with its reason, and every review. The tool was called `wt` while it was built, so `verify/reviews/`
and older records, such as the earlier entries of `verify/oracle-changes.toml`, still say `wt`
([why it was renamed](troubleshooting.md#the-old-name-wt)).

Before publishing, home paths, pool paths, host names, git identities and private repository names
in the reviews were replaced with neutral ones (`/Users/you`, `~/code/app` and its slots `app-wt1`
to `app-wt3`, `mac.example`, `acme/widget`), and session scratchpad and temporary paths with
`<scratchpad>` and `<tmp>`. The build's commits are not published, so a commit the records cite
is named by what it was: its round, its freeze or its change. Decisions are stated as outcomes,
without the conversation that led to them. The findings are unchanged.

## The loop: oracle first

One rule shapes the loop: **the builder never writes what decides PASS**
([threat model](design/status-panel/verification.md#threat-model)). Each phase (0 the harness and
`wtpool --version`, 1 `wtpool status` and the read only tab, 2 the hook, 3 `free` and `fg` from the tab)
ran:

1. **Spec.** [status-panel.md](design/status-panel/status-panel.md) gives every behaviour an ID
   (R, D, H, A, S, W, X, U, B). [verification.md](design/status-panel/verification.md) says how a
   build proves each one. The commit that approved both was tagged `spec-base`, and the harness
   measured changes from it. Without that tag it falls back to another base
   ([Development](development.md#environment-variables)).
2. **Oracle author** (prompt A, a fresh agent) writes the checks the trace list and scenario
   catalog name for the phase, from the docs alone: it never reads `crates/` or `vscode/src/`.
   Each new check must fail first, on an assertion, with its canary applied if it has one.
3. **Freeze.** `cargo xtask freeze --phase N` commits exactly the author's files and writes
   `verify/oracle.lock`, a SHA 256 per oracle file and key, the ids of the entries it spends, and
   a seal over the rest.
4. **Builders** (prompt C) write the product. They never edit oracle files: `oracle_unchanged`
   fails on any change that no accepted entry in `verify/oracle-changes.toml` pins at its current
   hash, judged from the lock alone, so a squashed copy of the history is held to the same gate.
5. **Clean-clone verify.** The lead clones the commit into `~/.cache/wtpool-verify/clones/` and runs
   the verify parts there (`quick`, `int`, `ui`, `bench`, and `cc`, which starts live Claude Code
   sessions only for a Claude Code version without a live result yet), then `--report`, which
   gives PASS, FAIL or NEEDS YOU.
6. **One verifier** (prompt B, a fresh agent) reviews the clone once against the spec, the checks
   and the test standards, and ends with `verdict: clean` or `verdict: not clean`.

A failure or a blocking finding against the product sends the builder another round, three at
most. A finding that names a wrong check becomes an entry in `oracle-changes.toml`; once it is
accepted, the oracle author fixes the check and the lead freezes again. The prompts and the routes
are in [Orchestration](design/status-panel/verification.md#orchestration).

## What lives where

| Path | What it holds |
|---|---|
| [docs/design/status-panel/](design/status-panel/status-panel.md) | The spec, the verification plan with prompts A, B and C, the scenario catalog, the user flows and the trace list. `xtask` parses the catalog, the flows and the trace list. |
| `xtask/` | The harness: `cargo xtask verify`, `freeze`, `canary`, `bench` and `scrub`. |
| `vscode/test/` | The extension's checks: render, host, screenshot, end to end and the Claude Code contract. |
| [verify/oracle.lock](../verify/oracle.lock) | The hashes of every oracle file and key at the last freeze, the checks it held, the entry ids it spent and its seal. |
| [verify/oracle-changes.toml](../verify/oracle-changes.toml) | Each later change to an oracle file: id, reason, and the hash of each path it changes, after the change. |
| [verify/accepted.txt](../verify/accepted.txt) | `accept <id> <entry hash>` lines, with dated comments. An entry counts only once accepted at its current content. |
| `verify/canaries/` | Patches that break the product or the harness on purpose, to show that a check can fail. |
| `verify/trace.toml`, `verify/checks.toml` | The trace list as data (which checks prove each ID), and the layers that run each check. |
| `verify/scenarios/`, `verify/fixtures/` | Scenario documents, hand-written hook payloads, and payloads recorded from Claude Code 2.1.310. |
| `verify/shots/approved/` | The approved screenshots, the visual baseline. |
| `verify/reviews/` | Per phase: the last verify report (coverage list trimmed), verifier reviews and canary runs; the live contract and PermissionDenied runs. |
| `verify/standards/` | The copied style guides, with sources in [THIRD-PARTY-NOTICES.md](../THIRD-PARTY-NOTICES.md). |

Phase 0, the harness itself, went through eight review rounds in which verifiers wrote canaries
against the harness (`verify/reviews/phase-0/round-1` to `round-8`). From verification.md
revision 9 one verifier reviews each phase once. By the end of phase 3 the lock held 230 checks and
`oracle-changes.toml` held 53 entries. Preparing the repository for publication added two more
(`showcase-scrub`, `no-decisions`), and the rounds after phase 3 (below) brought the count to 72
before the publish round. 68 of those are accepted. The other four are non blocking findings of
phase 0, left unaccepted and deferred under the standing rule in `verify/accepted.txt`.

## After phase 3

The same loop ran six more rounds. In each, the spec and the checks changed together through
accepted entries in `oracle-changes.toml`, and builders changed the product to match.

- **Generic** (status-panel.md revisions 16 to 18, entries `generic-spec` to `scrollbar-bottom`):
  wtpool works on any git repository with no settings. Records name their harness and give each
  call a neutral kind, so another agent tool can plug in. The status document became a documented
  contract, and the extension runs wtpool in the workspace folder.
- **Live labels** (revision 19, `live-labels`): a session's label is its title, and a build the
  session started keeps it working.
- **Live output** (revision 20, `live-output`): the hook records the path of each background
  task's output file, never the output. The detail strip shows its tail, and Follow streams it
  into a terminal.
- **Icons** (revision 21, `icons-robot-dial`, `visual-baseline-4`): working shows a robot whose
  arms swing, and building a spinning dial.
- **Rename** (revision 22, `rename-wtpool`): `wt` became `wtpool`, with no change in behaviour.
- **Publish** (entries `publish-*`): a Claude Code plugin installs the hooks, `wtpool bg` and
  `wtpool fg` print only the path on stdout, a failed `gh` call says why, a `done` subagent shows
  for 1 minute, an agent shows working while its background task runs, the output tail shows tabs
  as spaces, and the repository gained a security policy, a contributing guide, a changelog and
  pages on troubleshooting, privacy and uninstalling.

## Lessons from this build

- **A live smoke test caught what every replay missed.** `wtpool hook install` printed its presence
  note on stdout after the JSON block, so Claude Code skipped the settings without a word and no
  hook ran live. Every replay passed, because the oracle parsed only the first JSON object of
  stdout. H22 now says stdout is the block alone, and the checks parse stdout whole
  (entry `h22-stdout-json`).
- **The machine is part of the test.** Watch checks time file events. When another part wrote
  many files at the same time (the coverage rebuild, VS Code host profiles), macOS's FSEvents
  daemon fell behind and 12 watch checks and a budget failed with no product change. The parts now
  run in waves: `int`, then `bench`, then `quick` with `cc` and `ui` (`p2-int-first`).
- **Size stress to the real load.** The first stress checks backed up FSEvents on their own. They
  were sized to one wtpool instance and fewer than 10 worktrees, keeping each check's meaning: 200
  parallel writers became 16, 32 writers by 20 rounds became 8 by 10, a 20 worktree document became
  10 (`p2-realistic-load`).
- **Mutation testing was dropped for time.** Phase 2's run would have been about 984 mutants at
  about 65 s each, about 4 hours. The bar is now line coverage of unit, integration and end to end
  tests, 100 % of changed lines (`drop-mutants`).
- **Coverage found what the checks missed.** Phase 3 run 1 passed every check but `cover`, which
  named 10 lines of error handling in the confirm dialog that no layer ran. Two oracle changes
  added the missing cases (`p3-x12-error-paths`, `p3-spawn-throw`).
- **Canaries find holes in the harness.** Each canary that survived a phase 0 round became an
  oracle change: a lint that skipped every folder named `target`, an isolation check blind to HEAD
  moves under the machine's default git identity, a version check that never built a release binary.
- **A safety check can be the risk.** The first isolation check hunted for lingering processes and
  could kill the owner's own terminal shells. It was replaced by snapshots before and after each
  part that look for no processes and kill none (`p0r8-no-process-hunt`).
- **A live tool is not a script.** No scripted contract session produced a PermissionDenied:
  Claude Code's auto mode allowed the scripted force push. A hand-written scenario covers it, and a
  one-off live run later confirmed the event order (`verify/reviews/live-permission-denied-2026-10-25/`).

## Running it yourself

[Development](development.md) has the build and verify commands, what the harness needs and the
variables it reads. A fresh clone needs no `spec-base` tag, no recordings and no live Claude Code
run. Every verify part runs on sandbox pools in `verify/out/<run>/`, and `isolation__left_no_trace`
fails a run that left traces outside its run folder, including in your own pool when
`WTPOOL_FOREGROUND` names it.

## Phase 5a

Phase 5a added OpenCode, the second agent harness, as its own crate, `crates/wtpool-opencode`, with a
TypeScript plugin that forwards its events. Like the earlier phases, it was built oracle-first: the
checks, which include replays of sessions recorded from OpenCode 2.2.0, were written from the
design, and the product was changed to meet them. Each later change to a check goes into
`verify/oracle-changes.toml` with its reason.
