//! [`JobFolders`]: the job folder and the folder of the machine config. A watch follows each
//! once it exists, not recursively. Until then it follows the nearest existing of the folder's
//! parent and grandparent, so the folder's creation shows at once.

use std::ffi::OsStr;
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
    /// The ancestor watched while the folder is missing, and its spellings.
    ancestor: Option<(PathBuf, Vec<PathBuf>)>,
}

/// The number of levels above a missing folder a watch waits in. A heartbeat finds a folder
/// made higher up.
const ANCESTORS: usize = 2;

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
                            ancestor: None,
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
        use notify::Watcher;
        let watch = |watcher: &mut notify::RecommendedWatcher, path: &Path| {
            watcher
                .watch(path, notify::RecursiveMode::NonRecursive)
                .is_ok()
                .then(|| vec![path.to_path_buf(), comparable(path)])
        };
        for folder in &mut self.folders {
            if !folder.spellings.is_empty() {
                continue;
            }
            if folder.path.is_dir() {
                if let Some(spellings) = watch(&mut self.watcher, &folder.path) {
                    folder.spellings = spellings;
                    if let Some((ancestor, _)) = folder.ancestor.take() {
                        let _ = self.watcher.unwatch(&ancestor);
                    }
                }
                continue;
            }
            let nearest = folder
                .path
                .ancestors()
                .skip(1)
                .take(ANCESTORS)
                .find(|ancestor| ancestor.is_dir());
            let waiting_in = folder.ancestor.as_ref().map(|(path, _)| path.as_path());
            if let Some(nearest) = nearest
                && waiting_in != Some(nearest)
                && let Some(spellings) = watch(&mut self.watcher, nearest)
            {
                if let Some((before, _)) = folder.ancestor.take() {
                    let _ = self.watcher.unwatch(&before);
                }
                folder.ancestor = Some((nearest.to_path_buf(), spellings));
            }
        }
    }

    /// The change an event at `path` in one of the folders may mean.
    #[must_use]
    pub fn classify(&self, path: &Path) -> Option<Change> {
        self.folders.iter().find_map(|folder| folder.classify(path))
    }

    /// `true` while a folder is still waited for, so a change may have made it.
    #[must_use]
    pub fn waits(&self) -> bool {
        self.folders.iter().any(Followed::waits)
    }
}

impl Followed {
    fn classify(&self, path: &Path) -> Option<Change> {
        let parent = path.parent()?;
        let in_ancestor = self
            .ancestor
            .as_ref()
            .is_some_and(|(_, spellings)| spellings.iter().any(|spelling| spelling == parent));
        if in_ancestor || self.spellings.iter().any(|spelling| spelling == path) {
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
        self.ancestor.is_some()
    }
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

    use rstest::rstest;

    use super::*;

    fn followed(kind: Kind) -> Followed {
        Followed {
            kind,
            path: PathBuf::from("/s/jobs"),
            spellings: vec![PathBuf::from("/s/jobs"), PathBuf::from("/real/s/jobs")],
            ancestor: None,
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

    #[test]
    fn followed_classify__should_see_a_change_where_it_waits_for_the_folder() {
        // Given
        let waiting = Followed {
            kind: Kind::Jobs,
            path: PathBuf::from("/s/wtpool/jobs"),
            spellings: Vec::new(),
            ancestor: Some((PathBuf::from("/s"), vec![PathBuf::from("/s")])),
        };

        // When
        let made = waiting.classify(Path::new("/s/wtpool"));
        let other = waiting.classify(Path::new("/t/wtpool"));

        // Then
        assert_eq!((made, other), (Some(Change::Job(OsString::new())), None));
        assert!(waiting.waits());
    }
}
