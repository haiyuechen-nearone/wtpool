# Code review, phase 0, round 8 (steps 4 and 5)

- Clone: `~/.cache/wt-verify/clones/phase-0-r8` at the round 8 test split, clean tree.
- Spec: docs/design/ at the round 8 freeze (no spec change since spec-base, per the report).
- Run reviewed: `verify/out/phase0-r8`, verdict PASS.
- Product diff: from spec-base to the round 8 test split under `crates/` and `README.md`. The product commits are
  the `wt --version` commit, the version build fix and this round's test split. The split changes only test code: it splits
  `crates/wt-core/tests/pool.rs` and `crates/wt/tests/cli.rs` into folder test binaries and moves
  the `mod tests` body of `crates/wt-core/src/settings.rs` to `crates/wt-core/src/settings/tests.rs`.
  `crates/wt/src/main.rs`, `crates/wt/build.rs`, `crates/wt/Cargo.toml` and `README.md` are
  unchanged since the version build fix, so the product code is the code that rounds 2 to 7 reviewed.
- Scope: steps 4 and 5 of prompt B. Canaries (steps 1 and 3) and the oracle review (step 2) are
  reviewed elsewhere.

## Blocking

None.

## Should fix (non blocking)

1. **Carried over from round 7, still open: the build script panics on an environment variable
   that is not UTF 8** (`crates/wt/build.rs:19`). `std::env::vars()` panics while it iterates over
   such a variable. The round 8 test split does not touch `build.rs`. `verify/oracle-changes.toml` entry
   `p0r7-oracle-should-fix` records it as "deferred: non blocking" (builder side). Fix: read
   `CARGO_CFG_FEATURE`, or use `std::env::vars_os()`.
2. **Carried over from round 7, still open: the feature list also takes `CARGO_FEATURE_*`
   variables from the caller's environment** (`crates/wt/build.rs:19-24`). An exported
   `CARGO_FEATURE_NOT_A_FEATURE=1` gives `features: not-a-feature`. The mapping also cannot
   restore a feature name that has an underscore. Fix: read `CARGO_CFG_FEATURE`, which also fixes
   item 1. Deferred in the same entry.
3. **Files well above 500 lines with a clean split** (file length rule: about 500, hard cap 1000).
   No product file is over 1000. These were not in this round's brief, which named only the files
   over the cap:
   - `crates/wt-core/src/pool.rs` (886 lines). The refusal helpers (`not_resolved` to
     `unreferenced_refusal`, `pool.rs:687-771`) and slot creation (`next_slot_name`,
     `create_slot`, `seed`, `tracks_envrc`, `create_without_following`, `pool.rs:603-795`) are
     self contained. Moving them to `pool/refusals.rs` and `pool/slots.rs` leaves about 550 lines.
     The `mod tests` at `pool.rs:826-886` can go to `pool/tests.rs` the same way as settings.
   - `crates/wt-core/src/resolve.rs` (714 lines). The sync step (`sync_branch`, `warn_if_stale`,
     `Divergence`, `left_right`, `resolve.rs:458-636`) is one unit and can go to
     `resolve/sync.rs`, leaving about 535 lines.
   - `crates/wt-core/src/table.rs` (564 lines) and `crates/wt-core/src/worktree.rs` (519 lines):
     the `mod tests` blocks (`table.rs:299-564`, 266 lines; `worktree.rs:289-519`, 231 lines)
     move cleanly to `table/tests.rs` and `worktree/tests.rs`, as the round 8 test split did for settings.
   - Near 500 and fine as they are: `crates/wt-core/src/settings.rs` (537),
     `crates/wt-core/tests/pool/support.rs` (524), `crates/wt-core/tests/pool/background.rs`
     (503). If `support.rs` grows, the git state helpers (`must_stop_a_merge` and the like) are
     the natural `support/git_states.rs`.

## Notes (style only)

- Seven of the new test files hold one section, but keep the old divider and section title
  under a module doc that says the same: `crates/wt-core/tests/pool/foreground.rs:11-12`,
  `free.rs:13-14`, `resolve.rs:8-9`, `sync.rs:12-13`, `tracking.rs:9-10`,
  `crates/wt/tests/cli/commands.rs:12-13`, `crates/wt/tests/cli/tools.rs:7-8`. The `//!` line is
  enough. The files with more than one section (`background.rs`, `list.rs`, `slots.rs`,
  `startup.rs`) need their dividers.
- Round 7's notes still apply: `crates/wt/src/main.rs:274-277` prints and returns inside the
  `match` that builds `action` (an `Action::Version` variant would keep one place for output), and
  `Build<'a>` (`main.rs:336`) only ever holds `'static` strings.
- `verify/out/phase0-r8/logs/unit.log:84`: nextest marks
  `wt::cli commands::new__should_add_a_slot::case_2_given_name` LEAK (it passed). A child of the
  test, most likely a git process that `wt new` started, kept the output pipe open after the test
  ended. The test did not change this round. It is not a failure, but a repeat in later rounds
  would be worth a look.
- For the oracle reviewer, not a product finding: `settings/tests.rs` is in neither lcov file,
  although `xtask/src/cover.rs:141` (`is_measured_kind`) counts it as measured. `cargo llvm-cov`
  leaves out a file it treats as test code, so the report lists all 429 lines as "not measured"
  while `cover` passes with "42 of 42". Here the file is `#[cfg(test)] mod tests;`
  (`settings.rs:536-537`), so nothing is lost. But product code in a file `cargo llvm-cov` drops
  would also appear only as "not measured", and the verifier would have to catch it by hand.

## The test split: no test case dropped, changed or renamed

Compared the round 7 freeze (the last commit before the split) with the round 8 test split. Scripts are in my
scratchpad, not in the clone.

1. **Test functions and their text.** I split each old and new file into top level items, kept
   those with `#[rstest]` or `#[test]`, and compared them by function name, with whitespace and
   `//` comments removed:
   - `crates/wt-core/tests/pool.rs` → `tests/pool/*.rs`: 72 test functions, 153 `#[case]` rows,
     before and after. None missing, none extra, none changed.
   - `crates/wt/tests/cli.rs` → `tests/cli/*.rs`: 31 test functions, 57 `#[case]` rows, before
     and after. None missing, none extra, none changed.
   - `settings.rs` `mod tests` → `settings/tests.rs`: 17 test functions, 19 `#[case]` rows, before
     and after. Three differ only in rustfmt output after the dedent:
     `env_values__should_refuse_a_variable_that_is_not_unicode` (a closure body loses its braces,
     `settings/tests.rs:133-134`), `invalid__should_name_where_the_value_came_from` (the
     parameter list joins one line and loses the trailing comma, `settings/tests.rs:293`), and
     `into_config__should_carry_every_setting_over` (only the old module's closing brace). A
     line by line `diff` of the dedented old module against the new file shows nothing else.
2. **Every other line.** Sorted line multisets (leading whitespace trimmed, blank lines dropped)
   of old against new differ only in `pub(crate)` on the shared helpers, the `use` lists, the
   `mod` lines, the new `//!` docs, and one new section title (`tracking.rs:10`). No assertion,
   case row or helper body changed. `#![cfg(unix)]` and `#![cfg(test)]` stay at the top of each
   binary root (`tests/pool/main.rs:1-3`, `tests/cli/main.rs:1-3`).
3. **Test names as run.** I built the expected nextest names from the round 7 freeze's sources (function
   name, `case_N` padded as rstest pads it, plus the case label) and compared them with the names
   in `verify/out/phase0-r8/logs/unit.log`, module prefix removed: `wt-core::pool` 186 of 186,
   `wt::cli` 75 of 75, `wt-core` `settings::tests::*` 31 of 31. None missing, none extra. The only
   change in a full name is the new module path (`commands::`, `background::` and so on), which
   the split itself brings. The binary names `pool` and `cli` stay the same, so a nextest filter
   on them still matches. All 542 tests passed.
4. No oracle file, script or doc names the old paths (searched `xtask/src`, `verify/*.toml`,
   `README.md`, `docs/`). The only hit, `xtask/src/cover.rs:312`, is a sample path in a test
   case of the oracle.

## File lengths

Every product file under `crates/` and `README.md` is at most 1000 lines. The longest is
`crates/wt-core/src/pool.rs` at 886. The new test files run from 9 to 524 lines. See should
fix 3 for the files well above 500.

## Code against each ID of phase 0 or earlier

Phase 0 has no behaviour ID. All IDs are phase 1 or later (verification.md, Trace list). Phase 0
is `wt --version` (version, commit, cargo features) and the harness core
(status-panel.md, Phases).

- `wt --version` and `-V` print `wt <version>`, `commit <sha>` (with ` (dirty)`, or `unknown`
  when git failed) and `features: <list>` (`none` when empty) (`crates/wt/src/main.rs:345-361`).
  It runs before the configuration is read (`main.rs:274-277`, before `Settings::load` at
  `main.rs:289`). This matches status-panel.md, Phases, row 0.
- `build.rs` reads the commit and the dirty state with git (`--no-optional-locks`, so no index
  write), and runs on every build through a `rerun-if-changed` path that never exists
  (`build.rs:7-9`). See should fix 1 and 2 for the feature list.
- The `verify` feature exists (`crates/wt/Cargo.toml:9-11`) and the release build leaves it out
  (`release__should_have_no_verify_feature` passes).
  `version__should_list_version_commit_and_features` passes.
- README (`README.md:27`, `README.md:92`) and the help text (`main.rs:21`) name `wt --version`.
  No live doc names the moved test files.

## Lines no oracle layer covers

From the coverage section of `verify/out/phase0-r8/report.md`.

- `crates/wt-core/src/settings/tests.rs:1-429` (not measured): justified. The file is the
  `#[cfg(test)]` test module of `settings.rs` (`settings.rs:536-537`), moved unchanged (see the
  split section). It is test code, run by `unit`, with no product behaviour to cover. It is in
  neither `cover/unit.lcov` nor `cover/int.lcov`.
- `crates/wt/src/main.rs:347` (unit only): justified. The `unknown` commit text runs only when
  git fails at build time, and no harness build makes git fail.
- `crates/wt/src/main.rs:353` (unit only): justified. The `none` features text runs only in a
  build without features. The `int` layer builds with `verify`, and the release binary that
  `release__should_have_no_verify_feature` runs has no coverage instrumentation.

## Tests against the test standards

- The moved tests are unchanged (see above), so the round 7 findings hold: `rstest` tables with
  exhaustive cases and no overlap, `<sut>__should_<assertion>` names, bare `// Given`, `// When`,
  `// Then`. `version_text__should_list_version_commit_state_and_features`
  (`main.rs:396-464`, 8 cases) and
  `version__should_print_the_build_without_reading_the_configuration`
  (`crates/wt/tests/cli/startup.rs:64`, 2 cases) cover phase 0.
- The split follows the standards: each file has a `//!` doc that says what it holds, shared
  helpers live in one `support.rs` per binary with `pub(crate)` visibility, and nothing is `pub`
  beyond the binary. `static:clippy` (`-D warnings`, so no dead helper after the split),
  `static:fmt` and `lint__no_suppressions` pass.
- No new `unwrap`, `expect` or panic outside tests. No suppression was added.

## Screenshots against the normative mockup parts (step 5)

Phase 0 has no normative mockup part to check. It has no `shot` or `e2e` layer, and the visual
review starts in phase 1 (verification.md, Canaries). The run has no screenshots: the only PNG
files under `verify/out/phase0-r8` are the five mockups copied into
`rebuild/repo/docs/design/status-panel/attachments/`. Nothing to compare.

verdict: clean
