mod analyzer;
mod cache;
mod cli;
mod config;
mod config_editor;
mod context;
mod daemon;
mod diff;
mod explain;
mod git;
mod language;
mod model;
mod profile_editor;
mod runtime;
mod server;
mod snapshot;
mod terminal;
mod web;

use analyzer::RepositoryAnalyzer;
use anyhow::{Context, Result};
use clap::Parser;
use cli::{CacheAction, Cli, Command, ConfigAction, ContextAction, ProfileAction};
use config::{
    add_profile_with_update, display_preset, display_provider, edit_config, edit_profile,
    format_application_show, format_profile_show, init_repository_config, init_user_config,
    profile_names, remove_profile, use_profile, use_repository_profile, ConfigLoader, ConfigUpdate,
    ListUpdate, ProfileDraft, ProfileNotFound, ProfileUpdate,
};
use git_explain::ollama_context;
use snapshot::SnapshotGeneration;
use std::io::{self, IsTerminal};

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
    if let Some(Command::Context(command)) = &cli.command {
        let loader = ConfigLoader::for_context(None)?;
        let resolved = loader.resolve(cli.profile.as_deref())?;
        if resolved.model.preset.as_deref() != Some("ollama") {
            anyhow::bail!("Context history is currently available only for an Ollama profile.");
        }
        let tracker = ollama_context::OllamaRequestTracker::for_user_config(&loader.paths.user);
        match &command.action {
            ContextAction::Stats => {
                print_context_stats(&resolved.profile, &resolved.model, &tracker)
            }
            ContextAction::Recommend => {
                let caps = model::openai::discover_context_capabilities(&resolved.model).await;
                let records = tracker.records(&resolved.profile, false);
                let recommendation = ollama_context::recommend(
                    &records,
                    caps.capacity.runtime_allocated,
                    caps.capacity.model_max,
                );
                println!("Context recommendation\n\nProfile: {}\nCurrent Ollama context: {}\nRecommended context: {}\n\n{}", resolved.profile, caps.capacity.runtime_allocated.map_or_else(|| "not loaded".into(), |v| format!("{v} tokens")), recommendation.recommended.map_or_else(|| "not available".into(), |v| format!("{v} tokens")), recommendation.reason);
            }
            ContextAction::Reset { force } => {
                if !force {
                    anyhow::bail!("Context reset is destructive. Re-run with --force to remove local history for profile '{}'.", resolved.profile);
                }
                println!(
                    "Removed {} local context records for profile {}.",
                    tracker.reset(&resolved.profile)?,
                    resolved.profile
                );
            }
        }
        return Ok(());
    }
    if let Some(Command::Config(command)) = &cli.command {
        let repository = git::RepositoryContext::discover().ok();
        let loader = ConfigLoader::for_context(repository.as_ref())?;
        match &command.action {
            ConfigAction::Path => {
                println!("User configuration:\n{}", loader.paths.user.display());
                println!(
                    "Repository configuration:\n{}",
                    loader.paths.repository.as_ref().map_or_else(
                        || "not available outside a Git repository".into(),
                        |path| path.display().to_string()
                    )
                );
            }
            ConfigAction::Init { force, repo } => {
                let (path, created) = if *repo {
                    let context = git::RepositoryContext::discover()?;
                    let path = config::repository_config_path(&context.git_dir);
                    let created = init_repository_config(&path, *force)?;
                    (path, created)
                } else {
                    let created = init_user_config(&loader.paths.user, *force)?;
                    (loader.paths.user.clone(), created)
                };
                if created {
                    println!("Created {}", path.display());
                } else {
                    anyhow::bail!(
                        "{} already exists; use --force to overwrite it",
                        path.display()
                    );
                }
            }
            ConfigAction::Show => {
                let application = loader.application_config(cli.profile.as_deref())?;
                print!("{}", format_application_show(&application));
            }
            ConfigAction::Edit { repo } => {
                if !io::stdin().is_terminal() {
                    anyhow::bail!("Interactive configuration editing is unavailable because standard input is not a terminal.\n\nUse a section command instead.\n\nExamples:\ngit explain config server --port 9000\ngit explain config cache --enabled false");
                }
                let path = config_path(*repo, &loader)?;
                let names = profile_names(&loader.paths.user)?;
                let current = loader.application_config(None)?;
                let mut input = io::stdin().lock();
                let stdout = io::stdout();
                let mut output = stdout.lock();
                config_editor::run(&mut input, &mut output, &path, *repo, &names, &current)?;
            }
            ConfigAction::Reader {
                experience,
                add_known_language,
                remove_known_language,
                clear_known_languages,
                add_learning_language,
                remove_learning_language,
                clear_learning_languages,
                add_known_framework,
                remove_known_framework,
                clear_known_frameworks,
                add_learning_framework,
                remove_learning_framework,
                clear_learning_frameworks,
                repo,
            } => {
                let update = ConfigUpdate {
                    reader: config::ReaderUpdate {
                        experience: experience.clone(),
                        known_languages: ListUpdate {
                            add: add_known_language.clone(),
                            remove: remove_known_language.clone(),
                            clear: *clear_known_languages,
                        },
                        learning_languages: ListUpdate {
                            add: add_learning_language.clone(),
                            remove: remove_learning_language.clone(),
                            clear: *clear_learning_languages,
                        },
                        known_frameworks: ListUpdate {
                            add: add_known_framework.clone(),
                            remove: remove_known_framework.clone(),
                            clear: *clear_known_frameworks,
                        },
                        learning_frameworks: ListUpdate {
                            add: add_learning_framework.clone(),
                            remove: remove_learning_framework.clone(),
                            clear: *clear_learning_frameworks,
                        },
                    },
                    ..Default::default()
                };
                persist_config_update(&loader, *repo, update)?;
            }
            ConfigAction::Explanation {
                depth,
                annotation_limit,
                annotation_word_limit,
                explain_language_concepts,
                explain_framework_concepts,
                infer_intent,
                repo,
            } => {
                let update = ConfigUpdate {
                    explanation: config::ExplanationUpdate {
                        default_depth: depth.clone(),
                        max_annotations: *annotation_limit,
                        max_annotation_words: *annotation_word_limit,
                        explain_language_concepts: *explain_language_concepts,
                        explain_framework_concepts: *explain_framework_concepts,
                        infer_intent: *infer_intent,
                    },
                    ..Default::default()
                };
                persist_config_update(&loader, *repo, update)?;
            }
            ConfigAction::Cache { enabled, repo } => persist_config_update(
                &loader,
                *repo,
                ConfigUpdate {
                    cache: config::CacheUpdate { enabled: *enabled },
                    ..Default::default()
                },
            )?,
            ConfigAction::Server {
                host,
                port,
                open_browser,
                repo,
            } => persist_config_update(
                &loader,
                *repo,
                ConfigUpdate {
                    server: config::ServerUpdate {
                        host: host.clone(),
                        port: *port,
                        open_browser: *open_browser,
                    },
                    ..Default::default()
                },
            )?,
            ConfigAction::Git {
                diff_target,
                include_staged,
                include_untracked,
                repo,
            } => persist_config_update(
                &loader,
                *repo,
                ConfigUpdate {
                    git: config::GitUpdate {
                        diff_target: diff_target.clone(),
                        include_staged: *include_staged,
                        include_untracked: *include_untracked,
                    },
                    ..Default::default()
                },
            )?,
            ConfigAction::Model {
                profile,
                clear_profile,
                repo,
            } => persist_config_update(
                &loader,
                *repo,
                ConfigUpdate {
                    model: config::ModelSelectionUpdate {
                        profile: profile.clone(),
                        clear_profile: *clear_profile,
                    },
                    ..Default::default()
                },
            )?,
        }
        return Ok(());
    }
    if let Some(Command::Profile(command)) = &cli.command {
        let repository = git::RepositoryContext::discover().ok();
        let loader = ConfigLoader::for_context(repository.as_ref())?;
        match &command.action {
            ProfileAction::List => {
                let names = profile_names(&loader.paths.user)?;
                if names.is_empty() {
                    println!("No model profiles configured.\n\nLocal llama.cpp example:\ngit explain profile add local --preset llama-cpp --model <model>\n\nOllama example:\ngit explain profile add ollama --preset ollama --model <model>");
                } else {
                    let user_loader = ConfigLoader::for_context(None)?;
                    let user_default = user_loader.resolve(None).ok().map(|config| config.profile);
                    let repository_selection = loader.resolve(None).ok().and_then(|config| {
                        (config.profile_selection_source
                            == config::ProfileSelectionSource::Repository)
                            .then_some(config.profile)
                    });
                    if let Some(default) = &user_default {
                        println!("User default: {default}");
                    }
                    if let Some(selected) = &repository_selection {
                        println!("Repository selection: {selected}");
                    }
                    println!("\nProfiles:");
                    for name in names {
                        let profile = user_loader.resolve(Some(&name))?;
                        println!("\n{name}\n  Default: {}\n  Provider: {}\n  Preset: {}\n  Model: {}\n  Base URL: {}", if user_default.as_deref() == Some(&name) { "yes" } else { "no" }, display_provider_name(&profile.model.provider), profile.model.preset.as_deref().map(display_preset_name).unwrap_or("<none>"), profile.model.model, profile.model.base_url);
                    }
                }
            }
            ProfileAction::Show { name } => {
                let user_loader = ConfigLoader::for_context(None)?;
                ensure_profile_exists(&user_loader.paths.user, name, "show")?;
                let resolved = user_loader.resolve(Some(name))?;
                print!("{}", format_profile_show(&resolved));
            }
            ProfileAction::Use { name, repo } => {
                if *repo {
                    // Resolve from user configuration first so a repository can never
                    // create or select an incomplete untrusted profile.
                    let context = git::RepositoryContext::discover()?;
                    let user_loader = ConfigLoader::for_context(None)?;
                    ensure_profile_exists(&user_loader.paths.user, name, "use")?;
                    user_loader.resolve(Some(name))?;
                    let path = config::repository_config_path(&context.git_dir);
                    use_repository_profile(&path, name)?;
                    println!(
                        "Repository profile changed.\n\nRepository:\n{}\n\nProfile:\n{name}",
                        context.worktree_root.display()
                    );
                } else {
                    ensure_profile_exists(&loader.paths.user, name, "use")?;
                    use_profile(&loader.paths.user, name)?;
                    println!("Default profile changed to: {name}");
                }
            }
            ProfileAction::Add {
                name,
                provider,
                preset,
                base_url,
                model_port,
                model,
                api_key_env,
                context_window,
                normal_reasoning,
                normal_max_tokens,
                normal_temperature,
                deep_reasoning,
                deep_max_tokens,
                deep_temperature,
            } => {
                if name.is_none()
                    && provider.is_none()
                    && preset.is_none()
                    && base_url.is_none()
                    && model_port.is_none()
                    && model.is_none()
                    && api_key_env.is_none()
                    && context_window.is_none()
                    && normal_reasoning.is_none()
                    && normal_max_tokens.is_none()
                    && normal_temperature.is_none()
                    && deep_reasoning.is_none()
                    && deep_max_tokens.is_none()
                    && deep_temperature.is_none()
                {
                    if !io::stdin().is_terminal() {
                        anyhow::bail!("Profile creation is unavailable because standard input is not a terminal.\n\nUse explicit options instead.\n\nExample:\ngit explain profile add local --preset llama-cpp --model <model>\n\nRun:\ngit explain profile add -h");
                    }
                    let mut input = io::stdin().lock();
                    let stdout = io::stdout();
                    let mut output = stdout.lock();
                    profile_editor::run_add(&mut input, &mut output, &loader.paths.user)?;
                    return Ok(());
                }
                let name = name.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("a profile name is required when using explicit options")
                })?;
                let model = model.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("--model is required when using explicit options")
                })?;
                add_profile_with_update(
                    &loader.paths.user,
                    ProfileDraft {
                        name: name.into(),
                        provider: provider.as_ref().map(|value| value.replace('-', "_")),
                        preset: preset.as_ref().map(|value| value.replace('-', "_")),
                        base_url: base_url.clone(),
                        model_port: *model_port,
                        model: model.into(),
                        api_key_env: api_key_env.clone(),
                    },
                    ProfileUpdate {
                        context_window: *context_window,
                        normal_reasoning: *normal_reasoning,
                        normal_max_tokens: *normal_max_tokens,
                        normal_temperature: *normal_temperature,
                        deep_reasoning: *deep_reasoning,
                        deep_max_tokens: *deep_max_tokens,
                        deep_temperature: *deep_temperature,
                        ..ProfileUpdate::default()
                    },
                )?;
                let created = ConfigLoader::for_context(None)?.resolve(Some(name))?;
                println!("Created profile: {name}\n\nProvider: {}\nPreset: {}\nBase URL: {}\nModel: {}\n\nNext:\ngit explain profile test {name}", display_provider(&created.model.provider), created.model.preset.as_deref().map(display_preset).unwrap_or("<none>"), created.model.base_url, created.model.model);
            }
            ProfileAction::Edit {
                name,
                provider,
                preset,
                base_url,
                model_port,
                model,
                api_key_env,
                context_window,
                clear_preset,
                clear_api_key_env,
                clear_context_window,
                normal_reasoning,
                normal_max_tokens,
                normal_temperature,
                deep_reasoning,
                deep_max_tokens,
                deep_temperature,
                clear_normal_reasoning,
                clear_normal_max_tokens,
                clear_normal_temperature,
                clear_deep_reasoning,
                clear_deep_max_tokens,
                clear_deep_temperature,
            } => {
                ensure_profile_exists(&loader.paths.user, name, "edit")?;
                let update = ProfileUpdate {
                    provider: provider.as_ref().map(|value| value.replace('-', "_")),
                    preset: preset.as_ref().map(|value| value.replace('-', "_")),
                    base_url: base_url.clone(),
                    model_port: *model_port,
                    model: model.clone(),
                    api_key_env: api_key_env.clone(),
                    context_window: *context_window,
                    clear_preset: *clear_preset,
                    clear_api_key_env: *clear_api_key_env,
                    clear_context_window: *clear_context_window,
                    normal_reasoning: *normal_reasoning,
                    normal_max_tokens: *normal_max_tokens,
                    normal_temperature: *normal_temperature,
                    deep_reasoning: *deep_reasoning,
                    deep_max_tokens: *deep_max_tokens,
                    deep_temperature: *deep_temperature,
                    clear_normal_reasoning: *clear_normal_reasoning,
                    clear_normal_max_tokens: *clear_normal_max_tokens,
                    clear_normal_temperature: *clear_normal_temperature,
                    clear_deep_reasoning: *clear_deep_reasoning,
                    clear_deep_max_tokens: *clear_deep_max_tokens,
                    clear_deep_temperature: *clear_deep_temperature,
                };
                let current = if update.has_changes() {
                    None
                } else {
                    Some(ConfigLoader::for_context(None)?.resolve(Some(name))?)
                };
                if update.has_changes() {
                    edit_profile(&loader.paths.user, name, update)?;
                    println!("Updated profile: {name}");
                } else if !io::stdin().is_terminal() {
                    anyhow::bail!("No profile changes were specified.\n\nInteractive editing is unavailable because standard input is not a terminal.\n\nUse explicit options instead.\n\nExample:\ngit explain profile edit {name} --model <model>\n\nRun:\ngit explain profile edit -h");
                } else {
                    let mut input = io::stdin().lock();
                    let stdout = io::stdout();
                    let mut output = stdout.lock();
                    profile_editor::run(
                        &mut input,
                        &mut output,
                        &loader.paths.user,
                        name,
                        &current.expect("interactive profile was resolved").model,
                    )?;
                }
            }
            ProfileAction::Remove { name } => {
                ensure_profile_exists(&loader.paths.user, name, "remove")?;
                let user_loader = ConfigLoader::for_context(None)?;
                if user_loader
                    .resolve(None)
                    .ok()
                    .is_some_and(|config| config.profile == *name)
                {
                    anyhow::bail!("Cannot remove profile \"{name}\" because it is the user default.\n\nChoose another default first:\n\ngit explain profile use <name>");
                }
                if loader.resolve(None).ok().is_some_and(|config| {
                    config.profile == *name
                        && config.profile_selection_source
                            == config::ProfileSelectionSource::Repository
                }) {
                    anyhow::bail!("Cannot remove profile \"{name}\".\n\nThe current repository selects this profile.\n\nChoose another repository profile first:\n\ngit explain profile use <name> --repo");
                }
                remove_profile(&loader.paths.user, name)?;
                println!("Removed profile: {name}");
            }
            ProfileAction::Test { name } => {
                let user_loader = ConfigLoader::for_context(None)?;
                ensure_profile_exists(&user_loader.paths.user, name, "test")?;
                let resolved = user_loader.resolve(Some(name))?;
                print!("{}", test_profile(name, &resolved.model).await?);
            }
        }
        return Ok(());
    }
    let context = git::RepositoryContext::discover()?;
    let repo = context.worktree_root.clone();
    let loader = ConfigLoader::for_context(Some(&context))?;
    if cli.debug {
        let application = loader.application_config(cli.profile.as_deref())?;
        let analyzer = RepositoryAnalyzer::with_git_config(&repo, application.git);
        let snapshot = if let Some(revision) = cli.revision.as_deref() {
            analyzer.analyze_commit(revision, SnapshotGeneration(1))?
        } else {
            analyzer.analyze_working_tree(SnapshotGeneration(1))?
        };
        if let Some(message) = snapshot.context.no_op.as_deref() {
            println!("{message}");
        } else {
            explain::print_debug(&snapshot)?;
            if let Ok(resolved) = loader.resolve(cli.profile.as_deref()) {
                print_debug_context(&resolved.model);
            }
        }
        return Ok(());
    }
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
    if cli.direct {
        return run_direct(repo, resolved, cli.revision.as_deref(), cli.port, cli.debug).await;
    }
    daemon::open_repository(
        &repo,
        cli.revision.as_deref(),
        cli.profile.as_deref(),
        cli.port,
    )
    .await
}

fn print_context_stats(
    profile: &str,
    model: &config::ResolvedProfile,
    tracker: &ollama_context::OllamaRequestTracker,
) {
    let records = tracker.records(profile, false);
    let stats = ollama_context::OllamaContextStatistics::from_records(&records);
    println!("Context statistics\n\nProfile: {profile}\nPreset: Ollama\nModel: {}\n\nRequests tracked: {}\n\nRequired context:\n  p50: {}\n  p90: {}\n  p95: {}\n  max: {}\n\nCompaction: {} requests\nHard context failures: {}\nProvider context overflows: {}\nOutput truncations: {}\nAverage latency: {}", model.model, stats.count, stats.required_p50.map_or_else(|| "insufficient samples".into(), |v| format!("{v} tokens")), stats.required_p90.map_or_else(|| "insufficient samples".into(), |v| format!("{v} tokens")), stats.required_p95.map_or_else(|| "insufficient samples".into(), |v| format!("{v} tokens")), stats.required_max.map_or_else(|| "none".into(), |v| format!("{v} tokens")), stats.compactions, stats.hard_failures, stats.overflows, stats.truncations, stats.average_latency_ms.map_or_else(|| "unavailable".into(), |v| format!("{v} ms")));
}

fn print_debug_context(model: &config::ResolvedProfile) {
    let capacity = context::ContextCapacity {
        profile_limit: model.context_window,
        ..Default::default()
    };
    let control = model::openai::context_control_for_profile(model);
    for (label, generation, deep) in [
        ("Normal", &model.normal, false),
        ("Deep", &model.deep, true),
    ] {
        let requirement = context::calculate_context_requirement(
            "",
            generation,
            deep,
            &context::ConservativeTokenEstimator,
        );
        let negotiation = context::negotiate_context(
            &context::ContextCapabilities {
                capacity: capacity.clone(),
                control,
            },
            requirement,
        );
        let (budget, required) = match negotiation {
            Ok(negotiation) => (
                context::ContextBudget::from_negotiation(&negotiation),
                negotiation.requirement.minimum_required_context,
            ),
            Err(error) => {
                println!(
                    "\nContext planning ({label}, no network discovery)\n  Control: {}\n  Result: {error}",
                    control.description(),
                );
                continue;
            }
        };
        println!(
            "\nContext planning ({label}, no network discovery)\n  Control: {}\n  Requested context: none (not supported by this transport)\n  Estimated input: diagnostic baseline only\n  Output reserve: {} tokens\n  Safety/headroom: {} tokens\n  Minimum required context: {} tokens\n  Capacity: {} tokens ({:?})\n  Available input: {} tokens",
            control.description(),
            budget.output_reserve,
            budget.safety_margin,
            required,
            budget.total,
            capacity.effective().source,
            budget.input_budget,
        );
    }
}

fn display_provider_name(value: &str) -> &str {
    display_provider(value)
}
fn config_path(repo: bool, loader: &ConfigLoader) -> Result<std::path::PathBuf> {
    if repo {
        loader
            .paths
            .repository
            .clone()
            .context("This command must be run inside a Git repository when using --repo.")
    } else {
        Ok(loader.paths.user.clone())
    }
}
fn persist_config_update(loader: &ConfigLoader, repo: bool, update: ConfigUpdate) -> Result<()> {
    let path = config_path(repo, loader)?;
    let profiles = profile_names(&loader.paths.user)?;
    if edit_config(&path, repo, &update, &profiles)? {
        println!("Configuration updated: {}", path.display());
    } else {
        println!("No configuration changes were necessary.");
    }
    Ok(())
}
fn display_preset_name(value: &str) -> &str {
    display_preset(value)
}

fn ensure_profile_exists(path: &std::path::Path, name: &str, action: &str) -> Result<()> {
    let profiles = profile_names(path)?;
    if profiles.iter().any(|profile| profile == name) {
        return Ok(());
    }
    anyhow::bail!("{}", profile_not_found_message(name, &profiles, action));
}

fn profile_not_found_message(name: &str, profiles: &[String], action: &str) -> String {
    let available = if profiles.is_empty() {
        "  <none>".into()
    } else {
        profiles
            .iter()
            .map(|profile| format!("  {profile}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut message =
        format!("Profile \"{name}\" does not exist.\n\nAvailable profiles:\n{available}");
    if let Some(suggestion) = profile_suggestion(name, profiles) {
        message.push_str(&format!(
            "\n\nDid you mean:\n  {suggestion}\n\nTry:\n  git explain profile {action} {suggestion}"
        ));
    }
    message.push_str("\n\nOr list profiles:\n  git explain profile list");
    message
}

fn profile_suggestion<'a>(name: &str, profiles: &'a [String]) -> Option<&'a str> {
    let name = name.to_ascii_lowercase();
    let matches = profiles
        .iter()
        .filter(|profile| profile.to_ascii_lowercase().starts_with(&name))
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0].as_str())
}

async fn test_profile(name: &str, model: &config::ResolvedProfile) -> Result<String> {
    if model.provider != "openai_compatible" {
        anyhow::bail!(
            "unsupported provider '{}'; use openai_compatible",
            model.provider
        );
    }
    if let Some(environment) = &model.api_key_env {
        if model.api_key.is_none() {
            anyhow::bail!(
                "Profile: {name}\n\nRequired environment variable is not set:\n{environment}"
            );
        }
    }
    let mut request =
        reqwest::Client::new().get(format!("{}/models", model.base_url.trim_end_matches('/')));
    if let Some(key) = &model.api_key {
        request = request.bearer_auth(key);
    }
    let response = request.send().await.map_err(|_| {
        anyhow::anyhow!(
            "Profile: {name}\n\nCould not connect to:\n{}\n\nCheck that the model server is running and reachable.",
            model.base_url
        )
    })?;
    let listing_status = response.status();
    let listed_models = listing_status.is_success();
    if listed_models {
        let listing: serde_json::Value = response.json().await.context("read model listing")?;
        if let Some(models) = listing.get("data").and_then(serde_json::Value::as_array) {
            let found = models.iter().any(|entry| {
                entry.get("id").and_then(serde_json::Value::as_str) == Some(&model.model)
            });
            if !found {
                anyhow::bail!(
                    "Profile: {name}\n\nThe endpoint is reachable, but model \"{}\" was not found.",
                    model.model
                );
            }
        }
        if model.preset.as_deref() != Some("ollama")
            || model::openai::discover_context_capacity(model)
                .await
                .runtime_allocated
                .is_some()
        {
            return Ok(profile_test_success(name, model, "verified").await);
        }
        // The OpenAI model listing does not mean Ollama has loaded this model.
        // Continue to the tiny, source-free probe so /api/ps can report runtime context.
    } else if matches!(
        listing_status,
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    ) {
        anyhow::bail!("Profile: {name}\n\nThe model endpoint rejected authentication.\n\nCredential source:\n{}", model.api_key_env.as_deref().unwrap_or("<none>"));
    }
    if !listed_models
        && !matches!(
            listing_status,
            reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED
        )
    {
        anyhow::bail!(
            "Profile: {name}\n\nThe endpoint returned HTTP {} while listing models.",
            listing_status
        );
    }

    let mut probe = reqwest::Client::new()
        .post(format!(
            "{}/chat/completions",
            model.base_url.trim_end_matches('/')
        ))
        .json(&serde_json::json!({
            "model": model.model,
            "messages": [{"role": "user", "content": "Reply with OK."}],
            "max_tokens": 1,
        }));
    if let Some(key) = &model.api_key {
        probe = probe.bearer_auth(key);
    }
    let response = probe.send().await.map_err(|_| {
        anyhow::anyhow!(
            "Profile: {name}\n\nCould not complete a source-free compatibility check at:\n{}",
            model.base_url
        )
    })?;
    if response.status().is_success() {
        Ok(profile_test_success(name, model, "verified by a safe compatibility request").await)
    } else if matches!(
        response.status(),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    ) {
        anyhow::bail!("Profile: {name}\n\nThe model endpoint rejected authentication.\n\nCredential source:\n{}", model.api_key_env.as_deref().unwrap_or("<none>"));
    } else if response.status() == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!(
            "Profile: {name}\n\nThe endpoint is reachable, but model \"{}\" was not found.",
            model.model
        );
    } else {
        anyhow::bail!(
            "Profile: {name}\n\nThe source-free compatibility check failed with HTTP {}.",
            response.status()
        );
    }
}

async fn profile_test_success(
    name: &str,
    model: &config::ResolvedProfile,
    availability: &str,
) -> String {
    let capabilities = model::openai::discover_context_capabilities(model).await;
    let capacity = &capabilities.capacity;
    let effective = capacity.effective();
    let context = format!(
        "\nContext:\n  Control: {}\n  Requested context: none (not supported by this transport)\n  Model maximum: {}\n  Runtime allocated: {}\n  git-explain limit: {}\n  Effective context: {} tokens ({:?})\n",
        capabilities.control.description(),
        capacity.model_max.map_or_else(|| "unknown".into(), |value| format!("{value} tokens")),
        capacity.runtime_allocated.map_or_else(|| "unknown".into(), |value| format!("{value} tokens")),
        model.context_window.map_or_else(|| "automatic".into(), |value| format!("{value} tokens")),
        effective.tokens,
        effective.source,
    );
    let remediation = if model.preset.as_deref() == Some("ollama")
        && capacity.runtime_allocated == Some(4096)
    {
        "\nOllama currently allocated 4096 tokens. This can be too small for larger explanations. Increase Ollama's context allocation, reload the model, then run this test again. `context_window` only caps git-explain's budget; it does not reconfigure Ollama.\n"
    } else {
        ""
    };
    format!(
        "Profile: {name}\n\nProvider: {}\nPreset: {}\nEndpoint: reachable\nAuthentication: {}\nModel: {}\nModel availability: {availability}\n{context}{remediation}\nProfile is ready.\n",
        display_provider_name(&model.provider),
        model.preset.as_deref().map(display_preset_name).unwrap_or("<none>"),
        if model.api_key_env.is_some() { "accepted" } else { "not required" },
        model.model,
    )
}

fn user_facing_error(error: &anyhow::Error) -> String {
    let details = format!("{error:#}");
    if let Some(not_found) = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<ProfileNotFound>())
    {
        return profile_not_found_message(&not_found.requested, &not_found.available, "show");
    }
    if details.contains("configuration error:") {
        return format!(
            "Could not load configuration.\n\n{details}\n\nRun:\ngit explain config path"
        );
    }
    if details.contains("not inside a Git repository") {
        return "This command must be run inside a Git repository. Run it from a repository containing the changes or commit to explain.".into();
    }
    if details.contains("Unable to resolve Git revision") {
        return format!(
            "The requested Git revision could not be resolved. Check the revision name and try again.\nDetails: {details}"
        );
    }
    if details.contains("daemon") || details.contains("bind loopback") {
        return format!(
            "The git-explain daemon could not complete the request. Try `git explain daemon status` or use `git explain --direct`.\nDetails: {details}"
        );
    }
    if details.contains("context budget exceeded")
        || details.contains("context length")
        || details.contains("maximum context")
    {
        return format!(
            "The configured model context is too small for this explanation.\n\n{details}\n\nFor Ollama, run `git explain profile test <name>` to inspect the runtime allocation, then increase Ollama's context and reload the model."
        );
    }
    if details.contains("connect")
        || details.contains("timed out")
        || details.contains("model request")
    {
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
        || details.contains("configuration error:")
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

#[cfg(test)]
mod profile_error_tests {
    use super::*;

    #[test]
    fn unknown_explicit_profile_suggests_the_single_prefix_match() {
        let message = profile_not_found_message("q", &["qwen35b".into()], "test");
        assert_eq!(message, "Profile \"q\" does not exist.\n\nAvailable profiles:\n  qwen35b\n\nDid you mean:\n  qwen35b\n\nTry:\n  git explain profile test qwen35b\n\nOr list profiles:\n  git explain profile list");
    }

    #[test]
    fn ambiguous_prefix_does_not_suggest_a_profile() {
        let message = profile_not_found_message(
            "qwen",
            &["qwen35b".into(), "qwen72b".into(), "qwen-cloud".into()],
            "show",
        );
        assert!(!message.contains("Did you mean"));
        assert!(message.contains("git explain profile list"));
    }
}
