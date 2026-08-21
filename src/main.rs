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
use explain::{
    AnalysisContext, AnalysisMode, GitCommitSourceProvider, SourceProvider,
    WorkingTreeSourceProvider,
};

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
    let (changes, source_provider, context): (Vec<_>, Box<dyn SourceProvider>, AnalysisContext) =
        if let Some(revision) = cli.revision.as_deref() {
            let analysis = git::commit_analysis(&repo, revision)?;
            let context = AnalysisContext {
                mode: AnalysisMode::Commit {
                    oid: analysis.oid.clone(),
                    parent_oid: analysis.parent_oid.clone(),
                    subject: analysis.subject.clone(),
                    merge_parent_count: analysis.parent_count,
                },
                deleted_files: analysis
                    .changes
                    .iter()
                    .filter(|change| change.kind == crate::diff::ChangeKind::Deleted)
                    .map(|change| change.path.display().to_string())
                    .collect(),
            };
            (
                analysis.changes,
                Box::new(GitCommitSourceProvider::new(&repo, analysis.oid)),
                context,
            )
        } else {
            (
                git::working_tree_changes(&repo, &resolved.git)?,
                Box::new(WorkingTreeSourceProvider::new(&repo)),
                AnalysisContext::working_tree(),
            )
        };
    if changes.is_empty() {
        anyhow::bail!("No supported changes found for analysis.");
    }
    if cli.debug {
        explain::print_debug(source_provider.as_ref(), &changes, &context)?;
        return Ok(());
    }
    let provider = model::openai::OpenAiProvider::from_config(
        resolved.model,
        resolved.reader,
        resolved.explanation.clone(),
    );
    let items = explain::explain_items(
        source_provider.as_ref(),
        &changes,
        provider.clone(),
        &context,
        false,
    )
    .await?;
    let mut server = resolved.server;
    if let Some(port) = cli.port {
        server.port = port;
    }
    server::serve(items, provider, context, server).await
}
