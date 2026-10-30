# Configuration

[README](../README.md) · [Commands](commands.md) · **Configuration** ·
[Status and hook](status-and-hook.md) · [Panel](panel.md) · [Heavy jobs](heavy-jobs.md) ·
[Development](development.md)

## Which repository

`wtpool` works on the pool of one git repository:

1. the repository `WTPOOL_FOREGROUND` names, when that variable is set;
2. else the repository that holds the current directory. From the main worktree, a slot or any
   folder inside them, that is the same repository, so every worktree of the pool finds the same
   pool. No `wtpool.toml` is needed;
3. else `wtpool` refuses:
   `not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND`.

A bare repository found from the current directory inside a worktree of another repository, such
as one a clone carries in its files, is refused before its `wtpool.toml` is read, as git refuses it
with `safe.bareRepository=explicit`: `could not tell which git repository … belongs to`. Set
`WTPOOL_FOREGROUND` to a worktree of it to use it all the same.

`wtpool help`, `wtpool --version`, `wtpool hook`, `wtpool hook install` and `wtpool status --schema` need no
repository. Every other command needs one, `wtpool hook gc` included: run it inside the repository
or set `WTPOOL_FOREGROUND`.

## Settings

| Key | Variable | Default | `wtpool config` source of the default |
|---|---|---|---|
| `foreground` | `WTPOOL_FOREGROUND` | the main worktree | `main worktree` |
| `slot_dir` | `WTPOOL_SLOT_DIR` | the folder that holds the main worktree | `parent of the main worktree` |
| `slot_prefix` | `WTPOOL_SLOT_PREFIX` | the main worktree's folder name, then `-wt` | `main worktree name + -wt` |
| `slot_count` | `WTPOOL_SLOT_COUNT` | `3` | `default` |
| `remote` | `WTPOOL_REMOTE` | `origin` | `default` |
| `heavy` | none | `[]` | not listed |
| `build_processes` | none | `[]` | not listed |
| `copy` | none | `[]` | not listed |
| `on_new_slot` | none | none | not listed |

The main worktree is the one `git clone` or `git init` made, the first one `git worktree list`
shows. For a repository in `~/code/app`, the defaults give the foreground `~/code/app` and the
slots `~/code/app-wt1` to `~/code/app-wt3`. Any other worktree, such as `~/code/app-review`, is
listed as unmanaged and never picked.

A bare repository has no main worktree, so nothing can be derived from it. Set `foreground`,
`slot_dir` and `slot_prefix`, in `wtpool.toml` or as variables, or `wtpool` refuses, for example:
`/Users/you/code/app.git is a bare repository, so it has no main worktree to derive foreground
from; set WTPOOL_FOREGROUND, or foreground in /Users/you/code/app.git/wtpool.toml`.

## `wtpool.toml`

Settings live in `wtpool.toml` in the git common directory (`git rev-parse --git-common-dir`), so the
foreground and every slot read the same file. Git never reads it and no branch carries it.

```toml
# ~/code/app/.git/wtpool.toml
slot_count = 4
remote = "upstream"
heavy = ["cargo test", "make"]
copy = [".envrc", ".env"]
on_new_slot = "direnv allow"
```

## Which value wins

Each setting takes the first value it finds: the variable, `wtpool.toml`, the default.

- The foreground must be a worktree of the repository.
- Paths in `wtpool.toml` may start with `~/`. Relative paths start from the main worktree's parent
  folder. Without `HOME`, as on Windows, `~/` is the system's home folder. An empty or relative
  `HOME` counts as unset.
- Unknown keys, wrong types, empty values, a slot count of 0, a remote name git would refuse, a
  symlinked or unreadable `wtpool.toml`, and a variable that is set but empty or invalid are errors.
  `wtpool` never falls back to a default.
- `heavy` lists this repository's heavy commands, as patterns such as `cargo test`: `wtpool status`
  counts processes that match them as builds, and the heavy job policy judges commands by them
  ([Commands](commands.md#a-repositorys-heavy-commands)). `limit` and `enforce` are not keys of
  `wtpool.toml`: they live in this machine's `machine.toml`
  ([Commands](commands.md#the-machine-config)).
- `build_processes`, the older key, still counts each of its names as a one-word heavy pattern,
  and `wtpool config` reports it as replaced by `heavy`.
- `heavy`, `build_processes`, `copy` and `on_new_slot` have no variable, and `wtpool config` does
  not list them.

## New slots: `copy` and `on_new_slot`

A new worktree holds only what git tracks. `copy` and `on_new_slot` set up the rest in each slot
`wtpool` creates (`wtpool bg`, `wtpool new`, `wtpool setup`). Both are optional, live in `wtpool.toml` only and have no
variable.

`copy` lists files to copy from the main worktree into the slot, as paths relative to the main
worktree. An absolute path or one with `..` is an error. `wtpool` checks each path in this order:

1. A path the slot already has is skipped without a word, whatever it is. It is never
   overwritten. A slot already holds the files its commit tracks, so a tracked file is usually
   skipped here, before the warning of step 4.
2. A file the main worktree does not have is skipped without a word.
3. No symlink is followed: a file that is a symlink, a folder on the way that is one, in the main
   worktree or in the slot, and anything other than a regular file is skipped with a warning.
4. A file git does not ignore in the main worktree is skipped with a warning on stderr. So a copy
   never shows as a change, and the slot stays free.
5. Otherwise the file is copied, with its permissions.

`wtpool setup` also copies, into every existing slot, the files it lacks.

`on_new_slot` is one command line, run once in each slot `wtpool` creates, after the copies, with the
slot as its working directory. It runs through the platform shell: `sh -c` on Unix, `cmd /C` on
Windows. Its output, standard output included, goes to stderr, so `wtpool bg` still prints only the
slot's path on stdout. `wtpool setup` does not run it in slots that already exist.

When a copy or `on_new_slot` fails, `wtpool` reports it and exits non-zero, but keeps the slot: `wtpool bg`
still parks the branch there, `wtpool new` and `wtpool setup` still create every slot. Rerunning `wtpool bg`
then prints the slot that holds the branch.

With [direnv](https://direnv.net), each slot needs its own `.envrc`, approved once. Git must ignore
`.envrc` in the main worktree, for example through `.gitignore` or `.git/info/exclude`:

```toml
copy = [".envrc"]
on_new_slot = "direnv allow"
```

For an app that reads its secrets from `.env` files that git ignores, and installs its
dependencies per worktree:

```toml
copy = [".env", "config/master.key"]
on_new_slot = "npm ci"
```

## `remote`

`remote` names the git remote that `wtpool` looks pull requests up in, fetches branches from and has
local branches track. The variable is `WTPOOL_REMOTE`, the default `origin`. In a fork, where `origin`
is your copy and `upstream` the repository that holds the pull requests, set
`remote = "upstream"`.

- A branch ref that is not local is looked up on the remote. When the repository has no such
  remote, `wtpool` fails with `no branch 'feature' here, and no remote to look it up`, and
  `--refresh` warns and leaves the local branch as it is.
- A PR number or a PR URL needs [`gh`](https://cli.github.com) and a remote on github.com, in the
  form `https://github.com/acme/app`, `git@github.com:acme/app` or
  `ssh://git@github.com/acme/app`, with or without `.git`. GitHub Enterprise and other hosts are
  not supported: `wtpool` says the remote is not on github.com and changes nothing.

## `wtpool config`

`wtpool config` prints each setting with its source:

- `env WTPOOL_FOREGROUND` and the like;
- the path of `wtpool.toml`;
- `main worktree`, `parent of the main worktree` or `main worktree name + -wt`, for the defaults
  that come from the main worktree;
- `default`.

`wtpool config --json` prints the same for scripts. In `~/code/app` without a `wtpool.toml`, or in any of
its slots:

```console
$ wtpool config
SETTING      VALUE                SOURCE
foreground   /Users/you/code/app  main worktree
slot_dir     /Users/you/code      parent of the main worktree
slot_prefix  app-wt               main worktree name + -wt
slot_count   3                    default
remote       origin               default
```

## Variables of the agent harnesses

These are no settings: `wtpool config` does not list them, and `wtpool.toml` has no key for them.

| Variable | Used by | Meaning |
|---|---|---|
| `CLAUDE_CONFIG_DIR` | `wtpool hook install` | The folder of Claude Code's `settings.json`, by default `~/.claude`. |
| `OPENCODE_CONFIG_DIR` | OpenCode, and `wtpool hook install --harness opencode` | The folder OpenCode loads plugins from, as `plugins/<name>/index.ts`. Else `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`. The install note checks `plugins/wtpool/index.ts` there. |
| `WTPOOL_SESSION` | set by the OpenCode plugin, read by `wtpool fg` and `wtpool free` | The record stem of the OpenCode session whose shell call runs the command, such as `opencode-ses-e76df7aa0ffe3vdlGbn0I4xCYq`. It tells the command which session called it when one server runs many ([Commands](commands.md#safety)). Do not set it yourself: the plugin sets it for each shell call, and removes a value it cannot bind to one session. |

`OPENCODE_CONFIG_DIR` is OpenCode's own variable. Set it in the environment of the OpenCode server,
the same one that loads the plugin, and in the shell where you run `wtpool hook install`.
