//! The int checks, run in sandbox pools: `wtpool status` and the scenarios of the catalog,
//! `wtpool hook` and its subcommands, `wtpool setup`, agents in `wtpool status`, the agent guard of
//! `wtpool free` and `wtpool fg`, the strings of git the CLI prints, the PR lookup of `wtpool bg`,
//! heavy jobs and `wtpool job`, and the OpenCode adapter beside Claude Code's. Only the bench part
//! runs the Rust budgets of [`bench`].

mod agent_guard;
mod bench;
mod bg_pr;
mod budget_size;
mod cc_identical;
mod cc_model;
mod cli_text;
mod hook_files;
mod hook_gc;
mod hook_idle;
mod hook_io;
mod hook_place;
mod hook_policy;
mod hook_policy_waits;
mod hook_state;
mod hook_strings;
mod hook_tasks;
mod hook_wait;
mod install;
mod install_plugin;
mod jobs_config;
mod jobs_decay;
mod jobs_ls;
mod jobs_queue;
mod jobs_run;
mod jobs_settings;
mod jobs_settings_files;
mod jobs_signals;
mod jobs_status;
mod jobs_suggest;
mod jobs_watch;
mod launched_agents;
mod oc_events;
mod oc_install;
mod oc_policy;
mod oc_records;
mod pr_lookup;
mod scenarios;
mod scenarios_agents;
mod scenarios_jobs;
mod shared_guard;
mod shared_records;
mod shared_status;
mod status;
mod status_agents;
mod status_builds;
mod status_facts;
mod status_files;
mod status_outputs;
mod status_stop_failure;
mod watch;
mod watch_agents;
mod watch_changes;
