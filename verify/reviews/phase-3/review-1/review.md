# Verifier review 1, phase 3 (status-panel.md revision 11, verification.md revision 14)

- **Spec read:** `docs/design/` at the phase 3 freeze, which is the same as the phase 3 review 1 head. I judged against revisions 10 and 11 of status-panel.md and revisions 13 and 14 of verification.md.
- **Clone and run:**
  - `~/.cache/wt-verify/clones/phase-2-r2` holds the phase 3 review 1 head.
  - `verify/out/phase3-r2` says PASS. Every part ran fresh and no check was flaky.
  - It reports 4792 of 4792 changed lines covered.
  - All 4 phase 3 flows, the 3 X12 host checks, `wait__should_wait_on_a_question_still_in_flight`, `flows__should_each_have_an_e2e_check`, `trace_complete`, `isolation__left_no_trace` and `oracle_unchanged` passed.
- **Scope:** I reviewed all of `git diff` from the phase 2 review 2 head to the phase 3 review 1 head:
  - the author's freeze, the phase 3 freeze
  - the oracle change p3-x12-error-paths
  - the product commits for H13 and X12, and the spawn fix
  - the status-panel.md change (revision 11)
  - Review 1 and review 2 of phase 2 stand for all unchanged code.
- **What I ran:** no verify part, no cargo, no wt and no checkout. I did three narrow things:
  - **Node spawn behaviour,** in the scratchpad with Node 26.12, to settle the spawn question (results in N21).
  - **Screenshots:** a byte comparison of the phase3-r2 shots with `verify/shots/approved/` and with phase2-r6.
  - **Coverage:** I read `cover/host.lcov`.

## 1. The author's checks against the spec, and flow coverage

| Check | Verdict |
|---|---|
| `actions__should_confirm_naming_the_worktree` (`vscode/test/host/actions.test.cjs:135-181`) | Right. One modal names wt2 and `feature-a`. Confirm runs `wt free wt2` or `wt fg feature-a` exactly once, and cancel runs nothing. |
| `actions__should_refuse_free_with_a_working_actor` (`:266-324`) | Right. It covers working, waiting, and an idle session with a working subagent. No wt runs, and the user sees a message naming wt2. The confirm-time re-judgement (`working_by_the_confirm`) is a stricter reading the lead accepted. The fixture passes an `activity` that matches the actors, so the check cannot tell whether the product reads actors or activity. That is harmless, because A4 makes the two agree. |
| `actions__should_warn_free_with_an_idle_actor` (`:326-374`) | Right. It checks a warning modal for idle and for quiet (the accepted reading that A4 counts quiet as idle), and no idle wording when the row has no actor. |
| p3-x12-error-paths cases (`:183-264`, `fake-wt.mjs` `actionStderr`) | Acceptable, see below. |
| `wait__should_wait_on_a_question_still_in_flight` (`xtask/tests/checks/hook_wait.rs:132-332`) | Right. It covers the end of a wait by Post, by Failure, by Denied, by a Batch entry, and by the end of a Notification wait (a stricter reading). It also covers a second question, and a question already answered (no wait). The older-writer variants use real processes: the older hook starts first and gets its stdin late. So they exercise `merge_older` through H24, and now reach `apply.rs:196-202` in int. |
| `wait__should_hash_input_with_sorted_keys` (moved to `hook_wait.rs:49-130`) | Now right. It sends raw stdin with keys in reverse order and with spacing, as review 1 asked. A product that hashes the raw input text would now fail it. Review 1's "wrong check" is closed. |
| `transitions__should_match_the_event_table` (the post_tool_use, post_tool_use_failure and permission_denied cases in `hook_state.rs`) | Now right. Each case starts from a bound wait and asserts that the wait was bound. Review 1's wrong check is closed. |
| `tab.test.cjs` (X11), `panel.test.mjs`, `shot/panel.spec.mjs` keyboard, `flows-exclusive.spec.mjs` (missing and narrow) | Right. They add free and fg to the set. They reject free or fg for an unknown id or with an extra field, and they require free and fg on a slot that holds a branch. They are loose: they allow free and fg on detached slots, which the product never offers. |
| Flows `flow__free_confirm`, `flow__free_cancel`, `flow__free_refused`, `flow__fg_confirm` (`vscode/test/e2e/flows-free.spec.mjs`) | Right. Each runs in a real VS Code through a row button, VS Code's in-window modal and the release `wt` on the sandbox pool. Each asserts what the user sees and what git holds afterwards. |

**p3-x12-error-paths** (the author read `confirm.ts`):
- **They test visible behaviour, not the code.** A failing `wt` shows one error that names the command and contains wt's stderr. Nothing runs again and nothing asks again. A row whose branch changed, or that vanished while the modal was open, gets a message naming wt2 and no wt runs.
- **They do not mirror the code.** They assert no wording, no "Worktrees:" prefix and no 300-character cut.
- **One choice was taken from the code:** fg refuses after a branch change, although `wt fg feature-a` would still be what the modal named. It is the safe reading of "naming worktree and branch", so I accept it.
- **Gap (N27):** no case shows that an unrelated document update during the modal still lets confirm run. A product that refused on any update would pass.

**Flows:** every flow of phase 3 or earlier (26) has a passing `flow__` e2e check.

## 2. Code against the IDs

| ID | Verdict |
|---|---|
| H13 (revision 11) | OK. `wait_on_a_question` (`crates/wt-core/src/hook/apply.rs:359-379`) runs after `end_call` / `end_listed` / `end_notified_wait` in End (`:93-105`) and Batch (`:106-116`), and in the call-ended branch of `merge_older` (`:198-203`). It binds the oldest `AskUserQuestion` or `ExitPlanMode` call in `in_flight` by id. An ended call is out of `in_flight`, so it never binds. A standing wait is never replaced. Review 2's R1 is resolved. Small leftover: N25. |
| X12 | Logic OK (`vscode/src/host/confirm.ts:78-90`). Free refuses on a working or waiting actor before asking and again at confirm. It warns for idle or quiet. The modal names the worktree and the branch. It runs `wt free <name>` or `wt fg <branch>` with an argv array and no shell. The visual integration is **B1**. Open spec gap: N20. |
| X11 | OK. `actionOf` accepts free and fg only for `holdsBranch` rows (`actions.ts:31-38`). |
| X4, R8 | OK. Changes ask first. `wt` runs from the setting through `spawn` with `shell: false`. Dialog text goes through `plain()`. N23 is about the toasts. |
| U4, X14, X13 | OK. The buttons have `aria-label` and `title`, Tab reaches them (shot check), axe passes, and the colours are `--vscode-*`. The column width is B1. |
| All other IDs | Unchanged since the phase 2 review 2 head. Review 1 and review 2 stand. |

**Lines no oracle layer covers:**
- `apply.rs:191, 246, 311` (unit only) are review 1's `:188, 242, 307`, shifted by the new lines. They stay justified.
- `apply.rs:196-202` are now covered by int, so N19 is closed.
- No vscode line is reported uncovered.
- Two callbacks sit on lines that count as run only because their listener is registered there: the `.catch` at `panel.ts:94` and the `'error'` handler at `confirm.ts:72`. No check runs either one (N21).

**Real setup:** the extension now runs the real `wt free` / `wt fg` on the owner's pool, only after a modal. wt itself refuses:
- a dirty foreground or holder
- an operation in progress
- freeing the foreground

The e2e flows ran only on sandbox pools, and isolation passed. The sandbox `window.dialogStyle: custom` goes only into the sandbox user-data dir. I found no new risk beyond N20.

**Test standards:**
- The host and e2e checks have `// Given` / `// When` / `// Then` and `<sut>__should_` names.
- The Rust cases are rstest rows.
- `merge_older__should_end_a_wait_whose_call_the_older_writer_ended` branches on `question` and computes `since` in the test body (`apply/tests.rs:396-418`). That is a minor style point (N28).
- No file is over 500 lines.

## 3. Screenshots against the normative mockup parts

**The fixed screenshots are unchanged.**
- All 14 approved shots in `phase3-r2/shots/` are byte-identical to `verify/shots/approved/` and to phase2-r6. Phase 3 adds no new fixed set, because Visual review names none for phase 3.
- **So nothing new needs approval, unless B1 is fixed by widening the column.** That fix would change every approved shot and need a new baseline.
- No fixed shot shows a hovered or focused row, so the fixed set cannot show the phase 3 action bar.

**The unapproved `dark-every_activity-1100.png` does show it.**
- In phase2-r6, the focused slot row wt1 showed "30 s" in Last, then its 3 buttons.
- In phase3-r2, its 5 buttons cover the Last cell.
- The e2e shots differ from r6 only by run-to-run variance: they are identical to r5 and r1.

## Findings

### Blocking

**B1. Hovering or focusing a slot that holds a branch hides its Last ages**
- **Where:**
  - `vscode/src/webview/style.css:392-420`: `.actions` / `.action-bar`, absolutely positioned and "grows left over the Last column".
  - `.col-actions` stays at 88 px (`:192-194`).
  - `vscode/src/webview/row.ts:111-129`.
- **What breaks:**
  - The normative Main mockup (columns, row text, row actions) shows the focused slot row app-wt2, which holds a branch, with its Last ages "now / 12 m" beside its three actions. U1 lists Last as a column.
  - The build shows five 24 px buttons, 120 px plus an 8 px inset, in an 88 px column. They cover the age on exactly the rows phase 3 adds actions to.
  - That age is the fact the idle warning asks the user to weigh before freeing.
  - At 600 px the bar also reaches into the Agent column.
- **Why no check saw it:** no check or fixed shot looks at a hovered or focused row (N27).
- **Fix options:**
  - **Widen the actions column to 128 px.** This changes the locked baseline, so the shots need a new approval (an oracle change). It is likely why the builder overlaid instead.
  - **Accept the overlay** as a spec amendment.
- **Check I think wrong:** none. This is a gap in what the oracle shoots.

### Non blocking (new)

- **N20. fg from a busy slot: a spec gap.**
  - Only free refuses (X12). `wt fg <branch>` detaches the slot that holds the branch (`crates/wt-core/src/pool.rs:146-153`). `refuse_busy_holder` refuses only a dirty slot or one mid operation (`:714-724`).
  - So fg from the panel can detach a clean slot under an agent that is working there. Its next commits go onto a detached HEAD. They are not lost: wt refuses to free unreferenced commits, and the reflog keeps them.
  - The fg modal does not mention the agent (`confirm.ts:45-55`).
  - Suggestion: fg warns or refuses like free does.
- **N21. A synchronous spawn throw (the lead's question): acceptable.**
  - **What can throw:** only invalid argument values, such as a NUL in `wt.path`. I confirmed that Node 26 throws `ERR_INVALID_ARG_VALUE` synchronously for this.
  - **How it is reached:** the watch uses the same binary, but `binary()` is read at each start and the watch keeps running after a settings change. So it takes changing `wt.path` to an invalid value while the tab shows rows.
  - **What happens:** nothing runs, nothing is destroyed, and the user sees no message, only the log (`panel.ts:94`).
  - **The `'error'` handler at `confirm.ts:72`** (ENOENT, EACCES) is also never run by a check, for the same same-line reason. Node emits `error` then `close` with no `exit`, so the user gets one message.
  - Suggestion: show an error in the `.catch` too, and add a missing-binary host case.
- **N22. `runWt` resolves on `'exit'`, not `'close'`** (`confirm.ts:73`).
  - The stderr pipe can still hold data when `exit` fires, so the error can read "exit 1" without wt's reason.
  - There is also no timeout: a hung `wt fg` (for example a fetch) gives no feedback.
- **N23. Toasts carry document and wt text** (`confirm.ts:28-30, 63, 85`).
  - VS Code notifications render markdown links, which conflicts with R8's "plain text".
  - Branch names cannot contain `[`, but folder names and wt's stderr can.
  - Low risk, because free and fg exist only on wt-named slots. Escaping `[` and `]` would close it.
- **N24. A branch named only with digits, or `#digits`, is read by `wt fg` as a PR** (`crates/wt-core/src/refs.rs:79-81`, called from `confirm.ts:89`). fg would then check out another branch than the modal named. This is a rare edge case, a candidate for a spec amendment.
- **N25. `since` after a permission wait becomes a question wait differs by path.**
  - Direct apply keeps the permission wait's `since`, because the state stays `waiting` (`apply.rs:149-152`).
  - `merge_older` sets it to `updated` (`:202`), and the unit test locks that in.
  - The spec does not define `since` here. U5 shows it.
- **N26. Shared icons.** The free and fg buttons reuse the legend glyphs for the free and foreground activities (`style.css:462-468`, same as `:357` and `:337`). A reader may take them for activity icons.
- **N27. Gaps in the checks.**
  - The changed-row cases (`actions.test.cjs:215-264`) never show that an unrelated update lets confirm run.
  - `flow__free_refused` (`flows-free.spec.mjs:177-209`) relies on `working` not decaying to `quiet` within 120 s during setup, which is a flake risk.
  - Visual review shoots no focused slot row, which is how B1 got through. Spec suggestion: add one, for example `every_activity` at 1100 px.
- **N28. Test style.** `apply/tests.rs:396-418` uses logic in the test body instead of case columns for `question` and `since`.

### Closed since phase 2

- R1 is closed by revision 11 and the H13 fix.
- N18 is closed: the late AskUserQuestion, the question during a Notification wait and the standing call ending while the question is in flight are all now rows.
- N19 is closed: the branch is reached in int, and `since` is asserted.
- Two of review 1's wrong checks are fixed: `wait__should_hash_input_with_sorted_keys` and the three transitions cases.

### Unchanged from review 1 and review 2

- N1, N2, N4 to N17 stand. N11 stands too: by design there is no live cc run.
- These wrong checks stand, all non blocking: `budget__b11`, the read-once part of `budget__b6`, and `hook__should_restart_when_gc_deleted_the_lock`.

**Screenshots to approve:** none, as phase 3 added no fixed screenshot set. If B1 is fixed by widening the actions column, the 14 approved shots change and need a new approval.

verdict: not clean
