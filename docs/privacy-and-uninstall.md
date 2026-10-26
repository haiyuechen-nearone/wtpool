# Privacy and uninstall

[README](../README.md) · [Troubleshooting](troubleshooting.md) · **Privacy and uninstall** ·
[Security](../SECURITY.md)

The examples use a repository in `~/code/app` with its slots `~/code/app-wt1` to `app-wt3`. The
git common directory, which `git rev-parse --git-common-dir` prints, is `~/code/app/.git`.

## What wtpool writes, and where

| What | Where | Who writes it |
|---|---|---|
| The binary | `~/.cargo/bin/wtpool`, or `$CARGO_HOME/bin/wtpool` | you, with `cargo install` |
| The hooks, as the plugin | Claude Code's plugin folders, the marketplace `wtpool`, and `"wtpool@wtpool": true` under `enabledPlugins` in `~/.claude/settings.json` | Claude Code, when you install the plugin |
| The hooks, by hand | the 14 entries you merged into the `hooks` of `~/.claude/settings.json`, or of `settings.json` in `$CLAUDE_CONFIG_DIR` | you. wtpool never writes that file |
| Activity records | `~/code/app/.git/wtpool-activity/`: `<session_id>.json` per session and `<session_id>.<agent_id>.json` per subagent, each with a `.lock` file | `wtpool hook`. `wtpool setup` and every command that changes the pool create the folder |
| The gc stamp | `~/code/app/.git/wtpool-activity/gc.stamp`: one line, the time the last gc of old records began | `wtpool hook` after a session ends, at most once a day, and `wtpool hook gc` |
| Settings | `~/code/app/.git/wtpool.toml` | you. wtpool only reads it |
| The pool lock | `~/code/app/.git/wtpool.lock`, an empty file | every command that changes the pool |
| Backup refs | `refs/wtpool-backup/<branch>/<sha>` in the repository | `wtpool bg` and `wtpool fg` with `--reset` |
| Slots | `~/code/app-wt1` to `app-wt3`, or in `slot_dir`, each a git worktree | `wtpool setup`, `wtpool bg`, `wtpool new` |
| Files in new slots | the `copy` files, and whatever `on_new_slot` writes | wtpool, when it creates a slot |
| Hook log | `~/.local/state/wtpool/hook.log` and `hook.log.1`, or `$XDG_STATE_HOME/wtpool/` | `wtpool hook`, only on a panic, a watchdog stop, or an event it skipped because it could not lock or write its record |
| The extension | VS Code's extensions folder | you, with `code --install-extension` |
| The `wtpool.path` setting | your VS Code user `settings.json` | you, if you set it |
| The tab's state | VS Code's own storage: the path of the selected row, to restore the tab after a reload | VS Code |

On Windows, without `HOME`, the hook log is under your user folder:
`%USERPROFILE%\.local\state\wtpool\`.

`wtpool status` writes no file. It prints the machine's name, the pool's paths, branches,
upstreams, agent labels and actions, and the paths of background tasks' output files. That stays on
your machine unless you share the output, for example in a bug report.

## What a record holds

One record per session or subagent. The hook keeps it within 16 KiB by dropping the oldest ids
of ended calls, then the oldest calls in flight. A record that still does not fit is not
written: the record on disk stays as it was, and the hook's log notes the skipped event. wtpool
ignores a record that is larger. It holds:

- the session and agent ids, the agent type, and the harness (`claude-code`);
- the label: the session's title, or `session HH:MM`. A subagent's label is its agent type;
- the worktree it works in, how it was placed there, and the folder it started in;
- its state, what it waits for (`permission`, `question`, `plan approval`, `input`), and the
  error text of a failed turn, cut to 80 characters;
- its last action and its calls in flight: the tool name and a target. The target is a file path
  in the worktree, or for Bash the program name, plus one subcommand word for tools such as `git`
  or `cargo`, as in `cargo test`;
- a SHA 256 hash of the input of each call in flight and of a waiting call, which ties a
  permission prompt to its call. A hash does not show the input, but it can confirm a guess of a
  short one;
- the pid and start time of the Claude Code process;
- the titles of the newest 16 subagents: the description of the Agent call that started each,
  which the panel shows as its label;
- how far the hook has read the transcript (a byte offset);
- the last 5 background tasks: Claude Code's task id, the path of the output file, the command's
  summary such as `cargo build`, and when the task started and ended;
- timestamps.

A record never holds prompt text, replies, command lines, command output, file contents or
environment variables. Of a Bash command it keeps only the program name and that one subcommand
word, never another argument or anything inside `$(…)` or backquotes.

Every stored string loses control, bidi and zero width characters and is cut to 80 characters,
labels to 40, with two exceptions:

- the worktree paths and the folder the session started in are stored as they are, with no
  cleaning and no length limit;
- an output file's path may have up to 1,024 bytes, and is not stored when it holds one of those
  characters.

The full list of fields is in the spec,
[status-panel.md](design/status-panel/status-panel.md#activity-records-any-harness).

Session titles can reflect prompt content. Claude Code generates a title from your conversation,
and the label keeps its first 40 characters.

The panel shows the last lines of a background task's output. The extension reads them from the
file Claude Code writes, on this machine, for the selected row only. Neither wtpool nor the
extension copies that output anywhere.

## What talks to the network

Only `git` and `gh` use the network on purpose, for PR and remote branch features:

- `wtpool bg` and `wtpool fg` run `git ls-remote` and `git fetch` on the configured remote
  (`origin` unless `remote` names another) for a branch that is not local, for a PR, and for
  `--refresh` and `--reset`.
- A PR number or URL also runs `gh pr view`, which asks github.com for the PR's head.
- The `on_new_slot` command you set may use the network. wtpool does not look at what it does.

`wtpool hook`, `wtpool status` and the extension open no network connection themselves. The
lints `lint__no_socket_apis` and `lint__no_network_modules` enforce that in the code.

The hook resolves the paths that the model writes in tool calls, before you approve the call. On
Windows it never reads such a path with a UNC, device or verbatim prefix other than a drive's,
or with a root but no drive. Elsewhere it never reads one that starts with exactly two slashes.
Two cases can still make the system contact another machine:

- where autofs serves `/net`, a path under `/net/<host>` makes the system contact that host;
- a local path that goes through a symlink whose target is a UNC path, such as `\\host\share`,
  makes Windows connect to that host over SMB when the hook resolves it.

[Security](../SECURITY.md#paths-can-reach-the-network) has the details.

When you install the plugin, Claude Code itself fetches this repository from GitHub.

## Uninstall

Do the steps in this order: the first one needs the `wtpool` binary, which the last one removes.

1. **Empty each pool.** In each repository, run `wtpool ls`. Commit or move anything you want to
   keep from the slots, then remove each slot:

   ```sh
   cd ~/code/app
   wtpool rm app-wt1
   wtpool rm app-wt2
   wtpool rm app-wt3
   ```

   `wtpool rm` refuses a slot with uncommitted files or commits that no ref holds. The branches
   stay in the repository.

2. **Delete wtpool's files in the git common directory:**

   ```sh
   cd ~/code/app
   rm -r "$(git rev-parse --git-common-dir)/wtpool-activity"
   rm -f "$(git rev-parse --git-common-dir)/wtpool.lock" "$(git rev-parse --git-common-dir)/wtpool.toml"
   ```

3. **Delete the backup refs.** They hold the old tips of branches that `--reset` moved. List them
   first, and keep any commit you still need on a branch:

   ```sh
   git for-each-ref refs/wtpool-backup/
   git for-each-ref --format='delete %(refname)' refs/wtpool-backup/ | git update-ref --stdin
   ```

4. **Remove the hooks.** For the plugin, run `/plugin uninstall wtpool@wtpool` and
   `/plugin marketplace remove wtpool` in Claude Code, or `claude plugin uninstall wtpool@wtpool`
   and `claude plugin marketplace remove wtpool` in a shell. For the block, delete from
   `~/.claude/settings.json` each hook entry whose `command` is your `wtpool` and whose `args`
   are `["hook"]`.

5. **Remove a daily job, if you made one.** wtpool needs none: the hook runs gc itself. An
   earlier wtpool asked for a cron or launchd job that runs `wtpool hook gc`. Delete its line
   with `crontab -e`, or for launchd run `launchctl bootout gui/$(id -u)/<label>` and delete its
   plist in `~/Library/LaunchAgents/`.

6. **Remove the extension.** In the Extensions view, find wtpool and choose **Uninstall**. Or run
   `code --list-extensions | grep wtpool` and pass the id it prints to
   `code --uninstall-extension`. Delete `"wtpool.path"` from your user `settings.json` if you set
   it.

7. **Delete the hook log:** `rm -r ~/.local/state/wtpool`, or `$XDG_STATE_HOME/wtpool`.

8. **Remove the binary:** `cargo uninstall wtpool`.

## For contributors

The verify harness keeps two folders outside the clone:

- `~/.cache/wtpool-verify/` (or `$XDG_CACHE_HOME/wtpool-verify/`): clean clones, the short links
  for VS Code's sockets, a downloaded VS Code, and the lock that keeps the budgets alone;
- `~/.local/share/wtpool-verify/` (or `$XDG_DATA_HOME/wtpool-verify/`): the build copy of
  `wtpool` that the contract sessions run, and recordings of live contract runs in
  `recordings/` and `recordings-backup/`.

Delete both with `rm -r ~/.cache/wtpool-verify ~/.local/share/wtpool-verify`. In the clone,
`verify/out/` holds each run's sandboxes and reports, and `target/` the builds. Playwright keeps
its Chromium in its own cache, `~/Library/Caches/ms-playwright` on macOS or
`~/.cache/ms-playwright` on Linux, which other projects may share.
