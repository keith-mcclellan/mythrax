use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "mythrax", version = env!("CARGO_PKG_VERSION"), about = "Mythrax Local Memory and Cognitive Daemon CLI")]
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
        /// Run in non-interactive mode
        #[arg(long)]
        non_interactive: bool,
    },
    /// Daemon operations
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Start the MCP server over stdin/stdout
    Mcp,
    /// Memory operations (query, record, feedback)
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// Hypothesis-Tree Refinement (Arbor) operations
    Htr {
        #[command(subcommand)]
        action: HtrAction,
    },
    /// Short-term memory (STM) and handoff operations
    Stm {
        #[command(subcommand)]
        action: StmAction,
    },
    /// Vault management, lifecycle, ingestion, and auditing
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
    /// Harness and LLM configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Install the git pre-commit hook to sanitize secrets
    InstallHook,
    /// Run secret filtering on staged files (internal pre-commit hook)
    PreCommit,
    /// Execute a command safely in the Mythrax environment
    Exec {
        /// The command to run
        command_name: String,
        /// Arguments to pass to the command
        args: Vec<String>,
    },
    /// Bootstrap local memory from historical logs and customizations
    Bootstrap {
        /// Dry-run mode to print what would be bootstrapped without saving
        #[arg(short, long)]
        dry_run: bool,
        /// Ingest only conversations modified since this ISO timestamp
        #[arg(long)]
        since: Option<String>,
        /// Scope tag (defaults to 'general')
        #[arg(short, long, default_value = "general")]
        scope: String,
        /// Model to use for distillation
        #[arg(long)]
        distill_model: Option<String>,
        /// Force re-processing of already processed conversations
        #[arg(short, long)]
        force: bool,
    },
    /// Bulk ingest logs into the memory store in chronological batches
    Ingest {
        /// Path to the log directory or file
        #[arg(short, long)]
        source: String,
        /// Harness type (e.g. 'antigravity', 'claude', 'cursor', etc.)
        #[arg(short, long)]
        harness: String,
        /// Optional scope (defaults to 'general')
        #[arg(long, default_value = "general")]
        scope: String,
        /// Batch size for chunked ingestion
        #[arg(long, default_value_t = 50)]
        batch_size: usize,
    },
    /// Run the pre-invocation hook (reads stdin, queries daemon, prints stdout)
    PreInvocation,
}

#[derive(Subcommand, Debug, Clone)]
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

#[derive(Subcommand, Debug, Clone)]
pub enum MemoryAction {
    /// Query the long-term memory store (LTM)
    Query {
        /// The search query text
        query: String,
        /// Optional scope to filter by
        #[arg(short, long)]
        scope: Option<String>,
        /// Maximum number of results
        #[arg(short, long, default_value_t = 15)]
        limit: usize,
        /// Optional offset for pagination
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Similarity threshold (0.0 to 1.0)
        #[arg(short, long, default_value_t = 0.55)]
        threshold: f32,
        /// Token budget for truncation
        #[arg(long)]
        token_budget: Option<usize>,
        /// Allow downward graph traversal
        #[arg(long)]
        allow_downward: bool,
        /// Include episodes in search results
        #[arg(long)]
        include_episodes: bool,
        /// Include artifacts in search results
        #[arg(long)]
        include_artifacts: bool,
        /// Optional session ID for tracking citations
        #[arg(long)]
        session_id: Option<String>,
    },
    /// Record an episodic memory in the vault
    Record {
        /// Title of the memory note
        title: String,
        /// Path to the markdown file containing the body
        #[arg(short, long)]
        file: String,
        /// Optional scope for the saved episode
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Record reinforcement learning feedback for a rule
    Feedback {
        /// The rule record ID (e.g. 'wisdom:rule_abc')
        id: String,
        /// Success flag
        success: bool,
    },
    /// Get the active Obsidian vault root directory path
    Root,
}

#[derive(Subcommand, Debug, Clone)]
pub enum StmAction {
    /// Store a key-value pair in session-based short-term memory
    Put {
        session_id: String,
        key: String,
        value: String,
    },
    /// Retrieve a stashed STM variable or list all active variables
    Get {
        session_id: String,
        key: Option<String>,
    },
    /// Clear all short-term memory variables for a session
    Clear {
        session_id: String,
    },
    /// Save a parent-to-subagent task handoff and link context
    Handoff {
        parent_conversation_id: String,
        subagent_conversation_id: String,
        summary: String,
        handoff_file_path: String,
        #[arg(long)]
        scope: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum VaultAction {
    /// Organize the vault files by renaming and resolving duplicates
    Organize,
    /// Verify vault integrity and run self-healing repairs
    Verify {
        /// Fix any issues found
        #[arg(short, long)]
        fix: bool,
    },
    /// Reprocess episodes with missing vector embeddings
    Reprocess,
    /// Summarize episodes and generate system wisdom rules
    Summarize {
        /// Optional scope
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Bulk ingest logs into the memory store
    IngestBulk {
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
    /// Forge a source document to extract rules and wiki nodes
    IngestForge {
        /// Path to the source file (text, markdown, or PDF)
        source_path: String,
        /// Optional scope (defaults to 'general')
        #[arg(short, long)]
        scope: Option<String>,
    },
    /// Run safety compliance audits on the active directory
    Audit {
        /// Workspace directory to audit
        #[arg(short, long)]
        workspace: Option<String>,
    },
    /// Clean stale sessions and git branches from the vault
    Clean {
        /// Dry-run mode to print what would be cleaned without executing
        #[arg(short, long)]
        dry_run: bool,
        /// Automatically confirm cleanup (skip interactive prompt)
        #[arg(short, long)]
        confirm: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigAction {
    /// Get the current LLM configuration
    Get,
    /// Update the LLM configuration settings
    Set {
        /// Provider type ('local' or 'cloud')
        #[arg(short, long)]
        provider: String,
        /// Duration ('temporary' or 'permanent')
        #[arg(short, long)]
        duration: Option<String>,
        /// Model identifier
        #[arg(short, long)]
        model: Option<String>,
        /// Cloud provider name
        #[arg(long)]
        cloud_provider: Option<String>,
        /// API Key for cloud access
        #[arg(long)]
        api_key: Option<String>,
    },
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

pub use crate::vault::operations::{handle_merge_vault, run_auditor, stringify_record_id};
