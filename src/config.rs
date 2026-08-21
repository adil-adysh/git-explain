use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedConfig {
    pub profile: String,
    pub model: ModelConfig,
    pub reader: ReaderConfig,
    pub explanation: ExplanationConfig,
    pub server: ServerConfig,
    pub git: GitConfig,
    pub cache: CacheConfig,
    pub paths: ConfigPaths,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key_env: Option<String>,
    pub api_key: Option<String>,
    pub normal: GenerationConfig,
    pub deep: GenerationConfig,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GenerationConfig {
    pub reasoning: bool,
    pub max_tokens: u32,
    pub temperature: f32,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigPaths {
    pub user: PathBuf,
    pub repository: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvironmentOverrides {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub profile: Option<String>,
}

impl EnvironmentOverrides {
    pub fn from_process() -> Self {
        Self {
            base_url: std::env::var("GIT_EXPLAIN_BASE_URL").ok(),
            model: std::env::var("GIT_EXPLAIN_MODEL").ok(),
            api_key: std::env::var("GIT_EXPLAIN_API_KEY").ok(),
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
        if let Some(repository) = &self.paths.repository {
            if repository.exists() {
                file.merge(load_file(repository)?);
            }
        }
        resolve(file, &self.paths, cli_profile, environment)
    }
}

pub fn default_user_config_path() -> Result<PathBuf> {
    let directories = ProjectDirs::from("", "", "git-explain")
        .context("determine the user configuration directory")?;
    Ok(directories.config_dir().join("config.toml"))
}

pub fn repository_config_path(root: &Path) -> PathBuf {
    root.join(".git").join("git-explain.toml")
}

pub fn init_user_config(path: &Path, force: bool) -> Result<bool> {
    if path.exists() && !force {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(path, example_config()).with_context(|| format!("write {}", path.display()))?;
    Ok(true)
}

pub fn example_config() -> &'static str {
    r#"# git-explain configuration
# Precedence: CLI > environment > repository > user > defaults

[model]
profile = "qwen35b"

[profiles.qwen35b]
provider = "llama_cpp"
base_url = "http://127.0.0.1:8081/v1"
model = "qwen36-35b-a3b"
api_key_env = "GIT_EXPLAIN_API_KEY"

[profiles.qwen35b.normal]
reasoning = false
max_tokens = 500
temperature = 0.2

[profiles.qwen35b.deep]
reasoning = true
max_tokens = 2500
temperature = 0.3

[profiles.unsloth35b]
provider = "llama_cpp"
base_url = "http://127.0.0.1:8083/v1"
model = "git-explain-unsloth35b"
api_key_env = "GIT_EXPLAIN_API_KEY"

[profiles.unsloth35b.normal]
reasoning = false
max_tokens = 500
temperature = 0.2

[profiles.unsloth35b.deep]
reasoning = true
max_tokens = 2500
temperature = 0.3

[profiles.ministral]
provider = "openai_compatible"
base_url = "http://127.0.0.1:11434/v1"
model = "ministral-3:8b"

[reader]
# experience = "experienced"
# known_languages = ["python", "go"]
# learning_languages = ["rust", "typescript"]

[explanation]
# default_depth = "normal"
# max_annotations = 3
# max_annotation_words = 60
# explain_language_concepts = true
# explain_framework_concepts = true
# infer_intent = false

[cache]
# enabled = true

[server]
# host = "127.0.0.1"
# port = 8081
# open_browser = true

[git]
# diff_target = "HEAD"
# include_staged = true
# include_untracked = false # parsed but not yet included in Git diffs
"#
}

pub fn format_show(config: &ResolvedConfig) -> String {
    let mut output = String::new();
    writeln!(output, "User config:\n{}", config.paths.user.display()).unwrap();
    writeln!(
        output,
        "Repository config:\n{}",
        config.paths.repository.as_ref().map_or_else(
            || "<not in a Git repository>".into(),
            |path| path.display().to_string()
        )
    )
    .unwrap();
    writeln!(output, "\nActive profile:\n{}", config.profile).unwrap();
    writeln!(
        output,
        "\nModel:\nprovider: {}\nbase_url: {}\nmodel: {}\napi_key: {}",
        config.model.provider,
        config.model.base_url,
        config.model.model,
        if config.model.api_key.is_some() {
            "<set>"
        } else {
            "<not set>"
        }
    )
    .unwrap();
    writeln!(
        output,
        "\nNormal:\nreasoning: {}\nmax_tokens: {}\ntemperature: {}",
        config.model.normal.reasoning,
        config.model.normal.max_tokens,
        config.model.normal.temperature
    )
    .unwrap();
    writeln!(
        output,
        "\nDeep:\nreasoning: {}\nmax_tokens: {}\ntemperature: {}",
        config.model.deep.reasoning, config.model.deep.max_tokens, config.model.deep.temperature
    )
    .unwrap();
    writeln!(output, "\nReader:\nexperience: {}\nknown languages: {}\nlearning languages: {}\nknown frameworks: {}\nlearning frameworks: {}", config.reader.experience, config.reader.known_languages.join(", "), config.reader.learning_languages.join(", "), config.reader.known_frameworks.join(", "), config.reader.learning_frameworks.join(", ")).unwrap();
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
    toml::from_str(&text).with_context(|| format!("parse configuration {}", path.display()))
}

fn resolve(
    file: PartialConfig,
    paths: &ConfigPaths,
    cli_profile: Option<&str>,
    environment: &EnvironmentOverrides,
) -> Result<ResolvedConfig> {
    let defaults = default_partials();
    let mut merged = defaults;
    merged.merge(file);
    let profile = cli_profile
        .or(environment.profile.as_deref())
        .or(merged
            .model
            .as_ref()
            .and_then(|model| model.profile.as_deref()))
        .unwrap_or("default")
        .to_string();
    let profile_partial = merged.profiles.get(&profile).cloned().ok_or_else(|| {
        anyhow::anyhow!(
            "unknown model profile '{}'; available profiles: {}",
            profile,
            merged
                .profiles
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let default_profile = default_profile();
    let model = ModelConfig {
        provider: profile_partial
            .provider
            .clone()
            .or(default_profile.provider.clone())
            .unwrap_or_else(|| "openai_compatible".into()),
        base_url: environment
            .base_url
            .clone()
            .or(profile_partial.base_url.clone())
            .or(default_profile.base_url.clone())
            .unwrap_or_else(|| "http://127.0.0.1:8000/v1".into()),
        model: environment
            .model
            .clone()
            .or(profile_partial.model.clone())
            .or(default_profile.model.clone())
            .unwrap_or_else(|| "local-model".into()),
        api_key_env: profile_partial
            .api_key_env
            .clone()
            .or(default_profile.api_key_env.clone()),
        api_key: environment.api_key.clone().or_else(|| {
            profile_partial
                .api_key_env
                .as_deref()
                .or(default_profile.api_key_env.as_deref())
                .and_then(|name| std::env::var(name).ok())
        }),
        normal: generation(
            profile_partial
                .normal
                .clone()
                .or(default_profile.normal.clone()),
            false,
            450,
            0.2,
        ),
        deep: generation(
            profile_partial
                .deep
                .clone()
                .or(default_profile.deep.clone()),
            true,
            1200,
            0.2,
        ),
    };
    Ok(ResolvedConfig {
        profile,
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

fn default_partials() -> PartialConfig {
    let mut config = PartialConfig::default();
    config.model = Some(PartialModelConfig {
        profile: Some("default".into()),
    });
    config.profiles.insert("default".into(), default_profile());
    config
}

fn default_profile() -> PartialProfileConfig {
    PartialProfileConfig {
        provider: Some("openai_compatible".into()),
        base_url: Some("http://127.0.0.1:8000/v1".into()),
        model: Some("local-model".into()),
        api_key_env: Some("GIT_EXPLAIN_API_KEY".into()),
        normal: Some(PartialGenerationConfig {
            reasoning: Some(false),
            max_tokens: Some(450),
            temperature: Some(0.2),
        }),
        deep: Some(PartialGenerationConfig {
            reasoning: Some(true),
            max_tokens: Some(1200),
            temperature: Some(0.2),
        }),
    }
}

fn generation(
    partial: Option<PartialGenerationConfig>,
    reasoning: bool,
    max_tokens: u32,
    temperature: f32,
) -> GenerationConfig {
    let partial = partial.unwrap_or_default();
    GenerationConfig {
        reasoning: partial.reasoning.unwrap_or(reasoning),
        max_tokens: partial.max_tokens.unwrap_or(max_tokens),
        temperature: partial.temperature.unwrap_or(temperature),
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
        let config = ConfigLoader::with_paths(PathBuf::from("missing-user"), None)
            .resolve_with_environment(None, &EnvironmentOverrides::default())
            .unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8081);
        assert!(!config.explanation.infer_intent);
    }

    #[test]
    fn repository_deep_merges_user_values() {
        let config = loader(
            "[server]\nport = 8081\nopen_browser = true\n",
            Some("[server]\nport = 8090\n"),
        )
        .resolve_with_environment(None, &EnvironmentOverrides::default())
        .unwrap();
        assert_eq!(config.server.port, 8090);
        assert!(config.server.open_browser);
    }

    #[test]
    fn environment_and_cli_profile_override_files() {
        let config = loader("[model]\nprofile = \"one\"\n[profiles.one]\nbase_url = \"http://file\"\nmodel = \"file\"\n[profiles.two]\nbase_url = \"http://two\"\nmodel = \"two\"\n", None).resolve_with_environment(Some("two"), &EnvironmentOverrides { base_url: Some("http://env".into()), model: Some("env-model".into()), ..Default::default() }).unwrap();
        assert_eq!(config.profile, "two");
        assert_eq!(config.model.base_url, "http://env");
        assert_eq!(config.model.model, "env-model");
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
        assert!(toml::from_str::<PartialConfig>(&fs::read_to_string(path).unwrap()).is_ok());
    }

    #[test]
    fn generated_config_includes_unsloth_llama_profile() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("config.toml");
        init_user_config(&path, false).unwrap();
        let config = ConfigLoader::with_paths(path, None)
            .resolve(Some("unsloth35b"))
            .unwrap();
        assert_eq!(config.model.provider, "llama_cpp");
        assert_eq!(config.model.base_url, "http://127.0.0.1:8083/v1");
        assert_eq!(config.model.model, "git-explain-unsloth35b");
        assert_eq!(config.model.deep.max_tokens, 2500);
    }

    #[test]
    fn show_redacts_api_key() {
        let mut config = ConfigLoader::with_paths(PathBuf::from("missing"), None)
            .resolve_with_environment(
                None,
                &EnvironmentOverrides {
                    api_key: Some("secret-value".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        config.model.api_key = Some("secret-value".into());
        let shown = format_show(&config);
        assert!(!shown.contains("secret-value"));
        assert!(shown.contains("api_key: <set>"));
    }
}
