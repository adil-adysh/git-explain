use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "git-explain",
    about = "Read and understand code changed in the working tree"
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
    #[arg(long, hide = true, help = "Use the original one-shot server path")]
    pub direct: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    Config(ConfigCommand),
    Cache(CacheCommand),
    Daemon(DaemonCommand),
}

#[derive(Clone, Debug, Args)]
pub struct DaemonCommand {
    #[command(subcommand)]
    pub action: DaemonAction,
}

#[derive(Clone, Debug, Subcommand)]
pub enum DaemonAction {
    Start,
    Stop,
    Status,
    Refresh,
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
    Show,
    Path,
    Init {
        #[arg(long)]
        force: bool,
    },
}
