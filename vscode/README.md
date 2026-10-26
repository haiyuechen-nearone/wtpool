# Worktrees

![The Worktrees tab in VS Code: one row per worktree of the pool, with its branch, git state and what each agent there does](https://raw.githubusercontent.com/haiyuechen-nearone/wtpool/main/docs/images/demo.gif)

A VS Code editor tab that shows the worktrees of a [wtpool](https://github.com/haiyuechen-nearone/wtpool)
pool. Per worktree it shows the branch, the git state, what each Claude Code agent there does, and
whether the worktree needs you or is free.

Screenshots and the meaning of each row, activity and action are in
[docs/panel.md](https://github.com/haiyuechen-nearone/wtpool/blob/main/docs/panel.md).

## Requirements

- The `wtpool` CLI, on the `PATH` VS Code sees, or named by the `wtpool.path` setting. The
  [wtpool README](https://github.com/haiyuechen-nearone/wtpool#readme) says how to install it.
- VS Code 1.140 or newer.
- A git repository open in the window. A github.com remote (`origin` unless wtpool's `remote` setting
  names another) gives the `owner/repo` heading, and wtpool needs it and `gh` for PR numbers and URLs.
- For agent activity: Claude Code 2.1.295 or newer, its hooks (see [Agent activity](#agent-activity)),
  and `wtpool setup` run once in the pool. Without them the tab shows git state and builds only, and
  says how to turn agents on.

Cursor and other editors built on VS Code may work if their VS Code version meets the `engines`
range, 1.140 or newer. They are not tested.

## Install

There is no Marketplace release yet. Build the `.vsix` from a clone of the repository, in its
`vscode/` folder:

```sh
npm ci && npm run package
```

Then install `wtpool.vsix`, either with VS Code's command line:

```sh
code --install-extension wtpool.vsix
```

or from the Command Palette: run **Extensions: Install from VSIX…** and pick `wtpool.vsix`.

On macOS, `code` is on your `PATH` only after you run **Shell Command: Install 'code' command in
PATH** from VS Code's Command Palette.

### `spawn wtpool ENOENT`

If the tab reads `wtpool did not start: spawn wtpool ENOENT`, VS Code cannot find `wtpool`. VS Code
started from the Dock or a desktop menu may not get your shell's `PATH`. Set `wtpool.path` in your
user settings to the full path, such as the output of `command -v wtpool`.

## Agent activity

The tab shows what each Claude Code agent does from the records of wtpool's hooks. Run
`wtpool setup` once in the pool, then add the hooks to Claude Code in one of two ways.

The plugin: run these two lines in Claude Code.

```text
/plugin marketplace add haiyuechen-nearone/wtpool
/plugin install wtpool@wtpool
```

Or by hand: merge the block that `wtpool hook install` prints into the `hooks` of
`~/.claude/settings.json` (or `$CLAUDE_CONFIG_DIR/settings.json`), next to the hooks already there,
never over them. Use one way, not both, or every event runs twice.

Each hook runs `wtpool hook`, so `wtpool` must be on the `PATH` that Claude Code's hooks see. To
check, run `wtpool hook install`: it says whether the hooks are present, and what is missing when
they are not.

## Use

Run **Worktrees: Open Status** from the Command Palette. The tab opens beside the active editor
group, or comes to the front when it is already open. It runs `wtpool status --watch` while open, and
closing it stops `wtpool`. Each window runs its own tab. The title reads `Worktrees (N)` while N
worktrees wait.

`wtpool` runs in the window's first workspace folder, so the tab shows the pool of the repository you
have open, for example `~/code/app` with its slots `~/code/app-wt1` to `app-wt3`. A
`WTPOOL_FOREGROUND` in the environment VS Code starts with still names the pool; the extension sets
none of its own. With no folder open, the tab says "Open a folder inside a git repository to see
its worktrees." and runs nothing. When the first folder changes, the tab starts over in the new
one.

The tab shows one repository per window, the one of its first workspace folder. A heading row
names it above the worktrees, `acme/app · my-mac`: the GitHub `owner/repo`, or the name of the
main worktree's folder when there is no github.com remote, then the machine.

When `wtpool` fails, the header reads "updates paused, retrying in N s" and shows wtpool's error under it,
such as `wtpool: not inside a git repository: run wtpool in your repository or set WTPOOL_FOREGROUND`. If it
reads `wtpool did not start: spawn wtpool ENOENT`, VS Code cannot find `wtpool`: see
[`spawn wtpool ENOENT`](#spawn-wtpool-enoent).

Click a row, or press Enter on it, to open its detail strip: the exact path, upstream, git facts,
why `wtpool bg` would not take it, and each agent's state, action and start time. It follows every
update while it is open. When an agent runs a background command, such as a Claude Code Bash call
with `run_in_background`, the strip's Output section shows the last 15 lines of that command's
output as it grows, for each running command and the one that ended last. **Follow** opens a VS
Code terminal that streams the whole output from its last 40 lines on. The extension reads that
output from the file Claude Code writes, on this machine; `wtpool` itself keeps only the file's path,
never the output.

Row actions open the worktree in a new window, open a terminal there, or copy its path. On a slot
that holds a branch, they also free the slot (`wtpool free`) or bring its branch to the foreground
(`wtpool fg`). Both ask first, and refuse while an agent in the slot works or waits. Nothing else
changes the pool, and the extension opens no network connection.

## Remote windows

The extension runs on your local machine (`extensionKind: ["ui"]`), and so does the `wtpool` it
starts. A Remote-SSH, WSL or dev container window therefore shows a pool on the local machine,
never one on the remote side. Its folder is not on the local machine, so `wtpool` runs in your home
folder there: set `WTPOOL_FOREGROUND` in the environment VS Code starts with to name the local pool,
or the tab shows wtpool's error.

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `wtpool.path` | `wtpool` | The `wtpool` binary the tab runs. Application scope, so only your user settings can set it. It runs with an argument array and no shell. |

## Build

```sh
npm ci
npm run generate   # src/generated/ from the JSON Schema of wtpool-core
npm run build      # out/extension.js, out/html.js, media/webview.js, media/webview.css
npm run package    # build, then wtpool.vsix
```

`npm run generate` runs `cargo run -p wtpool-core --example status_schema` and writes the schema, the
TypeScript types and an ajv standalone validator into `src/generated/`. The files are committed,
and a second run changes nothing.

The Marketplace icon, `media/marketplace-icon.png`, is rendered from `media/marketplace-icon.svg`
with the Chromium of Playwright that `test/` installs. After an edit of the SVG, run
`node scripts/icon.mjs` and commit both files.
