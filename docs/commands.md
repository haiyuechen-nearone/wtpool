# Commands

[README](../README.md) · **Commands** · [Configuration](configuration.md) ·
[Status and hook](status-and-hook.md) · [Panel](panel.md) · [Heavy jobs](heavy-jobs.md) ·
[Development](development.md)

```
wtpool ls                              what is parked where (also plain `wtpool`)
wtpool fg   <ref> [--refresh|--reset] [--force]
                                       bring a branch or PR into the foreground, print its path
wtpool bg   <ref> [--refresh|--reset]  park a branch or PR in a slot, print its path
wtpool free <slot|branch> [--force]    detach a slot from its branch (files stay)
wtpool path <branch>                   print the worktree that holds <branch>
wtpool new  [name]                     add a slot (the name must start with slot_prefix)
wtpool rm   <slot>                     remove a slot
wtpool setup                           fill the pool up to slot_count, copy what slots lack
wtpool config [--json]                 print each setting and where it came from
wtpool status --json                   print the pool as one JSON document
wtpool status --watch                  print it again on each change, until stdin closes
wtpool status --schema                 print the JSON schema of that document
wtpool hook                            record one Claude Code hook event from stdin
wtpool hook install                    print the hook block for $CLAUDE_CONFIG_DIR/settings.json
wtpool hook gc                         delete old activity records
wtpool hook policy                     judge one Claude Code shell call against the heavy job policy
wtpool job [--label <l>] [--quiet] -- <command>…
                                       run a heavy command once fewer jobs than the limit run
wtpool job ls [--json]                 print the limit, the policy, the heavy commands and the jobs
wtpool job limit [<N>|default]         print or set this machine's limit on heavy jobs
wtpool job enforce [off|warn|require|default]
                                       print or set the policy for heavy commands outside a job
wtpool job heavy                       print the heavy commands, each with its source
wtpool job heavy add [--repo] [--suggested] <pattern>…
                                       add heavy commands to this machine's list or the repository's
wtpool job heavy rm [--repo] <pattern>…
                                       remove heavy commands from that list
wtpool job heavy suggest [--json]      print the heavy commands suggested for this repository
wtpool help                            print the help text
wtpool --version                       print the version, commit and cargo features
```

`wtpool` works on the pool of the repository you run it in, or the one `WTPOOL_FOREGROUND` names; outside
a repository it refuses. `wtpool job` is the exception: it needs no repository, and only
`heavy --repo` does. Outside a repository, `heavy suggest` prints `suggested: none`.
`wtpool status` and `wtpool hook` are described in [Status and hook](status-and-hook.md), the
settings and `wtpool config` in [Configuration](configuration.md), and how to start with heavy
jobs in [Heavy jobs](heavy-jobs.md).

## Refs, `--refresh` and `--reset`

- `<ref>` is a PR number, a GitHub PR URL or a branch name. A fork PR becomes `pr-<n>/<branch>`.
- PR numbers and URLs need `gh` and a `remote` on github.com (default `origin`, see
  [`remote`](configuration.md#remote)). GitHub Enterprise is not supported.
- A branch that is not local is fetched from that remote and tracks it. Without the remote, `wtpool`
  says there is no such branch and no remote to look it up.
- An existing local branch is reused as is. `wtpool` warns when it lags or has diverged from the
  remote head, and never clobbers unpushed commits.
- `--refresh` fetches and fast forwards the local branch to the remote head.
- `--reset` also moves a diverged branch (a rebased PR) to the remote head, after saving its old
  tip as `refs/wtpool-backup/<branch>/<sha>`.

## `bg` and `fg`

- `bg` and `fg` print only the worktree's path on stdout. Every other line, such as
  `feature-x -> app-wt1`, goes to stderr. So `cd "$(wtpool bg feature-x)"` works.
- `bg` prints the slot that already holds the branch, if any. Else it reuses a free slot, or adds
  one while the pool is below `slot_count`. Otherwise it fails at once and says why each slot is
  busy, one line per slot: the branch it holds, its uncommitted files, or why it cannot be reused.
- `bg` refuses a branch that the foreground or an unmanaged worktree has checked out.
- `bg` never creates a new branch. The branch must exist locally or on the remote. For a new one,
  create it first: `git branch feat && wtpool bg feat`.
- `fg` checks the branch out in the foreground. When a slot holds it, `fg` first releases that
  slot, which then shows as detached.
- `fg` refuses a branch that an unmanaged worktree has checked out, as `wtpool` never detaches
  or resets a worktree outside the pool. Work on it there, or release it with
  `wtpool free <branch>`.
- A new slot gets the `copy` files that git ignores in the main worktree, then runs
  `on_new_slot`, when `wtpool.toml` sets them. See
  [New slots](configuration.md#new-slots-copy-and-on_new_slot). When either fails, `bg` still
  parks the branch there, then exits non-zero.

The last line of each command below is its stdout. The other lines are stderr.

```console
$ cd ~/code/app
$ wtpool bg feature-x
feature-x: already local
feature-x -> app-wt1
/Users/you/code/app-wt1
$ wtpool fg feature-x
feature-x: already local
releasing feature-x from app-wt1
feature-x -> app
  92ed926 init
/Users/you/code/app
```

When no slot is free:

```console
$ wtpool bg fix-login
wtpool: app-wt1 holds branch feature-x
wtpool: app-wt2 has 2 uncommitted file(s)
wtpool: app-wt3 holds branch review
wtpool: no free slot and the pool is at its 3 slot limit; release one with 'wtpool free <slot>' (see 'wtpool ls')
```

## `wtpool ls` states

```console
$ wtpool ls
WORKTREE    BRANCH      ROLE        STATE
app         main        foreground  clean
app-review  review      unmanaged   clean
app-wt1     feature-x   slot        clean
app-wt2     fix-login   slot        clean
app-wt3     (detached)  slot        clean
```

| STATE | Meaning |
|---|---|
| `clean` | nothing below applies |
| `dirty:N` | N uncommitted or untracked files |
| `ahead:N`, `behind:N` | commits compared with the upstream |
| `unreferenced:N` | N commits that no branch, tag, remote branch or backup ref holds |
| `rebase`, `am`, `merge`, `cherry-pick`, `revert`, `bisect` | operation in progress |
| `dirty:?`, `unreferenced:?`, `operation:?` | git could not tell, so the slot counts as not safe |
| `upstream:?` | git could not count against the upstream (display only) |

A slot is free only when it is detached and shows none of these.

## Heavy jobs: `wtpool job`

`wtpool job` runs builds, test suites and other heavy commands under one limit per user and
machine, shared by every repository. [Heavy jobs](heavy-jobs.md) is the guide; this section is
the reference.

### `wtpool job -- <command>`

```console
$ wtpool job -- cargo test
$ wtpool job --label "unit tests" -- npm test
$ cd ~/code/app-wt1 && wtpool job -- nix develop -c cargo nextest run
```

- It runs the command once fewer jobs than the limit run, and waits until then. Waiting jobs start
  in the order they came. A job that dies, even by `SIGKILL`, frees its place at once.
- The command runs exactly as given. There is no shell: `<command>` is a program found on `PATH`,
  then its arguments. stdin, stdout and stderr are the command's own, so a terminal stays a
  terminal. `wtpool job` never reads stdin and never writes stdout.
- On stderr it writes one line when it starts to wait, such as
  `wtpool job: 4 of 4 jobs run; waiting, 2 ahead`, and one when it starts after a wait, such as
  `wtpool job: started after 3 m 12 s`. Nothing otherwise, and nothing with `--quiet`.
- `--label <l>` names the job in `wtpool job ls` and the panel. Without one, a job shows as the
  words of the heavy pattern it matches, such as `cargo test`, else as its program, plus one
  subcommand word for tools such as `git`, `cargo` or `npm`.
- The command gets `WTPOOL_JOB=<id>`. A `wtpool job` inside a live job runs its command at once, in
  that job's place, so a job inside a job never waits for its parent's place.
- Jobs live in `~/.local/state/wtpool/jobs/`, or `$XDG_STATE_HOME/wtpool/jobs/`. Processes that see
  different `XDG_STATE_HOME` values form two queues. A job records its summary, label, folder, pid
  and times, never its arguments, environment or output
  ([Privacy](privacy-and-uninstall.md#what-a-job-records)).
- On Unix, while the command runs, `wtpool job` ignores `SIGINT` and `SIGQUIT`, which a terminal
  sends to the whole foreground group, and passes `SIGTERM` and `SIGHUP` on to the command. While
  it waits, any of the four takes it out of the queue and ends it. On Windows, Ctrl+C reaches the
  command through the console.
- Do not wrap a server, a watcher or anything else that does not end: it holds its place until it
  stops.

| Exit | Meaning |
|---|---|
| the command's | the command ran. A command ended by `SIGINT`, `SIGQUIT`, `SIGTERM`, `SIGHUP`, `SIGPIPE` or `SIGKILL` ends `wtpool job` by the same signal; any other signal N gives 128 + N |
| 125 | `wtpool job` itself failed, before the command ran: an unknown option, no `--` or nothing after it, an invalid `machine.toml`, an unsafe job folder, a file system without locks |
| 126 | the command was found but cannot start |
| 127 | the command was not found |

These are the codes `env` uses for its own failures.

### `wtpool job ls`

`wtpool job ls` prints the rules first: the limit and the policy with their sources, the path of
`machine.toml`, each heavy pattern with its source, this repository's suggestions while it has
any, and the form to run heavy commands in. Then the running jobs, the waiting ones in queue order,
and the heavy processes outside any job, each with its label or summary, folder and age.

```console
$ cd ~/code/app-wt1
$ wtpool job ls
limit: 2 (machine)
enforce: warn (default)
config: /Users/you/.config/wtpool/machine.toml
heavy: just (machine)
heavy: cargo test (repository)
heavy: cargo build (repository)
suggested: cargo nextest (Cargo.toml), cargo clippy (Cargo.toml); add them with: wtpool job heavy add --repo --suggested
run heavy commands as: wtpool job -- <command>
running (2 of 2):
  cargo test  /Users/you/code/app-wt1  3 m
  unit tests  /Users/you/code/app-wt2  12 s
waiting (1):
  1. cargo build  /Users/you/code/app  5 s
not queued (1):
  cargo build  /Users/you/code/app-wt3  40 s (not queued)
```

Waiting jobs carry their place in line. Under `enforce = "off"` the last list reads
`outside any job`, and its lines have no `(not queued)`. When `machine.toml` is not valid,
`wtpool job ls` still prints and exits 0: an `error:` line comes first, the `limit:` line is
left out, and the running list reads `of ?`. `--json` prints the same as one
JSON object: `limit`, `default`, `min`, `max`, `enforce`, `source`, `config`, `error`, `heavy`,
`suggested`, `running`, `waiting` and `outside`, each job with `id`, `summary`, `label`, `folder`
and `since`. It is the `machine.jobs` object of `wtpool status --json`, with folders in place of
worktrees and agents.

### `limit`, `enforce` and `heavy`

Each prints a setting with its source, or sets it and prints the new value. `default` removes the
key from `machine.toml`, so the default applies again.

```console
$ wtpool job limit
limit: 4 (default)
$ wtpool job limit 6
limit: 6 (machine)
$ wtpool job enforce require
enforce: require (machine)
$ wtpool job limit default
limit: 4 (default)
```

- `wtpool job limit [<N>|default]`: how many jobs run at once, a whole number from 1 to 64. The
  default is a quarter of the cores the system reports, rounded down and kept from 1 to 8, so 16
  cores give 4. A lowered limit stops no job: the running jobs finish, and the next starts once
  fewer than the new limit run. A raised limit starts the first waiting jobs within a second.
- `wtpool job enforce [off|warn|require|default]`: the policy for heavy commands outside a job,
  `warn` by default. [warn and require](heavy-jobs.md#warn-and-require) says what each does.
  Under `require`, it and `wtpool job ls` note on stderr when Claude Code's `settings.json`
  neither enables the plugin nor holds the `wtpool hook policy` entry, so nothing refuses yet.
- `wtpool job heavy` prints the heavy patterns, each with its source: `machine`, `repository` or
  `build_processes`.
- `wtpool job heavy add <pattern>…` adds patterns to this machine's list, used in every
  repository. With `--repo` it adds them to this repository's `heavy` in its
  `.git/wtpool.toml`. `add --repo --suggested` also adds each current suggestion that the list
  lacks, so it needs no pattern.
- `wtpool job heavy rm <pattern>…` removes patterns from this machine's list, or with `--repo`
  from this repository's. `rm --repo` also takes a `build_processes` name.
- `wtpool job heavy suggest` prints this repository's suggestions, each with the marker it came
  from, and writes nothing. `--json` prints them as JSON.
- `add` and `rm` print the list they changed, one `heavy:` line per pattern, or `heavy: none`.

Quote a pattern of several words, so that it is one argument. `--` before the patterns is
optional; the panel always passes it:

```sh
wtpool job heavy add --repo -- 'cargo xtask verify' make
wtpool job heavy add -- just
wtpool job heavy rm --repo -- protoc
```

A write changes only the edited value. Every other byte of the file, other keys and comments
included, stays. wtpool writes a new file next to it and renames it over the old one, under a lock:
`machine.toml.lock` for the machine config, `wtpool.lock` for `wtpool.toml`.

Each exits 125 and changes nothing for a value out of bounds, an invalid pattern, a pattern added
twice or removed while it is not listed, a file that is not valid TOML, a file that is a symlink
(the error names its target, to edit by hand), and a comment inside the `heavy` list it would
replace (the error names the file and the line). A file whose only fault is a value out of bounds
is still rewritten, so `wtpool job limit 4` fixes `limit = 99`.

### Heavy patterns

A heavy pattern names a command by its first words, such as `make`, `cargo test` or
`pnpm run build`.

- One to three words, split by single spaces.
- Each word has 1 to 32 characters from `A-Z a-z 0-9 . _ + -`. Words after the first may also hold
  `:`. No word starts with `-` or `+`.
- The first word names a program. Only the first word may end in `*`, which matches any end of the
  program's name.
- No regular expressions.

`make`, `cargo build`, `cargo xtask verify`, `cargo-*`, `npm run build` and `pnpm run test:e2e`
are patterns. `*`, `make *`, `-rf` and `cargo  build` (two spaces) are not.

A command's words are its program's name, without its folder, then its next two arguments that do
not start with `-` or `+`. So `cargo +nightly test -p app` reads `cargo test app`, which the
pattern `cargo test` matches: a pattern matches when its words equal the command's first words.

- In a shell command, such as an agent's Bash call, each simple command between `;`, `&&` and the
  like counts on its own, after the prefixes `VAR=value`, `env`, `timeout N`, `time`, `nice`,
  `nix develop -c` and `wtpool job … --`.
- In the process table, a script's interpreter gives way to the script, without its folder and
  extension: `node node_modules/vite/bin/vite.js build` reads `vite build`, and
  `python3 -m pytest` reads `pytest`. This holds for `node`, `python`, `python3`, `ruby`, `perl`,
  `sh`, `bash` and `zsh`. npm names its process after its command, as `npm test`. A process whose
  arguments cannot be read is its name alone.
- On Windows `.exe` is dropped and names compare without case.

The heavy list is this machine's patterns plus those of the repository a command runs in. Nothing
is built in: both lists start empty. While a repository has no list of its own, its suggestions
show running builds in `wtpool status` and the panel, and do nothing else: they never queue, flag
or refuse a command.

### The machine config

The limit, the policy and the machine's own patterns live in `~/.config/wtpool/machine.toml`, or
`$XDG_CONFIG_HOME/wtpool/machine.toml`. Without `HOME`, as on Windows, `~` is the system's home
folder.

```toml
# ~/.config/wtpool/machine.toml
[jobs]
limit = 4
enforce = "warn"
heavy = ["just", "cargo xtask verify"]
```

| Key in `[jobs]` | Value | Default |
|---|---|---|
| `limit` | a whole number from 1 to 64 | a quarter of the cores, 1 to 8 |
| `enforce` | `off`, `warn` or `require` | `warn` |
| `heavy` | a list of patterns, used in every repository | `[]` |

- No environment variable sets it, and no repository can: every job of your user reads one limit
  and one policy.
- A missing file or key takes the default. An unknown key, a wrong type, a value out of bounds, a
  bad pattern or an unreadable file is an error: `wtpool job` reports it, naming the file and the
  fault, and exits 125 before it runs anything. wtpool never falls back to a default.
- Reading follows a symlink to a regular file. Writing refuses one.
- On Unix the file and its folder must belong to you and be writable by no one else. A new
  `machine.toml` gets mode 0600.

### A repository's heavy commands

A repository adds patterns in its `.git/wtpool.toml` (the git common directory), as the top-level
key `heavy`:

```toml
# ~/code/app/.git/wtpool.toml
slot_count = 4
heavy = ["cargo build", "cargo test", "cargo nextest", "make"]
```

No clone carries this file, so a repository you clone never brings patterns of its own. A
repository can only add patterns: `limit` or `enforce` in `wtpool.toml` is an unknown key, and the
error names `wtpool job limit` or `wtpool job enforce`. A bad pattern makes the whole `wtpool.toml`
invalid, as any other bad value does.

Each name in `build_processes` counts as a one-word pattern of the repository, and
`wtpool job heavy` lists it with the source `build_processes`. `heavy` replaces that key.

## Safety

- `fg`, `free` and `rm` refuse a worktree with uncommitted files or an operation in progress. `fg`
  and `rm` also refuse unreferenced commits. `fg` and `bg` check before any fetch or change.
- `free` and `fg` also refuse while an agent works or waits in a worktree they would detach or
  move: the slot `free` names, and for `fg` the foreground and the slot that holds the branch. An
  agent that went quiet while working counts, an idle or done one does not. The refusal names
  the worktree and each such agent there. `--force` skips this check.
- A failed git command means unknown, and unknown means refuse.
- `wtpool` reads `git status` without optional locks, so it never takes `index.lock` from an agent's
  git in the same worktree.
- Commands that change the pool hold `wtpool.lock` in the git common directory, so two agents never
  take the same slot.
- `wtpool` acts only on worktrees git lists for this repository, never on the foreground by mistake.
- No daemon, no listening socket, and no shell except for the `on_new_slot` command you set.
