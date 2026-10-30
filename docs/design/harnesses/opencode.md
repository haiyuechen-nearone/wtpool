# OpenCode integration

**Status:** revision 5, 2026-10-30: facts 6 and 14 record that `run --title` creates and then renames its session and that a folder's instance often misses both events, so OC5 adds a once-per-session `ctx.session.get` lookup forwarded as the lookup `session.get` (§3 row; OC4, OC6, OC10 and OC13 follow), §11 drops the lost-title limit, and §13 adds the js check, two translation cases and the live session `oc_run_title`; the owner reviewed phase 5b (jobs.md revision 9). Revision 4, 2026-10-29: phase 5b, pending the owner's review: G4, OC4 and OC11 put the shim folder on each shell call's `PATH` and set `WTPOOL_AGENT_DIR` under `--policy`; OC22 judges in any folder; OC23 notes harnesses whose agents get no shims; Limits, §12, and a new check and live session in §13; revision 3, 2026-10-29: OC31 and its checks: a floating label on hovering a mark shows the model and harness as text, in place of the line's tooltip (the owner); revision 2, 2026-10-29: OC31 the mark beside the state icon, which stays (the owner's decision); `oc_coordinator`'s action `opencode` (H14); revision 1, 2026-10-29: first design; built in phase 5a; pending the owner's review

wtpool shows OpenCode sessions beside Claude Code's, in the same records, document and panel
([status-panel.md](../status-panel/status-panel.md)). This is the first adapter of
[harnesses.md](harnesses.md) to be built: the crate `crates/wtpool-opencode`, which implements
core's `Adapter` trait, and a TypeScript plugin whose source lives in that crate and which
`wtpool hook install --harness opencode` prints. The plugin forwards; the crate translates. §10
covers coordination across harnesses: a Claude Code agent that starts OpenCode agents in the pool's
worktrees, with one view and one guard over all of them. The design rests on OpenCode 2.2.0 as
recorded on 2026-10-29 (§1), and where that recording contradicts harnesses.md §10, the recording
wins (§1, last table). OpenCode's 1.x plugin API is out of scope. Rows are `OC1` to `OC32`; §12
lists the rows of status-panel.md and harnesses.md that change with them.

## Requirements

| # | Requirement |
|---|---|
| G1 | The split holds: OpenCode is the crate and its plugin, and the binary registers the crate. Core gains neutral features only (§12): no OpenCode name, event, field, id shape, path or threshold. |
| G4 | The plugin changes a session's work only under `--policy`: by the deny (OC12) and, from phase 5b, by the shim folder on its shell calls' `PATH` (OC11). Its awaited hooks return within B1 without it (OC7). |
| G5 | Claude Code's records, documents, hook output and install block stay byte-identical, but for the changes that coordination across harnesses needs (§10), which the owner asked for: a record and an actor may name their `model` (H32), a shell call that starts an agent moves no actor (H9), and an agent another agent launched has `launched_by` (S17) and keeps its builds to itself (A1). |
| G6 | No new dependency: the plugin imports nothing and carries its own minimal types; the marks of U18 are SVG paths copied into the extension. |
| OC1 | Proved by recordings: OpenCode 2.2.0's recorded payloads are the translation goldens and the replays, and the `oc` part runs the installed `opencode` with the shipped plugin in a sandbox, on a scripted local model and on one free model of OpenCode's own provider, with no credential (§13). |
| OC2 | Managed however launched: an OpenCode session in a worktree of the pool shows, counts for X12 and takes part in jobs, whether it runs in the TUI, in `opencode run` through the background service, in `opencode run --standalone`, or was started by a shell call of another agent. The plugin loads in every OpenCode server (fact 4), so every mode reports. |

## 1. OpenCode 2.2.0, as recorded

The recording ran the installed `opencode` 2.2.0 (Homebrew `opencode-v2`, Bun 1.4.4 inside) in a
sandbox against a scripted local model, since 2.2.0 keeps credentials only in its own database.
Every hook, event, permission, deny and process fact is OpenCode's own; real token counts,
reasoning events and provider retries are not (§11). `NN:L` is line L of recorded session NN,
which the oracle round commits as `verify/fixtures/opencode-2.2.0/raw/NN-*.hooks.jsonl`.

| Fact | 2.2.0 | Evidence |
|---|---|---|
| 1 | A plugin is a folder, `<config>/plugins/<name>/` with `index.ts` or `server.ts`, where `<config>` is `$OPENCODE_CONFIG_DIR`, else `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`. The `.opencode/plugins/*/` folders of a project and the config key `plugins` load plugins too. TypeScript runs as is. There is no trust step. | 01:1; `OPENCODE_CONFIG_DIR` from the code only |
| 2 | The v2 API: `export default {id, setup(ctx)}`, with no import needed; 18 hooks, each registered by `ctx.<domain>.hook(name, cb)` and awaited with one mutable object; events from `ctx.event.subscribe()`, an async iterable that `setup` must not await; `ctx.location.directory` is the instance's folder. The 1.x `client`, `$` and `directory` are gone. `Bun` exists. | 01:1 to 32; the `@opencode/plugin` 2.2.0 types |
| 3 | The tools are `read`, `edit`, `write`, `patch`, `glob`, `grep`, `shell`, `subagent`, `question`, `skill`, `webfetch` and `websearch`. `bash`, `task` and `todowrite` are gone. | 01:48 |
| 4 | One server process runs the sessions: the background service (`opencode serve --service`, which every TUI and plain `opencode run` use), an `opencode serve`, or, under `run --standalone`, a child `opencode serve --stdio --port 0` of the run. A process that a hook spawns has that server as its parent. | `11-process`; the service as the owner's machine runs it, not recorded |
| 5 | That process holds one plugin instance per folder, each its own module evaluation. Hooks fire only in the instance of the session's folder; every event reaches every instance. | 10:5, 10:151, 10:159, 10:177 to 179 |
| 6 | The first session of a folder is often not seen as created by that folder's instance: the events it sends while the instance loads never reach that instance, whose first event of it is then `session.prompt` or `session.inbox.enqueued`. A folder's plugins load one after another, so one loaded after a slow plugin misses them every time. `ctx.session.get({sessionID})` gives the session as stored, with its `title` once it has one. | 01:56, 03:56, 10:2 to 4; the probe of `lookups.json` |
| 7 | Every event carries `location.directory` but `session.execution.*`, `session.usage.updated`, `session.deleted` and a `session.renamed` from an API rename. | catalogue; 09:24 |
| 8 | A turn is `session.execution.started`, then `.succeeded`, `.failed` (`error`) or `.interrupted` (`reason` `user` or `shutdown`). `session.status` and `session.idle` are never sent. | 01:60, 01:195, 00:86, 05:52, 03:163 |
| 9 | A call starts with the hook `tool.execute.before` (`tool`, `sessionID`, `id`, `input`, the input as the model wrote it, a path relative to the session folder) and ends with `session.tool.success` or `session.tool.failed` (`error.type`). `tool.execute.after` never fires for a user's reject, a deny or an interrupt. | 01:87, 01:96, 03:160, 08:90, 05:90 |
| 10 | Session ids are `ses_` and 26 letters and digits: 12 hex digits of inverted time, then 14 random. Call ids are the provider's, verbatim (`call_s1_0_0` from the scripted model). | the ids table of the recording |
| 11 | A held permission sends `permission.asked` (`id` `per_…`, `sessionID`, `action`, `source.id` the call's id) before the user decides, then `permission.replied` (`requestID`, `reply`). A reject fails the call and ends the turn (`interrupted`, `shutdown`); an interrupt drops the ask with no reply. `opencode run` without a server rejects each ask at once. | 03:94, 03:97, 03:160, 03:163, 05:90, `03-permission-run-autoreject` |
| 12 | The `question` tool sends `form.created` (`form.id` `frm_…`, `form.sessionID`, `form.metadata.kind` `question`, `form.metadata.tool.id` the call's id), and the answer `form.replied` (`id`, `answer`); no `permission.asked`. | 04:35, 04:38 |
| 13 | A subagent is a child session: `session.created` with `parentID`, `agent` and `title`, the call's description, in the parent's folder. Its tool hooks fire with its own `sessionID` in the same instance, and it ends with its own `session.execution.succeeded` before the parent's call ends. | 02:94, 02:113, 02:119, 02:141, 02:142 |
| 14 | A session's title is `session.created`'s `title`, null when none was given; a generated or renamed title comes as `session.renamed` (`title`). `run --title` creates its session with none and renames it at once, before the first prompt, so in a fresh folder the instance often sees neither (fact 6), while `ctx.session.get` gives the title. There is no `session.updated`. | 03:122, 04:1, 01:80, 09:1, 09:24, `06-title-given`, the probe of `lookups.json` |
| 15 | A streamed answer sends `session.text.delta` about every 120 ms (162 over 18.9 s), and nothing else until `session.text.ended`. | 07:83 to 246 |
| 16 | A throw in `tool.execute.before` blocks the call: `session.tool.failed` with `error.type` `unknown` and the thrown message, which the model gets as the call's result; the turn goes on. | 08:87 to 90, 08:108, `08-deny.model-input:3` |
| 17 | `shell.create.before` (`command`, `cwd`, `timeout`, `shell`, `env`) follows a shell call's `tool.execute.before` and comes before any permission ask. `command` is the call's; `cwd` is the session folder, whatever the command `cd`s to; `env` is the server's environment, with `OPENCODE_SERVER_PASSWORD` when the server has one, and is the one field the types leave writable. | 01:135 to 137, 03:90 |
| 18 | The only end of a session is `session.deleted`. A server that exits sends `location.shutdown` to each instance. | 09:25, `00-serve-shutdown` |
| 19 | A spawned child that is not awaited does not hold the hook (0.5 ms); a spawn of `/usr/bin/true` takes about 1 ms. Under `run --standalone` such a child is killed when the run ends, 145 ms after its spawn in the recording. | 11:4, 11:8 |
| 20 | Each step names its model: `session.step.started` with `model.id` and `model.providerID`. | 00:84 |

Where harnesses.md §10 differs, the recording wins:

| harnesses.md §10 said | 2.2.0 does |
|---|---|
| a plugin file `~/.config/opencode/plugins/wtpool.ts` | a plugin folder, `plugins/wtpool/index.ts` (fact 1) |
| two plugin sources, for the 1.x and v2 APIs | the v2 API only (fact 2) |
| turns from `session.status` `busy` and `session.idle` | `session.execution.*` (fact 8) |
| a call ends with `tool.execute.after`, fields `callID`, `tool`, `args` | `session.tool.success` or `.failed`; fields `id`, `tool`, `input` (fact 9) |
| `permission.asked` binds by `tool.callID` | by `source.id`; `permission.replied` names the request, not the call (fact 11) |
| a title from `session.updated`'s `info.title` | `session.created`'s `title` and `session.renamed` (fact 14) |
| `session.error` | `session.execution.failed` (fact 8) |
| progress from `message.part.delta` and `session.status` `busy` | `session.text.delta` (fact 15) |
| `info.directory` is the `cwd` | the envelope's `location.directory`, else the instance's folder (facts 5, 7) |
| hooks inside subagents are disputed | they fire, with the child's id (fact 13) |
| a placeholder title `New session - <time>` | null (fact 14) |
| a contract run through `opencode run --format json` | that stream has no call start and no delta; the `oc` part reads records (§13) |

## 2. The plugin

OpenCode loads the plugin from the user's plugin folder, one instance per folder (fact 5). It
forwards OpenCode's own hooks and events, trimmed; the crate translates them (§3).

| ID | Behaviour |
|---|---|
| OC3 | Shape: one file, `export default { id: "wtpool", setup }`, with no `import`, `require` or package, and its own minimal types for `ctx`, `Bun` and the hook objects. It uses erasable TypeScript only, so Node runs it as is in its tests (§13). `setup` registers `tool.execute.before` and `shell.create.before`, reads `ctx.event.subscribe()` without awaiting it, and returns a cleanup that drops its maps. A hook that fails to register leaves the others working. |
| OC4 | Generated: the source holds one data line, `const CONFIG: Config = {…};`, which install replaces by the JSON of this wtpool's absolute path, the event table's forwarded names with their kept fields and the field that names the session (§3), the lookup's with the fields it is made for (OC5), the id form of OC14, `policy` (`--policy`), from phase 5b with `--policy` `shims`, the absolute path of the shim folder ([jobs.md](../jobs/jobs.md), J26), and the limits of this section. The committed line holds a sample of the same shape, so the file lints and type-checks as committed. |
| OC5 | Once per process: an instance forwards an event only when its `location.directory` is the instance's own folder, `ctx.location.directory`. An event without a location it forwards only for a session of its own: one it has seen in a located event of its folder, forwarded or not, or in a hook. So each event leaves the process once, from the instance of its session's folder. A session it first sees in an event or hook whose row keeps no `title`, it looks up once with `ctx.session.get({sessionID})`, which no hook awaits, as the events that named the session may have gone out before the instance read any (fact 6); it forwards the answer as the lookup `session.get` of §3 only when that answer is the session's and holds a title. |
| OC6 | Payload: one JSON object per forwarded hook or event, `{"v":1, "kind":"event", "hook" or "lookup", "name", "directory": the instance's folder, "parent": the root session's id (OC8), "call": the call's id (OC8), "data"}`, `parent` and `call` only when known. `data` holds only the kept fields of the event's row (§3): never tool output, message text, a prompt, a form's answer, a diff, a header or `env`. A payload over 1 MiB is not sent. |
| OC7 | Spawn: `Bun.spawn([<bin>, "hook", "--harness", "opencode"])` by argv, with no shell, the payload given as its stdin at the spawn, stdout and stderr ignored, never awaited, so the hook returns at once (fact 19). A spawn that throws, or a child that fails, hangs or exits non-zero, changes nothing. Outside the deny of OC12 the plugin never throws, never reads or writes a file and logs nothing. |
| OC8 | Maps, per instance, each bounded: its sessions with their root session, 1024, the least recently seen dropped; a child session's root, from `session.created`'s `parentID`, where a child of a child takes its parent's root; each pending permission's call (`per_…` to `source.id`) and form's call (`frm_…` to `form.metadata.tool.id`), dropped at its reply, 256 each; the pending shell calls of OC11, 64, each dropped after 10 s. Each event of a child session carries `parent`, and each reply its `call`. |
| OC9 | Streaming: `session.text.delta`, `session.reasoning.delta` and `session.tool.input.delta` go out as one `progress` per session at most every 10 s, the first at once, with no text (fact 15). `session.step.started` goes out only when its model differs from the last one sent for that session. |
| OC10 | It forwards the event table of §3 and nothing else, and knows no meaning of an event beyond its kept fields, the maps, the throttle and the lookup of OC5. |
| OC11 | `WTPOOL_SESSION`: a shell call's `tool.execute.before` adds a pending call, its root session and its `command`. `shell.create.before` takes the oldest pending call with that `command` and sets `env.WTPOOL_SESSION` to its root session's stem: `opencode-` and the id in the readable form of OC14. When pending calls with that `command` name more than one root session, when none does, or when the id has another form, it deletes any `WTPOOL_SESSION` the server's environment holds, so an inherited one binds nothing. From phase 5b, under `--policy`, it also puts `CONFIG`'s shim folder first on every shell call's `PATH`, when `env` holds one: `env.PATH` becomes the folder, the platform's `PATH` separator and the value `env` held. It sets `env.WTPOOL_AGENT_DIR` to the hook's `cwd`, the session folder (fact 17), so the call's heavy commands, and those of the scripts it runs, queue through the shims ([jobs.md](../jobs/jobs.md), J25, J27, J29). It forwards nothing of this hook. |
| OC12 | Deny, under `--policy` only: once a `shell` call's `call_start` is sent, `tool.execute.before` runs `wtpool hook policy --harness opencode` with `{tool, sessionID, id, input.command, directory}` on stdin and awaits its stdout for at most 2 s. When stdout is exactly one line `{"decision":"deny","reason":<text>}`, it drops the pending call and throws `new Error(<text>)`, which OpenCode gives the model (fact 16). Any other stdout or none, an exit code, a crash, a missing binary or the 2 s passing lets the call run; a child that still runs at 2 s is killed. |

## 3. The event table

The table is one piece of data in the crate. The plugin's forwarded names and kept fields (OC4),
the parser and the translation cases (§13) all come from it.

| Source | Kept fields | Root session | Child session (`parent` set) |
|---|---|---|---|
| event `session.created`, no `parentID` | `sessionID`, `title` | `session_start`, with the title (OC17) | |
| event `session.created` with `parentID` | `sessionID`, `parentID`, `title`, `agent` | | `subagent_start`: `agent_type` is `agent`, the title its label |
| event `session.execution.started` | `sessionID` | `turn_start` | `subagent_start`: a known subagent works again |
| events `session.execution.succeeded`, `session.execution.interrupted` | `sessionID`, `reason` | `turn_end` with `subagents_done` | `subagent_end` |
| event `session.execution.failed` | `sessionID`, `error.type` | `turn_end` with `subagents_done` and `error`, the `error.type` | `turn_end` with that `error`: `done` with it (A2) |
| hook `tool.execute.before` | `sessionID`, `id`, `tool`, `input.path`, `input.command`, `input.agent` | `call_start` (OC16) | the same, on the subagent |
| events `session.tool.success`, `session.tool.failed` | `sessionID`, `id` | `call_end` | the same |
| event `permission.asked` | `sessionID`, `id`, `source.id` | `wait` `permission` on the call `source.id` (OC18) | the same |
| event `permission.replied` | `sessionID`, `requestID`, `reply`, and `call` (OC8) | `wait_end`, on `call` when known | the same |
| event `form.created` | `form.id`, `form.sessionID`, `form.metadata.kind`, `form.metadata.tool.id` | `wait` `question` for the kind `question`, else `input`, on the call `form.metadata.tool.id` | the same |
| event `form.replied` | `id`, `sessionID`, and `call` (OC8) | `wait_end` | the same |
| event `session.renamed` | `sessionID`, `title` | `info` with the title | `info`: the subagent's label |
| lookup `session.get` (OC5) | `id`, `parentID`, `title` | `info` with the title | `info`: the subagent's label |
| event `session.step.started` (OC9) | `sessionID`, `model.id`, `model.providerID` | `info` with `model` `<providerID>/<id>` (H32) | the same |
| events `session.text.delta`, `session.reasoning.delta`, `session.tool.input.delta` (OC9) | `sessionID` | `progress` | the same |
| event `session.deleted` | `sessionID` | `session_end` | `subagent_end` |

Nothing else is forwarded: not `tool.execute.after`, `permission.evaluate`, `session.prompt`,
`session.step.ended`, `session.usage.*`, `location.shutdown` or any other hook or event.
`session.reasoning.delta` and `session.tool.input.delta` come from the 2.2.0 schema, as the scripted
model sent neither (§11).

| ID | Behaviour |
|---|---|
| OC13 | The adapter reads one payload of OC6. `directory`, an absolute path, is the event's `cwd`. With `parent`, the event is the subagent's: its session is `parent` and its agent the session the payload names (`sessionID`, a lookup's `id`). A payload of another `v`, a name outside the table, a missing field its row needs or a `directory` that is not absolute writes nothing and leaves one H23 line. No event needs a `session.created` before it: whichever event of a session comes first creates its record (fact 6), as for every harness, an `info` creating it `idle` as `session_start` does, while a first `progress` or `wait_end` writes nothing. The record's `process` is the hook's parent, the server (fact 4), with `shared` (OC20). |

## 4. Ids

| ID | Behaviour |
|---|---|
| OC14 | A session or subagent id of OpenCode's own form, 1 to 8 lowercase letters, `_`, then letters and digits, 64 characters at most, takes its readable form: its `_` becomes `-`. So `ses_e76df7aa0ffe3vdlGbn0I4xCYq` is `ses-e76df7aa0ffe3vdlGbn0I4xCYq`, with the stem `opencode-ses-e76df7aa0ffe3vdlGbn0I4xCYq` (39 characters; a subagent's 70). Any other id follows HR2: kept when it matches H17's pattern, else `h` and the first 40 hex digits of its SHA 256. |
| OC15 | A call id is kept when it matches H17's call pattern, else hashed as HR2 says, as `functions.read:0` would be. Permission, form, message and event ids stay inside the plugin and never reach a record. |

The readable form is safe. It maps OpenCode's own form one to one. OpenCode makes only ids with a
`_` (fact 10), so an id equal to a readable form needs a forged payload, which can already write any
record under its own prefix (harnesses.md §8). On a file system that ignores case, two ids meet
only with the same time part and the same 14 random characters but for case. And its stems stay
well inside H17's lengths.

## 5. Calls, titles, waits and subagents

| ID | Behaviour |
|---|---|
| OC16 | Kinds and paths (H26, H8): `shell` is `shell`, its `input.command` read by H9 from the session folder, which a `cd` inside the command does not change (fact 17); `edit`, `write` and `patch` are `edit`; `subagent` is `agent`; every other tool is `other`. The `input.path` of `read`, `edit`, `write`, `glob` and `grep` is the call's path, a relative one joined to the session folder (fact 9). A `question` call waits for the user, `asks` `question`, from the adapter's waiting tools (H13). |
| OC17 | Labels and titles: a session's label is its title, else `opencode HH:MM` (H16, HR7). Every OpenCode title counts as `by` `harness`, so the newest wins: OpenCode already orders its titles, a given one stopping generation (fact 14). A null, empty or blank title, and the 1.x placeholder `New session - <ISO time>`, are no title. A child session's title, its call's description, is the subagent's label. |
| OC18 | Waits: a `wait` with a call binds to the call in flight with that id, else as H13 says. A `wait_end` ends the wait bound to its call, or, without a call, any wait of the actor, which then works. A reject ends the call and then the turn (fact 11); an interrupt ends the call, which ends its wait (H13), and then the turn. Under `opencode run`'s own reject a wait lasts one event. |
| OC19 | Subagents: each child session is a subagent of its root session, stem `<root stem>.<child id>`, placed at its root's place (H10) until its own paths move it; its calls, waits, model and end are its own (§3). A child of a child is one more subagent of the root. |

## 6. One process, many sessions

| ID | Behaviour |
|---|---|
| OC20 | The adapter declares `shared` in every mode, as one server process runs many sessions (fact 4), so each record has `process.shared` `true` (H19). Status then applies, to every shared record and with one set of constants for any shared adapter: no newer session ends an older one (H19); no session works on a build (A1); an `idle` or `quiet` session or subagent hides once its last event is 1 hour old and shows again at its next; a newer main session of the same process placed in the same worktree hides an older `idle` one (A1); gc deletes it 7 days after its `updated` (H21); X12's calling session is the one `WTPOOL_SESSION` names (OC21); and a job's actor comes only from its shell call (S15). |
| OC21 | X12: a shared record counts as the calling session only when the `wtpool` command's environment names its stem in `WTPOOL_SESSION`, set by OC11, and its process is an ancestor of that command, at most 16 levels up (HR25). Without the variable no shared record is left out, and `--force` clears a refusal of the caller's own session. |

## 7. Policy

| ID | Behaviour |
|---|---|
| OC22 | `wtpool hook policy --harness opencode` reads OC12's payload as a shell call of `input.command` in `directory`, which from phase 5b is also the agent's folder ([jobs.md](../jobs/jobs.md), J20, J25), so a `directory` outside every repository is judged by the machine's list; POSIX, never in the background, as 2.2.0's `shell` sends no such flag, so H31 judges no OpenCode call. Where J21 refuses, it prints `{"decision":"deny","reason":<J21's reason>}` and exits 0; on every other path it prints nothing and exits 0 (HR15). |
| OC23 | Under `require`, `wtpool job enforce` and `wtpool job ls` add one note per adapter whose harness refuses nothing yet, or from phase 5b whose agents' shells get no shim folder ([jobs.md](../jobs/jobs.md), J29): Claude Code's as today, from phase 5b also while its settings run no shims entry (H1, H22), and OpenCode's when OC25's plugin file exists and was printed without `--policy`, naming how to print and load it with the policy. |

## 8. Install and presence

| ID | Behaviour |
|---|---|
| OC24 | `wtpool hook install --harness opencode [--policy]` prints the plugin (OC4) on stdout, refuses a binary under `target` (H1) and writes no file. Without `--harness` it prints Claude Code's block, as today. |
| OC25 | Its note on stderr starts `hooks present:` when `<config>/plugins/wtpool/index.ts` (fact 1) holds exactly what this wtpool prints, with or without `--policy`, and says whether its policy is on; else `hooks missing:` and why: no file, the plugin of another wtpool path, or other text, such as an older or edited plugin. Then it says to save the text there, making the folder first, that OpenCode loads plugins when its server starts, so the background service needs `opencode service restart`, which stops the sessions it runs, that project plugins in `.opencode/plugins/` load with no trust step while wtpool installs only this user level file, what the plugin reports, and that it was recorded with OpenCode 2.2.0 and does not support the 1.x API. |
| OC26 | `wtpool hook --harness <name>` and `wtpool hook policy --harness <name>` with a name the binary does not register exit 0 with no output and one H23 line. `wtpool hook install --harness <name>` is a usage error that lists `claude-code` and `opencode`. |

## 9. Security

| Threat | Rule |
|---|---|
| A repository ships an OpenCode plugin | project plugins load with no trust step (fact 1): a repository's plugin can do anything the user can, which wtpool cannot prevent. wtpool installs only the user level file (OC25), and its note says so |
| The plugin leaks what OpenCode sees | it forwards only kept fields (OC6): no prompt, message text, tool output, form answer, header, credential or `env`, so neither the provider key of `session.model.request` nor `OPENCODE_SERVER_PASSWORD` (fact 17) leaves OpenCode; a payload is at most 1 MiB; it spawns `wtpool` by argv with no shell, reads and writes no file, and opens no socket |
| A forged event, from a project plugin or an injected agent that runs `wtpool hook --harness opencode` | as harnesses.md §8: it writes only `opencode-` records in the pool of its `directory` (H6, H7), ids checked or hashed (OC14), strings cleaned (H15), and never another harness's record (H25) |
| A forged or inherited `WTPOOL_SESSION` | it leaves a shared record out of X12 only when that record's process is an ancestor of the command (OC21), and the plugin deletes an inherited one it cannot bind (OC11) |
| The deny is spoofed | the plugin throws only on one exact deny line of the policy command it started (OC12); the policy never allows (HR15) |
| A test run sends text to a model | the scripted sessions reach only loopback; `oc_real_model` uses a free model of OpenCode's own provider, which may keep prompts, so it sends only the sandbox's test prompt about the sandbox repository, never a file or a variable of the owner's (§13) |

## 10. Coordination across harnesses

The owner's flow: a coordinating agent in one harness, such as Claude Code in the foreground,
starts agents hosted in another, such as OpenCode in slots, and wtpool manages them all, with marks
for each agent's model and harness.

| ID | Behaviour |
|---|---|
| OC27 | Managed however launched (OC2): an OpenCode session reports from whichever server runs it (fact 4), so it shows, X12 counts it and its jobs are attributed (S15), with no step of the coordinator's. |
| OC28 | The launch link (S17): a session whose harness process descends from another live agent's own harness process names that agent's session in `launched_by`, and X12 leaves it out when that session runs `fg` or `free` (X12). A session in OpenCode's background service has no link, as the service is no child of the coordinator: a coordinator that wants the link runs `opencode run --standalone`. A build the launched session runs belongs to its own process, never to its launcher (A1). |
| OC29 | A shell call that starts an agent, such as `cd ../app-wt3 && opencode run --standalone …`, moves no actor (H9), so the coordinator stays where it works; its action still names the program. |
| OC30 | Model: a record and an actor may name the newest model the actor used (H32, S11): OpenCode's from `session.step.started` (§3), Claude Code's from its transcript (H16). |
| OC31 | Marks: an agent line keeps its state icon and shows beside it its model maker's mark, with its harness's mark as a badge unless the harness is the maker's own; hovering the mark shows a floating label with the model and harness as text, `<model> · <harness>`, and the detail strip reads `<harness> · <model>` (U13, U18, U5). |
| OC32 | Docs: `docs/status-and-hook.md` and a skill of the repository's Claude Code plugin, `plugins/wtpool/skills/delegate-agents/SKILL.md`, say how a coordinator starts an OpenCode agent in a slot and waits for it: `wtpool bg <branch>` for the slot, then `cd <slot> && opencode run --standalone --format json "<task>"` as a background command, its completion notice and its output file for the result, `wtpool status --json` for its state, and that a run through the background service has no launch link. The skill adds no hook entry, so H28's block is unchanged. |

## 11. Limits

- The first session of a folder shows from its first prompt, not from its creation (fact 6); a
  title it has by then, as one given with `run --title`, comes from the lookup of OC5 (fact 14).
- An idle session has no end event: it hides after 1 hour and ends only by deletion or by its
  server's exit (fact 18, OC20).
- A server restart forgets the plugin's maps: a subagent's later events then show as its own
  session, and a reply as an unbound `wait_end`.
- Under `run --standalone` the last events may lose their `wtpool` process when the run ends (fact
  19); the record then misses its `turn_end`, and hides once its process is gone (A3).
- Unrecorded, so proved only by `oc_real_model` and the owner's live test: real tool choices,
  `session.reasoning.delta`, `session.tool.input.delta`, the inputs of `write`, `patch`, `glob`
  and `grep`, provider retries, compaction, the `always` reply, shells and subagents in the
  background, workspaces, project plugins, `OPENCODE_CONFIG_DIR`, and the background service's own
  plugin lifetimes, inferred from `opencode serve`. Whether `shell.create.before` passes a variable
  it adds to the shell is proved by the `oc` part (§13).
- A plugin installed twice, at the user level and in a project, forwards every event twice.
- The deny and, from phase 5b, the shims need `--policy`; without it `require` only warns for OpenCode sessions, their scripts' heavy commands included (OC23; jobs.md, J30).
- No model shows for a Claude Code subagent: its transcript is not read (H16).

## 12. Changes to the other documents

Core gains these, each neutral; the rows say them.

| Change | Rows |
|---|---|
| an open, validated harness name that each adapter declares; one frozen legacy name for Claude Code's unprefixed stems; a first stem segment of at most 89 characters; the binary's registry as the only list of names; the hook never writes over another harness's record, and status skips a stem whose prefix disagrees | status-panel.md H25; harnesses.md HR17 |
| ids outside the pattern hashed for every adapter but Claude Code's | H17; HR2 |
| `shared` processes: the adapter declares it, the record says it, and status applies one set of rules | H19, H21, A1, X12, S15; HR16 |
| the neutral events `session_start`, `wait_end`, `info` (title and model), `progress`, a wait bound by call id first, and `turn_end` with both `error` and `subagents_done` | H11; HR5, HR30 |
| the label noun of each adapter | H16; HR7 |
| `model` on records and actors | H32, H16, S11 |
| `launched_by`, from the process table, and a build owned by its nearest harness process | S17, X12, U5, A1 |
| a shell call that runs an agent program places nothing | H9 |
| `--harness <name>` on `hook`, `hook policy` and `hook install`, an unknown name silent; neutral help text | §1's opening; OC26 |
| a note per adapter for `require` | OC23; HR13 |
| the agent marks | U5, U13, U18 |
| from phase 5b, every heavy command of an agent queues: the shim folder first on its shells' `PATH` and the agent's folder in `WTPOOL_AGENT_DIR` | [jobs.md](../jobs/jobs.md) J25 to J30; H29, H33, S4, S14, S15, A8, U11, U14, U19 |

Status-panel.md's Phases table gains phase 5a, and harnesses.md marks the rows it builds. The
harness side (not core) generalises replay, scrub, the fixture corpus and the scenario `hook` step
by harness name, and adds the `js` layer and the `oc` part (§13). Left planned, as harnesses.md
says: `reports` and U16's hints (HR9 to HR12), the launcher and detection (HR23 to HR27), the generic
adapter (HR20 to HR22), the CPU sign (HR32), other harnesses, and the move of status-panel.md's
Claude Code rows into their own document (harnesses.md Q11).

## 13. Verification

Every OC row and the new rows H32, S17 and U18 are phase 5 in `verify/trace.toml`, phase 5a being
that phase's first build; a changed row of §12 keeps its phase and gains its checks below from
phase 5. Checks are named `<sut>__should_<assertion>`; translation cases `oc_translate:<case>`
and replays `oc_replay:<session>`, as `cc_replay:<name>` is named.

**Layers and parts.**

| Layer | Part | What runs |
|---|---|---|
| `static` | `quick` | `static:eslint` and `static:prettier` also cover `crates/wtpool-opencode/plugin/`, with the extension's config and tools; `static:plugin_types` runs the extension's `tsc` on the plugin with `strict`, `noEmit` and `erasableSyntaxOnly`, and no type package |
| `js` (new) | `quick` | `node --test` on `vscode/test/opencode/*.test.mjs`: Node's own runner, no package. The tests import `crates/wtpool-opencode/plugin/wtpool.ts` as is, Node removing its types, with a test `CONFIG` line, a fake `ctx` and a fake `Bun`, and feed it the raw recordings |
| `int` | `int` | the checks below, the translation cases and the replays, on the `verify` build |
| `render`, `shot`, `e2e` | `quick`, `ui` | the marks (U18) and the flow `coordinator_launches_opencode` |
| `oc` (new) | `oc` | the live layer: the installed `opencode` with the built `wtpool` and the shipped plugin in a sandbox, its sessions on a scripted local model with exact asserts, and one session on a free model of OpenCode's own provider with the asserts a real model keeps stable; no credential and no cost, so it needs no `--contract` |

**Fixtures** (the oracle round): `verify/fixtures/opencode-2.2.0/raw/` holds the recording's
scrubbed streams per session and instance; `forwarded/` what the shipped plugin forwards from each,
frozen; `lookups.json` what `ctx.session.get` answers in them; `translate/<case>.json` holds
`before` (a record or null), `payload`, `after` (a record, or null for no write) and `log`. A
payload that only the 2.2.0 schema gives, as for the two deltas the scripted model never sent, is
marked `schema`, and the report names such fixtures.

**Checks.**

| Check | Layer | Asserts | Rows |
|---|---|---|---|
| `plugin__should_register_only_its_hooks` | `js` | `setup` registers `tool.execute.before` and `shell.create.before`, reads events without awaiting, keeps working when one registration throws; cleanup drops the maps | OC3 |
| `plugin__should_import_nothing` | `js` | no `import`, `require` or package name; exactly one `CONFIG` line; erasable syntax only | OC3, G6 |
| `plugin__should_forward_each_event_once_per_process` | `js` | `10-two-folders` fed to three instances: each listed event spawns once across them, an event of another folder none, an event without a location only from the instance that knows its session, an unknown session's none | OC5, OC10 |
| `plugin__should_keep_only_the_listed_fields` | `js` | every forwarded payload of every raw stream holds only its row's kept fields, never `env`, output, text, a prompt or an answer; a payload over 1 MiB is not sent | OC6 |
| `plugin__should_send_one_progress_per_session_per_10_s` | `js` | `07-long-answer`'s deltas on a fake clock give one progress at once and the next only 10 s later, per session apart, with no text; `session.step.started` only when its model changes | OC9 |
| `plugin__should_attach_the_root_and_the_call` | `js` | a child's and a grandchild's events carry the root; replies carry their call, an unknown reply none; each map keeps its bound | OC8 |
| `plugin__should_never_await_or_throw_on_the_record_path` | `js` | a spawn that throws, a child that hangs, exits 1 or refuses its stdin: every hook resolves before the child exits, nothing throws, events keep flowing, no file is opened | OC7 |
| `plugin__should_deny_only_on_a_deny_line` | `js` | the deny line throws its reason; empty stdout, garbage, two lines, `allow`, a deny on stderr, a crash, a missing binary and 2 s passing (the child killed) let the call run; `policy` false starts no policy command | OC12 |
| `plugin__should_put_the_shim_folder_first` | `js` | from phase 5b: with `policy` true every shell call's `env.PATH` starts with `CONFIG`'s shim folder and the separator, the server's `PATH` after it unchanged, and `env.WTPOOL_AGENT_DIR` is the hook's `cwd`; with `policy` false, or no `PATH` in `env`, `PATH` is untouched; no other key changes | OC11, G4 |
| `plugin__should_set_wtpool_session_for_its_own_shell_calls` | `js` | the root's stem for a child's call; the variable deleted when two roots pend the same command, when none pends, for an id of another form, and an inherited one removed | OC11 |
| `plugin__should_forward_the_recorded_streams_as_frozen` | `js` | each raw stream gives exactly its `forwarded/` fixture | OC4 to OC10 |
| `plugin__should_look_up_a_session_first_seen_without_its_title` | `js` | a session first seen in `session.inbox.enqueued` or a hook is looked up once, the hook not waiting for it; the lookup sends `id` and `title` alone, a child's with its root; none after `session.created` or `session.renamed`, and nothing for an answer without a title, of another session, an error, none or one after cleanup | OC5, OC6 |
| `oc_translate:<case>` | `int` | each case's `after` and `log` | OC13 to OC19, §3 |
| `oc_replay:<session>` | `int` | each recorded session's forwarded stream through `wtpool hook --harness opencode` gives its expected records and documents, judged as `cc_replay` judges | OC1, §3 |
| `install__should_print_the_opencode_plugin` | `int` | stdout is the template with `CONFIG` holding this binary's absolute path, the event table and `policy`, the same on every run; `--policy` sets it, and from phase 5b adds `shims`, the shim folder's absolute path; a binary under `target` refused; no file written | OC4, OC24 |
| `install__should_report_opencode_presence` | `int` | for `$OPENCODE_CONFIG_DIR`, `$XDG_CONFIG_HOME` and `HOME`: no file, the same text, the same with the other policy, another wtpool's path, edited text, each first line and note | OC25 |
| `hook__should_ignore_an_unknown_harness` | `int` | `hook` and `hook policy` exit 0 with no output and one log line; `hook install` a usage error naming both harnesses | OC26 |
| `policy__should_encode_the_opencode_deny` | `int` | golden stdout and exit 0 for a heavy command under `require`; nothing otherwise, nothing for a background flag; from phase 5b a deny for a `directory` outside every repository when the machine's list names the command | OC22 |
| `policy__should_never_allow` | `int` | over every OpenCode payload: stdout empty or one deny | OC22, HR15 |
| `job__should_note_each_harness_that_refuses_nothing` | `int` | under `require`: OpenCode's note with a plugin printed without `--policy`, none with it or without a file; Claude Code's note as today | OC23 |
| `ids__should_map_opencode_ids` | `int` | the readable form and its stems; hashed for a `.`, a `:`, Unicode, a 65 character own form; kept for a valid id without `_`; call ids kept or hashed | OC14, OC15, H17 |
| `records__should_follow_the_open_stem_grammar` | `int` | an 89 character first segment reads, 90 does not; an `opencode` record without the prefix is skipped and counted; a `claude-code` record named `opencode-…` shows | H25 |
| `hook__should_never_write_over_another_harness_record` | `int` | a Claude Code payload for `opencode-ses-x` over OpenCode's record, and the reverse: no write, one log line; every other path as before | H25 |
| `session_start__should_create_an_idle_record_once` | `int` | an unknown session: `idle`, placed by `cwd`; a known one: bytes unchanged | HR5 |
| `info__should_set_title_and_model_only` | `int` | title and model set, state, `since` and `updated` unchanged; an unknown session created `idle`; an unknown subagent untouched | OC17, H32, HR5 |
| `progress__should_keep_only_working_records_busy` | `int` | on `working`: `updated` refreshed and the action `generating`, a call in flight keeping its action; on `idle`, `done`, `waiting`, `ended` and an unknown actor, and within 10 s of `updated`: no write | HR30 |
| `wait__should_bind_by_call_id_first` | `int` | the call with the id, also when an older call has the same tool and hash; an id not in flight binds by H13 | OC18 |
| `wait_end__should_end_only_its_wait` | `int` | on its call; without a call any wait; another call's wait kept; an unknown actor untouched | OC18 |
| `turn_end__should_finish_subagents_with_an_error` | `int` | `error` with `subagents_done`: the session `idle` with the error, its subagents `done` | HR5 |
| `labels__should_use_the_adapters_noun` | `int` | `opencode HH:MM` for an untitled OpenCode session; null, blank and the 1.x placeholder give it; Claude Code's `session HH:MM` unchanged | OC17, H16 |
| `shared__should_not_supersede` | `int` | two sessions of one shared process both show | H19 |
| `shared__should_never_work_on_a_build` | `int` | a build under the shared process: the worktree `building`, the sessions `idle`; a build under a launched OpenCode server under `claude`: the same, and the Claude Code session shows no `build:` | A1, OC28 |
| `shared__should_hide_after_an_hour` | `int` | `idle` and `quiet` at 59 min 59 s shown, at 1 h hidden, back at the next event; `waiting` and a shell call during a build kept | A1 |
| `shared__should_hide_an_older_idle_session_in_its_worktree` | `int` | the newer session in the same worktree hides the older `idle` one; another worktree, an older working one and an own process keep both | A1 |
| `gc__should_delete_shared_records_by_age` | `int` | a shared record of a live process deleted at 7 days, kept at 6; an own process record as today | H21 |
| `guard__should_leave_out_only_the_named_shared_session` | `int` | `WTPOOL_SESSION` names s1 of a shared ancestor: s1 and its subagents left out, s2 of the same process refuses; without it both refuse; a named record whose process is no ancestor leaves nothing out; `--force` | X12, OC21 |
| `launch__should_name_the_launching_session` | `int` | through `WTPOOL_VERIFY_PROCS`: an OpenCode server under a shell under `claude` names that session; a shared ancestor, pid 1, a dead launcher and 17 levels name none | S17 |
| `guard__should_leave_out_sessions_the_caller_launched` | `int` | the coordinator's `free` of the slot where its launched agent works goes on; another session's refuses; the launched agent's own `free` of the coordinator's worktree refuses | X12, S17 |
| `jobs__should_attribute_a_shared_process_job_by_its_call_only` | `int` | the actor whose shell call matches; no match gives null, never a guessed session; own processes as today | S15 |
| `placement__should_not_move_a_caller_that_starts_an_agent` | `int` | `cd {wt3} && opencode run …`, `sh -c 'cd {wt3} && claude -p x'`, `(cd {wt3} && aider) &` and `timeout 600 codex exec x` move nothing, the action naming the program; `cd {wt3} && ls` still moves | H9 |
| `model__should_follow_the_claude_code_transcript` | `int` | the newest assistant entry's model; `<synthetic>` skipped; a model past the line's first 4 KiB not read; a FIFO or symlink not read; a subagent none | H16, H32 |
| `claude_code__should_stay_byte_identical` | `int` | every Claude Code fixture, scenario and `cc_replay`: records, documents, hook, policy and install output equal goldens made by the base build (wtpool 0.1.0), but for `model`; no fixture runs an agent program or one Claude Code under another, so the changes of H9, S17 and A1 show in none | G5 |
| `budget__b1` | `bench` | also over the forwarded OpenCode corpus: p95 at most 5 ms, max at most 30 ms | B1 |
| `agent_mark__should_follow_model_and_harness` | `render` | each row of U18's makers table; a badge only when the harness is not the maker's; the harness's mark without a model; U13's icon for a harness without a mark; the hover label's text 300 ms after the pointer enters a mark and none after it leaves, with no `title` on the line; the detail strip's `<harness> · <model>` and `launched by` | U18, U5 |
| `agent_mark__should_show_its_label_on_hover` | `shot` | in the four themes, the hover label of a badged OpenCode line and of a Claude Code line: its text, inside the panel's visible area, over the rows without moving them, 3:1 text contrast on the hover background | U18 |
| `agent_mark__should_meet_contrast_and_size` | `shot` | in the four themes, on plain, hovered and selected rows: 3:1, the badge's ring visible, one size with U13's icons | U18, U13 |
| `marks__should_name_their_licence` | `static` | each mark the extension ships has its source and licence in `THIRD-PARTY-NOTICES.md` | U18 |
| `flow__coordinator_launches_opencode` | `e2e` | the flow below, with stub harness processes: a stub `claude` that starts a stub OpenCode server sending forwarded fixtures | OC2, S17, H9, U5, U18 |
| `contract__should_run_opencode_in_the_sandbox` | `oc` | the owner's OpenCode folders, background service (its pid and start) and launchd entries are unchanged; no host but loopback was reached, and in `oc_real_model` only OpenCode's own provider; the sandbox held no credential | OC1 |

**Translation cases.** One per row of §3, both columns, and the edges: `session_created`,
`session_created_titled`, `session_created_child`, `execution_started`,
`execution_started_child`, `execution_succeeded`, `execution_succeeded_child`,
`execution_interrupted_user`, `execution_interrupted_shutdown`, `execution_failed`,
`execution_failed_child`, `tool_before_read_relative`, `tool_before_edit`, `tool_before_write`,
`tool_before_shell`, `tool_before_shell_cd_slot`, `tool_before_subagent`, `tool_before_question`,
`tool_before_child`, `tool_success`, `tool_failed_aborted`, `tool_failed_permission_rejected`,
`tool_failed_denied`, `permission_asked`, `permission_asked_unknown_call`,
`permission_replied_once`, `permission_replied_reject`, `permission_replied_without_call`,
`form_created_question`, `form_created_input`, `form_replied`, `session_renamed`,
`session_renamed_child`, `session_renamed_unknown_session`, `lookup_title`,
`lookup_title_working`, `title_null`, `title_blank`, `title_placeholder`, `step_started_model`,
`step_started_child`, `text_delta_working`, `text_delta_idle`, `reasoning_delta`,
`tool_input_delta`, `session_deleted`,
`session_deleted_child`, `first_event_unknown_session`, `unknown_name`, `unknown_v`,
`missing_field`, `relative_directory`, `ids_readable`, `ids_hashed`, `ids_too_long`,
`ids_valid_without_underscore`, `call_id_hashed`, `other_harness_record`.

**Replays.** One per recorded session: `provider_error_401`, `serve_shutdown`,
`run_read_edit_shell`, `subagent`, `permission_approve_reject`, `permission_run_autoreject`,
`question`, `interrupt`, `title_given`, `long_answer`, `deny`, `delete_api`, `two_folders`.

**The live layer.** `oc` runs as `cc` does (verification.md, Contract test), with OpenCode's own
sandbox: `HOME` and every `XDG_*` in the run folder, only an allowlist of variables, a
`service.json` that turns the background service off, `OPENCODE_DISABLE_AUTOUPDATE=1`,
`OPENCODE_DISABLE_MODELS_FETCH=1`, `OPENCODE_DISABLE_CLAUDE_CODE=1` and
`OPENCODE_DISABLE_CLAUDE_CODE_SKILLS=1`, `run --standalone` or an `opencode serve` on loopback with
a password, and an empty stdin for each `run`. The plugin is what `wtpool hook install --harness
opencode --policy` prints, saved under the run's config with its binary path replaced by a
wrapper that logs each argv and stdin and then runs the build. A recorder plugin beside it gives
the OpenCode asserts. The model is a scripted local server, `vscode/test/oc/stub-model.mjs`, a
Node port of the recording's stub, beside the `cc` runner. A failed OpenCode assert is
`assumption_broken`, a failed wtpool assert a fail. Without an installed `opencode`, the `oc` checks
count by their replays alone, and the report says so.

| Session | Script | OpenCode asserts | wtpool asserts (records and `wtpool status --json`) |
|---|---|---|---|
| `oc_read_edit_shell` | `run --standalone` in fg: read and edit in wt1, `ls`, then `cd {wt1} && git status` | the hooks and events of fact 9, in order | one record `opencode-ses-….json`, harness `opencode`, `process` the `serve --stdio` child with `shared`; on wt1 by `path`; the generated title as label; `idle` after the turn, hidden once the run exits |
| `oc_subagent` | a subagent reads in wt2 | the child's `session.created` with `parentID`; its hooks with its id | a subagent record `<stem>.ses-…` on wt2, its description as label, `done` after its end |
| `oc_permission` | serve, `shell` set to ask, `cd {wt3} && git status` held 5 s, then `once` | `permission.asked` during the hold | `waiting`, reason `permission`, during the hold; `working` after the reply, before the call ends; `idle` at the end |
| `oc_permission_reject` | the same, rejected | `session.tool.failed`, then `interrupted` | `waiting`, then `idle`, nothing in flight |
| `oc_question` | the question tool, answered after 3 s | `form.created` | `waiting`, reason `question`, then `working`, then `idle` |
| `oc_interrupt` | an interrupt while it streams, and one while a permission waits | `interrupted`, `user` | `idle` each time, never `quiet`, nothing in flight |
| `oc_long_answer` | an 800 word answer over about 19 s | at least 100 deltas | the action `generating` and `updated` advanced while it streams, at most once per 10 s; at most 3 progress spawns in the wrapper's log |
| `oc_deny` | `--policy`, `enforce = require`, `cargo test` heavy, the model runs `cargo test` | `session.tool.failed` with J21's reason, the model's next input holding it | the command did not run; the turn ends `idle`; nothing in flight |
| `oc_shims` | from phase 5b: `--policy`, `enforce = require`, limit 1, `cargo build` heavy, the probe command as `cargo` on `PATH`, a job running; the model runs `sh build.sh`, whose one line is `cargo build` | the call's output holds J15's wait line, then the probe's | the probe's `cargo build` is a job of the session once the first job ends; `outside` stays empty |
| `oc_two_sessions` | serve: a session in wt1 and one in wt2 at once, then a newer one in wt1 | one process, an instance per folder | three records of one pid with `shared`, none ending another; the older wt1 one hidden once `idle` while the newer works; each forwarded event spawned once in the wrapper's log |
| `oc_session_delete` | a session deleted through the API | `session.deleted` | `ended` |
| `oc_title` | a session created with a title through the API, then renamed | `session.renamed` | the label follows each |
| `oc_run_title` | `run --standalone --title` in fg, with a plugin named to load before wtpool's that waits 400 ms in `setup` | the plugins after it get no `session.created` or `session.renamed` of the session | one lookup in the wrapper's log; the record's label and title are the given title |
| `oc_shell_env` | serve with a password; a shell call `printenv WTPOOL_SESSION` | the call's output | the output is the session's stem; the password is in no wrapper log line, record or hook log line |
| `oc_coordinator` | a coordinator process with a Claude Code record runs `wtpool bg oc-task`, then `cd {slot} && opencode run --standalone …` in the background as its child | the run's events | the OpenCode session in the slot with `launched_by` the coordinator's key; the coordinator still in fg, its action `opencode` (H14); another session's `wtpool free {slot}` refused, naming the agent; once the run ends the slot's agent leaves and that `free` goes on |
| `oc_wtpool_missing` | the plugin's binary path names no file | the session runs and ends as without the plugin | no record; nothing thrown |
| `oc_real_model` | `run --standalone` from fg on the free model `WTPOOL_VERIFY_OC_MODEL` names, by default `opencode/glm-5.3-flash-free`, with no credential: list the files of wt1 with the shell tool | the model is listed by `opencode models --standalone` in the sandbox, else `assumption_broken`; it calls the shell tool, else `inconclusive` | the session's actor appears in fg, then in wt1 by its shell call (`placed_by` `path`); `working` during the turn and `idle` after it; its `model` the one named; its `harness` and `model` give U18's Zhipu mark with OpenCode's badge |

`oc_real_model` checks only what a real model keeps stable; the scripted sessions keep the exact
asserts. It reaches no host but loopback and OpenCode's own provider, and it may run as often as the
scripted sessions.

**Scenarios** (catalog of verification.md, phase 5): `opencode_two_sessions_one_process`,
`opencode_older_idle_hidden`, `opencode_permission_replied`, `opencode_long_answer` and
`opencode_build_under_shared_process`, each in `int`, `render` and `host` (`clock` steps limit
them to `int` and `render`), and `coordinator_launches_opencode`, also in `e2e`. A scenario's `hook`
step takes `harness`, `claude-code` by default.

**User flow** `coordinator_launches_opencode` (phase 5): a Claude Code session in fg runs `wtpool
bg oc-task`, then starts `opencode run --standalone` in that slot in the background. Within 1 s the
slot's row shows the OpenCode agent `working`, with the DeepSeek mark and OpenCode's badge, the
hover label `deepseek-v4.1-flash-free · OpenCode` on its mark and, in the detail strip, `launched by`
the coordinator's label; the coordinator's line stays in fg with the Anthropic mark and no badge;
once the agent's run ends, its line leaves the slot.

## 14. Open questions

Each decision the lead recommended, with this design's answer, for the owner's review.

| # | Question | Answer |
|---|---|---|
| Q1 | The harness name: open, or a list in core? | Adopted: an open validated name that each adapter declares (HR17), one frozen legacy name for Claude Code's unprefixed stems, the list of accepted names in the binary's registry. |
| Q2 | Ids: readable or hashed? | Adopted: OpenCode's own form takes the readable form, every other id HR2's hash (OC14). No reason against it showed (§4). |
| Q3 | Shared processes: which rules and numbers? | Adopted as recommended: no supersede, no own build, hidden after 1 hour idle or quiet, an older `idle` one hidden by a newer one in its worktree, gc at 7 days by age, one set of constants for every shared adapter (OC20). The recording shows nothing for other numbers: no instance was disposed while its server lived. |
| Q4 | X12 with a shared process, and `wtpool job` (S15)? | Adopted: only the session `WTPOOL_SESSION` names, which the plugin sets through `shell.create.before`'s writable `env` (fact 17, OC11), checked live by `oc_shell_env`; without it, no shared record is left out. The plugin computes the stem from OC14's form, which install writes into `CONFIG`, so the form exists once. S15 uses no such variable: it attributes a shared process's job by its shell call only, with no fallback, as a job record does not keep the variable; that waits for the launcher (HR25). |
| Q5 | The neutral events? | Adopted: `session_start`, `wait_end`, `progress`, and the title on events, here with the model as the event `info`; a wait binds by call id first. Added: `turn_end` with both `error` and `subagents_done`. |
| Q6 | Labels and the placeholder? | Adopted: `opencode HH:MM`; Claude Code keeps `session HH:MM`. 2.2.0 has no placeholder but null (fact 14); the 1.x form counts as no title too (OC17). |
| Q7 | Generate the plugin from the event table, or check that they agree? | Generate (OC4): HR14 asks for one table; the plugin cannot drift from the parser; presence compares the whole text; and the one data line keeps the source valid TypeScript for its gates. |
| Q8 | The deny? | Adopted: under `--policy`, the policy awaited for at most 2 s, which H1 gives Claude Code's policy entry and which outlasts H4's 1 s watchdog; a deny is one JSON line, thrown as its reason (OC12, OC22); the `require` note is per adapter (OC23). |
| Q9 | Install? | Adopted: the plugin with this wtpool's absolute path, saved by the user as `<config>/plugins/wtpool/index.ts`, a folder as 2.2.0 needs (fact 1), not harnesses.md's single file; the note names it and `opencode service restart`; presence reads that file (OC24, OC25). |
| Q10 | What is deferred? | As recommended (§12): `reports` and U16's hints, detection and the launcher, the generic adapter, the CPU sign, other harnesses, and moving status-panel.md's Claude Code rows into their own document. |
| Q11 | Turns, from the prompt hook or the execution events? | `session.execution.*` (fact 8): they also end on a failure or an interrupt. |
| Q12 | A call's start, from `tool.execute.before` or `session.tool.called`? | The hook: the event carries no tool name. It spawns and returns, about 1 ms (fact 19). Ends come only from `session.tool.*` (fact 9). |
| Q13 | Rank a given OpenCode title over a generated one? | No: OpenCode cannot tell a rename from a generated title (fact 14) and orders them itself, so every OpenCode title is `harness` and the newest wins (OC17). |
| Q14 | A Claude Code session named `opencode-…` and an OpenCode session share a stem: what then? | The first harness to write a stem keeps it: the hook never writes over another harness's record, and status skips a record whose prefix disagrees with its `harness` (H25). Claude Code's other paths are unchanged. |
| Q15 | `launched_by`: in the record or in the document? | In the document's actor (S17), computed by `wtpool status` from S4's process table, which already holds every parent. The hook would need that table and every other record on each new session, past B1, and a link in a record would outlive a reparented or dead launcher. |
| Q16 | H9's agent programs: a setting now, or data in core? | Data in core, HR26's default list. The setting `agent_processes` comes with detection (HR26), which needs it anyway. |
| Q17 | Claude Code's model: from where? | Its transcript, which H16 already reads: the newest assistant entry's model in the new part, `<synthetic>` skipped, looked for in a line's first 4 KiB so B1 holds; subagents have none in 5a (H16, H32). |
| Q18 | Marks for model and harness? | Decided by the owner (U18): the model maker's mark, with a badge of the harness's mark unless the harness is the maker's own; the harness's mark when the model is unknown; U13's icon when neither is known; the model and harness as text in a floating label on hover, and `<harness> · <model>` in the detail strip. |
| Q19 | The live layer's model? | The scripted local model, ported to Node beside the `cc` runner, for exact asserts; and one session on a free model of OpenCode's own provider, which needs no login, for the asserts a real model keeps stable (§13). No credential is copied. |
| Q20 | Where the plugin's TypeScript gets its gates and tests? | The extension's own ESLint, Prettier and `tsc` pointed at the plugin (`static`), and Node's test runner in the new `js` layer of `quick`: no new package. |

What the owner decides after this review: when to run `opencode service restart` once the plugin
is installed, since it stops the sessions the service runs; which makers' marks may ship, by each
mark's licence and trademark terms (U18); and the text for their own worktree skill, which lives
outside the repository (OC32).
