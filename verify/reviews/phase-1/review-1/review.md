# Verifier review, phase 1 (verification.md revision 9)

- Spec read: `docs/design/` at the phase 1 review 1 freeze (status-panel.md revision 8, verification.md revision 9).
  `git diff -- docs verify xtask vscode/test vscode/package.json` from the phase 1 review 1 freeze to the phase 1 review 1 head is empty.
- Clone: `~/.cache/wt-verify/clones/phase-1-r2` at the phase 1 review 1 head, clean tree.
- Run: `verify/out/phase1-r2`, verdict PASS. The parts `quick`, `int`, `ui` and `bench` record commit
  the phase 1 review 1 head. `parts/mutants.json` records the p1-host merge.
- **Mutants copy confirmed.** `git diff -- crates Cargo.toml Cargo.lock` from the p1-host merge to the phase 1 review 1 head is empty.
  The whole diff from the p1-host merge to the phase 1 review 1 head touches only `vscode/src/webview/` (6 files). Mutants run only
  over `crates/`, and the unit and int tests are unchanged, so the result of the p1-host merge (251 mutants: 207
  caught, 0 missed, 44 unviable) holds for the phase 1 review 1 head. `verify/mutants-allowed.toml` does not exist, so
  there are no entries to review.
- Oracle changes since the first phase 1 freeze are p1-host-coverage, p1-host-fault-order,
  p1-gitignore-css and p1-mutants-jobs. All four are accepted in `verify/accepted.txt` and frozen
  into the phase 1 review 1 freeze. Deferred entries are not treated as findings.
- Method: I read the spec, the report, the oracle diff from the final phase 0 freeze to the phase 1 review 1 freeze, the product code and the
  run artifacts. I opened the mockups and screenshots. Parts of the reading were done by four
  read-only helper passes; I re-checked every finding below that is classed blocking against the
  source, and the line numbers are mine. I ran no build, no verify part and no VS Code, and I
  started no process. The only file I created is this one.

## Known items (a) to (d)

**(a) `flow__hide_and_show` passing only on a retry: the check is wrong, not the product.**
`verify/out/phase1-r2/e2e/results.json` gives the failed first attempt (9.8 s) as
`wt1 is not current within 2 s of showing the tab (2103 ms)`. The rows were there, but stale:
wt1 still showed `free` and `clean`, where `not reusable` and `1 dirty` were expected. So the
document posted on show did arrive and render. The race named in the brief would leave the table
empty instead (see B4 below, which is real but is not what failed here).

The cause is the change the check makes. `vscode/test/e2e/flows-tab.spec.mjs:139` writes an
untracked file into wt1. That touches no folder W2 watches; the product watches only the common git
directory (`crates/wt-core/src/status/watch.rs:167`). Under R6 and W7 such a change shows "within
one 10 s heartbeat". The check gives it about 5 s: `sleep(3000)` at :140, then at most 2 s after
showing at :149. It passes only when a heartbeat falls inside that window, roughly half of the
attempts. When the heartbeat falls in the 2 s after showing, the check also passes without testing
"showing it posts the last document at once" at all. The retry that passed took 24.3 s.

**(b) Unicode glyphs and CSS shapes instead of codicons: acceptable.** Each of the ten activities
has its own `::before` icon (`vscode/src/webview/style.css:314-373`), the icon span is
`aria-hidden` and holds no text, and every state also shows its word. That meets U6, X14 and Test
interfaces item 6. `legend__should_cover_every_activity_value` checks that the icons are distinct
and empty of text. The screenshots show the icons are recognisable next to the States mockup (see
section 3). The remaining risk is that the glyphs depend on the system fonts.

**(c) `retry_at` on paused views: a sensible addition, but it shows a spec gap.** X6 requires the
N in "retrying in N s", and item 5's fields cannot carry it. So the addition
(`vscode/src/host/panel.ts:25,115`, `vscode/src/webview/panel.ts:20,158`) is needed, and it
breaks nothing, since item 5 does not forbid extra fields. Two consequences:
- No layer checks N (blocking B3).
- Without `retry_at`, or once it is in the past, the webview shows "retrying in 1 s" for as long as
  the view lasts (`webview/panel.ts:158`: `Math.max(1, ... || 1)`). This also happens in
  production. `WatchStream.start()` clears `retryAt` (`host/stream.ts:69`) but posts nothing, so
  after a restart that hangs, the header reads "retrying in 1 s" for up to 30 s.

Item 5 should list `retry_at`.

**(d) A fault on a tab's very first line: the product's choice is reasonable, but leaves a blank
tab.** `fail()` posts, but `post()` returns early when there is no document yet
(`host/panel.ts:105-106`). Item 5 requires a `status` document, so nothing can be sent. The tab
shows only the table header, and the reason goes only to the "Worktrees" output channel. The most
likely real causes are a wrong `wt.path` (for example `~/...`, which no shell expands) or the old
bash `wt`, which rejects `status`. Both give a silent blank tab that retries every 30 s. Non
blocking. The spec should say what the tab shows before its first document, for example a
`paused` view with `status: null`.

## 1. Checks in the author's diff from the final phase 0 freeze to the phase 1 review 1 freeze, and flow coverage

Every check that the trace list names for a phase 1 ID exists, is registered in its layer, and
passed. The phase 1 scenarios match their catalog lines (phase, covers plus R3 and U1, and the
expected worktree and activity rows). The int runner (`xtask/tests/checks/scenarios.rs:19-81`)
really applies the git and process steps, and compares each expected document strictly: schema,
pool fields, every worktree field, and reasons as an exact set. Between them, the scenarios pin
all 8 S9 reasons. `flows__should_each_have_an_e2e_check` (`xtask/src/flows.rs`) reads the frozen
table and fails on a missing, failing or unknown `flow__` check. Each of the 19 phase 1 flows has a
passing `flow__` check that drives the UI through the palette, webview clicks and keys, or the
settings file.

Checks I think wrong or incomplete:

- **`flow__hide_and_show`** (`vscode/test/e2e/flows-tab.spec.mjs:130-155`): wrong. See (a) and
  blocking B1.
- **W4 has no check for its second sentence** (`xtask/tests/checks/watch_changes.rs:63`,
  `:114`): blocking B2.
- **X6's N is never asserted.** `vscode/test/render/panel.test.mjs:230` matches only
  `/^updates paused, retrying in \d+ s$/`. `vscode/test/e2e/flows-rows.spec.mjs:68` captures N but
  never asserts it. The host stream tests check only `connection === 'paused'`. Blocking B3.
- `flow__reduced_motion` (`vscode/test/e2e/flows-look.spec.mjs:97-123`) cannot fail in phase 1.
  It uses `building_without_agent`, but the only animation is on working rows (`style.css:400`),
  and there is no run with reduce motion off to compare against. Shot
  `motion__should_stop_the_spinner` does have that comparison, so X14 is covered. Non blocking:
  switch the flow to a working scenario in phase 2.
- `detail__should_show_every_fact_and_actor` (`vscode/test/render/panel.test.mjs:183-200`): two
  items are hollow. The reason `dirty` is a substring of the git text `3 dirty`, and `13:41` also
  occurs in the label `session 13:41`. Other items (`branch`, `13:47`) still catch a strip without
  reasons or local times. Non blocking.
- `render__should_escape_and_isolate_controls` (`panel.test.mjs:378`, `:386-389`): the raw
  character scan leaves out `\n`, `\t` and U+061C. The isolation test accepts any descendant with
  `dir` or `<bdi>`. Non blocking.
- `oversize_line` tests only 1 MiB + 16 bytes (`vscode/test/fake-wt.mjs:138`), so a limit far
  below 1 MiB would pass. `messages__should_ignore_everything_outside_the_set` never sends an id
  that was a row only in an earlier document. `stall_30s` allows 31 s
  (`vscode/test/host/stream.test.cjs:196-199`). Non blocking.
- W5 (`watch_changes.rs:209-233`) does not pin 200 ms: any trailing merge from about 15 ms to
  about 900 ms passes, and no case checks that events more than 200 ms apart give two documents.
  Non blocking.
- W7 `watch__should_recompute_on_rescan` (`watch_changes.rs:285`) replaces `wt-activity/`, and
  never raises a rescan flag. The product's rescan branch (`watch.rs:224-238`) is covered by unit
  only. The flag is hard to provoke through the CLI, so this is non blocking.
- W9 and B5 (`watch_changes.rs:370-372`, `xtask/tests/checks/bench.rs:195-197`) never assert that
  the first round logged any git start, so a git run that bypasses the logging shim would pass.
  `status_files.rs:156` mitigates this for the shared runner. Non blocking.
- R2 and S9 use the product's own `wt ls` and `wt bg` as the reference (`status.rs:191`,
  `status_facts.rs:297`). The scenarios' fixed values pin most facts independently. Cherry-pick,
  revert and am are never produced (`xtask/src/pool.rs`). No case has a detached, clean, unmanaged
  worktree. Non blocking.
- S2's fsmonitor flag is checked only on `--json` (`status_files.rs:137`), not on `--watch`.
  S4's "without lsof" is not enforced. No check adds a worktree while the watch runs (W2). Nothing
  checks that the Rust watcher does not poll (D2). The product is right on all four, so these are
  non blocking.
- The exclusive e2e flows write the system clipboard and restore text only (`flows.mjs:95-103`).
  An image, or rich content the owner copied during a run, is lost. Non blocking.

Budgets B3, B4, B5, B7, B8 and B10 measure what the spec says. The bench pool is a bare
`--no-hardlinks` clone of `~/code/app` with its remote removed (`bench.rs:42-54`), and every
`wt` gets sandbox `HOME`, `WT_FOREGROUND` and `WT_SLOT_DIR`. I found no risk to the real pool.

## 2. Code against the IDs of phase 1 and earlier

### Rust (`crates/wt-core/src/status/`, `crates/wt/src/`)

The following are correct:
- **S2:** every git call gets `--no-optional-locks -c core.fsmonitor=false` (`run.rs:172`, set
  before settings load at `main.rs:311`). git's stdin is null. There are no writes or sockets.
- **S4:** `procs.rs` reads processes through sysinfo's `proc_pidinfo`, with no `unsafe` and no
  lsof. It matches the names `cargo`, `cargo-*` and `rustc`, plus `build_processes`, and assigns a
  process to a worktree by path components. The real reading is exercised by int: the case
  `real_process` of `builds__should_match_cargo_star_and_rustc` runs without `WT_VERIFY_PROCS`.
- **S9/A5:** the same rule as `wt bg`. All 8 reasons, in spec order. "Git cannot tell" gives
  `unknown`, which makes a slot `not_reusable`.
- **S1, S3, S7, S8, A6, R1, R2:** correct.
- **W1, W2, W3, W5, W7, W8, W9, and R7's idle watcher:** correct. W2 watches all of `<common>`
  recursively with FSEvents and filters by `classify`. That is broader than the four named folders,
  but it is one stream and the extra events are dropped.

Gap: **W4's PostToolUse refresh and its once-a-second cap are not implemented** (blocking B2).

Nothing puts the owner's real pool at risk: no writes, no `index.lock`, no recursion into `target/`,
bounded memory, no busy loop. The existing pool commands are unchanged apart from the optional
`build_processes` key, and `version.rs` is a pure move.

### Extension (`vscode/src/`)

X1, X2, X3, X4, X5, X8, X9, X11, X13 to X17, U1 to U7, D1, D3, B7, B8, B10 and R8 are
implemented as specified. In particular:
- The CSP equals the template.
- `spawn` uses `shell: false` and an argument array.
- Messages are accepted only with exactly the keys `id` and `type`.
- `retainContextWhenHidden` is off.
- Each group's colour token is exactly as U6 lists it.
- The age thresholds are correct.

Gap: **the first view after a show or a reload can be lost** (blocking B4).

### Lines no oracle layer covers

The report lists 2519 of 2519 changed lines as covered. None is in `vscode/`. Lines marked
"(not measured)" are all in test files and `lib.rs`.

| Lines (unit only) | Verdict |
|---|---|
| `config_file.rs:231,244`, `run.rs:202-218`, `document.rs:272-318`, `mod.rs:79-136`, `procs.rs:156-248`, `time.rs:153-162` | Justified: inside `#[cfg(test)] mod tests` (document.rs:268, mod.rs:72, procs.rs:128, run.rs:197, time.rs:93, config_file.rs:192). |
| `fixture.rs` (85 lines) | Justified: `#[cfg(test)]` module (mod.rs:6). |
| `schema.rs` (181 lines) | Justified: schema generation, run by the example and the unit test that compares it with the frozen schema. `schema__generated_files_current` covers the generated output. |
| `mod.rs:53`, `time.rs:79-90` | Justified: the `WT_VERIFY_CLOCK` test interface. No phase 1 check sets a clock; W6 is phase 2. |
| `time.rs:30-37` | Justified, but used only by tests: could be `cfg(test)`. |
| `facts.rs:76,96,126,128,129` | Justified: other arms of rules int already covers (upstream count failure, unreferenced unknown, am, cherry-pick and revert names), each in a unit table. |
| `records.rs:102-115,145,147`, `state.rs:92,102,110-144`, most of `watch.rs` | Justified: int watch checks run these paths and assert their effects within 1 s. Coverage is lost because `Watch::drop` SIGKILLs the instrumented `wt` (`xtask/tests/support/mod.rs:466-471`), so no profile is written. Oracle non blocking: close stdin and wait in `drop`. |
| `records.rs:190,193,200,208` | Justified: S5 and S6 skip paths, phase 2. |
| `watch.rs:159-161,179-182,237-238,305` | Justified: watcher start error, notify error, the FSEvents rescan flag and a defensive arm. They cannot be provoked through the CLI; unit tests cover them. |
| `main.rs` (2 lines), `version.rs:23,29` | Justified: a usage error path, a line dead in the verify build, and version arms for builds without a commit or features. |

### Test standards

The builder's unit tests mostly conform: `rstest` tables, `<sut>__should_` names, bare
Given/When/Then, and files under 520 lines. Exceptions, all non blocking:
- `facts/tests.rs:265` copies `fixture.rs:14`.
- `worktree()` (`state/tests.rs:5`) and `at()` (`records/tests.rs:15`) panic but lack the `must_`
  prefix.
- The loop tests use real 200 ms timing.
- Some doc comments carry phase context (`document.rs:125-126`, `facts.rs:163-164`).

The oracle's scenario tests (`scenarios.rs:225-263`) lack Given/When/Then.

## 3. Screenshots against the normative mockup parts

I opened Main.png, Tooltip.png and States.png, and the run's `shots/` folder: four themes of
`idle_pool` and `every_activity` at 1100 px, `dark-idle_pool-600.png` and `e2e-idle_pool.png`.

- **Main, normative parts:**
  - Columns: Activity, Worktree, Branch, Git, Agent · Doing, Last, in that order with the same
    header text.
  - Row text: `fg` and `unmanaged` badges, `(detached)` muted, `clean`, `2 dirty`, `↑1`,
    `no agent`, ages.
  - Header: "Worktrees", chips, and "live" with a dot at the right.
  - Machine heading row with its icon.
  - Waiting highlight bar on the left edge and the selected row fill.
  - Row actions at the right on focus.

  All match. The 600 px shot cuts the branch with `…`, keeps every column, and has no sideways
  scroll.
- **Tooltip, normative parts:** the detail strip shows Path, Upstream, Git, Not free, and per actor
  the label and state, then "tool target · placed by … · since HH:MM". This matches. Two minor
  differences: a clean worktree reads `Git 0 dirty`, and without an upstream ahead and behind are
  left out. Non blocking.
- **States, normative parts:** each activity has its own icon and word. In the product:

  | Activity | Product icon |
  |---|---|
  | foreground | house |
  | waiting | bell |
  | working | circular arrow |
  | building | hammers |
  | idle | dot |
  | free | ring |
  | not reusable | warning triangle |
  | unused | dotted ring |
  | unknown | circled question mark |
  | missing | boxed minus |

  All ten are distinct, and each matches the meaning of the mockup icon. Missing (boxed minus
  against the mockup's archive box) and building are the weakest likenesses. Non blocking.
- Colours follow U6 groups in all four themes. A non-normative observation: `.chip-free` uses
  `--vscode-badge-background` (`style.css:62-65`). That is grey in `verify/themes/dark.json` but
  blue in VS Code's default Dark Modern theme, as `e2e-idle_pool.png` shows. So in the owner's editor
  the free chip has the active group's hue. Non blocking.
- A missing worktree, and one whose HEAD git cannot read, has `branch: null` and shows
  `(detached)` (`vscode/src/webview/row.ts:69`; see `every_activity` row wt8). U1 does not say what
  a missing row shows, and that row is not in the normative part of Main. Non blocking, but
  misleading: the spec could add `(missing)` or an empty branch.

## Blocking findings

**B1. Wrong check: `flow__hide_and_show`** (`vscode/test/e2e/flows-tab.spec.mjs:139-149`).
- The check changes wt1 with an untracked file. The spec shows that change only on the 10 s
  heartbeat (W2, W7, R6), but the check allows about 5 s.
- So the user flow `hide_and_show` (X7) has no check that reliably exercises it.
- It fails about every other attempt. A run will eventually fail on "flaky in 3 runs in a row"
  with a correct product.
- When it passes late, it does not test "posts the last document at once".
- Fix: make a watched git change while the tab is hidden (for example `git add` and commit, as
  `flow__live_update_keeps_place` does), wait about 2 s, show the tab, and require current rows
  within about 1 s of the frame appearing.
- Route: oracle change.

**B2. W4's second sentence is untested and not implemented.** W4 is phase 1 in the trace list.
- Its checks (`watch_changes.rs:63`, `:114`) cover only "recomputes only what changed". No check
  writes a record whose call ended with an edit tool or Bash and then expects that worktree's git
  to refresh, and none tests "once a second at most".
- `grep -rni "posttooluse\|\"Edit\"\|\"Bash\"" crates/wt-core/src/status/` finds nothing outside
  tests. The product refreshes git only on git folder events and heartbeats. Agent edits to the
  working tree therefore show only on the heartbeat.
- Records come from hooks, which are phase 2, and the spec does not say which record fields signal
  a PostToolUse. So there are two cheap resolutions:
  - either move W4's second sentence to phase 2 in the trace list and say how a record shows an
    ended edit or Bash call,
  - or have the oracle author add the check now and the builder implement it.

**B3. X6's countdown N is untested.**
- No layer asserts the value of N in "updates paused, retrying in N s": `render/panel.test.mjs:230`
  checks the shape only, and `e2e/flows-rows.spec.mjs:68` captures N but never asserts it.
- A product that always shows "1 s" passes. The current product does show "1 s" during a hung
  restart (see (c)).
- Fix:
  - Add `retry_at` to Test interfaces item 5, so render can check N against it.
  - Have `flow__connection_states` assert that N is at most the current backoff and counts down.
  - Separately, the product should post a view when a restart starts, or drop the number once
    `retry_at` has passed.

**B4. The first view after a show or a reload can be lost** (X7, X2; product).
- The host posts once, when the tab becomes visible (`host/panel.ts:75-79`) or when a document
  arrives. After that it posts only on a change: alive lines post nothing (B7).
- The webview's listener is attached by an external script at the end of `<body>`, after an
  external stylesheet (`vscode/src/html.ts:23,27`, `vscode/src/webview/main.ts:15`).
- VS Code queues messages for a loading frame, but flushes them on the frame's `load` event **or
  after 200 ms, whichever comes first**. See VS Code 1.143.1 `out/vs/workbench/contrib/webview/
  browser/pre/index.html:1175-1179` and the flush at `:1162-1165`.
- If `webview.css` and `webview.js` take longer than 200 ms to load, the view message is delivered
  before any listener exists and is dropped. That is plausible on a reload while a large cargo
  build loads the machine. The tab then shows an empty table. On an idle pool it stays empty until
  something in git changes, or until the tab is hidden and shown again.
- Not observed in the runs; found by reading the code.
- A fix within the spec: an inline nonce script at the top of `<head>`, before the stylesheet,
  that buffers `message` events for `main.ts` to drain. X8 allows nonce scripts and forbids only
  inline style.
- A `ready` message would need X11 to change.

## Non blocking findings

1. Report: `xtask/src/report.rs:341` always writes "Budgets: None in this phase", although phase 1
   ran B3, B4, B5, B7, B8 and B10. The report has no measured values, so the owner cannot see the
   margins (verification.md "The report" lists budgets). Oracle.
2. Coverage: int watch lines are lost because `Watch::drop` SIGKILLs `wt`
   (`xtask/tests/support/mod.rs:466-471`). Oracle.
3. X10: `visible()` (`vscode/src/webview/text.ts:54-73`) covers C0, C1, ALM, U+200B-200F,
   U+202A-202E, U+2060, U+2066-2069 and U+FEFF. That is every character named "zero width", plus
   controls and bidi, so X10 holds as written.
   - Still shown raw, with no width: U+00AD, U+180E, U+2061-2064 and the tag characters
     U+E0000-E007F.
   - `charCodeAt` cannot see characters above U+FFFF.
   - The host already uses `/[\p{Cc}\p{Cf}\p{Zl}\p{Zp}]/u` (`host/actions.ts:34`). Using it with
     `codePointAt` would also remove the duplicate definition.
4. X5: the backoff resets on any status document (`host/stream.ts:117`). A `wt` that prints one
   document and then dies restarts about every second, with a full git scan each time.
5. B10 margin: SIGKILL fires at exactly 1 s (`host/stream.ts:18,45`), the B10 limit. This matters
   only for a `wt` that ignores both EOF and SIGTERM.
6. `deactivate` disposes the panels (`host/tabs.ts:34-36`). Restore after a reload passes; stopping
   the streams instead would not rely on VS Code saving editor state first.
7. The module-wide `lastKnown` (`host/panel.ts:53`) lets a new tab show an earlier tab's document as
   `live` before its own first line.
8. `spawn` sets no `cwd` and ignores stderr (`host/stream.ts:80`), so `wt`'s own error text never
   reaches the output channel.
9. A waiting actor with a null `reason` renders as `label · : tool target` (`webview/text.ts:142`).
   `validateFormats: false` (`scripts/generate.mjs:43`) lets a malformed time render as `NaN`.
   Neither happens in phase 1.
10. The detail strip is rebuilt on every document (`webview/panel.ts:209`), which clears a text
    selection inside it. X15's focus, selection and strip survive.
11. Glyphs in `.chip-*::before` and `.connection-*::before` are read aloud by screen readers. Use
    `content: "…" / ""`.
12. Rust:
    - When the foreground folder is gone, `--json` errors and `--watch` keeps stale facts
      (`state.rs:71`).
    - `git worktree move` shows only on the heartbeat.
    - `Head::Unknown` serializes as `branch: null`, which shows `(detached)`.
    - A panicking reader thread drops its row silently (`state.rs:259`).
    - `READ_ONLY` is a process-wide flag that `main.rs` must set before any status call
      (`run.rs:33-39`).
13. Duplication:
    - `ViewMessage` and `View` (host and webview).
    - `actions.ts:8` derives `Worktree` again instead of using the generated type.
    - Several exported constants have no consumer.
    - `.vscodeignore` ships `webview.js.map`.
14. The oracle findings listed in section 1 as non blocking.

verdict: not clean
