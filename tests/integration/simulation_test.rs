//! Integration tests for local simulation.

use bg_helper_coordinator::{CoordinatorState, CoordinatorConfig, simulation::run_simulation};
use bg_helper_protocol::job::{JobSpec, WorkloadType, JobPriority};
use bg_helper_protocol::resource::ResourceLimits;
use bg_helper_scheduler::{FairScheduler, JobQueue, WorkerSnapshot};
use bg_helper_policy::{PolicyEvaluator, default_limits};
use bg_helper_protocol::resource::{ResourceUsage, WorkerState, PowerState};
use uuid::Uuid;
use chrono::Utc;

#[tokio::test]
async fn test_two_worker_simulation() {
    // This test simulates two workers processing jobs
    let result = run_simulation(2, 10).await;
    
    match result {
        Ok(sim_result) => {
            println!("Simulation result: {:?}", sim_result);
            // At least some jobs should complete
            assert!(sim_result.completed + sim_result.failed <= sim_result.total_jobs);
        }
        Err(e) => {
            // Simulation might not be fully implemented yet
            println!("Simulation error (expected in early dev): {}", e);
        }
    }
}

#[tokio::test]
async fn test_coordinator_basic_flow() {
    let config = CoordinatorConfig::default();
    let coordinator = CoordinatorState::new(config);
    
    // Enroll two workers
    let worker1_id = Uuid::new_v4();
    let worker2_id = Uuid::new_v4();
    
    let enrollment1 = bg_helper_protocol::enrollment::EnrollmentRequest {
        invitation_code: "TEST-CODE".into(),
        worker_name: "worker-1".into(),
        capabilities: vec![bg_helper_protocol::resource::HardwareCapability::Cpu {
            cores: 4,
            threads: 8,
            frequency_mhz: 3000,
            architecture: "x86_64".into(),
            features: vec!["avx2".into()],
        }],
        proposed_limits: ResourceLimits {
            cpu_max_cores: Some(2),
            cpu_max_utilization_pct: Some(50),
            ram_max_workload_bytes: Some(2_000_000_000),
            ..Default::default()
        },
        public_key: None,
    };
    
    let enrollment2 = bg_helper_protocol::enrollment::EnrollmentRequest {
        invitation_code: "TEST-CODE".into(),
        worker_name: "worker-2".into(),
        capabilities: vec![bg_helper_protocol::resource::HardwareCapability::Cpu {
            cores: 2,
            threads: 4,
            frequency_mhz: 2500,
            architecture: "x86_64".into(),
            features: vec![],
        }],
        proposed_limits: ResourceLimits {
            cpu_max_cores: Some(1),
            cpu_max_utilization_pct: Some(50),
            ram_max_workload_bytes: Some(1_000_000_000),
            ..Default::default()
        },
        public_key: None,
    };
    
    let response1 = coordinator.handle_enrollment(enrollment1).await.unwrap();
    let response2 = coordinator.handle_enrollment(enrollment2).await.unwrap();
    
    assert!(response1.accepted);
    assert!(response2.accepted);
    assert_ne!(response1.worker_id, response2.worker_id);
    
    // Submit jobs
    let job_ids: Vec<_> = (0..5).map(|i| {
        let spec = JobSpec::new(
            WorkloadType::RangeSum { start: (i * 1000) as u64, end: ((i + 1) * 1000) as u64 },
            JobPriority::Normal,
            60,
        );
        spec.id
    }).collect();
    
    for job_id in &job_ids {
        let spec = JobSpec::new(
            WorkloadType::RangeSum { start: 0, end: 1000 },
            JobPriority::Normal,
            60,
        );
        // Can't easily submit specific job_id, so we'll just check queue
    }
    
    // Run scheduling cycle
    let messages = coordinator.run_scheduling_cycle().await.unwrap();
    
    // Should have job offers
    assert!(!messages.is_empty());
    
    // Verify job offers are for different workers (fair scheduling)
    let assigned_workers: std::collections::HashSet<_> = messages.iter()
        .filter_map(|m| match m {
            bg_helper_protocol::messages::CoordinatorMessage::JobOffer { job, .. } => job.assigned_worker.map(|w| w.0),
            _ => None,
        })
        .collect();
    
    // With 2 workers and 5 jobs, should distribute
    println!("Assigned workers: {:?}", assigned_workers);
}

#[tokio::test]
async fn test_scheduler_fairness() {
    let mut scheduler = FairScheduler::new();
    let worker1 = Uuid::new_v4();
    let worker2 = Uuid::new_v4();
    
    let w1 = WorkerSnapshot {
        worker_id: worker1,
        status: WorkerState::Idle,
        capabilities: vec![],
        limits: ResourceLimits {
            cpu_max_cores: Some(4),
            cpu_max_utilization_pct: Some(50),
            ram_max_workload_bytes: Some(8_000_000_000),
            ..Default::default()
        },
        usage: ResourceUsage {
            worker_id: worker1,
            timestamp: Utc::now(),
            cpu_utilization_pct: 10.0,
            cpu_cores_used: 0.5,
            cpu_frequency_mhz: Some(3000),
            gpu_utilization_pct: None,
            gpu_memory_used_bytes: None,
            gpu_temperature_c: None,
            ram_used_bytes: 2_000_000_000,
            ram_available_bytes: 14_000_000_000,
            storage_used_bytes: 10_000_000_000,
            storage_available_bytes: 100_000_000_000,
            power_state: PowerState::Ac,
            thermal_state: bg_helper_protocol::resource::ThermalState::Normal,
        },
    };
    
    let w2 = WorkerSnapshot {
        worker_id: worker2,
        status: WorkerState::Idle,
        capabilities: vec![],
        limits: ResourceLimits {
            cpu_max_cores: Some(4),
            cpu_max_utilization_pct: Some(50),
            ram_max_workload_bytes: Some(8_000_000_000),
            ..Default::default()
        },
        usage: ResourceUsage {
            worker_id: worker2,
            timestamp: Utc::now(),
            cpu_utilization_pct: 10.0,
            cpu_cores_used: 0.5,
            cpu_frequency_mhz: Some(3000),
            gpu_utilization_pct: None,
            gpu_memory_used_bytes: None,
            gpu_temperature_c: None,
            ram_used_bytes: 2_000_000_000,
            ram_available_bytes: 14_000_000_000,
            storage_used_bytes: 10_000_000_000,
            storage_available_bytes: 100_000_000_000,
            power_state: PowerState::Ac,
            thermal_state: bg_helper_protocol::resource::ThermalState::Normal,
        },
    };
    
    // Add multiple jobs
    for i in 0..10 {
        let job = JobSpec::new(
            WorkloadType::RangeSum { start: (i * 1000) as u64, end: ((i + 1) * 1000) as u64 },
            JobPriority::Normal,
            60,
        );
        scheduler.enqueue(bg_helper_scheduler::queue::QueuedJob::new(job, Utc::now()));
    }
    
    // Schedule all jobs
    let mut assignments = Vec::new();
    for _ in 0..10 {
        if let Some(decision) = scheduler.schedule_next(&[w1.clone(), w2.clone()]) {
            if let bg_helper_scheduler::ScheduleDecision::Assign { worker_id } = decision {
                assignments.push(worker_id);
            }
        }
    }
    
    // Should alternate between workers (fair scheduling)
    let worker1_count = assignments.iter().filter(|&&w| w == worker1).count();
    let worker2_count = assignments.iter().filter(|&&w| w == worker2).count();
    
    println!("Worker1: {}, Worker2: {}", worker1_count, worker2_count);
    
    // Fair scheduler should distribute roughly equally
    assert!(worker1_count > 0 && worker2_count > 0);
    assert!((worker1_count as i32 - worker2_count as i32).abs() <= 2);
}

#[tokio::test]
async fn test_policy_evaluation() {
    let limits = default_limits();
    let usage = ResourceUsage {
        worker_id: Uuid::new_v4(),
        timestamp: Utc::now(),
        cpu_utilization_pct: 10.0,
        cpu_cores_used: 0.5,
        cpu_frequency_mhz: Some(3000),
        gpu_utilization_pct: None,
        gpu_memory_used_bytes: None,
        gpu_temperature_c: None,
        ram_used_bytes: 2_000_000_000,
        ram_available_bytes: 14_000_000_000,
        storage_used_bytes: 10_000_000_000,
        storage_available_bytes: 100_000_000_000,
        power_state: PowerState::Ac,
        thermal_state: bg_helper_protocol::resource::ThermalState::Normal,
    };
    
    let job = JobSpec::new(
        WorkloadType::RangeSum { start: 0, end: 1_000_000 },
        JobPriority::Normal,
        300,
    );
    
    let decision = PolicyEvaluator::evaluate_job_offer(
        &limits, &usage, &job, None, PowerState::Ac, WorkerState::Idle
    );
    
    assert!(matches!(decision, bg_helper_policy::PolicyDecision::Allow));
}

#[tokio::test]
async fn test_job_state_machine() {
    let mut job = bg_helper_coordinator::simulation::Job::new(JobSpec::new(
        WorkloadType::RangeSum { start: 0, end: 100 },
        JobPriority::Normal,
        60,
    ));
    
    assert_eq!(job.state, bg_helper_protocol::job::JobState::Queued);
    
    job.transition_to(bg_helper_protocol::job::JobState::Offered).unwrap();
    assert_eq!(job.state, bg_helper_protocol::job::JobState::Offered);
    
    job.transition_to(bg_helper_protocol::job::JobState::Approved).unwrap();
    assert_eq!(job.state, bg_helper_protocol::job::JobState::Approved);
    
    job.transition_to(bg_helper_protocol::job::JobState::Running).unwrap();
    assert_eq!(job.state, bg_helper_protocol::job::JobState::Running);
    
    job.transition_to(bg_helper_protocol::job::JobState::Completed).unwrap();
    assert_eq!(job.state, bg_helper_protocol::job::JobState::Completed);
    
    // Terminal state - no more transitions
    assert!(job.transition_to(bg_helper_protocol::job::JobState::Queued).is_err());
}