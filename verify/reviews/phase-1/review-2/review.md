# Verifier review 2, phase 1 (verification.md revision 10)

- **Spec read:** `docs/design/` at the phase 1 review 2 freeze (status-panel.md revision 9, verification.md revision 10).
- **Oracle unchanged since the freeze:** `git diff -- verify xtask vscode/test docs vscode/package.json vscode/package-lock.json vscode/.gitignore` from the phase 1 review 2 freeze to the phase 1 run 4 head is empty.
- **Clone and run:**
  - The clone `~/.cache/wt-verify/clones/phase-1-r4` holds the phase 1 run 4 head, and its tree is clean.
  - The run `verify/out/phase1-r4` has the verdict PASS.
  - All five parts record the phase 1 run 4 head, `mutants` included.
  - The run's `report.md` is byte for byte the copy in `verify/reviews/phase-1/run-4/verify-report.md`.
  - Every attempt in e2e and shot passed the first time. No check is flaky.
- **Product delta since review 1:** `git diff` from the phase 1 review 1 head to the phase 1 run 4 head touches only `vscode/`:
  - `scripts/build.mjs`;
  - `src/host/{actions,panel,stream}.ts`;
  - `src/html.ts`;
  - `src/webview/{early,main,panel,text}.ts` and `style.css`;
  - `package.json`, through the oracle.

  `crates/`, `Cargo.toml` and `Cargo.lock` are unchanged, so review 1's Rust review still holds.
- **Method:**
  - I read the spec, review 1, the oracle round 2 diff (the phase 1 oracle round 2 commit) and the phase 1 spec change commit.
  - I read the whole extension host and webview as of the phase 1 run 4 head, the round 4 diffs (the paused view fix, and the paused header and codicon fix), and the run artefacts.
  - I opened Main.png, Tooltip.png and States.png and every image in `shots/`.
  - I mapped each codicon code point in `style.css` to its name with `node_modules/@vscode/codicons/dist/codicon.css`.
  - I ran no build, no verify part and no VS Code, and started no process. The only file I created is this one.

## Review 1's blocking items

| Item | Resolved? | Evidence |
|---|---|---|
| B1 `flow__hide_and_show` tested the wrong thing | Yes | `vscode/test/e2e/flows-tab.spec.mjs:131-162`. The check commits in wt1 while the tab is hidden (:141-144), which W2 sees on wt1's HEAD. It then waits 2.5 s and requires `not reusable` and `1 unreferenced` within 2 s of the show (:160). The document reaches the host while the tab is hidden. Alive lines post nothing (B7) and the heartbeat prints only changes (W5, W7). So only the post on show (X7) can fill the reloaded webview in that window. It passed on the first attempt. |
| B2 W4's PostToolUse refresh | Yes, by the spec | status-panel.md revision 9 makes the second sentence "From phase 2". `verify/trace.toml:251-253` keeps W4's phase 1 checks on "recomputes only what changed". See non blocking N3 for phase 2. |
| B3 the N in "retrying in N s" was untested | Yes | Three layers now pin N. **Render:** `panel.test.mjs:219-245` checks N against `retry_at` (floor or ceil) and checks that it counts down 3 s later. **Host:** `stream.test.cjs:59-73,127-131` checks that each paused view carries `retry_at` and that no restart comes before it, and requires the restart within about a second of it. **e2e:** `flows-rows.spec.mjs:72-104` keeps N in 1..30, requires a pause with N ≥ 4, then N drops to between 1 and N - 1 two seconds later. A constant N, or "always 1 s", now fails. |
| B4 the first view could be lost before the script attached | Mitigated | `media/early.js` loads first in `<head>`, before the stylesheet (`html.ts:28`). It keeps the latest message and hands it to `main.ts` (`early.ts:13-28`, `main.ts:17-19`). The race window shrinks from "css plus js loaded" to one small script fetch, but does not close: a fetch over 200 ms still drops the message. An inline nonce script would close it. The render checks block that (`panel.test.mjs:407`, `:464`), which is the known item about inline scripts, and I do not re-raise it. |

## 1. The oracle

### Checks in the author's diff from the final phase 0 freeze to the phase 1 review 2 freeze

Review 1 covered the diff from the final phase 0 freeze to the phase 1 review 1 freeze. The only new oracle commit is the phase 1 oracle round 2 commit, plus the phase 1 spec change commit, the phase 1 round 2 acceptance commit and the phase 1 review 2 freeze. Its entries p1r2-* and p1r2-spec-decisions are all in `verify/accepted.txt`.

| Check | Spec | Verdict |
|---|---|---|
| `flow__hide_and_show` (changed) | X7 | Right; see B1 above. |
| `header__should_show_live_updated_and_paused` (changed) | X6, item 5 | Right. It accepts floor or ceil of the seconds left, which "counted down to the next restart" allows. |
| `header__should_show_paused_before_the_first_document` (new) | X6 last sentence, item 5 | Right. It posts `status: null` with `paused` and `retry_at`, then checks four things: no errors, the header counts down, one table with zero rows, and a later live document shows 2 rows. |
| `stream__should_post_paused_before_the_first_document` (new) | X6, item 5 | Right. An `early_exit` on the first start must post `paused`, `version: 1`, `status: null` and a `retry_at`, and must restart at 1 s, not before. |
| `restartAfter` / `secondRetry` in X5's fault checks (changed) | X5, X6, item 5 | Right. The `bad` filter (`stream.test.cjs:132-135`) now also proves that a paused view after the first document carries that document, not null ("null only before the first document"). The tolerance is ±1 s, because the time may hold whole seconds only. |
| `flow__connection_states` (changed) | X6 | Right; see B3 above. |
| `csp__should_match_the_template` (relaxed) | X8 | Right. X8 allows any script that carries the nonce. The check still refuses a script without the nonce, and a `src` outside `cspSource`. |
| `vscode/.gitignore` and `package.json` codicons | oracle files | Right. The patterns are rooted at `/media/` and git tracks nothing under `media/`. |

**The per tab reading of "before the first document" fits the spec.** These four points support it:
- X1 allows one tab per window, and X7 stops the watch when the tab closes. A reopened tab therefore has no current data of its own.
- Showing an earlier tab's document would label stale facts `live`. Review 1 non blocking 7 found exactly that with the old module-wide `lastKnown`.
- Item 5 says `status` is null "only before the first document". Per tab, that still holds.
- The host check enforces the reading. `resetFake` closes every editor (`host/support.cjs:89-92`), and earlier checks in the same extension host have produced documents, so a cache across tabs fails `paused.status === null`.

The product matches the reading (`host/panel.ts:51-52`).

### User flows

All 19 phase 1 flows have a passing `flow__` check in this run, and `flows__should_each_have_an_e2e_check` passed. Since review 1, only `hide_and_show` and `connection_states` changed, and both now exercise their flows. No flow of phase 2 or 3 is due.

## 2. The code

### IDs of phase 1 and earlier

- **Rust:** unchanged since the phase 1 review 1 head. Review 1's conclusions stand: S1 to S4, S7 to S9, A5, A6, R1, R2, W1 to W3, W4 (phase 1 sentence), W5, W7 to W9, R7's idle watcher. The W4 gap is gone with the spec change.
- **X6, round 4:**
  - `StatusPanel.document` starts null per tab (`host/panel.ts:52`) and is posted as `status` (`:109`).
  - Every paused view gets `retry_at` (`:111`). `WatchStream.fail` sets it to the backoff deadline (`host/stream.ts:143`).
  - The webview renders the header without rows when `status` is null (`webview/panel.ts:136-140`).
  - It counts N as the ceiling of the seconds left, clamped at 0 (`:165`).
  - `actionOf` refuses every action while there is no document (`host/actions.ts:28-29`), so X11 holds.
  - The gap in N1 below concerns only the time after a restart begins.
- **U6:** every icon is a codicon glyph from `media/codicon.ttf`, which `build.mjs:51-54` copies from `@vscode/codicons`. `style.css:7-11` declares the font, and `font-src ${cspSource}` allows it. The installed extension in the e2e run holds `media/codicon.ttf`, and `e2e_csp_clean` passed. The ten activity glyphs are distinct code points (`style.css:336-373`): home, bell, sync, rocket, circle-filled, circle-large, warning, circle (outline), question, archive. The colour tokens per group are unchanged.
- **X10:** `visible()` (`webview/text.ts:53-82`) now reads code points (`codePointAt`). It also escapes U+00AD, U+180E, U+2061-2064 and the tag block, which settles review 1 non blocking 3.
- **X2, X7, X8, X9, X13 to X17, U1 to U5, U7, D1, D3, B7, B8, B10, R8:** the parts round 4 left alone are unchanged since review 1. The parts it touched (`early.js`, the null view) keep them: `early.js` sends nothing to the host and uses no HTML sink.

### Lines no oracle layer covers

The report lists 2567 of 2567 changed lines as covered. The list of lines that only `unit` covers, or that are not measured, is identical line for line to review 1's report and to run 3's. No line in `vscode/` appears. So review 1's per line verdicts carry over unchanged, and every line is justified: test modules, schema generation, the `WT_VERIFY_CLOCK` interface, other arms of rules int covers, watch lines lost to `Watch::drop`'s SIGKILL (the known item), S5/S6 paths of phase 2, and arms that cannot be provoked through the CLI.

### Mutants

- Run on the phase 1 run 4 head itself: 251 mutants, 207 caught, 0 missed, 44 unviable.
- `verify/mutants-allowed.toml` does not exist, so there is no entry to review.

### Tests against the test standards

- The new and changed oracle tests carry `// Given`, `// When`, `// Then`, and their names follow the trace list.
- The builder added no unit tests in round 4. Review 1's notes on the Rust unit tests stand: duplicated fixture, missing `must_` prefixes, real 200 ms timing.
- File lengths:
  - No product or test file is over 1000 lines.
  - The largest product file is `vscode/src/webview/style.css` at 547 lines.
  - The largest test file is `vscode/test/render/panel.test.mjs` at 493 lines.
  - Some oracle files run past 500: `xtask/src/suppress.rs` 580, `oracle.rs` 531, `clone.rs` 528. All are under the cap.

## 3. Screenshots against the normative mockup parts

I opened every image in `shots/`: four themes of `idle_pool` and `every_activity` at 1100 px, `dark-idle_pool-600.png` and `e2e-idle_pool.png`.

- **Main:**
  - The columns and header text match: Activity, Worktree, Branch, Git, Agent · Doing, Last.
  - The row text matches: the `fg` and `unmanaged` badges, `(detached)` muted, `clean`, `2 dirty`, `↑1`, `no agent` and the ages.
  - The header has "Worktrees" and the chips, each with its codicon (ring, sync, bell). The connection reads "live" with a dot at the right.
  - The machine heading row has a desktop icon.
  - The waiting row has its highlight bar, and the selected row is filled.
  - Row actions show at the right: link-external, terminal, copy.

  All of this matches the mockup.
  - The paused header's codicon is `debug-pause` (`style.css:150-151`), which is the mockup's "‖".
  - At 600 px the branch is cut with `…`, nothing scrolls sideways, and the actions stay reachable. The column title "Agent · doing" is also cut ("AGENT · DOI…"), which U1 does not forbid.
- **Tooltip:** the detail strip shows Path, Upstream, Git and Not free. Per actor it shows the label and state, then "tool target · placed by … · since HH:MM". This matches. Review 1's minor differences remain: `Git 0 dirty`, and ahead and behind are left out without an upstream.
- **States:** each activity has its own icon and word.

  | Activity | Product icon | Mockup |
  |---|---|---|
  | foreground | home | house |
  | waiting | bell | bell |
  | working | sync arrows | sync arrows |
  | building | rocket | rocket |
  | idle | filled dot | dot |
  | free | large ring | ring |
  | not reusable | warning | triangle |
  | unused | medium ring | dashed ring |
  | unknown | circled question | circled question |
  | missing | archive | archive |

  The codicons are closer to the mockup than round 3's CSS shapes were. Building and missing now match exactly.
  - **unused:** codicons has no dashed circle. I checked `codicon.csv`: there is `circle`, `circle-small`, `circle-large`, `loading` and `dash`, but no dashed or dotted ring. So "unused" is a ring smaller than "free"'s, in the same grey. They are distinct glyphs and each has its word, so U6 ("its own icon and word") holds, and the States mockup's normative part, "icon and word per activity", is met.
  - At a glance, free and unused differ only in ring size. Round 3's CSS dotted ring was closer to the mockup, but U6 now requires codicons. Acceptable. Non blocking N4.
- **Colours:** they follow the U6 groups in all four themes. The free chip's badge colour is a known item.

## Blocking findings

None.

## Non blocking findings

- **N1. A restart that has not printed yet shows "retrying in 0 s"** (product, X6).
  - What happens:
    - `WatchStream.start()` (`vscode/src/host/stream.ts:68-90`) arms the 30 s silence restart.
    - While paused, `arm()` sets `retryAt` to that time (`:133-135`), but nothing posts.
    - Only `read` and `fail` and the stale timer call `events.changed` (`:117`, `:129`, `:147`).
    - So the visible webview keeps the old `retry_at`, which has now passed, and `webview/panel.ts:165` clamps N to 0.
  - Effect:
    - "updates paused, retrying in 0 s" shows from the restart until the new `wt` prints its first line. Normally that takes about 200 ms (B3).
    - If the restarted `wt` hangs, it shows for up to 30 s. A loaded machine, or git blocked on a slow disk, can cause this.
  - The commit message of the paused view fix says every paused view carries the 30 s silence restart. That holds only for a view posted for another reason: a show (`host/panel.ts:76`) or a stale tick.
  - This was in review 1 under B3 ("Separately, …"). It is non blocking because:
    - X6 defines `paused` as the restart backoff and says nothing of the time between a restart and its first line;
    - the window is short in the normal case;
    - the host already computes the right time.
  - Fix:
    - Call `this.events.changed()` at the end of `start()` when the connection is `paused`.
    - Or have the spec say what the header shows while a restarted `wt` has not answered.
    - No check covers this. The fix stays inside `flow__connection_states`'s bounds (N ≤ 30).
- **N2. Oracle: `flow__connection_states` rejects N = 0** (`vscode/test/e2e/flows-rows.spec.mjs:98`).
  - Between a failing restart's `retry_at` and the next paused view, the product shows 0 for a few milliseconds (N1).
  - A 1 Hz tick inside that gap, sampled by the 100 ms poll, would fail a correct product.
  - The odds are low, retries absorb them, and the check passed on the first attempt.
- **N3. Phase 2 note on W4.** The trace list names no check for W4's second sentence.
  - The phase 2 author must add one. Otherwise `trace_complete` stays green while that sentence is untested.
  - The spec should also say which record fields mark an ended edit or Bash call, as review 1 B2 noted.
- **N4. "unused" and "free" are both grey rings, told apart only by size** (`vscode/src/webview/style.css:356-365`).
  - This is within U6 and the States mockup's normative part.
  - For a stronger difference, the spec could name a glyph, for example `circle-small` or `dash`, or add a note.
- **N5. `main.ts` assumes `window.wtEarly` exists** (`vscode/src/webview/main.ts:17`).
  - If `early.js` alone failed to load, the webview would throw and show nothing.
  - Both files come from the same `media/` root, so this is unlikely.
- **N6. Review 1's non blocking findings still apply,** except these, which round 3 and round 4 settled:
  - 3 (X10 characters);
  - 7 (module-wide `lastKnown`);
  - (d) (blank tab before the first document).
- **N7. The known items listed in the brief are unchanged,** and none turned out worse:
  - `report.rs` "Budgets: None in this phase";
  - `Watch::drop` SIGKILL;
  - the weak checks for reduced_motion, W5's 200 ms and W7's rescan;
  - the free chip's badge colour;
  - a missing worktree shows "(detached)";
  - the clipboard restore keeps text only;
  - `media/early.js` stays instead of an inline nonce script.

  Of these, B4's remaining window is the `early.js` item.

verdict: clean
