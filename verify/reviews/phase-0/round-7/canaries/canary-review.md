# Canary review, phase 0, round 7 (steps 1 and 3 only)

- Clone: `~/.cache/wt-verify/clones/phase-0-r7` at the round 7 freeze (product unchanged since the version build fix)
- Results: `canary-run.txt`. Scripts and patch sources: `experiments/`
- New canaries: `isolation-lock-pool-worktree-after-part.patch`,
  `isolation-relock-pool-worktree-naming-run.patch`, `edit-oracle-under-round-6-entry.patch`

Each job used its own scratch clone of the round 7 freeze and its own temporary `HOME`, so its own canary
clone and its own target folders (`cp -c -R` from the lead clone). Each patch ran with
`cargo xtask canary`, which runs only the part of its check. Isolation canaries ran on a fake pool
from `experiments/mkpool.sh` under the `HOME` that the patch names. No run touched the real pool.
No process of these runs is left.

## Blocking

1. **Canary survived: `isolation-lock-pool-worktree-after-part.patch`.**
   Wrong check: `isolation__left_no_trace`, its snapshot times
   (`xtask/src/parts.rs:141-153`, `xtask/src/isolation.rs:456-480`, `xtask/src/report.rs:101`).
   - The patch makes `wt --version`, when an int check runs it, start a detached child with null
     stdio in its own process group. The child waits until the int part has written its record,
     then locks slot `app-wt1` with a reason that names its sandbox foreground. This is the same
     lock that the round 6 canary made. Only the time differs.
   - Result (`canary-run.txt`, job11): "2 part runs left no trace". The int record says
     `listing_mentions_run` false before and after. The lock appeared at 00:38:14, in the same second
     as the int record. The report followed at 00:38:16. After the run, `git worktree list` shows
     `locked <canary clone>/verify/out/canary-1792794985080/sandbox/version-61236-0/pool/fg`.
   - Spec: verification.md:124-129. The check fails when the run left the run folder in the
     listing. A process that the run started left it there.
   - Cause: each part snapshots right after its commands return (`parts.rs:152`). `cmd::run` kills
     the process group only on a timeout (`cmd.rs:55`), and a child in its own group escapes even
     that. `--report` takes no new snapshot. It reads only the stored part records, and only
     `verify_refs` and `outside_remotes` read the present state (`isolation.rs:479-480`). So every
     trace kind that a leftover process writes after its part goes unseen, not only this lock.
   - Fix: in `--report`, take one more snapshot and compare it with the last part's `after`.
     Also fail a part when a process that the run started is still alive after it, for example a
     process whose environment holds `WT_VERIFY_SANDBOX=1`. Otherwise a child that waits past the
     report still escapes. This is oracle code, so it needs an entry.
   - Weight: in phase 0 only `wt --version` runs. From phase 1, parts start long-lived processes:
     `wt status --watch`, VS Code in `host` and `e2e`, and `claude` sessions with hooks in `cc`.
     A hook that moves slow work into a detached child to keep its 5 ms budget (B1) is a plausible
     design. A leftover process can write to the owner's real pool after the run says PASS. So this is
     blocking.

## Non blocking

None.

## Notes

- The child in the canary waits for the part's record. This makes the timing the same on every
  run. Any child that writes after its part ends gives the same result.
- `isolation-relock-pool-worktree-naming-run.patch` (pool with the owner's lock; the run replaces the
  reason) is caught: "int: git -C ~/code/app worktree list names the run folder". So a locked
  line that exists before the part does not hide the new reason.
- The round 1 original `suppress-in-target-module.patch` reports SURVIVED again. It has no module
  file and does not build, as round 2 noted. Not counted. Its adopted copy is caught.

## Earlier findings

| Round 6 finding | Status |
|---|---|
| Blocking 1, `isolation-lock-pool-worktree-naming-run.patch` survived | Closed. The round 6 file, on its fake pool under `v6/h8`, is caught: "int: git -C ~/code/app worktree list names the run folder" (job1). The lock line is still in the listing after the run, so the patch did apply. |
| Should fix 1, unit layer has the real `TMPDIR` | Deferred under `p0r6-unit-tmpdir`. Not open. |

## Canaries

46 counted runs: 45 caught, 1 survived. 1 run not counted (the round 1 original that does not
build).
- All 29 canaries in `verify/canaries/phase-0/` are caught at the round 7 freeze (jobs 9 and 10).
- Earlier rounds' originals, 14: round 3 `isolation-move-pool-head` and
  `isolation-add-pool-worktree`; round 4 `isolation-remove-pool-worktree` and
  `version-release-never-dirty`; round 5 `isolation-delete-pool-worktree-folder`,
  `isolation-move-pool-worktree` and `isolation-unit-test-moves-pool-head`; all 7 of round 6.
  All are caught.
- New, 3: `edit-oracle-under-round-6-entry` (oracle_unchanged, it removes the new listing rule
  under the spent entry `p0r6-listing-names-run`) and `isolation-relock-pool-worktree-naming-run`
  are caught. `isolation-lock-pool-worktree-after-part` survived: blocking 1.
- The set covers all four phase 0 categories: it breaks `wt --version`, stops a frozen check,
  edits an oracle file and adds a `CLAUDE.md`.

verdict: not clean
