//! The agent guard of `wtpool free` and `wtpool fg`.

use std::path::Path;

use crate::error::Error;
use crate::github::ViewPr;
use crate::status::{self, Actor, ActorState, Sources};
use crate::worktree::Worktree;

use super::Pool;

/// The rule of `free` and `fg` for agents in a worktree they would detach or repoint.
#[derive(Clone, Copy)]
pub enum AgentGuard<'a> {
    /// Refuse while an agent works or waits there, as `wtpool status` shows the activity records
    /// with the clock and process table of these [`Sources`].
    Refuse(&'a Sources),
    /// Go ahead whatever the agents there do, as `--force` asks.
    Force,
}

impl<V: ViewPr> Pool<V> {
    /// Refuses the first of `worktrees` with an agent that works or waits, unless `guard` is
    /// [`AgentGuard::Force`]. `common` is the repository's common git directory.
    pub(super) fn refuse_busy_agents(
        &self,
        guard: AgentGuard<'_>,
        common: &Path,
        worktrees: &[&Worktree],
    ) -> Result<(), Error> {
        let AgentGuard::Refuse(sources) = guard else {
            return Ok(());
        };
        let paths: Vec<&Path> = worktrees
            .iter()
            .map(|worktree| worktree.path().as_path())
            .collect();
        let actors = status::actors_at(self.config.clone(), common.to_path_buf(), sources, &paths)?;
        for (worktree, actors) in worktrees.iter().zip(actors) {
            let agents: Vec<String> = actors.iter().filter_map(busy).collect();
            if !agents.is_empty() {
                return Err(Error::AgentsBusy {
                    worktree: worktree.name(),
                    agents,
                });
            }
        }
        Ok(())
    }
}

/// `<label> (<state>)` of an actor that works or waits, `None` for one that keeps no command
/// out. `quiet` counts, as an agent silent for a while may still work. The key stands in for an
/// empty label.
fn busy(actor: &Actor) -> Option<String> {
    let state = match actor.state {
        ActorState::Working => "working",
        ActorState::Quiet => "quiet",
        ActorState::Waiting => "waiting",
        ActorState::Idle | ActorState::Done => return None,
    };
    let name = if actor.label.is_empty() {
        &actor.key
    } else {
        &actor.label
    };
    Some(format!("{name} ({state})"))
}
