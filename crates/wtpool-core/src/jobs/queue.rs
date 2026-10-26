//! [`Place`]: a job's ticket, lock and record, and its turn to run once fewer jobs than the limit
//! run and no live job with a lower ticket waits.

use std::fs::File;

use super::config::Limit;
use super::folder::{FolderError, JobFolder};
use super::record::{JobId, JobRecord, JobState};
use crate::clock::Timestamp;

/// A job in the queue: its lock, held until it leaves, and its record.
#[derive(Debug)]
pub struct Place {
    folder: JobFolder,
    lock: Option<File>,
    record: JobRecord,
}

/// Whether a job may run now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    Run,
    Wait(Standing),
}

/// The queue as a waiting job sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    /// Live jobs that run.
    pub running: usize,
    /// Live jobs that wait with a lower ticket.
    pub ahead: usize,
}

impl Place {
    /// Queues `record`'s job under the queue lock: takes the next ticket, creates and locks the
    /// job's lock file, and only then writes the record, running when its turn came, else
    /// waiting. `record`'s ticket, state and times are set here.
    pub fn join(
        folder: JobFolder,
        mut record: JobRecord,
        limit: Limit,
        now: Timestamp,
    ) -> Result<(Self, Turn), FolderError> {
        let queue = folder.lock_queue()?;
        let live = folder.sweep(&queue, None);
        record.ticket = folder.take_ticket(&queue, &live)?;
        let lock = folder.create_lock(&record.id)?;
        let turn = turn_of(&live, record.ticket, limit);
        record.state = match turn {
            Turn::Run => JobState::Running,
            Turn::Wait(_) => JobState::Waiting,
        };
        record.queued = now;
        record.since = now;
        let place = Self {
            folder,
            lock: Some(lock),
            record,
        };
        place.folder.write_record(&place.record)?;
        drop(queue);
        Ok((place, turn))
    }

    /// Checks again under the queue lock whether this waiting job may run under `limit`, and
    /// writes its record running from `now` once it may.
    pub fn check(&mut self, limit: Limit, now: Timestamp) -> Result<Turn, FolderError> {
        let queue = self.folder.lock_queue()?;
        let live = self.folder.sweep(&queue, Some(&self.record.id));
        let turn = turn_of(&live, self.record.ticket, limit);
        if turn == Turn::Run {
            self.record.state = JobState::Running;
            self.record.since = now;
            self.folder.write_record(&self.record)?;
        }
        Ok(turn)
    }

    #[must_use]
    pub fn id(&self) -> &JobId {
        &self.record.id
    }

    /// Deletes the record, then the lock file, and frees the lock.
    pub fn leave(mut self) {
        self.release();
    }

    fn release(&mut self) {
        if let Some(lock) = self.lock.take() {
            self.folder.delete(&self.record.id);
            drop(lock);
        }
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        self.release();
    }
}

/// The turn of the job holding `ticket`, among the other `live` jobs.
fn turn_of(live: &[JobRecord], ticket: u64, limit: Limit) -> Turn {
    let running = live
        .iter()
        .filter(|record| record.state == JobState::Running)
        .count();
    let ahead = live
        .iter()
        .filter(|record| record.state == JobState::Waiting && record.ticket < ticket)
        .count();
    let free = usize::try_from(limit.get()).is_ok_and(|limit| running < limit);
    if ahead == 0 && free {
        Turn::Run
    } else {
        Turn::Wait(Standing { running, ahead })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rstest::rstest;

    use super::*;
    use crate::jobs::record::RECORD_VERSION;

    fn record(ticket: u64, state: JobState) -> JobRecord {
        JobRecord {
            version: RECORD_VERSION,
            id: JobId::new(7),
            pid: 7,
            started: None,
            summary: "make".to_owned(),
            label: None,
            cwd: PathBuf::from("/w"),
            ticket,
            state,
            queued: Timestamp::from_seconds(0),
            since: Timestamp::from_seconds(0),
        }
    }

    #[rstest]
    #[case::a_free_place(&[(1, JobState::Running)], 2, Turn::Run)]
    #[case::no_free_place(&[(1, JobState::Running), (2, JobState::Running)], 2, Turn::Wait(Standing { running: 2, ahead: 0 }))]
    #[case::a_lower_ticket_waits(&[(2, JobState::Waiting)], 2, Turn::Wait(Standing { running: 0, ahead: 1 }))]
    #[case::a_higher_ticket_waits(&[(9, JobState::Waiting)], 1, Turn::Run)]
    #[case::over_a_lowered_limit(&[(1, JobState::Running), (2, JobState::Running), (3, JobState::Running)], 1, Turn::Wait(Standing { running: 3, ahead: 0 }))]
    fn turn_of__should_run_the_lowest_ticket_while_a_place_is_free(
        #[case] others: &[(u64, JobState)],
        #[case] limit: i64,
        #[case] expected: Turn,
    ) {
        // Given
        let live: Vec<JobRecord> = others
            .iter()
            .map(|(ticket, state)| record(*ticket, *state))
            .collect();

        // When
        let turn = turn_of(&live, 5, Limit::new(limit).unwrap());

        // Then
        assert_eq!(turn, expected);
    }
}
