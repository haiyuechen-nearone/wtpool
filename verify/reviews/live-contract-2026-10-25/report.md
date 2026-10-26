# Live contract run, 2026-10-25 (review item N11)

`cargo xtask verify --phase 3 --part cc --contract --run live-contract`, once, on main after the H22 docs fix (includes the
install fix), Claude Code 2.1.306, model claude-haiku-5-5, sessions 07:43-07:51 CEST. Locked
recordings untouched (no `--record`).

**Result: all 14 cc checks pass; wt wrote exactly one record in each of the 12 pools**, each ending
`ended` with `process.pid` = that session's claude. The hook log stayed empty.

| Session | Claude Code asserts | wt asserts |
|---|---|---|
| contract__should_run_in_the_sandbox | only the run's settings, no canary, auto mode, 2.1.306 | n/a |
| cc_read_edit_bash | H8 path field on Pre/PostToolUse | wt1 `idle` after Stop |
| cc_subagent_elsewhere | agent_id, agent_type Explore | a1 on wt2 |
| cc_subagent_permission | PermissionRequest has agent_id | only the subagent waits |
| cc_cd_into_slot | - | s1 on wt2 |
| cc_permission | PermissionRequest during the 70 s hold | `waiting` in every sample |
| cc_permission_deny | a PostToolBatch lists the denied call | not `waiting` after the batch |
| cc_permission_deny_interrupt | no Stop | still `waiting` (no idle_prompt in 70 s; N10) |
| cc_interrupt | no PermissionRequest, no Stop within 10 s | `quiet` 125 s after |
| cc_idle_prompt | - | never `waiting`, `idle` at end |
| cc_stop_failure | StopFailure | `idle` with error model_not_found |
| cc_session_end | SessionEnd | record `ended`, no subagent record left |
| cc_no_hook_output | n/a | no hook output |
| cc_records_claude_pid | hooks' parent is claude | records hold that pid |

The earlier "wt wrote no record in any session" came from the phase 2 oracle step, whose build
predates `wt hook` (the first hook commit is not an ancestor of the first phase 2 freeze): expected, not a product bug.

Payload key sets are unchanged from the locked recordings; only timing order (cc_cd_into_slot,
cc_session_end), one fewer PostToolBatch in cc_read_edit_bash, and per-run values differ. Adopting
this run would rewrite all 11 fixtures (prompt_id is not renamed by the scrubber), so the locked
fixtures stay. Oracle quirk noted, not fixed: the scrubber names agent ids in order of appearance,
so the committed cc_session_end fixture keeps a raw agent id in tool_response.

Details: cc-results.json, part-cc.json, run-output.txt, recordings-vs-backup.txt, fixtures-diff.txt,
fresh-scrub/.
