# BackgroundHelper Architecture

## Overview

BackgroundHelper is a distributed computing platform with a coordinator-worker architecture. The coordinator manages job scheduling and cluster state, while workers execute jobs under owner-defined resource policies.

## Component Diagram

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Administrator │────▶│   Coordinator   │◀───▶│     Worker      │
│   (Dashboard)   │     │                 │     │  (Owner Device) │
└─────────────────┘     └────────┬────────┘     └────────┬────────┘
                                 │                       │
                    ┌────────────┼────────────┐          │
                    ▼            ▼            ▼          ▼
             ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
             │ Protocol │ │ Scheduler│ │  Policy  │ │ Registry │
             │          │ │          │ │          │ │          │
             └──────────┘ └──────────┘ └──────────┘ └──────────┘
```

## Crates

### `bg-helper-protocol`
Shared types for all communication:
- Job specifications, states, results
- Resource descriptions, limits, usage
- Enrollment messages
- Coordinator↔Worker message types
- Validation logic

### `bg-helper-policy`
Pure local resource policy evaluation:
- `PolicyEvaluator`: Decides if jobs can run given limits/usage
- `ResourceMonitor`: Collects system metrics via `sysinfo`
- `LimitsBuilder`: Interactive limit configuration
- No network dependencies

### `bg-helper-scheduler`
Pure job placement and fairness:
- `FairScheduler`: Virtual-time based fair scheduling
- `JobQueue`: Priority-ordered job queue
- `PlacementStrategy`: Best-fit, least-loaded, hardware-match
- No I/O, fully testable

### `bg-helper-coordinator`
Cluster management:
- `CoordinatorState`: Job queue, worker registry, scheduling loop
- `WorkerRegistry`: Invitation codes, enrollment
- `Api`: REST API for dashboards
- `Simulation`: Local testing with simulated workers

### `bg-helper-worker`
Owner-facing workload manager:
- `ConfigManager`: Persistent configuration (TOML)
- `EnrollmentManager`: Invitation codes, QR codes
- `ResourceMonitor`: System metrics collection
- `JobExecutor`: Runs approved workloads
- `Runtime`: Main event loop, owner controls

## Data Flow

### Job Submission
1. Administrator submits job via dashboard → Coordinator API
2. Coordinator validates job spec → adds to `JobQueue`
3. Scheduler picks job, scores workers → `ScheduleDecision::Assign`
4. Coordinator sends `JobOffer` to selected worker
5. Worker evaluates offer against local policy → accepts/rejects
6. If accepted: Coordinator marks job `Approved` → `Running`
7. Worker executes job → sends `JobComplete` with result
8. Coordinator verifies result → marks job `Completed`

### Enrollment
1. Admin generates invitation code → shares with owner
2. Owner enters code in worker UI → `EnrollmentRequest`
3. Coordinator validates code, merges limits → `EnrollmentResponse`
4. Worker stores assigned limits, begins heartbeats

### Resource Policy
- **Owner controls**: Max cores, utilization %, RAM, storage, schedule, battery
- **Worker enforces**: PolicyEvaluator runs locally before each job
- **Coordinator respects**: Scheduler checks limits before offering
- **Adaptive**: Worker reduces usage when foreground apps need resources

## Security Model

- No arbitrary code execution - only predefined workloads
- Worker policy is authoritative - coordinator cannot override
- Revocation is immediate - worker stops current job
- No persistent background processes without owner consent
- All communication authenticated (future: mTLS)

## Deployment

```
Development:     Single process, simulated workers
Testing:         Developer's own devices (RX580, MacBook Pro)
Production:      Coordinator on admin's machine, workers on volunteers' devices
                 Private network (Tailscale, WireGuard, LAN)
```

## Future Extensions

- GPU workloads (CUDA, ROCm, Vulkan, Metal)
- Distributed compilation (distcc/sccache integration)
- AI inference/training (data parallel)
- App-owned storage with quotas
- Coordinator migration with owner consent