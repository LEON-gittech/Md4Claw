mod document_xml;
mod mcp_protocol;
mod storage;
mod tools;

use mcp_protocol::McpServer;
use storage::AppFlowyStorage;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
    .with_writer(std::io::stderr)
    .init();

  tracing::info!("Starting AppFlowy MCP Server");

  let data_dir = std::env::var("APPFLOWY_DATA_DIR").ok();
  let storage = AppFlowyStorage::new(data_dir)?;
  let server = McpServer::new(storage);
  server.run_stdio().await?;

  Ok(())
}
