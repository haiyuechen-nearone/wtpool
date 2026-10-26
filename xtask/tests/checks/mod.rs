//! The int checks: `wtpool status` and the scenarios of the catalog, run in sandbox pools (phase 1),
//! `wtpool hook`, `wtpool hook gc`, `wtpool hook install`, `wtpool setup` and agents in `wtpool status` (phase 2),
//! the agent guard of `wtpool free` and `wtpool fg`, the strings of git the CLI prints, the PR
//! lookup of `wtpool bg` and the PR branches it fetches and refuses, heavy jobs, `wtpool job` and
//! `wtpool hook policy` (phase 4, the `jobs_` and `hook_policy` modules), and the Rust budgets,
//! which only the bench part runs (`checks::bench`).

mod agent_guard;
mod bench;
mod bg_pr;
mod budget_size;
mod cli_text;
mod hook_files;
mod hook_gc;
mod hook_io;
mod hook_place;
mod hook_policy;
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
mod pr_lookup;
mod scenarios;
mod scenarios_agents;
mod scenarios_jobs;
mod status;
mod status_agents;
mod status_builds;
mod status_facts;
mod status_files;
mod status_outputs;
mod watch;
mod watch_agents;
mod watch_changes;
