# Verify report, phase 1

**Verdict: PASS**

- Commit: the phase 1 run 4 head
- spec-base: spec-base
- Claude Code: 2.1.306 (Claude Code)
- Run: phase1-r4

## Needs you

Nothing.

## Oracle changes since the freeze

```
none
```

## verify/accepted.txt since the freeze

```
none
```

## Spec changes since spec-base

```
 docs/design/status-panel/status-panel.md |   8 +-
 docs/design/status-panel/verification.md | 136 ++++++++++++++++++++-----------
 2 files changed, 94 insertions(+), 50 deletions(-)
```

## Failures

None.

## Flaky checks

Checks that passed only on a retry. One flaky in 3 runs in a row fails.

None.

## Budgets

None in this phase.

## Coverage

2567 of 2567 changed lines covered by unit or an oracle layer (int, render, host). Lines no oracle layer covers:

The per-line list (2744 lines: 513 unit only, 2231 not measured) was dropped from this copy for size.

## Mutants

251 mutants in the diff since spec-base: 207 caught, 0 missed, 0 timeouts (caught), 44 unviable. Surviving mutants:

None.


## Check results

| Check | Layer | Result | Seconds |
|---|---|---|---|
| `actions__should_match_the_row_kind` | render | pass | 0.0 |
| `agents_off` | host+render | pass | 0.0 |
| `ages__should_tick_without_messages` | render | pass | 0.0 |
| `ages__should_use_the_format` | render | pass | 0.0 |
| `budget__b10` | host | pass | 0.0 |
| `budget__b7` | host+render | pass | 0.0 |
| `building_without_agent` | host+render | pass | 0.0 |
| `bundle__should_contain_no_listen_call` | static | pass | 0.0 |
| `catalog__should_match_the_files` | static | pass | 0.0 |
| `chips__should_count_and_scroll_to_waiting` | render | pass | 0.0 |
| `csp__should_match_the_template` | render | pass | 0.0 |
| `detail__should_show_every_fact_and_actor` | render | pass | 0.0 |
| `early_exit` | host | pass | 0.0 |
| `folder_missing` | host+render | pass | 0.0 |
| `garbage_line` | host | pass | 0.0 |
| `git_operations` | host+render | pass | 0.0 |
| `harness:self_test` | harness | pass | 7.1 |
| `header__should_show_live_updated_and_paused` | render | pass | 0.0 |
| `header__should_show_paused_before_the_first_document` | render | pass | 0.0 |
| `hidden_then_shown` | host | pass | 0.0 |
| `hostile_git_strings` | host+render | pass | 0.0 |
| `idle_pool` | host+render | pass | 0.0 |
| `lint__no_colour_literals` | static | pass | 0.0 |
| `lint__no_html_sinks` | static | pass | 0.0 |
| `lint__no_network_modules` | static | pass | 0.0 |
| `lint__no_socket_apis` | static | pass | 0.0 |
| `lint__no_suppressions` | static | pass | 0.0 |
| `live_region__should_announce_only_waiting` | render | pass | 0.0 |
| `manifest__should_be_a_ui_extension` | static | pass | 0.0 |
| `manifest__should_contribute_no_status_bar_item` | host+static | pass | 0.0 |
| `manifest__should_contribute_no_views` | static | pass | 0.0 |
| `manifest__should_scope_wt_path_to_application` | static | pass | 0.0 |
| `messages__should_ignore_everything_outside_the_set` | host | pass | 0.0 |
| `open__should_reveal_or_create_beside` | host | pass | 0.0 |
| `options__should_limit_roots_and_disable_commands_and_forms` | host | pass | 0.0 |
| `oracle_unchanged` | harness | pass | 0.0 |
| `order__should_not_change_when_states_change` | render | pass | 0.0 |
| `oversize_line` | host | pass | 0.0 |
| `partial_lines` | host | pass | 0.0 |
| `render__should_escape_and_isolate_controls` | render | pass | 0.0 |
| `repo_context__should_be_unchanged` | harness | pass | 0.0 |
| `restore__should_ignore_hostile_state` | render | pass | 0.0 |
| `rows__should_follow_the_grammar` | render | pass | 0.0 |
| `rows__should_show_name_role_and_branch` | render | pass | 0.0 |
| `schema__generated_files_current` | static | pass | 0.0 |
| `slow_start` | host | pass | 0.0 |
| `spawn__should_use_the_setting_and_no_shell` | host | pass | 0.0 |
| `stall_30s` | host | pass | 0.0 |
| `static:clippy` | static | pass | 0.9 |
| `static:clippy_all_features` | static | pass | 0.6 |
| `static:doc_links` | static | pass | 0.0 |
| `static:eslint` | static | pass | 0.9 |
| `static:fmt` | static | pass | 0.1 |
| `static:rustdoc` | static | pass | 0.4 |
| `static:stylelint` | static | pass | 0.6 |
| `stream__should_post_paused_before_the_first_document` | host | pass | 0.0 |
| `stream__should_spawn_one_watch_and_never_poll` | host | pass | 0.0 |
| `stream__should_survive_2000_documents` | host | pass | 0.0 |
| `unknown_git` | host+render | pass | 0.0 |
| `unknown_version` | host | pass | 0.0 |
| `unmanaged_worktree` | host+render | pass | 0.0 |
| `update__should_keep_focus_selection_and_detail` | render | pass | 0.0 |
| `validator__should_accept_every_scenario_document` | host | pass | 0.0 |
| `web:build` | static | pass | 1.0 |
| `web:package` | static | pass | 1.6 |
| `agents_off` | int | pass | 0.0 |
| `building_without_agent` | int | pass | 0.0 |
| `builds__should_match_cargo_star_and_rustc` | int | pass | 0.0 |
| `builds__should_read_the_setting` | int | pass | 0.0 |
| `folder_missing` | int | pass | 0.0 |
| `free__should_agree_with_wt_bg` | int | pass | 0.0 |
| `git_operations` | int | pass | 0.0 |
| `hostile_git_strings` | int | pass | 0.0 |
| `idle_pool` | int | pass | 0.0 |
| `int:build` | int | pass | 0.8 |
| `int:build_release` | int | pass | 3.0 |
| `last__should_use_the_newest_known_time` | int | pass | 0.0 |
| `release__should_have_no_verify_feature` | int | pass | 0.0 |
| `status__should_leave_every_file_unchanged` | int | pass | 0.0 |
| `status__should_match_the_schema` | int | pass | 0.0 |
| `status__should_never_create_index_lock` | int | pass | 0.0 |
| `status__should_not_start_fsmonitor` | int | pass | 0.0 |
| `status__should_print_one_pool` | int | pass | 0.0 |
| `status__should_report_the_facts_wt_ls_reports` | int | pass | 0.0 |
| `unit` | unit | pass | 25.3 |
| `unknown_git` | int | pass | 0.0 |
| `unmanaged_worktree` | int | pass | 0.0 |
| `version__should_list_version_commit_and_features` | int | pass | 0.0 |
| `watch__should_exit_on_stdin_eof` | int | pass | 0.0 |
| `watch__should_exit_zero_on_a_closed_pipe` | int | pass | 0.0 |
| `watch__should_ignore_lock_and_temp_files` | int | pass | 0.0 |
| `watch__should_merge_a_burst_into_one_document` | int | pass | 0.0 |
| `watch__should_print_a_document_at_once` | int | pass | 0.0 |
| `watch__should_print_alive_lines` | int | pass | 0.0 |
| `watch__should_print_only_changes` | int | pass | 0.0 |
| `watch__should_read_only_the_changed_record` | int | pass | 0.0 |
| `watch__should_recompute_git_after_a_change` | int | pass | 0.0 |
| `watch__should_recompute_on_rescan` | int | pass | 0.0 |
| `watch__should_see_a_fetch` | int | pass | 0.0 |
| `watch__should_see_head_and_index_after_many_renames` | int | pass | 0.0 |
| `watch__should_see_nested_branch_refs` | int | pass | 0.0 |
| `watch__should_see_packed_refs` | int | pass | 0.0 |
| `watch__should_show_a_missed_change_within_one_heartbeat` | int | pass | 0.0 |
| `watch__should_stay_quiet_on_an_idle_pool` | int | pass | 0.0 |
| `agents_off` | e2e | pass | 0.0 |
| `axe__should_report_no_violation` | shot | pass | 0.0 |
| `building_without_agent` | e2e | pass | 0.0 |
| `csp__should_report_no_violation` | shot | pass | 0.0 |
| `e2e_csp_clean` | e2e | pass | 0.0 |
| `e2e_no_listening_sockets` | e2e | pass | 0.0 |
| `flow__agents_off_note` | e2e | pass | 0.0 |
| `flow__close_stops_watch` | e2e | pass | 0.0 |
| `flow__connection_states` | e2e | pass | 0.0 |
| `flow__copy_path` | e2e | pass | 0.0 |
| `flow__every_activity_shows` | e2e | pass | 0.0 |
| `flow__hide_and_show` | e2e | pass | 0.0 |
| `flow__keyboard_only` | e2e | pass | 0.0 |
| `flow__live_update_keeps_place` | e2e | pass | 0.0 |
| `flow__missing_worktree_actions` | e2e | pass | 0.0 |
| `flow__narrow_window` | e2e | pass | 0.0 |
| `flow__open_again_reveals` | e2e | pass | 0.0 |
| `flow__open_from_palette` | e2e | pass | 0.0 |
| `flow__open_in_new_window` | e2e | pass | 0.0 |
| `flow__open_terminal` | e2e | pass | 0.0 |
| `flow__reduced_motion` | e2e | pass | 0.0 |
| `flow__reload_restores` | e2e | pass | 0.0 |
| `flow__select_and_deselect` | e2e | pass | 0.0 |
| `flow__theme_switch` | e2e | pass | 0.0 |
| `flow__two_windows` | e2e | pass | 0.0 |
| `folder_missing` | e2e | pass | 0.0 |
| `git_operations` | e2e | pass | 0.0 |
| `hostile_git_strings` | e2e | pass | 0.0 |
| `idle_pool` | e2e | pass | 0.0 |
| `keyboard__should_reach_every_row_and_action` | shot | pass | 0.0 |
| `legend__should_cover_every_activity_value` | shot | pass | 0.0 |
| `motion__should_stop_the_spinner` | shot | pass | 0.0 |
| `restore__should_rebuild_after_reload` | e2e | pass | 0.0 |
| `shot__should_render_every_theme` | shot | pass | 0.0 |
| `unknown_git` | e2e | pass | 0.0 |
| `unmanaged_worktree` | e2e | pass | 0.0 |
| `web:build` | static | pass | 0.7 |
| `web:package` | static | pass | 1.4 |
| `windows__should_run_one_watcher_each` | e2e | pass | 0.0 |
| `budget__b3` | bench | pass | 0.0 |
| `budget__b4` | bench | pass | 0.0 |
| `budget__b5` | bench | pass | 0.0 |
| `budget__b8` | bench-chromium | pass | 0.0 |
| `web:build` | static | pass | 0.7 |
| `mutants__should_leave_no_survivor` | mutants | pass | 2297.9 |
| `cover` | cover | pass | 0.0 |
| `isolation__left_no_trace` | report | pass | 0.0 |
| `flows__should_each_have_an_e2e_check` | report | pass | 0.0 |
| `trace_complete` | report | pass | 0.0 |
