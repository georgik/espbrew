//! Cluster command implementations

use crate::cli::args::ClusterAction;
use crate::cluster::discovery::discover_cluster_nodes;
use crate::cluster::node::ClusterNodeBuilder;
use crate::cluster::state::NodeRole;
use crate::cluster::DEFAULT_CLUSTER_NAME;
use anyhow::{Context, Result};
use log::{error, info};

/// Execute cluster command
pub async fn execute_cluster_command(
    name: Option<String>,
    role: Option<String>,
    action: ClusterAction,
) -> Result<()> {
    let cluster_name = name.unwrap_or_else(|| DEFAULT_CLUSTER_NAME.to_string());

    match action {
        ClusterAction::Start { bind } => {
            execute_start(cluster_name, role, bind).await
        }
        ClusterAction::Stop => {
            execute_stop().await
        }
        ClusterAction::Status { watch } => {
            execute_status(cluster_name, watch).await
        }
        ClusterAction::Topology => {
            execute_topology(cluster_name).await
        }
        ClusterAction::Join { master } => {
            execute_join(cluster_name, master).await
        }
        ClusterAction::Leave => {
            execute_leave().await
        }
        ClusterAction::Nodes => {
            execute_nodes(cluster_name).await
        }
        ClusterAction::Devices => {
            execute_devices(cluster_name).await
        }
    }
}

/// Start cluster node
async fn execute_start(cluster_name: String, role: Option<String>, bind: String) -> Result<()> {
    let node_role = parse_role(role)?;

    info!("Starting ESPBrew cluster node");
    info!("Cluster: {}", cluster_name);
    info!("Role: {:?}", node_role);
    info!("Bind: {}", bind);

    let mut node = ClusterNodeBuilder::new()
        .cluster_name(cluster_name)
        .role(node_role)
        .bind_address(bind)
        .build();

    // Start the node
    node.start().await?;

    println!("Cluster node started successfully");
    println!();
    println!("Press Ctrl+C to stop");

    // Wait for shutdown signal
    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            println!();
            info!("Shutdown signal received");
        }
        Err(e) => {
            error!("Failed to listen for shutdown signal: {}", e);
        }
    }

    // Stop the node
    node.stop().await?;

    Ok(())
}

/// Stop cluster node
async fn execute_stop() -> Result<()> {
    info!("Stopping cluster node");
    println!("Stopping cluster node...");
    println!();
    println!("TODO: Implement cluster node shutdown");

    Ok(())
}

/// Show cluster status
async fn execute_status(cluster_name: String, watch: bool) -> Result<()> {
    let nodes = discover_cluster_nodes(&cluster_name, 5).await?;

    println!("Cluster: {}", cluster_name);
    println!("Nodes: {}", nodes.len());
    println!();

    if nodes.is_empty() {
        println!("No nodes found in cluster");
        return Ok(());
    }

    println!("Nodes:");
    for node in &nodes {
        println!("  {} ({})", node.node_id, node.role);
        println!("    Address: {}", node.address);
        println!("    Devices: {}", node.device_count);
        println!("    Capabilities: {}", node.capabilities.join(", "));
    }

    if watch {
        println!();
        println!("Watching for changes (Ctrl+C to exit)...");
        // TODO: Implement watch mode
    }

    Ok(())
}

/// Show cluster topology
async fn execute_topology(cluster_name: String) -> Result<()> {
    let nodes = discover_cluster_nodes(&cluster_name, 5).await?;

    println!("Cluster Topology: {}", cluster_name);
    println!();

    if nodes.is_empty() {
        println!("No nodes found in cluster");
        return Ok(());
    }

    // Group by role
    let masters: Vec<_> = nodes.iter().filter(|n| n.role == "master").collect();
    let workers: Vec<_> = nodes.iter().filter(|n| n.role == "worker").collect();

    if !masters.is_empty() {
        println!("Master:");
        for master in masters {
            println!("  {} @ {}", master.node_id, master.address);
        }
    }

    if !workers.is_empty() {
        println!();
        println!("Workers:");
        for worker in workers {
            println!("  {} @ {} ({} devices)", worker.node_id, worker.address, worker.device_count);
        }
    }

    Ok(())
}

/// Join existing cluster
async fn execute_join(cluster_name: String, master: Option<String>) -> Result<()> {
    info!("Joining cluster: {}", cluster_name);

    let master_addr = if let Some(m) = master {
        m
    } else {
        // Auto-discover master
        let nodes = discover_cluster_nodes(&cluster_name, 5).await?;
        let master = nodes
            .iter()
            .find(|n| n.role == "master")
            .context("No master found in cluster")?;

        master.address.clone()
    };

    println!("Joining cluster:");
    println!("  Cluster: {}", cluster_name);
    println!("  Master: {}", master_addr);
    println!();
    println!("TODO: Implement cluster join");

    Ok(())
}

/// Leave cluster
async fn execute_leave() -> Result<()> {
    info!("Leaving cluster");
    println!("Leaving cluster...");
    println!();
    println!("TODO: Implement cluster leave");

    Ok(())
}

/// List nodes in cluster
async fn execute_nodes(cluster_name: String) -> Result<()> {
    let nodes = discover_cluster_nodes(&cluster_name, 5).await?;

    println!("Nodes in cluster '{}':", cluster_name);
    println!();

    if nodes.is_empty() {
        println!("No nodes found");
        return Ok(());
    }

    for node in &nodes {
        println!("  {}", node.node_id);
        println!("    Role: {}", node.role);
        println!("    Address: {}", node.address);
        println!("    Devices: {}", node.device_count);
        println!("    Version: {}", node.version);
        println!();
    }

    Ok(())
}

/// List devices in cluster
async fn execute_devices(cluster_name: String) -> Result<()> {
    let nodes = discover_cluster_nodes(&cluster_name, 5).await?;

    let total_devices: usize = nodes.iter().map(|n| n.device_count).sum();

    println!("Devices in cluster '{}':", cluster_name);
    println!();

    if nodes.is_empty() {
        println!("No nodes found");
        return Ok(());
    }

    println!("Total devices: {}", total_devices);
    println!();

    for node in &nodes {
        if node.device_count > 0 {
            println!("  {} ({} devices):", node.node_id, node.device_count);
            println!("    TODO: Query device details from node");
        }
    }

    Ok(())
}

/// Parse role from string
fn parse_role(role: Option<String>) -> Result<NodeRole> {
    match role.as_deref() {
        None | Some("auto") => Ok(NodeRole::Auto),
        Some("master") => Ok(NodeRole::Master),
        Some("worker") => Ok(NodeRole::Worker),
        Some(s) => Err(anyhow::anyhow!("Invalid role: {}", s)),
    }
}
