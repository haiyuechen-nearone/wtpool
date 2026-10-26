# Verifier review 2, phase 3 (status-panel.md revision 12, verification.md revision 15)

- **Spec read:** `docs/design/` at the phase 3 freeze. Where the docs changed on 2026-10-25, I judged against the phase 3 review 2 head: status-panel.md revision 12 (X12, U4) and verification.md revision 15 (Visual review).
- **Clone and run:** `~/.cache/wt-verify/clones/phase-2-r2` holds the phase 3 review 2 head, and `verify/out/phase3-r4` says PASS.
  - Every part ran fresh. Nothing failed and nothing was flaky.
  - Cover: 4817 of 4817 changed lines are covered. The list of lines no oracle layer covers is the same as in run 2, and it holds no vscode line.
  - These passed: `actions__should_guard_fg_by_the_holding_slot` (host), `actions__should_not_cover_another_column` (shot), the 4 phase 3 flows, `flows__should_each_have_an_e2e_check`, `trace_complete`, `isolation__left_no_trace` and `oracle_unchanged`.
- **Scope:**
  - All of `git diff` from the phase 3 review 1 head to the phase 3 review 2 head:
    - the product commits: the actions column fix and the spawn error fix
    - the oracle changes p3-review1-fg-and-overlap, p3-spawn-throw, p3-spec-rev12 and p3-visual-baseline
    - the docs changes
  - Review 1 stands for everything unchanged since the phase 3 review 1 head. That includes its verdicts on the author's own checks in from the phase 2 freeze to the phase 3 freeze, which this diff does not touch.
- **What I ran:**
  - No verify part, no cargo, no wt, no checkout and no VS Code.
  - A byte comparison of the phase3-r4 shots with `verify/shots/approved/`.
  - A diff of the run 2 and run 4 lists of uncovered lines.
  - A look at the 16 approved shots, the Main mockup and the phase 3 review 1 head's version of `dark-permission_wait_and_answer-600`.

## 1. The new checks against the spec, and flow coverage

| Check | Verdict |
|---|---|
| `actions__should_guard_fg_by_the_holding_slot` (`vscode/test/host/actions.test.cjs:307-433`, which shares its cases with free) | **Right.** It tests X12 revision 12: fg of wt2's branch refuses while an actor in wt2 works or waits, and it refuses again at confirm (`working_by_the_confirm`). It warns with a warning modal naming wt2, the branch and idle or quiet. It gives no idle wording without actors. The fixture puts the actors on wt2 only, so a product that guarded the foreground or any other worktree would fail. It asserts visible behaviour, not the code's wording. |
| `actions__should_confirm_naming_the_worktree` `{free,fg}_spawn_throws` (`actions.test.cjs:216-254`) | **Right.** It covers R8, X4 and X12 for a `wt` that cannot start: one error naming `wt free wt2` or `wt fg feature-a`, one modal only, and the tab keeps wt2. The "no wt started" assertion cannot fail with a NUL path, but the other assertions carry the check. It changes `wt.path` only in the host's own `--user-data-dir` (`vscode/test/host/run.mjs:86-92`) and restores it in `finally`. This poses no risk to the owner's settings. |
| `actions__should_not_cover_another_column` 1100 and 600 (`vscode/test/shot/overlap.spec.mjs`) | **Right.** It tests U4's new sentence on every row of `every_activity`, hovered, focused and with its last action focused, at both widths. It requires all five actions on the slots that hold a branch, so it cannot pass by hiding them. It would fail on the overlay of the phase 3 review 1 head. It also writes the revision 15 baseline `dark-every_activity_focused_slot-{1100,600}` and compares it to `verify/shots/approved/` (`playwright.config.mjs:18`). One small gap is N31. |
| Refactor of the free refuse and warn cases into the `guarded` table | **Right.** The free cases keep their names and assertions. |

**Flows:** every flow of phase 3 or earlier (26) has a passing `flow__` e2e check. Revision 15 adds no flow, so fg refusing or warning is checked in host only. That is allowed, because the flow table does not list it.

## 2. Code against the IDs

| ID | Verdict |
|---|---|
| X12 (revision 12) | **OK.** **N20 is resolved.** `confirm()` (`vscode/src/host/confirm.ts:105-116`) refuses both verbs on `busy` before it asks (`:106`) and again at confirm (`:114`). For fg the row is the slot that holds the branch (`actions.ts:31,36-38`). `askFg` warns for idle or quiet (`:58-67`), and its detail now says that wt fg detaches the slot. The refusal names the branch and the slot (`:40-44`). A spec gap remains: N29. |
| U4 (revision 12) | **OK.** **B1 is resolved.** The actions are now an ordinary table cell. `.col-actions` is 128 px, or 124 px at 600 px (`style.css:192-194`, `:544-546`). The absolute `.action-bar` is gone (`style.css:392-410`, `row.ts:111-128`). Five 24 px buttons fit: at 600 px, 124 px minus the 4 px right padding leaves 120 px. The focused shots show Last ("34 s") beside all five actions at both widths. |
| R8, X4: spawn throw (review 1 N21) | **Closed.** The `try`/`catch` around `spawn` (`confirm.ts:84-89`) shows the same error as `'error'`, and a `done` flag stops two messages (`:74-77`). The `.catch` at `panel.ts:95` now handles only unexpected errors. |
| N22 (`'exit'` and `'close'`) | **Half closed.** `runWt` resolves on `'close'` (`confirm.ts:95-100`), so the error holds all of wt's stderr, and a signal is named. There is still no timeout for a hung `wt fg`. |
| N23 (toast text) | **Fixed in code.** `toast()` (`confirm.ts:32-38`) turns `[` and `]` into fullwidth brackets in every error notification, and every notification goes through `error()`. Modal dialogs keep `plain()`, which is fine because a modal renders no markdown. No check asserts the bracket escape (N33). |
| All other IDs | Unchanged since the phase 3 review 1 head. Review 1 stands. |

**Lines no oracle layer covers:**
- The list is identical to run 2: the `config_file.rs`, `apply.rs:191/246/311` and `bash.rs` lines are unit only, and the test files are not measured.
- No vscode line is in it.
- The 2 lines that failed run 3 (the spawn throw) are now covered by host, through a check that tests the spec.

**Real setup:**
- The extension still runs real `wt free` and `wt fg` only after a modal, and now refuses or warns for fg too.
- Every new test runs in sandboxes: a host with its own user data dir, and Chromium on `buildHtml`. Isolation passed.
- The only new real-setup question is N29, and it is non blocking.

**Test standards:**
- The new cases have `// Given` / `// When` / `// Then` and `<sut>__should_` names.
- `actions.test.cjs` is 437 lines and `overlap.spec.mjs` is 147 lines.
- `style.css` shrank from 580 to 557 lines. That is over the soft limit of about 500 but under the hard cap of 1000.
- The verb loops keep each title and assertion explicit.
- One minor style point: `overlap.spec.mjs:128` shoots the baseline from inside a conditional in the When block.

## 3. Screenshots against the normative mockup parts

- **14 re-approved shots:** byte-identical between phase3-r4 and `verify/shots/approved/`.
- **2 focused-row shots:** not byte-identical, but within 0.1 %, because the shot check compared them and passed.
- **Against the Main mockup (columns, row text, icons, highlight, chips, header, row actions):**
  - The focused slot row wt2 shows its row text, its Last age right-aligned, and then its row actions. That is the same order as the mockup's focused app-wt2. The mockup shows three actions, and the build shows five on a slot that holds a branch. The extra two are X12's free and fg.
  - The focus border is visible.
  - At 1100 px no column text is cut. Columns, chips, header and highlight bar are unchanged from the shots approved in phase 2, apart from the actions column taking 40 px more.
  - At 600 px no action covers text. The cost is N32.
- **Nothing new needs approval:** all 16 shots are approved (p3-visual-baseline).

## Findings

### Blocking

None. B1 (actions covered Last) is resolved by U4 revision 12, the actions column fix and a check that tests the amended spec. N20 is resolved by X12 revision 12, the fg guard and a host check that tests the amended spec.

### Non blocking (new)

- **N29. fg can switch the branch under an agent working in the foreground.** A spec gap.
  - `wt fg` runs `git switch <branch>` in the foreground (`crates/wt-core/src/pool.rs:156-159`). It refuses only a dirty foreground, one in the middle of an operation, or one with commits on no branch (`:119-128`).
  - The panel guards only the slot that holds the branch (`confirm.ts:106,114`). So a clean foreground with a working agent has its branch changed under the agent after one confirm. The `foreground_with_agent` scenario shows that this state exists.
  - Nothing is lost, because the commits go to the new branch. It is still a surprise.
  - Revision 12 deliberately names only the holding slot. Suggestion: X12 also warns or refuses on the foreground's actors for fg.
- **N30. Doc drift in verification.md.** Its check catalog still lists only the old checks for X12 (`docs/design/status-panel/verification.md:271`) and for U4 (`:273`). `verify/trace.toml` and `checks.toml` name the new checks `actions__should_guard_fg_by_the_holding_slot` and `actions__should_not_cover_another_column`, so `trace_complete` is unaffected.
- **N31. A gap in the overlap check.** `actions__should_not_cover_another_column` skips text that is hidden or clipped away (`overlap.spec.mjs:55,57-65`). A product that hid the Last or Agent text while actions show would pass it. Only the dark wt2 focused baseline would catch that. The check is not wrong.
- **N32. The 600 px cost of a fixed 124 px column.**
  - The actions column is reserved even when no row offers five actions.
  - In `dark-permission_wait_and_answer-600` the header now reads "AGENT · …" instead of "AGENT · DOI…". The agent text shrinks from "push the fix ·…" to "push the …", and Branch "(detached)" becomes "(detache…".
  - U1 allows cutting the branch with … when narrow, and these shots are approved.
  - A possible fix: size the column to the widest action set in the document.
- **N33. N23's escape is untested.** No check sends a `[label](command:…)` name or stderr through a toast.

### Closed since review 1

- **B1:** closed.
- **N20:** closed.
- **N21:** closed.
- **N22:** the `'close'` half is closed. The timeout half stands.
- **N23:** fixed in code, see N33.
- **N27:** the "no focused slot row in Visual review" part is closed by revision 15. Its other two points stand: no case shows that an unrelated update lets confirm run, and `flow__free_refused` has a flake risk.

### Unchanged from review 1

- N24, N25, N26 and N28 stand.
- N1, N2, N4 to N17 stand, and N11 still stands by design.
- The standing wrong checks stay non blocking: `budget__b11`, the read-once part of `budget__b6`, and `hook__should_restart_when_gc_deleted_the_lock`.

**Checks I think wrong:** none new.

**Screenshots to approve:** none. The 16 approved shots match the run and the normative mockup parts.

verdict: clean
