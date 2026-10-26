# Verifier review, phase 0 (verification.md revision 9)

- Spec read: `docs/design/` at the final phase 0 freeze (status-panel.md revision 8, verification.md revision 9).
- Clone: `~/.cache/wt-verify/clones/phase-0-r9` at the final phase 0 freeze, clean tree.
- Run: `verify/out/phase0-r9`, verdict PASS. All three parts (`quick`, `int`, `mutants`) record
  the final phase 0 freeze.
- Author diff reviewed: from the round 8 freeze to the final phase 0 freeze. The oracle part of it is the final phase 0 freeze.
  The round 8 test split is the builder's, and two docs commits are the approved revision 9 docs.
- The freeze clone `~/.cache/wt-verify/clones/freeze` (eec798b) has the same tree as the final phase 0 freeze
  (3d126de). Its canary run `canary-1792804062240` is the freeze evidence for the mutants check.
- I ran no build and no verify part. I only read files, logs and git output, and I started no
  process that is still running.

## 1. Checks in the author's diff against the spec

### Removal of the lingering process hunt (p0r8-no-process-hunt): correct

- `xtask/src/isolation/processes.rs` is deleted. `Snapshot.lingering`, `Scope`, `stop_lingering`,
  `invocation_id`, `INVOCATION_VAR` (`WT_VERIFY_INVOCATION`) and `cmd::kill` are gone, along with
  their callers in `parts.rs` and `report.rs`. A grep of `xtask/` and `crates/` for `kill`, `signal`,
  `ps`, `pgrep`, `process_group`, `setsid`, `pre_exec` and `WT_VERIFY_INVOCATION` finds only
  `xtask/src/cmd.rs`.
- `isolation__left_no_trace` now matches verification.md:124-130. It compares the snapshots
  before and after each part (`parts.rs:142-157`). `--report` takes one more snapshot
  (`report.rs:104-109`) and compares it with the last part's after snapshot (p0r7-late-traces is
  kept). It looks for no processes and kills none. `report.rs:106-108` now writes the final
  snapshot to `isolation-report.json`, which fixes round 8 oracle non blocking item 3.
- The canary `isolation-detached-child-outlives-part.patch` is removed with the rule it tested.
  This is consistent with the spec.

**Can the oracle signal or kill a process it did not start? No.** The only signal left is the
timeout path of `cmd::run_with` (`xtask/src/cmd.rs:41-79`, `:116-122`):

- With `new_group` (all oracle commands except the mutants shim's), the child is spawned with
  `process_group(0)`, so its pgid is its own pid. At the timeout, `kill -s KILL -- -<pid>` goes to
  that group. `try_wait` has just returned `None` and nothing has reaped the child yet, so the pid
  stays reserved, even as a zombie, and no other group can take that id. The group holds only that
  child and the descendants that stayed in it, and the oracle started all of them.
- Without `new_group` (the shim, `shim.rs:142`), `Child::kill` signals the unreaped direct child
  only.
- `cargo mutants` gets `Duration::MAX` (`mutants/mod.rs:41`, `:247`), so the oracle never kills
  it. cargo mutants kills only the process group of its own child (the shim) at a mutant's timeout.
  nextest kills only its own test processes. The oracle starts both of them.
- Nothing reads the process table any more, so a shell or editor that the owner opens in a run folder
  can no longer be matched (round 8 B1 is gone).

### `mutants__should_leave_no_survivor` (p0r9-mutants): tests what the spec says

Spec (verification.md:359-368): `cargo mutants --in-diff` against `spec-base` over `crates/`,
with the `unit` and `int` tests. The check fails on a mutant that builds and that no test catches,
unless `verify/mutants-allowed.toml` lists it with a reason. Timeouts count as caught, unviable
mutants as nothing, and the part is not bound by the 1 hour limit.

- Diff: `crates_diff` (`mutants/mod.rs:158-194`) is `git diff <spec-base> -- crates/`, plus
  untracked files as whole-file diffs. A table test covers it (`:424-457`).
- Unit and int: cargo mutants runs `--test-tool=nextest --test-workspace=true --baseline=run`
  with the `unit` profile (`:226-246`). `CARGO` is the xtask shim (`shim.rs`). The shim widens every
  package selection to `--workspace` (`shim.rs:45-63`). After a passing unit run, it builds the
  `verify` debug and release `wt` in the mutated copy, makes the rebuild steps, and runs the frozen
  `int` checks against them (`shim.rs:134-183`). It uses the same environment variables as the
  `int` part (`parts.rs:412-505`). The baseline must end `Success` (`mod.rs:258-264`). The run's
  baseline log shows 542 unit tests passing, then both int checks passing.
- Verdicts: `parse_outcomes` (`outcomes.rs:152-187`) maps `CaughtMutant`, `MissedMutant`,
  `Timeout` and `Unviable`. Any other summary is an error. `judge` counts `Timeout` as caught and
  `Unviable` as nothing (`:105-147`). `verdict` fails on any `Missed` survivor that no entry names
  by exact file, line and mutation (`mod.rs:91-118`). An entry needs a non-blank reason, and
  unknown keys are refused (`outcomes.rs:84-102`).
- Completeness: the number tested must equal the number `--list` found (`mod.rs:265-270`). The
  output folder is emptied first (`:56`), so a stale `outcomes.json` cannot count, and a partial
  run is an error. `.cargo/mutants.toml` is not read (`--no-config`), and `#[mutants::skip]` in
  `crates/` fails the check. That is the stricter reading, and I agree with it.
- Time limit: `MUTANTS_LIMIT = Duration::MAX`, as the spec says.
- `verify/mutants-allowed.toml` is excluded from the oracle files (`oracle.rs:26-31`), with a table
  case and a lookalike case. The file does not exist, so I have no entry to review.
- Wiring: `required_parts(0)` now includes `mutants` (`report.rs:39-44`). `verify/trace.toml` has
  the check at phase 0 and `flows__should_each_have_an_e2e_check` at phase 1, and the new self test
  `trace_file__should_list_the_harness_checks_of_the_trace_list` (`trace.rs:312-355`) ties
  `trace.toml` to the trace list paragraph. The freeze `HARNESS_CHECKS` and `part_of` know the new
  part.

**Could the check pass with a surviving mutant? Not through its own logic.** A `MissedMutant`
always fails it unless an entry names it. Every listed mutant must have an outcome. The freeze
canary `mutants-test-assertion-removed.patch` shows this end to end: on the frozen tree it gave
`fail`, "11 mutants … 5 caught, 6 missed", and named each surviving mutant of `is_even` and
`main`. The remaining ways are outside the judgement, and they are listed as non blocking items 1
to 3. In this run, all 5 mutants (`crates/wt/src/main.rs:265`, `:274` twice, `:346` twice) were
caught by real assertion failures in `crates/wt/tests/cli/{commands,startup}.rs` and in the
`version_text` table. None was caught by an infrastructure error (`mutants.out/log/*.log`).

### Code moved without changes

- `xtask/src/freeze.rs` is the code removed from `clone.rs`, verbatim, apart from visibility.
- `xtask/src/rebuild.rs` is the code removed from `parts.rs`. Two things are added: the
  `warm_target` argument, which the shim uses to keep one rebuild target across mutants, and
  `pub` visibility. The `int` part passes `None`, so its behaviour is unchanged.
- The isolation rules lose only the lingering cases. All other cases are unchanged.

### User flows

Phase 0 has no user flows. The User flows table (verification.md:220-247) starts at phase 1, so no
`flow__` e2e check is due. `flows__should_each_have_an_e2e_check` is registered at phase 1 in
`verify/trace.toml`. Phase 0 has no e2e layer.

## 2. Code against the spec

### IDs of phase 0 or earlier

No behaviour ID is phase 0 (verification.md:254-255: every ID is phase 1, 2 or 3). Phase 0 is
`wt --version` and the harness core (status-panel.md:228). The product has not changed since
the round 8 test split, which only moved tests, and the round 8 code review found it clean.
`crates/wt/src/main.rs:274-277`, `:327-361` and `crates/wt/build.rs` give the version, the full
commit, the dirty marker and the features. `version__should_list_version_commit_and_features`
(debug, release and the four rebuild steps of each) and `release__should_have_no_verify_feature`
pass. The `verify` feature exists and release builds leave it out (`crates/wt/Cargo.toml`).

### Lines no oracle layer covers

- `crates/wt-core/src/settings/tests.rs:1-429` (not measured): **justified**. It is the
  `#[cfg(test)] mod tests` that the round 8 test split moved out of `settings.rs` without change. It is test code,
  not product code.
- `crates/wt/src/main.rs:347` (unit only): **justified**. The `"unknown"` arm needs a build where
  git fails. The int layer cannot make that build, and `version_text` table cases cover it.
- `crates/wt/src/main.rs:353` (unit only): **justified**. `"none"` is printed by the release build,
  which the int layer runs (`release__should_have_no_verify_feature`) but does not instrument. Unit
  table cases cover it.

### Mutants in `verify/mutants-allowed.toml`

None. The file does not exist, and the run reports no survivors and no unused entries.

### Tests against the test standards

- The new oracle tests (`mutants/mod.rs`, `mutants/outcomes.rs`, `mutants/shim.rs`, `trace.rs`)
  use rstest tables where there are cases, `<sut>__should_<assertion>` names, and bare
  `// Given`, `// When`, `// Then`. Their cases do not overlap.
- The product tests have not changed since the round 8 review. That review checked the split case
  by case.
- No Rust file is over 1000 lines. The largest are `crates/wt-core/src/pool.rs` (886) and
  `crates/wt-core/src/resolve.rs` (714).

## 3. Screenshots against the normative mockup parts

Phase 0 has none. The visual review starts in phase 1 (verification.md:370-375), and phase 0 has
no `shot` or `e2e` layer.

## Findings

### Blocking

None.

### Non blocking

1. **A test run that fails for an unrelated reason counts as a catch.** The shim turns an
   int layer that "could not run" into exit 1 (`xtask/src/mutants/shim.rs:102-108`). cargo mutants
   also counts any failing unit test as caught. So a flaky test, or an infrastructure error
   specific to one mutant, would hide a real survivor. The baseline must pass, and the per mutant
   logs are kept, so a reviewer can see it. All 5 catches in this run are genuine. Option: give
   "could not run" a distinct exit code, and report it as an error rather than a catch.
2. **`#[mutants::skip]` under any folder named `target` goes unseen.** `skip_attributes` skips
   every path component named `target` (`xtask/src/mutants/mod.rs:130`). cargo mutants would still
   obey a skip in `crates/wt/src/target/mod.rs`. This is the bug fixed by p0-nested-target,
   repeated in new code. It is a gap in a rule that goes beyond the spec, and an honest builder
   would not hit it. Fix: skip only the repository's own `target/`. Also, `skip_lines`
   (`mod.rs:146-155`) misses a skip split across lines, as in `#[mutants::` then `skip]`. Only
   deliberate evasion writes that.
3. **cargo mutants does not mutate build scripts.** The run listed no mutants in
   `crates/wt/build.rs`, although that file is in the diff. Its commit, dirty marker and feature
   logic is checked only by `version__should_list_version_commit_and_features` and the `version-*`
   canaries, which cover it well. This is a limit of the tool, not a wrong check. It is worth one
   sentence in verification.md's Mutation testing section.
4. **The spec contradicts itself on `verify/mutants-allowed.toml`.** verification.md:77-78 lists
   the exceptions under `verify/` without this file, which makes it an oracle file. :363-364 says
   it belongs to the builder. The oracle follows :363-364 (`xtask/src/oracle.rs:26-31`), and I agree
   with that. Only the owner edits docs, so the fix is a spec edit: add the file to the exceptions at :77-78.
5. **Mutated product code runs with the real `HOME`** in the unit runs of the mutants part
   (`xtask/src/mutants/mod.rs:274-290`, `shim.rs:66-85`), the same as the `unit` layer. This is
   already deferred under p0r7-oracle-should-fix ("unit layer real HOME and XDG") and
   p0r6-unit-tmpdir, so it is not an open finding. Mutation makes it a little more relevant from
   phase 1 on. A mutant of a sandbox guard, or of the `WT_FOREGROUND` default, runs with the real
   home. Today the product tests start `wt` with `env_clear` and a sandbox `HOME`
   (`crates/wt/tests/cli/support.rs:144-160`), and the library tests pass explicit settings, so no
   current mutant can reach the real pool.
6. **Allowed entries that name no survivor are reported but do not fail** (`outcomes.rs:130-146`).
   The spec does not ask for a failure. Stale entries could build up, so the verifier should read
   the report's "Entries … that name no surviving mutant" list each phase.
7. **Carried over, builder side, not open:** the `build.rs` handling of non UTF-8 environment
   variables and exported `CARGO_FEATURE_*` variables (deferred under p0r7-oracle-should-fix), and
   files well over 500 lines: `crates/wt-core/src/pool.rs` (886) and `resolve.rs` (714), from round 8
   code review item 3.

## Checks I think wrong

None blocking. For `mutants__should_leave_no_survivor`, see non blocking items 1 and 2.

verdict: clean
