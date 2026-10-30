//! The [`AgentGuard`] of `wtpool free` and `wtpool fg`.

use std::path::Path;

use crate::error::{Error, Refusal};
use crate::processes::ListProcesses;
use crate::status::{self, Actor, ActorState, Caller, Sources};
use crate::worktree::Worktree;

use super::Pool;

/// The rule of `free` and `fg` for agents in a worktree they would detach or repoint.
#[derive(Clone, Copy)]
pub enum AgentGuard<'a> {
    /// Refuse while an agent there works, waits or is quiet, as `wtpool status` reads it from
    /// these [`Sources`], unless it belongs to the session that runs the command.
    Refuse(&'a Sources),
    /// Go ahead whatever the agents there do, as `--force` asks.
    Force,
}

impl<F> Pool<F> {
    /// Refuses the first of `worktrees` with a busy agent of another session, unless `guard` is
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
        let git_mode = self.git().mode();
        // A session may repoint or detach the worktree it works in, so the actors of the session
        // that runs this command, found through its ancestors, are left out.
        let ancestors = sources.processes.ancestors();
        let caller = Caller {
            ancestors: &ancestors,
            named_session: sources.named_session.as_deref(),
        };
        let actors =
            status::actors_at(&self.config, common_dir, sources, &paths, git_mode, &caller)?;
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
