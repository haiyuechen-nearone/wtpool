# Oracle review, phase 0, round 8

Scope: step 2 of prompt B. Other files of this round hold the canaries (steps 1 and 3) and the
code review (steps 4 and 5).

- Clone: `~/.cache/wt-verify/clones/phase-0-r8` at the round 8 test split. The tree is clean, apart from the
  ignored `target/`, `xtask/target/` and `verify/out/`.
- Spec and freeze: docs/design/ at the round 8 freeze. Author diff: from the round 7 freeze to the round 8 freeze.
- Run read: `verify/out/phase0-r8`. The verdict is PASS, and both parts ran at the round 8 test split.
  `harness:self_test` passed, so the new `processes` and `rules` tests ran.
- I only read code and ran `ps`, `lsof` and `git` to read state. No `wt` and no xtask ran. I
  touched nothing in the real pool.

## Oracle changes since the freeze

None. The diff from the round 8 freeze to the round 8 test split changes only `crates/`, which are product files: the builder's split of
`pool.rs`, `cli.rs` and `settings.rs`. The report shows "none" for oracle changes and for
`verify/accepted.txt`, and `oracle_unchanged` passed.

## Author diff from the round 7 freeze to the round 8 freeze: entries

| Changed oracle file | Entry | Accepted |
|---|---|---|
| `xtask/src/suppress.rs` | `p0r7-suppress-parser` | `accepted.txt:21` |
| `xtask/src/cmd.rs`, `parts.rs`, `report.rs` | `p0r7-late-traces` | `accepted.txt:22` |
| `xtask/src/isolation.rs` (removed), `xtask/src/isolation/` | `p0r8-file-length`, and `p0r7-late-traces` for `isolation.rs` | `accepted.txt:24` |
| 4 new patches in `verify/canaries/phase-0/` | the author's own canaries for the entries above | the author may write them (verification.md:85-90) |

The new process code is in `isolation/processes.rs`. That path is listed only under
`p0r8-file-length`, which is a "no behaviour change" entry. But `p0r7-late-traces` names that
code, so the coverage is right in substance.

## Author diff against the spec

### `lint__no_suppressions` (p0r7-suppress-parser): correct

verification.md:158-160 says the check fails on any `allow` or `expect` of an oracle lint.

- `attributes` (`suppress.rs:30-61`) now continues at the character after each `#`, and it counts
  each line in one pass. So a lone `#[` in a comment or string can no longer hide a later
  attribute, and line numbers stay right after a multi line attribute.
- `bracket_body` can still run to the end of the text. That only makes one body too long. The next
  `#[` is still scanned on its own.
- The new cases (`suppress.rs:375-384`, `:436-452`) cover the round 7 comment and string forms.
  They also cover an `allow` inside another attribute, an `allow` in a comment (now flagged: this
  is the stricter reading), and a line number after a lone `#[` and after a multi line attribute.
- The two new canaries add the round 7 shapes to the product tree.
- No check in this part is wrong.

### `isolation__left_no_trace` (p0r7-late-traces): one blocking finding, see below

What it adds:

- A final snapshot in `--report` (`report.rs:101-104`). `rules::check` compares it with the
  `after` of the newest part record (`rules.rs:144-199`). It orders the records by the
  milliseconds in their names, not by name, and that is right.
- A `lingering` field in each snapshot (`snapshot.rs:41-43`, `:179`). It is judged in `traces`
  (`rules.rs:116-131`). If the process table cannot be read, the result is a failure.
- `stop_lingering` after each part and after the report snapshot (`parts.rs:165`,
  `report.rs:103`).

This tests what verification.md:124-125 says, and it is stricter than the spec, which reads
traces only "over the run's parts". It also catches the round 7 survivor. That survivor's
`/bin/sh` child keeps its working folder in the sandbox `home`, so `of_the_run` finds it by its
folder: its parent is launchd. The part fails and the child is killed before it waits for the
record. The new canary `isolation-detached-child-outlives-part` is found by `WT_VERIFY_SANDBOX=1`
together with a sandbox `HOME` in its environment.

### The split (p0r8-file-length): no lost code and no behaviour change

- I compared `xtask/src/isolation.rs` at the round 7 freeze (998 lines) line by line with the five new
  files (1751 lines). Every old line is still present, with these exceptions, which all come from
  `p0r7-late-traces`:
  - the `use crate::repo::{entry_hash, relative, walk}` line, now split per module;
  - the `snapshot` signature, which adds `scope`;
  - `check`, which adds `last`, the newest record and the sort by time;
  - the three test calls of `snapshot`, which pass a scope;
  - the case `traces fallback_identity_unknown`, which is now `{ fallback_email: None, ..known() }`.
    This is needed because `Snapshot::default()` now has `lingering: None`, which is a trace of
    its own. The case still tests one trace.
- A function by function diff shows the same thing. `snapshot.rs:14-250` matches the old lines
  16-250, apart from `lingering`. `present.rs` matches the old lines 360-455 exactly. `traces`
  matches the old lines 251-358, apart from the new `lingering` block.
- Every old test function is still present. Five are new: `of_the_run`, `join_tables`,
  `lingering`, `stop_lingering`, and `check__should_compare_the_last_snapshot_with_the_newest_part`.
- `mod.rs` re-exports every item that `parts.rs`, `report.rs` and `clone.rs` use. The run built
  and its self test passed.
- File lengths: `processes.rs` 573, `snapshot.rs` 518, `rules.rs` 474, `present.rs` 159,
  `mod.rs` 27. None is over the 1000 line cap, and all are close to 500.

## Can the lingering process check match or kill one of the owner's processes?

Only two things can match (`processes.rs:119-132`):

- `WT_VERIFY_SANDBOX=1` in the environment, together with an in-scope marker or the run folder
  named in the environment;
- a working folder inside the run folder, together with an in-scope marker or the ancestry rule.

I checked these points and they hold:

- Arguments are never read as the environment. `ps` runs twice, and `join_tables` (`:244-265`)
  gives no environment when the arguments differ.
- Another run's processes never match: `names_folder` requires a path boundary, and the marker
  carries `<run>#<pid>`.
- xtask, its ancestors and its children never match.
- `sccache`, which the owner's builds share, is never matched or killed.
- A VS Code terminal never matches. I traced the owner's live shells: zsh, then `Code Helper` with
  its folder at `/`, then `Code`, then launchd. The helper is not found, so the walk stops there.
- Claude Code agents with a Bash folder in the run folder never match, because the `claude`
  process's own folder is outside the run folder.

There is one way, B1 below.

### Blocking

1. **The ancestry rule counts an ancestor missing from the table as found, so it matches and
   kills the owner's shells in Terminal, iTerm2 or Ghostty** (`xtask/src/isolation/processes.rs:110-112`,
   used at `:131`; the kill is at `:273-284`, called from `parts.rs:165` and `report.rs:103`).
   - Rule: the doc comment (`processes.rs:55-57`) says a process found only by its folder counts
     when every ancestor up to launchd is also found, "so a shell the owner opened in the run folder
     is not the run's".
   - Cause: `processes()` reads `ps -x` (`:212`). That command lists only this user's processes.
     On macOS, Terminal.app, iTerm2 and Ghostty start each shell under `/usr/bin/login`. That
     program is setuid root (`-r-sr-xr-x root wheel` on this machine), so `ps -x` does not list
     it. Here `ps -x -o user=` shows 560 processes of `you` and one of root, which is `ps`
     itself. So for a `-zsh` whose folder is in the run folder, the walk asks for its parent
     `login`, does not find it, and returns `true` at `:110-112`. The shell is "found", and so is
     every child of it whose folder is in the run folder.
   - Result: say the owner `cd`s into `~/.cache/wt-verify/clones/<clone>/verify/out/<run>` in a
     Terminal tab and opens `report.md` with `less` or `vim`. Then the next part's after snapshot,
     or `--report`, lists the shell and the pager as "process … of the run was still running". The
     run fails, and `stop_lingering` sends `SIGKILL` to their shell and editor. A Claude Code session
     that they start from Terminal in that folder is killed the same way.
   - The table test misses this because its Terminal fixture puts the shell directly under
     Terminal (`processes.rs:305-321`, cases `:356-373`), with no `login` between them.
   - Fix: when a parent with pid above 1 is not in the table, treat it as not found and return
     `false`. A detached process of the run is reparented to launchd (pid 1), so it is still found.
     Another fix: read the ancestry from `ps -ax -o pid=,ppid=` but keep matching only this user's
     rows. Add a case to `of_the_run__should_find_only_processes_the_run_left`: launchd, then
     Terminal, then a `login` absent from the table, then `-zsh` in `/run`, expected `false`. This
     is oracle code, so it needs an entry.

### Non blocking

1. **An orphan the owner leaves in the run folder still matches by design** (`processes.rs:104-105`).
   One example: a `python3 -m http.server` started with `&` in `verify/out/<run>` to view
   `report.html`, after its terminal is closed. Its parent is launchd, so the ancestry rule finds
   it and it is killed at the next part end or at `--report`. Without a marker, it looks the same
   as a detached process of the run. This is a narrow case, and the run folder is the run's own
   place. One option: report folder-only matches that have no marker, but kill only processes that
   have the marker or `WT_VERIFY_SANDBOX=1`.
2. **A detached macOS system program outside the run folder is never found** (`processes.rs:44-46`,
   `:89-98`). `ps -E` shows no environment for `/bin/sh`, `/bin/sleep` or `/usr/bin/git`. So a
   detached `sh -c 'cd /; sleep 600; git -C ~/code/app …'` matches neither rule. The report
   snapshot sees its write only when the write comes before `--report`. The new canary uses the
   `wt` binary, whose environment is visible. The spec asks only for the traces, not for this
   process check, so this is a known limit and not a wrong check. Write it in the processes doc
   comment.
3. **The report snapshot makes the window longer for false failures from the owner's pool work**
   (`rules.rs:171-180`). The rules for a lost worktree, a missing folder and `wt.toml` used to
   apply only while a part ran. Now they also apply from the end of the last part to `--report`.
   Also, the `last` snapshot is not written to the run folder, so a reviewer cannot read what the
   report compared. One option: write it as `isolation/report-<millis>.json`, and keep the
   ordering in `check` from treating it as a part.
4. **The "about 500 lines" rule: two oracle files are well above it** (`xtask/src/parts.rs`
   753 lines, `xtask/src/clone.rs` 694 lines). They are below the 1000 line cap, and
   `p0r8-file-length` required a split only above 1000, so this is not open. I list it for the
   next oracle round.

## Whole phase 0 oracle against verification.md

I read these files in full: `oracle.rs`, `trace.rs`, `context.rs`, `suppress.rs`, `results.rs`,
the report flow in `report.rs`, the `Ctx`, quick and rebuild code in `parts.rs`, the canary runner
in `clone.rs`, `tests/support/mod.rs`, all of `isolation/`, `verify/trace.toml` and the lock. I
compared them with verification.md: oracle files (:73-90), isolation (:113-129), test standards
(:150-160), trace list (:210-238), outcomes (:297-306), the report (:308-317) and canaries
(:319-326). The other xtask files have the same hashes as at the round 7 freeze, where the round 7 oracle
review read all of them.

- `oracle_unchanged`: only entries that are not yet in the freeze commit's ledger count, and
  untracked oracle files are hashed too. It tests verification.md:87-90. It is not wrong.
- `trace_complete`: a check that did not run counts as a failure, harness checks are read by
  phase, and IDs that name other IDs are followed with a floor phase. It is not wrong.
- `repo_context__should_be_unchanged`: names are compared without case, nested `.claude/` is
  included, and only the repository's own output folders are skipped. It is not wrong.
- `lint__no_suppressions`: see above. One remaining gap is a comment between `allow` and `(`, as
  in `#[allow/**/(x)]`. The body starts with `allow/**/(`, so it is not flagged. Only deliberate
  evasion produces that text, and forgery is outside the threat model, so this is a note only.
- `release__should_have_no_verify_feature`, `version__should_list_version_commit_and_features`,
  the report's part commit, and the unit layer `HOME`, `XDG_*` and `TMPDIR`: the round 7 should
  fix items, now deferred under `p0r7-oracle-should-fix` and `p0r6-unit-tmpdir`. They are not
  open findings.
- `p0r3-rebuild-edit` and `p0r5-worktree-list-failure` are deferred, so they are not open.

## Checks I think wrong

- `isolation__left_no_trace`: blocking 1. The lingering process rule can match and kill the owner's
  shells in Terminal, iTerm2 or Ghostty that have their folder in the run folder.

verdict: not clean
