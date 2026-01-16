//! VRaftLS Gateway - LSP gateway binary

use clap::Parser;
use tower_lsp::{LspService, Server};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use vraftls_lsp::LspGateway;

#[derive(Parser)]
#[command(name = "vraftls-gateway")]
#[command(about = "VRaftLS LSP gateway")]
struct Args {
    /// Use stdio for LSP communication (default)
    #[arg(long, default_value = "true")]
    stdio: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing to stderr (stdout is used for LSP)
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let _args = Args::parse();

    tracing::info!("Starting VRaftLS gateway");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| LspGateway::new(client));

    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
