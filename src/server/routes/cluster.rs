//! Cluster API routes for ESPBrew

use crate::cluster::messaging::*;
use crate::server::app::ServerState;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use warp::Filter;

/// Cluster device info for API responses
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClusterDeviceInfo {
    pub id: String,
    pub node_id: String,
    pub backend: String,
    pub board_type: String,
    pub status: String,
    pub capabilities: Vec<String>,
    pub location: String,
    pub logical_name: Option<String>,
}

/// Cluster node info for API responses
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClusterNodeInfo {
    pub id: String,
    pub role: String,
    pub address: String,
    pub last_seen: String,
    pub capabilities: Vec<String>,
    pub device_count: usize,
    pub active_jobs: usize,
}

/// Job info for API responses
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClusterJobInfo {
    pub id: String,
    pub command: String,
    pub target_device: String,
    pub assigned_node: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Create cluster API routes
pub fn create_cluster_routes(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let cluster_base = warp::path("api")
        .and(warp::path("v1"))
        .and(warp::path("cluster"))
        .and(warp::path("end"));

    // GET /api/v1/cluster/status - Cluster status
    let status = cluster_base
        .and(warp::path("status"))
        .and(warp::path::end())
        .and(warp::get())
        .and(with_state(state.clone()))
        .and_then(handle_cluster_status);

    // GET /api/v1/cluster/nodes - List nodes
    let nodes = cluster_base
        .and(warp::path("nodes"))
        .and(warp::path::end())
        .and(warp::get())
        .and(with_state(state.clone()))
        .and_then(handle_cluster_nodes);

    // GET /api/v1/cluster/devices - List all devices
    let devices = cluster_base
        .and(warp::path("devices"))
        .and(warp::path::end())
        .and(warp::get())
        .and(with_state(state.clone()))
        .and_then(handle_cluster_devices);

    // POST /api/v1/cluster/jobs - Submit job
    let jobs = cluster_base
        .and(warp::path("jobs"))
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::path::end())
        .and(with_state(state.clone()))
        .and(warp::body::json())
        .and_then(handle_submit_job);

    // GET /api/v1/cluster/jobs/{id} - Job status
    let job_status = cluster_base
        .and(warp::path("jobs"))
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::get())
        .and(with_state(state.clone()))
        .and_then(|job_id, state| async move { handle_job_status(state, job_id).await });

    status.or(nodes).or(devices).or(jobs).or(job_status)
}

/// Warp filter to inject state
fn with_state(
    state: Arc<RwLock<ServerState>>,
) -> impl Filter<Extract = (Arc<RwLock<ServerState>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || state.clone())
}

/// Handle cluster status request
async fn handle_cluster_status(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state_guard = state.read().await;

    if let Some(cluster_master) = &state_guard.cluster_master {
        let status = cluster_master.status_summary().await;
        Ok(warp::reply::json(&status))
    } else {
        Ok(warp::reply::json(&json!({
            "error": "Cluster not enabled",
            "message": "Start with --cluster flag to enable cluster mode",
            "cluster_name": null,
            "node_count": 0,
            "device_count": 0,
            "available_devices": 0,
            "queued_jobs": 0,
            "running_jobs": 0,
        })))
    }
}

/// Handle cluster nodes list request
async fn handle_cluster_nodes(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state_guard = state.read().await;

    if let Some(cluster_master) = &state_guard.cluster_master {
        let state_lock = cluster_master.state();
        let cluster_state = state_lock.read().await;
        let nodes: Vec<ClusterNodeInfo> = cluster_state
            .nodes
            .values()
            .map(|n| ClusterNodeInfo {
                id: n.id.clone(),
                role: n.role.as_str().to_string(),
                address: n.address.clone(),
                last_seen: n.last_seen.to_rfc3339(),
                capabilities: n.capabilities.clone(),
                device_count: n.device_count,
                active_jobs: n.active_jobs,
            })
            .collect();

        Ok(warp::reply::json(&nodes))
    } else {
        Ok(warp::reply::json(&json!([])))
    }
}

/// Handle cluster devices list request
async fn handle_cluster_devices(
    state: Arc<RwLock<ServerState>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state_guard = state.read().await;

    if let Some(cluster_master) = &state_guard.cluster_master {
        let state_lock = cluster_master.state();
        let cluster_state = state_lock.read().await;
        let devices: Vec<ClusterDeviceInfo> = cluster_state
            .devices
            .values()
            .map(|d| ClusterDeviceInfo {
                id: d.id.clone(),
                node_id: d.node_id.clone(),
                backend: d.backend.as_str().to_string(),
                board_type: d.board_type.clone(),
                status: format!("{:?}", d.status),
                capabilities: d.capabilities.clone(),
                location: d.location.clone(),
                logical_name: d.logical_name.clone(),
            })
            .collect();

        Ok(warp::reply::json(&devices))
    } else {
        Ok(warp::reply::json(&json!([])))
    }
}

/// Job submission request
#[derive(Debug, serde::Deserialize)]
struct JobSubmitRequest {
    pub command: String,
    pub selector: String,
    pub timeout_secs: Option<u64>,
}

/// Job submission response
#[derive(Debug, serde::Serialize)]
struct JobSubmitResponse {
    pub success: bool,
    pub job_id: Option<String>,
    pub error: Option<String>,
}

/// Handle job submission
async fn handle_submit_job(
    state: Arc<RwLock<ServerState>>,
    request: JobSubmitRequest,
) -> Result<impl warp::Reply, warp::Rejection> {
    let command = match request.command.as_str() {
        "reset" => CommandType::Reset,
        "flash" => CommandType::Flash {
            binary_data: vec![], // TODO: Accept binary data
        },
        "monitor" => CommandType::Monitor { baud_rate: 115200 },
        _ => {
            return Ok(warp::reply::json(&JobSubmitResponse {
                success: false,
                job_id: None,
                error: Some(format!("Invalid command: {}", request.command)),
            }));
        }
    };

    let selector = parse_selector(&request.selector);

    // Take write lock for submit_job
    let mut state_guard = state.write().await;

    if let Some(master) = &mut state_guard.cluster_master {
        match master
            .submit_job(command, selector, request.timeout_secs.unwrap_or(30))
            .await
        {
            Ok(job_id) => Ok(warp::reply::json(&JobSubmitResponse {
                success: true,
                job_id: Some(job_id),
                error: None,
            })),
            Err(e) => Ok(warp::reply::json(&JobSubmitResponse {
                success: false,
                job_id: None,
                error: Some(e.to_string()),
            })),
        }
    } else {
        Ok(warp::reply::json(&JobSubmitResponse {
            success: false,
            job_id: None,
            error: Some("Cluster not enabled".to_string()),
        }))
    }
}

/// Handle job status request
async fn handle_job_status(
    state: Arc<RwLock<ServerState>>,
    job_id: String,
) -> Result<impl warp::Reply, warp::Rejection> {
    let state_guard = state.read().await;

    if let Some(cluster_master) = &state_guard.cluster_master {
        let state_lock = cluster_master.state();
        let cluster_state = state_lock.read().await;

        if let Some(job) = cluster_state.jobs.get(&job_id) {
            let job_info = ClusterJobInfo {
                id: job.id.clone(),
                command: format!("{:?}", job.command),
                target_device: job.target_device.clone(),
                assigned_node: job.assigned_node.clone(),
                status: format!("{:?}", job.status),
                created_at: job.created_at.to_rfc3339(),
                started_at: job.started_at.map(|t| t.to_rfc3339()),
                completed_at: job.completed_at.map(|t| t.to_rfc3339()),
            };

            Ok(warp::reply::json(&job_info))
        } else {
            Ok(warp::reply::json(&json!({
                "error": "Job not found",
                "job_id": job_id
            })))
        }
    } else {
        Ok(warp::reply::json(&json!({
            "error": "Cluster not enabled",
            "job_id": job_id
        })))
    }
}

/// Parse device selector from string
fn parse_selector(selector_str: &str) -> DeviceSelector {
    if selector_str == "any" {
        return DeviceSelector::Any;
    }

    if let Some(rest) = selector_str.strip_prefix("id:") {
        return DeviceSelector::Specific(rest.to_string());
    }

    if let Some(rest) = selector_str.strip_prefix("type:") {
        return DeviceSelector::ByType(rest.to_string());
    }

    if let Some(rest) = selector_str.strip_prefix("backend:") {
        return DeviceSelector::ByBackend(rest.to_string());
    }

    if let Some(rest) = selector_str.strip_prefix("node:") {
        return DeviceSelector::ByNode(rest.to_string());
    }

    if let Some(rest) = selector_str.strip_prefix("name:") {
        return DeviceSelector::ByName(rest.to_string());
    }

    DeviceSelector::Any
}
