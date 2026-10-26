//! One hook event end to end: where it belongs (H6, H7), what it changes (H8 to H14), the
//! records it writes under their locks (H18 to H20, H24), and after a session's end the day's gc
//! (H21). A harness's adapter turns its input into the [`HookInput`] this works on.

use std::path::{Path, PathBuf};

use super::adapter::Adapter;
use super::apply::{self, Change, Titles};
use super::bash;
use super::env::{HookEnv, Me};
use super::gc;
use super::input::{CallFacts, Event, HookInput};
use super::log;
use super::place::{self, Shell, ToolPath, Worktree};
use super::record::{Record, SubagentTitle};
use super::store::Activity;
use super::tasks::{self, Started, TaskEvent};
use super::text::{self, MAX_LABEL, MAX_TEXT};
use crate::activity::{ActorKey, PlacedBy, State};
use crate::clock::{ReadClock, Timestamp};
use crate::paths::comparable;

/// One event's facts that need the file system, worked out before any lock is taken.
struct Context<'a, A> {
    adapter: &'a A,
    input: &'a HookInput,
    env: &'a HookEnv,
    activity: Activity,
    /// The actor the event is of.
    key: ActorKey,
    cwd: PathBuf,
    cwd_worktree: Worktree,
    me: Me,
    now: String,
    now_time: Timestamp,
}

/// Handles one raw hook input of the harness `adapter` reads. Malformed input, an id outside
/// its pattern, a folder outside every pool or a pool without a usable `wtpool-activity/` writes
/// nothing.
pub fn handle<A: Adapter>(adapter: &A, input: &[u8], env: &HookEnv) {
    if let Some(input) = adapter.parse(input) {
        handle_input(adapter, &input, env);
    }
}

fn handle_input<A: Adapter>(adapter: &A, input: &HookInput, env: &HookEnv) {
    let cwd = PathBuf::from(&input.cwd);
    if !cwd.is_absolute() {
        return;
    }
    let Some(cwd_worktree) = place::worktree_of(&cwd) else {
        return;
    };
    let Some(activity) = Activity::open(&cwd_worktree.common) else {
        return;
    };
    if activity.is_sandbox() && !env.sandbox {
        return;
    }
    let now = env.clock.now();
    let context = Context {
        adapter,
        input,
        env,
        activity,
        key: ActorKey {
            harness: A::HARNESS,
            session: input.session_id.clone(),
            agent: input.agent_id.clone(),
        },
        cwd,
        cwd_worktree,
        me: env.me(),
        now: now.to_rfc3339(),
        now_time: now,
    };
    if context.run(&input.event).is_some() && input.event == Event::SessionEnd {
        let deadline = env.entered_micros.saturating_add(gc::HOOK_BUDGET_MICROS);
        let _ = gc::after_session_end(&context.activity, now, &env.processes, deadline);
    }
}

impl<A: Adapter> Context<'_, A> {
    fn run(&self, event: &Event) -> Option<()> {
        let stem = self.key.stem();
        if let Event::Notified(_) = event
            && self.input.agent_id.is_none()
            && self.subagent_waits()
        {
            return None;
        }
        let lock = self
            .activity
            .lock(&stem)
            .or_else(|| self.skipped("its record's lock cannot be taken"))?;
        let current = self.activity.read(&stem);
        if let Event::Notified(_) = event
            && current.as_ref().is_some_and(|r| r.waiting.is_some())
        {
            return None;
        }
        let (event, moved_to) = self.located(event, current.as_ref());
        let change = self.change(current.as_ref(), moved_to);
        let applied = apply::apply(current.as_ref(), &event, &change);
        let mut result = match &current {
            Some(current) => {
                apply::settle(current, applied, self.me.started_micros, A::WAITING_TOOLS)
            }
            None => applied,
        };
        tasks::update(&mut result, &self.task_event(&event), &self.now);
        if let Err(error) = self.activity.write(&stem, &result) {
            return self.skipped(&format!("its record cannot be written: {error}"));
        }
        drop(lock);
        self.end_session_tasks(&stem);
        self.side_effects(&event, &result, &change);
        Some(())
    }

    /// `None`, once the hook's log notes that the event wrote nothing, for `reason` (H23). The
    /// hook still prints nothing and exits 0 (H2, H3).
    fn skipped<T>(&self, reason: &str) -> Option<T> {
        if let Some(dir) = &self.env.log {
            log::append(dir, &format!("wtpool hook: skipped an event: {reason}"));
        }
        None
    }

    /// The list of work in flight a turn's end sends covers its whole session (H27):
    /// it also ends the old running tasks it leaves out in the session's other records that name
    /// this event's harness process (H19), each under its own lock, and changes nothing else in
    /// them. Without the list, or without a known parent process, nothing more ends.
    fn end_session_tasks(&self, own: &str) {
        let (Some(running), Some(process)) = (&self.input.tasks_running, &self.me.parent) else {
            return;
        };
        let others = self
            .activity
            .session_stems(&self.key)
            .into_iter()
            .filter(|stem| stem != own);
        for stem in others {
            let Some(lock) = self.activity.lock(&stem) else {
                continue;
            };
            if let Some(mut record) = self.activity.read(&stem)
                && record.process.as_ref() == Some(process)
                && tasks::end_unlisted(&mut record, running, &self.now)
            {
                let _ = self.activity.write(&stem, &record);
            }
            drop(lock);
        }
    }

    /// What the event says of the actor's background tasks (H27): a task its call started,
    /// in the output file the harness's adapter names, and the tasks it ends.
    fn task_event(&self, event: &Event) -> TaskEvent {
        let started = match (event, &self.input.task_start) {
            (Event::CallEnd(call), Some(task)) => self
                .adapter
                .task_file(self.input, task, self.activity.owner().unwrap_or(0))
                .map(|path| Started {
                    id: task.clone(),
                    path,
                    summary: call.target.clone(),
                }),
            _ => None,
        };
        TaskEvent {
            started,
            running: self.input.tasks_running.clone(),
            stopped: self.input.task_stopped.clone(),
            all_ended: *event == Event::SessionEnd,
        }
    }

    /// Whether a subagent of the session waits, so an [`Event::Notified`] leaves the session be.
    fn subagent_waits(&self) -> bool {
        self.activity
            .subagents_of(&self.key)
            .iter()
            .filter_map(|stem| self.activity.read(stem))
            .any(|record| record.state == State::Waiting)
    }

    /// The event with its call targets, and the worktree its path names (H8 to H10, H14).
    fn located(&self, event: &Event, current: Option<&Record>) -> (Event, Option<String>) {
        let base = current.map_or_else(
            || self.cwd_worktree.root.clone(),
            |record| PathBuf::from(&record.worktree),
        );
        match event {
            Event::CallStart(call) | Event::PermissionAsked(call) | Event::CallEnd(call) => {
                let (target, moved) = self.tool_place(&base);
                let call = CallFacts {
                    target,
                    ..call.clone()
                };
                let event = match event {
                    Event::CallStart(_) => Event::CallStart(call),
                    Event::PermissionAsked(_) => Event::PermissionAsked(call),
                    _ => Event::CallEnd(call),
                };
                (event, moved)
            }
            Event::CwdChange => (event.clone(), self.cwd_change(current)),
            other => (other.clone(), None),
        }
    }

    /// The target of the event's tool and the worktree its path names, if in this pool.
    fn tool_place(&self, base: &Path) -> (String, Option<String>) {
        match &self.input.tool_path {
            ToolPath::File(text) => {
                let path = place::absolute(&self.cwd, text);
                let found = self.in_pool(&path);
                let root = found.as_ref().map_or(base, |w| w.root.as_path());
                let target = file_target(text, &path, root);
                (target, found.map(|w| path_text(&w.root)))
            }
            ToolPath::Command { command, shell } => {
                let parsed = match shell {
                    Shell::Posix => bash::parse(command, &self.cwd, self.env.home.as_deref()),
                };
                let moved = self.command_place(&parsed);
                (text::clean(&parsed.summary, MAX_TEXT), moved)
            }
            ToolPath::None => (String::new(), None),
        }
    }

    /// The last `cd` wins, else a single worktree among the path words, else none (H9).
    fn command_place(&self, parsed: &bash::Command) -> Option<String> {
        if let Some(last) = parsed.cds.last() {
            return self.in_pool(last).map(|w| path_text(&w.root));
        }
        let mut found: Vec<PathBuf> = parsed
            .paths
            .iter()
            .filter_map(|path| self.in_pool(path))
            .map(|w| w.root)
            .collect();
        found.sort();
        found.dedup();
        match found.as_slice() {
            [only] => Some(path_text(only)),
            _ => None,
        }
    }

    /// The worktree of `path` when it belongs to this event's repository.
    fn in_pool(&self, path: &Path) -> Option<Worktree> {
        place::worktree_of(path).filter(|w| w.common == self.cwd_worktree.common)
    }

    /// An [`Event::CwdChange`] moves the actor, unless back to the session's first folder (H10).
    fn cwd_change(&self, current: Option<&Record>) -> Option<String> {
        let new_cwd = place::local(Path::new(self.input.new_cwd.as_deref()?))?;
        let first = current
            .and_then(|r| r.first_cwd.as_deref())
            .map_or_else(|| self.cwd.clone(), PathBuf::from);
        if place::is_local(&first)
            && place::same_components(&comparable(new_cwd), &comparable(&first))
        {
            return None;
        }
        self.in_pool(new_cwd).map(|w| path_text(&w.root))
    }

    fn change(&self, current: Option<&Record>, moved_to: Option<String>) -> Change {
        let agent = self
            .input
            .agent_id
            .as_ref()
            .map(|id| (id.as_str().to_owned(), self.agent_type()));
        let label = agent.as_ref().and_then(|(_, kind)| kind.clone());
        let (titles, transcript_offset) = match &agent {
            Some(_) => (Titles::default(), None),
            None => self.titles(current),
        };
        let subagent = self.input.spawned.as_ref().and_then(|spawned| {
            Some(SubagentTitle {
                agent_id: spawned.agent_id.as_str().to_owned(),
                title: label_text(&spawned.title)?,
            })
        });
        let new_label = match &agent {
            Some((_, kind)) => kind.clone().unwrap_or_else(|| "subagent".to_owned()),
            None => text::session_label(self.now_time, self.env.tz.as_deref()),
        };
        let first_place = match (&agent, current) {
            (Some(_), None) => self.parent_place(),
            _ => self.cwd_place(),
        };
        Change {
            harness: A::HARNESS,
            waiting_tools: A::WAITING_TOOLS,
            now: self.now.clone(),
            started_micros: self.me.started_micros,
            process: self.me.parent.clone(),
            first_place,
            first_cwd: self.input.cwd.clone(),
            moved_to,
            new_label,
            label,
            titles,
            transcript_offset,
            subagent,
            session_id: self.input.session_id.as_str().to_owned(),
            agent,
        }
    }

    fn agent_type(&self) -> Option<String> {
        self.input.agent_type.as_deref().and_then(label_text)
    }

    /// The titles a session's event finds: the one the user gave in its payload, else in the
    /// new part of its transcript, and the newest generated one there, with how far the
    /// transcript is read now (H16). The read starts where the record's last one ended.
    fn titles(&self, current: Option<&Record>) -> (Titles, Option<u64>) {
        let offset = current.map_or(0, |record| record.transcript_offset);
        let read = self
            .input
            .transcript
            .as_deref()
            .map(Path::new)
            .filter(|path| path.is_absolute())
            .and_then(|path| self.adapter.titles(path, offset));
        let found = read.clone().unwrap_or_default();
        let given = self.input.session_title.as_deref().and_then(label_text);
        let titles = Titles {
            user: given.or_else(|| found.user.as_deref().and_then(label_text)),
            generated: found.generated.as_deref().and_then(label_text),
        };
        (titles, read.map(|read| read.offset))
    }

    /// A new subagent starts at its session's worktree with the session's `placed_by` (H10).
    fn parent_place(&self) -> (String, PlacedBy) {
        match self.activity.read(&self.key.session_key().stem()) {
            Some(parent) => (parent.worktree, parent.placed_by),
            None => self.cwd_place(),
        }
    }

    fn cwd_place(&self) -> (String, PlacedBy) {
        (path_text(&self.cwd_worktree.root), PlacedBy::Cwd)
    }

    /// What an event does to other records: [`Event::Idle`] finishes the subagents, the session's
    /// end deletes them (H20). That a newer main session in one process ends the older ones (H19)
    /// is read from the records by `wtpool status`; gc keeps both until they end or die.
    fn side_effects(&self, event: &Event, result: &Record, change: &Change) {
        if result.is_subagent() {
            return;
        }
        match event {
            Event::Idle => {
                for stem in self.activity.subagents_of(&self.key) {
                    let _ = self.update(&stem, |record| apply::finish_subagent(record, change));
                }
            }
            Event::SessionEnd => {
                for stem in self.activity.subagents_of(&self.key) {
                    if let Some(lock) = self.activity.lock(&stem) {
                        self.activity.delete(&stem, lock);
                    }
                }
            }
            _ => {}
        }
    }

    /// Rewrites the record `stem` under its lock, keeping a state a later hook wrote (H24).
    fn update(&self, stem: &str, change: impl Fn(&Record) -> Record) -> Option<()> {
        let lock = self.activity.lock(stem)?;
        let current = self.activity.read(stem)?;
        let result = apply::settle(
            &current,
            change(&current),
            self.me.started_micros,
            A::WAITING_TOOLS,
        );
        self.activity.write(stem, &result).ok()?;
        drop(lock);
        Some(())
    }
}

/// `path` as a record stores it.
fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `text` as a label stores it (H15), `None` when nothing is left.
fn label_text(text: &str) -> Option<String> {
    Some(text::clean(text, MAX_LABEL)).filter(|text| !text.is_empty())
}

/// A file tool's target: its path inside `root` as written, else its file name (H14).
fn file_target(text: &str, path: &Path, root: &Path) -> String {
    let root_text = root.to_string_lossy();
    let relative = text
        .strip_prefix(root_text.as_ref())
        .and_then(|rest| rest.strip_prefix('/'))
        .map(str::to_owned)
        .or_else(|| {
            let resolved = place::local(path).map(resolve)?;
            resolved.strip_prefix(root).ok().map(path_text)
        })
        .unwrap_or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
    text::clean(&relative, MAX_TEXT)
}

/// `path` with its existing part's symlinks resolved and the rest kept as written.
fn resolve(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    while existing.canonicalize().is_err()
        && let Some(name) = existing.file_name().map(std::ffi::OsStr::to_os_string)
    {
        rest.push(name);
        existing.pop();
    }
    let rejoin = |resolved: PathBuf| rest.iter().rev().fold(resolved, |acc, part| acc.join(part));
    existing
        .canonicalize()
        .map_or_else(|_| path.to_path_buf(), rejoin)
}

#[cfg(test)]
mod tests;
