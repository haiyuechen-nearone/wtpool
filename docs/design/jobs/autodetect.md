# Measured heavy commands

**Status:** approved, planned for phase 6, revision 2 (2026-10-26: approved with every recommendation; QA1 to QA12 resolved as recommended; measured evidence may suggest `cargo check`, which no marker suggests, and one Dismiss hides it (JA11, QA2); jobs.md holds its phase 6 additions; revision 1: first design)

wtpool learns which commands are heavy from what they cost on this machine, for any stack. A
`wtpool status --watch` samples the process table, groups processes into runs of one command,
and keeps each heavy run's CPU time, peak memory and wall time under the command's key, per
repository. A key whose runs keep several cores busy for minutes becomes a suggestion in the
machine strip, beside the stack-marker suggestions of [jobs.md](jobs.md) (J23, J24), with its
evidence: `cargo xtask verify · 3 runs, median 11 cores for 12 min`. The user adds or dismisses
it; nothing becomes heavy by itself (JR7). This document holds the JA rows.
[status-panel.md](../status-panel/status-panel.md) holds the rows it amends: R7, S2, S4, S14, W11,
U15, X11, X21, X22 and the Phases table. Every new ID is phase 6, after the phase 4 of jobs.md
and the phase 5 of [harnesses.md](../harnesses/harnesses.md): planned, with no check
yet.

## Requirements

| # | Requirement |
|---|---|
| JAR1 | Stack neutral: no build tool's name in the rules. The only names are the shells and wrappers that H14 and HR16 ([harnesses.md](../harnesses/harnesses.md)) already list. |
| JAR2 | Suggest only. Nothing is heavy until the user adds it (JR7); no measurement queues, flags or refuses a command. |
| JAR3 | Store only command keys (JA3), numbers, times and origins, per repository and on this machine: never an argument beyond a key, a folder but the repository's, a pid, a label, the environment or output. |
| JAR4 | No new crate, daemon, socket or OS tool. Linux, macOS and Windows through `sysinfo`. |
| JAR5 | Cheap: at most 1% of one core while sampling, no read beyond the heartbeat's while nothing is busy, a bounded file per repository. |
| JAR6 | Harness neutral: core reads no harness payload; an agent is known only through its records (H19, HR16). |

## Measured on this Mac, 2026-10-26

An M4 Max with 16 cores, 48 GiB and 945 processes, `sysinfo` 0.39.6, through scratch programs
outside the repository:

- A whole-table read with each process's CPU time and memory, one `System` kept and `cmd` and
  `cwd` read once per process: 7.4 ms p50, 10.5 ms p95, 7.6 ms of CPU. S4's read today, a new
  `System` with `cwd` each time: 6.7 ms p50, 6.8 ms of CPU. A read of 40 known pids: 0.2 ms. So a
  read every 2 s costs 0.4% of one core.
- `sysinfo`'s CPU time is the process's own (`pti_total_user` and `_system` on macOS, `utime`
  and `stime` on Linux), never its children's.
- With `rustc-wrapper = sccache`, rustc ran in some compiles as a child of the sccache server,
  itself a child of `launchd`, and in others as a child of the client `sccache rustc …`, inside
  cargo's tree. A cargo run's own tree holds only part of its compiles (JA4).
- A 15 minute prototype of JA1 to JA4, without JA4's name rule, read the table 449 times, every
  2 s, while agents worked: 8.5 ms p50 and 19.4 ms p95 a read. Runs of the old lock script,
  `heavy cargo xtask`, averaged 0.1 to 0.3 cores with peaks of 12.6, since the script waits for
  its slot inside the run; a `grep` ran 221 s at 1.0 core; each part of a pipe (`| tail`) was a
  light run of its own. A watch script that ran before the prototype started took 88% of its
  CPU from processes of its worktree that were not its own, the editor's among them, and one of
  them, counted whole at its first read, gave a peak of 183 cores: hence JA4's name rule and
  JA8's rules on runs and processes seen late.

## Signals

| Signal | Sees | Misses | Cost | Verdict |
|---|---|---|---|---|
| A harness's tool-call duration (Claude Code's PostToolUse `duration_ms`; any harness's call start to end) | an agent's foreground shell calls, queued or not | CPU and memory, so a network wait or a `sleep` looks like a build; background calls, whose call ends at launch (`duration_ms` 3 for an async Agent call in the 2.1.306 recordings) and whose H27 task ends only at a later Stop; terminal, task and editor commands; a wrapped call's queue wait counts; a compound command lumps its programs | none, already in the payload | no source in this revision (QA6) |
| The process table, sampled by a watcher (`sysinfo`: CPU time, resident memory, parent, start, arguments, folder) | every command of the user in a repository, from agents, terminals, tasks and jobs, queued or not | children born and gone between two reads, so CPU is a lower bound; work a long-lived server does (Docker's VM, a Gradle daemon, a Bazel server); everything while no watcher runs | 7.6 ms of CPU a read idle, 8.5 ms p50 a read under load, every 2 s while busy | the source (JA6 to JA8) |
| `wtpool job`'s own measurement | its command exactly: `getrusage(RUSAGE_CHILDREN)` counts every descendant that was waited for | the unqueued commands autodetect is for; `getrusage` needs `unsafe` or a crate, Windows a job object; sampling its own tree costs a whole-table read per job | 0 to 7.6 ms a read per job | no source: the sampler measures jobs' commands too, as origin `job` (JA5) |
| Load average, disk I/O, battery | the machine; per process I/O | which command caused it | small | not used; I/O is QA11 |

## Choices

| Choice | Options | Recommended, and why |
|---|---|---|
| What a command is | a tool call; a process name; a run, the outermost process a shell, a harness or a job started | runs (JA2): they cover terminals, tasks and jobs, and the process table says what they cost |
| Its key | H14's summary; J19's words; J19's program and the words before the first option | the last (JA3): H14 keeps a second word for eight programs only, J19 keeps option values such as `-p wtpool-core`, and each such key is a pattern that matches its run |
| Heavy | wall time; busy cores at a peak; busy cores on average over the run; peak memory | average busy cores or peak memory, over at least a minute (JA9): a server, a watcher or a network wait averages low |
| Thresholds | fixed numbers; shares of the machine | shares: a quarter of the cores, from 2 to 8, as J5's limit; a quarter of the memory |
| Who samples | every watcher; one per machine; jobs too | one watcher per machine, by a lock (JA6) |
| Where data lives | `<common>/`; the user's state folder | `<state>/wtpool/measure/` (JA12): the numbers are this machine's, one sampler covers every repository, and a watcher only reads the repository |
| Accepting | auto-accept; suggest only | suggest only (JR7, QA8) |
| Dismissals | in the measurement file; in `wtpool.toml` | `heavy_dismissed` in `<common>/wtpool.toml` (JA17): the user's choice, kept beside `heavy`, written in place as J6 writes it |

## Architecture

```
sysinfo process table ─► sampler, in one wtpool status --watch per machine (JA6) ─► runs (JA1 to JA5)
records (H19, HR16) and jobs (J8) ─┘           │ heavy runs, and later runs of their keys (JA14)
                                               ▼
<state>/wtpool/measure/<h>.json ─► suggestions (JA11) ─► S14 suggested, dismissed, autodetect ─► strip (U15)
<common>/wtpool.toml heavy, heavy_dismissed ─┘                                                      │
                       wtpool job heavy add | dismiss | restore ◄──────────── X21 ◄─────────────────┘
```

## 1. Runs

| ID | Behaviour |
|---|---|
| JA1 | A launcher only starts other processes: a process whose J19 program is one of the shells and wrappers of HR16 ([harnesses.md](../harnesses/harnesses.md)) (`sh`, `bash`, `zsh`, `dash`, `ksh`, `fish`, `pwsh`, `powershell`, `cmd`, `env`), so a shell that runs a script is none, as J19 names it by its script; a process whose J19 words start with a prefix H14 skips (`nix develop`, `timeout`, `time`, `nice`, `wtpool job`); the harness process of a live record (H19, HR16); and a live job's process (J8). A boundary is a harness process, a job's process, or an interactive shell: a shell launcher without `-c` (`/c` for `cmd`, `-Command` for `pwsh` and `powershell`). |
| JA2 | A process's run: walk its parents up to the nearest boundary, at most 64 steps, stopping where a pid comes back (as A1 does); the run is the outermost process on that path that is no launcher and whose parent is one. Without one the process is in no run. So `claude → zsh -c → cargo → rustc` is a run of `cargo`; `zsh → make → sh -c → cc` one run, `make`; `zsh -c → npm run build → sh -c → node vite.js` one run, `npm run build`; a VS Code task (`zsh -c` under the editor) a run; the editor's own `rust-analyzer → cargo check`, under no launcher, none. `wtpool job` is a launcher, so a job's run is its command, which starts after the wait: no run holds a queue wait. |
| JA3 | A run's key: its root's J19 program, then the arguments after it up to the first that starts with `-` or `+` or is not a word of `^[a-z][a-z0-9:-]{0,31}$`, at most two. `cargo xtask verify --part quick` gives `cargo xtask verify`, `cargo test -p wtpool-core` `cargo test`, `pnpm run test:e2e` itself, `pytest tests/test_api.py` `pytest`, `ssh build.example.com` `ssh`, and `python3 -m pytest -x` `pytest`. A key is a J18 pattern that matches its run under J19, so adding it makes that command heavy, and it is the only text of a run that is kept. |
| JA4 | Delegated work: a process in no run counts toward a run when it started after the run did, its folder lies in the run's worktree (H7) and in no other live run's, and at that read a process of the run has the delegated process's program name as its own, or as the basename of one of its first two arguments. So a compile that sccache's server runs for the client `sccache rustc …` counts toward the cargo run that waits for it, while a language server started earlier, or the editor's own `git`, does not. |
| JA5 | A run belongs to the repository whose worktree holds its root's folder (H7), when that repository's common folder holds `wtpool-activity/` (H6: wtpool is set up there); else it is not measured. Its origin is `job` when a live job's process is an ancestor (J8), else `agent` when the harness process of a live record is one (A1, HR16), else `other`: a terminal, a task or a script. |

## 2. Sampling

| ID | Behaviour |
|---|---|
| JA6 | The sampler is the `wtpool status --watch` of the user and machine that holds an exclusive lock on `<state>/wtpool/measure/sampler.lock`, through `std`'s file locks as J8 uses them. Each watcher tries it, without waiting, on each heartbeat (W7), and samples while it holds it; the OS frees the lock of a process that dies, so another watcher takes over within a heartbeat. With `enabled = false` (JA10) no watcher takes it. `wtpool status --json` never samples. |
| JA7 | The sampler reads the whole process table, with each process's CPU time and resident memory (`sysinfo`'s `accumulated_cpu_time` and `memory`), on each heartbeat, the read S4 already makes, and also every 2 s while a run used one core or more since the last read. It keeps one `sysinfo::System` for its life, reads `cmd` and `cwd` once per process (`UpdateKind::OnlyIfNotSet`), and CPU time and memory on each read. A read over 50 ms keeps it to the heartbeat until a read takes 50 ms or less. S4, A1, S15 and the CPU sign of HR32 ([harnesses.md](../harnesses/harnesses.md)) use the same read (arch F10's single process port). |
| JA8 | A run's CPU time is the sum, over its processes (JA2) and its delegated ones (JA4), of each one's CPU time gained since the last read; a process seen for the first time counts all of its own when it started after the read before, else only what it gains from then on. Its peak memory is the largest sum of their resident memory at one read; its wall time runs from its root's start to the last read that saw it, plus half a period. A run counts only when the first read that saw it came at most 12 s after its root started and the same sampler saw its root go; else it is dropped, never kept as a lower bound. |

## 3. Heavy runs and suggestions

| ID | Behaviour |
|---|---|
| JA9 | A run is heavy when its wall time is at least `min_seconds` and at most `max_minutes`, and either its CPU time over its wall time, its busy cores on average, is at least `cores`, or its peak memory is at least `memory_percent` of the machine's memory. A dev server, a watcher or an interactive program averages low over its life or outlasts `max_minutes`; a command that waits on the network averages low; an editor's `cargo check` is in no run (JA2). |
| JA10 | `machine.toml` (J4) gains `[autodetect]`: `enabled` (`true` or `false`), `cores` (1 to 64), `memory_percent` (5 to 100), `min_seconds` (10 to 3600), `max_minutes` (1 to 1440) and `runs` (1 to 10). Defaults: `true`; a quarter of the cores `available_parallelism` reports, kept from 2 to 8; 25; 60; 120; 3. J5's rules hold: an unknown key, a wrong type or a value out of bounds is an error, `wtpool job` exits 125, and the watcher samples nothing and reports it (JA19). `wtpool job autodetect` writes `enabled` (J6); the other keys are edited by hand. |
| JA11 | A key is suggested for its repository when at least `runs` of its last 10 kept runs are heavy under the thresholds of now, the newest of those at most 30 days old, no pattern of the heavy list (J20) matches it (J19), and no dismissal (JA17) does. It joins S14's `suggested` with `from` `measured` and its evidence. A J23 or J24 suggestion whose pattern matches the key takes the key's runs instead, as `cargo nextest` takes those of `cargo nextest run`, and shows once, as measured, with `also from <marker>`; its evidence counts the runs of every key it matches. A measured suggestion, like J23's, counts for showing builds (S4) while the repository has no list (J20), and for nothing else. A measured key may be `cargo check`, which no marker suggests (J23): measured evidence suggests it like any other key, and one Dismiss hides it (JA17). Under `enabled = false` none shows. |

## 4. Storage and privacy

| ID | Behaviour |
|---|---|
| JA12 | The folder is `<state>/wtpool/measure/`, `<state>` as in jobs.md, mode 0700, under J7's checks: a symlink, another owner, or a group or world write bit stops the sampler, which reports it (JA19). Each repository has `<h>.json`, `<h>` the first 16 hex digits of the SHA 256 of its common folder's real path. Only the sampler writes it, as H18 writes a record: a temp file opened `O_CREAT \| O_EXCL \| O_NOFOLLOW`, mode 0600, renamed over it, under `<h>.lock`; `wtpool job heavy forget` deletes it under the same lock. |
| JA13 | A file holds `version` 1, `common_dir`, `updated` and `commands`: per key, its kept runs, newest first, each with `ended` (RFC 3339 UTC), `seconds`, `cpu_seconds`, `peak_memory` (bytes) and `origin` (`agent`, `job` or `other`). A file of another version reads as no runs and is never written; one that does not parse is replaced at the next write. |
| JA14 | A run is kept when it is heavy, or when its key already has a kept heavy run, so a light command's key is never written. A key keeps its 10 newest runs, a repository the 64 keys with the newest runs. Each write drops runs over 30 days old and keys without runs, and deletes a file left without keys; once a day the sampler deletes each file whose `common_dir` is gone. A file stays under 100 KiB. A key the heavy list matches keeps no runs. |
| JA15 | Privacy: a file holds the repository's common folder, keys (JA3), numbers, times and origins. The sampler holds each process's arguments and folder in memory only, as S4 does, and drops them with the process. No argument leaves memory but the words of a key; nothing leaves the machine. |

## 5. The CLI and the strip

| ID | Behaviour |
|---|---|
| JA16 | Evidence reads `<H> runs`, or `<H> of <N> runs` when N kept runs hold H heavy ones; then `, median <C> cores for <D>` over the heavy runs, C their busy cores on average, one decimal under 10 and whole from 10, D their wall time, `N s` under a minute, `N min` under an hour, else `N h M min`; then `, peak <M> GiB`, their median peak memory, one decimal under 10, when memory made one of them heavy; then ` · by agents`, ` · in jobs` or ` · outside agents` when all the heavy runs share an origin; then ` · also from <marker>`. The CLI and the panel word it alike, from S14's numbers. |
| JA17 | `wtpool job heavy dismiss -- <pattern>…` adds each J18 pattern to `heavy_dismissed` in the repository's `<common>/wtpool.toml`, written in place as J6 writes `heavy`, and `restore -- <pattern>…` removes it; a pattern already there, or not there, exits 125 and changes nothing. A dismissal hides each measured suggestion it matches (J19), so `cargo check` also hides `cargo check foo`. A dismissed key keeps its runs, so Restore shows its evidence at once. `wtpool job heavy forget` deletes this repository's measurements (JA12), prints the path, and keeps the dismissals. |
| JA18 | `wtpool job heavy suggest [--json]` (J3) prints each suggestion with `from <marker>` or `measured: <evidence>`, then each dismissal with its evidence, then the sampler's line: `measured since <date> by wtpool status --watch`, `not measured: no wtpool status --watch runs on this machine`, `not measured: autodetect is off in <path of machine.toml>`, or the error. `--json` prints S14's `suggested`, `dismissed` and `autodetect`. `wtpool job ls` (J2) words a measured suggestion as `suggest` does. `wtpool job autodetect [on\|off\|default]` prints `enabled` with its source, or sets it (J3). |
| JA19 | The document (S14, additive under S12): a `suggested` entry may hold `measured`, `{runs, heavy, cores, seconds, peak_memory, origin, also}`, `peak_memory` null unless memory made a run heavy, `origin` `agent`, `job`, `other` or null for a mix, `also` a marker or null; `from` stays an open string and reads `measured` for a suggestion only measurement makes. `dismissed` lists `{pattern, measured}`. `autodetect` holds `enabled`, `sampling` (a sampler holds the lock now), `since` (the oldest kept run's `ended`, or null), `cores`, `memory` (bytes), `min_seconds`, `max_minutes`, `runs` and `error`. A wtpool without autodetect omits all three. |

The strip (U15, X11, X21 and X22): under `Suggested for this repository` the group
`measured on this machine` comes first, one suggestion a line: the dashed chip with its add
button, then `Dismiss`, and the evidence under them in the muted colour. The marker groups
follow; then the note `A run is heavy at <cores> busy cores on average or <memory> GiB at its
peak, over at least <min>; <runs> heavy runs make a suggestion. Measured since <date> by wtpool
status --watch.`, or why nothing is measured; then, while any, `Dismissed for this repository ·
.git/wtpool.toml` with a restore chip each.

| Mockup | Shows |
|---|---|
| [Measured](attachments/Measured.png), [Measured-600](attachments/Measured-600.png) | four measured suggestions with their evidence, one also from `Cargo.toml` and one heavy by memory, above the `Cargo.toml` group, at 1100 and 600 px |
| [MeasuredDismissed](attachments/MeasuredDismissed.png) | `cargo check` dismissed: gone from the measured group and listed under Dismissed with its restore button, the status line, focus on the next suggestion |

[render.mjs](attachments/render.mjs) renders them with the jobs mockups;
[autodetect.css](attachments/autodetect.css) holds their parts. Only the rows bind.

## 6. Harness neutrality

| ID | Behaviour |
|---|---|
| JA20 | No rule here reads a harness's payload or tool names. Runs come from the process table, and an agent is a record's `process` (H19), found as HR16 of [harnesses.md](../harnesses/harnesses.md) finds it for every harness; launchers and boundaries are the same for every harness, and `wtpool run` (HR23) makes a harness without hooks a boundary too. Tool-call durations are no source (QA6). A later revision that uses them reads an optional `duration_ms` of the neutral `call_end` (HR5, an addition HR1 allows), which the Claude Code adapter fills from PostToolUse's `duration_ms`; core never reads a Claude Code payload. |

## Interplay

| With | Then |
|---|---|
| Stack markers (J23, J24) | one list under `Suggested for this repository`, measured first; a marker suggestion that matches a measured key shows once, measured, `also from <marker>`; `Add all` adds every suggestion shown, measured ones too |
| The heavy list (J20) | a pattern that matches a key ends its suggestion and its runs (JA11, JA14); removing the pattern starts the count again |
| The queue | a job's command is a run like any other, origin `job`, its wait left out (JA2); measuring never changes what `wtpool job` does |
| `warn` (J21) | a measured-heavy command outside the list is never `(not queued)`: only the list is heavy (JR7) |
| `require` (J21, H29) | a measured suggestion never refuses a call: H29 reads only the list |
| A measured-heavy command never queued | shows only as a suggestion, its evidence `by agents` or `outside agents`; while the repository has no list, its process also shows as a build (S4, J20) |
| `· N suggested` in the heading (U10, jobs Q16) | counts measured suggestions too, as they are in S14's `suggested` |

## User flows

Each flow is an e2e flow of phase 6, `flow__<name>`.

**`autodetect_suggests`** (JA2, JA3, JA8, JA9, JA11, JA16, JA19, U15, X21)
1. Under a scripted process table, an agent's harness process in wt1 runs `zsh -c 'cargo xtask verify --part quick'` three times, 12 min each at 11 busy cores.
2. Within one heartbeat of the third run's end, the strip shows `cargo xtask verify` under `measured on this machine` with `3 runs, median 11 cores for 12 min · by agents`, and `wtpool job heavy suggest` prints the same evidence.
3. Press its add button: `wtpool job heavy add --repo -- 'cargo xtask verify'` runs, the chip moves to This repository, and the measured group goes.

**`autodetect_dismiss`** (JA17, U15, X11, X21, X22)
1. The measured group lists `cargo check` with `3 of 7 runs, median 6.4 cores for 2 min · outside agents`.
2. Focus its chip and press Delete, or press Dismiss: `wtpool job heavy dismiss -- 'cargo check'` runs, `.git/wtpool.toml` gains `heavy_dismissed` and keeps its other keys and comments, the chip moves to `Dismissed for this repository`, the live region says `Dismissed cargo check.`, and focus moves to the next suggestion.
3. Press its restore button: `wtpool job heavy restore -- 'cargo check'` runs, and it shows again with the same evidence.

**`autodetect_light`** (JA2, JA4, JA9, JA14)
1. Scripted: `npm run dev` runs 3 h at 0.3 cores in wt2's terminal; an agent's `curl` waits 5 min on the network; the editor's `rust-analyzer` runs `cargo check` at 8 cores for 2 min, three times, in wt2.
2. No suggestion shows, and no measurement file holds a key.

**`autodetect_off`** (JA6, JA10, JA18)
1. Run `wtpool job autodetect off`.
2. Within one heartbeat no watcher holds the sampler lock, the measured group goes, and the note reads `Not measured: autodetect is off in ~/.config/wtpool/machine.toml.` The file stays until `wtpool job heavy forget` or its runs age out.

## Failure modes

| Case | Sampler, CLI | Panel |
|---|---|---|
| no watcher runs, as with no Worktrees tab open | nothing is measured, a run whose sampler stops is dropped (JA8), and `suggest` says `not measured` | a tab that opens starts a watcher, which takes the lock within a heartbeat |
| the lock cannot be taken: a file system without locks, an unsafe folder (JA12) | no sampling; `autodetect.error` names the folder | the note shows the error in the problem colour |
| `[autodetect]` invalid | `wtpool job` exits 125; no watcher samples | the note shows the error |
| `XDG_STATE_HOME` differs between watchers | two samplers, two folders | `suggest` and the note name their folder |
| a measurement file of a newer version, or one that does not parse | read as none; never written, or replaced at the next write | no measured suggestions until runs come again |
| a compiler cache's server (sccache) | its compiles count by JA4 | |
| a long-lived server does the work: Docker's VM, a Gradle daemon, a Bazel server | not seen, so such commands measure light | the stack markers suggest `gradle`, `bazel build` (J23) |
| a wrapper that waits inside its run, as the old lock script did | its wait lowers its average | move it to `wtpool job` (jobs.md, Adoption) |
| a harness without hooks run from a terminal | its agent's commands make one run, the harness's, which outlasts `max_minutes` | nothing suggested; `wtpool run` (HR23, [harnesses.md](../harnesses/harnesses.md)) makes it a boundary |
| children that live under a second | lost between reads: CPU is a lower bound | builds still pass: sustained parallel work has long-lived children |
| the machine sleeps during a run | the wall time holds the sleep, so the average drops | |
| a reused pid | a process is its pid and start time, as H19's | |
| 10 000 processes | a read over 50 ms keeps the sampler to the heartbeat (JA7) | |
| a process whose arguments or folder cannot be read | its key is its name alone (J19); without a folder it is in no repository | |
| a hostile repository | it cannot write a measurement or a dismissal: only the sampler writes, from the process table; a command must really burn CPU to be suggested, and the user decides | suggestions shown as text only (X10) |

## Privacy and security

| Topic | Rule |
|---|---|
| What is stored | per repository, on this machine: the common folder, keys, numbers, times and origins (JA13, JA15); dismissals in `.git/wtpool.toml` (JA17) |
| What is read | the process table through `sysinfo`, with arguments and folders in memory only, as S4 reads them; no file of a process is opened |
| Who writes | the sampler, one watcher (JA6); `wtpool job heavy dismiss`, `restore` and `forget`; `wtpool job autodetect`. The panel writes nothing (X21) |
| Who reads | the user: the folder is 0700, its files 0600 (JA12) |
| `wtpool status` | `--json` writes nothing (S2); the sampler of `--watch` writes only its folder |
| Network | none |

When it is built, SECURITY.md (What wtpool stores) and docs/privacy-and-uninstall.md list the
folder, its fields and `wtpool job heavy forget`.

## Verification plan

Every new ID is phase 6. They stay out of `verify/trace.toml`, the User flows table and the
scenario catalog until approval; then the phase 6 oracle author adds them in one change entry,
and verification.md takes this section.

Test interfaces it adds:
1. **Cargo feature `verify`:** `WTPOOL_VERIFY_PROCS` lines gain `cpu_ms` (the CPU time so far) and `rss` (bytes), beside jobs.md's `cmd`; the file is read again on each read, so a scenario steps it. `WTPOOL_VERIFY_MEMORY=<bytes>` replaces the machine's memory, as `WTPOOL_VERIFY_CORES` (jobs.md) the cores; `WTPOOL_VERIFY_SAMPLE_MS=<n>` replaces the 2 s; times come from the verify clock; each read appends a line to `WTPOOL_VERIFY_SAMPLE_LOG`.
2. **The state folder** under the sandbox `HOME` and `XDG_STATE_HOME`.
3. **Webview input:** S14's `suggested[].measured`, `dismissed` and `autodetect`.
4. **DOM:** `ul.heavy-measured` of `li` with `.heavy-chip[data-from="measured"]`, `button.suggest-add`, `button.measured-dismiss` and `.measured-evidence`; `.measured-note`; `ul.heavy-chips[data-scope="dismissed"]` of `li.heavy-chip.dismissed` with `button.restore-dismissed`.
5. **Fake wtpool:** answers `job heavy dismiss` and `restore` as it answers `job heavy`.

| ID | `unit` | `int` | `render`, `host` | `shot` | `e2e` |
|---|---|---|---|---|---|
| JA1, JA2 | `runs__should_find_the_outermost_root_below_a_boundary` (each example of JA2, `bash ci.sh`, `pwsh -Command`, a chain that loops, 65 steps) | `measure__should_group_processes_into_runs` (through `WTPOOL_VERIFY_PROCS`: agent, terminal, task, job, editor) | | | `autodetect_light` |
| JA3 | `key__should_stop_at_the_first_option` (each example, `cargo +nightly build`, `.exe`, a 33 character word, Unicode, `_`) | | | | |
| JA4 | | `measure__should_count_delegated_work` (a cache server's compile during a cargo run; a language server started before; the editor's `git`; two runs in one worktree) | | | `autodetect_light` |
| JA5 | | `measure__should_attribute_repository_and_origin` (no `wtpool-activity/`, no repository, a job's child, an agent's, a terminal's) | | | |
| JA6 | | `sampler__should_be_one_per_machine` (two watchers, one samples; the holder killed: the other within a heartbeat; `enabled = false`; `--json`) | | | `autodetect_off` |
| JA7 | | `sampler__should_read_on_the_heartbeat_and_while_busy` (the sample log: idle, one busy run, a 60 ms read) | | | |
| JA8 | `run__should_sum_cpu_and_peak_memory` | `measure__should_drop_runs_seen_late_or_cut` (first read 13 s after start; the sampler killed mid run) | | | |
| JA9 | `heavy__should_follow_the_rule` (3 h at 0.3 cores, 5 min at 0.1, 12 min at 11, 50 s at 12, 13 GiB at 1 core, 121 min at 10, each threshold exactly) | | | | `autodetect_light` |
| JA10 | | `autodetect_config__should_validate_and_default` (each key's bounds; defaults at 1, 4, 16 and 64 cores; an unknown key exits 125; the watcher reports it) | | | `autodetect_off` |
| JA11 | `suggest__should_need_runs_of_the_last_10` (2, 3, 3 of 11, 31 days) | `suggest__should_skip_listed_and_dismissed` (`cargo` listed covers `cargo test`; a dismissal; a J23 overlap shown once with `also`; `cargo check` measured heavy is suggested, and dismissed is hidden) | | | `autodetect_suggests` |
| JA12 | | `measure__should_write_privately_and_atomically` (modes, a symlinked or shared folder refused, rename), `forget__should_delete_this_repository` | | | |
| JA13 | | `measure__should_read_only_its_version` (version 2 untouched, garbage replaced) | | | |
| JA14 | | `measure__should_bound_the_file` (light keys never written, 11 runs, 65 keys, 31 days, a gone `common_dir`, 100 KiB at the caps) | | | `autodetect_light` |
| JA15 | | `measure__should_store_only_keys_and_numbers` (a token argument, a folder name, a label: no byte of them under the folder) | | | |
| JA16 | `evidence__should_word_the_numbers` (each form, 9.96 cores, 59 s, 61 min, 10.0 GiB) | | `measured__should_word_evidence_as_the_cli` | | `autodetect_suggests` |
| JA17 | | `dismiss__should_edit_in_place` (comments and keys kept, twice, missing, a longer key hidden) | | | `autodetect_dismiss` |
| JA18 | | `suggest_cli__should_print_sources_and_the_sampler` (each sampler line, `--json` against `verify/schema/`), `autodetect_cli__should_print_and_set` | | | `autodetect_suggests`, `autodetect_off` |
| JA19 | | `status__should_list_measured_suggestions` (fields, `from` `measured`, all omitted without autodetect) | `machine_strip__should_show_measured_suggestions` | | `autodetect_suggests` |
| JA20 | | `measure__should_name_no_harness_or_tool` (a scan of the measure module's source: no adapter imported, no harness, tool or build tool named) | | | |
| U15 | | | `heavy_editor__should_show_measured_and_dismissed` | `measured__should_clip_nothing_in_every_theme` (1100 and 600 px, axe, chip outlines at 3:1, evidence at 4.5:1) | `autodetect_suggests`, `autodetect_dismiss` |
| X11, X21 | | | `messages__should_ignore_everything_outside_the_set` gains `dismiss` and `restore` (a pattern not suggested or not dismissed, the `machine` scope); `job_settings__should_run_wtpool_once_each_and_report` gains their argv | | `autodetect_dismiss` |
| X22 | | | `heavy_editor__should_follow_the_keyboard` gains the measured and dismissed lists | the focus checks on Dismiss and restore | `autodetect_dismiss`, `keyboard_only` |

The requirements stand for the checks of the rows they name: JAR1 JA1, JA3, JA20. JAR2 JA11, J21.
JAR3 JA13, JA15. JAR4 JA6, JA7. JAR5 JA7, JA14, B16, B17. JAR6 JA20.

Budgets: B16 (6): over 10 min with 1000 processes and 4 busy runs, the sampler's CPU time is at
most 1% of one core's, and while nothing is busy it makes no read beyond the heartbeat's,
counted by the sample log. B17 (6): the watcher's resident memory grows by at most 16 MiB with 2000
processes. The `unit` tests, which decide nothing, cover the run walk, the key, the rule, the
suggestion count and the evidence text.

## Phase 6 additions in jobs.md

jobs.md holds these, each marked from phase 6:

1. J3: `wtpool job heavy dismiss`, `restore` and `forget`, and `wtpool job autodetect [on|off|default]`; `heavy suggest` prints measured suggestions, dismissals and the sampler's line (JA17, JA18).
2. J2: `wtpool job ls` words a measured suggestion as `suggest` does.
3. J4, J5: `machine.toml` gains `[autodetect]` (JA10), under J5's rules.
4. J6: `heavy_dismissed` in `<common>/wtpool.toml`, written in place as `heavy` is.
5. J20: measured suggestions count for showing builds while the repository has no list, as J23's do.
6. J23 and Q13: no marker suggestion is `cargo check`, but measured evidence may suggest it; the rule that a suggestion the heavy list holds is not shown covers measured ones.
7. Q18 (resolved: not now) stays for marker suggestions; JA17 dismisses measured ones only (QA2).
8. Privacy and security: a row for the measurement folder and `heavy_dismissed`; `What the panel runs` adds `dismiss` and `restore`; `Reads by wtpool status` adds CPU time and memory.
9. §2's S14 and U15 rows: the measured suggestions, dismissals and sampler state, `from` staying an open string in the schema so `measured` is additive (arch C2), and the measured and dismissed lists.
10. Verification plan: `WTPOOL_VERIFY_PROCS` lines gain `cpu_ms` and `rss` beside `cmd`.

## Open questions

| # | Question | Answer |
|---|---|---|
| QA1 | Measure by default? | Resolved: yes, as it only suggests, writes the user's own state folder, and `wtpool job autodetect off` stops it. |
| QA2 | Should a dismissal also hide a marker suggestion? | Resolved: not now, as jobs.md Q18 answers for markers: a marker suggestion stays put, while measured ones keep coming back with new runs and need the dismissal. The one `heavy_dismissed` list can take markers later. The marker rule still leaves `cargo check` out (J23), measured evidence may suggest it, and one Dismiss hides it (JA11). |
| QA3 | Learn for the machine's list too, from runs outside any repository? | Resolved: no; the machine's list is the user's own, and runs outside a repository are not kept. |
| QA4 | Suggest removing a pattern whose runs measure light? | Resolved: later; this revision keeps no runs of listed keys (JA14). |
| QA5 | Should a running `wtpool job` sample while no watcher runs? | Resolved: no; each job would read the whole table, and the strip, where suggestions show, runs a watcher. |
| QA6 | Use tool-call durations as a weak source? | Resolved: no; wall time cannot tell a build from a network wait, background calls end at launch, and a job's call holds its wait. |
| QA7 | Keep the memory criterion? | Resolved: yes, at 25%: a memory-bound run, such as a big link or a JVM test suite, overloads a machine as parallel builds do. |
| QA8 | Auto-accept after enough runs? | Resolved: no, at no level: an accepted pattern can refuse an agent's call under `require`, and JA4's attribution needs a human look. |
| QA9 | Merge sibling keys, as `cargo test a` and `cargo test b` into `cargo test`? | Resolved: no; JA3 stops at the first option, so siblings are rare, and the user can type the shorter pattern. |
| QA10 | Count delegated work (JA4)? | Resolved: yes; with sccache, a cargo build's cacheable compiles can run under its server, the name rule keeps most of the editor's work out, and what it lets in, such as the editor's `cargo check` during a build in the same worktree, only adds to a run that is busy already. |
| QA11 | Disk I/O as a third criterion (`sysinfo`'s `disk_usage`)? | Resolved: no; a queue that limits cores helps an I/O-bound install or copy little, and builds that read much also pass the cores rule. |
| QA12 | Phase 6, after the harness design, or with jobs in phase 4? | Resolved: phase 6, as it needs J23's suggestions in the strip, the single process port (arch F10) and the process rule of HR16 ([harnesses.md](../harnesses/harnesses.md)). |
