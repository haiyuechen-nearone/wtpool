# Verifier review 2, phase 2 (verification.md revision 14, status-panel.md revision 10)

- **Spec read:** `docs/design/` at the phase 2 freeze. I judged against the phase 2 review 2 head where later spec revisions changed it: verification.md revisions 13 and 14, and status-panel.md revision 10 (the H24 amendment). verification.md did not change between the phase 2 review 1 head and the phase 2 review 2 head.
- **Clone and run:**
  - The clone `~/.cache/wt-verify/clones/phase-2-r2` holds the phase 2 review 2 head and its tree is clean.
  - The run `verify/out/phase2-r6` says PASS, and every part ran fresh. It reports 4644 of 4644 changed lines covered by unit or an oracle layer. `flow__theme_switch` was flaky once.
- **Scope:** review 1 (`verify/reviews/phase-2/review-1/review.md`, at the phase 2 review 1 head) stands for everything unchanged. I reviewed all of `git diff` from the phase 2 review 1 head to the phase 2 review 2 head:
  - `hook/apply.rs` and `hook/apply/tests.rs` (the B1 fix)
  - status-panel.md (H24)
  - `verify/accepted.txt` and `verify/oracle-changes.toml`, entry p2-visual-baseline
  - `verify/shots/approved/`
  - review 1 itself and the run 5 report, which are text only
- **What I ran:** no verify part, cargo or wt, and no checkout. I read the code, the run 6 report and its artifacts. I also read the 2.1.306 recordings, to check the order in which events arrive.

## Oracle changes since review 1

| Entry | Review |
|---|---|
| p2-visual-baseline | Right. It locks the 9 shots that Visual review asks for in phase 2: `permission_wait_and_answer` and `several_agents_one_worktree` in four themes at 1100 px, plus `permission_wait_and_answer` in Dark Modern at 600 px. Each approved PNG is byte-identical to the same file in `phase2-r5/shots/` (the approved set) and in `phase2-r6/shots/`. The phase 1 `idle_pool` set is unchanged. |

No check under `xtask/tests/`, `vscode/test/` or `verify/scenarios/` changed. Part 1 of review 1 (oracle against the spec, the wrong checks, and every flow of phase 2 or earlier having a passing `flow__` e2e check) stands. In run 6, `flows__should_each_have_an_e2e_check` passed again.

## 1. B1 (H12, H13): the fix

**The B1 cases are fixed.** `crates/wt-core/src/hook/apply.rs:262-297`:
- **A question or plan PreToolUse never replaces a standing wait.** The `record.waiting.is_none()` guard at `:273` blocks it, and `pre` then returns `active_state` (`:296`). So a PreToolUse can neither end nor replace a wait.
- **A late PreToolUse for an ended id creates no wait.** `ended` (`:268-271`) blocks the question or plan wait. The bind branch (`:289-292`) already skipped done ids, and `add_call` (`:244`) adds nothing to `in_flight`.
- **It also holds when two hooks race.** The writer reads the record under the lock and applies the event to it (`hook/event.rs:112-125`, `:325-330`). A late Pre therefore sees the `done_ids` of the Post that beat it, both on the direct path and on the `merge_older` path.
- **One limit:** `done_ids` is a ring of 64. A Pre arriving after 64 newer calls ended would wait again. H12 accepts this, because it names the ring.

**N3 (`merge_older`) is fixed.** `apply.rs:190-199`: when the merged `done_ids` holds the call that the newer writer's wait is bound to, the wait ends, `state` becomes `working`, and `since` becomes the merged `updated`. This matches H13 ("ends only when that call ends") and H24 (the older writer keeps its calls but no other state).
- `result` is computed from the record read under the lock, so its `in_flight` and `done_ids` already contain the newer writer's.
- A wait that a Notification set (no `call`) is kept. That is right, because the older writer's call end came before that Notification.

**Unit tests against the test standards** (`apply/tests.rs:291-362`):
- Names, rstest tables and the Given/When/Then comments are fine. `apply__should_leave_a_standing_wait_and_ended_calls_alone` has no `// Given`, like the existing event-table test in the same file.
- Each case fails on the code before the fix: the first two would show a `Question` wait, and `plan_after_its_post` would show `Waiting`.
- **The table is not exhaustive** (non blocking, N18):
  - No case has a late `AskUserQuestion` (only `ExitPlanMode`).
  - No case has a question during a wait that a Notification set.
  - No case has the standing call ending while the question is still in flight (R1 below).
  - These cases could have been rows of the existing event-table rstest, which has the same shape.
- In `merge_older__should_end_a_wait_whose_call_the_older_writer_ended`:
  - The `wait_on_another_call` row ends `c2` with tool `Bash` and hash `h`, but `c2` is a `Read r` call. This is harmless, because End matches by id.
  - Nothing asserts `since` (`:198`). Revision 13 dropped mutants, so nothing else would catch its removal.
  - The test feeds both writers the same `before`, while the real older writer reads the newer one's record. `merge_older`'s rule is still tested correctly.

**Coverage:** `apply.rs:196-198` (the N3 branch) are unit only. int could reach them: `merge__should_refuse_an_older_state` already sets each writer's start time through `WT_VERIFY_PROCS`, so a variant that pairs a newer PermissionRequest with an older PostToolUse would reach these lines. Non blocking (N19). The other unit-only lines in run 6 (`apply.rs:188, 242, 307`, `config_file.rs:231, 244`) are lines review 1 already justified, shifted by the new lines above them.

### R1. A question that arrives during a stale wait never shows as waiting (non blocking; spec gap)
- **The case:** `permission_wait()` (Bash `c1`), then `Pre(q AskUserQuestion)`, then `End(c1)`.
  - Result: `end_call` clears the `c1` wait (`apply.rs:332-338`), `active_state` returns `working`, and `q` stays in flight with no wait.
  - The question dialog is open, but the row shows `working`.
- **How it is reached:** PreToolUse and PostToolUse are both async (H1), and tools that are not concurrency-safe run one after another. So a question PreToolUse that lands while a permission wait stands nearly always meets a stale wait whose PostToolUse has not been written yet. Take one assistant message with "Bash (permission granted), then AskUserQuestion". The async `Post(c1)` and `Pre(q)` are spawned within milliseconds of each other. If `Pre(q)` takes the lock first, the row ends at `working`, whether `Post(c1)` then writes directly or through `merge_older`.
- **Before the B1 fix** this case ended correctly: the question replaced the stale wait.
- **The fix is not wrong:** it is the literal reading of H13 ("PreToolUse never ends a wait"), with one wait slot.
- **Suggested fix, compatible with H13:** when a wait ends (`end_call`, `end_listed`, the N3 branch of `merge_older`) and an `AskUserQuestion` or `ExitPlanMode` call is still in `in_flight`, open a wait bound to that call by id. Add the case to the table.
- **Why it does not block:** no ID says that a pending question waits once another wait ends, and the trigger is narrow. It may still be worth fixing before install, because a missed question is the failure this panel exists to prevent ("a wait never decays, as missing one costs more than a stale one").

## 2. B2 (H24): the amended spec

The code matches revision 10's H24:
- **Start time:** `HookEnv::from_process` (`hook/mod.rs:55`) takes `clock::system_micros()`, wall clock in microseconds. `run_hook` calls it first, right after the `hook` argv check in `wt/src/main.rs:279-283`, before stdin is read.
- **System path:** `ProcessTable::System` uses that time as `started_micros` (`hook/procs.rs:69-72`).
- **Ordering:** `settle` (`apply.rs:171-177`) merges only when `current.writer_started > started_micros`, so ties go to the later writer.
- **Test:** `me__should_read_the_system` (`procs.rs:190-198`) is the unit test of the system path that review 1 asked for.
- **Coverage:** int pins `WT_VERIFY_PROCS` by design, so the system path stays unit only, as justified in review 1.
- **One limit:** the time is wall-clock time, so an NTP step between two hooks could reorder them. That falls inside the accepted edge case.

B2 is resolved.

## Code against the IDs (changes only)

| ID | Verdict |
|---|---|
| H12, H13 | OK. B1 is fixed, and so is N3. R1 and N18 are non blocking. |
| H24 | OK against revision 10. N19 is non blocking. |
| H11 | OK. The PreToolUse row ("`working` unless waiting") holds. R1 is where the question row falls silent. |
| All other IDs | Unchanged since the phase 2 review 1 head. Review 1's verdicts stand. |

**The owner's real setup:** the diff changes only pure state functions. No file, path, settings or process behaviour changed. I found no new risk.

## 3. Screenshots against the normative mockup parts

All 9 phase 2 shots in run 6 are byte-identical to run 5's. Review 1 compared those with the Main, StatusBar, 600 px, Tooltip and States parts of the mockup, and they were approved as p2-visual-baseline. Nothing changed, so that comparison stands. The diff touched no `vscode/` file and no render path. N17 (the waiting reason is cut at 600 px) stays open, as review 1 noted, and is now locked into the baseline.

## The flaky flow__theme_switch

The new diff could not cause it:
- **Where it failed:** in the check's setup, not in its Then. The error is "setup: the webview body never got vscode-dark for Default Dark Modern: vscode-light" (`phase2-r6/e2e/artifacts/flows-look-flow-theme-switch/error-context.md`; check at `vscode/test/e2e/flows-look.spec.mjs:62-93`). VS Code did not apply the Dark Modern theme within the 15 s poll after the test wrote `settings.json`.
- **Why the diff is not involved:** the diff changes only `hook/apply.rs`, which the theme flow never runs (it uses no hooks). It also changes docs and the approved PNGs, which this check does not read. No `vscode/` file changed.
- **The likely cause:** VS Code's settings file watcher lagged under load, the same fseventsd pressure seen in runs 2 to 4. It passed on retry. Only 3 flaky runs in a row fail it.

## Findings

**Blocking:** none.
- B1 is fixed (`apply.rs:268-275`).
- B2 is resolved by the H24 amendment, and the code matches it.

**Non blocking (new):**
- **R1** (`hook/apply.rs:272-296`, `:332-338`, `:190-199`): a question PreToolUse that lands during a stale wait is never shown as waiting once that wait ends. The case is reachable with async Pre and Post. The suggested fix and the spec question are above.
- **N18** (`hook/apply/tests.rs:291-326`): the new table is not exhaustive. It lacks a late `AskUserQuestion`, a question during a Notification wait, and the standing call ending while the question is in flight.
- **N19** (`apply.rs:196-198`): the `merge_older` call-ended branch is unit only. int could reach it with a `WT_VERIFY_PROCS` variant of `merge__should_refuse_an_older_state`. `since` (`:198`) is not asserted by any test.

**Unchanged from review 1:**
- N1, N2, N4 to N17 and the wrong checks stand as listed: `wait__should_hash_input_with_sorted_keys`, three cases of `transitions__should_match_the_event_table`, `budget__b11`, the read-once part of `budget__b6`, and `hook__should_restart_when_gc_deleted_the_lock`.
- N3 is closed.
- There is still no live cc run of this build (N11). The live test comes after install.

verdict: clean
