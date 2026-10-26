# Commands

[README](../README.md) · **Commands** · [Configuration](configuration.md) ·
[Status and hook](status-and-hook.md) · [Panel](panel.md) · [Development](development.md)

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
wtpool help                            print the help text
wtpool --version                       print the version, commit and cargo features
```

`wtpool` works on the pool of the repository you run it in, or the one `WTPOOL_FOREGROUND` names; outside
a repository it refuses. `wtpool status` and `wtpool hook` are described in
[Status and hook](status-and-hook.md), and the settings and `wtpool config` in
[Configuration](configuration.md).

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
