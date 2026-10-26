# Troubleshooting

[README](../README.md) · [Commands](commands.md) · [Status and hook](status-and-hook.md) ·
[Panel](panel.md) · **Troubleshooting** · [Privacy and uninstall](privacy-and-uninstall.md)

## Agents show "off"

The tab says how to turn agents on, and `wtpool status --json` has `"agents":"off"`. The
repository has no `wtpool-activity/` folder in its git common directory, and the hook never
creates one. Run this in the repository:

```sh
wtpool setup
```

`wtpool setup` creates the folder, and so does every command that changes the pool. Then
`wtpool status --json` has `"agents":"on"`. If agents still do not show, see
[Hooks do not fire](#hooks-do-not-fire).

## `spawn wtpool ENOENT` in VS Code

The tab reads `wtpool did not start: spawn wtpool ENOENT`. VS Code cannot find `wtpool` on its
`PATH`. VS Code started from the Dock or a desktop menu may not get your shell's `PATH`.

Find the binary with `command -v wtpool` (`where wtpool` on Windows). Then set `wtpool.path` to
that full path in your user `settings.json`:

```json
"wtpool.path": "/Users/you/.cargo/bin/wtpool"
```

Only user settings can set `wtpool.path`, not a workspace. The tab reads it each time it starts
`wtpool`, so its next retry uses the new value.

## `gh` is not logged in

PR numbers and URLs need the [`gh`](https://cli.github.com) CLI, logged in. When `gh` is missing,
not logged in or fails for another reason, `wtpool` names the problem and shows the last line of
`gh`'s stderr, for example:

```
gh is not logged in: run `gh auth login`
```

Run `gh auth login`, then check with `gh auth status`. A PR that does not exist reads
`no PR #1234 in acme/app`. The remote must be on github.com: GitHub Enterprise is not supported.
In a fork, where `origin` is your copy, set `remote = "upstream"` in `wtpool.toml`
([Configuration](configuration.md#remote)).

## `wtpool bg` says there is no such branch

`wtpool bg` parks a branch that exists locally or on the remote, and never creates one. Without a
remote it says `no branch 'feature-y' here, and no remote to look it up`. Create the branch
first:

```sh
git branch feature-y && wtpool bg feature-y
```

## Hooks do not fire

Agents are `"on"`, but no agent shows, and no record appears in the activity folder after you
send Claude Code a prompt:

```sh
ls "$(git rev-parse --git-common-dir)/wtpool-activity"
```

Check these in turn:

1. **Are the hooks there?** Run `wtpool hook install > /dev/null`. Its stderr says whether the
   hooks are present, through the plugin, the settings block or both. When they are not, it says
   what is missing: which events, a stale binary path, or a `settings.json` that is not valid
   JSON.
2. **The plugin.** `claude plugin list` shows `wtpool@wtpool` and whether it is enabled. Install
   it at user scope: `wtpool hook install` looks only at your user settings. In a session,
   `/hooks` lists the hooks Claude Code loaded.
3. **`PATH`.** The plugin runs `wtpool hook` by name. Claude Code started from an IDE or a desktop
   app may not get your shell's `PATH`, and then finds no `wtpool`. Put `~/.cargo/bin` on the
   `PATH` that environment gets, or use the settings block, which names an absolute path.
4. **A stale path.** The settings block runs the absolute path of the `wtpool` that printed it.
   After you move or reinstall `wtpool` elsewhere, print the block again and replace the old
   entries.
5. **Claude Code's version.** `claude --version` must be 2.1.295 or newer.
6. **The session's folder.** A session records into the repository it works in. A session in a
   folder outside this pool does not show in this pool.

Two hook routes at once make every event run twice. `wtpool hook install` warns about that and
says how to remove the settings block.

## What `hook.log` holds

The hook writes its diagnostics to `$XDG_STATE_HOME/wtpool/hook.log`, by default
`~/.local/state/wtpool/hook.log`. On Windows, without `HOME`, that is
`%USERPROFILE%\.local\state\wtpool\hook.log`. On Unix the file is created with mode 0600. It
grows to 1 MiB, then rotates once to `hook.log.1`. The hook never writes it inside a git directory.

It holds three kinds of line only:

- `wtpool hook: panicked at <file>:<line>`, when the hook panics;
- `wtpool hook: the watchdog ended a run after 1 s`, when a run took too long;
- `wtpool hook: skipped an event: <reason>`, when the hook could not take a record's lock or
  write the record, for example one that does not fit in 16 KiB.

The hook logs nothing for input it ignores on purpose, such as malformed JSON, an event outside
any pool, or a pool without `wtpool-activity/`. So an empty or missing log is normal. No line
holds payload text.

## Old records and `wtpool hook gc`

No cron line or launchd job is needed. The hook deletes old records itself: when a session ends,
at most once a day per repository, it deletes `ended` or dead records after 7 days, and `done`
subagents after 15 minutes, keeping the newest per worktree. It spends at most 200 ms on this. A
repository where no session ends keeps its records until one does.

To clean up now, run the same gc by hand in the repository, or with `WTPOOL_FOREGROUND` set:

```console
$ cd ~/code/app
$ wtpool hook gc
wtpool hook gc: kept 3, deleted 12 records
```

`wtpool-activity/gc.stamp` holds the time the last gc began. Delete it, and the next session that
ends runs gc again.

If you scheduled `wtpool hook gc` with cron or launchd for an earlier wtpool, you can remove that
job, as [Uninstall](privacy-and-uninstall.md#uninstall) says.

## "no free slot"

`wtpool bg` fails with one line per slot, saying why it is busy, then the error:

```
wtpool: app-wt1 holds branch feature-x
wtpool: app-wt2 has 2 uncommitted file(s)
wtpool: no free slot and the pool is at its 3 slot limit; release one with 'wtpool free <slot>' (see 'wtpool ls')
```

Every slot holds a branch, or is detached but not safe to reuse, for example because of
uncommitted files or commits that no ref holds.
`wtpool ls` shows what each slot holds. Then do one of these:

- free a slot whose branch you no longer need there: `wtpool free <slot|branch>`. The branch
  stays, and the command refuses a slot with uncommitted files;
- add a slot above the limit with `wtpool new`;
- raise `slot_count` in `.git/wtpool.toml`, or set `WTPOOL_SLOT_COUNT`.

## The old name, `wt`

wtpool was called `wt` while it was built. It was renamed before it was published, because
[worktrunk](https://github.com/max-sixty/worktrunk) installs a `wt` binary, `wt` starts Windows
Terminal on Windows, and the crates.io names `wt` and `wt-core` were taken.

| Before | Now |
|---|---|
| the command `wt` | `wtpool` |
| `.git/wt.toml`, `.git/wt-activity/`, `.git/wt.lock` | `.git/wtpool.toml`, `.git/wtpool-activity/`, `.git/wtpool.lock` |
| `refs/wt-backup/` | `refs/wtpool-backup/` |
| `WT_FOREGROUND` and every other `WT_*` variable | `WTPOOL_FOREGROUND`, `WTPOOL_*` |
| `~/.local/state/wt/` | `~/.local/state/wtpool/` |
| the VS Code setting `wt.path` | `wtpool.path` |

wtpool reads none of the old names. The review records in `verify/reviews/` and the older entries
of `verify/oracle-changes.toml` still say `wt`.
