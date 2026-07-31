use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// A background job tracked by the shell.
#[derive(Clone)]
pub struct Job {
    pub id: usize,
    // Used once the shell reaps finished jobs (later stage).
    #[allow(dead_code)]
    pub pid: u32,
    pub command: String,
}

/// Next job number to assign to a background job. Starts at 1 and increments
/// with each background job started by the shell.
static NEXT_JOB_ID: AtomicUsize = AtomicUsize::new(1);

/// Background jobs known to the shell, in job-number order (the most recently
/// added job is last).
static JOBS: OnceLock<Mutex<Vec<Job>>> = OnceLock::new();

/// Adds a background job to the shell's job table and returns its job number.
pub fn add_job(pid: u32, command: String) -> usize {
    let id = NEXT_JOB_ID.fetch_add(1, Ordering::SeqCst);
    JOBS.get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .push(Job { id, pid, command });
    id
}

/// The background jobs currently known to the shell, oldest first.
pub fn list_jobs() -> Vec<Job> {
    JOBS.get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .clone()
}
