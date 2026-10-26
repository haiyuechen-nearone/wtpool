# Oracle review, phase 0, round 7

Scope: step 2 of prompt B. The canaries (steps 1 and 3) and the code review (steps 4 and 5) are
in other files of this round.

- Clone: `~/.cache/wt-verify/clones/phase-0-r7` at the round 7 freeze, clean tree.
- Spec and freeze: docs/design/ at the round 7 freeze. Author diff: from the round 6 freeze to the round 7 freeze.
- Run read: `verify/out/phase0-r7`, verdict PASS, both parts at the round 7 freeze.
- Experiments ran only in the scratchpad: one git pool under `/tmp`, and a copy of the attribute
  scanner of `xtask/src/suppress.rs` built with `rustc`. No `wt` ran, and nothing touched the real
  pool.

## Oracle changes since the freeze

None. `/Users/you/code/wt` and the clone are both at the round 7 freeze with a clean tree. The
report says "none" for oracle changes and for `verify/accepted.txt`, and `oracle_unchanged`
passed.

The author diff touches one oracle file, `xtask/src/isolation.rs`, plus the lock the freeze wrote.
Entry `p0r6-listing-names-run` covers it, and `verify/accepted.txt:20` accepts it. The other
round 6 entry, `p0r6-unit-tmpdir`, is deferred, so it is not open.

## Author diff from the round 6 freeze to the round 7 freeze against the spec

`isolation__left_no_trace` tests what verification.md:124-125 says. That line fails a run that
left the run folder in `git -C ~/code/app worktree list`.

- `snapshot` now runs `mentions` over the whole porcelain listing (`isolation.rs:125-126`). The
  porcelain listing includes each lock reason, so a lock reason that names the run folder is found.
- `traces` fails whenever the listing after a part names the run folder (`isolation.rs:282-286`).
  This includes a listing that already named it before the part. That is right, because the trace
  is still there.
- I checked the round 6 survivor by reading the code. Its lock reason is
  `<clone>/verify/out/canary-<n>/sandbox/release-<n>-0/pool/fg`, and that text contains the run
  folder. So `listing_mentions_run` is true after the part, and the check fails on an assertion.
- In my scratch pool, git 2.51.1 prints a lock reason with a newline as one C quoted line. The
  path text is still in it, so `mentions` finds it.
- Tests. The three new `traces` cases (`isolation.rs:562-572`), with `untouched`, cover all four
  before and after states with no overlap. The new `snapshot` test (`isolation.rs:879-940`) runs
  real git for an untouched slot, a lock naming the run, a lock that names something else, and a
  slot inside the run folder. Names and bare Given, When, Then follow the standards. Style note 4
  is about this test.

No check in the diff is wrong.

## Whole phase 0 oracle against verification.md

I read `xtask/` (all modules, `tests/int.rs`, `tests/support/mod.rs`, the manifests) and `verify/`
except `out/` and `reviews/` (trace list, absent list, nextest profiles, lock, entries, canary
headers). I compared them with verification.md: oracle files, test interfaces, isolation, layers,
test standards, trace list, outcomes, the report and canaries.

### Blocking

1. **`lint__no_suppressions` misses an `allow` after an unbalanced `#[`**
   (`xtask/src/suppress.rs:28-58`, cause at line 55).
   - Spec: verification.md:158-160. The check fails on any `allow` or `expect` of an oracle lint.
   - Cause: `attributes` does not skip comments or string literals. Each `#[` it finds starts a
     body that runs to the matching `]`. Scanning then goes on after that `]` (`index = end`). So
     a `#[` in a comment or string, with no `]` after it, swallows the code that follows. Any
     real attribute in that code is never scanned on its own. A string also flips `in_string` on
     at its closing quote.
   - Shown with a copy of lines 13-154 compiled on its own:
     - `#[allow(dead_code)]` alone is flagged.
     - `// opens with #[ here` on the line before it gives no finding.
     - `const OPEN: &str = "#[";` on the line before it gives no finding.
     In both cases clippy accepts the `allow`, and the check passes.
   - Weight: the product needs a lone `#[` in a comment or string before the `allow`. That is
     unlikely, but the spec says "any allow". This is a canary that would survive, and the fix is
     small. Round 6 gave a case of the same weight the same class.
   - Fix: after each attribute, go on at the next character, not at `end`, and count lines again
     from the start of the text. Then a nested `#[allow(` is still found on its own. The cost is
     false positives for `#[allow(` text inside comments or strings, and that is the stricter
     reading. Add the comment case and the string case to
     `scan_rust__should_flag_lint_suppressions`. This is oracle code, so it needs an entry.

### Should fix (non blocking)

1. **The isolation identity rules depend on a builder owned file**
   (`.cargo/config.toml:4-6`, `xtask/src/parts.rs:68-92`, `xtask/src/oracle.rs:33-37`).
   verification.md:118-119 says `.cargo/config.toml` forces `GIT_CONFIG_GLOBAL=/dev/null`. But
   the oracle keys hold only the `[alias]` table of that file, and `Ctx::command` sets `EMAIL`
   but not `GIT_CONFIG_GLOBAL` or `GIT_CONFIG_NOSYSTEM`. Here `git config --global user.email`
   gives the owner's address, and the real pool sets no local `user.email`. So if a build drops the
   `[env]` block, the `unit` and `int` processes get the owner's global identity. A HEAD move or a
   commit those processes then make in the real pool carries the owner's address. The HEAD reflog rule
   (`isolation.rs:310-340`) and `verify_refs` (`isolation.rs:433-454`) ignore that address by
   design. No check fails on a missing `[env]` block. Fix: set both variables in `Ctx::command`,
   as `Sandbox::run` already does (`tests/support/mod.rs:156-157`). Another fix is to make
   `.cargo/config.toml:env` an oracle key, but that changes the spec.
2. **`release__should_have_no_verify_feature` finds a test hook only as a literal string**
   (`xtask/tests/int.rs:163-168`). A release build that reads `WT_VERIFY_CLOCK` through a built
   name, for example `format!("WT_VERIFY_{name}")`, passes. A shared helper for
   `WT_VERIFY_SANDBOX`, which release builds read (H6), makes this a plausible honest design. The
   feature list assertion still catches a release build that enables `verify`. It does not catch
   hook code left outside `cfg(feature = "verify")`. Phase 0 has no hook yet, so this does not
   block now. For the phase 1 author: also test behaviour. Run the release `wt status --json` with
   `WT_VERIFY_CLOCK` set to a fixed time, and assert that `generated` is not that time.
3. **The report does not check that its parts ran at the report's commit**
   (`xtask/src/report.rs:87-104`, `xtask/src/results.rs:89-95`). Each part file records
   `commit`, but `report` never compares it with `HEAD` or with the other parts. With a reused
   `--run` name, a PASS for commit C can rest on a part run at another commit. The default run
   name holds the commit and a dirty fingerprint, so only a named run is exposed. Fix: when a
   part's commit or dirty state differs from the tree, add a failed `part:<name>` result.
4. **The new listing rule spells the run folder another way than git does**
   (`xtask/src/isolation.rs:107`, `:126`, `:268-281`; `xtask/src/report.rs:146-158`).
   - git stores a worktree path as its real path. My scratch pool under `/tmp` lists
     `/private/tmp/...`. A lock reason, by contrast, is kept as written.
   - `run_text` is the run folder as given, not resolved. The worktree rule compares with the
     same spelling. So if the run folder path had a symlink in it, a worktree in the run folder
     would be missed by both rules. Today `~/.cache/wt-verify/clones/` has no symlink, so this
     does not fail now.
   - The worktree rule treats `~/.cache/wt-verify/` and `~/.local/share/wt-verify/` as the run's
     own (`sandbox_roots`). The listing rule looks only for the run folder.
   - Fix: keep the lock reasons in the snapshot, and let `traces` match them and the worktree
     paths against every sandbox root, resolved as `url_inside` already does
     (`isolation.rs:400-430`).
5. **The unit layer gives `wt` the real `HOME` and `XDG_*` too** (`xtask/src/parts.rs:68-92`,
   `:583-589`). verification.md:115-116 gives `wt` a `HOME`, `TMPDIR` and `XDG_*` in the run
   folder. The deferred `p0r6-unit-tmpdir` names only `TMPDIR`. This is the same gap, so give the
   entry the other variables too. It is not a new open finding.

### Notes

- No part takes a snapshot after the `--report` step. A process that a test detaches, and that
  writes to the real pool after the int part's last snapshot, is not seen. The only exception is
  a branch or tag at a wt-verify commit, which `verify_refs` reads at report time
  (`isolation.rs:456-480`). The check could take one more snapshot at report time and compare it
  with the newest one recorded.
- `snapshot__should_record_whether_the_listing_names_the_run_folder` (`isolation.rs:886-890`)
  crosses a `bool` with a three value enum, but has only 4 of the 6 cases. The two missing ones
  (a slot in the run folder that also has a lock) add nothing. One enum of the four slot setups
  would make the table exhaustive by type. Style only, like the round 6 note on the folder test.
- The round 6 survivor `isolation-lock-pool-worktree-naming-run.patch` is not in
  `verify/canaries/phase-0/`. Round 5 adopted `version-release-misses-an-edit` there, but most
  isolation survivors stayed in `verify/reviews/`. The spec does not require adoption. Adopting it
  would make each later freeze rerun it.
- `p0r3-rebuild-edit`, `p0r5-worktree-list-failure` and `p0r6-unit-tmpdir` are deferred, so they
  are not open.

## Checks I think wrong

- `lint__no_suppressions`: blocking 1.
- `isolation__left_no_trace`: should fix 1 and 4.
- `release__should_have_no_verify_feature`: should fix 2.
- The report verdict: should fix 3.

verdict: not clean
