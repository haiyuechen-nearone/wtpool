# `wtpool status` and the agent hooks

[README](../README.md) · [Commands](commands.md) · [Configuration](configuration.md) ·
**Status and hook** · [Panel](panel.md) · [Heavy jobs](heavy-jobs.md) ·
[Development](development.md)

## `wtpool status`

`wtpool status --json` prints one line: a JSON document with every worktree of the pool. The pool names
its repository: `repository.name` is the folder name of the main worktree, and `repository.github`
is `owner/repo`, such as `acme/app`, when the configured remote is on github.com, else `null`. Per
worktree it gives the role (`foreground`, `slot`, `unmanaged`), branch, upstream, the git facts of
`wtpool ls`, whether `wtpool bg` would take it (`free`, `reasons`), its activity, the agents there, the
build processes working in it, and the times of its last event and last change. Each agent names
the tool it runs in as `harness`, `claude-code` or `opencode`. It names the model it used last as
`model`, such as `claude-opus-5-5` or `opencode/glm-5.3-flash-free`, once its harness has told wtpool
one. When another agent started it, it names that agent in `launched_by`: see
[Coordinating agents across harnesses](#coordinating-agents-across-harnesses). An agent with
background tasks lists them in `outputs`: the running ones, newest first, then the one that ended last, each with its task
id, the path of its output file, the command's summary, whether it still runs, and when it started
and ended. `outputs` is left out when there are none.

`wtpool status --watch` prints a document at once, then one per change, and
`{"type":"alive","seq":N}` every 10 s. It watches the git common directory and rereads only what
changed: a worktree's `HEAD`, index or operation files, the refs, or one activity record. Agent
events and git changes show within 1 s. Anything else, such as an edited file, shows within one 10 s
heartbeat. It exits 0 when stdin closes or stdout fails.

Builds are processes working in a worktree whose command matches a heavy pattern of this machine
or of this repository, such as `cargo test` or `make` ([Heavy jobs](heavy-jobs.md)). While the
repository has no patterns of its own, its suggestions count, so a repository with a `Cargo.toml`
shows `cargo build` and `cargo test` as builds from the start. A build's `name` is the words its
pattern matched, and `queued` says whether a job holds it.

The document's `machine.jobs` holds this machine's heavy jobs: the limit, the policy and their
sources, the path of `machine.toml`, the heavy patterns with their sources, this repository's
suggestions, and the running jobs, the waiting ones in queue order and the heavy processes outside
any job. Each job or process has its summary, its label, its worktree and agent in this pool, else
`null`, and since when it runs or waits. None holds an argument or the environment. A `wtpool`
without `wtpool job` leaves `jobs` out.

A build a session started keeps that session `working`, also after its turn ended, as when it runs
the build in the background: any build process that descends from the session's Claude Code
process, wherever it works. The agent line then reads `build: <name>`, the outermost such build,
such as `build: cargo test`. When the build ends, the session shows its own state again, within
one 10 s heartbeat. Other children of Claude Code, such as MCP servers, never count, and neither
does a build you start yourself. A build belongs to the nearest harness process above it, so the
build of an agent that another agent started is never the starter's. A session in a shared process
(see [Shared processes](#shared-processes)) works on no build: its builds show as the worktree's
`building`.

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

`wtpool hook` turns an agent harness's hook events into one activity record per session or
subagent, in `<git common dir>/wtpool-activity/`. `wtpool hook` reads Claude Code's events, and
`wtpool hook --harness opencode` reads what the OpenCode plugin forwards
([OpenCode](#opencode)). A record holds the agent tool that wrote it (`harness`, `claude-code` or
`opencode`), the agent's label (see below), the worktree it acts in, its state (`working`,
`waiting`, `idle`, `done`, `ended`), what it waits for (permission, question, plan approval,
input), its last tool and target (a file path, or a program such as `cargo test`), its calls in
flight, its background tasks (below), the id of the harness process, and the newest model it
used. It stores no prompt text and no command output.

Each call in flight has a neutral `kind`: `shell`, `edit`, `agent` or `other`. The Claude Code
adapter maps Bash to `shell`; Edit, Write and NotebookEdit to `edit`; Agent to `agent`; any other
tool to `other`. The OpenCode adapter maps `shell` to `shell`; `edit`, `write` and `patch` to
`edit`; `subagent` to `agent`; any other tool to `other`. `wtpool status` reads these kinds, never
tool names: a `shell` call during a build in its worktree, or an `agent` call while its subagent
works or waits, keeps an agent working, and the end of a `shell` or `edit` call makes `--watch`
reread that worktree's git. A record from an older `wtpool`, without `harness` or `kind`, reads as
Claude Code, with each kind mapped from its tool.

An agent is placed by its start directory, then moved by the paths its tools touch (files, Bash
`cd` and path words). So a session started in the foreground that edits files in `app-wt2` shows
on `app-wt2`. A shell call that starts an agent, such as `cd ../app-wt3 && opencode run …`, moves
nothing: the agent that starts another stays where it works, and its action names the program.

The hook prints nothing, runs no shell and no git, exits 0 on every path, and ends itself after 1 s.
In Claude Code, tool calls wait for no hook but `PermissionRequest` and the heavy job policy: the
other tool call events are async, and the remaining sync events have a 2 s timeout. The policy,
`wtpool hook policy`, runs before each Bash, PowerShell and Monitor call, and prints at most one
refusal, under `enforce = "require"`, else nothing
([Heavy jobs](heavy-jobs.md#warn-and-require)). Diagnostics go to
`$XDG_STATE_HOME/wtpool/hook.log`, by default `~/.local/state/wtpool/hook.log`.
`--harness <name>` picks the adapter of that name for `wtpool hook` and `wtpool hook policy`. A
name the binary does not register exits 0 with no output and leaves one line in `hook.log`.

### Labels

A session's label is its title, as the Claude Code tab shows it, else `session HH:MM`, the local
time of its first event. A name you gave the session (`/rename`) wins over the title Claude Code
generated, and the label follows each change of either. An OpenCode session without a title reads
`opencode HH:MM`. OpenCode orders its own titles, so the newest one it sends wins; a null or blank
title, and the `New session - <time>` placeholder of OpenCode 1.x, are no title. The Claude Code
hook reads titles from two places:

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
titles. An OpenCode subagent's label is the title of its session, which is its call's description.

### Models

A record names the newest model its agent used, as the harness names it, cleaned and cut like any
stored string. The panel shows it as a mark ([Panel](panel.md#agent-marks)).

- **Claude Code.** The hook reads it from the transcript's new part, the same read that finds the
  titles: the newest assistant entry's model, `<synthetic>` skipped. It looks only in the first
  4 KiB of a line and keeps only that string. A subagent shows no model, as its transcript is not
  read.
- **OpenCode.** The plugin forwards the model of each step when it changes, as
  `<provider>/<model>`, such as `opencode/glm-5.3-flash-free`.

An event that names a model replaces the old one, and one that names none keeps it. A record
with no known model has none.

### Background tasks

These are Claude Code's. OpenCode sessions have none: the plugin forwards no task. When Claude
Code runs a Bash command in the background, the hook adds a task to the agent's
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

### OpenCode

`wtpool hook --harness opencode` reads what a small plugin forwards from OpenCode 2.2.0, whose
plugin API is the v2 one. OpenCode 1.x is not supported. The plugin is one TypeScript file with
no import, which `wtpool hook install --harness opencode` prints
([The OpenCode plugin](#the-opencode-plugin)). OpenCode loads it in every server it runs, so a
session shows whether it runs in the TUI, in `opencode run` through the background service, in
`opencode run --standalone`, or was started by another agent's shell call.

The plugin forwards these hooks and events, and nothing else:

| What OpenCode does | What the record shows |
|---|---|
| a session starts, or its first prompt arrives | the session appears, `idle`, placed by its folder |
| a turn starts, ends, fails or is interrupted | `working`, then `idle`; a failure shows its error type |
| a tool call starts, and succeeds or fails | the call in flight and the action; the end of the call |
| a permission or a question waits for you, and you answer | `waiting` with its reason, then `working` |
| the title changes | the label |
| a step starts with another model | the model |
| the model streams text, thinking or tool input | the action `generating`, at most once per 10 s |
| a subagent starts, works and ends | a subagent record, `done` at its end |
| the session is deleted | `ended` |

Each forwarded payload holds only the fields of its row: the session id, call id, tool name, a
path or command of a call, the model's name, a title, a permission or question id and the reason.
**It never forwards a prompt, message text, tool output, a form's answer, a diff, a header, a
credential or the environment**, and it sends no payload over 1 MiB. Each event leaves OpenCode
once, from the plugin instance of its session's folder.

- The plugin starts `wtpool hook --harness opencode` by argument list, with no shell, and gives it
  the payload on stdin. It never waits for it, so OpenCode is never held up. A `wtpool` that is
  missing, slow or fails changes nothing.
- It reads and writes no file, opens no socket and logs nothing.
- It changes a session's work in one case only: the deny of `--policy`
  ([Heavy jobs](heavy-jobs.md#claude-code-opencode-and-other-agent-tools)). It awaits
  `wtpool hook policy --harness opencode` for at most 2 s before a shell call, and refuses the
  call only when that prints the one deny line.
- It sets `WTPOOL_SESSION` in the environment of each shell call a session starts, to the stem of
  that session's record, such as `opencode-ses-e76df7aa0ffe3vdlGbn0I4xCYq`. `wtpool fg` and
  `wtpool free` use it to tell the calling session from its neighbours
  ([Shared processes](#shared-processes)). It removes a `WTPOOL_SESSION` the server inherited and
  it cannot bind to one session. Do not set it yourself.

An OpenCode record is `opencode-<session id>.json` in `wtpool-activity/`, and a subagent's is
`opencode-<session id>.<child id>.json`. OpenCode's own ids, such as `ses_e76df7aa0ffe3vdlGbn0I4xCYq`,
read with the `_` as `-`: `opencode-ses-e76df7aa0ffe3vdlGbn0I4xCYq`. Any other id becomes `h` and
40 hex digits of its SHA 256. The record's process is OpenCode's server, with `shared`.

#### Shared processes

One OpenCode server runs many sessions: the background service, an `opencode serve`, or the child
server of a `run --standalone`. A record whose process runs many sessions has `"shared": true`
in its `process`. `wtpool status` then applies these rules to every shared record:

| Rule | Why |
|---|---|
| A newer session never ends an older one of the same process. | They run side by side. |
| The session never works on a build. A build under the process shows as the worktree's `building`. | The process cannot say which session started it. |
| An `idle` or `quiet` session or subagent hides once its last event is 1 hour old, and shows again at its next event. `waiting` stays. | An idle session has no end event. |
| A newer main session of the same process in the same worktree hides an older `idle` one. | A new prompt in a folder usually means a new session. |
| `wtpool hook gc` deletes the record 7 days after its last update, its process live or not. | The process outlives its sessions. |
| `wtpool fg` and `wtpool free` leave out as the calling session only the one `WTPOOL_SESSION` names, when its process is an ancestor of the command. Without the variable no shared session is left out, and `--force` clears the refusal. | A command cannot otherwise tell its own session from the others. |
| A job's agent is the one whose shell call matches the job. A job that matches none has no agent. | The job's process cannot say which session started it. |

#### OpenCode limits

- The first session of a folder shows from its first prompt, not from its creation, and a title
  given with `opencode run --title` there is never seen.
- An idle session has no end event. It hides after 1 hour and ends only by deletion or by its
  server's exit.
- A restart of the OpenCode server makes the plugin forget its sessions: a subagent's later events
  then show as its own session.
- Under `opencode run --standalone` the last events may lose their `wtpool` process when the run
  ends. The record then misses its turn end and hides once its process is gone.
- A plugin installed twice, at user level and in a project folder, forwards every event twice.
- Without `--policy`, the plugin cannot refuse anything, so `require` only warns for OpenCode.
- Real tool choices, thinking and tool input streams, provider retries, compaction, shells and
  subagents in the background, workspaces and the background service's own plugin lifetimes were
  not in the recording of OpenCode 2.2.0. A live test covers them.
- A Claude Code subagent shows no model.

### Coordinating agents across harnesses

A coordinating agent in one harness can start agents of another in the pool's worktrees, and
wtpool shows and guards all of them in one view. The flow is checked for Claude Code as the
coordinator and OpenCode as the agents. To start an OpenCode agent from Claude Code:

1. **Park the branch.** `wtpool bg fix-login` prints the slot's path on stdout.
2. **Start the run there,** as a background command of the coordinator. In Claude Code that is a
   Bash call with `run_in_background`:

   ```sh
   cd ~/code/app-wt1 && opencode run --standalone --format json "fix the login test"
   ```

3. **Wait** for the completion notice of that command, then read its output file. The coordinator
   runs no polling loop.
4. **Check** with `wtpool status --json` at any time: the slot's worktree lists the agent, `working`
   while it runs.
5. **Release the slot** with `wtpool free <slot>` once the run has ended. The agent leaves the
   slot when its process exits.

What wtpool does for such a flow:

- **The agent shows however it started.** An OpenCode session reports from whichever server runs
  it, so it shows, counts for `wtpool fg` and `wtpool free`, and takes part in jobs, with no step
  of the coordinator's.
- **The launch link.** The agent's `launched_by` names the session that started it, and the
  panel's detail strip reads `launched by <label>`. The link needs `--standalone`: the run's own
  server is then a descendant of the coordinator. A run through OpenCode's background service,
  which every TUI and a plain `opencode run` use, has no link, as the service is not a child of
  the coordinator.
- **The coordinator may free the slot.** `wtpool fg` and `wtpool free` leave out the sessions the
  caller launched, and their subagents, so the coordinator can free the slot where its own agent
  works. Another session's agent there still refuses. The launched agent's own `wtpool free` of
  its coordinator's worktree refuses too.
- **The coordinator stays where it works.** The shell call that starts an agent places nothing,
  so the coordinator's line stays in its worktree, and its action names the program, such as
  `opencode`. This holds for `claude`, `codex`, `gemini`, `aider` and the other agent programs
  wtpool knows, also inside `sh -c` and a subshell.
- **Builds stay with their agent.** A build the launched agent runs belongs to it, never to the
  coordinator.
- **Each agent shows its model and harness,** as [the panel](panel.md#agent-marks) says.

The skill `delegate-agents` of [the Claude Code plugin](#the-plugin) tells a coordinator these
steps.

### Other agent tools

Records are the place for other agent tools to plug in. Claude Code names its records
`<session_id>.json`; any other tool must name them `<harness>-<session_id>.json` and set `harness`
to its own name, as OpenCode does with `opencode-…`. The harness name is lower case letters, digits
and single hyphens, 24 characters at most, and the binary's registry lists the names it accepts:
`claude-code` and `opencode`. The hook never writes over a record of another harness, and
`wtpool status` skips and counts a record whose stem does not start with its harness. The record
format is in the spec, [status-panel.md](design/status-panel/status-panel.md) (H25, H26), and the
OpenCode adapter in [opencode.md](design/harnesses/opencode.md). An adapter that knows its sessions'
titles sets `title` and `label`; `transcript_offset` is for an adapter that reads a transcript as the
Claude Code one does (H16).

## Turning it on

1. Install the hooks for each harness you use. For Claude Code, that is the plugin, or the settings
   block by hand: use one of the two, not both. Either way the hooks need Claude Code 2.1.295 or
   newer. For OpenCode, it is the plugin file ([below](#the-opencode-plugin)), for OpenCode 2.2.0.
2. `wtpool setup` creates `wtpool-activity/`, and so does every command that changes the pool. The hook
   writes only into an existing `wtpool-activity/` and never creates it. Without it `agents` is
   `"off"` in `wtpool status`, and the panel says how to turn it on.

`wtpool hook install` checks your Claude Code setup. It reads `settings.json` in
`$CLAUDE_CONFIG_DIR`, by default `~/.claude/settings.json`, and says on stderr whether Claude Code
runs the hooks. The first
line starts `hooks present:`, `hooks present twice:` or `hooks missing:`. A missing setup is named:
no settings file, a file that is not valid JSON, the events without a `wtpool hook` entry, a
block without the policy entry, or a block that runs another `wtpool`. Stdout holds only the
block.

### The plugin

Run these two lines in Claude Code:

```
/plugin marketplace add haiyuechen-nearone/wtpool
/plugin install wtpool@wtpool
```

The plugin's hooks run `wtpool hook` for the same 14 events, and `wtpool hook policy` before shell
calls, with the same timeouts and async flags, as the block below. The plugin also ships two
skills: `heavy-jobs`, which tells Claude Code to run heavy commands through `wtpool job`, and
`delegate-agents`, which tells a coordinator how to start an OpenCode agent in a slot
([Coordinating agents across harnesses](#coordinating-agents-across-harnesses)). Neither adds a
hook. Like the block, the hooks never hold Claude Code up: a hook that fails or times out lets
Claude Code go on.

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
`"args"` are `["hook"]` or `["hook", "policy"]`, or restore `settings.json.bak`.

### The OpenCode plugin

`wtpool hook install --harness opencode` prints the plugin on stdout and writes no file. Save it
in OpenCode's config folder, which is `$OPENCODE_CONFIG_DIR`, else `$XDG_CONFIG_HOME/opencode`,
else `~/.config/opencode`, as `plugins/wtpool/index.ts`: OpenCode 2.2.0 loads a plugin from a
folder. Then restart OpenCode's service:

```sh
mkdir -p ~/.config/opencode/plugins/wtpool
wtpool hook install --harness opencode > ~/.config/opencode/plugins/wtpool/index.ts
opencode service restart
```

- **`opencode service restart` stops the sessions the service runs.** OpenCode loads plugins when
  its server starts, so choose a time when no session needs to go on. The TUI and a plain
  `opencode run` use the service. A `run --standalone` starts its own server and loads the plugin
  at once.
- **`--policy`** adds the heavy job deny (`wtpool hook install --harness opencode --policy`).
  Without it the plugin refuses nothing, and `require` only warns for OpenCode
  ([Heavy jobs](heavy-jobs.md#claude-code-opencode-and-other-agent-tools)).
- The plugin runs the absolute path of the `wtpool` that printed it, and the command refuses a
  binary under a `target` folder. When you move `wtpool`, print the plugin again and save it over
  the old one.
- Project plugins in `.opencode/plugins/` load with no trust step. wtpool installs only this user
  level file.

Without `--harness`, `wtpool hook install` prints the Claude Code block, as before. A name the
binary does not register is a usage error that lists `claude-code` and `opencode`.

```console
$ wtpool hook install --harness opencode > /dev/null
```

Its stderr starts `hooks present:` when the file holds exactly the plugin this `wtpool` prints,
with or without `--policy`, and says whether its policy is on. Else it starts `hooks missing:` and
says why: no file, the plugin of another `wtpool` path, or other text, such as an older or an
edited plugin. The rest of the note says where to save the plugin, that the service needs a
restart and what the restart stops, that project plugins load with no trust step, what the plugin
reports, and that the plugin was recorded with OpenCode 2.2.0 and does not support 1.x.

## Cleaning up

The hook cleans up after itself, so no cron or launchd job is needed. After a session ends, at
most once a day per repository, it deletes `ended` or dead records after 7 days, and `done`
subagents after 15 minutes, keeping the newest per worktree. It also deletes a record of a shared
process 7 days after its last update, whether its process still runs or not.
`wtpool-activity/gc.stamp` holds the time the last gc began. The hook takes the stamp's lock
without waiting, so two sessions that end at once never both run gc. It stops gc 200 ms after the
hook started, and leaves the rest to the next session's end. The status scan, the watcher and the
record limits ignore the stamp.

`wtpool hook gc` runs the same gc at once, whatever the stamp says, and stamps it too. It cleans
one repository's pool, so run it inside the repository or set `WTPOOL_FOREGROUND`.

```console
$ wtpool hook gc
wtpool hook gc: kept 0, deleted 0 records
```
