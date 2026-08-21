mod cli;
mod config;
mod diff;
mod explain;
mod git;
mod language;
mod model;
mod server;
mod web;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command, ConfigAction};
use config::{format_show, init_user_config, ConfigLoader};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Config(command)) = &cli.command {
        let repository = git::repository_root().ok();
        let loader = ConfigLoader::for_repository(repository.as_deref())?;
        match &command.action {
            ConfigAction::Path => {
                println!("User config:\n{}", loader.paths.user.display());
                println!(
                    "Repository config:\n{}",
                    loader.paths.repository.as_ref().map_or_else(
                        || "<not in a Git repository>".into(),
                        |path| path.display().to_string()
                    )
                );
            }
            ConfigAction::Init { force } => {
                let created = init_user_config(&loader.paths.user, *force)?;
                if created {
                    println!("Created {}", loader.paths.user.display());
                } else {
                    anyhow::bail!(
                        "{} already exists; use --force to overwrite it",
                        loader.paths.user.display()
                    );
                }
            }
            ConfigAction::Show => {
                let resolved = loader.resolve(cli.profile.as_deref())?;
                print!("{}", format_show(&resolved));
            }
        }
        return Ok(());
    }
    let repo = git::repository_root()?;
    let loader = ConfigLoader::for_repository(Some(&repo))?;
    let resolved = loader.resolve(cli.profile.as_deref())?;
    let changes = git::working_tree_changes(&repo, &resolved.git)?;
    if changes.is_empty() {
        anyhow::bail!("No supported changes found relative to HEAD.");
    }
    if cli.debug {
        explain::print_debug(&repo, &changes)?;
        return Ok(());
    }
    let default_depth_deep = resolved
        .explanation
        .default_depth
        .eq_ignore_ascii_case("deep");
    let provider = model::openai::OpenAiProvider::from_config(
        resolved.model,
        resolved.reader,
        resolved.explanation.clone(),
    );
    let items =
        explain::explain_items(&repo, &changes, provider.clone(), default_depth_deep).await?;
    let mut server = resolved.server;
    if let Some(port) = cli.port {
        server.port = port;
    }
    server::serve(items, provider, server).await
}
