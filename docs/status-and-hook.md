# `wtpool status` and the Claude Code hook

[README](../README.md) · [Commands](commands.md) · [Configuration](configuration.md) ·
**Status and hook** · [Panel](panel.md) · [Development](development.md)

## `wtpool status`

`wtpool status --json` prints one line: a JSON document with every worktree of the pool. The pool names
its repository: `repository.name` is the folder name of the main worktree, and `repository.github`
is `owner/repo`, such as `acme/app`, when the configured remote is on github.com, else `null`. Per
worktree it gives the role (`foreground`, `slot`, `unmanaged`), branch, upstream, the git facts of
`wtpool ls`, whether `wtpool bg` would take it (`free`, `reasons`), its activity, the agents there, the
build processes working in it, and the times of its last event and last change. Each agent names
the tool it runs in as `harness`, today always `claude-code`. An agent with background tasks lists
them in `outputs`: the running ones, newest first, then the one that ended last, each with its task
id, the path of its output file, the command's summary, whether it still runs, and when it started
and ended. `outputs` is left out when there are none.

`wtpool status --watch` prints a document at once, then one per change, and
`{"type":"alive","seq":N}` every 10 s. It watches the git common directory and rereads only what
changed: a worktree's `HEAD`, index or operation files, the refs, or one activity record. Agent
events and git changes show within 1 s. Anything else, such as an edited file, shows within one 10 s
heartbeat. It exits 0 when stdin closes or stdout fails.

Builds are processes named `cargo`, `cargo-*`, `rustc` or a name in `build_processes`, working in a
worktree. To count other build tools, add their process names to `build_processes`
([Configuration](configuration.md)).

A build a session started keeps that session `working`, also after its turn ended, as when it runs
the build in the background: any build process that descends from the session's Claude Code
process, wherever it works. The agent line then reads `build: <name>`, the outermost such build,
such as `build: cargo-nextest`. When the build ends, the session shows its own state again, within
one 10 s heartbeat. Other children of Claude Code, such as MCP servers, never count, and neither
does a build you start yourself.

`wtpool status` writes no file and opens no socket. Its git runs with `--no-optional-locks` and
`-c core.fsmonitor=false`.

## The contract

The document is the contract between `wtpool` and whatever shows it. The VS Code extension is one
consumer; another IDE, a terminal tool or an agent tool can read the same lines.

`wtpool status --schema` prints the JSON Schema of the document, and every property in it has a
one-line description. The rules:

- Within document `version` 1, changes only add properties. A removal or a rename raises
  `version`.
- Consumers ignore properties they do not know. The schema's objects allow them.
- `wtpool` itself emits only the properties the schema lists.

`repository.github` is read offline from the URL of the configured remote (`origin` unless
`remote` names another, see [Configuration](configuration.md)). It accepts
`https://github.com/owner/repo`, `git@github.com:owner/repo` and `ssh://git@github.com/owner/repo`,
each with or without `.git`. Any other URL gives `null`.

## The hook

`wtpool hook` turns Claude Code hook events into one activity record per session or subagent, in
`<git common dir>/wtpool-activity/`. A record holds the agent tool that wrote it (`harness`, here
`claude-code`), the agent's label (see below), the worktree it acts in, its state (`working`, `waiting`, `idle`, `done`, `ended`), what it waits for (permission,
question, plan approval, input), its last tool and target (a file path, or a program such as
`cargo test`), its calls in flight, its background tasks (below), and the id of the Claude Code
process. It stores no prompt text and no command output.

Each call in flight has a neutral `kind`: `shell`, `edit`, `agent` or `other`. The Claude Code
adapter maps Bash to `shell`; Edit, Write and NotebookEdit to `edit`; Agent to `agent`; any other
tool to `other`. `wtpool status` reads these kinds, never tool names: a `shell` call during a build in
its worktree, or an `agent` call while its subagent works or waits, keeps an agent working, and
the end of a `shell` or `edit` call makes `--watch` reread that worktree's git. A record from an
older `wtpool`, without `harness` or `kind`, reads as Claude Code, with each kind mapped from its tool.

An agent is placed by its start directory, then moved by the paths its tools touch (files, Bash
`cd` and path words). So a session started in the foreground that edits files in `app-wt2` shows
on `app-wt2`.

The hook prints nothing, runs no shell and no git, exits 0 on every path, and ends itself after 1 s.
Tool calls wait for no hook but `PermissionRequest`: the other tool call events are async, and the
remaining sync events have a 2 s timeout. Diagnostics go to `$XDG_STATE_HOME/wtpool/hook.log`, by
default `~/.local/state/wtpool/hook.log`.

### Labels

A session's label is its title, as the Claude Code tab shows it, else `session HH:MM`, the local
time of its first event. A name you gave the session (`/rename`) wins over the title Claude Code
generated, and the label follows each change of either. The hook reads them from two places:

- the payload's `session_title`, which newer Claude Code versions send once you named the session;
- the session's transcript, the file the payload's `transcript_path` names. Claude Code appends a
  `{"type":"custom-title","customTitle":…}` line when you name the session and an
  `{"type":"ai-title","aiTitle":…}` line with its generated title, and writes both again as the
  session goes on.

The hook reads the transcript for nothing else. It parses only lines that hold a JSON string ending
in `-title` and keeps only the title of those two entries, cut to 40 characters: no prompt, reply
or tool output is read into the record. Transcripts grow to many MB, so the record keeps
`transcript_offset`, the end of the last whole line read, and each event of the session reads only
the bytes after it, at most the last 1 MiB. A transcript that was cut or replaced, so that the
offset is past its end or not at the start of a line, is read again from its start, with the same
cap. A subagent's events read no transcript.

A subagent's label is the `description` of the Agent call that started it, such as
`Find the parser tests`, else its agent type, such as `Explore`. The call's PostToolUse names the
subagent it started (`tool_response.agentId`), and the session's record keeps the newest 16 such
titles.

### Background tasks

When Claude Code runs a Bash command in the background, the hook adds a task to the agent's
record: Claude Code's task id, the path of the file Claude Code writes the output to, the command's
summary (as in the agent line, such as `cargo build`), and when it started and ended. The path is
`<tmp>/claude-<uid>/<project>/<session_id>/tasks/<task_id>.output`, where `<tmp>` is
`$CLAUDE_CODE_TMPDIR`, else `/tmp`, and `<project>` the folder that holds the session's transcript;
the hook accepts no other path. A record keeps the last 5 tasks. A task ends when Claude Code's
`Stop` no longer lists it among the session's background tasks, when the agent stops it with
TaskStop, or when the session ends.

**Privacy.** wtpool keeps the path, never the output: the hook never opens the file, and no record or
document holds a line of it. The panel reads the output from Claude Code's own file, on this
machine only ([Panel](panel.md#live-output)).

### Other agent tools

Records are the place for other agent tools to plug in. Claude Code names its records
`<session_id>.json`; any other tool must name them `<harness>-<session_id>.json` and set `harness`
to its own name. `wtpool hook --harness <name>` is reserved for such adapters and not built yet. The
record format is in the spec, [status-panel.md](design/status-panel/status-panel.md) (H25, H26).
An adapter that knows its sessions' titles sets `title` and `label`; `transcript_offset` is for an
adapter that reads a transcript as the Claude Code one does (H16).

## Turning it on

1. Install the hooks: the Claude Code plugin, or the settings block by hand. Use one of the two,
   not both. Either way the hooks need Claude Code 2.1.295 or newer.
2. `wtpool setup` creates `wtpool-activity/`, and so does every command that changes the pool. The hook
   writes only into an existing `wtpool-activity/` and never creates it. Without it `agents` is
   `"off"` in `wtpool status`, and the panel says how to turn it on.

`wtpool hook install` checks your setup. It reads `settings.json` in `$CLAUDE_CONFIG_DIR`, by
default `~/.claude/settings.json`, and says on stderr whether Claude Code runs the hooks. The first
line starts `hooks present:`, `hooks present twice:` or `hooks missing:`. A missing setup is named:
no settings file, a file that is not valid JSON, the events without a `wtpool hook` entry, or a
block that runs another `wtpool`. Stdout holds only the block.

### The plugin

Run these two lines in Claude Code:

```
/plugin marketplace add haiyuechen-nearone/wtpool
/plugin install wtpool@wtpool
```

The plugin's hooks run `wtpool hook` for the same 14 events, with the same timeouts and async
flags, as the block below. Like the block, they never hold Claude Code up: a hook that fails or
times out lets Claude Code go on.

**`wtpool` must be on the `PATH` that Claude Code passes to its hooks.** The plugin cannot know
where you installed `wtpool`, so its hooks run the bare command `wtpool`. `cargo install` puts it in
`~/.cargo/bin`, which rustup adds to your `PATH`. Claude Code started from an app or an IDE may get
a shorter `PATH` than your shell. Where no `wtpool` is found, each hook fails to start, and Claude
Code shows a hook error and goes on. Then start Claude Code from a shell that finds `wtpool`, or use
the settings block, which runs the absolute path.

Check it in the shell you start Claude Code from:

```console
$ wtpool hook install > /dev/null
hooks present: ~/.claude/settings.json enables the plugin wtpool@wtpool.
Its hooks run the wtpool on PATH, here /Users/you/.cargo/bin/wtpool.
```

The plugin's version is the version of `wtpool`, so a new release brings new hooks. To update it,
run `claude plugin marketplace update wtpool`, then `claude plugin update wtpool@wtpool`,
in your shell. To remove it, run `claude plugin uninstall wtpool@wtpool`.

### The settings block

`wtpool hook install` prints the hook block on stdout, alone and as valid JSON. Merge it into the
`hooks` of your settings file yourself, next to the hooks already there: **wtpool never writes that
file.** The block runs the absolute path of the `wtpool` you ran, so run the copy you keep, such as
the one in `~/.cargo/bin`. It refuses a binary under a `target` folder.

With [jq](https://jqlang.org), this adds the block's entries to each event and keeps everything
else. It saves a copy of the file first. Run it once: a second run adds the entries again.

```sh
cd ~/.claude    # or "$CLAUDE_CONFIG_DIR"
cp settings.json settings.json.bak
wtpool hook install > wtpool-hooks.json
jq -s '.[0] as $s | .[1].hooks as $b | $s | .hooks = (reduce ($b | keys[]) as $e ($s.hooks // {}; .[$e] = ((.[$e] // []) + $b[$e])))' \
  settings.json.bak wtpool-hooks.json > settings.json
rm wtpool-hooks.json
```

Without a `settings.json` yet, the block is the whole file:
`wtpool hook install > ~/.claude/settings.json`.

```console
$ wtpool hook install > /dev/null
hooks present: ~/.claude/settings.json holds this block.
```

When you move `wtpool`, run `wtpool hook install` from the new place. The note names the path the
block still runs: replace it in `settings.json` with the new one.

If you also install the plugin, every event runs `wtpool hook` twice, and `wtpool hook install`
says `hooks present twice:`. Remove the block then: delete each entry under `"hooks"` whose
`"args"` are `["hook"]`, or restore `settings.json.bak`.

## Cleaning up

The hook cleans up after itself, so no cron or launchd job is needed. After a session ends, at
most once a day per repository, it deletes `ended` or dead records after 7 days, and `done`
subagents after 15 minutes, keeping the newest per worktree. `wtpool-activity/gc.stamp` holds the
time the last gc began. The hook takes the stamp's lock without waiting, so two sessions that end
at once never both run gc. It stops gc 200 ms after the hook started, and leaves the rest to the
next session's end. The status scan, the watcher and the record limits ignore the stamp.

`wtpool hook gc` runs the same gc at once, whatever the stamp says, and stamps it too. It cleans
one repository's pool, so run it inside the repository or set `WTPOOL_FOREGROUND`.

```console
$ wtpool hook gc
wtpool hook gc: kept 0, deleted 0 records
```
