# Coding agent harnesses

**Status:** approved, planned for phase 5, revision 4 (2026-10-30: OpenCode's `session.created` row says a missed creation starts the session at its first event and its title is looked up (opencode.md revision 5); revision 3, 2026-10-29: phase 5a builds OpenCode first ([opencode.md](opencode.md)); HR2, HR5 (`session_start`, `wait_end`, `info`, `progress`, a wait bound by call id first), HR7, HR9, HR13, HR15, HR16, HR17, HR25, HR28 and HR30 say what 5a builds; HR19 and §10: the plugin forwards OpenCode's own events, which the adapter translates; §6, §7 and §10 take the recorded 2.2.0 facts (a plugin folder, the v2 API only, `session.execution.*` turns, `session.tool.*` call ends, `session.text.delta`, `shell` and `subagent`, no `session.status`, `session.idle` or `session.updated`); §11, §12, §13 and §15 note 5a; revision 2, 2026-10-26: approved with every recommendation; Q1 to Q16 resolved as recommended; §11's rows go into status-panel.md when phase 5 is built; revision 1: first design)

wtpool's live agent view comes from a harness's hooks, today Claude Code's only
([status-panel.md](../status-panel/status-panel.md), §1). This design lets any coding agent
harness feed the same records and the same document. The core sees only neutral events (§1). An
adapter per harness translates its payloads (§3), the generic adapter reads wtpool's own event
format (§4), and the launcher and process detection cover harnesses without hooks (§5). §7 adds
a sign that an agent is busy while its model generates. This document holds the HR rows; §11 lists
the rows of status-panel.md that change. Every new ID is phase 5, after the phase 4 of
[jobs.md](../jobs/jobs.md): planned, with no check yet. The research behind §10 is of 2026-10-26,
from public docs and, where they say nothing, public source; Sources gives each key's URL.

## Requirements

| # | Requirement |
|---|---|
| G1 | The core's rules see no harness: no tool, event or field name of a harness outside its adapter. |
| G2 | What a harness cannot report shows as unknown, never as a confident state. Every harness maps onto the document's closed sets (`state`, `reason`, `kind`, `placed_by`, `activity`). |
| G3 | No repository adds, changes or forges wtpool's config, and no event names a file that wtpool did not vouch for. |
| G4 | A hook never changes an agent's work or slows it past B1. The one exception is the deny of the jobs policy under `require` (J21). |
| G5 | Claude Code's records, documents, hook output and install block stay byte-identical. |
| G6 | No new dependency. Each adapter is proved by recorded payloads of a named harness version. |

## Choices

The lead's starting idea, checked against the research (§10):

| Idea | Verdict | Why |
|---|---|---|
| A neutral event protocol | kept, with changes: calls may lack an id, a wait can end by its own event, tasks and titles ride on events, and the version rule of S12 | Copilot and Gemini send no call id [GH-R][GM-R]; OpenCode answers a permission prompt with its own event [OC-E] |
| Tiers 0 to 3 | kept as a summary; the panel degrades per capability (`reports`, HR9) | one number hides the gap that matters most, waits: Cursor and Goose can deny a call before it runs, yet send no event while a permission prompt shows [CU-3P][GO-H] |
| A generic adapter | kept, plus events from argv for hooks with no payload, `x-` names, and outputs only in a wtpool folder | Aider's only hook is a command at turn end, with no payload [AI-N] |
| Glue for every harness | replaced for the Claude family: one parser with a dialect table per harness | Codex, Copilot (PascalCase names), Qwen Code, Factory Droid, Kiro, Junie, Devin, Crush and Continue copy Claude Code's event and field names [CX-H][GH-R][QW][FD][KR][JU][DV-LH][CR][CN] |
| A launcher | kept, second to process detection | detection needs no change of habit; the launcher adds an exact end and `WTPOOL_SESSION` |
| Install and tests | kept; adds translation goldens; live runs only for built adapters | a live run needs the owner's paid login per vendor |
| (new) foreign payloads | the Claude Code adapter refuses other harnesses' payloads (HR18) | Cursor runs the hooks of `~/.claude/settings.json` by default, Devin too, and Copilot those of a repository's `.claude/settings.json` [CU-3P][DV-H][GH-R] |
| (new) shared processes | an adapter says whether one process runs one session (HR16) | Cursor, VS Code, OpenCode and Amp run many sessions in one process [CU-H][VS-A][OC-S][AM-P]; H19 would end all but the newest |
| (new) a policy hook never allows | no decision is silence (HR15) | an `allow` can let a call run past the harness's own prompt: Cursor and Copilot document `allow` beside `deny` and `ask` [CU-H][GH-R] |

## Architecture

```
harness hooks ─► wtpool hook --harness <name> ─► adapter ─► neutral event ─► core rules ─► <common>/wtpool-activity/<stem>.json
harness hooks ─► wtpool hook policy --harness <name> ─► adapter ─► policy call ─► J21 ─► adapter encodes a deny
wtpool run -- <agent> ─► launcher record, WTPOOL_SESSION for the agent's hooks and jobs ───────────────────┘
process table ─► wtpool status: detected agents (HR26) + records ─► S12 document
```

Adapters implement core's harness trait, which the `adapter-split` branch creates with core's
`activity` module; built-in adapters are crates, such as `crates/wtpool-claude-code`, and the
binary picks one by `--harness`. The generic adapter is one more implementation of the same trait.
It lives in core beside the neutral types, as its format is their serialization, and no rule calls
it directly. A trait that only turns a payload into an event falls short in ten places, so the
trait also carries them: the decision request and its encoding (HR15), the vouching of output paths
(HR8), the adapter's own environment variables, the process identity and `process` mode (HR16),
`reports` (HR9), install and presence (HR28), transcript titles read through core's safe reader
(H16), foreign payloads (HR18), the shell dialect of a command (the call's `shell`), and the means
of its liveness sign with its CPU floor (HR29, HR32).

## 1. Neutral events

Every event is one JSON object, the generic adapter's input format (§4) and the type the adapters
produce. Envelope, on every event:

| Field | Type | Rule |
|---|---|---|
| `v` | integer | `1` |
| `event` | string | a name of the HR5 table |
| `session` | string | the harness's session id (HR2) |
| `agent` | string or null | the subagent's id; null or absent for the session itself |
| `cwd` | string | absolute: the working folder of the event (HR6) |
| `agent_type` | string or null | the subagent's type, a label fallback (H16) |
| `title` | object or null | `text` (string) and `by` (`user` or `harness`) (HR7) |
| `model` | string or null | the model the actor uses, as the harness names it (status-panel.md H32); from phase 5a |

The call object, in `call` and in `calls`:

| Field | Type | Rule |
|---|---|---|
| `id` | string or null | the harness's call id (HR2, HR4) |
| `tool` | string | the harness's tool name, display only (H15) |
| `kind` | string | `shell`, `edit`, `agent` or `other` (H26); any other text reads as `other` |
| `asks` | string or null | `question` or `plan`: the call itself waits for the user (H13) |
| `command` | string or null | a shell command, read by H9 and H14 |
| `shell` | string | the dialect of `command`: `posix` (default), `powershell` or `cmd`; only `posix` is parsed for paths, the others give their program's basename only |
| `path` | string or null | a file path, read by H7 and H14 |
| `input` | any JSON | hashed for wait binding (H13), never stored |

| ID | Behaviour |
|---|---|
| HR1 | Version: an event is one JSON object of at most 1 MiB with `v` 1. A reader ignores fields it does not know, and an event name it does not know writes nothing and is no error, so newer glue degrades on an older wtpool. Within `v` 1 changes only add optional fields and event names. A removal, a rename, a new meaning or a new value of a closed set (`kind`, `asks`, `shell`, `reason`, `by`) raises `v`; wtpool reads every `v` it knows, and an unknown one writes nothing and leaves one H23 log line. A wrong type or a missing required field writes nothing (H5). |
| HR2 | Ids: `session` and `agent` follow H17's actor pattern, a call's `id` H17's call pattern. The Claude Code adapter keeps H17: an id outside its pattern writes nothing. Every other adapter replaces such an id by `h` and the first 40 hex digits of its SHA 256, so `thr_123` or `20260512_3` [CX-H][GO-H] still name one actor, and no id carries `.`, `/` or a control character. Built in phase 5a, where OpenCode's own id form takes a readable form instead ([opencode.md](opencode.md), OC14). |
| HR3 | One run reads one event. A payload that ends several calls at once uses `calls_end`, at most 64 calls. |
| HR4 | Calls without ids: a `call_start` without `id` gets `n` and its event's microseconds (H24's clock). A `call_end` without `id` ends the oldest call in flight with its tool and input hash, else the oldest with its tool, else nothing. Waits bind as H13 says, by tool and hash. |
| HR5 | Events set the record as this table says, which generalises H11's table; each adapter maps its own events onto it (§9, §10). An event with `agent` applies to that subagent's record. Phase 5a builds the events as core's own types, not yet this JSON form (HR20): `session_start`, `wait_end`, `info` and `progress` are new, `wait` binds by call id first, and `turn_end` may carry `error` and `subagents_done` together ([opencode.md](opencode.md)). |
| HR6 | Placement: the `cwd` of an actor's first event places it (H10, `placed_by` `cwd`); a call's `path` or `command` (H7, H9) and `cwd_change` move it (`path`), and a change back to the first `cwd` moves nothing. A first event without an absolute `cwd` writes nothing. An adapter whose harness sends no `cwd` derives one: the workspace root that holds the event's path, else the first root (Cursor's `workspace_roots` [CU-H], Cline's `workspaceRoots` [CL-1]), else the hook's own working folder. |
| HR7 | Labels: `title` follows H16's ranks, a given title over a generated one. A session without a title is `session HH:MM` for Claude Code, as today, and `<harness> HH:MM` for every other harness, the `x-` of HR17 dropped, as in `codex 14:02`. A subagent's label is the `title` of its `subagent_start`, else `agent_type`, else `subagent`. Built in phase 5a: each adapter declares its noun, `session` for Claude Code and `opencode` for OpenCode. |
| HR8 | Tasks: a `call_end` may carry `task` (`id`, `output`), and `turn_end`, `subagent_end`, `task_end` and `session_end` end tasks as H27 says. The adapter vouches for `output` when the hook writes the task (arch F14): it keeps the path only when its harness writes that task's output there, by that harness's documented layout (H27 for Claude Code), and else drops it, keeping the task. Status keeps its neutral check (S13) and knows no layout. |

| Event | Fields besides the envelope | Effect |
|---|---|---|
| `session_start` | | a new session record, `idle`, placed by `cwd`; nothing for a known one |
| `turn_start` | | `working`; ends a wait, clears `error` and `in_flight` (H12, H13) |
| `call_start` | `call` | adds the call in flight (H12), sets the action (H14); `working` unless waiting; a call with `asks` makes the actor wait on it (H13) |
| `wait` | `reason` (`permission`, `question`, `plan`, `input`), `call` (optional) | `waiting`; with a call it binds to the call in flight with that id, else as H13 says, else it waits for the session if none of its actors waits, until the session's next call end |
| `wait_end` | `call` (optional) | ends the wait bound to that call, or without a call any wait of the actor; `working`; nothing for an actor without a record |
| `call_end` | `call`, `started_agent` (`agent`, `title`), `task` (`id`, `output`), `stops_task` (id), all but `call` optional | ends the call (H12, HR4) and a wait bound to it; `working` unless waiting on another call; may move the actor (HR6); `started_agent` titles a subagent (H16), `task` starts one (HR8), `stops_task` ends one |
| `calls_end` | `calls` (1 to 64) | as `call_end` for each call; ends a wait it lists (H13) |
| `turn_end` | `error` (string), `subagents_done` (bool), `tasks_running` (ids), all optional | `idle`, with `error`; clears `in_flight` and waits; `subagents_done` makes its subagents `done`; `tasks_running` ends the tasks it leaves out (H27) |
| `subagent_start` | | a new `working` subagent at its session's place (H10) |
| `subagent_end` | `tasks_running` (optional) | `done` (A2); clears `in_flight`; tasks as `turn_end` |
| `cwd_change` | | the envelope's `cwd` is the new folder (HR6) |
| `task_end` | `task` (id) | ends that task (H27) |
| `session_end` | | `ended`, its subagent records deleted, its tasks ended (H20, H27), then the day's gc (H21) |
| `progress` | | the model generates: on a `working` record, `updated` refreshed and the action `generating` (HR30) |
| `info` | | sets the envelope's `title` and `model` only: state, `since` and `updated` unchanged; an unknown session is first made as `session_start` makes it, an unknown subagent gets nothing |

## 2. Capabilities and tiers

| ID | Behaviour |
|---|---|
| HR9 | Each adapter declares what its harness reports, `reports`, a list of capability names: `turn_start`, `turn_end`, `calls` (call start and end, with a path or command), `waits` (permission prompts, questions), `subagents` (start and end, with ids), `outputs` (task output files), `titles`, `session_end`, `decide` (a pre-execution deny) and `generating` (a sign between events, HR29). It also declares `process`: `own` when one harness process runs one session at a time, else `shared` (HR16). The tier is a summary: 0 without `turn_end` and `calls`, 1 with `turn_end`, 2 with `turn_start`, `turn_end` and `calls`, 3 with tier 2 and `decide`. Built in phase 5a: `process` only, declared by each adapter; `reports` and the tiers stay planned. |
| HR10 | A record of a harness other than Claude Code holds `reports`: its adapter's list, the list the generic adapter's glue declares (HR20), and each capability an event of the actor showed, as a first `wait` shows `waits`. A Claude Code record holds none and reads as every capability, so its bytes stay as they are (G5). |
| HR11 | Closed sets (arch F3, C2): the generated schema closes `state`, `reason`, `kind`, `placed_by` and `activity`, and a deployed extension takes a new value for a schema error and restarts (X5), so a new value is no addition. Adapters map every harness onto the sets as they are: a detected or launched agent is `idle`, an unknown wait reason is `input`, an unknown tool `other`, every placement `cwd` or `path`. New facts go into new properties or open lists: `reports` is a list of strings each matching `^[a-z][a-z0-9_]{0,23}$`, so a new capability adds, and a consumer ignores names it does not know. S12 gains the rule that a new value of an existing enum raises `version` (§11). |
| HR12 | The panel shows only what an actor's `reports` hold, and says so where it matters, as this table says (U16). |
| HR13 | The jobs policy: `require` (J21) refuses only for actors whose adapter reports `decide`, with its policy entry installed (HR28). For every other actor `require` acts as `warn`: its heavy processes outside a job show `(not queued)` (A8). `enforce` stays one machine value (J4); the jobs strip names the harnesses of live actors that only warn (U11). Phase 5a builds only a note per adapter in `wtpool job enforce` and `wtpool job ls` under `require` ([opencode.md](opencode.md), OC23). |

| Feature | Needs | Without it the panel shows | U16 says |
|---|---|---|---|
| `working` per turn, `idle` at its end | `turn_start`, `turn_end` | calls alone: `working` per call, `quiet` 120 s after the last (A1), never `idle` | `reports no turns` |
| action, tool and target (H14) | `calls` | the state alone | `reports no calls` |
| `waiting` (permission, question, plan, input) | `waits` | `quiet` while a prompt shows, as `quiet · may wait` | `cannot report waits` |
| placement by paths (H9, H10) | `calls` with a path or command | placement by `cwd` alone | |
| title as label (H16) | `titles` | `<harness> HH:MM` (HR7) | |
| subagents (A2, H16) | `subagents` | subagent work counted as the session's | `reports no subagents` |
| background output (S13, U9, X19) | `outputs` | no Output section, no `background:` action | |
| `working` while the model generates past 120 s (HR33) | `generating` | `quiet` once the last event is 120 s old | |
| builds it started (A1) | `process` `own` | the build as the worktree's `building` | |
| end of a session (H20) | `session_end` | the actor leaves when its process dies (A3) | |
| presence of a process | a hook, the launcher or detection | nothing | |
| jobs `require` (J21) | `decide` | `warn` for its actors (HR13) | `require only warns here` |

| Tier | Example | Panel |
|---|---|---|
| 0 | Aider without glue, any CLI by detection | `<harness> HH:MM · idle` while its process runs, `build: <name>` under it (A1), gone when it ends |
| 1 | Aider with its notify command | as 0, the turn ends timed |
| 2 | Goose by glue, without its policy entry | every feature its `reports` hold, `warn` only |
| 3 | Claude Code, Codex, Copilot, Cursor | every feature its `reports` hold, `require` too |

`outputs` and `generating` stand beside the tiers: Claude Code has both, Codex, Cursor, OpenCode and
Amp have `generating` by events, the others by CPU or not at all (§7).

## 3. Adapters

| ID | Behaviour |
|---|---|
| HR14 | An adapter gives core: its name (HR17), `reports` and `process` (HR9), one function from a payload and its own environment variables to one neutral event, the titles of its transcript (H16) if any, the vouched output path of a task (HR8), the decision request and the encoding of a deny (HR15), the foreign markers it refuses (HR18), and its install text and presence check (HR28). Its event table is one piece of data, from which its parser, its install text and any plugin hook file are generated, so the table exists once (hook F2). Claude Code's variable `CLAUDE_CODE_TMPDIR` is read by its adapter, not by core (hook F2). |
| HR15 | Decisions: `wtpool hook policy --harness <name>` (H29 for every harness). The adapter turns a pre-execution payload into a policy call (`session`, `cwd`, `command`, `shell`), J21 answers a deny with its reason or nothing, and the adapter encodes the deny in its harness's form (§10). No decision is silence: no stdout and exit 0, never an `allow`, which in several harnesses skips their own prompt. An error is no decision. An adapter whose harness takes a deny only as an exit code exits 2 with the reason on stderr (Kiro [KR], Cascade [WS-H]); every other path exits 0. Copilot denies a call when a `preToolUse` command crashes or exits non-zero [GH-R], and Kiro blocks on any non-zero exit [KR], so their adapters exit non-zero only to deny. Built in phase 5a for OpenCode: a deny on stdout, `{"decision":"deny","reason":<text>}`, which its plugin throws (opencode.md, OC12, OC22); the exit 2 form stays planned. |
| HR16 | Process: an actor's `process` (H19) is the hook's nearest ancestor, at most 4 levels up, that is not a shell or wrapper (`sh`, `bash`, `zsh`, `dash`, `ksh`, `fish`, `pwsh`, `powershell`, `cmd`, `env`), else the parent. So a harness that runs hooks through `sh -c` [GO-H][WS-H] still names itself. For an adapter with `process` `shared`, H19's "a newer main session in one process ends the older ones" does not apply, and its sessions never work on a build (A1): the build shows as the worktree's `building`. Built in phase 5a: the adapter declares `shared`, the record says `process.shared`, and status applies one set of rules to every shared record (status-panel.md H19, H21, A1, X12, S15; opencode.md, OC20). The walk past shells stays planned: OpenCode's plugin starts the hook by argv, so its parent is the server. |
| HR17 | Names and stems (H25, arch C5): a harness name matches `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`, at most 24 characters. Names of built-in adapters and detection patterns are reserved, as is `run`; a name that starts `x-` belongs to the generic adapter. Claude Code keeps `<session>` and `<session>.<agent>` (H17); every other harness writes `<harness>-<session>` and `<harness>-<session>.<agent>`, and the launcher `run-<16 hex digits>`. Core's `activity` module holds the grammar as a pattern, not an enum: a record name is `<first>[.<agent>].json`, `<first>` 1 to 89 characters of `[A-Za-z0-9-]` (a prefix of up to 25, then H17's 64), `<agent>` 1 to 64. Status and gc need no adapter registry: they read the harness from the record, and skip as S6 does a record not named `claude-code` whose stem starts neither with its harness and `-` nor with `run-`. Built in phase 5a: the open name, the prefix, the 89 character first segment and the prefix check, and the hook never writes over a record of another harness (status-panel.md H25); the binary's registry holds the names it accepts. `x-` names and `run-` stems stay planned. |
| HR18 | Foreign payloads: Cursor, Devin and Copilot run Claude Code's hooks [CU-3P][DV-H][GH-R], and their payload format there is undocumented [CU-3P]. The Claude Code adapter writes nothing for a payload that carries another harness's documented marker (`cursor_version`, `conversation_id` [CU-H]; `prompt_id` [DV-LH]), and logs one H23 line. The harness process's name cannot tell: Claude Code's native CLI runs as its version file, as `2.1.310`, and the VS Code extension's as `claude` (seen on 2026-10-26). Once a recording shows such a payload and that harness's adapter is built, the binary hands the payload to it instead. |
| HR19 | The Claude family: one parser serves Claude Code and every harness that copies its format, with one dialect table each: its event names, field aliases, tools with their kind and path field (H8, H26), its id rule (HR2), async support, deny form and foreign markers. Claude Code is the dialect `claude-code`, unchanged (G5). Bespoke formats (Cursor, Gemini) get adapters of their own; plugin harnesses (OpenCode, Amp) get a plugin that wtpool ships. OpenCode's, built in phase 5a, forwards OpenCode's own hooks and events, trimmed, and its adapter translates them, so the plugin needs no knowledge of the neutral events (opencode.md). |

## 4. Generic adapter

| ID | Behaviour |
|---|---|
| HR20 | `wtpool hook --harness x-<name> [--reports <list>] [--event <event>] [--session <id>]` reads one neutral event (§1) on stdin. With `--event` it takes the event from argv and reads no stdin, so a hook with no payload, such as a notify command, can call it. `--reports` declares capabilities (HR10). The session is the payload's, else `--session`, else the launcher's (HR25). It writes records as `x-<name>`, under stems `x-<name>-<session>` (HR17). H2 to H6, H15, H18, H23 and H24 hold as for Claude Code. |
| HR21 | `wtpool hook policy --harness x-<name>` reads a `call_start` and prints `{"v":1,"decision":"deny","reason":"<text>"}`, or nothing; the glue maps the deny onto its harness. |
| HR22 | Outputs: the generic adapter vouches only for an output that is a regular file, not a symlink, directly inside `<state>/wtpool/outputs/<x-name>/<session>/`, a folder of the user with mode 0700 (`<state>` as in jobs.md). Glue writes a background command's output there, as with `tee`, and no event makes the panel read another file. |

**Example A: Aider, tier 1.** Aider has no hooks; `--notifications-command` runs once the model
has answered and waits for input [AI-O][AI-N]. In `~/.aider.conf.yml`:

```yaml
notifications: true
notifications-command: /Users/me/.local/bin/wtpool hook --harness x-aider --event turn_end
```

Start Aider through the launcher, so the turn ends find their session (HR25):
`wtpool run --harness x-aider -- aider`. The panel shows `aider 14:02 · idle` from the start and
after each answer, `build: cargo` while a build under Aider runs (A1), and nothing after Aider
exits (HR24). Between answers it shows `idle` or `quiet`, never a guessed `working` (G2), and U16
says `reports no turns`. The docs do not say how Aider runs the command [AI-N]; through a shell,
HR16 skips it.

**Example B: Goose, tier 2.** Goose runs command hooks from a plugin's `hooks/hooks.json`, each
through `sh -c`, with a JSON payload of `event`, `session_id`, `tool_name`, `tool_input`,
`tool_call_id` and `working_dir` [GO-H]. In `~/.agents/plugins/wtpool/hooks/hooks.json`, with the
shape of [GO-H]:

```json
{
  "hooks": {
    "SessionStart": [{"hooks": [{"type": "command", "command": "jq -c --arg e session_start -f ~/.agents/plugins/wtpool/goose.jq | ~/.local/bin/wtpool hook --harness x-goose --reports turn_start,turn_end,calls,session_end"}]}],
    "UserPromptSubmit": [{"hooks": [{"type": "command", "command": "jq -c --arg e turn_start -f ~/.agents/plugins/wtpool/goose.jq | ~/.local/bin/wtpool hook --harness x-goose --reports turn_start,turn_end,calls,session_end"}]}],
    "PreToolUse": [{"matcher": ".*", "hooks": [{"type": "command", "command": "jq -c --arg e call_start -f ~/.agents/plugins/wtpool/goose.jq | ~/.local/bin/wtpool hook --harness x-goose --reports turn_start,turn_end,calls,session_end"}]}],
    "PostToolUse": [{"matcher": ".*", "hooks": [{"type": "command", "command": "jq -c --arg e call_end -f ~/.agents/plugins/wtpool/goose.jq | ~/.local/bin/wtpool hook --harness x-goose --reports turn_start,turn_end,calls,session_end"}]}],
    "Stop": [{"hooks": [{"type": "command", "command": "jq -c --arg e turn_end -f ~/.agents/plugins/wtpool/goose.jq | ~/.local/bin/wtpool hook --harness x-goose --reports turn_start,turn_end,calls,session_end"}]}],
    "SessionEnd": [{"hooks": [{"type": "command", "command": "jq -c --arg e session_end -f ~/.agents/plugins/wtpool/goose.jq | ~/.local/bin/wtpool hook --harness x-goose --reports turn_start,turn_end,calls,session_end"}]}]
  }
}
```

and `goose.jq`:

```jq
{v: 1, event: $e, session: .session_id, cwd: .working_dir}
+ (if .tool_name then {call: {id: .tool_call_id, tool: .tool_name, input: .tool_input,
    kind: (if .tool_name == "developer__shell" then "shell"
           elif .tool_name == "developer__text_editor" and .tool_input.command != "view" then "edit"
           else "other" end),
    command: (if .tool_name == "developer__shell" then .tool_input.command else null end),
    path: .tool_input.path}} else {} end)
```

`working_dir` and `tool_call_id` are conditional fields [GO-H], and the tool names and their input
fields come from Goose's developer extension: a recorded payload confirms them before use. Goose
sends no permission event and no subagent event [GO-H], so the panel says `cannot report waits`.
Each event costs a shell and a `jq` on top of wtpool; a built-in dialect (HR19) saves both. The
policy entry is one more `PreToolUse` command,
`jq … | wtpool hook policy --harness x-goose | jq -c '{decision: "block", reason}'`, as Goose takes
`{"decision":"block"}` on stdout [GO-H]; with it, every `--reports` adds `decide`, and Goose is tier
3.

## 5. Launcher and detection

| ID | Behaviour |
|---|---|
| HR23 | `wtpool run [--harness <name>] [--label <text>] -- <program> [<arg>…]` runs the agent as `wtpool job` runs a command (J13, J14: no shell, streams, exit status and signals passed through) and keeps a session record for its life: stem `run-<16 hex digits>`, `harness` the given name, else `x-` and the program's basename in lower case without characters outside `[a-z0-9-]`, else `x-agent`; `process` the child, so liveness is exact; `idle`, placed by its folder (H7); label `--label`, else HR7's. Without a usable `wtpool-activity/` (H6) it runs the agent and keeps no record. A failure of its own never stops the agent: the agent runs, and the failure goes to stderr. |
| HR24 | When the child exits, the launcher marks its record `ended`, and each record that names the child as its `process` too, as a session end does (H20). A harness whose session end comes late or not at all, as Codex's after 30 idle minutes [CX-H], Gemini's unawaited one [GM-R] or Amp's missing one [AM-A], still ends on time. |
| HR25 | The launcher gives the agent `WTPOOL_SESSION=<stem>` and `WTPOOL_HARNESS=<name>`. A hook whose payload names no session (HR20) takes that session only when its record's `process` is an ancestor of the hook, at most 16 levels up, so a leaked or forged variable binds nothing; `wtpool job` attributes a job (S15) by the same rule. While hook records name the launcher's process, its record hides (H19: they are newer), and it shows again once they end and the process lives. From phase 5a OpenCode's plugin sets `WTPOOL_SESSION` for its shell calls, which X12 reads for shared records (opencode.md, OC11, OC21); the launcher and its use by `wtpool job` stay planned. |
| HR26 | Detection, tier 0 without a launcher: `wtpool status` shows each process whose words (J19) match an agent pattern (J18), working in a worktree of the pool, as a session actor: key `proc-<pid>`, `harness` the pattern's, label `<harness> HH:MM` of the process start, `idle`, `placed_by` `cwd`, `reports` `[]`, builds under it (A1). It skips a process that a live record names (H19), and one that descends from such a process or from another match. The patterns are `agent_processes` in `wtpool.toml`, a list of `{pattern, harness}`, `[]` to turn detection off; the default: `claude` (claude-code), `codex`, `copilot`, `cursor-agent` (cursor), `gemini`, `qwen`, `opencode`, `goose`, `amp`, `cline`, `droid`, `kiro-cli` (kiro), `junie`, `devin`, `auggie`, `crush`, `cn` (continue), `aider`. It reads the process table that S4 reads, with arguments (arch F10), keeps only the matched words, and writes no file (S2). |
| HR27 | A detected actor of a harness with a built adapter adds U16's hint `wtpool hook install --harness <name>`. |

## 6. Install

| ID | Behaviour |
|---|---|
| HR28 | `wtpool hook install --harness <name> [--policy]` prints on stdout the config for that harness's user level file, with the absolute path of this wtpool (H1 refuses one under `target`), and on stderr a note: the file it goes into, whether the entries are there (as H22), the harness's trust step (Codex: review in `/hooks` [CX-H]; Gemini: a trusted folder [GM-T]; Cursor: a trusted workspace [CU-H]; Copilot: workspace admission [GH-R]), the capabilities its harness reports, and the harness versions its fixtures come from. `--policy` adds the pre-execution entry (H29) where the adapter reports `decide`. It never writes a file and never prints a project level file. Without `--harness` it prints Claude Code's block, as today (G5). For a plugin harness it prints the plugin's source, to save as one file in the user's plugin folder. `wtpool hook install --list` prints each adapter with its tier and `reports`. Built in phase 5a for OpenCode (opencode.md, OC24, OC25); `--list` stays planned. |

| Harness | User level file | Entries |
|---|---|---|
| codex | `~/.codex/hooks.json` [CX-H] | SessionStart, SessionEnd, UserPromptSubmit, PreToolUse, PermissionRequest, PostToolUse, SubagentStart, SubagentStop, Stop, Interrupt; `"async": true` on the tool events; `["hook", "--harness", "codex"]` |
| copilot | `~/.copilot/hooks/wtpool.json` [GH-R] | the PascalCase events of §10, exec form (`exec`, `args`, CLI only [GH-R]) |
| cursor | `~/.cursor/hooks.json` [CU-H] | sessionStart, sessionEnd, beforeSubmitPrompt, preToolUse, postToolUse, postToolUseFailure, subagentStart, subagentStop, stop; `beforeShellExecution` with `--policy` |
| opencode | `<config>/plugins/wtpool/index.ts`, `<config>` being `$OPENCODE_CONFIG_DIR`, else `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`: a plugin folder, as recorded with 2.2.0 (opencode.md, fact 1) | a plugin with `tool.execute.before`, `shell.create.before` and the events of opencode.md §3; built in phase 5a |
| others | as §10 names | on request, with their adapters |

## 7. Liveness while the model generates

Hooks fire on calls, turns and prompts. While a model writes a long answer or a large file with no
call, no event comes, and after 120 s A1 shows `quiet`, which looks the same as a stalled agent.
The starting idea was to read the growth of the session's transcript, by its size and modification
time only. The measurements and the research below show that no harness writes its transcript while
a message streams, so the sign comes from a progress event where a harness has one, and else from
the harness process's CPU time.

**Measured: Claude Code 2.1.306, model haiku, macOS, 2026-10-26.** Two sandboxed sessions, as the
`cc` part runs them: their own `CLAUDE_CONFIG_DIR` and `CLAUDE_CODE_TMPDIR`,
`CLAUDE_SECURESTORAGE_CONFIG_DIR` empty, the starting session's variables dropped. Each was asked
for a 1500 word essay as its reply, then for the essay saved with the Write tool. Every 100 ms a
sampler read the size and modification time of each file in those folders and the work folder, and
the CPU time of the process tree; it read no content. Run 1 used `-p` with `stream-json`
and partial messages, which timed each content block; run 2 used the terminal UI in a
pseudo-terminal and ran 15 s past the Stop hook. Times are seconds from the start.

| | Run 1, `-p` | Run 2, terminal UI |
|---|---|---|
| Generation | one API message from 2.3 to 45.2: thinking 23 s, text 11 s, the Write call's input 8.5 s | 6.7 (UserPromptSubmit) to 47.4 (PreToolUse) |
| Transcript writes while it generated | none; one write of 123 KB at 45.4, 0.1 s after the message ended, held all three blocks | none from 7.4 to 47.6 |
| Other files while it generated | `.last-cleanup` once at 5.6 (housekeeping) | `.claude.json` at 10.6 and 12.5 and `.last-update-result.json` at 12.5 (an auto-update, below); nothing in the session or tasks folders |
| CPU, % of one core | thinking 0.7, text 3.1, the call's input 2.3 | thinking about 2.9 to 3.6, text 4.8, the call's input 3.3; idle 0.7 before the prompt and 0.3 after Stop; 16 to 19 until 16 s, while the auto-update ran |
| Stream | no gap between deltas over 1.7 s; a `thinking_tokens` system message every 1.2 s while thinking | |

So, for Claude Code 2.1.306: the transcript is written once each API message is complete, with all
its content blocks at once, never while it streams; a thinking block is written only with its
message; nothing else in the session or tasks folders changes while it generates. CPU rises while it
generates in the terminal UI, thinking included, about five to ten times over idle, as the UI
redraws; under `-p` it stays at idle level while the model thinks. These are two runs on one
machine. Run 2's interactive session also auto-updated the machine's shared Claude Code install to
2.1.310 at 12.5 s, outside its sandbox: a contract run of an interactive harness turns its
auto-update off (Claude Code: `DISABLE_AUTOUPDATER=1`). The shared install stays at 2.1.310.

**Researched: the others.** No harness writes streamed output to disk.

| Harness | File during a turn | Progress hook | Sign |
|---|---|---|---|
| Claude Code | per complete message (measured) | none | `cpu` |
| Codex 0.164.0 | rollout per completed item; deltas are "transient" [CX-RO] | `PreModelRequest`, `PostModelRequest` around each model request, observation only, not on the hooks page yet [CX-MR] | `event` from `PreModelRequest`, plus `cpu` |
| Gemini CLI | model message once its stream ends [GM-CR] | `BeforeModel` per request; `AfterModel` per streamed chunk, synchronous [GM-R] | `event` from `BeforeModel` |
| Qwen Code | per record [QW-CR] | none found | `cpu` |
| Cursor | written asynchronously, may lag `stop` [CU-FT] | `afterAgentThought` per completed thinking block, `afterAgentResponse` per completed message [CU-H] | `event` from both; `cpu` in the CLI |
| Copilot CLI and VS Code | `events.jsonl` per completed message or tool event [GH-SE]; VS Code's own session file up to a minute late [VS-ST] | none per model call [GH-R] | `cpu` in the CLI |
| OpenCode 2.2.0 | not measured | the plugin gets `session.text.delta` about every 120 ms while text streams (opencode.md, fact 15) | `event` from the shipped plugin, built in phase 5a |
| Amp | no local file [AM-H] | `PluginThread.state` `running` [AM-A] | `event` from the shipped plugin |
| Goose | `sessions.db` per complete message [GO-SM] | none | `cpu` |
| Cline | per completed message [CL-T] | SDK plugins `beforeModel`, CLI only [CL-3] | `cpu` in the CLI |
| Aider | assistant text once its stream ends [AI-BC] | none | `cpu` with the launcher |
| Windsurf Cascade | transcript at turn end [WS-H] | none | none (retired) |

| ID | Behaviour |
|---|---|
| HR29 | `generating` is a capability (HR9): the adapter supplies a sign that the agent is busy between events, by one of three means it declares: `event` (HR30), `file` (HR31) or `cpu` (HR32). |
| HR30 | `progress` is a neutral event with the envelope only (HR5). On a `working` record it refreshes `updated`, so A1's 120 s start again, and sets the action to the tool `generating` with no target unless a call is in flight. On any other state it writes nothing: a late progress never wakes an `idle`, `done`, `waiting` or `ended` actor. A progress within 10 s of the record's `updated` writes nothing, and adapters and shipped plugins send at most one per actor per 10 s, so a hook at chunk rate costs one read. Sources: Codex's `PreModelRequest`, async; Gemini's `BeforeModel`, not `AfterModel`, which is synchronous and would start a process per chunk; Cursor's `afterAgentThought` and `afterAgentResponse`; the OpenCode plugin on `session.text.delta`, `session.reasoning.delta` and `session.tool.input.delta`, built in phase 5a (opencode.md, OC9); the Amp plugin while `PluginThread.state` is `running`. |
| HR31 | `file`: an adapter may vouch, as HR8 does, for a file its harness appends to while it generates; the record keeps its path as `live_file`. On each heartbeat (W7) `wtpool status --watch` reads its size and modification time through `lstat`, never its content, and only for a regular file; a change in the last 30 s is a sign. No adapter uses it today: every file above grows per completed item, which the hooks already report. |
| HR32 | `cpu`: for an adapter with `process` `own`, `wtpool status --watch` reads the CPU time of the harness process itself (H19), not of its children, so builds and MCP servers count for nothing, through the process port (sysinfo), on each heartbeat. A rate above the adapter's floor since the reading before, at least 5 s earlier, is a sign. Claude Code's floor is 1.5 % of one core: over its measured idle (0.3 to 0.7) and under its measured terminal UI generation (2.9 to 4.8). `wtpool status --json` reads once, so it has no CPU sign. A shared process gives none. |
| HR33 | A `working` record that A1 would show as `quiet`, its last event 120 s or more ago, shows `working` while a sign is under 30 s old: with the action `generating` for an `event` or `file` sign, and `busy` for a `cpu` sign, as the CPU shows only that the process works, not that a model makes progress. A sign never changes `waiting`, `idle`, `done` or `ended`, so a CPU sign on an idle session, such as the auto-update above, shows nothing. With no sign for 30 s the record shows as A1 says. |
| HR34 | The signs read sizes, modification times and CPU times only, never content. They ride on the heartbeat (W7), with no timer or process of their own, and their paths come from the adapter, never from a payload's text alone (HR8). |

Honest limits: `busy` cannot tell a model that streams from a stalled stream under a spinner that
still redraws; under `-p`, in the SDK and in IDE hosted agents (`shared`) Claude Code gives no CPU
sign while it thinks, so those still show `quiet`; and Claude Code's floor rests on two runs of one
version on one machine, so each contract run (§12) measures it again.

## 8. Security

| Threat | Rule |
|---|---|
| A repository sets wtpool's config | wtpool reads no adapter config at all; `wtpool.toml` lives in the git common folder, which no clone carries; the jobs patterns come from it and from the machine file (J4, J20) |
| A repository ships hook config for its harness | the harness's trust step gates it: Codex trusts by hash [CX-H], Gemini by folder and fingerprint [GM-H][GM-T], Cursor by workspace [CU-H], Copilot by admission [GH-R], Qwen by folder [QW]. OpenCode, Goose and Amp load project plugins with no such step [OC-P][GO-H][AM-P]: a repository that runs code there can do anything the user can, which wtpool cannot prevent. wtpool installs only user level config (HR28) and says so |
| A forged event, such as from a prompt injected agent that runs `wtpool hook` itself | it can write only records of its own harness prefix (HR17), in the pool of its `cwd` (H7), with ids checked or hashed (HR2), strings cleaned (H15), sizes capped (H5, HR1, HR3); no rule trusts a payload's harness name, time or process |
| An event names a file to read | only transcripts (H16) and outputs (HR8) are read: each adapter vouches for its harness's layout, the generic adapter only for wtpool's own folder (HR22), and readers open no symlink and only regular files (H16, R12) |
| A variable binds hooks to another session | `WTPOOL_SESSION` binds only a descendant of the launcher's process (HR25) |
| The policy hook is fooled into approving | it never approves (HR15); it can only deny or stay silent |
| Another harness runs the Claude Code block | HR18 |
| Payloads carry personal data | Cursor sends `user_email` [CU-H], others send prompts: parsers keep only the fields they need (H5), and no record holds prompt text (H15) |

## 9. Claude Code onto the protocol

| Claude Code | Neutral event | Note |
|---|---|---|
| `UserPromptSubmit` | `turn_start` | |
| `PreToolUse` | `call_start` | `AskUserQuestion` `asks: "question"`, `ExitPlanMode` `asks: "plan"` (H13); kinds H26; path fields H8 |
| `PermissionRequest` | `wait` `permission`, with `call` | binds by tool and hash (H13) |
| `PostToolUse`, `PostToolUseFailure`, `PermissionDenied` | `call_end` | `tool_response.agentId` gives `started_agent` (H16); `backgroundTaskId` gives `task` with the vouched path (H27); `TaskStop` and `KillShell` give `stops_task` |
| `PostToolBatch` | `calls_end` | |
| `Notification` `permission_prompt` | `wait` `permission`, no `call` | |
| `Notification` `elicitation_dialog`, `elicitation_url_dialog` | `wait` `input`, no `call` | |
| `Notification` `idle_prompt` | `turn_end`, `subagents_done` | |
| `Stop`, `StopFailure` | `turn_end`, with `error` after StopFailure | `background_tasks` gives `tasks_running` |
| `SubagentStart`, `SubagentStop` | `subagent_start`, `subagent_end` | `agent_id`, `agent_type`; SubagentStop's `background_tasks` |
| `CwdChanged` | `cwd_change` | `new_cwd` is the envelope's `cwd` |
| `SessionEnd` | `session_end` | |
| `session_title`, the transcript's title entries | `title` | the adapter's titles (H16) |

`reports`: every capability, `generating` by `cpu` (HR32); `process` `own`; tier 3 with `outputs`;
deny by `hookSpecificOutput` (H29).

## 10. Harnesses

Versions are the newest the docs or release pages showed on 2026-10-26. Tier and `reports` are
what the docs support; a recording may change them.

### Codex CLI, 0.164.0 (2026-10-23) [CX-L]

| Codex | Neutral | Note |
|---|---|---|
| SessionStart | `session_start` | |
| UserPromptSubmit | `turn_start` | |
| PreToolUse | `call_start` | `tool_use_id`; shell and `exec_command` come as `Bash`, `apply_patch` as `apply_patch`, `Edit` or `Write` [CX-H]; the path of a patch is the first file its text names (field unclear: a recording decides) |
| PermissionRequest | `wait` `permission`, `call` | no `tool_use_id` [CX-H]: binds by tool and hash |
| PostToolUse | `call_end` | |
| SubagentStart, SubagentStop | `subagent_start`, `subagent_end` | `agent_id`, `agent_type`, the parent's `session_id` [CX-H] |
| Stop, Interrupt | `turn_end` | an interrupt ends the turn at once, where Claude Code's decays to `quiet` |
| SessionEnd | `session_end` | always sync, 1 s by default [CX-H] |
| PreCompact, PostCompact | none | |

Tier 3. `reports`: turn_start, turn_end, calls, waits, subagents, session_end, decide, generating
(`PreModelRequest`, HR30). No outputs: background terminals have no documented file [CX-D]. No
titles: the transcript "isn't a stable interface" [CX-H]. `process`: `own` for the CLI; the IDE
extension's app server is unclear [CX-H], so `shared` until recorded. Deny:
`hookSpecificOutput.permissionDecision` `deny`, or exit 2 [CX-H]. Gaps: the docs give tool events
inside a subagent no `agent_id` [CX-H], though the source on main sends one on PreToolUse [CX-PT]: a
recording decides, and until then those calls count as the session's; the docs do not say whether
hooks run under `codex exec` [CX-X]; at most 8 async hooks run per session, and the rest wait
[CX-H]. Hooks are on by default, need review in `/hooks`, and project hooks need a trusted `.codex/`
layer [CX-H]. Contract: `codex exec --json`, `CODEX_HOME` in the run folder [CX-X].

### GitHub Copilot: CLI 1.0.103 (2026-10-22) and VS Code's Copilot harness [GH-CL][VS-A]

Install uses PascalCase event names, which give snake_case fields and Claude tool names
[GH-R]. Copilot CLI, VS Code and the Copilot app share one harness and format [VS-A].

| Copilot | Neutral | Note |
|---|---|---|
| sessionStart | `session_start` | |
| userPromptSubmitted | `turn_start` | |
| preToolUse | `call_start` | no call id in any tool event [GH-R]: HR4 |
| permissionRequest | `wait` `permission`, `call` | |
| notification | `wait` `permission` for `permission_prompt`, `wait` `input` for `elicitation_dialog`, `subagent_end` for `agent_completed` | `shell_completed`, `shell_detached_completed` and `agent_idle` map to nothing [GH-R] |
| postToolUse, postToolUseFailure | `call_end` | |
| subagentStart, subagentStop | `subagent_start`, `subagent_end` | `agent_id`, `parent_agent_id` [GH-R] |
| agentStop | `turn_end` | |
| sessionEnd | `session_end` | |
| errorOccurred, preCompact, userPromptTransformed | none | |

Tier 3. `reports`: turn_start, turn_end, calls, waits, subagents, session_end, decide, and
`generating` by `cpu` in the CLI. `process`: `own` for the CLI, `shared` in VS Code. Every hook runs
synchronously [GH-C], so each adds its time (B1). A `preToolUse` command that crashes or exits
non-zero denies the call, and a timeout lets it through [GH-R]: a missing `wtpool` would block every
tool, so install prints the absolute path and its note says so. Deny: `permissionDecision` `deny`
with `permissionDecisionReason` [GH-R]. It reads a repository's `.claude/settings.json` [GH-R]
(HR18). The cloud coding agent runs in GitHub Actions, not on this machine: out of scope. Contract:
`copilot -p --output-format json`, `COPILOT_HOME` in the run folder, `--trust-folder`
[GH-CMD][GH-P].

### Cursor: IDE and CLI v2026.10.22 [CU-H][CU-CL]

| Cursor | Neutral | Note |
|---|---|---|
| sessionStart | `session_start` | fire and forget [CU-H] |
| beforeSubmitPrompt | `turn_start` | the CLI's `-p` mode sends none [CU-F] |
| preToolUse | `call_start` | `tool_use_id`; `Shell` is `shell`, `Write` `edit`; `tool_input.working_directory` |
| postToolUse, postToolUseFailure | `call_end` | |
| subagentStart | `subagent_start` | `subagent_id`; its parent link is reported wrong [CU-B] |
| subagentStop | `subagent_end` | no `subagent_id` [CU-H]: matching unclear, a recording decides |
| stop | `turn_end` | the CLI's `-p` mode sends none [CU-F] |
| sessionEnd | `session_end` | |
| before/afterShellExecution, before/afterMCPExecution, beforeReadFile, afterFileEdit | none | preToolUse covers them; `beforeShellExecution` carries the policy |
| afterAgentResponse, afterAgentThought | `progress` | per completed message and thinking block (HR30) |
| preCompact, workspaceOpen | none | |

Tier 3 without waits: no event shows a permission prompt [CU-3P], so `quiet · may wait`. `reports`:
turn_start, turn_end, calls, subagents, session_end, decide, generating. `process`: `shared` in the
IDE, `own` in the CLI. Deny: `{"permission":"deny","user_message":…,"agent_message":…}` [CU-H].
Invalid JSON on exit 0 blocks a permission hook [CU-H]; whether empty stdout counts is unclear, so a
recording decides between silence and `{}`. Payloads carry `user_email` [CU-H], never read. It runs
Claude Code's hooks from `~/.claude/settings.json` by default [CU-3P] (HR18). Contract:
`agent -p --output-format stream-json --trust` [CU-P][CU-O], which has no turn events.

### The Claude family's other members

| Harness | Events of the family | Differences | Tier, deny | Sources |
|---|---|---|---|---|
| Qwen Code 0.26.1 | nearly all of Claude Code's, with PermissionRequest, PostToolBatch, Notification `permission_prompt` and `idle_prompt`, SubagentStart and SubagentStop | `background_tasks` on SubagentStop; no output layout documented; project hooks only in a trusted folder | 3 with waits and subagents; Claude JSON or exit 2 | [QW][QW-R] |
| Factory Droid | PreToolUse, PostToolUse, UserPromptSubmit, Notification, Stop, SubagentStop, PreCompact, SessionStart, SessionEnd | no subagent id; no PermissionRequest | 3, waits as Notification types allow (unclear); Claude JSON or exit 2 | [FD][FD-X] |
| Kiro CLI | SessionStart, SessionEnd, UserPromptSubmit, PreToolUse, PostToolUse, Stop | any non-zero exit of PreToolUse blocks | 3 without waits or subagents; exit 2 | [KR][KR-M][KR-HL] |
| Junie CLI | SessionStart, UserPromptSubmit, PreToolUse, Stop, StopFailure, PermissionRequest, SessionEnd | no PostToolUse: calls end only at turn end; project hooks only with `--config-location` | 1, with call starts, waits and `decide`; deny form unclear | [JU] |
| Devin CLI and Local | PreToolUse, PostToolUse, PermissionRequest, UserPromptSubmit, Stop, SessionStart, SessionEnd | `cwd` and `hook_event_name` undocumented; reads `~/.claude` (HR18) | 3 with waits; exit 2 or `decision: block` | [DV-H][DV-LH][DV-C] |
| Augment Auggie | PreToolUse, PostToolUse, Stop, SessionStart, SessionEnd | `conversation_id` and `workspace_roots`, no `session_id` or `cwd` | 3 without waits; exit 2 | [AU] |
| Crush | PreToolUse only | subagent calls are not hooked | 0 with `decide`: the policy only; exit 2 | [CR] |
| Continue CLI | "Claude Code-compatible", reads `.claude` | merged 2026-03-04, absent from the docs | unclear | [CN] |

### OpenCode 2.2.0, recorded on 2026-10-29: built in phase 5a [OC-P][OC-V2]

[opencode.md](opencode.md) holds the design, built in phase 5a; its §1 lists the recorded facts and
what they corrected here. wtpool ships the plugin, which forwards OpenCode's own hooks and events,
trimmed, to `wtpool hook --harness opencode`, where the adapter translates them.

| OpenCode 2.2.0 | Neutral | Note |
|---|---|---|
| `session.created` | `session_start`, or `subagent_start` of its parent for a child session (`parentID`) | a folder's instance can miss a session's creation, which comes before it listens: any first event starts the session, and the instance looks up its title ([opencode.md](opencode.md) OC5) |
| `session.execution.started`, `.succeeded`, `.interrupted`, `.failed` | `turn_start`, `turn_end` (with `error` after `.failed`) | `session.status` and `session.idle` are never sent |
| hook `tool.execute.before`; `session.tool.success`, `session.tool.failed` | `call_start`; `call_end` | `id`, `tool`, `input`; `tool.execute.after` misses rejects, denies and interrupts |
| `permission.asked`, `permission.replied`; `form.created`, `form.replied` | `wait` (`permission`, `question`, `input`), `wait_end` | bound by `source.id` or `form.metadata.tool.id` |
| `session.renamed`, `session.step.started` | `info` (title, model) | no `session.updated` exists |
| `session.text.delta` and the other deltas | `progress` | at most one per session per 10 s from the plugin (HR30) |
| `session.deleted` | `session_end` | the only end of a session |

Tier 3: a throw in `tool.execute.before` blocks the call, under `--policy`. Tool names are `shell`
and `subagent`, no longer `bash` and `task`. Hooks fire inside subagents with the child's id.
`process` is `shared` in every mode: the background service, `opencode serve`, and the private
server of `run --standalone` each run many sessions. Project plugins load with no trust step
[OC-P]. Only the v2 plugin API of 2.2.0 is supported; 1.x is out of scope. Kilo Code, built on
OpenCode, may share the hooks [KI], unrecorded. Contract: the `oc` part, on a scripted local model
and on a free model of OpenCode's own provider, in a sandbox (opencode.md §13).

### Gemini CLI 0.65.0, and Antigravity CLI [GM-REL][GM-AG]

| Gemini | Neutral | Note |
|---|---|---|
| SessionStart | `session_start` | |
| BeforeAgent, AfterAgent | `turn_start`, `turn_end` | |
| BeforeTool, AfterTool | `call_start`, `call_end` | no call id: HR4 |
| Notification `ToolPermission` | `wait` `permission`, no `call` | it cannot block [GM-R] |
| SessionEnd | `session_end` | not awaited [GM-R] |
| BeforeModel | `progress` | per model request (HR30) |
| AfterModel, BeforeToolSelection, PreCompress | none | AfterModel fires per chunk, synchronously |

Tier 3: `decision` `deny` or exit 2 on BeforeTool [GM-R]. `reports`: turn_start, turn_end, calls,
waits, session_end, decide, generating (`BeforeModel`). No subagent events, and subagent calls run
as the session's [GM-S]; no output files (background PIDs only [GM-SH]). Hooks run synchronously
[GM-H]. Google stopped serving Gemini CLI to free and AI Pro or Ultra individuals on 2026-06-18 and
points them to Antigravity CLI [GM-AG]. Antigravity's hooks (SessionStart, PreToolUse, PostToolUse,
PreInvocation, PostInvocation, Stop; camelCase, no `cwd`) have one undated page [AG]: not planned.

### Goose 1.56.0 (2026-10-23) [GO-H][GO-REL]

| Goose | Neutral |
|---|---|
| SessionStart, SessionEnd | `session_start`, `session_end` |
| UserPromptSubmit, Stop | `turn_start`, `turn_end` |
| PreToolUse | `call_start` (`tool_call_id`, `working_dir`) |
| PostToolUse, PostToolUseFailure | `call_end` |
| PreToolUseResult, BeforeReadFile, AfterFileEdit, BeforeShellExecution, AfterShellExecution | none |

Tier 3 without waits or subagents ("not currently emitted" [GO-H]); no outputs. Ids such as
`20260512_3` hash (HR2). Deny: exit 2, or `{"decision":"block"}` [GO-H]. Project plugins load with
no approval [GO-H]. First by glue (Example B), a dialect later.

### Amp [AM-P][AM-A]

| Amp plugin event | Neutral |
|---|---|
| `session.start` | `session_start`, `cwd` from `amp.system.workspaceRoot` |
| `agent.start`, `agent.end` | `turn_start`, `turn_end` |
| `tool.call`, `tool.result` | `call_start`, `call_end` (`toolUseID`) |
| `PluginThread.state` `running` | `progress`, every 10 s while it lasts |

Tier 3: `tool.call` can return `reject-and-continue` [AM-A]. No waits: Amp asks no approval by
default [AM-T]; subagent events undocumented [AM-P]; no session end [AM-A]; `process` `shared`, as
one CLI hosts several threads [AM-P]. A shipped plugin.

### Cline: VS Code extension 4.1.25, and the CLI [CL-7]

File hooks TaskStart, UserPromptSubmit, PreToolUse, PostToolUse, TaskComplete and TaskCancel map
to `session_start`, `turn_start`, `call_start`, `call_end` and `turn_end`; there is no `cwd`
(`workspaceRoots`, HR6); a deny is `{"cancel":true}`; the exit code is not read [CL-1][CL-5]. Its
hooks page is a stub since 2026-05-12, and SDK plugins work only in the CLI [CL-2][CL-3]. Not
planned until that settles.

### Windsurf, now Devin Desktop

Cascade's hooks (`pre_run_command`, `post_run_command`, `pre_write_code`, `post_write_code`,
`pre_user_prompt`, `post_cascade_response` and six more; `trajectory_id`; `cwd` only on commands;
exit 2 blocks) [WS-H] belong to an agent that stayed "through July" 2026 [WS-FAQ]; new
conversations start on Devin Local [WS-L], a Claude family member above. Not planned.

### Aider 0.86.2 [AI-PY]

No hooks [AI-O]. `--notifications-command` gives `turn_end` (Example A); with the launcher, tier 1;
by detection, tier 0. No JSON output for a contract run [AI-S].

### Without an adapter

Roo Code shut down on 2026-05-15 and never had hooks [RO-1][RO-2]. Zed's agent has tool
permissions only [ZD]. No hook docs were found for Warp. The Agent Client Protocol [ACP] is a
client protocol: wtpool would have to start and drive the agent, which R8 and the scope
("controlling agents") rule out.

## 11. Rows for status-panel.md, planned, phase 5

These rows change when phase 5 is built. Several of them amend rows that jobs.md's phase 4 also
changed (R7, H1 to H4, H22, A1, S4, S11), so each amendment applies to that phase 4 text.

Phase 5a, which builds OpenCode first ([opencode.md](opencode.md)), changes the §1 intro, H11, H17,
H19 (`shared` only), H25 and S11 below, A1's rule that a shared session works on no build, and the
Phases table, and adds rows this table does not list: H9's agent programs, H16's noun and Claude
Code's model, H21's gc of shared records, H32 (`model`), A1's hiding of shared records, S15, S17
(`launched_by`), X12's calling session for shared and launched sessions, U5, U13 and U18 (the
agent marks). The other rows stay planned.

| Row | Change |
|---|---|
| D6 | new: The core sees a harness only through its adapter, which turns its payloads into the neutral events of harnesses.md (HR1 to HR8). Replaces the stale `(D3)` citations of `hook/mod.rs` and `hook/event.rs`. |
| §1 intro | `wtpool hook --harness <name>` runs the adapter of that harness (harnesses.md §3, §4), no longer reserved. |
| R7 | add: where a harness runs every hook synchronously (Copilot, Gemini), each record hook is a hook it waits for, within B1. |
| H2, H3 | add, after H29: `wtpool hook policy` prints at most one deny in its harness's form, and exits 2 only to deny for a harness that takes no other form (HR15). |
| H11 | Events set `state` as the neutral table (HR5) says; the adapter's table maps its events onto it, as the table below does for Claude Code. |
| H17 | add: another adapter replaces an id outside its pattern by its hash (HR2); file names as H25 says. |
| H19 | `process` is the harness process: the hook's nearest ancestor that is no shell or wrapper (HR16). A newer main session in one process ends the older ones, unless the adapter's `process` is `shared`. |
| H25 | stems as HR17: the open name pattern, `x-` names, `run-` launcher records, and the prefix check. |
| H30 | new: foreign payloads (HR18). |
| H1, H22 | `--harness <name>` and `--list` (HR28); without them, as today. |
| A1 | add: a session of a `shared` adapter never works on a build (HR16); a `working` record past its 120 s shows `working` while a sign of HR29 is under 30 s old, with the action `generating` or `busy` (HR33); the A1 table gains that row before `quiet`. |
| Activity records | the record list adds `reports` (HR10) and `live_file` (HR31), both omitted for Claude Code. |
| W7 | add: each heartbeat also reads the signs of HR31 and HR32, for `working` records only. |
| legend | `working`: an agent acted here within 2 minutes, its shell call builds here, a build or background task it started runs, or it is busy generating (A1). |
| A9 | new: detected and launched agents (HR23, HR26) are session actors, `idle`, with builds under them (A1); a launched one ends when its child exits (HR24). |
| S5 | record names as HR17. |
| S11 | `harness` is an open name (HR17): `claude-code`, a built-in adapter's, an `x-` name or a detection pattern's. |
| S12 | add: a new value of an existing enum (`state`, `reason`, `kind`, `placed_by`, `activity`, `role`, `connection`) raises `version`, as a deployed consumer's validator refuses it (X5). New facts go into new properties or open string lists (S16). |
| S16 | new: an actor of a harness other than Claude Code has `reports` (HR9, HR10), an open list of names; absent means every capability. |
| U16 | new: the detail strip gives each such actor `<harness> · tier N · <what it cannot report>` (HR12's last column), or `no hooks: wtpool sees only that it runs` with HR27's hint; the agent line reads `quiet · may wait` when `waits` is missing. |
| U11 | add: under `require`, the jobs strip names the harnesses of live actors that only warn (HR13). |
| Phases | add 5: the refactor's end state (§13), the generic adapter, the launcher, detection, then adapters in §15's order. |

## 12. Verification plan

Every HR ID is phase 5. They stay out of `verify/trace.toml` until the phase 5 oracle author adds
them in one change entry, and verification.md takes this section. Phase 5a's checks, for the rows it builds, are
in [opencode.md](opencode.md) §13.

1. **Neutral scenarios.** Scenario steps gain `event`, a neutral event fed to
   `wtpool hook --harness x-verify`, so the core's rules (HR1 to HR8, HR16) are judged with no
   harness: calls without ids, `wait_end`, `calls_end`, `tasks_running`, a `shared` process, hashed
   ids. The Claude Code scenarios and `cc_replay` stay as they are, the proof of G5.
2. **Translation goldens.** Per adapter and harness version, `verify/fixtures/<harness>-<version>/`
   holds scrubbed payloads and the neutral event each must give; `int` checks
   `<harness>_translate:<case>`. Before a recording exists, a fixture may come from the harness's
   documented example, marked `docs`, and the report names such fixtures.
3. **Replay.** A recorded session's payloads replayed through `wtpool hook --harness <name>` give
   the expected records and documents, `<harness>_replay:<session>`, as `cc_replay` does.
4. **Contract runs.** One live run per harness version, only for built adapters, through its
   non-interactive mode in the sandbox, with its config in the run folder (`CODEX_HOME`,
   `COPILOT_HOME`, `XDG_CONFIG_HOME`, else a sandbox `HOME`), a recorder hook, the owner's own login
   and the harness's auto-update off; no credential is copied. Sessions mirror `cc`'s where the
   harness can: read, edit and shell in wt1 from fg, a subagent in wt2, a held permission, `cd` into
   a slot, an interrupt, a session end, a policy deny, and §7's measurement again, the file and CPU
   samples of a long answer before any call, which reset the adapter's CPU floor. A failed harness
   assert is `assumption_broken`, a failed wtpool assert a fail. The frozen scrubber of each harness
   also removes emails and prompts. Without a recording, the checks count by replay alone, and the
   report says so.
5. **Checks.** `generic__should_refuse_forged_events` (another prefix, a built-in name, ids outside
   the pattern, a relative `cwd`, an output outside HR22's folder or a symlink, 1 MiB plus one byte,
   an unknown `v`); `policy__should_encode_each_adapter_deny` (golden stdout and exit code);
   `policy__should_never_allow` (every adapter, the whole fixture corpus);
   `policy__should_exit_zero_without_a_deny`; `claude_code__should_drop_foreign_payloads` (each
   marker); `install__should_print_each_harness_config` (valid JSON or TypeScript, an absolute
   path, no project path); `run__should_pass_through_like_a_job` (J13 and
   J14's checks); `run__should_end_its_records_on_exit`; `run__should_bind_only_descendants`;
   `detect__should_show_agent_processes` (through `WTPOOL_VERIFY_PROCS` with `cmd`: the skip rules,
   `[]`); `records__should_follow_the_open_stem_grammar` (an 89 character first segment, a prefix
   that disagrees with `harness`); `progress__should_keep_only_working_records_busy` (on each
   state, within 10 s, with a call in flight); `liveness__should_show_busy_past_120_s` (through
   `WTPOOL_VERIFY_PROCS` lines with a CPU time: over and under the floor, a child's CPU, a `shared`
   process, 30 s without a sign, an `idle` record); `liveness__should_read_no_content` (a
   `live_file` that is a FIFO, a symlink and a file without read permission: sizes only).
6. **Budgets.** B1 holds per adapter: p95 5 ms over 1000 runs of its corpus. B15:
   `wtpool run -- true` adds at most 20 ms at p95 over 200 runs, as B12.

## 13. The refactor

The code reaches §3 in this order, each step byte-identical for Claude Code (G5). "hook F2" is
finding F2 of the hook review, which lists the leaks; "arch" names the architecture review.

| Step | What | Closes |
|---|---|---|
| 1 | core's `activity` module (with `adapter-split`): the record vocabulary, `ACTIVITY_DIR`, `create_activity`, `read_record_file`, a frozen `legacy_kind` for calls an older wtpool wrote (H26), and the stem grammar as HR17's open pattern; `harness` becomes an open name type in records and documents | hook F2: the stem grammar repeated in `store.rs` and `status/records.rs`, whose `is_record_name` caps the first segment at 64; arch F2, C5; hook F3 |
| 2 | stems with their prefix built in one place | hook F2: `event.rs:91-95` builds stems without H25's prefix |
| 3 | neutral `Event` variants (HR5) and the input split into envelope, call, task and title | hook F2: the event names of `apply.rs` are Claude Code's; hook F8 |
| 4 | the adapter trait and the binary's registry; `wtpool hook --harness` | hook F2: `handle()` calls `claude_code::parse` directly |
| 5 | each adapter's environment, and tasks carrying a path the adapter vouched for | hook F2: `CLAUDE_CODE_TMPDIR` in the neutral `HookEnv`, `TaskStart.project`; arch F14 |
| 6 | one event table per adapter, as data, for the parser, install and the plugin's `hooks.json` | hook F2: the event table in 3 places |
| 7 | install and presence per adapter | hook F2: install re-exported as neutral |
| 8 | the process identity of HR16, and the CPU time of HR32, on one process port | arch F10 (PK-4) |
| 9 | the shell dialect of a command | arch F17 |
| 10 | the jobs policy hook through the adapter's decision request and encoder | H29 |

Then phase 5 builds the generic adapter, the launcher and detection, then the adapters of §15. At the owner's request phase 5a
built OpenCode first, with the parts of steps 1 to 7 and 10 it needs ([opencode.md](opencode.md)).

## 14. Open questions

| # | Question | Answer |
|---|---|---|
| Q1 | The Claude family's dialects: in `wtpool-claude-code`, or a crate of their own? | Resolved: in `wtpool-claude-code`, as data beside its parser; Cursor and Gemini get crates of their own; plugin harnesses need no crate. |
| Q2 | Generic names: an `x-` prefix, or any free name? | Resolved: `x-`, so no generic name can ever meet a later built-in adapter's. |
| Q3 | Detection on by default? | Resolved: on, as it writes nothing, shows only processes in this pool, and `agent_processes = []` turns it off. |
| Q4 | A detected or launched agent with no events: `idle`, or a new state such as `present`? | Resolved: `idle` with `reports` `[]` and U16's line; a new state waits for a version 2 document (HR11). |
| Q5 | Foreign payloads in the Claude Code block: drop or route? | Resolved: drop and log now; route once a recording shows each format (HR18). |
| Q6 | Under `require`, harnesses without `decide`: warn only, or refuse to set `require` while one runs? | Resolved: warn only, named in the strip (HR13). |
| Q7 | Which harnesses get live contract runs? | Resolved: built adapters only, one run per harness version on the owner's logins; else replay and the owner's live tests. |
| Q8 | Synchronous harnesses (Copilot, Gemini): record every tool event, or turns only? | Resolved: every tool event, within B1; `wtpool hook install --light` prints the turn events only. |
| Q9 | Copilot fails closed when a `preToolUse` command cannot start. Install that entry? | Resolved: yes, with the absolute path; install refuses a path that is not an executable file, and the note says what a missing binary does. |
| Q10 | Plugins per harness (Codex and Copilot plugins, Cursor plugins, Gemini extensions, an OpenCode npm package), as H28 is for Claude Code? | Resolved: later, once each printed config is proved. |
| Q11 | Split status-panel.md §1's Claude Code adapter into its own document (arch F22)? | Resolved: yes, when the second adapter lands: `docs/design/harnesses/claude-code.md`; the neutral rows stay in status-panel.md. |
| Q12 | Codex's docs give subagent calls no `agent_id`, though its source on main sends one on PreToolUse [CX-PT]. Count such calls as the session's until then? | Resolved: yes, the session's, with no guessing by time (G2); use the field once a recording shows it. |
| Q13 | Build Gemini CLI at all? | Resolved: only on request, as Google moved individuals to Antigravity CLI [GM-AG], whose hooks are barely documented [AG]. |
| Q14 | A CPU sign: show `busy`, or keep `quiet`, as it cannot tell a stalled stream from a live one? | Resolved: show `busy`, never `generating`, only on `working` records, with the floor measured again by each contract run (HR32, HR33). |
| Q15 | Gemini's `AfterModel` fires per streamed chunk. Install it for a finer sign? | Resolved: no, as it is synchronous and would start a process per chunk; `BeforeModel` per request is enough (HR30). |
| Q16 | The transcript idea: keep `file` (HR31), which no adapter uses today? | Resolved: keep it in the design and build it only with the first adapter whose harness grows a file while it streams. |

## 15. Build order

| Rank | Adapter | Why |
|---|---|---|
| 0 | the refactor (§13), with `adapter-split` | every adapter needs it; HR18 also guards today's Claude Code hook |
| 1 | generic adapter, launcher and detection | one package: it makes the protocol public, tests the core with no harness, gives tier 0 to every CLI agent and tier 1 or 2 to any glue |
| 2 | Codex | a Claude family dialect, so the smallest work; tier 3 with waits and subagents; widely used; scriptable for contract runs |
| 3 | Copilot, CLI and VS Code | a dialect through PascalCase names; tier 3 with waits and subagents; the largest IDE user base; needs care: synchronous hooks, fail closed |
| 4 | Cursor | a bespoke format and no waits, but a large user base, and it already runs the Claude Code block (HR18) |
| 5 | Qwen Code, Factory Droid, Devin, Kiro, Junie, Auggie | dialect tables, each on request with its recording |
| 6 | OpenCode | a shipped plugin with waits, subagents and titles; built first, as phase 5a, at the owner's request ([opencode.md](opencode.md)) |
| 7 | Goose | glue first (Example B), a dialect later |
| 8 | Gemini CLI | moved to Antigravity CLI for individuals |
| 9 | Amp, Cline | plugin APIs still moving; no waits or no `cwd` |
| none | Aider, Windsurf Cascade, Roo Code, Crush, Zed, Warp, Copilot's cloud agent, ACP | glue, retired, shut down, policy only, no hooks, not local, or out of scope |

## Sources

All read on 2026-10-26. Pages without a date are marked undated. Source files on a project's main
branch were read where the docs say nothing; §7's measurements are this design's own.

| Key | URL |
|---|---|
| AG | https://antigravity.google/docs/hooks?tab=cli (undated) |
| ACP | https://agentclientprotocol.com/protocol/tool-calls |
| AI-N | https://aider.chat/docs/usage/notifications.html |
| AI-O | https://aider.chat/docs/config/options.html |
| AI-PY | https://pypi.org/project/aider-chat/ |
| AI-S | https://aider.chat/docs/scripting.html |
| AI-BC | https://raw.githubusercontent.com/Aider-AI/aider/main/aider/coders/base_coder.py |
| AM-H | https://ampcode.com/docs/threads |
| AM-A | https://ampcode.com/docs/plugin-api (undated) |
| AM-P | https://ampcode.com/docs/customize/plugins (undated) |
| AM-T | https://ampcode.com/docs/tools (undated) |
| AU | https://docs.augmentcode.com/cli/hooks.md |
| CL-1 | https://raw.githubusercontent.com/cline/cline/901d1b5c97249b3eb6514c503ab009f7864c9bba/docs/customization/hooks.mdx (2026-04 snapshot) |
| CL-2 | https://github.com/cline/cline/commits/main/docs/customization/hooks.mdx |
| CL-3 | https://docs.cline.bot/sdk/plugins.md |
| CL-5 | https://raw.githubusercontent.com/cline/cline/main/sdk/packages/core/src/hooks/hook-file-hooks.ts |
| CL-7 | https://raw.githubusercontent.com/cline/cline/main/CHANGELOG.md |
| CL-T | https://raw.githubusercontent.com/cline/cline/v3.36.0/src/core/task/index.ts |
| CN | https://github.com/continuedev/continue/pull/11029 |
| CR | https://raw.githubusercontent.com/charmbracelet/crush/main/docs/hooks/README.md |
| CU-3P | https://cursor.com/docs/reference/third-party-hooks (undated) |
| CU-B | https://forum.cursor.com/t/subagentstart-hook-parent-conversation-id-always-equals-conversation-id-and-subagent-conversations-have-no-link-back-to-their-parent/163054 |
| CU-CL | https://cursor.com/docs/cli/changelog |
| CU-FT | https://forum.cursor.com/t/richer-agent-transcripts-lifecycle-data-for-observability-langfuse-stop-hooks/166592 (2026-07) |
| CU-F | https://forum.cursor.com/t/cursor-cli-omits-beforesubmitprompt-afteragentresponse-and-stop-hooks-loses-token-usage-and-emits-inconsistent-generation-id-values/169059 (staff reply 2026-08-21) |
| CU-H | https://cursor.com/docs/agent/hooks (undated) |
| CU-O | https://cursor.com/docs/cli/reference/output-format |
| CU-P | https://cursor.com/docs/cli/reference/parameters |
| CX-D | https://learn.chatgpt.com/docs/developer-commands.md?surface=cli |
| CX-H | https://learn.chatgpt.com/docs/hooks.md (undated; developers.openai.com/codex redirects there) |
| CX-L | https://learn.chatgpt.com/docs/changelog |
| CX-MR | https://github.com/openai/codex/pull/55144 (merged 2026-10-18) and https://github.com/openai/codex/blob/main/codex-rs/hooks/src/events/model_request.rs |
| CX-PT | https://github.com/openai/codex/blob/main/codex-rs/hooks/src/events/pre_tool_use.rs |
| CX-RO | https://github.com/openai/codex/blob/main/codex-rs/rollout/src/policy.rs and https://github.com/openai/codex/blob/main/codex-rs/rollout/src/recorder.rs |
| CX-X | https://learn.chatgpt.com/docs/non-interactive-mode |
| DV-C | https://docs.devin.ai/cli/reference/commands |
| DV-H | https://docs.devin.ai/cli/extensibility/hooks/overview |
| DV-LH | https://docs.devin.ai/cli/extensibility/hooks/lifecycle-hooks |
| FD | https://docs.factory.com/harness/hooks.md |
| FD-X | https://docs.factory.com/cli/droid-exec/overview.md |
| GH-C | https://docs.github.com/en/copilot/concepts/agents/hooks |
| GH-CL | https://raw.githubusercontent.com/github/copilot-cli/main/changelog.md |
| GH-CMD | https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference |
| GH-P | https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-programmatic-reference |
| GH-R | https://docs.github.com/en/copilot/reference/hooks-reference (undated) |
| GH-SE | https://docs.github.com/en/copilot/how-tos/copilot-sdk/use-copilot-sdk/streaming-events |
| GM-CR | https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/core/geminiChat.ts and https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/services/chatRecordingService.ts |
| GM-AG | https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/ (2026-05-19) |
| GM-H | https://geminicli.com/docs/hooks/ (2026-04-13) |
| GM-R | https://geminicli.com/docs/hooks/reference.md (undated) |
| GM-REL | https://github.com/google-gemini/gemini-cli/releases/latest |
| GM-S | https://geminicli.com/docs/core/subagents/ (2026-06-08), and the scheduler source on main |
| GM-SH | https://geminicli.com/docs/tools/shell/ (2026-05-01) |
| GM-T | https://geminicli.com/docs/cli/trusted-folders/ (2026-04-23) |
| GO-H | https://goose-docs.ai/docs/guides/context-engineering/hooks/ |
| GO-REL | https://api.github.com/repos/aaif-goose/goose/releases/latest |
| GO-SM | https://raw.githubusercontent.com/aaif-goose/goose/main/crates/goose/src/session/session_manager.rs |
| JU | https://junie.jetbrains.com/docs/junie-cli-hooks.html (2026-10-23) |
| KI | https://kilo.ai/docs/automate/extending/plugins |
| KR | https://kiro.dev/docs/hooks/ (2026-10-21) |
| KR-HL | https://kiro.dev/docs/cli/headless/ |
| KR-M | https://kiro.dev/docs/cli/v3/hooks-migration/ |
| OC-C | https://opencode.ai/docs/cli/ |
| OC-E | https://unpkg.com/@opencode-ai/sdk@1/dist/v2/gen/types.gen.d.ts |
| OC-I | https://github.com/anomalyco/opencode/issues/5894 |
| OC-P | https://opencode.ai/docs/plugins/ (2026-10-24) |
| OC-PL | https://raw.githubusercontent.com/anomalyco/opencode/v1.19.4/packages/opencode/src/plugin/index.ts |
| OC-PR | https://raw.githubusercontent.com/anomalyco/opencode/v1.19.4/packages/opencode/src/session/processor.ts and https://raw.githubusercontent.com/anomalyco/opencode/v1.19.4/packages/schema/src/v1/session.ts |
| OC-S | https://opencode.ai/docs/server/ |
| OC-T | https://unpkg.com/@opencode-ai/plugin@1/dist/index.d.ts |
| OC-V2 | https://opencode.ai/v2/docs/build/plugins |
| QW | https://raw.githubusercontent.com/QwenLM/qwen-code/main/docs/users/features/hooks.md (undated) |
| QW-CR | https://github.com/QwenLM/qwen-code/blob/main/packages/core/src/services/chatRecordingService.ts |
| QW-R | https://github.com/QwenLM/qwen-code/releases |
| RO-1 | https://roocodeinc.github.io/Roo-Code/ |
| RO-2 | https://github.com/RooCodeInc/Roo-Code |
| VS-A | https://code.visualstudio.com/docs/agents/concepts/agent-harnesses (2026-10-21) |
| VS-H | https://code.visualstudio.com/docs/copilot/customization/hooks (2026-10-21, preview) |
| VS-ST | https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/workbench/contrib/chat/common/chatService/chatServiceImpl.ts and https://raw.githubusercontent.com/microsoft/vscode/main/src/vs/platform/storage/common/storage.ts |
| WS-FAQ | https://docs.devin.ai/desktop/devin-desktop-faq |
| WS-H | https://docs.devin.ai/desktop/cascade/hooks (undated) |
| WS-L | https://docs.devin.ai/desktop/devin-local |
| ZD | https://zed.dev/docs/ai/tool-permissions |
