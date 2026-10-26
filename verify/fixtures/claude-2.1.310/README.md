# Claude Code 2.1.310 hook recordings

Each `.jsonl` file holds the hook payloads, one JSON object per line, that Claude Code 2.1.310 (by
Anthropic) sent during one scripted session of the live contract test (verification.md, "Contract
test"). They were recorded on 2026-10-26 by `cargo xtask verify --part cc --contract --record`: the
cc layer ran the installed `claude` through `@anthropic-ai/claude-agent-sdk` on a sandbox pool,
with the prompts of `vscode/test/cc/sessions.mjs`, and the recorder hook
`vscode/test/cc/recorder.mjs` appended each payload as read. The raw recordings stay in
`recordings/claude-2.1.310/` of the harness data folder (`$XDG_DATA_HOME/wtpool-verify`, by default
`~/.local/share/wtpool-verify`) and are hashed in `verify/oracle.lock`.
`cargo xtask scrub` (`xtask/src/scrub.rs`) wrote these copies from them: it drops the header line,
writes `/Users/verify/.claude` for Claude Code's configuration folder, which the run keeps in its
run folder, `{pool}` for the pool folder, `/Users/verify` for the home folder, `{tmp}` for the
system's temporary folder and `{tmp}/claude` for Claude Code's per account one
(`/tmp/claude-<uid>`), and renames session, agent and tool use ids in order (`s1`, `a1`,
`toolu_01`); `fixtures__should_match_a_fresh_scrub` redoes it. The files are run output, with no
Claude Code code in them; see [THIRD-PARTY-NOTICES.md](../../../THIRD-PARTY-NOTICES.md).
