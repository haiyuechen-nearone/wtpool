//! One hook event end to end: the worktree it belongs to, the change it makes, the records it
//! writes under their locks, and after a session's end the day's gc. A harness's adapter turns
//! its input into the [`HookInput`] this works on.

use std::path::{Path, PathBuf};

use super::adapter::{Adapter, ProcessMode, Rejection, TranscriptTitles};
use super::apply::{self, Agent, Change, Found, Fresh, Place, Titles, Writer};
use super::env::{HookEnv, Me};
use super::gc;
use super::input::{CallFacts, CallRef, Event, HookInput};
use super::log;
use super::place::{self, Shell, ToolPath, Worktree, file_target};
use super::record::{HarnessProcess, Record, SubagentTitle};
use super::store::ActivityDir;
use super::tasks::{self, Started, TaskEvent};
use super::text::{self, MAX_LABEL, MAX_TEXT};
use crate::activity::{ActorKey, PlacedBy, State};
use crate::clock::{ReadClock, Timestamp};
use crate::paths::comparable;
use crate::platform::zone;
use crate::shell;

/// One event's facts that need the file system, worked out before any lock is taken.
struct Context<'a, A> {
    adapter: &'a A,
    input: &'a HookInput,
    env: &'a HookEnv,
    activity: ActivityDir,
    key: ActorKey,
    cwd: PathBuf,
    cwd_worktree: Worktree,
    me: Me,
    /// The harness process of the hook, as the actor's record names it.
    process: Option<HarnessProcess>,
    now: Timestamp,
}

/// Handles one raw hook input of the harness `adapter` reads. Malformed input, an id outside
/// its pattern, a folder outside every pool or a pool without a usable `wtpool-activity/` writes
/// nothing. The hook's log names an input the adapter finds invalid.
pub fn handle<A: Adapter>(adapter: &A, input: &[u8], env: &HookEnv) {
    match adapter.parse(input) {
        Ok(input) => handle_input(adapter, &input, env),
        Err(Rejection::Invalid(reason)) => {
            if let Some(dir) = &env.log {
                log::append(dir, &format!("wtpool hook: skipped an event: {reason}"));
            }
        }
        Err(Rejection::Ignored) => {}
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
    let Some(activity) = ActivityDir::open(&cwd_worktree.common) else {
        return;
    };
    if activity.is_sandbox() && !env.sandbox {
        return;
    }
    let now = env.clock.now();
    let me = env.me();
    let process = me.parent.clone().map(|parent| HarnessProcess {
        shared: A::PROCESS == ProcessMode::Shared,
        ..parent
    });
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
        me,
        process,
        now,
    };
    if context.run(&input.event).is_some() && input.event == Event::SessionEnd {
        let micros = env.entered_micros.saturating_add(gc::HOOK_BUDGET_MICROS);
        let deadline = gc::Deadline::new(&env.clock, micros);
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
            .ok()
            .or_else(|| self.skipped("its record's lock cannot be taken"))?;
        let current = self.activity.read(&stem);
        if let Some(other) = current.as_ref().and_then(|r| self.foreign(r)) {
            return self.skipped(&format!("its record names another harness, {other}"));
        }
        if let Event::Notified(_) = event
            && current.as_ref().is_some_and(|r| r.waiting.is_some())
        {
            return None;
        }
        let subagent = self.input.agent_id.is_some();
        if apply::writes_nothing(current.as_ref(), event, self.now, subagent) {
            return None;
        }
        let (event, moved_to) = self.located(event, current.as_ref());
        let change = self.change(current.as_ref(), moved_to);
        let applied = apply::apply(current.as_ref(), &event, &change);
        let info = event == Event::Info;
        let mut result = match &current {
            Some(current) if !info => apply::settle(current, applied, &change.writer),
            _ => applied,
        };
        if info && current.as_ref() == Some(&result) {
            return None;
        }
        tasks::update(&mut result, &self.task_event(&event), self.now);
        if let Err(error) = self.activity.write(&stem, &result) {
            return self.skipped(&format!("its record cannot be written: {error}"));
        }
        drop(lock);
        self.end_session_tasks(&stem);
        self.side_effects(&event, &result, &change);
        Some(())
    }

    /// The harness `record` names when it is not the adapter's: the hook never writes over it.
    fn foreign<'r>(&self, record: &'r Record) -> Option<&'r str> {
        (record.harness != A::HARNESS).then_some(record.harness.as_str())
    }

    /// Notes in the hook's log that the event wrote nothing, for `reason`, and returns `None`.
    fn skipped<T>(&self, reason: &str) -> Option<T> {
        if let Some(dir) = &self.env.log {
            log::append(dir, &format!("wtpool hook: skipped an event: {reason}"));
        }
        None
    }

    /// Notes in the hook's log that a change to the record `stem` was not written, for `reason`.
    fn dropped(&self, stem: &str, reason: &str) {
        if let Some(dir) = &self.env.log {
            log::append(
                dir,
                &format!("wtpool hook: skipped a change to the record {stem}: {reason}"),
            );
        }
    }

    /// Ends the old running tasks that a turn end's list of work in flight leaves out in the
    /// session's other records on this event's harness process, as the list covers the whole
    /// session. Nothing else in those records changes.
    fn end_session_tasks(&self, own: &str) {
        let (Some(running), Some(process)) = (&self.input.tasks_running, &self.process) else {
            return;
        };
        let others = self
            .activity
            .session_stems(&self.key)
            .into_iter()
            .filter(|stem| stem != own);
        for stem in others {
            let lock = match self.activity.lock(&stem) {
                Ok(lock) => lock,
                Err(error) => {
                    self.dropped(&stem, &format!("its lock cannot be taken: {error}"));
                    continue;
                }
            };
            if let Some(mut record) = self.activity.read(&stem)
                && self.foreign(&record).is_none()
                && record.process.as_ref().is_some_and(|own| own.is(process))
                && tasks::end_unlisted(&mut record, running, self.now)
                && let Err(error) = self.activity.write(&stem, &record)
            {
                self.dropped(&stem, &format!("it cannot be written: {error}"));
            }
            drop(lock);
        }
    }

    /// The event's facts of the actor's background tasks: a task its call started, with the
    /// output file the adapter names, and the tasks it ends.
    fn task_event(&self, event: &Event) -> TaskEvent {
        let started = match (event, &self.input.task_start) {
            (Event::CallEnd(CallRef::Facts(call)), Some(task)) => self
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

    fn subagent_waits(&self) -> bool {
        self.activity
            .subagents_of(&self.key)
            .iter()
            .filter_map(|stem| self.activity.read(stem))
            .any(|record| record.state == State::Waiting)
    }

    /// The event with its call targets, and the worktree its path names.
    fn located(&self, event: &Event, current: Option<&Record>) -> (Event, Option<String>) {
        let base = current.map_or_else(
            || self.cwd_worktree.root.clone(),
            |record| PathBuf::from(&record.worktree),
        );
        match event {
            Event::CallStart(call) => {
                let (target, moved) = self.tool_place(&base);
                (Event::CallStart(with_target(call, target)), moved)
            }
            Event::Wait(reason, call) => {
                let (target, moved) = self.tool_place(&base);
                (Event::Wait(*reason, ref_with_target(call, target)), moved)
            }
            Event::CallEnd(call) => {
                let (target, moved) = self.tool_place(&base);
                (Event::CallEnd(ref_with_target(call, target)), moved)
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
                    Shell::Posix => shell::parse(command, &self.cwd, self.env.home.as_deref()),
                };
                let moved = self.command_place(&parsed);
                (text::clean(&parsed.summary, MAX_TEXT), moved)
            }
            ToolPath::None => (String::new(), None),
        }
    }

    /// The worktree a command moves its actor to. A command that starts an agent moves none, as
    /// that agent works there and its caller stays where it is.
    fn command_place(&self, parsed: &shell::Command) -> Option<String> {
        if parsed.starts_agent {
            return None;
        }
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

    /// The worktree an [`Event::CwdChange`] moves the actor to, none when back to the session's
    /// first folder.
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
        let agent = self.input.agent_id.as_ref().map(|id| Agent {
            id: id.clone(),
            agent_type: self.agent_type(),
        });
        let label = agent.as_ref().and_then(|agent| agent.agent_type.clone());
        let read = match &agent {
            Some(_) => None,
            None => self.transcript(current),
        };
        let found = read.clone().unwrap_or_default();
        let given = self.input.session_title.as_deref().and_then(label_text);
        let titles = Titles {
            user: agent
                .is_none()
                .then(|| given.or_else(|| found.user.as_deref().and_then(label_text)))
                .flatten(),
            generated: self
                .input
                .title
                .as_deref()
                .and_then(title_text)
                .or_else(|| found.generated.as_deref().and_then(label_text)),
        };
        let model = self
            .input
            .model
            .as_deref()
            .or(found.model.as_deref())
            .and_then(model_text);
        let subagent = self.input.spawned.as_ref().and_then(|spawned| {
            Some(SubagentTitle {
                agent_id: spawned.agent_id.clone(),
                title: label_text(&spawned.title)?,
            })
        });
        let new_label = match &agent {
            Some(agent) => agent
                .agent_type
                .clone()
                .unwrap_or_else(|| "subagent".to_owned()),
            None => {
                text::untitled_label(A::LABEL, zone::local_time(self.now, self.env.tz.as_deref()))
            }
        };
        let place = match (&agent, current) {
            (Some(_), None) => self.parent_place(),
            _ => self.cwd_place(),
        };
        Change {
            writer: self.writer(),
            fresh: Fresh {
                session: self.input.session_id.clone(),
                agent,
                place,
                first_cwd: self.input.cwd.clone(),
                label: new_label,
            },
            found: Found {
                moved_to,
                label,
                titles,
                transcript_offset: read.map(|read| read.offset),
                subagent,
                model,
            },
        }
    }

    fn writer(&self) -> Writer {
        Writer {
            harness: A::HARNESS,
            waiting_tools: A::WAITING_TOOLS,
            now: self.now,
            started_micros: self.me.started_micros,
            process: self.process.clone(),
        }
    }

    fn agent_type(&self) -> Option<String> {
        self.input.agent_type.as_deref().and_then(label_text)
    }

    /// The titles and model the new part of the session's transcript gives, if it can be read.
    /// The read starts where the record's last one ended.
    fn transcript(&self, current: Option<&Record>) -> Option<TranscriptTitles> {
        let offset = current.map_or(0, |record| record.transcript_offset);
        self.input
            .transcript
            .as_deref()
            .map(Path::new)
            .filter(|path| path.is_absolute())
            .and_then(|path| self.adapter.titles(path, offset))
    }

    /// A new subagent starts at its session's worktree with the session's [`PlacedBy`].
    fn parent_place(&self) -> Place {
        match self.activity.read(&self.key.session_key().stem()) {
            Some(parent) => Place {
                worktree: parent.worktree,
                placed_by: parent.placed_by,
            },
            None => self.cwd_place(),
        }
    }

    fn cwd_place(&self) -> Place {
        Place {
            worktree: path_text(&self.cwd_worktree.root),
            placed_by: PlacedBy::Cwd,
        }
    }

    /// The change an event makes to other records: a turn end that finishes the subagents
    /// finishes them, and the session's end deletes them. An older session on the process of a
    /// newer one stays as it is, as `wtpool status` reads its end from the records.
    fn side_effects(&self, event: &Event, result: &Record, change: &Change) {
        if result.is_subagent() {
            return;
        }
        match event {
            Event::TurnEnd {
                subagents_done: true,
                ..
            } => {
                for stem in self.activity.subagents_of(&self.key) {
                    self.update(&stem, |record| {
                        apply::finish_subagent(record, &change.writer)
                    });
                }
            }
            Event::SessionEnd => {
                for stem in self.activity.subagents_of(&self.key) {
                    if let Ok(lock) = self.activity.lock(&stem) {
                        match self.activity.read(&stem) {
                            Some(record) if self.foreign(&record).is_some() => {
                                self.dropped(&stem, "it names another harness");
                            }
                            _ => self.activity.delete(&stem, lock),
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Rewrites the record `stem` under its lock, keeping a state a later hook wrote. A record
    /// `change` leaves as it is stays unwritten.
    fn update(&self, stem: &str, change: impl Fn(&Record) -> Option<Record>) {
        let lock = match self.activity.lock(stem) {
            Ok(lock) => lock,
            Err(error) => return self.dropped(stem, &format!("its lock cannot be taken: {error}")),
        };
        let current = self.activity.read(stem);
        if current.as_ref().is_some_and(|r| self.foreign(r).is_some()) {
            self.dropped(stem, "it names another harness");
        } else if let Some(current) = current
            && let Some(changed) = change(&current)
        {
            let result = apply::settle(&current, changed, &self.writer());
            if let Err(error) = self.activity.write(stem, &result) {
                self.dropped(stem, &format!("it cannot be written: {error}"));
            }
        }
        drop(lock);
    }
}

/// `path` as a record stores it.
fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `text` as a label stores it, `None` when nothing is left.
fn label_text(text: &str) -> Option<String> {
    Some(text::clean(text, MAX_LABEL)).filter(|text| !text.is_empty())
}

/// A title an event names, as a label stores it: `None` when only blanks are left.
fn title_text(text: &str) -> Option<String> {
    label_text(text).filter(|text| !text.trim().is_empty())
}

/// A model an event names, as a record stores it, `None` when nothing is left.
fn model_text(text: &str) -> Option<String> {
    Some(text::clean(text, MAX_TEXT)).filter(|text| !text.is_empty())
}

/// `call` with its target in the event's worktree.
fn with_target(call: &CallFacts, target: String) -> CallFacts {
    CallFacts {
        target,
        ..call.clone()
    }
}

/// `call` with its target, when the event gives its facts.
fn ref_with_target(call: &CallRef, target: String) -> CallRef {
    match call {
        CallRef::Facts(facts) => CallRef::Facts(with_target(facts, target)),
        CallRef::Id(id) => CallRef::Id(id.clone()),
    }
}

#[cfg(test)]
mod tests;
