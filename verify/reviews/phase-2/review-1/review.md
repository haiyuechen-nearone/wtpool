# Verifier review 1, phase 2 (verification.md revision 14)

- **Spec read:** `docs/design/` at the phase 2 freeze (status-panel.md revision 9, verification.md revision 12). I judged against the phase 2 review 1 head where later spec revisions changed it: revision 13 drops mutants, revision 14 sizes the load.
- **Clone and run:**
  - The clone `~/.cache/wt-verify/clones/phase-2-r2` holds the phase 2 review 1 head and its tree is clean.
  - The run `verify/out/phase2-r5` says PASS. Every part ran fresh and no check was flaky.
  - It reports 4628 of 4628 changed lines covered by unit or an oracle layer.
- **Method:**
  - I read both docs, `verify/oracle-changes.toml`, `verify/accepted.txt` and the run report.
  - I split the reading into four parallel read-only slices: hook code, status code, Rust oracle checks, and scenarios/cc/extension checks. Each slice read the code and the run outputs, ran no cargo, npm or wt, and returned file:line evidence.
  - I re-checked every blocking claim against the source myself: `hook/apply.rs:83-86,229-265`, `hook/procs.rs:44-73`, `hook/mod.rs:51-56`, `status/agents.rs:105-121`, `xtask/tests/support/mod.rs:477-481`, the int lcov, `~/.local/share/wt-verify/recordings/claude-2.1.306/cc-results.json` and the 2.1.306 fixtures.
  - I read the post-freeze harness diff `git diff -- xtask verify docs` from the phase 2 freeze to the phase 2 review 1 head myself, and opened the mockups and every phase 2 screenshot.
  - No verify part was rerun. One slice ran `git checkout -q` of the phase 2 review 1 head in the clone by mistake (HEAD detached at the same commit). It restored `main` at once and the tree stayed clean.

## Oracle changes since the freeze

Each entry is in `verify/accepted.txt`.

| Entry | Review |
|---|---|
| p2-b1-wait | Right. `support/hook.rs:341-371` blocks in `wait_with_output` with no polling sleep and no added tolerance. |
| p2-cc-interrupt-allow | Right. The block.sh allow rule is in the cc settings. `cc_interrupt.jsonl` was re-recorded without a PermissionRequest, so its "quiet after 125 s" assert agrees with H13/A1. |
| p2-w4-quiet-window | Right. `watch_agents.rs:103-112,154`: starts are stamped by the shim's own `$EPOCHREALTIME`. The window (1.5 s) and limit (0 starts) are unchanged. |
| drop-mutants | Right in substance. The mutants module, part, harness check, nextest profile, trace line and spec text are all gone, consistently (clone.rs, parts.rs, report.rs, reuse.rs, oracle.rs, trace.toml). The other xtask diffs under its blanket `xtask/src/` path are rustfmt reflows plus `.get()` in place of indexing (scrub.rs:121-133, replay.rs, cc.rs). None changes what a check asserts. See N14 for a small doc inconsistency. |
| p2-realistic-load | The counts match revision 14: H18 16 writers, H24 8 x 10, B9 10 worktrees of 5 actors. Contention is likely but no longer forced (N9). |
| p2-int-first | Right. `schedule.rs` WAVES is now `[int] [bench] [quick, cc] [ui]`. cc still runs after int, whose `cc_replay` results it reads. No check changes. |

## 1. The oracle

### Rust checks in the author's diff from the phase 1 review 2 freeze to the phase 2 freeze (`xtask/tests/`)

These checks test what the spec says, with their named parameters:
- **Install and output:**
  - H1 and B2: 14 events, exec form, 5 async, `timeout` 2, no timeout on SessionEnd, refuses a binary under `target`.
  - H22: mtime and size snapshot, presence.
  - H2, H3, H4, H5: 30 MiB accepted, 40 MiB writes nothing.
- **Files:**
  - H6: whole-sandbox tree snapshot; `setup__should_create_wt_activity` runs setup, bg, fg, free, new and rm.
  - H7: PATH without git.
  - H8, H9, H17: `../x`, 65 characters, Unicode, empty.
  - H23.
- **Text:**
  - H14: all four named cases, compared exactly, plus a secret scan.
  - H15, H16.
- **State and timing:**
  - H12: 32 newest in flight; the `done_ids` ring of 64 both ways.
  - H18 symlink, FIFO and 16 KiB.
  - H19, H21: 7 days, 15 minutes, newest done subagent per worktree, timed by `updated`, lock deleted.
  - `merge__should_refuse_an_older_state`.
- **Status:**
  - A1 decay: 119/121 s, build here or elsewhere, Agent with a subagent working, waiting or done, 14/16 minutes.
  - A4, A7, S5 (88 records skipped by age, a 2-day-old live session shown), S6.
  - W4 refresh: an unthrottled refresh and a 500 ms throttle are both caught, and the trailing refresh is checked.
  - B1, B4 (phase 2 part), B9.
- **Scenario comparison** (`scenarios.rs:116-275`): actors are compared whole (key, kind, label, state, reason, action, `placed_by`, error, since, updated), and so is every worktree fact. Loose only on actor order, which the webview sorts and e2e checks, on `generated` and `last_change`, and on the facts of a missing worktree.

Checks I think wrong (all non blocking: each ID keeps other passing checks, and the product is correct where these checks are blind):
- **`wait__should_hash_input_with_sorted_keys`** (`hook_state.rs:430-514`, H13) is vacuous on sorted keys.
  - `reversed()` rebuilds a `serde_json::Map`. The workspace has no `preserve_order`, so the map serialises sorted and both hooks receive identical text. Every scenario and fixture also goes through `Value`.
  - A product that hashes the raw input text would pass.
  - The product sorts keys (`hook/text.rs:37-41`), but only its unit test shows it.
  - Fix: send raw stdin bytes with a different key order.
- **`transitions__should_match_the_event_table`** (`hook_state.rs:123-160`, H11): the `post_tool_use`, `post_tool_use_failure` and `permission_denied` cases start `working` and expect `working`, so they cannot fail.
  - No oracle check shows that PostToolUseFailure or PermissionDenied ends the wait bound to its call.
  - The product handles all three through one `Event::End` arm (`apply.rs:92-101`), and PostToolUse is tested.
  - Fix: start these cases from a bound wait.
- **`budget__b11`** (`hook_gc.rs:199-251`) passes without gc. Every session sends SessionEnd, which by H20 deletes its subagents, leaving at most 28 records. B11 still follows from H21's checked rules.
- **`budget__b6`** (`bench/agents.rs:133-139,208-215`): "reads each changed record once" is vacuous.
  - The check makes a record unreadable and expects a change in `skipped_records`, but an unreadable record does not count there (`status/records.rs:38-44`).
  - The `3 * first_round` limit counts two rounds as the baseline.
  - `watch__should_read_only_the_changed_record` covers the read-once part of W4.
- **`hook__should_restart_when_gc_deleted_the_lock`** (`hook_files.rs:366-434`, H24) can pass by timing. gc and the writers have no barrier, and nothing asserts that gc deleted a record while a writer held its lock.

### Scenarios, fixtures, cc, extension checks

- **Scenarios 9-33:** each matches its catalog line. Both variants of 16 and 28 are present, as are the three states of 21 and all 40 commands of 32.
- **Fixtures and scrubber:**
  - `bash-commands.toml` covers every H9/H14 form, heredocs and five secret forms.
  - The hand fixtures match the 2.1.306 recordings in shape.
  - `scrub.rs` keeps what the checks need and leaves no home paths or secrets.
- **cc:** the sessions' Claude Code and wt asserts match the Contract test table.
  - Isolation holds: `settingSources: []` plus the run's settings, sandbox HOME for `wt hook install`, and transcript cleanup scoped to the run.
  - Nothing touches `~/.claude/settings.json` or `~/code/app`.
- **Extension:**
  - `title.test.cjs` has the three X18 checks, and the two canaries target them.
  - `flow__wait_appears` checks waiting, the "1 waiting" chip, `Worktrees (1)`, the live region and no popup.
  - `flow__wait_answered` checks the row returns to working and the title to `Worktrees`.
  - `flow__waiting_chip_focuses` checks focus on the waiting row.
  - All three drive the wait through the release `wt hook` on a sandbox pool, from a live stub parent process (`claude-stub.mjs`).

### User flows

All 22 flows of phase 2 or earlier have a passing `flow__` check in this run, and `flows__should_each_have_an_e2e_check` passed. N12 notes that the check accepts a `flow__` result from any layer.

## 2. The code

### Blocking

**B1. A PreToolUse for AskUserQuestion or ExitPlanMode overwrites a standing wait and ignores `done_ids`.** H13, H12: broken and untested.
- **Where:** `crates/wt-core/src/hook/apply.rs:250-265`.

  ```rust
  if let Some(reason) = reason {
      record.waiting = Some(Wait { reason, ..., call: call.id.clone() });
      return State::Waiting;
  }
  ```

- **Why it is wrong:**
  - The spec says "PreToolUse never ends a wait". Here a question Pre replaces a permission wait bound to another call. When the question's Post ends, the actor shows `working` while the permission dialog is still open.
  - `add_call` (`:229-235`) skips an id already in `done_ids`, but `pre` then still sets the wait.
  - PreToolUse and PostToolUse are both async (H1), so they can arrive out of order (that is why `post_before_pre` exists). A Pre that arrives after its Post then creates a wait bound to a finished call. Nothing ends that wait until UserPromptSubmit, Stop or `idle_prompt`. That breaks H12's "a later PreToolUse with that id adds nothing".
- **Untested:** no unit, int or scenario case covers a question/plan Pre while a wait stands, or a late question/plan Pre.
- **Fix:**
  - Leave an existing wait alone.
  - Do not create a wait when the id is in `done_ids` (or let a later end of that id clear it).
  - Add both cases.

**B2. H24's ordering uses the time `main` began, not the kernel start time, on the real process table.** H24 broken on the real path; int cannot see it.
- **Where:** `crates/wt-core/src/hook/procs.rs:69-72`, with `hook/mod.rs:55`.
  - `Self::System => Me { started_micros: entered_micros, .. }`.
  - `entered_micros` is `clock::system_micros()` taken when the process starts running.
- **Why it matters:** the spec says "kernel start time, `writer_started`". Hooks are spawned close together, and five of the events are async. If an earlier-spawned hook reaches `main` later (dyld, cold cache), it counts as the newer writer. Its state then wins, for example an async PostToolUse writing `working` over a Stop's `idle`.
- **Untested:** int pins `WT_VERIFY_PROCS`, where `started` comes from the table (`procs.rs:50-53`). So no oracle layer exercises the system path.
- **The likely cause:** sysinfo gives whole seconds, and `unsafe_code = "forbid"` rules out calling `proc_pidinfo` directly.
- **Two ways to resolve it:** read the start time in microseconds (for example through a safe libproc wrapper, if that dependency is approved), or amend H24 to accept the time `main` began. Either way, add a unit test of the system path.

### Non blocking

- **N1. A1, W6 timing in production (`status/time.rs:134-137`, `live.rs:93-100`).**
  - `watch__should_turn_quiet_on_time` changes the clock file, which reaches the loop as `Change::Clock`. So the production `deadline()` → `SystemClock::until` path never runs in int.
  - The unit tests do cover it: `run__should_tick_when_the_trackers_deadline_passes` and `system_clock_until__…`.
  - An int check without a clock file (a record 119 s old turns `quiet` within about 1 s) would close the gap.
- **N2. A1 subagent condition (`status/agents.rs:114-121`).** The code uses the subagent's raw record state (`Working | Waiting`), not its shown state.
  - An interrupted subagent never sends SubagentStop. Its parent's Agent call stays in flight, so the parent shows `working` forever.
  - In practice the interrupt sends PostToolUseFailure for the Agent call, and the Agent tool is async in 2.1.306 (N10). This is also a spec reading.
  - Suggest using the subagent's shown state.
- **N3. Merge race leaves a stale wait (`hook/apply.rs:182-190`).** When an older writer's Post ends a call, `merge_older` keeps the newer writer's `waiting`, bound to that now-done call. A PostToolBatch listing the call clears it. Clearing it when the merged `done_ids` contains `waiting.call` would close it.
- **N4. H19 "a newer main session ends the older ones" lives in status, not in the hook.**
  - The hook leaves it to `status/agents.rs:152-185`. Scenario 29 checks it there.
  - Status picks the newest `updated`, not the newest session, so a late async hook of the old session can briefly hide the new one.
- **N5. Record fields outside the spec's list, stored raw (`apply.rs:216`, `record.rs:37`).** `first_cwd`, `ended` and `in_flight[].match` are not in the record field list. `first_cwd` is stored unsanitised and is not limited to 80 characters (H15 reading). The spec should list these fields or exempt paths in H15.
- **N6. W4 triggers on more than the spec names (`apply.rs:93-99`, `records.rs:273-279`).** It also fires on PostToolUseFailure and PermissionDenied of Edit/Write/NotebookEdit/Bash. It stays rate limited to once a second.
- **N7. Event merging and memory (`live.rs:75-86`, `records.rs:173-186`).**
  - When a `wt-activity/` folder event merges with record events in one 200 ms window, the W4 refresh and the new-process read wait for the heartbeat.
  - S5 parses every record before applying the 512 cap. That is needed so a live session is never dropped, but memory grows with the folder until gc.
- **N8. Hook edge cases.**
  - H6 ownership is checked only indirectly, through the lock's uid (`store.rs:32-39,63-74`).
  - A watchdog exit between the temp file and the rename leaves a `.tmp` that gc never sweeps (`store.rs:93-109`).
  - `to_json` trimming to 16 KiB can fall below the 32/64 caps with 128-character ids (`record.rs:120-133`).
  - The sync PermissionRequest canonicalises every Bash path word, but B1 passes.
  - `thread::spawn` sits outside `catch_unwind` (`wt/src/hook.rs:14-28`), which matters only under resource exhaustion.
- **N9. Contention is likely but not forced (`hook_files.rs:121-185,298-364`).** After p2-realistic-load, 16 and 8 writers still race on one lock, but nothing makes them start together. Writing stdin only after every child has spawned, as `merge__should_refuse_an_older_state` does, would keep the meaning at the smaller counts.
- **N10. Spec input: what Claude Code 2.1.306 really sends.**
  - The Agent tool runs async (`isAsync: true`): PostToolUse-Agent and PostToolBatch come before the subagent's events. So A1's "Agent call in flight while its subagent works" never fires, and the main session shows `idle` while its subagent works.
  - No recording carries `session_title`, so labels are always `session HH:MM`.
  - No SDK session got an `idle_prompt`. In `cc_permission_deny_interrupt.jsonl`, a denial with interrupt is followed by no PostToolUseFailure, batch, Stop or `idle_prompt`, only SessionEnd. The row then stays `waiting` until the next prompt or SessionEnd, unless the terminal client sends `idle_prompt`. This is a different trigger from the known "approved long command" item. Worth watching in What needs you item 6.
- **N11. The cc live result never ran against this product.**
  - `~/.local/share/wt-verify/recordings/claude-2.1.306/cc-results.json` comes from the oracle step. Every wt assert in it is "fail assertion" ("wt wrote no record in any session").
  - The wt half of each cc check rests on `cc_replay` against recorded payloads with a fake process table. `cc_no_hook_output` is decided by that live run alone, whose hook wrote nothing. H2 stays covered by `hook__should_print_nothing_for_every_fixture` and every `cc_replay`.
  - The Trace list says the live result is in `verify/reviews/`; it is not there.
  - This follows revision 11 ("test once"), but the real hook has never run under real Claude Code. One `--part cc --contract` run with this build, or What needs you item 6 (one real terminal session), would close the gap.
- **N12. Weak or loose oracle checks.**
  - `post_before_pre` cannot observe "nothing in flight": it uses Read with no build.
  - The newline in `hostile_agent_strings` sits past the 40-character label cut (`$.steps[1].hook.with.session_title`), so it is never observed.
  - `dead_process` has no e2e layer, although only the pid-reuse variant needs the fake process table.
  - `cc_replay` skips the 70 s holds (`replay.rs:324,351`).
  - `cc_permission_deny` checks that some PostToolBatch has `tool_calls`, not the denied id (`sessions.mjs:302`).
  - The live-region assert matches only `/wt2/` (`flows-waits.spec.mjs:70`).
  - `flow__waiting_chip_focuses` has one waiting row, so it cannot tell "first" from "any".
  - The X18 title tests cannot tell waiting worktrees from waiting actors (`title.test.cjs:56`).
  - `flows__should_each_have_an_e2e_check` accepts a result from any layer (`xtask/src/flows.rs:125`).
  - B1 never asserts that its 1000 runs wrote records (`bench/agents.rs:60-90`).
- **N13. The coverage instrument under-reports `--watch` (`xtask/tests/support/mod.rs:477-481`).** `Watch::drop` sends SIGKILL, so the instrumented `wt` never writes its profile. int.lcov shows `run()` (watch.rs:190) entered only by the 3 runs that ended by EOF or a closed pipe. The error runs in the safe direction: lines int reaches are reported as "unit only", never the reverse. Fix: close stdin and wait, so the watch exits 0 by W8.
- **N14. verification.md line 3 says "coverage of unit, int and e2e is the bar".** Layers and the report measure unit, int, render and host; e2e is not measured. The wording should match.
- **N15. X18 after a restore.** A restored tab keeps its saved title, for example "Worktrees (2)", until the first document, and keeps it if `wt` never starts (`tabs.ts:25-27`).
- **N16. Test standards.**
  - Some tests bundle several Whens: `hook/mod.rs:121`, `hook/apply/tests.rs:186`, `hook/event/tests.rs:95,111,178`.
  - `session_label__should_read_localtime_without_tz` asserts only the length.
  - `status/live/tests.rs:314-351` depends on real time and could flake on a loaded machine.
  - `pool.rs` is 892 lines: under the cap, over about 500.
  - Names, bare Given/When/Then and the only suppression (the allowed `#[expect(clippy::panic)]`, `wt/src/hook.rs:91`) are fine.

### IDs of phase 2 and earlier

| ID | Verdict |
|---|---|
| H1 to H11, H14, H16, H17, H18, H20, H21, H22, H23, D5 | OK |
| H12, H13 | **B1** |
| H24 | **B2**; N3 |
| H15 | OK; N5 |
| H19 | OK for the parent pid (pid 1 excluded, process kept when reparented); N4 |
| A2, A3, A4, A5, A6, S5, S6, R4 | OK |
| A1 | OK; N1, N2 |
| A7 | OK. Small leaks: an out-of-pool subagent can keep an in-pool parent working, and out-of-pool records still set timers. |
| W4, W6 | OK; N1, N6, N7 |
| X18 | OK; N15 |
| Phase 1 IDs | Unchanged since phase 1 review 2 (`git diff -- vscode/src` from the phase 2 freeze to the phase 2 review 1 head is empty). The status changes keep S1, S2 (`read_only_git` for every status action, `wt/src/main.rs:340`), S7 (schema unchanged) and W1 to W9. |

**Verify feature:** `WT_VERIFY_CLOCK`, `WT_VERIFY_PROCS` and `WT_VERIFY_PANIC` are read only under `#[cfg(feature = "verify")]` in the `wt` crate (`hook.rs:79-100`, `main.rs:423-429`). `release__should_have_no_verify_feature` passed.

**The owner's real setup:**
- The hook writes nothing to stdout, runs no git and no shell, and never writes settings.
- It writes only `wt-activity/` (O_EXCL|O_NOFOLLOW temp file, then rename) and `~/.local/state/wt/hook.log`.
- It does nothing until a pool command creates `wt-activity/`.
- The watcher writes nothing and opens records `O_NOFOLLOW|O_NONBLOCK`.
- Beyond B1 and B2, I found no risk.

### Lines no oracle layer covers

| Group | Verdict |
|---|---|
| `#[cfg(test)]` code counted as product lines: hook/record.rs 156-217, procs.rs 140-198, mod.rs 121-148, text.rs 123-132, log.rs 77-93, install.rs 169-179, clock.rs 120-138 and 173, run.rs 202-218, config_file.rs 231 and 244, status/fixture.rs (all), procs.rs 166-262, mod.rs 81-138, document.rs 272-318, time.rs 219-324, schema.rs 255-283 | Justified: test code. |
| status/schema.rs 26-253 | Justified: schema generation, checked by `schema__generated_files_current`. |
| status/watch.rs (most), live.rs (all), records.rs 192-205/234-238/251-253/273-280, state.rs 94-155/169-185, agents.rs 141-148, time.rs 25-28/155-157 | Justified: int reaches them (W2, W4, W7 watch checks), but the SIGKILL drops their profile (N13). |
| hook/procs.rs 69-72, 87, 107-132 (system process table) | Justified as coverage, because int pins `WT_VERIFY_PROCS` by design and `me__should_read_the_system` is a unit test. The timing defect on this path is B2. |
| status/time.rs 134-137 (`SystemClock::until`) | Non blocking: N1. |
| hook/bash.rs 47, 130, 162-165, 178, 226, 269-283, 295-300, 318-330, 359-360, 387 | Justified: tokenizer details beyond the forms H9/H14 name (escapes, `&>`, `<<<`, `<<-`, time/nice/timeout options, `--manifest-path=`, a lone `cd`, unset HOME). The named forms run in int through `bash_attribution`. |
| hook/record.rs 127-132 | Non blocking: trimming to 16 KiB is reachable by int (N8). |
| status/agents.rs 34 (record without `process`), 129-130 (`input` wait reason) | Non blocking: int could reach both through a pid-1 parent and the `Notification-elicitation_dialog` fixture. Suggest adding both. |
| hook/apply.rs 187, 231, 288; clock.rs 18-22, 60, 78-90; event.rs 37, 86, 293-296; gc.rs 55-59; install.rs 68; log.rs 15; mod.rs 96, 110; payload.rs 76; place.rs 37; store.rs 107; pool.rs 391-394; wt/src/hook.rs 85, 100; main.rs 216, 221, 429; version.rs 23, 29 | Justified: error, edge or non-verify cfg paths. |
| status/watch.rs 144-146, 169-172, 199, 214-215, 220, 225, 321; records.rs 335; time.rs 103-107, 111; facts.rs 76, 96, 126-129 | Justified: notify failures, output errors, path forms FSEvents never sends, time formats the hook never writes, unknown counts, and the Am/CherryPick/Revert arms of the mechanism int covers. |

## 3. Screenshots against the normative mockup parts

**Main** (columns, row text, icons, highlight, chips, header, row actions), all four themes at 1100 px:
- The waiting row has the left highlight bar in the attention colour, with the bell icon and "waiting".
- The session label is in attention colour, then "· permission: Bash git status", as in the mockup.
- The chips read "2 free" and "1 waiting" (attention). The header reads "live".
- In `several_agents_one_worktree`, wt2 shows three lines: "Explore · Read f.txt", "first session · Agent" and a muted "second session · idle". Each line has its own Last, as in the mockup's second actor line.
- Row actions do not show, because no row has focus. They were reviewed in phase 1.
- Colours follow U6 in dark, light, high contrast and high contrast light.

**StatusBar** (tab title text, X18): `e2e-permission_wait_and_answer-1.png` shows the tab "Worktrees (1)" while waiting. `-2.png` shows "Worktrees" after the answer, with the row back to working. Both match the mockup.

**600 px** (`dark-permission_wait_and_answer-600.png`): no horizontal scroll, and the branches fit.
- Non blocking N17: the Agent cell cuts "push the fix ·…". The waiting reason ("permission: Bash git status"), the most useful text in the row, disappears.
- U1 names cutting only for Branch. Suggest cutting the label first, or wrapping the agent line.

**Tooltip and States:** no new state or detail fact in phase 2. The idle_pool baseline was approved in phase 1, and the phase 2 shots match it pixel for pixel in the unchanged rows.

Deferred phase 1 items are not re-raised: phase 2 did not make them worse.

## Screenshots to approve

All in `/Users/you/.cache/wt-verify/clones/phase-2-r2/verify/out/phase2-r5/shots/`:
- `dark-permission_wait_and_answer-1100.png`
- `light-permission_wait_and_answer-1100.png`
- `high-contrast-permission_wait_and_answer-1100.png`
- `high-contrast-light-permission_wait_and_answer-1100.png`
- `dark-several_agents_one_worktree-1100.png`
- `light-several_agents_one_worktree-1100.png`
- `high-contrast-several_agents_one_worktree-1100.png`
- `high-contrast-light-several_agents_one_worktree-1100.png`
- `dark-permission_wait_and_answer-600.png`
- The e2e set: `e2e-permission_wait_and_answer-1.png`, `e2e-permission_wait_and_answer-2.png`, `e2e-several_agents_one_worktree-1.png`

## Summary

- **Blocking (both against the product; route 2, builder round):**
  - **B1** (`hook/apply.rs:250-265`, H13/H12): a question/plan Pre overwrites a standing wait and ignores `done_ids`.
  - **B2** (`hook/procs.rs:69-72`, H24): writer order uses the time `main` began, not the kernel start time. Either fix it or amend H24.
- **Wrong checks (non blocking):** `wait__should_hash_input_with_sorted_keys`, three cases of `transitions__should_match_the_event_table`, `budget__b11`, the read-once part of `budget__b6`, and `hook__should_restart_when_gc_deleted_the_lock` (timing).
- **Open questions:** N10 (Agent async, so a main session shows idle while its subagent works; a denial with interrupt stays waiting with no `idle_prompt`), N11 (the real hook has never run under real Claude Code), N14 (coverage wording).

verdict: not clean
