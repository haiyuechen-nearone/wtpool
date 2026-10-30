# OpenCode 2.2.0 plugin recordings

These files back the OpenCode adapter's checks ([opencode.md](../../../docs/design/harnesses/opencode.md)
§13). They were recorded on 2026-10-29 with the installed `opencode` 2.2.0 (Homebrew
`opencode-v2`, Bun 1.4.4 inside). A recorder plugin registered every hook and read every event of
the 2.2.0 plugin API. The runs used a sandbox: `HOME` and every `XDG_*` folder in the run folder,
`env -i`, `OPENCODE_DISABLE_CLAUDE_CODE=1`, `OPENCODE_DISABLE_CLAUDE_CODE_SKILLS=1`,
`OPENCODE_DISABLE_AUTOUPDATE=1`, `OPENCODE_DISABLE_MODELS_FETCH=1`, and the background service
turned off.

**The model was scripted.** Every session ran against a local stub, an OpenAI compatible chat
server on loopback, configured as provider `stub` with model `scripted`. Each prompt names a
scenario (`[[scenario:s1]]`), and the stub answers it with fixed tool calls or text, streamed as a
provider streams. Everything OpenCode does around the model is OpenCode 2.2.0's own code path:
hooks, events, permissions, forms, interrupts, titles, subagents and the deny. Token counts, cost,
reasoning events and provider retries come from no real model. No credential was used and no model
cost anything. The prompts are the recorder's own, about the sandbox repository.

## `raw/`

The recording as scrubbed after the run. The run folder reads `/sandbox`, any home `/Users/me`,
the user `me`, and the machine name `host`. A server password reads `<password>`, and any email
`<email>`. The sandbox repository has a main worktree `/sandbox/repo/fg` and the slots
`/sandbox/repo/fg-wt1` and `/sandbox/repo/fg-wt2`.

- `NN-<name>.hooks.jsonl` is the recorder's log of one session. Each line is one JSON object:
  `seq`, `t` (milliseconds since the epoch), `hr`, `pid`, `ppid`, `instance` (the plugin instance:
  one per folder and process), `kind`, `name` and `payload`.
  - `kind` `setup`, `name` `context`: the context of a new instance. Its `location.directory` is
    the instance's folder.
  - `kind` `hook`: a hook call. `payload` is the one object OpenCode passed.
  - `kind` `event`: an event read from `ctx.event.subscribe()`. `payload` is the event.
  - The other kinds are the recorder's own: `register`, `transform`, `hook.done`, `probe`,
    `cleanup` and `event.end`.
  - In `payload`, the string `"<undefined>"` stands for a field that was present but undefined,
    and keys starting with `$` are the recorder's markers (`$class`, `$type`, `$namesOnly`).
    Neither is OpenCode's own. Header values, provider keys and strings over 2 KiB were never
    logged.
- `NN-<name>.stream.jsonl`: the `opencode run --format json` stream of a standalone session.
- `NN-<name>.sse.jsonl`: the server's `/api/event` stream of a session driven over `opencode serve`.
- `NN-<name>.driver.jsonl`: the driver's notes and HTTP calls of a served session.
- `08-deny.model-input.jsonl`: what the stub model received in the deny session.
- `11-process.jsonl`: the process probes (the parent of a spawned process, spawn times).

The served sessions (`00-serve-shutdown`, `03-permission-approve-reject`, `04-question`,
`05-interrupt`, `09-delete-api` and `10-two-folders`) are windows of one `opencode serve`
process. An instance that started in an earlier window has no `setup` line in a later one.
[`instances.json`](instances.json) names the folder of each such instance.

## `forwarded/`

**The frozen oracle** of the shipped plugin: what it must forward from each raw stream, derived
from opencode.md OC4 to OC10 alone, before any plugin code existed.
`forwarded/NN-<name>.jsonl` belongs to `raw/NN-<name>.hooks.jsonl`. It has one line per spawn of
`wtpool hook --harness opencode`, in spawn order:

```json
{"line": 87, "t": 1793301972571, "payload": {"v": 1, "kind": "hook", "name": "tool.execute.before", "directory": "/sandbox/repo/fg", "data": {…}}}
```

- `line` is the 1-based line of the raw stream that caused the spawn.
- `t` is that line's `t`.
- `payload` is the OC6 object the plugin writes on the hook's stdin, compared as a JSON value.

The derivation reads the raw stream as follows:

- **Instances.** Each `instance` of the stream is one plugin instance, started fresh. Its folder,
  `ctx.location.directory`, comes from its `setup` line, else from `instances.json`. A `cleanup`
  line drops that instance's maps.
- **Hooks.** The plugin registers only `tool.execute.before` and `shell.create.before`, so it
  sees no other hook. It forwards `tool.execute.before` and nothing of `shell.create.before`.
- **Objects.** The plugin gets each object with `"<undefined>"` fields absent and the `$` keys
  dropped.
- **The clock.** The plugin's clock at each line is that line's `t`.
- **Once per process (OC5).** A located event, one with `location.directory`, is seen only by the
  instance of that folder. That instance learns the event's session from its row's session field,
  else from `data.sessionID`, whether or not the event is forwarded. `tool.execute.before` teaches
  its `sessionID` too. An event without a location is forwarded only for a session the instance
  learned.
- **Kept fields (OC6).** `data` holds the kept fields of the event's row of §3, nested as in the
  source, and absent fields stay absent.
- **Throttle (OC9).** A delta goes out under its own name, `session.text.delta` for instance, as
  the crate maps it to `progress`. It goes out at once for a session, then the next only once `t`
  is at least 10000 ms past the last one sent for that session. `session.step.started` goes out
  only when its `model.id` and `model.providerID` differ from the last sent for that session.
- **Parent (OC8).** Every hook and event of a child session carries `parent`, the root session.
  `session.created`'s `parentID` makes the child, and a child of a child takes its parent's root.
- **Call (OC8).** Only the replies carry `call`. `permission.replied` takes it from
  `permission.asked`'s `id` and `source.id`, and `form.replied` from `form.created`'s `form.id`
  and `form.metadata.tool.id`. A reply drops its entry.
- **Lookups (OC5).** A session an instance first learns from an event or hook that keeps no
  `title` is looked up with `ctx.session.get({sessionID})`, which answers from
  [`lookups.json`](lookups.json), else with the session's id alone, as OpenCode answers for a
  session with no title yet. An answer with a `title` goes out as kind `lookup`, name
  `session.get`, with its kept fields, after the payload of the line that taught the session.

`00-serve-shutdown` forwards nothing, so its file is empty.

## `lookups.json`

What `ctx.session.get({sessionID})` answers in each stream, by stream and session id, for the
sessions that had a title when the plugin first learned them. It is hand-written, not recorded:
the recorder never called `get`. Its shape is the answer a probe of
`opencode run --standalone --title` got from 2.2.0 on 2026-10-30, in a sandbox and on the scripted
model as above, with this stream's ids, folder and title (`06:1`, `06:69`); the creation and
update times are left out. `run --title` creates the session with no title and renames it at
once. In 6 of 9 runs the plugin got neither `session.created` nor `session.renamed`, as
`06-title-given` shows, and `get` at the session's first event gave the title in each of them;
so it did in 3 of 3 runs with another plugin, named to load first, waiting 400 ms in its `setup`.

## `translate/`

`translate/<case>.json` holds one case of the adapter's translation, judged as
`oc_translate:<case>` in `int` by [`xtask/src/replay/translate.rs`](../../../xtask/src/replay/translate.rs):

- `before`: the record that exists before the payload, or null.
- `others`, when present: more records that exist before, such as a subagent's root session.
  They are not judged.
- `payload`: one OC6 payload, sent to `wtpool hook --harness opencode` from a process `opencode`
  at `2026-10-20T12:00:00Z`.
- `after`: the record after it, or null when the hook writes nothing, so every record file keeps
  its bytes and none is added. Records are compared as JSON values, `writer_started` aside, and
  `"{any}"` matches any value. It stands where the design fixes no value: the input hash
  (`match`), the tool and target a permission or form wait copies, and a subagent's `title`.
- `log`: the number of lines the hook adds to its log (H23).

A record's file is `<harness>-<session_id>[.<agent_id>].json`, or `<session_id>[.<agent_id>].json`
for `claude-code`. `{pool}` names the pool folder. A case marked `"schema": true` has a payload
from the 2.2.0 schema that the scripted model never sent: `reasoning_delta` and `tool_input_delta`.

These files are recorded run output and hand-written cases, with no OpenCode code in them.
