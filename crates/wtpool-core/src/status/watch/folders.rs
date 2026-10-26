//! [`JobFolders`]: the job folder and the folder of the machine config. A watch follows each
//! once it exists, not recursively. Until then it follows the nearest existing ancestor, so the
//! folder's creation shows at once.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::Change;
use crate::jobs::JobPaths;
use crate::paths::comparable;

const RECORD: &str = "json";
const LOCK: &str = "lock";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Jobs,
    MachineConfig,
}

impl JobFolders {
    /// The folders of `jobs`, followed through `watcher` once [`JobFolders::watch_new`] finds
    /// them.
    #[must_use]
    pub fn new(watcher: notify::RecommendedWatcher, jobs: Option<&JobPaths>) -> Self {
        let folders = jobs
            .map(|paths| {
                let config_folder = paths.config.parent().map(Path::to_path_buf);
                std::iter::once((Kind::Jobs, Some(paths.folder.clone())))
                    .chain(std::iter::once((Kind::MachineConfig, config_folder)))
                    .filter_map(|(kind, path)| {
                        Some(Followed {
                            kind,
                            path: path?,
                            spellings: Vec::new(),
                            waiting: None,
                        })
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
    pub fn classify(&self, path: &Path) -> Vec<Change> {
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
            if !self.spellings.is_empty() {
                return;
            }
            if self.path.is_dir() {
                if let Some(spellings) = watch(watcher, &self.path) {
                    self.spellings = spellings;
                    if let Some(before) = self.waiting.take() {
                        let _ = watcher.unwatch(&before.path);
                    }
                }
                return;
            }
            let Some((nearest, next)) = nearest_existing(&self.path) else {
                return;
            };
            if self
                .waiting
                .as_ref()
                .is_some_and(|waiting| waiting.path == nearest)
            {
                return;
            }
            let Some(spellings) = watch(watcher, &nearest) else {
                return;
            };
            if let Some(before) = self.waiting.take() {
                let _ = watcher.unwatch(&before.path);
            }
            self.waiting = Some(Ancestor {
                path: nearest,
                spellings,
                next,
            });
        }
    }

    fn classify(&self, path: &Path) -> Option<Change> {
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
        match self.kind {
            Kind::Jobs => job_stem(path).map(|stem| Change::Job(stem.to_os_string())),
            Kind::MachineConfig => is_machine_config(name).then_some(Change::MachineConfig),
        }
    }

    /// The change that a folder made or removed means: every job, or the config.
    fn made(&self) -> Change {
        match self.kind {
            Kind::Jobs => Change::Job(Default::default()),
            Kind::MachineConfig => Change::MachineConfig,
        }
    }

    /// `true` while a watch waits for the folder in an ancestor.
    fn waits(&self) -> bool {
        self.waiting.is_some()
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

/// The nearest existing ancestor of `path`, and the name below it on the way to `path`.
fn nearest_existing(path: &Path) -> Option<(PathBuf, OsString)> {
    let nearest = path
        .ancestors()
        .skip(1)
        .find(|ancestor| ancestor.is_dir())?;
    let next = path.strip_prefix(nearest).ok()?.iter().next()?;
    Some((nearest.to_path_buf(), next.to_os_string()))
}

/// The name without extension of a job's record or lock file.
fn job_stem(path: &Path) -> Option<&OsStr> {
    let extension = path.extension()?;
    let stem = path.file_stem()?;
    let own_file = extension == RECORD || extension == LOCK;
    (own_file && !stem.to_string_lossy().starts_with('.') && stem != "queue").then_some(stem)
}

/// `true` for the machine config or a file a write puts in its place, never for its lock.
fn is_machine_config(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    name.contains("machine.toml") && !name.ends_with(".lock")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::sync::mpsc::{Receiver, channel};
    use std::time::{Duration, Instant};

    use rstest::rstest;

    use super::super::{Message, message_of};
    use super::*;

    fn followed(kind: Kind) -> Followed {
        Followed {
            kind,
            path: PathBuf::from("/s/jobs"),
            spellings: vec![PathBuf::from("/s/jobs"), PathBuf::from("/real/s/jobs")],
            waiting: None,
        }
    }

    #[rstest]
    #[case::record("/s/jobs/42-0badc0de.json", Some("42-0badc0de"))]
    #[case::lock("/real/s/jobs/42-0badc0de.lock", Some("42-0badc0de"))]
    #[case::the_folder("/s/jobs", Some(""))]
    #[case::temp_file("/s/jobs/.42-0badc0de.json.42.tmp", None)]
    #[case::queue_lock("/s/jobs/queue.lock", None)]
    #[case::ticket("/s/jobs/ticket", None)]
    #[case::below("/s/jobs/x/42-0badc0de.json", None)]
    #[case::elsewhere("/t/jobs/42-0badc0de.json", None)]
    fn followed_classify__should_name_the_job_of_its_files(
        #[case] path: &str,
        #[case] stem: Option<&str>,
    ) {
        // When
        let change = followed(Kind::Jobs).classify(Path::new(path));

        // Then
        assert_eq!(change, stem.map(|stem| Change::Job(stem.into())));
    }

    #[rstest]
    #[case::the_file("/s/jobs/machine.toml", true)]
    #[case::a_file_renamed_over_it("/s/jobs/machine.toml.wtpool-verify", true)]
    #[case::its_lock("/s/jobs/machine.toml.lock", false)]
    #[case::another_file("/s/jobs/notes.txt", false)]
    #[case::the_folder("/s/jobs", true)]
    fn followed_classify__should_see_the_machine_config(#[case] path: &str, #[case] seen: bool) {
        // When
        let change = followed(Kind::MachineConfig).classify(Path::new(path));

        // Then
        assert_eq!(change, seen.then_some(Change::MachineConfig));
    }

    #[rstest]
    #[case::the_name_on_the_way("/s/wtpool", Some(Change::Job(OsString::new())))]
    #[case::its_other_spelling("/real/s/wtpool", Some(Change::Job(OsString::new())))]
    #[case::another_name("/s/notes.txt", None)]
    #[case::below_the_name("/s/wtpool/x", None)]
    #[case::elsewhere("/t/wtpool", None)]
    fn followed_classify__should_see_the_folder_on_the_way_where_it_waits(
        #[case] path: &str,
        #[case] expected: Option<Change>,
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

    #[test]
    fn nearest_existing__should_name_the_step_below_it() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state/wtpool/jobs");

        // When
        let nearest = nearest_existing(&path);

        // Then
        assert_eq!(
            nearest,
            Some((dir.path().to_path_buf(), OsString::from("state")))
        );
    }

    /// Waits for an event at a path called `name` that `folders` reads as `expected`.
    fn hears(
        folders: &JobFolders,
        events: &Receiver<Message>,
        name: &str,
        expected: &Change,
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
        Change::Job("42-0badc0de".into())
    )]
    #[case::folder_of_the_machine_config(
        "home/.config/wtpool",
        "machine.toml",
        Change::MachineConfig
    )]
    fn job_folders_watch_new__should_follow_a_folder_made_after_the_watch_starts(
        #[case] folder: &str,
        #[case] file: &str,
        #[case] change: Change,
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
            Change::Job(_) => Change::Job(OsString::new()),
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
