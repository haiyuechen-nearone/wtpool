//! [`MachineJobs`]: this user's machine config and live jobs as last read, each read again on its
//! own. Reading writes nothing, and a dead job's record is only skipped.

use std::collections::BTreeMap;

use super::JobPaths;
use super::config::MachineConfig;
use super::folder::JobFolder;
use super::record::{JobId, JobRecord, JobState};

/// The machine config and the live jobs of one user.
#[derive(Debug, Clone)]
pub struct MachineJobs {
    paths: JobPaths,
    cores: usize,
    /// The defaults while [`config_error`](Self::config_error) is set.
    config: MachineConfig,
    config_error: Option<String>,
    folder_error: Option<String>,
    records: BTreeMap<JobId, JobRecord>,
}

impl MachineJobs {
    /// The config and jobs in `paths`, on a machine of `cores` cores.
    #[must_use]
    pub fn read(paths: JobPaths, cores: usize) -> Self {
        let mut jobs = Self {
            config: MachineConfig::defaults(paths.config.clone(), cores),
            paths,
            cores,
            config_error: None,
            folder_error: None,
            records: BTreeMap::new(),
        };
        jobs.read_config();
        jobs.read_jobs();
        jobs
    }

    #[must_use]
    pub fn paths(&self) -> &JobPaths {
        &self.paths
    }

    /// Reads the machine config again.
    pub fn read_config(&mut self) {
        match MachineConfig::read(&self.paths.config, self.cores) {
            Ok(config) => {
                self.config = config;
                self.config_error = None;
            }
            Err(error) => {
                self.config = MachineConfig::defaults(self.paths.config.clone(), self.cores);
                self.config_error = Some(error.to_string());
            }
        }
    }

    /// Reads every record of the job folder again.
    pub fn read_jobs(&mut self) {
        self.records.clear();
        self.folder_error = None;
        match JobFolder::existing(self.paths.folder.clone()) {
            Ok(Some(folder)) => {
                self.records = folder
                    .live_records()
                    .into_iter()
                    .map(|record| (record.id.clone(), record))
                    .collect();
            }
            Ok(None) => {}
            Err(error) => self.folder_error = Some(error.to_string()),
        }
    }

    /// Reads the record of the job `id` again, and its lock.
    pub fn read_job(&mut self, id: &JobId) {
        match JobFolder::existing(self.paths.folder.clone()) {
            Ok(Some(folder)) => match folder.live_record(id) {
                Some(record) => {
                    self.records.insert(id.clone(), record);
                }
                None => {
                    self.records.remove(id);
                }
            },
            Ok(None) | Err(_) => self.read_jobs(),
        }
    }

    /// The config in effect: the file's, else the defaults while it cannot be read.
    #[must_use]
    pub fn config(&self) -> &MachineConfig {
        &self.config
    }

    /// The limit, `None` while the machine config is not valid.
    #[must_use]
    pub fn limit(&self) -> Option<u32> {
        self.config_error
            .is_none()
            .then_some(self.config.limit.get())
    }

    /// The fault of the machine config or the job folder, when one cannot be read.
    #[must_use]
    pub fn error(&self) -> Option<String> {
        let errors: Vec<&str> = [&self.config_error, &self.folder_error]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect();
        (!errors.is_empty()).then(|| errors.join("; "))
    }

    /// The live jobs, each in its own order.
    #[must_use]
    pub fn records(&self) -> Vec<&JobRecord> {
        self.records.values().collect()
    }

    /// The running jobs and the waiting ones, each in ticket order.
    #[must_use]
    pub fn queue(&self) -> (Vec<&JobRecord>, Vec<&JobRecord>) {
        let mut records = self.records();
        records.sort_by_key(|record| record.ticket);
        records
            .into_iter()
            .partition(|record| record.state == JobState::Running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dir: &std::path::Path) -> JobPaths {
        JobPaths {
            config: dir.join("config/machine.toml"),
            folder: dir.join("jobs"),
        }
    }

    fn record(id: &JobId, ticket: u64, state: JobState) -> JobRecord {
        JobRecord {
            id: id.clone(),
            ticket,
            state,
            ..JobRecord::sample(1)
        }
    }

    #[test]
    fn machine_jobs__should_read_live_jobs_in_queue_order_and_again_one_by_one() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let folder = JobFolder::create(dir.path().join("jobs")).unwrap();
        let ids = [JobId::new(1), JobId::new(2), JobId::new(3), JobId::new(4)];
        let _locks: Vec<_> = ids
            .iter()
            .map(|id| folder.create_lock(id).unwrap())
            .collect();
        folder
            .write_record(&record(&ids[0], 5, JobState::Waiting))
            .unwrap();
        folder
            .write_record(&record(&ids[1], 2, JobState::Waiting))
            .unwrap();
        folder
            .write_record(&record(&ids[2], 1, JobState::Running))
            .unwrap();
        let mut jobs = MachineJobs::read(paths(dir.path()), 16);
        let order = |records: &[&JobRecord]| -> Vec<JobId> {
            records.iter().map(|record| record.id.clone()).collect()
        };

        // When
        let (running, waiting) = jobs.queue();
        let (running, waiting) = (order(&running), order(&waiting));
        std::fs::remove_file(dir.path().join(format!("jobs/{}.lock", ids[1]))).unwrap();
        folder
            .write_record(&record(&ids[3], 6, JobState::Waiting))
            .unwrap();
        jobs.read_job(&ids[1]);
        jobs.read_job(&ids[3]);

        // Then
        assert_eq!(running, vec![ids[2].clone()]);
        assert_eq!(waiting, vec![ids[1].clone(), ids[0].clone()]);
        let (_, waiting) = jobs.queue();
        assert_eq!(order(&waiting), vec![ids[0].clone(), ids[3].clone()]);
        assert_eq!((jobs.limit(), jobs.error()), (Some(4), None));
    }

    #[cfg(unix)]
    #[test]
    fn machine_jobs__should_name_the_folder_it_cannot_use() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let mut paths = paths(dir.path());
        paths.folder = dir.path().join("jobs");
        std::os::unix::fs::symlink(&real, &paths.folder).unwrap();

        // When
        let jobs = MachineJobs::read(paths, 16);
        let error = jobs.error().unwrap_or_default();

        // Then
        assert!(jobs.records().is_empty());
        assert!(error.contains("it is a symlink"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn machine_jobs_read_job__should_read_every_job_when_the_folder_is_refused() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let folder = JobFolder::create(dir.path().join("jobs")).unwrap();
        let id = JobId::new(3);
        let _lock = folder.create_lock(&id).unwrap();
        folder
            .write_record(&record(&id, 1, JobState::Running))
            .unwrap();
        let mut jobs = MachineJobs::read(paths(dir.path()), 16);
        let before = jobs.records().len();
        std::fs::rename(dir.path().join("jobs"), dir.path().join("real")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("jobs")).unwrap();

        // When
        jobs.read_job(&id);

        // Then
        let error = jobs.error().unwrap_or_default();
        assert_eq!(before, 1);
        assert!(jobs.records().is_empty());
        assert!(error.contains("it is a symlink"), "{error}");
    }

    #[test]
    fn machine_jobs__should_name_the_paths_it_read() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let paths = paths(dir.path());

        // When
        let jobs = MachineJobs::read(paths.clone(), 16);

        // Then
        assert_eq!(jobs.paths(), &paths);
    }

    #[test]
    fn machine_jobs__should_keep_the_defaults_and_name_a_bad_config() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        std::fs::write(
            dir.path().join("config/machine.toml"),
            "[jobs]\nlimit = 0\n",
        )
        .unwrap();

        // When
        let mut jobs = MachineJobs::read(paths(dir.path()), 16);
        let broken = (jobs.limit(), jobs.error());
        std::fs::write(
            dir.path().join("config/machine.toml"),
            "[jobs]\nlimit = 2\n",
        )
        .unwrap();
        jobs.read_config();

        // Then
        assert_eq!(broken.0, None);
        assert!(broken.1.is_some_and(|error| error.contains("limit = 0")));
        assert_eq!((jobs.limit(), jobs.error()), (Some(2), None));
        assert_eq!(jobs.config().default_limit.get(), 4);
    }
}
