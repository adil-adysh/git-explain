mod cli;
mod diff;
mod explain;
mod git;
mod language;
mod model;
mod server;
mod web;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo = git::repository_root()?;
    let changes = git::working_tree_changes(&repo)?;
    if changes.is_empty() {
        anyhow::bail!("No supported changes found relative to HEAD.");
    }
    if cli.debug {
        explain::print_debug(&repo, &changes)?;
        return Ok(());
    }
    let provider = model::openai::OpenAiProvider::from_env();
    let items = explain::explain_items(&repo, &changes, provider.clone()).await?;
    server::serve(items, provider, cli.port).await
}
