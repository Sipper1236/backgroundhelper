# BackgroundHelper — Product Requirements and Development Specification

## 1. Purpose

BackgroundHelper is an open-source distributed computing platform for voluntarily contributed Windows and Linux computers. Its purpose is to help a single cluster administrator coordinate useful computing jobs across participating machines without interfering with their owners' normal work. Initial use cases include distributed compilation, numerical benchmarks, and independent scientific computing tasks. Longer-term goals include supported AI workloads and distributed artifact storage.

This document is an implementation brief for a coding assistant or development team. Build a small, testable system first; do not interpret the long-term roadmap as permission to implement unrestricted remote execution.

## 2. Product decisions

| Area | Requirement |
|---|---|
| Operating systems | Windows and Linux |
| Administrator | One designated cluster administrator |
| Coordinator | Portable between the administrator's own computers |
| User interfaces | Desktop and web dashboards |
| Network | Hybrid; local networks and explicitly configured private networks |
| Enrollment | Invitation codes, QR codes, and accounts |
| Resource types | CPU, GPU, RAM, and dedicated storage |
| Resource policy | Adaptive allocation within owner-defined limits |
| Job types | Approved predefined workloads first; isolated workloads later following security review |
| Offline behavior | Finish currently approved, bounded jobs, then pause |
| Distribution | Windows installer and portable distribution; Linux packages |
| Updates | Automatic signed stable releases after owner opt-in, with visible history, opt-out and recovery |
| Development | Worker and coordinator developed in parallel |
| License | Apache-2.0 proposed; confirm before publishing |

## 3. Trust and product boundaries

The administrator manages cluster work, not the computer owner's personal device. Joining the cluster must be voluntary and informed. The owner must see the identity of the administrator, the kinds of jobs supported, the resource ceilings, and the duration of sharing before accepting enrollment. The worker must provide a readily accessible local status and stop control. A quiet application may avoid routine pop-ups, but must not disguise its identity, conceal usage, imitate system processes, evade monitoring, or continue after revocation.

Do not implement arbitrary remote shell commands, remote installation of software, credential collection, personal file browsing, remote desktop control, hidden persistence, or mechanisms that bypass the owner's permissions. SSH configuration, if ever needed, should remain a separate owner-managed workflow using standard operating-system tools, outside this application. The initial product provides configuration through a limited typed settings interface, not a shell.

The administrator cannot raise a worker's locally approved resource ceilings or override pause, battery, thermal, or schedule restrictions. Revocation takes priority over finishing any current job. Owners can uninstall without contacting the administrator. Use ordinary application naming and clearly identify the publisher. Never ask participants to disable antivirus or operating-system protections.

## 4. Architecture and component responsibilities

Use a Rust workspace with separate coordinator and worker components. The dashboards are clients of the coordinator's application interface. The worker is responsible for enforcing owner policy, not merely trusting the coordinator's decisions. The coordinator maintains job state, resource advertisements, and an audit history; it must not have access to personal files on workers.

Proposed technologies, subject to compatibility and dependency review: Rust, Tokio, Axum, SQLite, React/TypeScript, Tauri, and sysinfo. Keep the initial prototype local to the developer's own machines or a simulated cluster. Use ordinary authenticated transport libraries rather than inventing cryptography. For production connectivity, prefer an already configured private network and authenticated TLS; postpone relay infrastructure until the trust design is reviewed.

Suggested repository layout:

```text
BackgroundHelper/
  crates/
    protocol/       # Typed messages, versions, validation
    policy/         # Pure local resource-policy evaluation
    scheduler/      # Pure job placement and fairness logic
    coordinator/    # Job state and dashboard interface
    worker/         # Owner-visible local workload manager
  apps/
    web/            # Administrator and owner views
    desktop/        # Desktop UI prototype
  tests/
    fixtures/       # Synthetic hardware and job fixtures
    integration/    # Local-only integration tests
  docs/
    architecture.md
    threat-model.md
    owner-guide.md
    development.md
  Cargo.toml
  README.md
  LICENSE
```

## 5. First milestone: local simulation and approved CPU jobs

Start with two simulated workers in a single development process. Implement deterministic numerical workloads with bounded input sizes, for example summing disjoint ranges of integers or computing independent matrix tiles from synthetic data. The coordinator combines results and verifies correctness against a known reference. No submitted scripts, executable payloads, dynamic plugins, filesystem paths, or shell fragments are accepted by this initial job system.

The first real-device demonstration should use only the developer's own RX580 desktop and MacBook Pro, with both devices visibly enrolled and each workload locally approved. Keep the test duration bounded. Do not distribute installers to other people until security and owner controls have been independently reviewed.

### Deliverables

- Shared typed job and resource models with validation.
- Pure scheduler and policy modules with unit tests.
- Local simulation supporting two workers, bounded CPU tasks, cancellation, and deterministic result verification.
- Coordinator job history and a dashboard backed by simulated data, then local test data.
- Owner-facing enrollment and resource settings UI mockups.
- Documentation explaining exactly what data is collected and what jobs do.

## 6. Worker resource policy

Resource advertisements describe available capacity, not a guarantee of allocation. Keep advertised hardware capacity separate from authorized capacity. The worker's local policy is authoritative.

| Resource | Owner controls | Adaptive behavior |
|---|---|---|
| CPU | Maximum cores or utilization target | Reduce workload when foreground use rises |
| GPU | Explicit opt-in, approved device and budget | Pause when gaming, rendering, or thermal pressure is detected |
| RAM | Maximum workload memory | Refuse new jobs when headroom is low |
| Storage | Dedicated workspace and quota | Refuse writes beyond quota; clean only app-owned artifacts |
| Time | Sharing schedule and session duration | Pause outside authorized hours |
| Power | Battery and AC preferences | Default to no heavy computation on battery |

Resource limits are workload-specific and platform-dependent. Do not claim that every platform can enforce exact GPU percentages. If reliable enforcement is unavailable, keep that workload disabled. Do not use system-wide changes, forced process termination of unrelated applications, or elevated privileges to free resources.

Recommended conservative defaults for the prototype: CPU-only, explicit start, one bounded job at a time, no GPU or storage contribution until enabled, and no automatic enrollment. Treat all numerical defaults as proposals to be validated by the owner.

## 7. Job lifecycle

A job is a declared computation with an identifier, supported workload type, validated parameters, resource estimate, maximum duration, and expected result format. Jobs contain data, not arbitrary commands. Model these states: queued, offered, approved, running, completed, failed, cancelled, expired. State transitions should be explicit and testable.

The scheduler must consider owner policy, compatible hardware, estimated resources, current load, and job deadlines. A disconnected worker must not receive additional work. An approved in-progress job may finish within its original deadline if the owner's policy still allows it; otherwise it stops. A revoked worker stops and does not accept new work. Avoid infinite retry loops and duplicate result accounting.

## 8. Dashboard requirements

Administrator web and desktop dashboards should present three equally important areas: hardware monitoring, job management, and permitted configuration. Show the last update time for each device, distinguish offline from idle, and distinguish hardware capacity from the resources actually offered. Display workload types, elapsed time, result verification status, failures, and cancellation state. Do not present aggregate RAM or VRAM as a single directly addressable memory pool.

The owner interface must show current job, administrator identity, current resource consumption, configured ceilings, schedule, recent activity, and a clearly visible pause/revoke control. Enrollment must explain resource use in plain language. The application should remain identifiable in normal OS management interfaces. Any background execution preference belongs to the device owner.

## 9. Enrollment and configuration requirements

Invitation codes, QR codes, and accounts are alternative ways to initiate an enrollment request, not substitutes for local consent. QR codes must not embed reusable secrets. Design the screens and data models first; production identity, credential storage, expiration, and replay protection require separate review. Use established authentication libraries and secure OS credential storage. Do not implement custom cryptographic schemes.

Configuration changes should be limited to typed, documented application settings. The worker must reject settings outside local authorization, unknown fields, and unsupported operations. Display remote-origin configuration changes in the owner activity history. No arbitrary scripts or executable content in configuration messages.

Coordinator migration is a future feature: owners must be able to verify the identity of the new coordinator and approve any trust change. Do not silently transfer administrator authority between computers.

## 10. Workload roadmap

### Distributed compilation

Investigate existing, documented distributed build tools. Compiler compatibility matters: architecture, target, compiler version, sysroot, and flags must match. Windows and Linux workers cannot automatically substitute for each other. Start with read-only compatibility reporting and local simulations. Any actual cross-machine compilation integration should use separately configured, owner-authorized tooling and a dedicated build workspace. Do not expose an unrestricted compiler daemon to public networks or use it as a general command runner.

### AI inference and training

Initially support planning and benchmarking of independent, known workloads on developer-owned GPUs. Detect advertised GPU capabilities and available memory without modifying drivers. Vulkan, ROCm, and CUDA are different environments; do not assume portability. Later evaluate data-parallel jobs or independent inference requests using established frameworks and dedicated, isolated environments. Distributed model execution is a separate engineering problem involving interconnect bandwidth, synchronization, and memory placement.

### Storage

Offer only app-owned, explicitly selected scratch storage, never an entire drive or home directory. Define quotas, retention, and cleanup ownership. Simulate storage scheduling before implementing network transfer. Do not collect or index personal files.

## 11. Distribution and automatic updates

The product should eventually have conventional installers and portable distributions with clear publisher identity, release notes, and uninstall instructions. A portable client should run only when started by its owner. Automatic updates are a later milestone after independent security review; prefer established signed package ecosystems or reviewed update frameworks rather than custom remote execution mechanisms.

### Automatic update acceptance requirements

- Offer automatic stable-channel updates as an explicit owner-selected installation preference. Provide an accessible setting to disable them or switch to manual approval.
- Publish release notes, version numbers, publisher identity and supported platforms. Show the installed version and update history in the owner interface.
- Require trusted publisher signatures and integrity verification. Reject unsigned, modified, untrusted or unauthorized releases and retain the working installation.
- Schedule approved updates outside active jobs and within owner-approved maintenance windows. Routine updates need not interrupt the owner with modal dialogs.
- Preserve resource limits, enrollment choices, sharing schedules and revocation state across updates. Require fresh local consent for any expanded permission or resource access.
- Support recovery to a known-good, authorized release after failed updates, subject to reviewed rollback protections.
- Keep update status and errors visible in the management interface. Never use updates to reinstall an uninstalled worker, restore revoked access, or conceal changes from the owner.
- Test invalid signatures, interrupted updates, insufficient disk space, active jobs, disabled updates and failed recovery before release.

### Quiet, transparent operation

A permanent tray icon is not required if the owner chooses a windowless experience. Provide an identifiable installed application, an accessible local settings interface, resource and job history, and immediate pause, revoke and uninstall controls. Avoid routine pop-ups, but communicate important security, permission and update failures. Do not hide processes, disguise the application's identity, obscure resource consumption, or evade operating-system security and management tools.

## 12. Testing strategy

Write unit tests before adding actual resource execution. Test scheduler decisions using simulated workers and synthetic inputs. Test result verification, cancellation, timeouts, duplicate jobs, stale advertisements, and disconnected coordinators. Test rejection of unknown workload types, invalid sizes, and out-of-policy resource requests. Test that local pause and revocation always win over remote requests, including during an offline job. Test that unsupported GPU limits fail closed. Test that storage cleanup cannot affect files outside a dedicated app-owned workspace.

For dashboard tests, distinguish simulated and real data clearly. Test accessibility, keyboard navigation, status clarity, and owner consent flows. Do not mark a feature complete based on a UI mockup alone. Record platform-specific limitations rather than silently bypassing them.

## 13. Parallel development plan

| Phase | Coordinator work | Worker work | Exit gate |
|---|---|---|---|
| 0 | Define typed state and fixture formats | Define local policy and owner UI mockup | Threat model and requirements reviewed |
| 1 | Pure scheduler and simulated job queue | Deterministic local workload simulation | Two simulated workers produce verified result |
| 2 | Local dashboard and job history | Owner-visible local CPU benchmark | Cancellation and limits tested on own device |
| 3 | Reviewed authenticated enrollment design | Local consent and revocation tests | Security review before network deployment |
| 4 | Dashboard integration and compatibility reports | Windows/Linux resource capability reporting | Owner controls tested on both platforms |
| 5 | Evaluate existing build-tool integration | Dedicated approved workload environments | Separate security review and controlled tests |
| 6 | Release workflow review | Installer and update design review | Independent review before sharing with friends |

## 14. Instructions for the coding assistant

Begin with phase 0 and phase 1 only. Inspect the repository before writing files. If no repository exists, propose the minimal workspace and dependency choices. Implement pure scheduling and policy logic plus a deterministic local simulation. Do not create remote execution, enrollment services reachable over the network, automatic startup, SSH access, background deployment, or update execution in this initial implementation. Keep UI prototypes clearly identified as mockups.

Make small reviewed changes. Run unit tests, formatting, and linting. Do not modify unrelated projects or existing NCM checkpoints. Ask before installing dependencies, changing system settings, opening ports, or executing anything outside the project. Keep a development log of assumptions and failed tests. Stop after a bounded number of unsuccessful repair attempts and report the blocker rather than attempting to bypass safeguards.

Expected initial commands (after creating a standard Rust workspace and obtaining approval for dependencies): `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets`. These are proposed development checks, not claims that the repository already exists.

## 15. Definition of done for v0.1

The prototype demonstrates two simulated workers processing a deterministic CPU workload, combines their results correctly, reports the job state in a simple interface, rejects jobs exceeding policy, and stops when the owner revokes approval. Tests reproduce these behaviors. Documentation states what is implemented and what remains simulated. No hidden operation, unrestricted remote execution, or third-party deployment is included.

## 16. Open decisions and limitations

The exact GUI visual style, minimum supported Windows version, Linux distribution coverage, coordinator migration mechanism, authentication provider, workload isolation technology, and release-signing infrastructure remain undecided. The proposed stack and license are engineering defaults, not final user decisions. Performance gains cannot be promised before testing; network overhead and heterogeneous hardware may make some jobs slower than local computation. A security review is a prerequisite for real third-party enrollment and unattended execution.
