# Verify report, phase 3 (round "generic", trimmed copy: the oracle and spec diffs are left out)

**Verdict: PASS**

- Commit: main after the fix-b4 merge
- Base: spec-base
- Claude Code: 2.1.306 (Claude Code)
- cc checks: by the cc part (the live result, or --contract)
- Run: final-generic-2

## Reused parts

None: every part ran in this run.

## Needs you

Nothing.

## Skipped on this machine

Checks whose subject this machine lacks. They decide nothing and fail no verdict.

None.

## Failures

None.

## Flaky checks

Checks that passed only on a retry. One flaky in 3 runs in a row fails.

None.

## Budgets

None in this phase.

## Coverage

6358 of 6358 changed lines covered by unit or an oracle layer (int, render, host). Lines no oracle layer covers:

The per-line list (unit only, not measured) is left out of this copy.

## Check results

| Check | Layer | Result | Seconds |
|---|---|---|---|
| `actions__should_confirm_naming_the_worktree` | host | pass | 0.0 |
| `actions__should_guard_fg_by_the_holding_slot` | host | pass | 0.0 |
| `actions__should_match_the_row_kind` | render | pass | 0.0 |
| `actions__should_refuse_free_with_a_working_actor` | host | pass | 0.0 |
| `actions__should_warn_free_with_an_idle_actor` | host | pass | 0.0 |
| `agent_working_in_slot` | host+render | pass | 0.0 |
| `agents_off` | host+render | pass | 0.0 |
| `ages__should_tick_without_messages` | render | pass | 0.0 |
| `ages__should_use_the_format` | render | pass | 0.0 |
| `auto_mode_denied` | render | pass | 0.0 |
| `bash_attribution` | host+render | pass | 0.0 |
| `budget__b10` | host | pass | 0.0 |
| `budget__b7` | host+render | pass | 0.0 |
| `building_without_agent` | host+render | pass | 0.0 |
| `bundle__should_contain_no_listen_call` | static | pass | 0.0 |
| `catalog__should_match_the_files` | static | pass | 0.0 |
| `chips__should_count_and_scroll_to_waiting` | render | pass | 0.0 |
| `csp__should_match_the_template` | render | pass | 0.0 |
| `cwd_changed_moves` | host+render | pass | 0.0 |
| `dead_process` | host+render | pass | 0.0 |
| `detail__should_show_every_fact_and_actor` | render | pass | 0.0 |
| `early_exit` | host | pass | 0.0 |
| `fixtures__should_match_a_fresh_scrub` | static | pass | 0.0 |
| `folder_missing` | host+render | pass | 0.0 |
| `foreground_with_agent` | host+render | pass | 0.0 |
| `garbage_line` | host | pass | 0.0 |
| `git_operations` | host+render | pass | 0.0 |
| `harness:self_test` | harness | pass | 2.6 |
| `header__should_show_live_updated_and_paused` | render | pass | 0.0 |
| `header__should_show_paused_before_the_first_document` | render | pass | 0.0 |
| `heading__should_name_the_repository_then_the_machine` | render | pass | 0.0 |
| `hidden_then_shown` | host | pass | 0.0 |
| `hostile_agent_strings` | host+render | pass | 0.0 |
| `hostile_git_strings` | host+render | pass | 0.0 |
| `idle_pool` | host+render | pass | 0.0 |
| `idle_prompt_is_idle` | host+render | pass | 0.0 |
| `interrupt_mid_tool` | render | pass | 0.0 |
| `lint__no_colour_literals` | static | pass | 0.0 |
| `lint__no_html_sinks` | static | pass | 0.0 |
| `lint__no_network_modules` | static | pass | 0.0 |
| `lint__no_socket_apis` | static | pass | 0.0 |
| `lint__no_suppressions` | static | pass | 0.0 |
| `live_region__should_announce_only_waiting` | render | pass | 0.0 |
| `main_waits_for_subagent` | render | pass | 0.0 |
| `manifest__should_be_a_ui_extension` | static | pass | 0.0 |
| `manifest__should_contribute_no_status_bar_item` | host+static | pass | 0.0 |
| `manifest__should_contribute_no_views` | static | pass | 0.0 |
| `manifest__should_scope_wt_path_to_application` | static | pass | 0.0 |
| `messages__should_ignore_everything_outside_the_set` | host | pass | 0.0 |
| `new_session_same_process` | host+render | pass | 0.0 |
| `open__should_reveal_or_create_beside` | host | pass | 0.0 |
| `options__should_limit_roots_and_disable_commands_and_forms` | host | pass | 0.0 |
| `oracle_unchanged` | harness | pass | 0.0 |
| `order__should_not_change_when_states_change` | render | pass | 0.0 |
| `outside_pool` | host+render | pass | 0.0 |
| `oversize_line` | host | pass | 0.0 |
| `parallel_batch_keeps_waiting` | host+render | pass | 0.0 |
| `partial_lines` | host | pass | 0.0 |
| `pathless_events_keep_place` | host+render | pass | 0.0 |
| `permission_denied` | host+render | pass | 0.0 |
| `permission_denied_then_interrupt` | render | pass | 0.0 |
| `permission_request_before_pre` | host+render | pass | 0.0 |
| `permission_wait_and_answer` | host+render | pass | 0.0 |
| `post_before_pre` | host+render | pass | 0.0 |
| `question_and_plan` | host+render | pass | 0.0 |
| `render__should_escape_and_isolate_controls` | render | pass | 0.0 |
| `repo_context__should_be_unchanged` | harness | pass | 0.0 |
| `restore__should_ignore_hostile_state` | render | pass | 0.0 |
| `rows__should_follow_the_grammar` | render | pass | 0.0 |
| `rows__should_show_name_role_and_branch` | render | pass | 0.0 |
| `schema__generated_files_current` | static | pass | 0.0 |
| `session_end_cleanup` | render | pass | 0.0 |
| `several_agents_one_worktree` | host+render | pass | 0.0 |
| `slow_start` | host | pass | 0.0 |
| `spawn__should_run_in_the_workspace_folder` | host | pass | 0.0 |
| `spawn__should_use_the_setting_and_no_shell` | host | pass | 0.0 |
| `stall_30s` | host | pass | 0.0 |
| `static:clippy` | static | pass | 1.6 |
| `static:clippy_all_features` | static | pass | 0.7 |
| `static:doc_links` | static | pass | 0.0 |
| `static:eslint` | static | pass | 0.7 |
| `static:fmt` | static | pass | 0.2 |
| `static:rustdoc` | static | pass | 0.5 |
| `static:stylelint` | static | pass | 0.5 |
| `stop_failure` | host+render | pass | 0.0 |
| `stream__should_accept_unknown_properties` | host | pass | 0.0 |
| `stream__should_post_paused_before_the_first_document` | host | pass | 0.0 |
| `stream__should_spawn_one_watch_and_never_poll` | host | pass | 0.0 |
| `stream__should_survive_2000_documents` | host | pass | 0.0 |
| `subagent_elsewhere` | render | pass | 0.0 |
| `subagent_permission` | host+render | pass | 0.0 |
| `tab__should_show_the_extension_icon` | host | pass | 0.0 |
| `title__should_count_waits_while_hidden` | host | pass | 0.0 |
| `title__should_drop_the_count_when_a_wait_ends` | host | pass | 0.0 |
| `title__should_read_worktrees_without_waits` | host | pass | 0.0 |
| `unknown_git` | host+render | pass | 0.0 |
| `unknown_version` | host | pass | 0.0 |
| `unmanaged_worktree` | host+render | pass | 0.0 |
| `update__should_keep_focus_selection_and_detail` | render | pass | 0.0 |
| `validator__should_accept_every_scenario_document` | host | pass | 0.0 |
| `web:build` | static | pass | 0.0 |
| `web:package` | static | pass | 0.0 |
| `activity__should_follow_the_precedence` | int | pass | 0.0 |
| `agent_working_in_slot` | int | pass | 0.0 |
| `agents_off` | int | pass | 0.0 |
| `auto_mode_denied` | int | pass | 0.0 |
| `bash_attribution` | int | pass | 0.0 |
| `bash_paths__should_match_the_table` | int | pass | 0.0 |
| `budget__b11` | int | pass | 0.0 |
| `budget__b2` | int | pass | 0.0 |
| `budget__b9` | int | pass | 0.0 |
| `building_without_agent` | int | pass | 0.0 |
| `builds__should_match_cargo_star_and_rustc` | int | pass | 0.0 |
| `builds__should_read_the_setting` | int | pass | 0.0 |
| `cc_replay:cc_cd_into_slot` | int | pass | 0.0 |
| `cc_replay:cc_idle_prompt` | int | pass | 0.0 |
| `cc_replay:cc_interrupt` | int | pass | 0.0 |
| `cc_replay:cc_permission` | int | pass | 0.0 |
| `cc_replay:cc_permission_deny` | int | pass | 0.0 |
| `cc_replay:cc_permission_deny_interrupt` | int | pass | 0.0 |
| `cc_replay:cc_read_edit_bash` | int | pass | 0.0 |
| `cc_replay:cc_records_claude_pid` | int | pass | 0.0 |
| `cc_replay:cc_session_end` | int | pass | 0.0 |
| `cc_replay:cc_stop_failure` | int | pass | 0.0 |
| `cc_replay:cc_subagent_elsewhere` | int | pass | 0.0 |
| `cc_replay:cc_subagent_permission` | int | pass | 0.0 |
| `cwd_changed_moves` | int | pass | 0.0 |
| `dead_process` | int | pass | 0.0 |
| `decay__should_match_the_table` | int | pass | 0.0 |
| `folder_missing` | int | pass | 0.0 |
| `foreground_with_agent` | int | pass | 0.0 |
| `free__should_agree_with_wt_bg` | int | pass | 0.0 |
| `gc__should_delete_done_subagents_but_the_newest` | int | pass | 0.0 |
| `gc__should_delete_only_old_dead_or_ended_records` | int | pass | 0.0 |
| `gc__should_delete_the_lock_with_the_record` | int | pass | 0.0 |
| `git_operations` | int | pass | 0.0 |
| `hook__should_cap_its_log` | int | pass | 0.0 |
| `hook__should_exit_within_a_second_when_stdin_stays_open` | int | pass | 0.0 |
| `hook__should_exit_within_a_second_when_the_record_is_a_fifo` | int | pass | 0.0 |
| `hook__should_exit_zero_after_a_panic` | int | pass | 0.0 |
| `hook__should_exit_zero_for_every_fixture` | int | pass | 0.0 |
| `hook__should_ignore_input_past_32_mib` | int | pass | 0.0 |
| `hook__should_keep_records_whole_under_parallel_writers` | int | pass | 0.0 |
| `hook__should_lose_no_update_under_parallel_writers` | int | pass | 0.0 |
| `hook__should_not_create_wt_activity` | int | pass | 0.0 |
| `hook__should_not_follow_a_symlinked_record` | int | pass | 0.0 |
| `hook__should_not_log_into_watched_folders` | int | pass | 0.0 |
| `hook__should_print_nothing_for_every_fixture` | int | pass | 0.0 |
| `hook__should_refuse_a_forged_dot_git_file` | int | pass | 0.0 |
| `hook__should_refuse_a_symlinked_wt_activity` | int | pass | 0.0 |
| `hook__should_refuse_ids_outside_the_pattern` | int | pass | 0.0 |
| `hook__should_restart_when_gc_deleted_the_lock` | int | pass | 0.0 |
| `hook__should_run_no_git` | int | pass | 0.0 |
| `hook__should_skip_a_sandbox_pool_without_the_variable` | int | pass | 0.0 |
| `hook__should_store_no_prompt_text` | int | pass | 0.0 |
| `hook__should_write_nothing_for_malformed_json` | int | pass | 0.0 |
| `hook__should_write_only_inside_wt_activity` | int | pass | 0.0 |
| `hostile_agent_strings` | int | pass | 0.0 |
| `hostile_git_strings` | int | pass | 0.0 |
| `idle_pool` | int | pass | 0.0 |
| `idle_prompt_is_idle` | int | pass | 0.0 |
| `in_flight__should_clear_on_turn_end` | int | pass | 0.0 |
| `in_flight__should_keep_at_most_32` | int | pass | 0.0 |
| `install__should_match_the_event_table` | int | pass | 0.0 |
| `install__should_never_write_settings` | int | pass | 0.0 |
| `install__should_print_only_the_block_on_stdout` | int | pass | 0.0 |
| `install__should_refuse_a_binary_under_target` | int | pass | 0.0 |
| `install__should_report_presence` | int | pass | 0.0 |
| `int:build` | int | pass | 0.8 |
| `int:build_release` | int | pass | 0.0 |
| `interrupt_mid_tool` | int | pass | 0.0 |
| `labels__should_prefer_session_title` | int | pass | 0.0 |
| `labels__should_use_agent_type` | int | pass | 0.0 |
| `last__should_use_the_newest_known_time` | int | pass | 0.0 |
| `liveness__should_keep_the_record_when_reparented` | int | pass | 0.0 |
| `liveness__should_record_the_parent_process` | int | pass | 0.0 |
| `main_waits_for_subagent` | int | pass | 0.0 |
| `merge__should_refuse_an_older_state` | int | pass | 0.0 |
| `new_session_same_process` | int | pass | 0.0 |
| `outside_pool` | int | pass | 0.0 |
| `parallel_batch_keeps_waiting` | int | pass | 0.0 |
| `pathless_events_keep_place` | int | pass | 0.0 |
| `permission_denied` | int | pass | 0.0 |
| `permission_denied_then_interrupt` | int | pass | 0.0 |
| `permission_request_before_pre` | int | pass | 0.0 |
| `permission_wait_and_answer` | int | pass | 0.0 |
| `placement__should_compare_paths_by_component` | int | pass | 0.0 |
| `placement__should_read_the_path_field_of_each_tool` | int | pass | 0.0 |
| `placement__should_use_the_nearest_existing_ancestor` | int | pass | 0.0 |
| `post_before_pre` | int | pass | 0.0 |
| `question_and_plan` | int | pass | 0.0 |
| `release__should_have_no_verify_feature` | int | pass | 0.0 |
| `sanitize__should_strip_controls_bidi_and_zero_width` | int | pass | 0.0 |
| `session_end_cleanup` | int | pass | 0.0 |
| `setup__should_create_wt_activity` | int | pass | 0.0 |
| `several_agents_one_worktree` | int | pass | 0.0 |
| `status__should_ignore_a_record_outside_the_pool` | int | pass | 0.0 |
| `status__should_keep_the_newest_512_records` | int | pass | 0.0 |
| `status__should_leave_every_file_unchanged` | int | pass | 0.0 |
| `status__should_match_the_schema` | int | pass | 0.0 |
| `status__should_never_create_index_lock` | int | pass | 0.0 |
| `status__should_not_start_fsmonitor` | int | pass | 0.0 |
| `status__should_print_one_pool` | int | pass | 0.0 |
| `status__should_report_the_facts_wt_ls_reports` | int | pass | 0.0 |
| `status__should_skip_an_unknown_record_version` | int | pass | 0.0 |
| `status__should_skip_fifos_symlinks_locks_and_large_records` | int | pass | 0.0 |
| `stop_failure` | int | pass | 0.0 |
| `subagent_elsewhere` | int | pass | 0.0 |
| `subagent_permission` | int | pass | 0.0 |
| `summary__should_match_the_table` | int | pass | 0.0 |
| `transitions__should_match_the_event_table` | int | pass | 0.0 |
| `unit` | unit | pass | 28.5 |
| `unknown_git` | int | pass | 0.0 |
| `unmanaged_worktree` | int | pass | 0.0 |
| `version__should_list_version_commit_and_features` | int | pass | 0.0 |
| `wait__should_hash_input_with_sorted_keys` | int | pass | 0.0 |
| `wait__should_wait_on_a_question_still_in_flight` | int | pass | 0.0 |
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
| `watch__should_refresh_git_after_an_edit_or_bash_call` | int | pass | 0.0 |
| `watch__should_see_a_fetch` | int | pass | 0.0 |
| `watch__should_see_head_and_index_after_many_renames` | int | pass | 0.0 |
| `watch__should_see_nested_branch_refs` | int | pass | 0.0 |
| `watch__should_see_packed_refs` | int | pass | 0.0 |
| `watch__should_show_a_missed_change_within_one_heartbeat` | int | pass | 0.0 |
| `watch__should_stay_quiet_on_an_idle_pool` | int | pass | 0.0 |
| `watch__should_turn_quiet_on_time` | int | pass | 0.0 |
| `actions__should_centre_their_glyphs` | shot | pass | 0.0 |
| `actions__should_not_cover_another_column` | shot | pass | 0.0 |
| `agent_working_in_slot` | e2e | pass | 0.0 |
| `agents_off` | e2e | pass | 0.0 |
| `axe__should_report_no_violation` | shot | pass | 0.0 |
| `bash_attribution` | e2e | pass | 0.0 |
| `building_without_agent` | e2e | pass | 0.0 |
| `chips__should_keep_their_outline_in_every_theme` | shot | pass | 0.0 |
| `contrast__should_meet_aa_in_every_theme` | shot | pass | 0.0 |
| `csp__should_report_no_violation` | shot | pass | 0.0 |
| `cwd_changed_moves` | e2e | pass | 0.0 |
| `detail__should_stand_out_from_the_page` | shot | pass | 0.0 |
| `e2e_csp_clean` | e2e | pass | 0.0 |
| `e2e_no_listening_sockets` | e2e | pass | 0.0 |
| `flow__agents_off_note` | e2e | pass | 0.0 |
| `flow__close_stops_watch` | e2e | pass | 0.0 |
| `flow__connection_states` | e2e | pass | 0.0 |
| `flow__copy_path` | e2e | pass | 0.0 |
| `flow__every_activity_shows` | e2e | pass | 0.0 |
| `flow__fg_confirm` | e2e | pass | 0.0 |
| `flow__free_cancel` | e2e | pass | 0.0 |
| `flow__free_confirm` | e2e | pass | 0.0 |
| `flow__free_refused` | e2e | pass | 0.0 |
| `flow__hide_and_show` | e2e | pass | 0.0 |
| `flow__keyboard_only` | e2e | pass | 0.0 |
| `flow__live_update_keeps_place` | e2e | pass | 0.0 |
| `flow__missing_worktree_actions` | e2e | pass | 0.0 |
| `flow__narrow_scroll` | e2e | pass | 0.0 |
| `flow__narrow_window` | e2e | pass | 0.0 |
| `flow__no_folder_open` | e2e | pass | 0.0 |
| `flow__open_again_reveals` | e2e | pass | 0.0 |
| `flow__open_from_palette` | e2e | pass | 0.0 |
| `flow__open_in_new_window` | e2e | pass | 0.0 |
| `flow__open_terminal` | e2e | pass | 0.0 |
| `flow__reduced_motion` | e2e | pass | 0.0 |
| `flow__reload_restores` | e2e | pass | 0.0 |
| `flow__select_and_deselect` | e2e | pass | 0.0 |
| `flow__theme_switch` | e2e | pass | 0.0 |
| `flow__two_windows` | e2e | pass | 0.0 |
| `flow__wait_answered` | e2e | pass | 0.0 |
| `flow__wait_appears` | e2e | pass | 0.0 |
| `flow__waiting_chip_focuses` | e2e | pass | 0.0 |
| `flow__workspace_folder_pool` | e2e | pass | 0.0 |
| `flow__wt_error_shown` | e2e | pass | 0.0 |
| `focus__should_show_every_ring_whole_in_contrast` | shot | pass | 0.0 |
| `folder_missing` | e2e | pass | 0.0 |
| `foreground_with_agent` | e2e | pass | 0.0 |
| `git_operations` | e2e | pass | 0.0 |
| `heading__should_name_the_repository_then_the_machine` | e2e | pass | 0.0 |
| `hook__should_give_each_call_its_kind` | e2e | pass | 0.0 |
| `hook__should_tag_records_with_the_harness` | e2e | pass | 0.0 |
| `hostile_agent_strings` | e2e | pass | 0.0 |
| `hostile_git_strings` | e2e | pass | 0.0 |
| `hover__should_show_in_every_theme` | shot | pass | 0.0 |
| `icons__should_share_one_size` | shot | pass | 0.0 |
| `idle_pool` | e2e | pass | 0.0 |
| `idle_prompt_is_idle` | e2e | pass | 0.0 |
| `keyboard__should_reach_every_row_and_action` | shot | pass | 0.0 |
| `layout__should_clip_no_text` | shot | pass | 0.0 |
| `legend__should_cover_every_activity_value` | shot | pass | 0.0 |
| `motion__should_stop_the_spinner` | shot | pass | 0.0 |
| `new_session_same_process` | e2e | pass | 0.0 |
| `outside_pool` | e2e | pass | 0.0 |
| `parallel_batch_keeps_waiting` | e2e | pass | 0.0 |
| `pathless_events_keep_place` | e2e | pass | 0.0 |
| `permission_denied` | e2e | pass | 0.0 |
| `permission_request_before_pre` | e2e | pass | 0.0 |
| `permission_wait_and_answer` | e2e | pass | 0.0 |
| `post_before_pre` | e2e | pass | 0.0 |
| `question_and_plan` | e2e | pass | 0.0 |
| `restore__should_rebuild_after_reload` | e2e | pass | 0.0 |
| `schema__should_be_published_by_wt_status` | e2e | pass | 0.0 |
| `scroll__should_bring_focus_and_waits_into_view` | shot | pass | 0.0 |
| `scroll__should_scroll_the_table_inside_the_tab` | shot | pass | 0.0 |
| `several_agents_one_worktree` | e2e | pass | 0.0 |
| `shot__should_render_every_theme` | shot | pass | 0.0 |
| `status__should_emit_only_listed_properties` | e2e | pass | 0.0 |
| `status__should_give_each_actor_its_harness` | e2e | pass | 0.0 |
| `status__should_name_the_repository` | e2e | pass | 0.0 |
| `status__should_read_kinds_only` | e2e | pass | 0.0 |
| `stop_failure` | e2e | pass | 0.0 |
| `subagent_permission` | e2e | pass | 0.0 |
| `tags__should_show_whole_after_the_name` | shot | pass | 0.0 |
| `unknown_git` | e2e | pass | 0.0 |
| `unmanaged_worktree` | e2e | pass | 0.0 |
| `web:build` | ui | pass | 0.0 |
| `web:package` | ui | pass | 0.0 |
| `windows__should_run_one_watcher_each` | e2e | pass | 0.0 |
| `budget__b1` | bench | pass | 0.0 |
| `budget__b3` | bench | pass | 0.0 |
| `budget__b4` | bench | pass | 0.0 |
| `budget__b5` | bench | pass | 0.0 |
| `budget__b6` | bench | pass | 0.0 |
| `budget__b8` | bench-chromium | pass | 0.0 |
| `web:build` | bench | pass | 0.0 |
| `cc_cd_into_slot` | cc | pass | 0.0 |
| `cc_idle_prompt` | cc | pass | 0.0 |
| `cc_interrupt` | cc | pass | 0.0 |
| `cc_no_hook_output` | cc | pass | 0.0 |
| `cc_permission` | cc | pass | 0.0 |
| `cc_permission_deny` | cc | pass | 0.0 |
| `cc_permission_deny_interrupt` | cc | pass | 0.0 |
| `cc_read_edit_bash` | cc | pass | 0.0 |
| `cc_records_claude_pid` | cc | pass | 0.0 |
| `cc_session_end` | cc | pass | 0.0 |
| `cc_stop_failure` | cc | pass | 0.0 |
| `cc_subagent_elsewhere` | cc | pass | 0.0 |
| `cc_subagent_permission` | cc | pass | 0.0 |
| `contract__should_run_in_the_sandbox` | cc | pass | 0.0 |
| `cover` | cover | pass | 0.0 |
| `isolation__left_no_trace` | report | pass | 0.0 |
| `flows__should_each_have_an_e2e_check` | report | pass | 0.0 |
| `trace_complete` | report | pass | 0.0 |
