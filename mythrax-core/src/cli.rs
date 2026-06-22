use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "mythrax", version = "0.1.0", about = "Mythrax Local Memory and Cognitive Daemon CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize the Mythrax configuration and directories
    Init,
    /// Daemon operations
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Check system status (DB connection, memory, config)
    Status,
    /// Save an episode or note from the CLI
    Save {
        /// Path to the markdown file to save
        #[arg(short, long)]
        file: String,
        /// Optional scope for the saved episode
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Query the memory store
    Search {
        /// The search query text
        query: String,
        /// Optional scope to filter by
        #[arg(short, long)]
        scope: Option<String>,
        /// Maximum number of results
        #[arg(short, long, default_value_t = 5)]
        limit: usize,
    },
    /// Run safety compliance audits on the active directory
    Verify {
        /// Workspace directory to audit
        #[arg(short, long)]
        workspace: Option<String>,
    },
    /// Start the MCP server over stdin/stdout
    Mcp,
}

#[derive(Subcommand, Debug)]
pub enum DaemonAction {
    /// Start the Mythrax background daemon
    Start {
        /// Port to bind the daemon REST API to
        #[arg(short, long, default_value_t = 8090)]
        port: u16,
        /// Path to the Obsidian vault to watch
        #[arg(short, long)]
        vault: Option<String>,
    },
}
