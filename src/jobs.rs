use std::process::Child;
use std::sync::{Mutex, OnceLock};

/// Whether a tracked background job is still running or has exited.
#[derive(Clone, Copy, PartialEq)]
pub enum JobStatus {
    Running,
    Done,
}

/// A snapshot of a background job, as reported by the `jobs` builtin.
pub struct JobSnapshot {
    pub id: usize,
    pub command: String,
    pub status: JobStatus,
}

/// A background job being tracked by the shell. The child handle lets the
/// shell poll (without blocking) whether the process has exited yet.
struct Job {
    id: usize,
    child: Child,
    command: String,
}

/// Background jobs known to the shell, in job-number order (the most recently
/// added job is last). Job numbers are recycled: when the table is empty the
/// next job gets [1], otherwise one more than the highest number in the table.
static JOBS: OnceLock<Mutex<Vec<Job>>> = OnceLock::new();

/// Adds a background job to the shell's job table and returns its job number.
pub fn add_job(child: Child, command: String) -> usize {
    let mut list = JOBS.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
    let id = list.iter().map(|job| job.id).max().map_or(1, |max| max + 1);
    list.push(Job { id, child, command });
    id
}

/// Checks every tracked job for completion and returns snapshots of all of
/// them. Jobs that have exited are reported with status `Done` and removed
/// from the table, so a later reap won't report them again.
pub fn reap() -> Vec<JobSnapshot> {
    let mut list = JOBS.get_or_init(|| Mutex::new(Vec::new())).lock().unwrap();
    let mut snapshots = Vec::with_capacity(list.len());
    let mut done_indexes = Vec::new();
    for (index, job) in list.iter_mut().enumerate() {
        // Non-blocking check: Ok(Some(_)) means the child exited and was
        // reaped; Ok(None) means it is still running.
        let done = matches!(job.child.try_wait(), Ok(Some(_)));
        if done {
            done_indexes.push(index);
        }
        snapshots.push(JobSnapshot {
            id: job.id,
            command: job.command.clone(),
            status: if done {
                JobStatus::Done
            } else {
                JobStatus::Running
            },
        });
    }
    // Remove finished jobs (in reverse so indexes stay valid).
    for index in done_indexes.into_iter().rev() {
        list.remove(index);
    }
    snapshots
}
