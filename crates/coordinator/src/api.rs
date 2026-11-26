//! Coordinator REST API for dashboard and administration.

use std::sync::Arc;
use axum::{
    extract::{State, Path, Query, Json},
    http::StatusCode,
    response::{IntoResponse, Json as JsonResponse},
    routing::{get, post},
    Router,
};
use bg_helper_protocol::{
    job::{JobSpec, JobId, JobPriority, WorkloadType, Job},
    resource::{WorkerSnapshot, WorkerState},
    enrollment::{EnrollmentRequest, EnrollmentResponse, CoordinatorInfo},
};
use crate::state::CoordinatorState;
use crate::registry::WorkerRegistry;
use uuid::Uuid;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, debug};

/// Shared state for API handlers.
#[derive(Clone)]
pub struct ApiState {
    pub coordinator: Arc<CoordinatorState>,
    pub registry: Arc<WorkerRegistry>,
}

/// Create the API router.
pub fn create_router(state: ApiState) -> Router<ApiState> {
    Router::new()
        // Health check
        .route("/health", get(health_check))
        // Coordinator info
        .route("/info", get(get_coordinator_info))
        // Worker management
        .route("/workers", get(list_workers))
        .route("/workers/:id", get(get_worker))
        .route("/workers/:id/revoke", post(revoke_worker))
        // Job management
        .route("/jobs", get(list_jobs))
        .route("/jobs", post(submit_job))
        .route("/jobs/:id", get(get_job))
        .route("/jobs/:id/cancel", post(cancel_job))
        // Queue status
        .route("/queue", get(queue_status))
        // Enrollment
        .route("/enroll", post(enroll_worker))
        // Invitation codes (admin)
        .route("/invitations", post(create_invitation))
}

/// Health check endpoint.
async fn health_check() -> JsonResponse<HealthResponse> {
    JsonResponse(HealthResponse {
        status: "healthy".into(),
        timestamp: Utc::now(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    timestamp: chrono::DateTime<Utc>,
    version: String,
}

/// Get coordinator info.
async fn get_coordinator_info(State(state): State<ApiState>) -> JsonResponse<CoordinatorInfo> {
    JsonResponse(state.coordinator.coordinator_info())
}

/// List all workers.
async fn list_workers(State(state): State<ApiState>) -> JsonResponse<Vec<WorkerSnapshot>> {
    let workers = state.coordinator.get_workers().await;
    JsonResponse(workers)
}

/// Get a specific worker.
async fn get_worker(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<JsonResponse<WorkerSnapshot>, StatusCode> {
    state.coordinator.get_worker(id).await
        .map(JsonResponse)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Revoke a worker.
async fn revoke_worker(
    State(state): State<ApiState>,
    Path(id): Path<Uuid>,
) -> Result<JsonResponse<RevokeResponse>, StatusCode> {
    state.coordinator.revoke_worker(id, bg_helper_protocol::enrollment::RevocationReason::AdminRevoked).await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(JsonResponse(RevokeResponse { success: true, worker_id: id }))
}

#[derive(Serialize)]
struct RevokeResponse {
    success: bool,
    worker_id: Uuid,
}

/// List all jobs (active + history).
async fn list_jobs(State(state): State<ApiState>) -> JsonResponse<JobListResponse> {
    let active = state.coordinator.get_active_jobs().await;
    let history = state.coordinator.get_job_history().await;
    JsonResponse(JobListResponse { active, history })
}

#[derive(Serialize)]
struct JobListResponse {
    active: Vec<Job>,
    history: Vec<Job>,
}

/// Submit a new job.
async fn submit_job(
    State(state): State<ApiState>,
    Json(request): Json<SubmitJobRequest>,
) -> Result<JsonResponse<SubmitJobResponse>, (StatusCode, JsonResponse<ErrorResponse>)> {
    let spec = request.to_job_spec()
        .map_err(|e| (StatusCode::BAD_REQUEST, JsonResponse(ErrorResponse { error: e })))?;
    
    let job_id = state.coordinator.submit_job(spec).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, JsonResponse(ErrorResponse { error: e.to_string() })))?;
    
    Ok(JsonResponse(SubmitJobResponse { job_id, status: "queued".into() }))
}

#[derive(Deserialize)]
struct SubmitJobRequest {
    workload_type: String,
    parameters: serde_json::Value,
    priority: Option<String>,
    max_duration_secs: Option<u32>,
}

impl SubmitJobRequest {
    fn to_job_spec(&self) -> Result<JobSpec, String> {
        let priority = match self.priority.as_deref() {
            Some("low") => JobPriority::Low,
            Some("high") => JobPriority::High,
            Some("critical") => JobPriority::Critical,
            _ => JobPriority::Normal,
        };
        
        let max_duration = self.max_duration_secs.unwrap_or(300);
        
        let workload = match self.workload_type.as_str() {
            "range_sum" => {
                let start = self.parameters.get("start")
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing start parameter")?;
                let end = self.parameters.get("end")
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing end parameter")?;
                WorkloadType::RangeSum { start, end }
            }
            "matrix_tile" => {
                let size = self.parameters.get("size")
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing size parameter")? as usize;
                let tile = self.parameters.get("tile")
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing tile parameter")? as usize;
                let row = self.parameters.get("row")
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing row parameter")? as usize;
                let col = self.parameters.get("col")
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing col parameter")? as usize;
                WorkloadType::MatrixTile { 
                    size, 
                    tile: bg_helper_protocol::job::TileSpec { row, col, tile_size: tile }
                }
            }
            "prime_count" => {
                let start = self.parameters.get("start")
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing start parameter")?;
                let end = self.parameters.get("end")
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing end parameter")?;
                WorkloadType::PrimeCount { start, end }
            }
            _ => return Err(format!("Unknown workload type: {}", self.workload_type)),
        };
        
        Ok(JobSpec::new(workload, priority, max_duration))
    }
}

#[derive(Serialize)]
struct SubmitJobResponse {
    job_id: JobId,
    status: String,
}

/// Get a specific job.
async fn get_job(
    State(state): State<ApiState>,
    Path(id): Path<JobId>,
) -> Result<JsonResponse<Job>, StatusCode> {
    state.coordinator.get_job(id).await
        .map(JsonResponse)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Cancel a job.
async fn cancel_job(
    State(state): State<ApiState>,
    Path(id): Path<JobId>,
) -> Result<JsonResponse<CancelResponse>, StatusCode> {
    // This would need implementation in coordinator state
    // For now, return not implemented
    Err(StatusCode::NOT_IMPLEMENTED)
}

#[derive(Serialize)]
struct CancelResponse {
    success: bool,
    job_id: JobId,
}

/// Get queue status.
async fn queue_status(State(state): State<ApiState>) -> JsonResponse<QueueStatusResponse> {
    let (queued, assigned, completed) = state.coordinator.get_queue_status().await;
    JsonResponse(QueueStatusResponse { queued, assigned, completed })
}

#[derive(Serialize)]
struct QueueStatusResponse {
    queued: usize,
    assigned: usize,
    completed: usize,
}

/// Enroll a worker.
async fn enroll_worker(
    State(state): State<ApiState>,
    Json(request): Json<EnrollmentRequest>,
) -> JsonResponse<EnrollmentResponse> {
    let response = state.registry.enroll_worker(request).await;
    JsonResponse(response)
}

/// Create an invitation code (admin).
async fn create_invitation(
    State(state): State<ApiState>,
    Json(request): Json<CreateInvitationRequest>,
) -> JsonResponse<bg_helper_protocol::enrollment::InvitationCode> {
    let invitation = state.registry.create_invitation(
        request.cluster_name,
        request.expires_in_hours.unwrap_or(24),
        request.max_workers,
    ).await;
    JsonResponse(invitation)
}

#[derive(Deserialize)]
struct CreateInvitationRequest {
    cluster_name: String,
    expires_in_hours: Option<u32>,
    max_workers: Option<u32>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// WebSocket handler for real-time updates (placeholder).
pub async fn websocket_handler() {
    // TODO: Implement WebSocket for real-time dashboard updates
}