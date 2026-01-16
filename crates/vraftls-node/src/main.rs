//! VRaftLS Node - Data node binary

use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "vraftls-node")]
#[command(about = "VRaftLS data node")]
struct Args {
    /// Node ID
    #[arg(long, default_value = "1")]
    node_id: u64,

    /// HTTP listen address
    #[arg(long, default_value = "127.0.0.1:8080")]
    listen: String,

    /// Data directory
    #[arg(long, default_value = "./data")]
    data_dir: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    tracing::info!(
        node_id = args.node_id,
        listen = %args.listen,
        "Starting VRaftLS node"
    );

    // TODO: Initialize node components
    // - Raft storage
    // - State machine
    // - HTTP server

    Ok(())
}
