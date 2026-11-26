//! Local simulation example: runs a coordinator with simulated workers.

use bg_helper_coordinator::{CoordinatorState, CoordinatorConfig, simulation::run_simulation};
use bg_helper_protocol::job::{JobPriority, WorkloadType};
use bg_helper_protocol::resource::{HardwareCapability, WorkerState, ResourceUsage};
use bg_helper_protocol::enrollment::EnrollmentRequest;
use std::time::Duration;
use std::sync::Arc;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("bg_helper=info".parse()?))
        .init();
    
    // Parse command line args
    let args: Vec<String> = std::env::args().collect();
    let num_workers = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2);
    let num_jobs = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
    
    println!("Starting BackgroundHelper simulation");
    println!("  Workers: {}", num_workers);
    println!("  Jobs: {}", num_jobs);
    println!();
    
    // Run simulation
    match run_simulation(num_workers, num_jobs).await {
        Ok(result) => {
            println!();
            println!("╔══════════════════════════════════════════╗");
            println!("║       Simulation Complete                ║");
            println!("╠══════════════════════════════════════════╣");
            println!("║  Total Jobs:     {:>20} ║", result.total_jobs);
            println!("║  Completed:      {:>20} ║", result.completed);
            println!("║  Failed:         {:>20} ║", result.failed);
            println!("║  Workers:        {:>20} ║", result.workers);
            println!("║  Success Rate:   {:>19.1}% ║", result.success_rate() * 100.0);
            println!("╚══════════════════════════════════════════╝");
            
            if result.failed > 0 {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Simulation failed: {}", e);
            std::process::exit(1);
        }
    }
    
    Ok(())
}