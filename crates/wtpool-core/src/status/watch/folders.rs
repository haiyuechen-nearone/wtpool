//! [`JobFolders`]: the job folder and the folder of the machine config. A watch follows each
//! once it exists, not recursively. Until then it follows the nearest existing ancestor, so the
//! folder's creation shows at once.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::FolderChange;
use crate::jobs::{JobFile, JobFolder, JobPaths};
use crate::paths::comparable;

/// The folders of this user's heavy jobs that a watch follows besides the common git directory.
pub struct JobFolders {
    watcher: notify::RecommendedWatcher,
    folders: Vec<Followed>,
}

/// One folder, and the spellings its events may name it by once it is watched.
struct Followed {
    kind: Kind,
    path: PathBuf,
    spellings: Vec<PathBuf>,
    waiting: Option<Ancestor>,
}

/// The nearest existing ancestor of a missing folder, watched until the folder exists.
struct Ancestor {
    path: PathBuf,
    spellings: Vec<PathBuf>,
    /// The name below it on the way to the folder: only its creation brings the folder nearer.
    next: OsString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Jobs,
    /// The folder of the machine config with this file name.
    MachineConfig(OsString),
}

/// The existing folder that a watch for a followed folder starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    Folder,
    /// The nearest existing ancestor, and the name below it on the way to the folder.
    Ancestor {
        path: PathBuf,
        next: OsString,
    },
}

impl JobFolders {
    /// The folders of `jobs`, followed through `watcher` once [`JobFolders::watch_new`] finds
    /// them.
    #[must_use]
    pub fn new(watcher: notify::RecommendedWatcher, jobs: Option<&JobPaths>) -> Self {
        let folders = jobs
            .map(|paths| {
                let config = paths.config.file_name().zip(paths.config.parent());
                let config_folder = config.map(|(name, folder)| {
                    (
                        Kind::MachineConfig(name.to_os_string()),
                        folder.to_path_buf(),
                    )
                });
                std::iter::once((Kind::Jobs, paths.folder.clone()))
                    .chain(config_folder)
                    .map(|(kind, path)| Followed {
                        kind,
                        path,
                        spellings: Vec::new(),
                        waiting: None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { watcher, folders }
    }

    /// Starts to watch each folder that exists now and is not watched yet, else the nearest
    /// existing ancestor it waits for the folder in.
    pub fn watch_new(&mut self) {
        for folder in &mut self.folders {
            folder.follow(&mut self.watcher);
        }
    }

    /// The changes an event at `path` in the folders may mean: one per folder, as a folder made
    /// on the way may bring both nearer.
    #[must_use]
    pub fn classify(&self, path: &Path) -> Vec<FolderChange> {
        self.folders
            .iter()
            .filter_map(|folder| folder.classify(path))
            .collect()
    }

    /// `true` while a folder is still waited for, so a change may have made it.
    #[must_use]
    pub fn waits(&self) -> bool {
        self.folders.iter().any(Followed::waits)
    }
}

impl Followed {
    /// Watches the folder once it exists, else its nearest existing ancestor. A folder made while
    /// a watch starts sends no event, so it looks again until the folder or ancestor stays the
    /// same, at most once per level.
    fn follow(&mut self, watcher: &mut notify::RecommendedWatcher) {
        use notify::Watcher;
        for _ in self.path.ancestors() {
            if self.watched() {
                return;
            }
            let found = nearest_existing(&self.path, Path::is_dir);
            let Some(target) = self.next_target(found) else {
                return;
            };
            let Some(spellings) = watch(watcher, target.path_for(&self.path)) else {
                return;
            };
            if let Some(before) = self.waiting.take() {
                let _ = watcher.unwatch(&before.path);
            }
            match target {
                Target::Folder => {
                    self.spellings = spellings;
                    return;
                }
                Target::Ancestor { path, next } => {
                    self.waiting = Some(Ancestor {
                        path,
                        spellings,
                        next,
                    });
                }
            }
        }
    }

    /// The target that a watch starts on next, given `found`, the nearest existing folder.
    /// `None` while the watch waits in that ancestor already.
    fn next_target(&self, found: Option<Target>) -> Option<Target> {
        let found = found?;
        let waits_there = matches!(&found, Target::Ancestor { path, .. }
            if self.waiting.as_ref().is_some_and(|waiting| &waiting.path == path));
        (!waits_there).then_some(found)
    }

    fn classify(&self, path: &Path) -> Option<FolderChange> {
        let parent = path.parent()?;
        let on_the_way = self.waiting.as_ref().is_some_and(|waiting| {
            path.file_name() == Some(waiting.next.as_os_str())
                && waiting.spellings.iter().any(|spelling| spelling == parent)
        });
        if on_the_way || self.spellings.iter().any(|spelling| spelling == path) {
            return Some(self.made());
        }
        if !self.spellings.iter().any(|spelling| spelling == parent) {
            return None;
        }
        let name = path.file_name()?;
        match &self.kind {
            Kind::Jobs => job_change(name),
            Kind::MachineConfig(config) => {
                is_machine_config(name, config).then_some(FolderChange::MachineConfig)
            }
        }
    }

    /// The change that a folder made or removed means: every job, or the config.
    fn made(&self) -> FolderChange {
        match self.kind {
            Kind::Jobs => FolderChange::AllJobs,
            Kind::MachineConfig(_) => FolderChange::MachineConfig,
        }
    }

    fn watched(&self) -> bool {
        !self.spellings.is_empty()
    }

    /// `true` while a watch waits for the folder in an ancestor.
    fn waits(&self) -> bool {
        self.waiting.is_some()
    }
}

impl Target {
    /// The path of this target on the way to `folder`.
    fn path_for<'a>(&'a self, folder: &'a Path) -> &'a Path {
        match self {
            Self::Folder => folder,
            Self::Ancestor { path, .. } => path,
        }
    }
}

/// The spellings of `path` once a watch of it alone has started, `None` when it cannot start.
fn watch(watcher: &mut notify::RecommendedWatcher, path: &Path) -> Option<Vec<PathBuf>> {
    use notify::Watcher;
    watcher
        .watch(path, notify::RecursiveMode::NonRecursive)
        .is_ok()
        .then(|| vec![path.to_path_buf(), comparable(path)])
}

/// `path` when it is a folder, else its nearest ancestor that is one. `None` when no ancestor
/// is a folder.
fn nearest_existing(path: &Path, is_dir: impl Fn(&Path) -> bool) -> Option<Target> {
    if is_dir(path) {
        return Some(Target::Folder);
    }
    let nearest = path.ancestors().skip(1).find(|ancestor| is_dir(ancestor))?;
    let next = path.strip_prefix(nearest).ok()?.iter().next()?;
    Some(Target::Ancestor {
        path: nearest.to_path_buf(),
        next: next.to_os_string(),
    })
}

/// The change an event at the job folder's file `name` means. A name of no job with a record's
/// or lock's extension reads every job again.
fn job_change(name: &OsStr) -> Option<FolderChange> {
    match JobFolder::file_kind(name) {
        JobFile::Record(id) | JobFile::Lock(id) => Some(FolderChange::Job(id)),
        JobFile::Other => {
            let extension = Path::new(name).extension();
            let own_extension =
                extension.is_some_and(|extension| extension == "json" || extension == "lock");
            own_extension.then_some(FolderChange::AllJobs)
        }
        JobFile::Queue | JobFile::Temp => None,
    }
}

/// `true` for the machine config `config` or a file a write renames over it, never for its lock.
fn is_machine_config(name: &OsStr, config: &OsStr) -> bool {
    let (name, config) = (name.to_string_lossy(), config.to_string_lossy());
    name.contains(config.as_ref()) && !name.ends_with(".lock")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::sync::mpsc::{Receiver, channel};
    use std::time::{Duration, Instant};

    use rstest::rstest;

    use super::super::{Message, message_of};
    use super::*;
    use crate::jobs::JobId;

    fn followed(kind: Kind) -> Followed {
        Followed {
            kind,
            path: PathBuf::from("/s/jobs"),
            spellings: vec![PathBuf::from("/s/jobs"), PathBuf::from("/real/s/jobs")],
            waiting: None,
        }
    }

    fn job(id: &str) -> Option<FolderChange> {
        Some(FolderChange::Job(JobId::parse(id).unwrap()))
    }

    #[rstest]
    #[case::record("/s/jobs/42-0badc0de.json", job("42-0badc0de"))]
    #[case::lock("/real/s/jobs/42-0badc0de.lock", job("42-0badc0de"))]
    #[case::the_folder("/s/jobs", Some(FolderChange::AllJobs))]
    #[case::a_record_of_no_job("/s/jobs/notes.json", Some(FolderChange::AllJobs))]
    #[case::a_lock_of_no_job("/s/jobs/notes.lock", Some(FolderChange::AllJobs))]
    #[case::temp_file("/s/jobs/.42-0badc0de.json.42.tmp", None)]
    #[case::queue_lock("/s/jobs/queue.lock", None)]
    #[case::ticket("/s/jobs/ticket", None)]
    #[case::another_extension("/s/jobs/notes.txt", None)]
    #[case::below("/s/jobs/x/42-0badc0de.json", None)]
    #[case::elsewhere("/t/jobs/42-0badc0de.json", None)]
    fn followed_classify__should_name_the_job_of_its_files(
        #[case] path: &str,
        #[case] expected: Option<FolderChange>,
    ) {
        // When
        let change = followed(Kind::Jobs).classify(Path::new(path));

        // Then
        assert_eq!(change, expected);
    }

    #[rstest]
    #[case::the_file("/s/jobs/machine.toml", true)]
    #[case::a_file_renamed_over_it("/s/jobs/machine.toml.wtpool-verify", true)]
    #[case::its_lock("/s/jobs/machine.toml.lock", false)]
    #[case::another_file("/s/jobs/notes.txt", false)]
    #[case::the_folder("/s/jobs", true)]
    fn followed_classify__should_see_the_machine_config(#[case] path: &str, #[case] seen: bool) {
        // Given
        let config = followed(Kind::MachineConfig(OsString::from("machine.toml")));

        // When
        let change = config.classify(Path::new(path));

        // Then
        assert_eq!(change, seen.then_some(FolderChange::MachineConfig));
    }

    #[rstest]
    #[case::the_name_on_the_way("/s/wtpool", Some(FolderChange::AllJobs))]
    #[case::its_other_spelling("/real/s/wtpool", Some(FolderChange::AllJobs))]
    #[case::another_name("/s/notes.txt", None)]
    #[case::below_the_name("/s/wtpool/x", None)]
    #[case::elsewhere("/t/wtpool", None)]
    fn followed_classify__should_see_the_folder_on_the_way_where_it_waits(
        #[case] path: &str,
        #[case] expected: Option<FolderChange>,
    ) {
        // Given
        let waiting = Followed {
            kind: Kind::Jobs,
            path: PathBuf::from("/s/wtpool/jobs"),
            spellings: Vec::new(),
            waiting: Some(Ancestor {
                path: PathBuf::from("/s"),
                spellings: vec![PathBuf::from("/s"), PathBuf::from("/real/s")],
                next: OsString::from("wtpool"),
            }),
        };

        // When
        let change = waiting.classify(Path::new(path));

        // Then
        assert_eq!(change, expected);
        assert!(waiting.waits());
    }

    fn ancestor(path: &str, next: &str) -> Option<Target> {
        Some(Target::Ancestor {
            path: PathBuf::from(path),
            next: OsString::from(next),
        })
    }

    #[rstest]
    #[case::the_folder(&["/", "/s", "/s/state", "/s/state/wtpool/jobs"], Some(Target::Folder))]
    #[case::an_ancestor(&["/", "/s"], ancestor("/s", "state"))]
    #[case::no_folder(&[], None)]
    fn nearest_existing__should_name_the_step_below_it(
        #[case] folders: &[&str],
        #[case] expected: Option<Target>,
    ) {
        // Given
        let is_dir = |path: &Path| folders.iter().any(|folder| path == Path::new(folder));

        // When
        let nearest = nearest_existing(Path::new("/s/state/wtpool/jobs"), is_dir);

        // Then
        assert_eq!(nearest, expected);
    }

    #[rstest]
    #[case::the_folder(Some(Target::Folder), Some(Target::Folder))]
    #[case::the_ancestor_it_waits_in(ancestor("/s", "state"), None)]
    #[case::a_nearer_ancestor(ancestor("/s/state", "wtpool"), ancestor("/s/state", "wtpool"))]
    #[case::nothing(None, None)]
    fn followed_next_target__should_move_only_nearer(
        #[case] found: Option<Target>,
        #[case] expected: Option<Target>,
    ) {
        // Given
        let waiting = Followed {
            kind: Kind::Jobs,
            path: PathBuf::from("/s/state/wtpool/jobs"),
            spellings: Vec::new(),
            waiting: Some(Ancestor {
                path: PathBuf::from("/s"),
                spellings: vec![PathBuf::from("/s")],
                next: OsString::from("state"),
            }),
        };

        // When
        let target = waiting.next_target(found);

        // Then
        assert_eq!(target, expected);
    }

    /// Waits for an event at a path called `name` that `folders` reads as `expected`.
    fn hears(
        folders: &JobFolders,
        events: &Receiver<Message>,
        name: &str,
        expected: &FolderChange,
    ) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let Ok(Message::Paths { paths, .. }) = events.recv_timeout(left) else {
                return false;
            };
            if paths.iter().any(|path| {
                path.file_name() == Some(OsStr::new(name))
                    && folders.classify(path).contains(expected)
            }) {
                return true;
            }
        }
        false
    }

    #[rstest]
    #[case::job_folder(
        "home/.local/state/wtpool/jobs",
        "42-0badc0de.json",
        job("42-0badc0de").unwrap()
    )]
    #[case::folder_of_the_machine_config(
        "home/.config/wtpool",
        "machine.toml",
        FolderChange::MachineConfig
    )]
    fn job_folders_watch_new__should_follow_a_folder_made_after_the_watch_starts(
        #[case] folder: &str,
        #[case] file: &str,
        #[case] change: FolderChange,
    ) {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let paths = JobPaths {
            config: dir.path().join("home/.config/wtpool/machine.toml"),
            folder: dir.path().join("home/.local/state/wtpool/jobs"),
        };
        let (sender, events) = channel();
        let watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(message_of(event));
        })
        .unwrap();
        let mut folders = JobFolders::new(watcher, Some(&paths));
        folders.watch_new();
        let made = match change {
            FolderChange::Job(_) => FolderChange::AllJobs,
            _ => change.clone(),
        };

        // When
        std::fs::create_dir_all(dir.path().join(folder)).unwrap();
        let top = folder.split('/').next().unwrap();
        let heard_made = hears(&folders, &events, top, &made);
        folders.watch_new();
        std::fs::write(dir.path().join(folder).join(file), "").unwrap();
        let heard_file = hears(&folders, &events, file, &change);

        // Then
        assert!(heard_made, "the folder's creation is not heard");
        assert!(heard_file, "a file in the folder is not heard");
    }
}
