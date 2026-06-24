use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "mythrax", version = "0.5.0", about = "Mythrax Local Memory and Cognitive Daemon CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize the Mythrax configuration and directories
    Init {
        /// Name of the harness to configure
        harness: Option<String>,
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Harness and LLM configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
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
        /// Include raw episodes in search results
        #[arg(short, long)]
        episodes: bool,
    },
    /// Run safety compliance audits on the active directory
    Verify {
        /// Workspace directory to audit
        #[arg(short, long)]
        workspace: Option<String>,
    },
    /// Start the MCP server over stdin/stdout
    Mcp,
    /// Forge a source document to extract rules and wiki nodes
    Forge {
        /// Path to the source file (text, markdown, or PDF)
        source_path: String,
        /// Optional scope (defaults to 'general')
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Vault management and lifecycle operations
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
    /// Hypothesis-Tree Refinement (Arbor) operations
    Htr {
        #[command(subcommand)]
        action: HtrAction,
    },
    /// Recover from a crash using dual-durability journals
    Recover {
        /// The session ID to recover
        #[arg(short, long)]
        session: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Configure Antigravity harness
    Antigravity {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure Claude Code harness
    Claude {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure Cursor harness
    Cursor {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure Codex harness
    Codex {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure OpenCode harness
    Opencode {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure OpenClaw harness
    Openclaw {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure Hermes harness
    Hermes {
        /// Optional path to historical logs to ingest
        #[arg(short, long)]
        source: Option<String>,
    },
    /// Configure LLM model and cloud provider settings
    Llm {
        /// Provider type ('local' or 'cloud')
        #[arg(short, long)]
        provider: String,
        /// Duration ('temporary' or 'permanent')
        #[arg(short, long)]
        duration: Option<String>,
        /// Model identifier (e.g. 'gemini-1.5-flash', 'mlx-community/Qwen3.6-35B-A3B-4bit')
        #[arg(short, long)]
        model: Option<String>,
        /// Cloud provider name ('gemini', 'anthropic')
        #[arg(long)]
        cloud_provider: Option<String>,
        /// API Key for cloud access
        #[arg(long)]
        api_key: Option<String>,
    },
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
    /// Run the Mythrax background daemon in the foreground
    Run {
        /// Port to bind the daemon REST API to
        #[arg(short, long, default_value_t = 8090)]
        port: u16,
        /// Path to the Obsidian vault to watch
        #[arg(short, long)]
        vault: Option<String>,
    },
    /// Stop the running background daemon using PID file
    Stop,
}

#[derive(Subcommand, Debug)]
pub enum VaultAction {
    /// Bulk ingest logs into the memory store
    Ingest {
        /// Path to the log directory or file
        #[arg(short, long)]
        source: String,
        /// Harness type (e.g. 'antigravity', 'claude', 'cursor', etc.)
        #[arg(short, long)]
        harness: String,
        /// Optional scope
        #[arg(long)]
        scope: Option<String>,
    },
    /// Organize the vault files by renaming and resolving duplicates
    Organize,
    /// Summarize episodes and generate system wisdom rules
    Summarize {
        /// Optional scope
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Verify vault integrity and run self-healing repairs
    Verify {
        /// Fix any issues found
        #[arg(short, long)]
        fix: bool,
    },
    /// Reprocess episodes with missing vector embeddings
    Reprocess,
}

#[derive(Subcommand, Debug, Clone)]
pub enum HtrAction {
    /// Initialize HTR session root node
    Init {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        hypothesis: String,
        #[arg(short, long, value_delimiter = ',')]
        files: Vec<String>,
    },
    /// Propose child hypotheses (Ideation)
    Ideate {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        node: String,
    },
    /// Execute hypothesis node test run
    Execute {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        node: String,
        #[arg(short, long)]
        test_command: String,
    },
    /// Backpropagate test results and evaluation insights
    Backprop {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        node: String,
    },
    /// Apply and commit the selected node's changes to the codebase
    Merge {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        node: String,
    },
    /// Run the HTR loop end-to-end for a given hypothesis and codebase files
    Run {
        #[arg(short, long)]
        scope: String,
        #[arg(short, long)]
        hypothesis: String,
        #[arg(short, long, value_delimiter = ',')]
        files: Vec<String>,
        #[arg(short, long)]
        test_command: String,
        #[arg(long, default_value_t = 5)]
        max_steps: usize,
    },
}
