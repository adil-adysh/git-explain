use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "git-explain",
    about = "Read and understand code changed in the working tree or a Git commit",
    after_help = "Examples:\n  git explain                 Explain current changes\n  git explain HEAD            Explain an existing commit\n  git explain --direct        Bypass the background daemon\n  git explain --debug         Inspect changes without opening a browser"
)]
pub struct Cli {
    #[arg(
        value_name = "REVISION",
        help = "Explain a Git commit instead of the working tree"
    )]
    pub revision: Option<String>,
    #[arg(long, help = "Print changed supported-language functions and exit")]
    pub debug: bool,
    #[arg(
        long,
        help = "Override the configured model profile for this invocation"
    )]
    pub profile: Option<String>,
    #[arg(long, help = "Override the configured web-server port")]
    pub port: Option<u16>,
    #[arg(long, help = "Bypass the background daemon and use a one-shot server")]
    pub direct: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    #[command(about = "Initialize and inspect configuration")]
    Config(ConfigCommand),
    #[command(about = "Inspect or clear the explanation cache")]
    Cache(CacheCommand),
    #[command(about = "Start, inspect, refresh, or stop the local daemon")]
    Daemon(DaemonCommand),
}

#[derive(Clone, Debug, Args)]
pub struct DaemonCommand {
    #[command(subcommand)]
    pub action: DaemonAction,
}

#[derive(Clone, Debug, Subcommand)]
pub enum DaemonAction {
    #[command(about = "Start the daemon if it is not already running")]
    Start,
    #[command(about = "Stop the daemon gracefully")]
    Stop,
    #[command(about = "Show daemon health and endpoint details")]
    Status,
    #[command(about = "Reanalyze the active repository without model inference")]
    Refresh,
    #[command(about = "Run the daemon in the foreground")]
    Run,
}

#[derive(Clone, Debug, Args)]
pub struct CacheCommand {
    #[command(subcommand)]
    pub action: CacheAction,
}
#[derive(Clone, Debug, Subcommand)]
pub enum CacheAction {
    Status,
    Clear,
}

#[derive(Clone, Debug, Args)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Clone, Debug, Subcommand)]
pub enum ConfigAction {
    #[command(about = "Show the resolved configuration with secrets redacted")]
    Show,
    #[command(about = "Show the user and repository config paths")]
    Path,
    Init {
        #[arg(long)]
        force: bool,
    },
}
