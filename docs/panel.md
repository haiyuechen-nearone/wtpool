# The VS Code panel

[README](../README.md) · [Commands](commands.md) · [Configuration](configuration.md) ·
[Status and hook](status-and-hook.md) · **Panel** · [Development](development.md)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="images/panel-waiting-dark.png">
  <img alt="The Worktrees tab: an agent in wt2 waits for permission to run git status, two slots are free" src="images/panel-waiting-light.png">
</picture>

Run **Worktrees: Open Status** from the Command Palette. A tab opens beside the active editor
group, or comes to the front. It runs `wtpool status --watch` while open; closing the tab stops it.
Each window has its own tab. The tab title reads `Worktrees (N)` while N worktrees wait.

To install it, see the [README](../README.md#the-extension). It needs the `wtpool` CLI. Agents
show only once the hooks are on: the plugin or the settings block, as the
[README](../README.md#the-hooks) says. Until then the tab names the two steps: `wtpool setup`, then
the hooks, through the plugin or the block `wtpool hook install` prints.

## Which pool it shows

The tab runs `wtpool` in the window's first workspace folder, both for the watch and for the row
actions, so it shows the pool of the repository that folder belongs to. It sets no `WTPOOL_FOREGROUND`
of its own; one in the environment VS Code started with still applies
([Configuration](configuration.md)). With no folder open, the tab says "Open a folder inside a git
repository to see its worktrees."

When `wtpool` exits with an error, for example because the folder is not inside a git repository,
the header shows its error message (the last non-empty line of its stderr) next to the paused
state, and retries. The message wraps; it is never cut off.

The extension runs on your local machine, and so does the `wtpool` it starts. In a Remote-SSH, WSL or
dev container window it shows the local pool, not the worktrees on the remote side.

## Header

Chips count `free`, `working` and `waiting` worktrees, and hide at 0. Click the waiting chip to
focus the first waiting row. The connection reads `live`, `updated N s ago` after 15 s without an
update, or `updates paused, retrying in N s` while the extension restarts `wtpool`.

## Rows

A heading row names the repository and the machine: `acme/app · my-mac`, with the GitHub
`owner/repo` when the remote is on github.com, else the main worktree's folder name. When the
document holds several pools, each gets its own heading.

Under it, one row per worktree: foreground, then slots, then unmanaged, by name. Rows never move.

| Column | Shows |
|---|---|
| Activity | icon and word, see below |
| Worktree | name, plus ` fg` or ` unmanaged` |
| Branch | branch, or `(detached)` |
| Git | `clean`, else `N dirty`, `↑N`, `↓N`, `N unreferenced`, the operation, `unknown` |
| Agent · doing | one line per agent, newest first: `<label> · <what it does>` |
| Last | the age of each line |

The agent line reads `<tool> <target>` while working, `quiet · …` after 2 minutes without an event,
`<reason>: <tool> <target>` while waiting, `idle` or `API error: …`, and `done` for a finished
subagent, shown for 1 minute. A row without agents reads `build: <process>` or `no agent`.

An agent whose turn ended still shows working while work it started runs:

- a session whose build still runs, such as a `cargo build` it started in the background, reads
  `build: <name>`, the outermost build, as in `build: cargo-nextest`;
- a session or subagent with a background task still running reads `background: <summary>`, for
  the newest running task, as in `background: cargo test`. A subagent that ends its turn to wait
  for its own background run shows working, not `done`.

Once the build ends, or the hook records the task's end, the agent shows its own state again.

Select a row to see the exact path, upstream, git facts, why `wtpool bg` refuses it, and per agent its
state, action, how it was placed (`cwd` or `path`) and since when (local time).

## Live output

When an agent runs a command in the background, such as a long `cargo build`, the selected row's
strip shows an Output section under that agent. For each task still running, and for the one that
ended last, it shows the command's summary (`cargo build`) with `running` or `ended HH:MM`, then
the last 15 lines of its output in a monospace block. New lines show within a second. The block
stays at its end as lines arrive; scroll it up to read, and it stays where you put it. Long lines
wrap. A tab shows as spaces, and any other control character as `\u{XXXX}`. A command the agent
runs in the foreground shows only in its agent line, as before.

**Follow** opens a VS Code terminal named after the agent and the task, such as
`session 20:31 · cargo build`. It shows the last 40 lines of the output, then each new line, with
its colours and with tabs as spaces. Closing the terminal stops it.

The output never passes through `wtpool`. The hook records only the path of the file Claude Code
writes the output to ([Status and hook](status-and-hook.md#background-tasks)), and the extension
reads that file on this machine, for the selected row only and only while the tab is visible. It
reads only paths the status document lists, and never a symlink or anything but a regular file.

## Activities

| Activity | Meaning |
|---|---|
| waiting | An agent waits for permission, an answer, a plan approval or MCP input. Highlighted. |
| working | An agent acted here within 2 minutes, its shell call builds here, a subagent it started works or waits, or a build or background task it started still runs. A robot swings its arms. |
| building | A build runs here without a working agent. A dial spins. |
| idle | A live session finished, went quiet, or hit an API error. |
| foreground | Your editor worktree, with no agent or build. |
| free | A detached, clean slot `wtpool bg` may reuse. |
| not reusable | A detached slot `wtpool bg` refuses. Select the row to see why. |
| unused | A slot that holds a branch, or an unmanaged worktree, with nothing running. |
| unknown | Git could not tell. Not safe. |
| missing | Its folder is gone. |

With VS Code's reduce motion on (`workbench.reduceMotion`, which follows the system by default),
the robot and the dial stand still.

## Row actions

Row actions appear on hover and keyboard focus:

- **Open in new window**, **Open terminal**, **Copy path**. A missing worktree offers only the copy.
- **Free this worktree…** runs `wtpool free <slot> --force`. **Bring its branch to the
  foreground…** runs `wtpool fg <branch> --force`. Both appear only on a slot that holds a branch.
  They pass `--force` because the tab judges the agents there itself, as the next paragraph says.

Free and fg always ask first, in a dialog that names the worktree and the branch. They refuse while
an agent in that slot works or waits, and warn when one is idle. They check the slot again after
you confirm, and run nothing if it changed. Errors from `wtpool` show as a notification. Nothing else
in the tab changes the pool.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `wtpool.path` | `wtpool` | The `wtpool` binary the tab runs. Only user settings can set it. It runs with an argument array and no shell. |
