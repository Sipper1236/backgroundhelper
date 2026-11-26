# BackgroundHelper

A voluntary distributed computing platform for Windows and Linux. Contribute your idle compute resources to a cluster you trust, with full owner control.

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

## Overview

BackgroundHelper lets a cluster administrator coordinate computational jobs across voluntarily contributed computers. The device owner maintains complete control:

- **Explicit consent** - Invitation codes, QR codes, or accounts
- **Resource limits** - You set maximum CPU, RAM, storage, schedule
- **Adaptive** - Automatically yields to your foreground apps
- **Revocable** - Pause or stop instantly, anytime
- **Transparent** - See exactly what's running and resource usage
- **No hidden operation** - Identifiable process, no auto-start without opt-in

## Status

**v0.1 Prototype** - Local simulation only

✅ Implemented:
- Typed protocol (jobs, resources, enrollment, messages)
- Pure policy engine with resource limits
- Fair scheduler with virtual-time fairness
- Coordinator with job queue and REST API
- Worker with config, enrollment, monitor, executor
- Local simulation (2 workers, 10 jobs)
- Integration tests

🚧 Not yet implemented:
- Network communication (TLS, private networks)
- Web/dashboard UI
- Windows/Linux installers
- GPU workloads
- Distributed compilation
- Automatic updates

## Quick Start

### Prerequisites
- Rust 1.75+
- SQLite3 dev libraries
- OpenSSL dev libraries

### Build & Test
```bash
git clone https://github.com/yourusername/BackgroundHelper
cd BackgroundHelper

# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Run local simulation (2 workers, 10 jobs)
cargo run --example simulation -- 2 10
```

### Simulation Output
```
Starting BackgroundHelper simulation
  Workers: 2
  Jobs: 10

╔══════════════════════════════════════════╗
║       Simulation Complete                ║
╠══════════════════════════════════════════╣
║  Total Jobs:                    10       ║
║  Completed:                     10       ║
║  Failed:                         0       ║
║  Workers:                        2       ║
║  Success Rate:               100.0%      ║
╚══════════════════════════════════════════╝
```

## Architecture

```
┌─────────────┐     ┌──────────────────┐     ┌─────────────┐
│ Admin       │────▶│ Coordinator      │◀───▶│ Worker      │
│ Dashboard   │     │ (Job Queue,      │     │ (Owner      │
│             │     │  Scheduler,      │     │  Device)    │
└─────────────┘     │  Registry)       │     └─────────────┘
                    └──────────────────┘
```

### Crates
| Crate | Purpose |
|-------|---------|
| `bg-helper-protocol` | Shared types, validation, messages |
| `bg-helper-policy` | Pure resource policy evaluation |
| `bg-helper-scheduler` | Fair job placement, queue |
| `bg-helper-coordinator` | Cluster management, REST API |
| `bg-helper-worker` | Owner-facing workload manager |

### Workload Types (v0.1)
- **RangeSum** - Sum a range of integers
- **PrimeCount** - Count primes in a range
- **MatrixTile** - Compute a matrix multiplication tile

All workloads are deterministic, bounded, and verifiable.

## Documentation

- [Architecture](docs/architecture.md) - System design
- [Threat Model](docs/threat-model.md) - Security analysis
- [Owner Guide](docs/owner-guide.md) - For device owners
- [Development Guide](docs/development.md) - For contributors

## Configuration

Worker configuration stored in:
- **Linux**: `~/.config/BackgroundHelper/worker.toml`
- **Windows**: `%APPDATA%\BackgroundHelper\worker.toml`

Example:
```toml
worker_name = "my-laptop"
coordinator_url = "http://coordinator:8080"
auto_start = false
log_level = "info"

[limits]
cpu_max_cores = 2
cpu_max_utilization_pct = 50
ram_max_workload_bytes = 2000000000
storage_quota_bytes = 5000000000

[limits.power]
allow_on_battery = false
min_battery_pct = 50

[limits.schedule]
timezone = "America/Los_Angeles"
allowed_windows = [
  { days = [1,2,3,4,5], start_hour = 9, start_minute = 0, end_hour = 17, end_minute = 0 },
  { days = [1,2,3,4,5], start_hour = 18, start_minute = 0, end_hour = 22, end_minute = 0 },
]
```

## Development

### Commands
```bash
# Format check
cargo fmt --check --workspace

# Linting
cargo clippy --workspace --all-targets -- -D warnings

# Tests with output
cargo test --workspace -- --nocapture

# Documentation
cargo doc --workspace --no-deps --open
```

### Adding a Workload
1. Add to `WorkloadType` in `crates/protocol/src/job.rs`
2. Add validation in `JobSpec::validate()`
3. Add output variant in `JobOutput`
4. Add estimation in `crates/scheduler/src/fair.rs`
5. Add executor in `crates/worker/src/executor.rs`
6. Add tests in `tests/integration/`
7. Update documentation

## Security

- No arbitrary code execution - only predefined workloads
- Worker policy is authoritative - coordinator cannot override
- Immediate revocation - stops current job
- No personal data collection - only resource metrics
- TLS for production communication
- See [Threat Model](docs/threat-model.md) for full analysis

## License

Apache License 2.0 - see [LICENSE](LICENSE)

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make changes with tests
4. Run `cargo fmt`, `cargo clippy`, `cargo test`
5. Submit a pull request

See [Development Guide](docs/development.md) for details.

## Roadmap

- [ ] v0.1: Local simulation ✅
- [ ] v0.2: Network communication, TLS
- [ ] v0.3: Web dashboard, real devices
- [ ] v0.4: Windows/Linux packages
- [ ] v0.5: GPU workloads, distributed compilation
- [ ] v1.0: Production ready, security audit

## Support

- Issues: GitHub Issues
- Documentation: `/docs`
- Security: See SECURITY.md (when published)