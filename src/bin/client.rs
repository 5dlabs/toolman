use anyhow::Result;
use clap::Parser;
use toolman::client::McpClient;

/// Toolman MCP Client
///
/// A client-side MCP implementation that provides intelligent routing between local and remote MCP servers,
/// enabling dynamic server management and tool switching for AI development workflows.
#[derive(Parser)]
#[command(name = "toolman-client")]
#[command(about = "Toolman MCP Client - client-side MCP implementation with local/remote routing")]
struct Args {
    /// HTTP server URL to connect to for remote tools
    ///
    /// URL of the Toolman HTTP server to connect to for remote tools.
    /// Can also be set via MCP_HTTP_URL environment variable.
    #[arg(long, default_value = "http://localhost:3000")]
    url: String,

    /// Working directory for local servers and configuration
    ///
    /// The working directory to use for local server spawning and config lookup.
    /// If not provided, uses the current working directory.
    #[arg(long)]
    working_dir: Option<String>,

    /// HTTP server URL (positional argument for compatibility)
    #[arg(value_name = "HTTP_URL", help = "HTTP server URL for remote tools")]
    http_url: Option<String>,

    /// Working directory (positional argument for compatibility)  
    #[arg(
        value_name = "WORKING_DIR",
        help = "Working directory for local servers"
    )]
    pos_working_dir: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Use positional arguments if provided, otherwise use flags
    let http_base_url = args.http_url.unwrap_or(args.url);
    let working_dir = args.pos_working_dir.or(args.working_dir);

    let client = McpClient::new(http_base_url, working_dir)?;
    client.run()?;

    Ok(())
}
