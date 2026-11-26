# BackgroundHelper Development Guide

## Getting Started

### Prerequisites
- Rust 1.75+ (via rustup)
- SQLite3 development libraries
- OpenSSL development libraries (for native TLS)

```bash
# Ubuntu/Debian
sudo apt-get install libsqlite3-dev libssl-dev pkg-config

# Fedora
sudo dnf install sqlite-devel openssl-devel pkg-config

# macOS
brew install sqlite openssl pkg-config
```

### Build
```bash
cd BackgroundHelper
cargo build --workspace
```

### Test
```bash
cargo test --workspace
```

### Run Simulation
```bash
cargo run --example simulation -- 2 10
# 2 workers, 10 jobs
```

### Linting
```bash
cargo fmt --check --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Project Structure

```
BackgroundHelper/
├── Cargo.toml                 # Workspace root
├── crates/
│   ├── protocol/              # Shared types, validation
│   ├── policy/                # Resource policy evaluation
│   ├── scheduler/             # Job scheduling, fairness
│   ├── coordinator/           # Cluster management, API
│   └── worker/                # Worker runtime, UI
├── examples/
│   └── simulation.rs          # Local simulation binary
├── tests/
│   ├── fixtures/              # Test data
│   └── integration/           # Integration tests
├── docs/
│   ├── architecture.md
│   ├── threat-model.md
│   ├── owner-guide.md
│   └── development.md
├── apps/
│   ├── web/                   # React/TypeScript dashboard (future)
│   └── desktop/               # Tauri desktop app (future)
└── target/                    # Build output
```

## Development Workflow

### 1. Make Changes
- Edit code in `crates/*/src/`
- Add tests alongside code (`#[cfg(test)] mod tests`)

### 2. Verify
```bash
# Format check
cargo fmt --check --workspace

# Linting
cargo clippy --workspace --all-targets -- -D warnings

# Tests
cargo test --workspace
```

### 3. Run Simulation
```bash
cargo run --example simulation -- 2 10
```

### 4. Commit
```bash
git add -A
git commit -m "feat: description"
```

## Adding a New Workload Type

### 1. Protocol Crate
Add to `WorkloadType` enum in `crates/protocol/src/job.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkloadType {
    // ... existing ...
    NewWorkload {
        param1: u64,
        param2: String,
    },
}
```

Add validation in `JobSpec::validate()`:
```rust
WorkloadType::NewWorkload { param1, param2 } => {
    if *param1 > MAX_VALUE { return Err(...); }
    if param2.is_empty() { return Err(...); }
}
```

Add output type in `JobOutput`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobOutput {
    // ... existing ...
    NewWorkload { result_data: Vec<u8> },
}
```

### 2. Scheduler Crate
Add estimation in `fair.rs`:
```rust
fn estimate_job_cores(job: &JobSpec) -> f32 {
    match &job.workload {
        // ... existing ...
        WorkloadType::NewWorkload { .. } => 1.0,
    }
}
```

### 3. Worker Crate
Add executor in `executor.rs`:
```rust
async fn execute_new_workload(&self, param1: u64, param2: &str) -> Result<JobOutput, ExecutionError> {
    // Implementation
    Ok(JobOutput::NewWorkload { result_data: vec![] })
}
```

In `execute()` match:
```rust
WorkloadType::NewWorkload { param1, param2 } => {
    self.execute_new_workload(*param1, param2).await
}
```

### 4. Coordinator Crate
Add to supported workloads in `CoordinatorConfig`:
```rust
supported_workloads: vec![
    "range_sum".into(),
    "matrix_tile".into(),
    "prime_count".into(),
    "new_workload".into(),
],
```

### 5. Tests
Add integration test in `tests/integration/simulation_test.rs`

### 6. Documentation
Update `docs/architecture.md` and `docs/owner-guide.md`

## Testing Strategy

### Unit Tests
- Every pure function should have tests
- Test edge cases: empty inputs, boundaries, errors
- Use `proptest` for property-based testing

### Integration Tests
- `tests/integration/simulation_test.rs` - full flow
- Test: enrollment, scheduling, execution, results
- Test: policy limits, revocation, cancellation

### Simulation Tests
```bash
# Run with different worker/job counts
cargo run --example simulation -- 1 5
cargo run --example simulation -- 4 20
cargo run --example simulation -- 2 100
```

## Dependency Management

### Adding Dependencies
1. Add to `Cargo.toml` `[workspace.dependencies]`
2. Add to crate's `Cargo.toml` with `workspace = true`
3. Run `cargo build` to verify

### Policy
- Minimize dependencies
- Prefer stdlib where possible
- Audit with `cargo-audit` regularly
- Pin versions in `Cargo.lock`

### Current Dependencies
| Crate | Purpose | License |
|-------|---------|---------|
| tokio | Async runtime | MIT |
| axum | Web framework | MIT |
| sqlx | Database | MIT |
| serde | Serialization | MIT/Apache-2.0 |
| uuid | UUID generation | MIT/Apache-2.0 |
| chrono | Date/time | MIT/Apache-2.0 |
| sysinfo | System metrics | MIT |
| tracing | Logging | MIT |
| thiserror | Error handling | MIT/Apache-2.0 |
| schemars | JSON Schema | MIT/Apache-2.0 |

## Code Style

### Naming
- Types: `PascalCase` (`JobSpec`, `ResourceLimits`)
- Functions/variables: `snake_case` (`submit_job`, `worker_id`)
- Constants: `SCREAMING_SNAKE_CASE` (`PROTOCOL_VERSION`)
- Crates: `kebab-case` (`bg-helper-protocol`)

### Error Handling
- Use `thiserror` for error types
- Return `Result<T, Error>` for fallible operations
- Use `?` operator for propagation
- Avoid `unwrap()`/`expect()` in production code

### Async
- Use `tokio` for all async operations
- Prefer `async fn` over `fn -> impl Future`
- Use `Arc<RwLock<T>>` for shared mutable state
- Avoid blocking in async context (`spawn_blocking` if needed)

### Documentation
- Public APIs must have doc comments
- Use `///` for items, `//!` for modules
- Include examples in doc comments where helpful

## Debugging

### Logging
```bash
# Debug level
RUST_LOG=bg_helper=debug cargo run --example simulation

# Trace level (verbose)
RUST_LOG=bg_helper=trace cargo run --example simulation

# Specific crate
RUST_LOG=bg_helper_scheduler=debug cargo test
```

### Common Issues

**Build fails with "unused manifest key"**
- Remove unused `dependencies` entries from crate `Cargo.toml`
- Workspace dependencies are inherited automatically

**Tests hang**
- Check for missing `.await`
- Verify channels have senders/receivers
- Look for deadlocks in `RwLock` usage

**Simulation doesn't complete jobs**
- Check scheduler is actually assigning jobs
- Verify workers are accepting offers
- Look at coordinator logs for rejections

## Release Process

### Versioning
Semantic versioning: `MAJOR.MINOR.PATCH`
- MAJOR: Breaking changes
- MINOR: New features, backward compatible
- PATCH: Bug fixes

### Pre-release Checklist
- [ ] All tests pass: `cargo test --workspace`
- [ ] No clippy warnings: `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] Formatted: `cargo fmt --check --workspace`
- [ ] No security advisories: `cargo audit`
- [ ] Documentation builds: `cargo doc --workspace --no-deps`
- [ ] Simulation works: `cargo run --example simulation -- 2 10`

### Publishing
```bash
# Tag release
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0

# GitHub Actions will build and publish
```

## IDE Setup

### VS Code
Install extensions:
- rust-analyzer
- Even Better TOML
- crates.io

Settings:
```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.cargo.allFeatures": true
}
```

### IntelliJ/RustRover
- Enable rust-analyzer
- Configure cargo test runner

## Architecture Decisions

### Why Rust?
- Memory safety without GC
- Excellent async support (tokio)
- Strong type system for protocol correctness
- Cross-platform (Windows, Linux, macOS)
- Growing ecosystem for systems programming

### Why Tokio?
- Industry standard async runtime
- Good integration with axum, sqlx, tower
- Excellent performance and stability

### Why SQLite?
- Embedded, zero-config
- Sufficient for coordinator metadata
- Easy to backup/migrate
- Can upgrade to PostgreSQL later if needed

### Why Pure Functions for Scheduler/Policy?
- Testable without mocks
- Deterministic behavior
- Easy to reason about
- Can run in simulation or production

### Why Separate Crates?
- Enforces clean boundaries
- Parallel compilation
- Independent testing
- Clear ownership

## Future Work

### Phase 2 (Dashboard)
- React/TypeScript web dashboard
- Real-time WebSocket updates
- Job visualization

### Phase 3 (Enrollment)
- mTLS authentication
- QR code enrollment
- Account-based enrollment

### Phase 4 (Production)
- Windows installer (MSI)
- Linux packages (deb, rpm, AppImage)
- Systemd/launchd service integration
- Auto-update framework

### Phase 5 (Workloads)
- Distributed compilation
- GPU workloads
- AI inference
- Storage scheduling