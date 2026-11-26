//! Job queue implementation.

use bg_helper_protocol::job::{JobId, JobPriority, JobSpec};
use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use uuid::Uuid;

/// A job in the queue with metadata.
#[derive(Debug, Clone)]
pub struct QueuedJob {
    pub spec: JobSpec,
    pub queued_at: DateTime<Utc>,
    pub priority: JobPriority,
    pub assigned_worker: Option<Uuid>,
    pub offer_expires_at: Option<DateTime<Utc>>,
}

impl QueuedJob {
    pub fn new(spec: JobSpec, queued_at: DateTime<Utc>) -> Self {
        let priority = spec.priority;
        Self {
            spec,
            queued_at,
            priority,
            assigned_worker: None,
            offer_expires_at: None,
        }
    }
}

/// Job queue with priority ordering (highest priority first, then FIFO).
pub struct JobQueue {
    queued: Vec<QueuedJob>,
    assigned: Vec<QueuedJob>,
    completed: Vec<JobId>,
}

impl JobQueue {
    pub fn new() -> Self {
        Self {
            queued: Vec::new(),
            assigned: Vec::new(),
            completed: Vec::new(),
        }
    }

    /// Add a job to the queue (maintains priority order).
    pub fn enqueue(&mut self, job: QueuedJob) -> JobId {
        let job_id = job.spec.id;
        self.insert_sorted(job);
        job_id
    }

    fn insert_sorted(&mut self, job: QueuedJob) {
        // Higher priority value = higher priority (Critical=3, High=2, Normal=1, Low=0)
        // We want highest priority jobs first (at lower indices), then FIFO within same priority
        // FIFO means older jobs first, so for same priority: insert after jobs with earlier timestamps
        let pos = self
            .queued
            .iter()
            .position(|j| {
                j.priority < job.priority
                    || (j.priority == job.priority && j.queued_at > job.queued_at)
            })
            .unwrap_or(self.queued.len());
        self.queued.insert(pos, job);
    }

    /// Remove and return the highest priority queued job.
    pub fn dequeue(&mut self) -> Option<QueuedJob> {
        self.queued.first().cloned().map(|job| {
            self.queued.remove(0);
            job
        })
    }

    /// Return the next job without removing it.
    pub fn peek(&self) -> Option<&QueuedJob> {
        self.queued.first()
    }

    /// Mark a job as assigned to a worker.
    pub fn assign(&mut self, job_id: JobId, worker_id: Uuid, offer_duration_secs: u32) -> bool {
        if let Some(pos) = self.queued.iter().position(|j| j.spec.id == job_id) {
            let mut job = self.queued.remove(pos);
            job.assigned_worker = Some(worker_id);
            job.offer_expires_at =
                Some(Utc::now() + chrono::Duration::seconds(offer_duration_secs as i64));
            self.assigned.push(job);
            true
        } else {
            false
        }
    }

    /// Complete a job (remove from assigned).
    pub fn complete(&mut self, job_id: JobId) -> Option<QueuedJob> {
        self.assigned
            .iter()
            .position(|j| j.spec.id == job_id)
            .map(|pos| {
                let job = self.assigned.remove(pos);
                self.completed.push(job_id);
                job
            })
    }

    /// Expire old offers and return expired job IDs.
    pub fn expire_offers(&mut self) -> Vec<JobId> {
        let now = Utc::now();
        let mut expired = Vec::new();
        let mut i = 0;
        while i < self.assigned.len() {
            if self.assigned[i]
                .offer_expires_at
                .map(|e| e < now)
                .unwrap_or(false)
            {
                let job = self.assigned.remove(i);
                expired.push(job.spec.id);
                self.queued.push(job); // Re-queue for retry
            } else {
                i += 1;
            }
        }
        expired
    }

    /// Get all queued jobs (highest priority first).
    pub fn all_queued(&self) -> &[QueuedJob] {
        &self.queued
    }

    /// Get all assigned jobs.
    pub fn all_assigned(&self) -> &[QueuedJob] {
        &self.assigned
    }

    /// Get completed job IDs.
    pub fn completed(&self) -> &[JobId] {
        &self.completed
    }

    /// Get queue length.
    pub fn len(&self) -> usize {
        self.queued.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queued.is_empty()
    }

    /// Get assigned queue length.
    pub fn assigned_len(&self) -> usize {
        self.assigned.len()
    }

    /// Requeue an assigned job back to the queued list (e.g., on worker rejection).
    pub fn requeue_assigned(&mut self, job_id: JobId) -> bool {
        if let Some(pos) = self.assigned.iter().position(|j| j.spec.id == job_id) {
            let mut job = self.assigned.remove(pos);
            job.assigned_worker = None;
            job.offer_expires_at = None;
            self.insert_sorted(job);
            true
        } else {
            false
        }
    }

    /// Clear completed history.
    pub fn clear_completed(&mut self) {
        self.completed.clear();
    }
}

impl Default for JobQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::job::{JobPriority, JobSpec, WorkloadType};
    use chrono::Utc;
    use uuid::Uuid;

    fn test_job(workload: WorkloadType, priority: JobPriority) -> JobSpec {
        let mut job = JobSpec::new(workload, priority, 300);
        job
    }

    #[test]
    fn test_queue_fifo_same_priority() {
        let mut queue = JobQueue::new();
        let now = Utc::now();

        let job1 = test_job(
            WorkloadType::RangeSum { start: 0, end: 100 },
            JobPriority::Normal,
        );
        let job2 = test_job(
            WorkloadType::RangeSum {
                start: 100,
                end: 200,
            },
            JobPriority::Normal,
        );

        queue.enqueue(QueuedJob::new(job1.clone(), now));
        queue.enqueue(QueuedJob::new(
            job2.clone(),
            now + chrono::Duration::seconds(1),
        ));

        // First in should be first out (since same priority)
        let first = queue.dequeue().unwrap();
        assert_eq!(first.spec.id, job1.id);

        let second = queue.dequeue().unwrap();
        assert_eq!(second.spec.id, job2.id);
    }

    #[test]
    fn test_queue_priority_order() {
        let mut queue = JobQueue::new();
        let now = Utc::now();

        let job_low = test_job(
            WorkloadType::RangeSum { start: 0, end: 100 },
            JobPriority::Low,
        );
        let job_high = test_job(
            WorkloadType::RangeSum {
                start: 100,
                end: 200,
            },
            JobPriority::High,
        );

        queue.enqueue(QueuedJob::new(job_low.clone(), now));
        queue.enqueue(QueuedJob::new(job_high.clone(), now));

        // High priority should come first
        let first = queue.dequeue().unwrap();
        assert_eq!(first.spec.id, job_high.id);
        assert_eq!(first.priority, JobPriority::High);
    }

    #[test]
    fn test_queue_assign_and_complete() {
        let mut queue = JobQueue::new();
        let now = Utc::now();
        let worker_id = Uuid::new_v4();

        let job = test_job(
            WorkloadType::RangeSum { start: 0, end: 100 },
            JobPriority::Normal,
        );
        let job_id = job.id;
        queue.enqueue(QueuedJob::new(job, now));

        // Assign
        assert!(queue.assign(job_id, worker_id, 60));
        assert_eq!(queue.len(), 0);
        assert_eq!(queue.assigned_len(), 1);

        // Complete
        let completed = queue.complete(job_id).unwrap();
        assert_eq!(completed.spec.id, job_id);
        assert_eq!(queue.assigned_len(), 0);
        assert_eq!(queue.completed().len(), 1);
    }

    #[test]
    fn test_queue_expire_offers() {
        let mut queue = JobQueue::new();
        let worker_id = Uuid::new_v4();

        let job = test_job(
            WorkloadType::RangeSum { start: 0, end: 100 },
            JobPriority::Normal,
        );
        let job_id = job.id;
        queue.enqueue(QueuedJob::new(job, Utc::now()));

        // Assign with very short duration
        queue.assign(job_id, worker_id, 0);

        // Should be expired
        let expired = queue.expire_offers();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0], job_id);
        assert_eq!(queue.len(), 1); // Re-queued
    }
}
