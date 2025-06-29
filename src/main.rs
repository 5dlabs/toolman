use anyhow::Result;
use clap::Parser;
use toolman::stdio_wrapper::StdioWrapper;

/// MCP Bridge Proxy Server
///
/// A bridge proxy that provides selective tool exposure from multiple MCP servers,
/// enabling dynamic server management and tool switching for AI development workflows.
#[derive(Parser)]
#[command(name = "toolman")]
#[command(about = "MCP Bridge Proxy - stdio wrapper")]
struct Args {
    /// HTTP server URL to connect to
    ///
    /// URL of the MCP Bridge Proxy HTTP server to connect to.
    /// Can also be set via MCP_HTTP_URL environment variable.
    #[arg(long, default_value = "http://localhost:3000")]
    url: String,

    /// Working directory to send as context
    ///
    /// The working directory to send to the HTTP server for user config lookup.
    /// If not provided, uses the current working directory.
    #[arg(long)]
    working_dir: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let wrapper = StdioWrapper::new(args.url, args.working_dir)?;
    wrapper.run()?;

    Ok(())
}
