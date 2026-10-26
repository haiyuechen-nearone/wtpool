//! The agent scenarios of the catalog in the `int` layer: hook steps from fake `claude` processes
//! at a fake time, then `wtpool status --json` against every expected document.

use super::scenarios::must_run_scenario;

#[test]
fn agent_working_in_slot() {
    must_run_scenario("agent_working_in_slot");
}

#[test]
fn auto_mode_denied() {
    must_run_scenario("auto_mode_denied");
}

#[test]
fn bash_attribution() {
    must_run_scenario("bash_attribution");
}

#[test]
fn cwd_changed_moves() {
    must_run_scenario("cwd_changed_moves");
}

#[test]
fn dead_process() {
    must_run_scenario("dead_process");
}

#[test]
fn foreground_with_agent() {
    must_run_scenario("foreground_with_agent");
}

#[test]
fn hostile_agent_strings() {
    must_run_scenario("hostile_agent_strings");
}

#[test]
fn idle_prompt_is_idle() {
    must_run_scenario("idle_prompt_is_idle");
}

#[test]
fn interrupt_mid_tool() {
    must_run_scenario("interrupt_mid_tool");
}

#[test]
fn main_waits_for_subagent() {
    must_run_scenario("main_waits_for_subagent");
}

#[test]
fn new_session_same_process() {
    must_run_scenario("new_session_same_process");
}

#[test]
fn outside_pool() {
    must_run_scenario("outside_pool");
}

#[test]
fn parallel_batch_keeps_waiting() {
    must_run_scenario("parallel_batch_keeps_waiting");
}

#[test]
fn pathless_events_keep_place() {
    must_run_scenario("pathless_events_keep_place");
}

#[test]
fn permission_denied() {
    must_run_scenario("permission_denied");
}

#[test]
fn permission_denied_then_interrupt() {
    must_run_scenario("permission_denied_then_interrupt");
}

#[test]
fn permission_request_before_pre() {
    must_run_scenario("permission_request_before_pre");
}

#[test]
fn permission_wait_and_answer() {
    must_run_scenario("permission_wait_and_answer");
}

#[test]
fn post_before_pre() {
    must_run_scenario("post_before_pre");
}

#[test]
fn question_and_plan() {
    must_run_scenario("question_and_plan");
}

#[test]
fn session_end_cleanup() {
    must_run_scenario("session_end_cleanup");
}

#[test]
fn several_agents_one_worktree() {
    must_run_scenario("several_agents_one_worktree");
}

#[test]
fn stop_failure() {
    must_run_scenario("stop_failure");
}

#[test]
fn subagent_elsewhere() {
    must_run_scenario("subagent_elsewhere");
}

#[test]
fn subagent_permission() {
    must_run_scenario("subagent_permission");
}
