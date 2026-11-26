# BackgroundHelper Threat Model

## Assets

| Asset | Description | Sensitivity |
|-------|-------------|-------------|
| Owner's compute resources | CPU, GPU, RAM, storage | High |
| Owner's personal files | Documents, photos, credentials | Critical |
| Job data | Input/output of computations | Medium |
| Coordinator admin credentials | Cluster control | High |
| Enrollment tokens | Worker authorization | High |
| Audit logs | Job history, worker activity | Medium |

## Trust Boundaries

```
┌─────────────────────────────────────────────────────────────┐
│                    OWNER'S DEVICE                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │   Worker     │  │  Owner UI    │  │  Personal Files  │  │
│  │  (Trusted)   │  │  (Trusted)   │  │   (OFF LIMITS)   │  │
│  └──────┬───────┘  └──────────────┘  └──────────────────┘  │
│         │                                                    │
│         ▼                                                    │
│  ┌──────────────┐                                           │
│  │ Local Policy │◀── Owner-defined, authoritative           │
│  │  Enforcement │                                           │
│  └──────────────┘                                           │
└─────────────────┬───────────────────────────────────────────┘
                  │ Network (Authenticated TLS)
                  ▼
┌─────────────────────────────────────────────────────────────┐
│                   COORDINATOR                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │  Scheduler   │  │ Job Registry │  │ Worker Registry  │  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
│         │                                                     │
│         ▼                                                     │
│  ┌──────────────┐                                           │
│  │ Admin Dashboard│  (Admin only)                            │
│  └──────────────┘                                           │
└─────────────────────────────────────────────────────────────┘
```

## Threat Actors

| Actor | Motivation | Capability |
|-------|------------|------------|
| Malicious admin | Steal compute, access data | Full coordinator control |
| Compromised coordinator | Lateral movement | Network access to workers |
| Malicious worker owner | Sabotage, data exfil | Local root access |
| Network attacker | MITM, DoS | Network position |
| Malicious job submitter | Resource exhaustion | Job submission access |

## Threat Analysis

### T1: Coordinator Compromise
**Threat**: Attacker gains control of coordinator, sends malicious jobs.
**Impact**: Workers execute unauthorized workloads.
**Mitigations**:
- Worker validates every job against local policy (authoritative)
- No arbitrary code execution - only predefined workloads
- Job parameters validated (bounded ranges, sizes)
- Revocation immediately stops all work
- Coordinator cannot access owner files

### T2: Malicious Job Parameters
**Threat**: Admin submits job with excessive resource requirements.
**Impact**: Resource exhaustion, DoS on worker.
**Mitigations**:
- Protocol validation: bounded input sizes (max 1B range, 10M primes, 10K matrix)
- Worker policy limits: max cores, RAM, utilization %
- Scheduler checks limits before offering
- Job timeout enforcement (max_duration_secs)

### T3: Worker Impersonation
**Threat**: Attacker pretends to be enrolled worker.
**Impact**: Fake resource reports, result manipulation.
**Mitigations**:
- Enrollment uses invitation codes (single-use, expiring)
- Future: mTLS with certificate pinning
- Heartbeat includes resource usage for anomaly detection
- Coordinator tracks worker identity

### T4: Result Tampering
**Threat**: Worker returns incorrect results.
**Impact**: Invalid computation results.
**Mitigations**:
- Deterministic workloads enable verification
- Coordinator can re-run on different worker (future)
- Result format validation
- Audit log of all job results

### T5: Network Eavesdropping
**Threat**: Attacker intercepts coordinator-worker communication.
**Impact**: Job data exposure, enrollment token theft.
**Mitigations**:
- TLS for all communication (production)
- No sensitive data in job parameters (only ranges, sizes)
- Invitation codes expire, single-use
- Private network deployment (Tailscale, WireGuard)

### T6: Owner Privacy Violation
**Threat**: Worker collects personal data beyond resource metrics.
**Impact**: Privacy breach.
**Mitigations**:
- ResourceMonitor only collects: CPU%, RAM, disk, power, thermal
- No file system access, no process listing, no network monitoring
- Open source - auditable
- Owner can inspect all collected data in UI

### T7: Unauthorized Background Execution
**Threat**: Worker runs without owner consent.
**Impact**: Resource theft, trust violation.
**Mitigations**:
- No auto-start by default (config option, opt-in)
- Owner must explicitly approve enrollment
- Pause/Revocation always wins (immediate stop)
- Visible in OS process list, identifiable name
- Tray/icon indicator when active

### T8: Coordinator Migration Attack
**Threat**: Admin silently transfers cluster to new coordinator.
**Impact**: Owner unknowingly serves new admin.
**Mitigations**:
- Migration requires explicit owner consent (future)
- Worker validates coordinator identity
- Owner sees admin identity in UI
- No automatic trust transfer

### T9: Resource Exhaustion via Legitimate Jobs
**Threat**: Many valid jobs overwhelm worker.
**Impact**: System slowdown, owner frustration.
**Mitigations**:
- Owner-defined concurrent job limits (future: max 1 for v0.1)
- Adaptive policy: reduces usage when foreground apps active
- Schedule limits: only run during allowed hours
- Battery/thermal protection

### T10: Supply Chain Attack
**Threat**: Compromised dependency or build.
**Impact**: Malicious code in worker/coordinator.
**Mitigations**:
- Minimal dependencies, pinned versions
- Cargo.lock committed
- Reproducible builds (future)
- Signed releases (future)
- Dependency auditing (cargo-audit)

## Security Requirements

| Requirement | Implementation |
|-------------|----------------|
| No arbitrary code execution | Predefined workload types only |
| Worker policy authoritative | PolicyEvaluator runs locally |
| Immediate revocation | Worker stops on RevocationNotice |
| Owner consent for enrollment | Invitation code + local approval |
| Visible operation | Named process, UI indicator |
| No personal data access | ResourceMonitor scope limited |
| Authenticated communication | TLS (production) |
| Bounded resource usage | Protocol + policy validation |
| Audit trail | Job history, state transitions |

## Residual Risks

| Risk | Likelihood | Impact | Acceptance |
|------|------------|--------|------------|
| Zero-day in sysinfo/tokio | Low | Medium | Accepted - monitored |
| Side-channel attacks on CPU | Low | Low | Accepted - not high-value target |
| Coordinator DoS | Medium | Medium | Rate limiting (future) |
| Worker hardware fingerprinting | Medium | Low | Accepted - voluntary |

## Security Testing

- Unit tests for policy evaluation edge cases
- Integration tests for revocation during job execution
- Fuzzing of protocol message parsing
- Dependency vulnerability scanning (cargo-audit)
- Penetration testing before v1.0

## Incident Response

1. **Worker compromise**: Owner revokes → immediate stop
2. **Coordinator compromise**: Workers detect policy violations → auto-revoke
3. **Network breach**: Rotate TLS certs, re-enroll workers
4. **Vulnerability disclosed**: Emergency release, auto-update (opt-in)