mod analyzer;
mod cache;
mod cli;
mod config;
mod daemon;
mod diff;
mod explain;
mod git;
mod language;
mod model;
mod runtime;
mod server;
mod snapshot;
mod web;

use analyzer::RepositoryAnalyzer;
use anyhow::Result;
use clap::Parser;
use cli::{CacheAction, Cli, Command, ConfigAction};
use config::{format_show, init_user_config, ConfigLoader};
use snapshot::SnapshotGeneration;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{}", user_facing_error(&error));
        std::process::exit(error_exit_code(&error));
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Daemon(command)) = &cli.command {
        return daemon::command(&command.action).await;
    }
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
    if let Some(Command::Cache(command)) = &cli.command {
        let cache = cache::ExplanationCache::open(&git::git_dir(&repo)?)?;
        match command.action {
            CacheAction::Status => println!(
                "Cache enabled: {}\nBackend: SQLite\nEntries: {}\nLocation: {}",
                if resolved.cache.enabled { "yes" } else { "no" },
                cache.count()?,
                cache.path().display()
            ),
            CacheAction::Clear => println!(
                "Cleared {} cache entries from {}",
                cache.clear()?,
                cache.path().display()
            ),
        }
        return Ok(());
    }
    if !cli.debug && !cli.direct {
        return daemon::open_repository(
            &repo,
            cli.revision.as_deref(),
            cli.profile.as_deref(),
            cli.port,
        )
        .await;
    }
    if cli.direct {
        return run_direct(repo, resolved, cli.revision.as_deref(), cli.port, cli.debug).await;
    }
    let analyzer = RepositoryAnalyzer::new(&repo, resolved.clone());
    let snapshot = if let Some(revision) = cli.revision.as_deref() {
        analyzer.analyze_commit(revision, SnapshotGeneration(1))?
    } else {
        analyzer.analyze_working_tree(SnapshotGeneration(1))?
    };
    if let Some(message) = snapshot.context.no_op.as_deref() {
        println!("{message}");
        return Ok(());
    }
    if cli.debug {
        explain::print_debug(&snapshot)?;
        return Ok(());
    }
    unreachable!("debug path returned above")
}

fn user_facing_error(error: &anyhow::Error) -> String {
    let details = format!("{error:#}");
    if details.contains("not inside a Git repository") {
        return "This command must be run inside a Git repository. Run it from a repository containing the changes or commit to explain.".into();
    }
    if details.contains("Unable to resolve Git revision") {
        return format!(
            "The requested Git revision could not be resolved. Check the revision name and try again.\nDetails: {details}"
        );
    }
    if details.contains("unknown model profile") {
        return format!(
            "The configured model profile is not available. Run `git explain config show` to inspect the available profiles.\nDetails: {details}"
        );
    }
    if details.contains("daemon") || details.contains("bind loopback") {
        return format!(
            "The git-explain daemon could not complete the request. Try `git explain daemon status` or use `git explain --direct`.\nDetails: {details}"
        );
    }
    if details.contains("connect") || details.contains("timed out") || details.contains("model") {
        return format!(
            "The configured model service could not complete the request. Run `git explain config show` to inspect the endpoint, then check that the model server is running.\nDetails: {details}"
        );
    }
    details
}

fn error_exit_code(error: &anyhow::Error) -> i32 {
    let details = format!("{error:#}");
    if details.contains("not inside a Git repository")
        || details.contains("Unable to resolve Git revision")
        || details.contains("unknown model profile")
        || details.contains("parse configuration")
        || details.contains("invalid repository path")
    {
        2
    } else if details.contains("daemon") || details.contains("bind loopback") {
        4
    } else if details.contains("connect")
        || details.contains("timed out")
        || details.contains("model")
    {
        3
    } else {
        1
    }
}

async fn run_direct(
    repo: std::path::PathBuf,
    resolved: config::ResolvedConfig,
    revision: Option<&str>,
    port: Option<u16>,
    debug: bool,
) -> Result<()> {
    let analyzer = RepositoryAnalyzer::new(&repo, resolved.clone());
    let snapshot = if let Some(revision) = revision {
        analyzer.analyze_commit(revision, SnapshotGeneration(1))?
    } else {
        analyzer.analyze_working_tree(SnapshotGeneration(1))?
    };
    if let Some(message) = snapshot.context.no_op.as_deref() {
        println!("{message}");
        return Ok(());
    }
    if debug {
        explain::print_debug(&snapshot)?;
        return Ok(());
    }
    let provider = model::openai::OpenAiProvider::from_config(
        resolved.model.clone(),
        resolved.reader.clone(),
        resolved.explanation.clone(),
    );
    let mut server = resolved.server;
    if let Some(port) = port {
        server.port = port;
    }
    let cache = if resolved.cache.enabled {
        Some(cache::ExplanationCache::open(&git::git_dir(&repo)?)?)
    } else {
        None
    };
    server::serve(
        snapshot,
        provider,
        server,
        cache,
        resolved.model,
        resolved.reader,
        resolved.explanation,
    )
    .await
}
