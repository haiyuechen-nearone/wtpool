//! The [`AgentGuard`] of `wtpool free` and `wtpool fg`.

use std::path::Path;

use crate::error::{Error, Refusal};
use crate::status::{self, Actor, ActorState, Sources};
use crate::worktree::Worktree;

use super::Pool;

/// The rule of `free` and `fg` for agents in a worktree they would detach or repoint.
#[derive(Clone, Copy)]
pub enum AgentGuard<'a> {
    /// Refuse while an agent there works, waits or is quiet, as `wtpool status` reads it from
    /// these [`Sources`].
    Refuse(&'a Sources),
    /// Go ahead whatever the agents there do, as `--force` asks.
    Force,
}

impl<F> Pool<F> {
    /// Refuses the first of `worktrees` with a busy agent, unless `guard` is
    /// [`AgentGuard::Force`].
    pub(super) fn refuse_busy_agents(
        &self,
        guard: AgentGuard<'_>,
        common_dir: &Path,
        worktrees: &[&Worktree],
    ) -> Result<(), Error> {
        let AgentGuard::Refuse(sources) = guard else {
            return Ok(());
        };
        let paths: Vec<&Path> = worktrees
            .iter()
            .map(|worktree| worktree.path().as_path())
            .collect();
        let (config, common_dir) = (self.config.clone(), common_dir.to_path_buf());
        let actors = status::actors_at(config, common_dir, sources, &paths, self.git().mode())?;
        for (worktree, actors) in worktrees.iter().zip(actors) {
            let agents: Vec<String> = actors.iter().filter_map(busy).collect();
            if !agents.is_empty() {
                return Err(Refusal::AgentsBusy {
                    worktree: worktree.name(),
                    agents,
                }
                .into());
            }
        }
        Ok(())
    }
}

/// A quiet [`Actor`] may still work, so it counts as busy.
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
