# BackgroundHelper Owner Guide

## What is BackgroundHelper?

BackgroundHelper is a voluntary distributed computing platform. You choose to contribute your computer's idle resources (CPU, optionally GPU) to help run computational jobs for a cluster administrator you trust.

**Key principles:**
- **You are in control** - You decide when, how much, and what resources to share
- **Transparent** - You can see exactly what's running and how much resources it uses
- **Revocable** - You can pause or stop sharing at any time, instantly
- **No surprises** - No hidden processes, no automatic startup without your consent

## Quick Start

### 1. Receive Invitation
The cluster administrator will send you an invitation code (looks like `ABCD-EFGH-IJKL-MNOP`) or a QR code.

### 2. Install Worker
- **Windows**: Run `BackgroundHelper-Setup.exe` (installs to Program Files)
- **Linux**: Install via package manager or download portable binary
- **Portable**: Download `backgroundhelper-worker` and run directly

### 3. Enroll
1. Open BackgroundHelper app
2. Click "Join Cluster"
3. Enter invitation code or scan QR code
4. Review resource limits (see below)
5. Click "Accept"

### 4. That's it!
The worker will now run in the background when your sharing schedule allows.

## Understanding Resource Limits

When you enroll, you set **maximum** limits. The worker will **never exceed** these.

| Limit | What it means | Recommended default |
|-------|---------------|---------------------|
| **Max CPU Cores** | How many CPU cores cluster jobs can use | 1-2 cores |
| **Max CPU %** | Maximum CPU utilization (total) | 50% |
| **Max RAM** | Memory for cluster workloads | 1-2 GB |
| **Storage Quota** | Disk space for cluster data | 5-10 GB |
| **Schedule** | When sharing is allowed | Work hours + evenings |
| **Battery** | Allow on battery power? | No |

**Example**: If you set "2 cores, 50%", and your computer has 8 cores:
- Cluster gets at most 2 cores worth of work
- Total CPU usage from cluster stays under 50%
- Your apps always get priority

## Owner Controls

### Pause Sharing
Click "Pause" in the app or system tray. The worker:
- Finishes current job (if policy allows)
- Stops accepting new jobs
- Shows "PAUSED" status

### Resume Sharing
Click "Resume" - worker immediately available for new jobs.

### Revoke Access
Click "Revoke" - this:
- Immediately stops any running job
- Removes enrollment permanently
- Requires new invitation to rejoin

### Adjust Limits
Open Settings to change:
- CPU cores / utilization %
- RAM limit
- Storage quota
- Sharing schedule
- Battery policy

Changes take effect immediately for new jobs.

## What You'll See

### System Tray / Menu Bar
- **Green icon**: Sharing active, idle
- **Blue icon**: Job running
- **Yellow icon**: Paused
- **Red icon**: Error / Revoked

### Main Window
```
╔══════════════════════════════════════════╗
║  BackgroundHelper Worker Status          ║
╠══════════════════════════════════════════╣
║  Worker: my-laptop                       ║
║  State:  Busy                            ║
║  Sharing: ACTIVE                         ║
╠══════════════════════════════════════════╣
║  Current Job:                            ║
║    ID: job-abc123                        ║
║    Type: Prime counting                  ║
║    Progress: 45%                         ║
║    Elapsed: 12s  Remaining: 15s          ║
╠══════════════════════════════════════════╣
║  Resource Usage:                         ║
║    CPU: 42% (2.1 cores)                  ║
║    RAM: 1.2 GB / 4.0 GB                  ║
║    Storage: 500 MB / 5 GB                ║
║    Power: AC                             ║
║    Thermal: Normal                       ║
╠══════════════════════════════════════════╣
║  Configured Limits:                      ║
║    Max CPU Cores: 2                      ║
║    Max CPU Utilization: 50%              ║
║    Max RAM: 4 GB                         ║
║    Storage Quota: 5 GB                   ║
║    Schedule: Mon-Fri 9-17, 18-22         ║
║    Battery: Not allowed                  ║
╚══════════════════════════════════════════╝
```

### Web Dashboard (if enabled)
Visit `http://localhost:8080` (or configured port) for:
- Real-time resource graphs
- Job history
- Settings panel
- Activity log

## What Jobs Run

**Current workload types (v0.1):**
1. **Range Sum** - Add up a range of integers
2. **Prime Count** - Count prime numbers in a range
3. **Matrix Tile** - Compute a piece of matrix multiplication

**All jobs are:**
- Pure computation (no network, no file access)
- Deterministic (same input = same output)
- Bounded (max runtime, memory, CPU)
- Verifiable (coordinator can check results)

**Future workloads (with your explicit consent):**
- Distributed compilation
- AI inference
- Scientific simulations

## Privacy & Security

| What we collect | What we DON'T collect |
|-----------------|----------------------|
| CPU usage % | Your files |
| RAM usage | Your browser history |
| Disk usage (cluster only) | Your passwords |
| Power state (AC/Battery) | Your personal documents |
| CPU temperature | Your emails |
| Job progress/results | Any personal data |

The worker **cannot**:
- Access your personal files
- See what other apps you're running
- Install software
- Run arbitrary commands
- Communicate outside the cluster

## Troubleshooting

### "Worker won't start"
- Check if already running in system tray
- Check Windows Defender / antivirus isn't blocking
- Try running as administrator once (for firewall rules)

### "High CPU usage"
- Lower "Max CPU %" in settings
- Reduce "Max CPU Cores"
- Enable "Pause when gaming" (future)

### "Battery draining"
- Disable "Allow on battery" in settings
- Set minimum battery % (e.g., 50%)
- Worker auto-pauses on battery

### "Can't enroll"
- Check invitation code hasn't expired
- Verify internet connection
- Contact cluster admin for new code

### "Want to uninstall"
- **Windows**: Settings → Apps → BackgroundHelper → Uninstall
- **Linux**: Package manager remove, or delete binary
- **Portable**: Delete the executable and config folder (`~/.config/BackgroundHelper`)

Uninstalling **automatically revokes** your enrollment.

## FAQ

**Q: Will this slow down my computer?**
A: With default limits (1 core, 50%), most users won't notice. The worker yields to your apps automatically.

**Q: Can the admin see my files?**
A: No. The worker has no file system access beyond its own workspace folder.

**Q: What if I'm on a metered connection?**
A: Jobs don't use network (except tiny heartbeats ~1KB/min). Results are small.

**Q: Can I run this on my work laptop?**
A: Only if your IT policy allows. Check with your administrator first.

**Q: How do I know it's not mining crypto?**
A: Open source - you can audit the code. Only predefined math workloads run.

**Q: What happens if my computer sleeps?**
A: Worker pauses, resumes when awake. Jobs have timeouts.

## Support

- **Issues**: GitHub Issues (link from admin)
- **Documentation**: This guide + `docs/architecture.md`
- **Admin contact**: Provided during enrollment

## License

Apache 2.0 - Free to use, modify, distribute.