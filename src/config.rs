use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedConfig {
    pub profile: String,
    pub profile_selection_source: ProfileSelectionSource,
    pub available_profiles: Vec<String>,
    pub model: ResolvedProfile,
    pub reader: ReaderConfig,
    pub explanation: ExplanationConfig,
    pub server: ServerConfig,
    pub git: GitConfig,
    pub cache: CacheConfig,
    pub paths: ConfigPaths,
}

/// The application-owned part of configuration. Unlike `ResolvedConfig`, this
/// can be inspected before a model profile has been created.
#[derive(Clone, Debug, PartialEq)]
pub struct ApplicationConfig {
    pub profile: Option<String>,
    pub profile_selection_source: Option<ProfileSelectionSource>,
    pub available_profiles: Vec<String>,
    pub reader: ReaderConfig,
    pub explanation: ExplanationConfig,
    pub server: ServerConfig,
    pub git: GitConfig,
    pub cache: CacheConfig,
    pub paths: ConfigPaths,
    pub reader_source: ConfigValueSource,
    pub explanation_source: ConfigValueSource,
    pub cache_source: ConfigValueSource,
    pub server_source: ConfigValueSource,
    pub git_source: ConfigValueSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigValueSource {
    Default,
    User,
    Repository,
}
impl ConfigValueSource {
    fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::User => "user configuration",
            Self::Repository => "repository configuration",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileSelectionSource {
    CommandLine,
    Environment,
    Repository,
    User,
}

impl ProfileSelectionSource {
    fn label(&self) -> &'static str {
        match self {
            Self::CommandLine => "command line",
            Self::Environment => "GIT_EXPLAIN_PROFILE",
            Self::Repository => "repository configuration",
            Self::User => "user configuration",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
/// A complete model configuration after profile selection, preset resolution,
/// validation, and credential lookup. This is the only profile shape that the
/// model transport is allowed to consume.
pub struct ResolvedProfile {
    pub provider: String,
    pub preset: Option<String>,
    pub base_url: String,
    pub model: String,
    pub api_key_env: Option<String>,
    pub api_key: Option<String>,
    pub normal: GenerationConfig,
    pub deep: GenerationConfig,
}

/// A profile being constructed before it is written to user configuration.
#[derive(Clone, Debug)]
pub struct ProfileDraft {
    pub name: String,
    pub provider: Option<String>,
    pub preset: Option<String>,
    pub base_url: Option<String>,
    pub model_port: Option<u16>,
    pub model: String,
    pub api_key_env: Option<String>,
}

/// The three states needed when changing a persisted optional value.
///
/// This avoids treating `None` ambiguously as both “leave untouched” and
/// “remove the stored value”. CLI flags are adapted to this type at the domain
/// boundary; interactive editing uses the same `ProfileUpdate` operation.
#[derive(Clone, Debug, PartialEq)]
pub enum Update<T> {
    Unchanged,
    Set(T),
    Clear,
}

impl<T> Update<T> {
    fn from_flag(value: Option<T>, clear: bool, set_name: &str, clear_name: &str) -> Result<Self> {
        match (value, clear) {
            (Some(_), true) => anyhow::bail!("cannot use {set_name} and {clear_name} together"),
            (Some(value), false) => Ok(Self::Set(value)),
            (None, true) => Ok(Self::Clear),
            (None, false) => Ok(Self::Unchanged),
        }
    }

    fn has_change(&self) -> bool {
        !matches!(self, Self::Unchanged)
    }
}

/// A typed profile lookup failure. Presentation code can provide command
/// specific recovery guidance without re-parsing an error string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileNotFound {
    pub requested: String,
    pub available: Vec<String>,
}

impl fmt::Display for ProfileNotFound {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown model profile '{}'", self.requested)
    }
}

impl Error for ProfileNotFound {}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReaderConfig {
    pub experience: String,
    pub known_languages: Vec<String>,
    pub learning_languages: Vec<String>,
    pub known_frameworks: Vec<String>,
    pub learning_frameworks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExplanationConfig {
    pub default_depth: String,
    pub max_annotations: u32,
    pub max_annotation_words: u32,
    pub explain_language_concepts: bool,
    pub explain_framework_concepts: bool,
    pub infer_intent: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub open_browser: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GitConfig {
    pub diff_target: String,
    pub include_staged: bool,
    pub include_untracked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CacheConfig {
    pub enabled: bool,
}

/// A field update shared by command-line and interactive configuration editing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListUpdate {
    pub add: Vec<String>,
    pub remove: Vec<String>,
    pub clear: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReaderUpdate {
    pub experience: Option<String>,
    pub known_languages: ListUpdate,
    pub learning_languages: ListUpdate,
    pub known_frameworks: ListUpdate,
    pub learning_frameworks: ListUpdate,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExplanationUpdate {
    pub default_depth: Option<String>,
    pub max_annotations: Option<u32>,
    pub max_annotation_words: Option<u32>,
    pub explain_language_concepts: Option<bool>,
    pub explain_framework_concepts: Option<bool>,
    pub infer_intent: Option<bool>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerUpdate {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub open_browser: Option<bool>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GitUpdate {
    pub diff_target: Option<String>,
    pub include_staged: Option<bool>,
    pub include_untracked: Option<bool>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CacheUpdate {
    pub enabled: Option<bool>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelSelectionUpdate {
    pub profile: Option<String>,
    pub clear_profile: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigUpdate {
    pub reader: ReaderUpdate,
    pub explanation: ExplanationUpdate,
    pub cache: CacheUpdate,
    pub server: ServerUpdate,
    pub git: GitUpdate,
    pub model: ModelSelectionUpdate,
}

impl ConfigUpdate {
    pub fn has_changes(&self) -> bool {
        let list = |v: &ListUpdate| v.clear || !v.add.is_empty() || !v.remove.is_empty();
        self.reader.experience.is_some()
            || list(&self.reader.known_languages)
            || list(&self.reader.learning_languages)
            || list(&self.reader.known_frameworks)
            || list(&self.reader.learning_frameworks)
            || self.explanation.default_depth.is_some()
            || self.explanation.max_annotations.is_some()
            || self.explanation.max_annotation_words.is_some()
            || self.explanation.explain_language_concepts.is_some()
            || self.explanation.explain_framework_concepts.is_some()
            || self.explanation.infer_intent.is_some()
            || self.cache.enabled.is_some()
            || self.server.host.is_some()
            || self.server.port.is_some()
            || self.server.open_browser.is_some()
            || self.git.diff_target.is_some()
            || self.git.include_staged.is_some()
            || self.git.include_untracked.is_some()
            || self.model.profile.is_some()
            || self.model.clear_profile
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigPaths {
    pub user: PathBuf,
    pub repository: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvironmentOverrides {
    pub profile: Option<String>,
}

impl EnvironmentOverrides {
    pub fn from_process() -> Self {
        Self {
            profile: std::env::var("GIT_EXPLAIN_PROFILE").ok(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConfigLoader {
    pub paths: ConfigPaths,
}

impl ConfigLoader {
    pub fn for_repository(repository_root: Option<&Path>) -> Result<Self> {
        Ok(Self {
            paths: ConfigPaths {
                user: default_user_config_path()?,
                repository: repository_root.map(repository_config_path),
            },
        })
    }

    pub fn for_context(context: Option<&crate::git::RepositoryContext>) -> Result<Self> {
        Ok(Self {
            paths: ConfigPaths {
                user: default_user_config_path()?,
                repository: context.map(|context| repository_config_path(&context.git_dir)),
            },
        })
    }

    #[allow(dead_code)]
    pub fn with_paths(user: PathBuf, repository: Option<PathBuf>) -> Self {
        Self {
            paths: ConfigPaths { user, repository },
        }
    }

    pub fn resolve(&self, cli_profile: Option<&str>) -> Result<ResolvedConfig> {
        self.resolve_with_environment(cli_profile, &EnvironmentOverrides::from_process())
    }

    pub fn resolve_with_environment(
        &self,
        cli_profile: Option<&str>,
        environment: &EnvironmentOverrides,
    ) -> Result<ResolvedConfig> {
        let mut file = PartialConfig::default();
        if self.paths.user.exists() {
            file.merge(load_file(&self.paths.user)?);
        }
        let mut repository_profile = None;
        if let Some(repository) = &self.paths.repository {
            if repository.exists() {
                let repository_file = load_repository_file(repository)?;
                repository_profile = repository_file
                    .model
                    .as_ref()
                    .and_then(|model| model.profile.clone());
                file.merge_repository(repository_file);
            }
        }
        resolve(
            file,
            &self.paths,
            cli_profile,
            environment,
            repository_profile,
        )
    }

    pub fn application_config_with_environment(
        &self,
        cli_profile: Option<&str>,
        environment: &EnvironmentOverrides,
    ) -> Result<ApplicationConfig> {
        let mut file = PartialConfig::default();
        let mut reader_source = ConfigValueSource::Default;
        let mut explanation_source = ConfigValueSource::Default;
        let mut cache_source = ConfigValueSource::Default;
        let mut server_source = ConfigValueSource::Default;
        let mut git_source = ConfigValueSource::Default;
        if self.paths.user.exists() {
            let user = load_file(&self.paths.user)?;
            if user.reader.is_some() {
                reader_source = ConfigValueSource::User;
            }
            if user.explanation.is_some() {
                explanation_source = ConfigValueSource::User;
            }
            if user.cache.is_some() {
                cache_source = ConfigValueSource::User;
            }
            if user.server.is_some() {
                server_source = ConfigValueSource::User;
            }
            if user.git.is_some() {
                git_source = ConfigValueSource::User;
            }
            file.merge(user);
        }
        let mut repository_profile = None;
        if let Some(repository) = &self.paths.repository {
            if repository.exists() {
                let repository_file = load_repository_file(repository)?;
                if repository_file.reader.is_some() {
                    reader_source = ConfigValueSource::Repository;
                }
                if repository_file.explanation.is_some() {
                    explanation_source = ConfigValueSource::Repository;
                }
                if repository_file.cache.is_some() {
                    cache_source = ConfigValueSource::Repository;
                }
                if repository_file.server.is_some() {
                    server_source = ConfigValueSource::Repository;
                }
                if repository_file.git.is_some() {
                    git_source = ConfigValueSource::Repository;
                }
                repository_profile = repository_file
                    .model
                    .as_ref()
                    .and_then(|model| model.profile.clone());
                file.merge_repository(repository_file);
            }
        }
        let selection = select_profile(&file, cli_profile, environment, repository_profile);
        if let Some(profile) = &selection.name {
            require_profile(&file.profiles, profile)?;
        }
        Ok(ApplicationConfig {
            profile: selection.name,
            profile_selection_source: selection.source,
            available_profiles: file.profiles.keys().cloned().collect(),
            reader: reader_config(file.reader),
            explanation: explanation_config(file.explanation),
            server: server_config(file.server),
            git: git_config(file.git),
            cache: CacheConfig {
                enabled: file.cache.and_then(|cache| cache.enabled).unwrap_or(true),
            },
            paths: self.paths.clone(),
            reader_source,
            explanation_source,
            cache_source,
            server_source,
            git_source,
        })
    }

    pub fn application_config(&self, cli_profile: Option<&str>) -> Result<ApplicationConfig> {
        self.application_config_with_environment(cli_profile, &EnvironmentOverrides::from_process())
    }
}

pub fn default_user_config_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("GIT_EXPLAIN_USER_CONFIG") {
        let path = PathBuf::from(path);
        if path.as_os_str().is_empty() {
            anyhow::bail!("GIT_EXPLAIN_USER_CONFIG must not be empty");
        }
        return Ok(path);
    }
    let directories = ProjectDirs::from("", "", "git-explain")
        .context("determine the user configuration directory")?;
    Ok(directories.config_dir().join("config.toml"))
}

pub fn repository_config_path(git_dir: &Path) -> PathBuf {
    git_dir.join("git-explain.toml")
}

pub fn init_user_config(path: &Path, force: bool) -> Result<bool> {
    if path.exists() && !force {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    atomic_write(path, example_config())?;
    Ok(true)
}

pub fn init_repository_config(path: &Path, force: bool) -> Result<bool> {
    if path.exists() && !force {
        return Ok(false);
    }
    atomic_write(path, repository_example_config())?;
    Ok(true)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfilePreset {
    pub id: &'static str,
    pub display_name: &'static str,
    pub provider: &'static str,
    pub default_base_url: Option<&'static str>,
}

const PROFILE_PRESETS: &[ProfilePreset] = &[
    ProfilePreset {
        id: "llama_cpp",
        display_name: "llama.cpp",
        provider: "openai_compatible",
        default_base_url: Some("http://127.0.0.1:8083/v1"),
    },
    ProfilePreset {
        id: "ollama",
        display_name: "Ollama",
        provider: "openai_compatible",
        default_base_url: Some("http://127.0.0.1:11434/v1"),
    },
];

pub fn profile_presets() -> &'static [ProfilePreset] {
    PROFILE_PRESETS
}

pub fn profile_preset(value: &str) -> Option<&'static ProfilePreset> {
    let value = value.replace('-', "_");
    PROFILE_PRESETS.iter().find(|preset| preset.id == value)
}

pub fn display_provider(value: &str) -> &str {
    if value == "openai_compatible" {
        "OpenAI-compatible"
    } else {
        value
    }
}

pub fn display_preset(value: &str) -> &str {
    profile_preset(value).map_or(value, |preset| preset.display_name)
}

#[derive(Clone, Debug, Default)]
pub struct ProfileUpdate {
    pub provider: Option<String>,
    pub preset: Option<String>,
    pub base_url: Option<String>,
    pub model_port: Option<u16>,
    pub model: Option<String>,
    pub api_key_env: Option<String>,
    pub clear_preset: bool,
    pub clear_api_key_env: bool,
    pub normal_reasoning: Option<bool>,
    pub normal_max_tokens: Option<u32>,
    pub normal_temperature: Option<f32>,
    pub deep_reasoning: Option<bool>,
    pub deep_max_tokens: Option<u32>,
    pub deep_temperature: Option<f32>,
    pub clear_normal_reasoning: bool,
    pub clear_normal_max_tokens: bool,
    pub clear_normal_temperature: bool,
    pub clear_deep_reasoning: bool,
    pub clear_deep_max_tokens: bool,
    pub clear_deep_temperature: bool,
}

#[allow(dead_code)]
pub fn add_profile(path: &Path, profile: ProfileDraft) -> Result<()> {
    add_profile_with_update(path, profile, ProfileUpdate::default())
}

pub fn add_profile_with_update(
    path: &Path,
    profile: ProfileDraft,
    update: ProfileUpdate,
) -> Result<()> {
    validate_profile_name(&profile.name)?;
    let mut root = user_toml(path)?;
    let profiles = table_mut(&mut root, "profiles");
    if profiles.contains_key(&profile.name) {
        anyhow::bail!(
            "profile '{}' already exists; use `git explain profile edit {}` to change it",
            profile.name,
            profile.name
        );
    }
    let preset = resolve_preset(profile.preset.as_deref())?;
    let provider = profile.provider.as_deref().unwrap_or("openai_compatible");
    validate_preset_provider(preset, provider)?;
    let base_url = resolve_endpoint(preset, profile.base_url, profile.model_port, None)?
        .ok_or_else(|| anyhow::anyhow!("Could not create profile \"{}\".\n\nA base URL is required when no preset provides one.\n\nSpecify --base-url <URL>, or choose --preset llama-cpp or --preset ollama.", profile.name))?;
    let mut value = toml::map::Map::new();
    value.insert("provider".into(), toml::Value::String(provider.into()));
    if let Some(preset) = preset {
        value.insert("preset".into(), toml::Value::String(preset.id.into()));
    }
    value.insert("base_url".into(), toml::Value::String(base_url));
    value.insert("model".into(), toml::Value::String(profile.model));
    if let Some(name) = profile.api_key_env {
        value.insert("api_key_env".into(), toml::Value::String(name));
    }
    generation_update(
        &mut value,
        "normal",
        GenerationUpdate::from_normal(&update)?,
    )?;
    generation_update(&mut value, "deep", GenerationUpdate::from_deep(&update)?)?;
    let name = profile.name;
    profiles.insert(name.clone(), toml::Value::Table(value));
    validate_profile_document(&root, path, &name).context("the profile was not added")?;
    write_toml(path, root)
}

pub fn edit_profile(path: &Path, name: &str, update: ProfileUpdate) -> Result<()> {
    let mut root = user_toml(path)?;
    apply_profile_update(&mut root, name, &update)?;
    validate_profile_document(&root, path, name).context("the existing profile was not changed")?;
    write_toml(path, root)
}

pub fn preview_profile(path: &Path, name: &str, update: &ProfileUpdate) -> Result<ResolvedProfile> {
    let mut root = user_toml(path)?;
    if update.has_changes() {
        apply_profile_update(&mut root, name, update)?;
    } else {
        validate_profile_name(name)?;
        if !table_mut(&mut root, "profiles").contains_key(name) {
            anyhow::bail!("unknown model profile '{name}'");
        }
    }
    let parsed: PartialConfig = root.try_into().context("validate profile configuration")?;
    resolve(
        parsed,
        &ConfigPaths {
            user: path.into(),
            repository: None,
        },
        Some(name),
        &EnvironmentOverrides::default(),
        None,
    )
    .map(|resolved| resolved.model)
}

fn apply_profile_update(root: &mut toml::Value, name: &str, update: &ProfileUpdate) -> Result<()> {
    validate_profile_name(name)?;
    let profile = table_mut(root, "profiles")
        .get_mut(name)
        .and_then(toml::Value::as_table_mut)
        .context(format!("unknown model profile '{name}'"))?;
    if !update.has_changes() {
        anyhow::bail!("No profile changes were specified.\n\nRun:\ngit explain profile edit -h");
    }
    ensure_not_both(
        "--preset",
        update.preset.is_some(),
        "--clear-preset",
        update.clear_preset,
    )?;
    ensure_not_both(
        "--api-key-env",
        update.api_key_env.is_some(),
        "--clear-api-key-env",
        update.clear_api_key_env,
    )?;
    let normal_update = GenerationUpdate::from_normal(update)?;
    let deep_update = GenerationUpdate::from_deep(update)?;
    if update.model_port.is_some() && update.base_url.is_some() {
        anyhow::bail!("`--model-port` cannot be used together with `--base-url`.\n\nUse `--model-port` to change only the port of a preset endpoint.\n\nUse `--base-url` to provide a complete custom endpoint.");
    }
    set(profile, "provider", update.provider.clone());
    set(profile, "preset", update.preset.clone());
    set(profile, "base_url", update.base_url.clone());
    set(profile, "model", update.model.clone());
    set(profile, "api_key_env", update.api_key_env.clone());
    clear(profile, "preset", update.clear_preset);
    clear(profile, "api_key_env", update.clear_api_key_env);
    generation_update(profile, "normal", normal_update)?;
    generation_update(profile, "deep", deep_update)?;
    let preset = resolve_preset(profile.get("preset").and_then(toml::Value::as_str))?;
    let provider = profile
        .get("provider")
        .and_then(toml::Value::as_str)
        .unwrap_or("openai_compatible");
    validate_preset_provider(preset, provider)?;
    let existing_base_url = profile.get("base_url").and_then(toml::Value::as_str);
    let resolved_endpoint = resolve_endpoint(preset, None, update.model_port, existing_base_url)?;
    if let Some(url) = resolved_endpoint {
        profile.insert("base_url".into(), toml::Value::String(url));
    }
    Ok(())
}

impl ProfileUpdate {
    pub fn has_changes(&self) -> bool {
        self.provider.is_some()
            || self.preset.is_some()
            || self.base_url.is_some()
            || self.model_port.is_some()
            || self.model.is_some()
            || self.api_key_env.is_some()
            || self.clear_preset
            || self.clear_api_key_env
            || self.normal_reasoning.is_some()
            || self.normal_max_tokens.is_some()
            || self.normal_temperature.is_some()
            || self.deep_reasoning.is_some()
            || self.deep_max_tokens.is_some()
            || self.deep_temperature.is_some()
            || self.clear_normal_reasoning
            || self.clear_normal_max_tokens
            || self.clear_normal_temperature
            || self.clear_deep_reasoning
            || self.clear_deep_max_tokens
            || self.clear_deep_temperature
    }
}

fn resolve_preset(value: Option<&str>) -> Result<Option<&'static ProfilePreset>> {
    value
        .map(|value| {
            profile_preset(value).ok_or_else(|| {
                anyhow::anyhow!(
                    "unsupported preset '{value}'; supported presets: llama-cpp, ollama"
                )
            })
        })
        .transpose()
}

fn validate_preset_provider(preset: Option<&ProfilePreset>, provider: &str) -> Result<()> {
    if let Some(preset) = preset {
        if provider != preset.provider {
            anyhow::bail!("Preset \"{}\" requires provider \"{}\".\n\nRemove `--provider` or use:\n\n--provider openai-compatible", preset.id.replace('_', "-"), preset.provider.replace('_', "-"));
        }
    }
    if provider != "openai_compatible" {
        anyhow::bail!("unsupported provider '{provider}'; use openai-compatible");
    }
    Ok(())
}

fn resolve_endpoint(
    preset: Option<&ProfilePreset>,
    explicit_base_url: Option<String>,
    model_port: Option<u16>,
    existing_endpoint: Option<&str>,
) -> Result<Option<String>> {
    if model_port == Some(0) {
        anyhow::bail!("model port must be between 1 and 65535");
    }
    if explicit_base_url.is_some() && model_port.is_some() {
        anyhow::bail!("`--model-port` cannot be used together with `--base-url`.");
    }
    if let Some(port) = model_port {
        let endpoint = existing_endpoint
            .or_else(|| preset.and_then(|p| p.default_base_url))
            .ok_or_else(|| anyhow::anyhow!("`--model-port` requires a profile preset.\n\nFor a custom endpoint, use --base-url <URL>."))?;
        let mut url = reqwest::Url::parse(endpoint).context("parse model endpoint URL")?;
        url.set_port(Some(port))
            .map_err(|_| anyhow::anyhow!("model endpoint URL does not support a port"))?;
        return Ok(Some(url.to_string()));
    }
    Ok(explicit_base_url
        .or_else(|| existing_endpoint.map(str::to_owned))
        .or_else(|| preset.and_then(|p| p.default_base_url.map(str::to_owned))))
}

fn set(table: &mut toml::map::Map<String, toml::Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        table.insert(key.into(), toml::Value::String(value));
    }
}
fn clear(table: &mut toml::map::Map<String, toml::Value>, key: &str, value: bool) {
    if value {
        table.remove(key);
    }
}
fn ensure_not_both(set_name: &str, is_set: bool, clear_name: &str, is_clear: bool) -> Result<()> {
    if is_set && is_clear {
        anyhow::bail!("cannot use {set_name} and {clear_name} together");
    }
    Ok(())
}
pub struct GenerationUpdate {
    reasoning: Update<bool>,
    max_tokens: Update<u32>,
    temperature: Update<f32>,
}
impl GenerationUpdate {
    fn from_normal(update: &ProfileUpdate) -> Result<Self> {
        Ok(Self {
            reasoning: Update::from_flag(
                update.normal_reasoning,
                update.clear_normal_reasoning,
                "--normal-reasoning",
                "--clear-normal-reasoning",
            )?,
            max_tokens: Update::from_flag(
                update.normal_max_tokens,
                update.clear_normal_max_tokens,
                "--normal-max-tokens",
                "--clear-normal-max-tokens",
            )?,
            temperature: Update::from_flag(
                update.normal_temperature,
                update.clear_normal_temperature,
                "--normal-temperature",
                "--clear-normal-temperature",
            )?,
        })
    }
    fn from_deep(update: &ProfileUpdate) -> Result<Self> {
        Ok(Self {
            reasoning: Update::from_flag(
                update.deep_reasoning,
                update.clear_deep_reasoning,
                "--deep-reasoning",
                "--clear-deep-reasoning",
            )?,
            max_tokens: Update::from_flag(
                update.deep_max_tokens,
                update.clear_deep_max_tokens,
                "--deep-max-tokens",
                "--clear-deep-max-tokens",
            )?,
            temperature: Update::from_flag(
                update.deep_temperature,
                update.clear_deep_temperature,
                "--deep-temperature",
                "--clear-deep-temperature",
            )?,
        })
    }
}
fn generation_update(
    profile: &mut toml::map::Map<String, toml::Value>,
    name: &str,
    update: GenerationUpdate,
) -> Result<()> {
    let GenerationUpdate {
        reasoning,
        max_tokens,
        temperature,
    } = update;
    if !reasoning.has_change() && !max_tokens.has_change() && !temperature.has_change() {
        return Ok(());
    }
    let generation = profile
        .entry(name)
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .expect("generation settings are a table");
    apply_generation_value(generation, "reasoning", reasoning, toml::Value::Boolean);
    apply_generation_value(generation, "max_tokens", max_tokens, |value| {
        toml::Value::Integer(value.into())
    });
    apply_generation_value(generation, "temperature", temperature, |value| {
        toml::Value::Float(value.into())
    });
    Ok(())
}

fn apply_generation_value<T>(
    generation: &mut toml::map::Map<String, toml::Value>,
    name: &str,
    update: Update<T>,
    value: impl FnOnce(T) -> toml::Value,
) {
    match update {
        Update::Unchanged => {}
        Update::Set(update) => {
            generation.insert(name.into(), value(update));
        }
        Update::Clear => {
            generation.remove(name);
        }
    }
}
fn validate_profile_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        anyhow::bail!("invalid profile name '{name}'; use letters, digits, '.', '_', or '-'");
    }
    Ok(())
}

fn validate_profile_document(root: &toml::Value, path: &Path, name: &str) -> Result<()> {
    let parsed: PartialConfig = root
        .clone()
        .try_into()
        .context("validate profile configuration")?;
    resolve(
        parsed,
        &ConfigPaths {
            user: path.into(),
            repository: None,
        },
        Some(name),
        &EnvironmentOverrides::default(),
        None,
    )
    .map(|_| ())
}

pub fn remove_profile(path: &Path, name: &str) -> Result<()> {
    let mut root = user_toml(path)?;
    let profiles = table_mut(&mut root, "profiles");
    if profiles.remove(name).is_none() {
        anyhow::bail!("unknown model profile '{name}'");
    }
    write_toml(path, root)
}

pub fn profile_names(path: &Path) -> Result<Vec<String>> {
    let root = user_toml(path)?;
    Ok(root
        .get("profiles")
        .and_then(toml::Value::as_table)
        .map(|profiles| profiles.keys().cloned().collect())
        .unwrap_or_default())
}

/// Validate a selected profile against the trusted user profile registry.
/// Selection callers use this instead of formatting their own lookup errors.
fn require_profile(
    profiles: &BTreeMap<String, PartialProfileConfig>,
    requested: &str,
) -> Result<()> {
    if profiles.contains_key(requested) {
        return Ok(());
    }
    Err(anyhow::Error::new(ProfileNotFound {
        requested: requested.to_owned(),
        available: profiles.keys().cloned().collect(),
    }))
}

pub fn use_profile(path: &Path, name: &str) -> Result<()> {
    let root = user_toml(path)?;
    if !root
        .get("profiles")
        .and_then(toml::Value::as_table)
        .is_some_and(|profiles| profiles.contains_key(name))
    {
        anyhow::bail!("unknown model profile '{name}'");
    }
    let mut root = root;
    table_mut(&mut root, "model").insert("profile".into(), toml::Value::String(name.into()));
    write_toml(path, root)
}

pub fn use_repository_profile(path: &Path, name: &str) -> Result<()> {
    let mut root = repository_toml(path)?;
    table_mut(&mut root, "model").insert("profile".into(), toml::Value::String(name.into()));
    write_toml(path, root)
}

/// Apply an application-configuration update and persist it atomically.  The same
/// TOML mutation and validation path is deliberately used by flags and the editor.
pub fn edit_config(
    path: &Path,
    repository: bool,
    update: &ConfigUpdate,
    profiles: &[String],
) -> Result<bool> {
    if !update.has_changes() {
        return Ok(false);
    }
    if update.model.profile.is_some() && update.model.clear_profile {
        anyhow::bail!("cannot set and clear --profile together");
    }
    if let Some(profile) = &update.model.profile {
        if !profiles.iter().any(|candidate| candidate == profile) {
            anyhow::bail!("unknown model profile '{profile}'");
        }
    }
    let mut root = if repository {
        repository_toml(path)?
    } else {
        user_toml(path)?
    };
    let before = root.clone();
    apply_config_update(&mut root, update)?;
    // Strictly parse the final document before replacing the original file.
    if repository {
        let _: RepositoryConfig = root
            .clone()
            .try_into()
            .context("validate repository configuration")?;
    } else {
        let _: PartialConfig = root
            .clone()
            .try_into()
            .context("validate user configuration")?;
    }
    validate_application_document(&root)?;
    if root == before {
        return Ok(false);
    }
    write_toml(path, root)?;
    Ok(true)
}

fn apply_config_update(root: &mut toml::Value, update: &ConfigUpdate) -> Result<()> {
    let reader = table_mut(root, "reader");
    set(reader, "experience", update.reader.experience.clone());
    update_list(reader, "known_languages", &update.reader.known_languages)?;
    update_list(
        reader,
        "learning_languages",
        &update.reader.learning_languages,
    )?;
    update_list(reader, "known_frameworks", &update.reader.known_frameworks)?;
    update_list(
        reader,
        "learning_frameworks",
        &update.reader.learning_frameworks,
    )?;
    let explanation = table_mut(root, "explanation");
    set(
        explanation,
        "default_depth",
        update.explanation.default_depth.clone(),
    );
    set_u32(
        explanation,
        "max_annotations",
        update.explanation.max_annotations,
    );
    set_u32(
        explanation,
        "max_annotation_words",
        update.explanation.max_annotation_words,
    );
    set_bool(
        explanation,
        "explain_language_concepts",
        update.explanation.explain_language_concepts,
    );
    set_bool(
        explanation,
        "explain_framework_concepts",
        update.explanation.explain_framework_concepts,
    );
    set_bool(explanation, "infer_intent", update.explanation.infer_intent);
    let cache = table_mut(root, "cache");
    set_bool(cache, "enabled", update.cache.enabled);
    let server = table_mut(root, "server");
    set(server, "host", update.server.host.clone());
    set_u16(server, "port", update.server.port);
    set_bool(server, "open_browser", update.server.open_browser);
    let git = table_mut(root, "git");
    set(git, "diff_target", update.git.diff_target.clone());
    set_bool(git, "include_staged", update.git.include_staged);
    set_bool(git, "include_untracked", update.git.include_untracked);
    let model = table_mut(root, "model");
    set(model, "profile", update.model.profile.clone());
    clear(model, "profile", update.model.clear_profile);
    Ok(())
}
fn set_bool(t: &mut toml::map::Map<String, toml::Value>, k: &str, v: Option<bool>) {
    if let Some(v) = v {
        t.insert(k.into(), toml::Value::Boolean(v));
    }
}
fn set_u16(t: &mut toml::map::Map<String, toml::Value>, k: &str, v: Option<u16>) {
    if let Some(v) = v {
        t.insert(k.into(), toml::Value::Integer(i64::from(v)));
    }
}
fn set_u32(t: &mut toml::map::Map<String, toml::Value>, k: &str, v: Option<u32>) {
    if let Some(v) = v {
        t.insert(k.into(), toml::Value::Integer(i64::from(v)));
    }
}
fn update_list(
    t: &mut toml::map::Map<String, toml::Value>,
    key: &str,
    update: &ListUpdate,
) -> Result<()> {
    if !update.clear && update.add.is_empty() && update.remove.is_empty() {
        return Ok(());
    }
    let mut values = if update.clear {
        Vec::new()
    } else {
        t.get(key)
            .and_then(toml::Value::as_array)
            .map(|v| {
                v.iter()
                    .filter_map(toml::Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    for value in &update.remove {
        values.retain(|existing| existing != value);
    }
    for value in &update.add {
        if value.trim().is_empty() {
            anyhow::bail!("{key} entries must not be empty");
        }
        if !values
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(value))
        {
            values.push(value.clone());
        }
    }
    t.insert(
        key.into(),
        toml::Value::Array(values.into_iter().map(toml::Value::String).collect()),
    );
    Ok(())
}
fn validate_application_document(root: &toml::Value) -> Result<()> {
    let partial: PartialConfig = root.clone().try_into().context("validate configuration")?;
    let reader = reader_config(partial.reader);
    let explanation = explanation_config(partial.explanation);
    let server = server_config(partial.server);
    let git = git_config(partial.git);
    if reader.experience.trim().is_empty() {
        anyhow::bail!("reader experience must not be empty");
    }
    for (name, values) in [
        ("known_languages", &reader.known_languages),
        ("learning_languages", &reader.learning_languages),
        ("known_frameworks", &reader.known_frameworks),
        ("learning_frameworks", &reader.learning_frameworks),
    ] {
        if values.iter().any(|value| value.trim().is_empty()) {
            anyhow::bail!("reader {name} entries must not be empty");
        }
    }
    if !matches!(explanation.default_depth.as_str(), "normal" | "deep") {
        anyhow::bail!("explanation default_depth must be 'normal' or 'deep'");
    }
    if explanation.max_annotations == 0 || explanation.max_annotation_words == 0 {
        anyhow::bail!("explanation limits must be greater than zero");
    }
    if server.port == 0 {
        anyhow::bail!("server port must be between 1 and 65535");
    }
    if !matches!(server.host.as_str(), "127.0.0.1" | "localhost" | "::1") {
        anyhow::bail!("server host must be a loopback address (127.0.0.1, localhost, or ::1)");
    }
    if git.diff_target.trim().is_empty() {
        anyhow::bail!("git diff_target must not be empty");
    }
    Ok(())
}

fn user_toml(path: &Path) -> Result<toml::Value> {
    if !path.exists() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    toml::from_str(
        &fs::read_to_string(path)
            .with_context(|| format!("read configuration {}", path.display()))?,
    )
    .with_context(|| format!("parse configuration {}", path.display()))
}
fn table_mut<'a>(
    root: &'a mut toml::Value,
    key: &str,
) -> &'a mut toml::map::Map<String, toml::Value> {
    root.as_table_mut()
        .expect("configuration root is a table")
        .entry(key)
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .expect("configuration section is a table")
}
fn repository_toml(path: &Path) -> Result<toml::Value> {
    if !path.exists() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    let text = fs::read_to_string(path).with_context(|| {
        format!(
            "configuration error: read repository configuration {}",
            path.display()
        )
    })?;
    load_repository_file(path)?;
    toml::from_str(&text).with_context(|| {
        format!(
            "configuration error: parse repository configuration {}",
            path.display()
        )
    })
}

fn write_toml(path: &Path, root: toml::Value) -> Result<()> {
    let values = toml::to_string_pretty(&root)?;
    let existing = fs::read_to_string(path).unwrap_or_default();
    let template = if existing.starts_with("# git-explain user configuration") {
        Some(example_config())
    } else if existing.starts_with("# git-explain repository configuration") {
        Some(repository_example_config())
    } else {
        None
    };
    let content = template.map_or(values.clone(), |template| {
        format!("{template}\n# Active values written by git-explain\n\n{values}")
    });
    atomic_write(path, &content)
}

fn atomic_write(path: &Path, content: &str) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::File::create(&temporary)
        .with_context(|| format!("create temporary configuration {}", temporary.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("write temporary configuration {}", temporary.display()))?;
    file.sync_all()
        .with_context(|| format!("flush temporary configuration {}", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("replace configuration {}", path.display()))
}

pub fn example_config() -> &'static str {
    r#"# git-explain user configuration
#
# This template documents every TOML-backed user setting. Values are commented
# so the application/provider defaults remain unpinned until you opt in.

# ----------------------------------------------------------------------
# Model selection
# ----------------------------------------------------------------------
# [model]
# profile = "local" # Default: no profile selected

# ----------------------------------------------------------------------
# Example local model profile (all lines are examples, not an active profile)
# ----------------------------------------------------------------------
# [profiles.local]
# provider = "openai_compatible" # The only supported provider value
# preset = "llama_cpp" # Presets: "llama_cpp", "ollama"
# base_url = "http://127.0.0.1:8083/v1" # llama.cpp preset endpoint
# model = "your-model"
# Store only an environment variable name here, never a secret value.
# api_key_env = "LOCAL_MODEL_API_KEY"
#
# [profiles.local.normal]
# Omit reasoning to let the provider/model decide.
# reasoning = false
# Example override; omitted values are omitted from requests.
# max_tokens = 500
# temperature = 0.2
#
# [profiles.local.deep]
# Omit reasoning to let the provider/model decide.
# reasoning = true
# Example override; omitted values are omitted from requests.
# max_tokens = 2500
# temperature = 0.3

# ----------------------------------------------------------------------
# Reader context
# ----------------------------------------------------------------------
# [reader]
# git-explain defaults experience to "experienced" and lists to [].
# experience = "experienced"
# known_languages = []
# learning_languages = []
# known_frameworks = []
# learning_frameworks = []

# ----------------------------------------------------------------------
# Explanation behavior
# ----------------------------------------------------------------------
# [explanation]
# Defaults: normal depth, 3 annotations, 60 words per annotation.
# default_depth = "normal"
# max_annotations = 3
# max_annotation_words = 60
# Defaults: language/framework concepts enabled; infer intent disabled.
# explain_language_concepts = true
# explain_framework_concepts = true
# infer_intent = false

# ----------------------------------------------------------------------
# Cache
# ----------------------------------------------------------------------
# [cache]
# git-explain defaults caching to enabled.
# enabled = true

# ----------------------------------------------------------------------
# Local git-explain web server (not the model endpoint)
# ----------------------------------------------------------------------
# [server]
# Defaults: loopback host, port 8081, open browser.
# host = "127.0.0.1"
# port = 8081
# open_browser = true

# ----------------------------------------------------------------------
# Git analysis
# ----------------------------------------------------------------------
# [git]
# Defaults: compare against HEAD and include staged changes.
# diff_target = "HEAD"
# include_staged = true
# Parsed but not yet implemented; default is false.
# include_untracked = false

# Runtime-only overrides are not stored in TOML:
#   git explain --profile cloud
#   git explain --port 9000
# Environment-only overrides are not stored in TOML:
#   GIT_EXPLAIN_USER_CONFIG (user configuration file path)
#   GIT_EXPLAIN_PROFILE (profile selection for this process)
"#
}

pub fn repository_example_config() -> &'static str {
    r#"# git-explain repository configuration
#
# A repository may select a trusted user-defined profile by name, but profiles,
# endpoints, providers, models, and credential references never belong here.

# [model]
# profile = "work"

# Repository-safe application settings use the same keys as user configuration.
# [reader]
# experience = "experienced"
# known_languages = []
# learning_languages = []
# known_frameworks = []
# learning_frameworks = []

# [explanation]
# default_depth = "normal"
# max_annotations = 3
# max_annotation_words = 60
# explain_language_concepts = true
# explain_framework_concepts = true
# infer_intent = false

# [cache]
# enabled = true

# [server]
# These configure the local git-explain server, never the model endpoint.
# host = "127.0.0.1"
# port = 8081
# open_browser = true

# [git]
# diff_target = "HEAD"
# include_staged = true
# include_untracked = false
"#
}

#[allow(dead_code)]
pub fn format_show(config: &ResolvedConfig) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "User configuration:\n{}",
        config.paths.user.display()
    )
    .unwrap();
    writeln!(
        output,
        "Repository configuration:\n{}",
        config.paths.repository.as_ref().map_or_else(
            || "not available outside a Git repository".into(),
            |path| path.display().to_string()
        )
    )
    .unwrap();
    writeln!(output, "\nSelected profile:\n{}", config.profile).unwrap();
    writeln!(
        output,
        "\nProfile selection source:\n{}",
        config.profile_selection_source.label()
    )
    .unwrap();
    writeln!(output, "\nProfile definition source:\nuser configuration").unwrap();
    writeln!(
        output,
        "Available profiles:\n{}",
        config.available_profiles.join(", ")
    )
    .unwrap();
    writeln!(
        output,
        "\nModel:\nProvider: {}\nPreset: {}\nBase URL: {}\nModel: {}\nAPI key environment variable: {}\nAPI key configured: {}",
        display_provider(&config.model.provider),
        config.model.preset.as_deref().map(display_preset).unwrap_or("<none>"),
        config.model.base_url,
        config.model.model,
        config.model.api_key_env.as_deref().unwrap_or("<none>"),
        if config.model.api_key.is_some() { "yes" } else { "no" }
    )
    .unwrap();
    writeln!(
        output,
        "\nNormal:\nReasoning: {}\nMax tokens: {}\nTemperature: {}",
        display_optional(config.model.normal.reasoning),
        display_optional(config.model.normal.max_tokens),
        display_optional(config.model.normal.temperature)
    )
    .unwrap();
    writeln!(
        output,
        "\nDeep:\nReasoning: {}\nMax tokens: {}\nTemperature: {}",
        display_optional(config.model.deep.reasoning),
        display_optional(config.model.deep.max_tokens),
        display_optional(config.model.deep.temperature)
    )
    .unwrap();
    writeln!(output, "\nReader:\nexperience: {}\nknown languages: {}\nlearning languages: {}\nknown frameworks: {}\nlearning frameworks: {}", config.reader.experience, config.reader.known_languages.join(", "), config.reader.learning_languages.join(", "), config.reader.known_frameworks.join(", "), config.reader.learning_frameworks.join(", ")).unwrap();
    writeln!(output, "\nExplanation:\ndefault depth: {}\nannotation limit: {}\nannotation word limit: {}\nexplain language concepts: {}\nexplain framework concepts: {}\ninfer intent: {}", config.explanation.default_depth, config.explanation.max_annotations, config.explanation.max_annotation_words, config.explanation.explain_language_concepts, config.explanation.explain_framework_concepts, config.explanation.infer_intent).unwrap();
    writeln!(output, "\nCache:\nenabled: {}", config.cache.enabled).unwrap();
    writeln!(
        output,
        "\nServer:\nhost: {}\nport: {}\nopen_browser: {}",
        config.server.host, config.server.port, config.server.open_browser
    )
    .unwrap();
    writeln!(
        output,
        "\nGit:\ndiff_target: {}\ninclude_staged: {}\ninclude_untracked: {} (not yet implemented)",
        config.git.diff_target, config.git.include_staged, config.git.include_untracked
    )
    .unwrap();
    output
}

pub fn format_application_show(config: &ApplicationConfig) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "Configuration\n\nUser configuration:\n{}",
        config.paths.user.display()
    )
    .unwrap();
    writeln!(
        output,
        "\nRepository configuration:\n{}",
        config.paths.repository.as_ref().map_or_else(
            || "not available outside a Git repository".into(),
            |path| path.display().to_string()
        )
    )
    .unwrap();
    writeln!(output, "\nReader (source: {}):\n  Experience: {}\n  Known languages: {}\n  Learning languages: {}\n  Known frameworks: {}\n  Learning frameworks: {}", config.reader_source.label(), config.reader.experience, display_list(&config.reader.known_languages), display_list(&config.reader.learning_languages), display_list(&config.reader.known_frameworks), display_list(&config.reader.learning_frameworks)).unwrap();
    writeln!(output, "\nExplanation (source: {}):\n  Default depth: {}\n  Annotation limit: {}\n  Annotation word limit: {}\n  Explain language concepts: {}\n  Explain framework concepts: {}\n  Infer intent: {}", config.explanation_source.label(), config.explanation.default_depth, config.explanation.max_annotations, config.explanation.max_annotation_words, yes_no(config.explanation.explain_language_concepts), yes_no(config.explanation.explain_framework_concepts), yes_no(config.explanation.infer_intent)).unwrap();
    writeln!(
        output,
        "\nCache (source: {}):\n  Enabled: {}",
        config.cache_source.label(),
        yes_no(config.cache.enabled)
    )
    .unwrap();
    writeln!(
        output,
        "\nServer (source: {}):\n  Host: {}\n  Port: {}\n  Open browser: {}",
        config.server_source.label(),
        config.server.host,
        config.server.port,
        yes_no(config.server.open_browser)
    )
    .unwrap();
    writeln!(
        output,
        "\nGit (source: {}):\n  Diff target: {}\n  Include staged: {}\n  Include untracked: {}",
        config.git_source.label(),
        config.git.diff_target,
        yes_no(config.git.include_staged),
        yes_no(config.git.include_untracked)
    )
    .unwrap();
    writeln!(
        output,
        "\nModel:\n  Selected profile: {}\n  Selection source: {}\n  Available profiles: {}",
        config.profile.as_deref().unwrap_or("<none>"),
        config
            .profile_selection_source
            .as_ref()
            .map(ProfileSelectionSource::label)
            .unwrap_or("default (no selection)"),
        display_list(&config.available_profiles)
    )
    .unwrap();
    output
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "<none>".into()
    } else {
        values.join(", ")
    }
}
fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

pub fn format_profile_show(config: &ResolvedConfig) -> String {
    format!("Profile: {}\nDefinition source: user configuration\n\nProvider: {}\nPreset: {}\nBase URL: {}\nModel: {}\nAPI key environment variable: {}\nAPI key configured: {}\n\nNormal:\nReasoning: {}\nMax tokens: {}\nTemperature: {}\n\nDeep:\nReasoning: {}\nMax tokens: {}\nTemperature: {}\n", config.profile, display_provider(&config.model.provider), config.model.preset.as_deref().map(display_preset).unwrap_or("unspecified"), config.model.base_url, config.model.model, config.model.api_key_env.as_deref().unwrap_or("unspecified"), if config.model.api_key.is_some() { "yes" } else { "no" }, display_optional(config.model.normal.reasoning), display_optional(config.model.normal.max_tokens), display_optional(config.model.normal.temperature), display_optional(config.model.deep.reasoning), display_optional(config.model.deep.max_tokens), display_optional(config.model.deep.temperature))
}

fn display_optional<T: std::fmt::Display>(value: Option<T>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unspecified".into())
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialConfig {
    model: Option<PartialModelConfig>,
    #[serde(default)]
    profiles: BTreeMap<String, PartialProfileConfig>,
    reader: Option<PartialReaderConfig>,
    explanation: Option<PartialExplanationConfig>,
    server: Option<PartialServerConfig>,
    git: Option<PartialGitConfig>,
    cache: Option<PartialCacheConfig>,
}

impl PartialConfig {
    fn merge(&mut self, other: Self) {
        merge_option(&mut self.model, other.model, |left, right| {
            left.merge(right)
        });
        for (name, profile) in other.profiles {
            merge_map_entry(&mut self.profiles, name, profile, |left, right| {
                left.merge(right)
            });
        }
        merge_option(&mut self.reader, other.reader, |left, right| {
            left.merge(right)
        });
        merge_option(&mut self.explanation, other.explanation, |left, right| {
            left.merge(right)
        });
        merge_option(&mut self.server, other.server, |left, right| {
            left.merge(right)
        });
        merge_option(&mut self.git, other.git, |left, right| left.merge(right));
        merge_option(&mut self.cache, other.cache, |left, right| {
            left.merge(right)
        });
    }

    fn merge_repository(&mut self, other: RepositoryConfig) {
        merge_option(&mut self.model, other.model, |left, right| {
            left.merge(right)
        });
        merge_option(&mut self.reader, other.reader, |left, right| {
            left.merge(right)
        });
        merge_option(&mut self.explanation, other.explanation, |left, right| {
            left.merge(right)
        });
        merge_option(&mut self.server, other.server, |left, right| {
            left.merge(right)
        });
        merge_option(&mut self.git, other.git, |left, right| left.merge(right));
        merge_option(&mut self.cache, other.cache, |left, right| {
            left.merge(right)
        });
    }
}

/// Configuration trusted from a repository. It deliberately cannot define profiles,
/// endpoints, credentials, or any other model transport settings.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryConfig {
    model: Option<PartialModelConfig>,
    reader: Option<PartialReaderConfig>,
    explanation: Option<PartialExplanationConfig>,
    server: Option<PartialServerConfig>,
    git: Option<PartialGitConfig>,
    cache: Option<PartialCacheConfig>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialModelConfig {
    profile: Option<String>,
}

impl PartialModelConfig {
    fn merge(&mut self, other: Self) {
        if other.profile.is_some() {
            self.profile = other.profile;
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialProfileConfig {
    provider: Option<String>,
    preset: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    api_key_env: Option<String>,
    normal: Option<PartialGenerationConfig>,
    deep: Option<PartialGenerationConfig>,
}

impl PartialProfileConfig {
    fn merge(&mut self, other: Self) {
        if other.provider.is_some() {
            self.provider = other.provider;
        }
        if other.preset.is_some() {
            self.preset = other.preset;
        }
        if other.base_url.is_some() {
            self.base_url = other.base_url;
        }
        if other.model.is_some() {
            self.model = other.model;
        }
        if other.api_key_env.is_some() {
            self.api_key_env = other.api_key_env;
        }
        merge_option(&mut self.normal, other.normal, |left, right| {
            left.merge(right)
        });
        merge_option(&mut self.deep, other.deep, |left, right| left.merge(right));
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialGenerationConfig {
    reasoning: Option<bool>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
}

impl PartialGenerationConfig {
    fn merge(&mut self, other: Self) {
        if other.reasoning.is_some() {
            self.reasoning = other.reasoning;
        }
        if other.max_tokens.is_some() {
            self.max_tokens = other.max_tokens;
        }
        if other.temperature.is_some() {
            self.temperature = other.temperature;
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialReaderConfig {
    experience: Option<String>,
    known_languages: Option<Vec<String>>,
    learning_languages: Option<Vec<String>>,
    known_frameworks: Option<Vec<String>>,
    learning_frameworks: Option<Vec<String>>,
}

impl PartialReaderConfig {
    fn merge(&mut self, other: Self) {
        if other.experience.is_some() {
            self.experience = other.experience;
        }
        if other.known_languages.is_some() {
            self.known_languages = other.known_languages;
        }
        if other.learning_languages.is_some() {
            self.learning_languages = other.learning_languages;
        }
        if other.known_frameworks.is_some() {
            self.known_frameworks = other.known_frameworks;
        }
        if other.learning_frameworks.is_some() {
            self.learning_frameworks = other.learning_frameworks;
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialExplanationConfig {
    default_depth: Option<String>,
    max_annotations: Option<u32>,
    max_annotation_words: Option<u32>,
    explain_language_concepts: Option<bool>,
    explain_framework_concepts: Option<bool>,
    infer_intent: Option<bool>,
}

impl PartialExplanationConfig {
    fn merge(&mut self, other: Self) {
        if other.default_depth.is_some() {
            self.default_depth = other.default_depth;
        }
        if other.max_annotations.is_some() {
            self.max_annotations = other.max_annotations;
        }
        if other.max_annotation_words.is_some() {
            self.max_annotation_words = other.max_annotation_words;
        }
        if other.explain_language_concepts.is_some() {
            self.explain_language_concepts = other.explain_language_concepts;
        }
        if other.explain_framework_concepts.is_some() {
            self.explain_framework_concepts = other.explain_framework_concepts;
        }
        if other.infer_intent.is_some() {
            self.infer_intent = other.infer_intent;
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialServerConfig {
    host: Option<String>,
    port: Option<u16>,
    open_browser: Option<bool>,
}

impl PartialServerConfig {
    fn merge(&mut self, other: Self) {
        if other.host.is_some() {
            self.host = other.host;
        }
        if other.port.is_some() {
            self.port = other.port;
        }
        if other.open_browser.is_some() {
            self.open_browser = other.open_browser;
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialGitConfig {
    diff_target: Option<String>,
    include_staged: Option<bool>,
    include_untracked: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialCacheConfig {
    enabled: Option<bool>,
}
impl PartialCacheConfig {
    fn merge(&mut self, other: Self) {
        if other.enabled.is_some() {
            self.enabled = other.enabled;
        }
    }
}

impl PartialGitConfig {
    fn merge(&mut self, other: Self) {
        if other.diff_target.is_some() {
            self.diff_target = other.diff_target;
        }
        if other.include_staged.is_some() {
            self.include_staged = other.include_staged;
        }
        if other.include_untracked.is_some() {
            self.include_untracked = other.include_untracked;
        }
    }
}

fn merge_option<T>(left: &mut Option<T>, right: Option<T>, merge: impl FnOnce(&mut T, T)) {
    match (left.as_mut(), right) {
        (Some(left), Some(right)) => merge(left, right),
        (None, Some(right)) => *left = Some(right),
        _ => {}
    }
}

fn merge_map_entry<T>(
    map: &mut BTreeMap<String, T>,
    key: String,
    value: T,
    merge: impl FnOnce(&mut T, T),
) {
    if let Some(existing) = map.get_mut(&key) {
        merge(existing, value);
    } else {
        map.insert(key, value);
    }
}

fn load_file(path: &Path) -> Result<PartialConfig> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read configuration {}", path.display()))?;
    toml::from_str(&text).with_context(|| {
        format!(
            "configuration error: parse user configuration {}",
            path.display()
        )
    })
}

fn load_repository_file(path: &Path) -> Result<RepositoryConfig> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read configuration {}", path.display()))?;
    toml::from_str(&text).map_err(|error| {
        let reason = if error.to_string().contains("unknown field `profiles`") {
            "Repository configuration cannot define model profiles. Profiles contain trusted model endpoints and must be defined in user configuration. To select an existing profile for this repository, use: git explain profile use <name> --repo"
        } else {
            "Repository configuration contains an unsupported or invalid setting."
        };
        anyhow::anyhow!("configuration error: could not load repository configuration.\n\nFile:\n{}\n\n{}\n\nDetails:\n{}", path.display(), reason, error)
    })
}

/// The selected profile name and the layer that supplied it. Keeping this
/// separate from `ResolvedProfile` avoids mixing profile selection with a
/// trusted profile definition or its runtime model settings.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProfileSelection {
    name: Option<String>,
    source: Option<ProfileSelectionSource>,
}

fn select_profile(
    file: &PartialConfig,
    cli_profile: Option<&str>,
    environment: &EnvironmentOverrides,
    repository_profile: Option<String>,
) -> ProfileSelection {
    if let Some(name) = cli_profile {
        return ProfileSelection {
            name: Some(name.to_owned()),
            source: Some(ProfileSelectionSource::CommandLine),
        };
    }
    if let Some(name) = &environment.profile {
        return ProfileSelection {
            name: Some(name.clone()),
            source: Some(ProfileSelectionSource::Environment),
        };
    }
    if let Some(name) = repository_profile {
        return ProfileSelection {
            name: Some(name),
            source: Some(ProfileSelectionSource::Repository),
        };
    }
    let name = file.model.as_ref().and_then(|model| model.profile.clone());
    ProfileSelection {
        source: name.as_ref().map(|_| ProfileSelectionSource::User),
        name,
    }
}

fn resolve(
    file: PartialConfig,
    paths: &ConfigPaths,
    cli_profile: Option<&str>,
    environment: &EnvironmentOverrides,
    repository_profile: Option<String>,
) -> Result<ResolvedConfig> {
    let merged = file;
    let selection = select_profile(&merged, cli_profile, environment, repository_profile);
    let profile = selection.name.context("configuration error: no model profile is configured.\n\nCreate one with:\ngit explain profile add <name> --base-url <url> --model <model>\n\nThen select it with:\ngit explain profile use <name>")?;
    let profile_selection_source = selection.source.expect("selected profile has a source");
    let profile_partial = merged.profiles.get(&profile).cloned().ok_or_else(|| {
        let available = merged.profiles.keys().cloned().collect::<Vec<_>>().join("\n  ");
        if profile_selection_source == ProfileSelectionSource::Repository {
            return anyhow::anyhow!(
                "configuration error: Repository profile \"{profile}\" is not defined in user configuration.\n\nAvailable profiles:\n  {}\n\nCreate the profile or choose another repository profile.",
                if available.is_empty() { "<none>" } else { &available }
            );
        }
        anyhow::Error::new(ProfileNotFound {
            requested: profile.clone(),
            available: merged.profiles.keys().cloned().collect(),
        })
    })?;
    let model = ResolvedProfile {
        provider: profile_partial
            .provider
            .clone()
            .context(format!("profile '{profile}' is missing provider"))?,
        preset: profile_partial.preset.clone(),
        base_url: profile_partial.base_url.clone().or_else(|| {
            profile_partial
                .preset
                .as_deref()
                .and_then(profile_preset)
                .and_then(|preset| preset.default_base_url.map(str::to_owned))
        }).ok_or_else(|| anyhow::anyhow!("profile '{profile}' is missing base_url; specify --base-url or choose preset llama-cpp or ollama"))?,
        model: profile_partial
            .model
            .clone()
            .context(format!("profile '{profile}' is missing model"))?,
        api_key_env: profile_partial.api_key_env.clone(),
        api_key: profile_partial
            .api_key_env
            .as_deref()
            .and_then(|name| std::env::var(name).ok()),
        normal: generation(profile_partial.normal.clone()),
        deep: generation(profile_partial.deep.clone()),
    };
    let preset = resolve_preset(model.preset.as_deref())?;
    validate_preset_provider(preset, &model.provider)
        .with_context(|| format!("invalid profile '{profile}'"))?;
    validate_model_config(&model, &profile)?;
    Ok(ResolvedConfig {
        profile,
        profile_selection_source,
        available_profiles: merged.profiles.keys().cloned().collect(),
        model,
        reader: reader_config(merged.reader),
        explanation: explanation_config(merged.explanation),
        server: server_config(merged.server),
        git: git_config(merged.git),
        cache: CacheConfig {
            enabled: merged.cache.and_then(|c| c.enabled).unwrap_or(true),
        },
        paths: paths.clone(),
    })
}

fn validate_model_config(model: &ResolvedProfile, profile: &str) -> Result<()> {
    let url = reqwest::Url::parse(&model.base_url).with_context(|| {
        format!(
            "profile '{profile}' has an invalid base_url: {}",
            model.base_url
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("profile '{profile}' base_url must use http or https");
    }
    if model.model.trim().is_empty() {
        anyhow::bail!("profile '{profile}' model must not be empty");
    }
    if model
        .api_key_env
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        anyhow::bail!("profile '{profile}' api_key_env must not be empty");
    }
    validate_generation(&model.normal, profile, "normal")?;
    validate_generation(&model.deep, profile, "deep")
}

fn validate_generation(generation: &GenerationConfig, profile: &str, mode: &str) -> Result<()> {
    if generation.max_tokens == Some(0) {
        anyhow::bail!("profile '{profile}' {mode}.max_tokens must be greater than zero");
    }
    if let Some(temperature) = generation.temperature {
        if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
            anyhow::bail!(
                "profile '{profile}' {mode}.temperature must be a finite value from 0 to 2"
            );
        }
    }
    Ok(())
}

fn generation(partial: Option<PartialGenerationConfig>) -> GenerationConfig {
    let partial = partial.unwrap_or_default();
    GenerationConfig {
        reasoning: partial.reasoning,
        max_tokens: partial.max_tokens,
        temperature: partial.temperature,
    }
}
fn reader_config(partial: Option<PartialReaderConfig>) -> ReaderConfig {
    let p = partial.unwrap_or_default();
    ReaderConfig {
        experience: p.experience.unwrap_or_else(|| "experienced".into()),
        known_languages: p.known_languages.unwrap_or_default(),
        learning_languages: p.learning_languages.unwrap_or_default(),
        known_frameworks: p.known_frameworks.unwrap_or_default(),
        learning_frameworks: p.learning_frameworks.unwrap_or_default(),
    }
}
fn explanation_config(partial: Option<PartialExplanationConfig>) -> ExplanationConfig {
    let p = partial.unwrap_or_default();
    ExplanationConfig {
        default_depth: p.default_depth.unwrap_or_else(|| "normal".into()),
        max_annotations: p.max_annotations.unwrap_or(3),
        max_annotation_words: p.max_annotation_words.unwrap_or(60),
        explain_language_concepts: p.explain_language_concepts.unwrap_or(true),
        explain_framework_concepts: p.explain_framework_concepts.unwrap_or(true),
        infer_intent: p.infer_intent.unwrap_or(false),
    }
}
fn server_config(partial: Option<PartialServerConfig>) -> ServerConfig {
    let p = partial.unwrap_or_default();
    ServerConfig {
        host: p.host.unwrap_or_else(|| "127.0.0.1".into()),
        port: p.port.unwrap_or(8081),
        open_browser: p.open_browser.unwrap_or(true),
    }
}
fn git_config(partial: Option<PartialGitConfig>) -> GitConfig {
    let p = partial.unwrap_or_default();
    GitConfig {
        diff_target: p.diff_target.unwrap_or_else(|| "HEAD".into()),
        include_staged: p.include_staged.unwrap_or(true),
        include_untracked: p.include_untracked.unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn loader(user: &str, repository: Option<&str>) -> ConfigLoader {
        let directory = tempdir().unwrap();
        let user_path = directory.path().join("user.toml");
        fs::write(&user_path, user).unwrap();
        let repository_path = repository.map(|text| {
            let path = directory.path().join("repo.toml");
            fs::write(&path, text).unwrap();
            path
        });
        let loader = ConfigLoader::with_paths(user_path, repository_path);
        std::mem::forget(directory);
        loader
    }

    #[test]
    fn defaults_are_safe() {
        let config = loader("[profiles.local]\nprovider = \"openai_compatible\"\nbase_url = \"http://127.0.0.1:8083/v1\"\nmodel = \"local\"", None)
            .resolve_with_environment(Some("local"), &EnvironmentOverrides::default())
            .unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8081);
        assert!(!config.explanation.infer_intent);
    }

    #[test]
    fn repository_deep_merges_user_values() {
        let config = loader(
            "[profiles.local]\nprovider = \"openai_compatible\"\nbase_url = \"http://127.0.0.1:8083/v1\"\nmodel = \"local\"\n[server]\nport = 8081\nopen_browser = true\n",
            Some("[server]\nport = 8090\n"),
        )
        .resolve_with_environment(Some("local"), &EnvironmentOverrides::default())
        .unwrap();
        assert_eq!(config.server.port, 8090);
        assert!(config.server.open_browser);
    }

    #[test]
    fn repository_cannot_define_a_profile_or_endpoint() {
        let error = loader(
            "[profiles.safe]\nprovider = \"openai_compatible\"\nbase_url = \"http://127.0.0.1:8083/v1\"\nmodel = \"local\"",
            Some("[profiles.safe]\nbase_url = \"https://unexpected.example/v1\""),
        )
        .resolve_with_environment(Some("safe"), &EnvironmentOverrides::default())
        .unwrap_err()
        .to_string();
        assert!(error.contains("Repository configuration cannot define model profiles"));
    }

    #[test]
    fn repository_selection_overrides_user_default_and_cli_overrides_repository() {
        let config = loader(
            "[model]\nprofile = \"local\"\n[profiles.local]\nprovider = \"openai_compatible\"\nbase_url = \"http://local\"\nmodel = \"local\"\n[profiles.work]\nprovider = \"openai_compatible\"\nbase_url = \"http://work\"\nmodel = \"work\"",
            Some("[model]\nprofile = \"work\""),
        );
        let repository = config
            .resolve_with_environment(None, &EnvironmentOverrides::default())
            .unwrap();
        assert_eq!(repository.profile, "work");
        assert_eq!(
            repository.profile_selection_source,
            ProfileSelectionSource::Repository
        );
        let cli = config
            .resolve_with_environment(Some("local"), &EnvironmentOverrides::default())
            .unwrap();
        assert_eq!(cli.profile, "local");
        assert_eq!(
            cli.profile_selection_source,
            ProfileSelectionSource::CommandLine
        );
    }

    #[test]
    fn repository_init_and_selection_only_write_safe_fields() {
        let directory = tempdir().unwrap();
        let path = directory.path().join(".git").join("git-explain.toml");
        assert!(init_repository_config(&path, false).unwrap());
        assert!(!init_repository_config(&path, false).unwrap());
        assert!(toml::from_str::<RepositoryConfig>(&fs::read_to_string(&path).unwrap()).is_ok());
        use_repository_profile(&path, "work").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("profile = \"work\""));
        assert!(!text.contains("base_url"));
        assert!(!text.contains("[profiles."));
    }

    #[test]
    fn repository_profile_write_preserves_unrelated_settings() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("git-explain.toml");
        fs::write(
            &path,
            "[model]\nprofile = \"local\"\n[git]\ninclude_staged = true\n",
        )
        .unwrap();
        use_repository_profile(&path, "work").unwrap();
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("profile = \"work\""));
        assert!(text.contains("include_staged = true"));
    }

    #[test]
    fn environment_and_cli_select_profiles_without_mutating_them() {
        let config = loader("[model]\nprofile = \"one\"\n[profiles.one]\nprovider = \"openai_compatible\"\nbase_url = \"http://file\"\nmodel = \"file\"\n[profiles.two]\nprovider = \"openai_compatible\"\nbase_url = \"http://two\"\nmodel = \"two\"\n", None).resolve_with_environment(Some("two"), &EnvironmentOverrides::default()).unwrap();
        assert_eq!(config.profile, "two");
        assert_eq!(config.model.base_url, "http://two");
        assert_eq!(config.model.model, "two");
    }

    #[test]
    fn profile_lookup_exposes_requested_and_available_names() {
        let error = loader(
            "[profiles.qwen35b]\nprovider = \"openai_compatible\"\nbase_url = \"http://127.0.0.1:8083/v1\"\nmodel = \"qwen\"",
            None,
        )
        .resolve_with_environment(Some("q"), &EnvironmentOverrides::default())
        .unwrap_err();
        let lookup = error.downcast_ref::<ProfileNotFound>().unwrap();
        assert_eq!(lookup.requested, "q");
        assert_eq!(lookup.available, ["qwen35b"]);
    }

    #[test]
    fn generation_update_has_unambiguous_set_clear_and_unchanged_states() {
        assert_eq!(
            Update::from_flag(Some(500), false, "--set", "--clear").unwrap(),
            Update::Set(500)
        );
        assert_eq!(
            Update::<u32>::from_flag(None, true, "--set", "--clear").unwrap(),
            Update::Clear
        );
        assert_eq!(
            Update::<u32>::from_flag(None, false, "--set", "--clear").unwrap(),
            Update::Unchanged
        );
        assert!(Update::<u32>::from_flag(Some(500), true, "--set", "--clear").is_err());
    }

    #[test]
    fn cloud_profile_uses_bearer_key_environment_setting() {
        let config = loader(
            "[profiles.cloud]\nprovider = \"openai_compatible\"\nbase_url = \"https://example.test/v1\"\nmodel = \"cloud-model\"\napi_key_env = \"OPENAI_API_KEY\"\n",
            None,
        )
        .resolve_with_environment(
            Some("cloud"),
            &EnvironmentOverrides::default(),
        )
        .unwrap();
        assert_eq!(config.model.api_key_env.as_deref(), Some("OPENAI_API_KEY"));
        assert!(config.model.api_key.is_none());
        assert_eq!(config.model.base_url, "https://example.test/v1");
    }

    #[test]
    fn malformed_and_unknown_fields_include_path() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(&path, "[server]\nmax_token = 1\n").unwrap();
        let error = ConfigLoader::with_paths(path.clone(), None)
            .resolve(None)
            .unwrap_err()
            .to_string();
        assert!(error.contains(&path.display().to_string()));
    }

    #[test]
    fn init_is_safe_and_generated_config_parses() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        assert!(init_user_config(&path, false).unwrap());
        assert!(!init_user_config(&path, false).unwrap());
        let generated = fs::read_to_string(path).unwrap();
        assert!(toml::from_str::<PartialConfig>(&generated).is_ok());
        assert!(generated.contains("This template documents every TOML-backed user setting"));
        assert!(generated.contains("# max_annotations = 3"));
        assert!(generated.contains("# Local git-explain web server (not the model endpoint)"));
    }

    #[test]
    fn user_template_documents_every_user_setting_without_creating_a_profile() {
        // Keep this inventory in sync whenever a user-editable TOML field is added.
        let template = example_config();
        for field in [
            "# profile = \"local\"",
            "# provider = \"openai_compatible\"",
            "# preset = \"llama_cpp\"",
            "# base_url = \"http://127.0.0.1:8083/v1\"",
            "# model = \"your-model\"",
            "# api_key_env = \"LOCAL_MODEL_API_KEY\"",
            "# reasoning = false",
            "# reasoning = true",
            "# max_tokens = 500",
            "# max_tokens = 2500",
            "# temperature = 0.2",
            "# temperature = 0.3",
            "# experience = \"experienced\"",
            "# known_languages = []",
            "# learning_languages = []",
            "# known_frameworks = []",
            "# learning_frameworks = []",
            "# default_depth = \"normal\"",
            "# max_annotations = 3",
            "# max_annotation_words = 60",
            "# explain_language_concepts = true",
            "# explain_framework_concepts = true",
            "# infer_intent = false",
            "# enabled = true",
            "# host = \"127.0.0.1\"",
            "# port = 8081",
            "# open_browser = true",
            "# diff_target = \"HEAD\"",
            "# include_staged = true",
            "# include_untracked = false",
            "GIT_EXPLAIN_USER_CONFIG",
            "GIT_EXPLAIN_PROFILE",
            "git explain --profile cloud",
            "git explain --port 9000",
        ] {
            assert!(template.contains(field), "missing template field: {field}");
        }
        let parsed: PartialConfig = toml::from_str(template).unwrap();
        assert!(parsed.model.is_none());
        assert!(parsed.profiles.is_empty());
        assert!(parsed.reader.is_none());
        assert!(parsed.explanation.is_none());
        assert!(parsed.cache.is_none());
        assert!(parsed.server.is_none());
        assert!(parsed.git.is_none());
    }

    #[test]
    fn generated_user_template_stays_documented_after_profile_and_config_edits() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        init_user_config(&path, false).unwrap();
        assert!(profile_names(&path).unwrap().is_empty());
        add_profile(
            &path,
            ProfileDraft {
                name: "qwen35b".into(),
                provider: None,
                preset: Some("llama_cpp".into()),
                base_url: None,
                model_port: None,
                model: "git-explain-unsloth35b".into(),
                api_key_env: None,
            },
        )
        .unwrap();
        assert_eq!(profile_names(&path).unwrap(), ["qwen35b"]);
        assert_eq!(
            ConfigLoader::with_paths(path.clone(), None)
                .resolve_with_environment(Some("qwen35b"), &EnvironmentOverrides::default())
                .unwrap()
                .model
                .model,
            "git-explain-unsloth35b"
        );
        edit_config(
            &path,
            false,
            &ConfigUpdate {
                server: ServerUpdate {
                    port: Some(9000),
                    ..ServerUpdate::default()
                },
                ..ConfigUpdate::default()
            },
            &[],
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# Example local model profile"));
        assert!(text.contains("# Active values written by git-explain"));
        assert_eq!(
            ConfigLoader::with_paths(path, None)
                .application_config(None)
                .unwrap()
                .server
                .port,
            9000
        );
    }

    #[test]
    fn repository_template_documents_all_safe_settings_and_no_profile_definition() {
        // Keep this inventory in sync whenever a repository-safe TOML field changes.
        let template = repository_example_config();
        for field in [
            "# profile = \"work\"",
            "# experience = \"experienced\"",
            "# known_languages = []",
            "# default_depth = \"normal\"",
            "# max_annotations = 3",
            "# enabled = true",
            "# host = \"127.0.0.1\"",
            "# port = 8081",
            "# diff_target = \"HEAD\"",
            "# include_staged = true",
            "# include_untracked = false",
        ] {
            assert!(
                template.contains(field),
                "missing repository field: {field}"
            );
        }
        let parsed: RepositoryConfig = toml::from_str(template).unwrap();
        assert!(parsed.model.is_none());
        assert!(parsed.reader.is_none());
        assert!(parsed.explanation.is_none());
        assert!(parsed.cache.is_none());
        assert!(parsed.server.is_none());
        assert!(parsed.git.is_none());
        for unsafe_field in [
            "[profiles.",
            "base_url",
            "api_key_env",
            "provider =",
            "preset =",
        ] {
            assert!(
                !template.contains(unsafe_field),
                "unsafe repository field: {unsafe_field}"
            );
        }
    }

    #[test]
    fn generated_config_has_no_fake_profile() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        init_user_config(&path, false).unwrap();
        assert!(profile_names(&path).unwrap().is_empty());
        let error = ConfigLoader::with_paths(path, None)
            .resolve(None)
            .unwrap_err();
        assert!(error.to_string().contains("no model profile is configured"));
    }

    #[test]
    fn editing_a_profile_is_partial_and_can_clear_optional_fields() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(&path, "[model]\nprofile = \"local\"\n[profiles.local]\nprovider = \"openai_compatible\"\npreset = \"llama_cpp\"\nbase_url = \"http://127.0.0.1:8083/v1\"\nmodel = \"old\"\napi_key_env = \"LOCAL_KEY\"\n[profiles.local.normal]\nmax_tokens = 500\ntemperature = 0.2\n[reader]\nexperience = \"new\"\n[profiles.other]\nprovider = \"openai_compatible\"\nbase_url = \"http://example.test/v1\"\nmodel = \"other\"").unwrap();
        edit_profile(
            &path,
            "local",
            ProfileUpdate {
                model: Some("new".into()),
                clear_preset: true,
                clear_api_key_env: true,
                clear_normal_temperature: true,
                ..ProfileUpdate::default()
            },
        )
        .unwrap();
        let config = ConfigLoader::with_paths(path.clone(), None)
            .resolve(None)
            .unwrap();
        assert_eq!(config.model.model, "new");
        assert_eq!(config.model.preset, None);
        assert_eq!(config.model.api_key_env, None);
        assert_eq!(config.model.normal.max_tokens, Some(500));
        assert_eq!(config.model.normal.temperature, None);
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("experience = \"new\""));
        assert!(text.contains("[profiles.other]"));
    }

    #[test]
    fn invalid_edit_does_not_change_existing_profile() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let original = "[profiles.local]\nprovider = \"openai_compatible\"\nbase_url = \"http://127.0.0.1:8083/v1\"\nmodel = \"local\"\n";
        fs::write(&path, original).unwrap();
        assert!(edit_profile(
            &path,
            "local",
            ProfileUpdate {
                base_url: Some("banana".into()),
                ..ProfileUpdate::default()
            }
        )
        .is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn add_rejects_duplicates_and_invalid_profile_values() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let profile = ProfileDraft {
            name: "local".into(),
            provider: Some("openai_compatible".into()),
            preset: Some("llama_cpp".into()),
            base_url: Some("http://127.0.0.1:8083/v1".into()),
            model: "local".into(),
            api_key_env: None,
            model_port: None,
        };
        add_profile(&path, profile.clone()).unwrap();
        assert!(add_profile(&path, profile)
            .unwrap_err()
            .to_string()
            .contains("already exists"));
        assert!(add_profile(
            &path,
            ProfileDraft {
                name: "bad name".into(),
                provider: Some("openai_compatible".into()),
                preset: None,
                base_url: Some("banana".into()),
                model: "".into(),
                api_key_env: None,
                model_port: None
            }
        )
        .is_err());
    }

    #[test]
    fn add_profile_with_update_round_trips_all_generation_fields() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        add_profile_with_update(
            &path,
            ProfileDraft {
                name: "complete".into(),
                provider: Some("openai_compatible".into()),
                preset: Some("llama_cpp".into()),
                base_url: None,
                model_port: Some(9000),
                model: "qwen".into(),
                api_key_env: Some("MODEL_KEY".into()),
            },
            ProfileUpdate {
                normal_reasoning: Some(false),
                normal_max_tokens: Some(600),
                normal_temperature: Some(0.2),
                deep_reasoning: Some(true),
                deep_max_tokens: Some(3000),
                deep_temperature: Some(0.35),
                ..ProfileUpdate::default()
            },
        )
        .unwrap();
        let resolved = ConfigLoader::with_paths(path, None)
            .resolve(Some("complete"))
            .unwrap();
        assert_eq!(resolved.model.base_url, "http://127.0.0.1:9000/v1");
        assert_eq!(resolved.model.api_key_env.as_deref(), Some("MODEL_KEY"));
        assert_eq!(resolved.model.normal.reasoning, Some(false));
        assert_eq!(resolved.model.normal.max_tokens, Some(600));
        assert_eq!(resolved.model.normal.temperature, Some(0.2));
        assert_eq!(resolved.model.deep.reasoning, Some(true));
        assert_eq!(resolved.model.deep.max_tokens, Some(3000));
        assert_eq!(resolved.model.deep.temperature, Some(0.35));
    }

    #[test]
    fn show_redacts_api_key() {
        let mut config = loader("[profiles.local]\nprovider = \"openai_compatible\"\nbase_url = \"http://127.0.0.1:8083/v1\"\nmodel = \"local\"", None)
            .resolve_with_environment(Some("local"), &EnvironmentOverrides::default())
            .unwrap();
        config.model.api_key = Some("secret-value".into());
        let shown = format_show(&config);
        assert!(!shown.contains("secret-value"));
        assert!(shown.contains("API key configured: yes"));
        assert!(shown.contains("Available profiles:"));
        assert!(shown.contains("local"));
        assert!(!shown.contains("Some("));
        assert!(!shown.contains("None"));
        assert!(shown.contains("Reasoning: unspecified"));
        assert!(shown.contains("OpenAI-compatible"));
    }

    #[test]
    fn profile_definition_display_does_not_claim_selection_provenance() {
        let config = loader("[profiles.local]\nprovider = \"openai_compatible\"\npreset = \"llama_cpp\"\nbase_url = \"http://127.0.0.1:8083/v1\"\nmodel = \"local\"", None)
            .resolve_with_environment(Some("local"), &EnvironmentOverrides::default())
            .unwrap();
        let shown = format_profile_show(&config);
        assert!(shown.contains("Profile: local"));
        assert!(shown.contains("Provider: OpenAI-compatible"));
        assert!(shown.contains("Preset: llama.cpp"));
        assert!(!shown.contains("Profile selection source"));
        assert!(!shown.contains("Some("));
    }

    #[test]
    fn presets_supply_endpoints_and_explicit_urls_win() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        add_profile(
            &path,
            ProfileDraft {
                name: "local".into(),
                provider: None,
                preset: Some("llama-cpp".into()),
                base_url: None,
                model: "foo".into(),
                api_key_env: None,
                model_port: None,
            },
        )
        .unwrap();
        add_profile(
            &path,
            ProfileDraft {
                name: "ollama".into(),
                provider: None,
                preset: Some("ollama".into()),
                base_url: Some("http://custom:9999/v1".into()),
                model: "bar".into(),
                api_key_env: None,
                model_port: None,
            },
        )
        .unwrap();
        let loader = ConfigLoader::with_paths(path, None);
        assert_eq!(
            loader.resolve(Some("local")).unwrap().model.base_url,
            "http://127.0.0.1:8083/v1"
        );
        assert_eq!(
            loader.resolve(Some("ollama")).unwrap().model.base_url,
            "http://custom:9999/v1"
        );
    }

    #[test]
    fn add_without_endpoint_has_actionable_error() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let error = add_profile(
            &path,
            ProfileDraft {
                name: "cloud".into(),
                provider: None,
                preset: None,
                base_url: None,
                model: "foo".into(),
                api_key_env: None,
                model_port: None,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("A base URL is required"));
        assert!(error.contains("--preset llama-cpp"));
    }

    #[test]
    fn model_port_overrides_preset_endpoint_and_rejects_invalid_combinations() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        add_profile(
            &path,
            ProfileDraft {
                name: "local".into(),
                provider: None,
                preset: Some("llama-cpp".into()),
                base_url: None,
                model_port: Some(9000),
                model: "foo".into(),
                api_key_env: None,
            },
        )
        .unwrap();
        assert_eq!(
            ConfigLoader::with_paths(path.clone(), None)
                .resolve(Some("local"))
                .unwrap()
                .model
                .base_url,
            "http://127.0.0.1:9000/v1"
        );
        add_profile(
            &path,
            ProfileDraft {
                name: "ollama".into(),
                provider: None,
                preset: Some("ollama".into()),
                base_url: None,
                model_port: Some(12000),
                model: "foo".into(),
                api_key_env: None,
            },
        )
        .unwrap();
        assert_eq!(
            ConfigLoader::with_paths(path.clone(), None)
                .resolve(Some("ollama"))
                .unwrap()
                .model
                .base_url,
            "http://127.0.0.1:12000/v1"
        );
        let error = add_profile(
            &path,
            ProfileDraft {
                name: "bad".into(),
                provider: None,
                preset: None,
                base_url: None,
                model_port: Some(9000),
                model: "foo".into(),
                api_key_env: None,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("requires a profile preset"));
        let error = add_profile(
            &path,
            ProfileDraft {
                name: "conflict".into(),
                provider: None,
                preset: Some("ollama".into()),
                base_url: Some("http://custom:1/v1".into()),
                model_port: Some(9000),
                model: "foo".into(),
                api_key_env: None,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("cannot be used together"));
    }

    #[test]
    fn model_port_edit_preserves_endpoint_components() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        add_profile(
            &path,
            ProfileDraft {
                name: "local".into(),
                provider: None,
                preset: Some("llama-cpp".into()),
                base_url: Some("https://models.example.internal:8443/v1".into()),
                model_port: None,
                model: "foo".into(),
                api_key_env: Some("MODEL_KEY".into()),
            },
        )
        .unwrap();
        edit_profile(
            &path,
            "local",
            ProfileUpdate {
                model_port: Some(9000),
                ..Default::default()
            },
        )
        .unwrap();
        let config = ConfigLoader::with_paths(path, None)
            .resolve(Some("local"))
            .unwrap();
        assert_eq!(
            config.model.base_url,
            "https://models.example.internal:9000/v1"
        );
        assert_eq!(config.model.model, "foo");
        assert_eq!(config.model.api_key_env.as_deref(), Some("MODEL_KEY"));
    }

    #[test]
    fn incompatible_preset_provider_is_rejected() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let error = add_profile(
            &path,
            ProfileDraft {
                name: "bad".into(),
                provider: Some("incompatible".into()),
                preset: Some("llama-cpp".into()),
                base_url: None,
                model: "foo".into(),
                api_key_env: None,
                model_port: None,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("requires provider"));
    }

    #[test]
    fn edit_preserves_custom_endpoint_when_changing_preset() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        add_profile(
            &path,
            ProfileDraft {
                name: "local".into(),
                provider: None,
                preset: Some("ollama".into()),
                base_url: Some("http://192.168.1.10:11434/v1".into()),
                model: "old".into(),
                api_key_env: None,
                model_port: None,
            },
        )
        .unwrap();
        edit_profile(
            &path,
            "local",
            ProfileUpdate {
                preset: Some("llama-cpp".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let config = ConfigLoader::with_paths(path, None)
            .resolve(Some("local"))
            .unwrap();
        assert_eq!(config.model.base_url, "http://192.168.1.10:11434/v1");
        assert_eq!(config.model.preset.as_deref(), Some("llama-cpp"));
    }

    #[test]
    fn edit_without_changes_is_rejected_without_writing() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let original = "[profiles.local]\nprovider = \"openai_compatible\"\nbase_url = \"http://local\"\nmodel = \"local\"\n";
        fs::write(&path, original).unwrap();
        let error = edit_profile(&path, "local", ProfileUpdate::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("No profile changes were specified"));
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn application_updates_are_atomic_and_cover_all_sections() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(&path, "[profiles.local]\nprovider = \"openai_compatible\"\nbase_url = \"http://local\"\nmodel = \"local\"\n[model]\nprofile = \"local\"\n").unwrap();
        let update = ConfigUpdate {
            reader: ReaderUpdate {
                experience: Some("intermediate".into()),
                known_languages: ListUpdate {
                    add: vec!["Rust".into()],
                    ..Default::default()
                },
                ..Default::default()
            },
            explanation: ExplanationUpdate {
                default_depth: Some("deep".into()),
                max_annotations: Some(20),
                max_annotation_words: Some(80),
                explain_language_concepts: Some(false),
                explain_framework_concepts: Some(true),
                infer_intent: Some(true),
            },
            cache: CacheUpdate {
                enabled: Some(false),
            },
            server: ServerUpdate {
                host: Some("127.0.0.1".into()),
                port: Some(9000),
                open_browser: Some(false),
            },
            git: GitUpdate {
                diff_target: Some("HEAD~1".into()),
                include_staged: Some(false),
                include_untracked: Some(true),
            },
            model: ModelSelectionUpdate {
                profile: Some("local".into()),
                clear_profile: false,
            },
        };
        assert!(edit_config(&path, false, &update, &["local".into()]).unwrap());
        let resolved = ConfigLoader::with_paths(path.clone(), None)
            .resolve(None)
            .unwrap();
        assert_eq!(resolved.reader.known_languages, ["Rust"]);
        assert_eq!(resolved.explanation.max_annotations, 20);
        assert!(!resolved.cache.enabled);
        assert_eq!(resolved.server.port, 9000);
        assert!(resolved.git.include_untracked);
        let original = fs::read_to_string(&path).unwrap();
        let invalid = ConfigUpdate {
            server: ServerUpdate {
                host: Some("0.0.0.0".into()),
                port: Some(9001),
                open_browser: None,
            },
            ..Default::default()
        };
        assert!(edit_config(&path, false, &invalid, &["local".into()]).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn application_show_does_not_require_a_model_profile() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(&path, "[cache]\nenabled = false\n[server]\nport = 9000\n").unwrap();
        let application = ConfigLoader::with_paths(path, None)
            .application_config(None)
            .unwrap();
        assert!(application.profile.is_none());
        let shown = format_application_show(&application);
        assert!(shown.contains("Cache (source: user configuration):\n  Enabled: no"));
        assert!(shown.contains("Port: 9000"));
        assert!(!shown.contains("Base URL"));
    }
}
