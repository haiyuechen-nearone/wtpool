# Changelog

All notable changes to this project are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - Unreleased

The first version. It is alpha: tested on macOS; Linux and Windows build but are untested.

### Added

- **The pool.** `wtpool` keeps one foreground worktree and a pool of background slots for any git
  repository, with no configuration. `ls`, `fg`, `bg`, `free`, `path`, `new`, `rm`, `setup` and
  `config` move branches between them. A ref is a branch, a PR number or a GitHub PR URL.
  `--refresh` fast forwards a local branch, and `--reset` moves a rebased one after saving its old
  tip under `refs/wtpool-backup/`.
- **Safety rules.** wtpool refuses to repoint, free or remove a worktree with uncommitted files,
  an operation in progress or commits that no ref holds, and refuses when git cannot tell. A lock
  keeps two agents from taking the same slot. `free` and `fg` also refuse while an agent works or
  waits in a worktree they would detach or repoint, unless `--force`.
- **Settings.** `wtpool.toml` in the git common directory and `WTPOOL_*` variables set the
  foreground, the slot folder, prefix and count, the remote and extra build processes. `copy` and
  `on_new_slot` set up each new slot.
- **Output for scripts.** `wtpool bg` and `wtpool fg` print only the worktree's path on stdout. A
  failed `gh` call names the cause, such as `gh` not being logged in.
- **Status.** `wtpool status --json` prints the pool as one JSON document, `--watch` prints it
  again on each change, and `--schema` prints its JSON Schema. The document is a versioned
  contract that other tools can read.
- **The Claude Code hook.** `wtpool hook` records what each session and subagent does, in
  `wtpool-activity/`. Labels follow the session's title. Background tasks are recorded by the
  path of their output file, never their output. Old records go by themselves: after a session
  ends, the hook runs gc at most once a day per repository, so no cron or launchd job is needed.
  `wtpool hook gc` runs it at once.
- **Hook install.** A Claude Code plugin, `wtpool@wtpool`, installs the hooks. `wtpool hook install`
  prints the same hooks as a settings block to merge by hand, and reports which of the two is
  present.
- **Activity rules.** An agent whose turn ended shows working while a build or a background task
  it started runs. A `done` subagent shows for 1 minute.
- **Heavy jobs.** `wtpool job -- <command>` runs a heavy command once fewer jobs than this
  machine's limit run, in the order they came, with its arguments, streams and exit status passed
  through; a job inside a job runs at once. `wtpool job ls` prints the limit, the policy, the heavy
  patterns and the jobs. The limit, the policy and the machine's patterns live in
  `<config>/wtpool/machine.toml`, a repository adds patterns through `heavy` in its `wtpool.toml`,
  and a job records its summary, never its arguments.
- **The VS Code extension.** The Worktrees tab shows one row per worktree, with its branch, git
  state and what each agent does, and highlights waits. Its detail strip shows the live tail of
  each background task's output, and Follow streams it into a terminal. Row actions open the
  worktree in a new window or a terminal, copy its path, free a slot or bring its branch to the
  foreground. Free and fg ask first.
- **The verify harness.** `cargo xtask verify` runs the checks of the spec in five parts, from
  lints to end to end checks in a real VS Code and a live Claude Code contract.
