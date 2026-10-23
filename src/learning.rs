use crate::config::{ConfigPaths, ReaderConfig};
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::PathBuf};

macro_rules! choices { ($name:ident { $($v:ident),+ }) => {
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all="snake_case")]
pub enum $name { $($v),+ }
}; }
choices!(LanguageHelp {
    Automatic,
    Explain,
    Brief,
    Skip
});
choices!(Focus {
    Balanced,
    Syntax,
    Execution,
    Reasoning,
    Architecture
});
choices!(Detail {
    Concise,
    Balanced,
    Detailed
});

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LearningPreferences {
    pub language_help: Option<LanguageHelp>,
    pub focus: Option<Focus>,
    pub detail: Option<Detail>,
    pub known_languages: Option<Vec<String>>,
    pub learning_languages: Option<Vec<String>>,
    pub known_frameworks: Option<Vec<String>>,
    pub learning_frameworks: Option<Vec<String>>,
    pub known_domains: Option<Vec<String>>,
    pub learning_domains: Option<Vec<String>>,
    pub known_concepts: Option<Vec<String>>,
    pub learning_concepts: Option<Vec<String>>,
}
impl LearningPreferences {
    fn overlay(&mut self, other: &Self) {
        macro_rules! apply { ($($field:ident),+) => { $(if other.$field.is_some() { self.$field = other.$field.clone(); })+ }; }
        apply!(
            language_help,
            focus,
            detail,
            known_languages,
            learning_languages,
            known_frameworks,
            learning_frameworks,
            known_domains,
            learning_domains,
            known_concepts,
            learning_concepts
        );
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LearningProfile {
    pub preferences: LearningPreferences,
    pub languages: BTreeMap<String, LearningPreferences>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LearningConfig {
    pub defaults: LearningPreferences,
    pub profile: Option<String>,
    pub profiles: BTreeMap<String, LearningProfile>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RepositoryLearningConfig {
    pub defaults: LearningPreferences,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LearningSelection {
    pub profile: Option<String>,
    pub clear_profile: bool,
    pub use_repository_defaults: bool,
    pub session: LearningPreferences,
    pub overrides: LearningPreferences,
}
impl LearningSelection {
    pub fn selected_profile<'a>(&'a self, saved: &'a Option<String>) -> Result<Option<&'a str>> {
        if self.clear_profile && self.profile.is_some() {
            bail!("cannot select and clear a learning profile together");
        }
        Ok(if self.clear_profile {
            None
        } else {
            self.profile.as_deref().or(saved.as_deref())
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedExplanationPolicy {
    pub language: String,
    pub language_help: LanguageHelp,
    pub focus: Focus,
    pub detail: Detail,
    pub known_languages: Vec<String>,
    pub learning_languages: Vec<String>,
    pub known_frameworks: Vec<String>,
    pub learning_frameworks: Vec<String>,
    pub known_domains: Vec<String>,
    pub learning_domains: Vec<String>,
    pub known_concepts: Vec<String>,
    pub learning_concepts: Vec<String>,
}
impl Default for ResolvedExplanationPolicy {
    fn default() -> Self {
        Self {
            language: String::new(),
            language_help: LanguageHelp::Brief,
            focus: Focus::Balanced,
            detail: Detail::Balanced,
            known_languages: vec![],
            learning_languages: vec![],
            known_frameworks: vec![],
            learning_frameworks: vec![],
            known_domains: vec![],
            learning_domains: vec![],
            known_concepts: vec![],
            learning_concepts: vec![],
        }
    }
}
impl ResolvedExplanationPolicy {
    pub fn id(&self) -> String {
        hash(&serde_json::to_vec(self).expect("policy serialization"))
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn normalize_language(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "rs" => "rust".into(),
        "py" => "python".into(),
        "js" => "javascript".into(),
        "ts" => "typescript".into(),
        "c#" | "csharp" => "c_sharp".into(),
        other => other.into(),
    }
}
fn canonical(values: Option<Vec<String>>, languages: bool) -> Vec<String> {
    let mut values: Vec<_> = values
        .unwrap_or_default()
        .iter()
        .map(|v| {
            if languages {
                normalize_language(v)
            } else {
                v.trim().to_lowercase()
            }
        })
        .filter(|v| !v.is_empty())
        .collect();
    values.sort();
    values.dedup();
    values
}
#[derive(Clone, Debug, PartialEq)]
pub struct LearningContext {
    pub config: LearningConfig,
    pub repository_defaults: Option<LearningPreferences>,
    pub reader: ReaderConfig,
}
impl LearningContext {
    pub fn load(paths: &ConfigPaths, _reader: &ReaderConfig) -> Result<Self> {
        let config = LearningStore::new(paths.user.clone()).inspect()?.config;
        let repository_defaults = if let Some(path) = &paths.repository {
            let root = read_root(path)?;
            root.get("learning")
                .map(|v| {
                    v.clone()
                        .try_into::<RepositoryLearningConfig>()
                        .map(|v| v.defaults)
                })
                .transpose()?
        } else {
            None
        };
        Ok(Self {
            config,
            repository_defaults,
            reader: _reader.clone(),
        })
    }
    /// Presentation metadata deliberately excluded from policy identity.
    pub fn provenance(
        &self,
        language: &str,
        selection: &LearningSelection,
    ) -> Result<BTreeMap<String, String>> {
        let mut sources = BTreeMap::new();
        for field in [
            "language_help",
            "focus",
            "detail",
            "known_languages",
            "learning_languages",
            "known_frameworks",
            "learning_frameworks",
            "known_domains",
            "learning_domains",
            "known_concepts",
            "learning_concepts",
        ] {
            sources.insert(field.to_owned(), "application defaults".to_owned());
        }
        fn apply(
            sources: &mut BTreeMap<String, String>,
            p: &LearningPreferences,
            label: &str,
        ) -> Result<()> {
            for (key, value) in serde_json::to_value(p)?
                .as_object()
                .expect("preferences object")
            {
                if !value.is_null() {
                    sources.insert(key.clone(), label.to_owned());
                }
            }
            Ok(())
        }
        apply(&mut sources, &self.config.defaults, "user defaults")?;
        if selection.use_repository_defaults {
            if let Some(p) = &self.repository_defaults {
                apply(&mut sources, p, "repository defaults")?;
            }
        }
        if let Some(name) = selection.selected_profile(&self.config.profile)? {
            let profile = self
                .config
                .profiles
                .get(name)
                .with_context(|| format!("unknown learning profile '{name}'"))?;
            apply(&mut sources, &profile.preferences, "learning profile")?;
            for (key, p) in &profile.languages {
                if normalize_language(key) == normalize_language(language) {
                    apply(&mut sources, p, "language preferences")?;
                }
            }
        }
        apply(&mut sources, &selection.session, "session preferences")?;
        apply(&mut sources, &selection.overrides, "current view")?;
        Ok(sources)
    }
    pub fn resolve(
        &self,
        language: &str,
        selection: &LearningSelection,
    ) -> Result<ResolvedExplanationPolicy> {
        let language = normalize_language(language);
        let mut p = LearningPreferences::default();
        p.overlay(&self.config.defaults);
        if selection.use_repository_defaults {
            if let Some(repo) = &self.repository_defaults {
                p.overlay(repo);
            }
        }
        if let Some(name) = selection.selected_profile(&self.config.profile)? {
            let profile = self
                .config
                .profiles
                .get(name)
                .with_context(|| format!("unknown learning profile '{name}'"))?;
            p.overlay(&profile.preferences);
            let mut seen = std::collections::BTreeSet::new();
            for (key, values) in &profile.languages {
                let key = normalize_language(key);
                if !seen.insert(key.clone()) {
                    bail!("duplicate normalized language '{key}'");
                }
                if key == language {
                    p.overlay(values);
                }
            }
        }
        p.overlay(&selection.session);
        p.overlay(&selection.overrides);
        Ok(ResolvedExplanationPolicy {
            language: language.clone(),
            language_help: match p.language_help.unwrap_or(LanguageHelp::Automatic) {
                LanguageHelp::Automatic => {
                    if canonical(p.learning_languages.clone(), true).contains(&language) {
                        LanguageHelp::Explain
                    } else if canonical(p.known_languages.clone(), true).contains(&language) {
                        LanguageHelp::Skip
                    } else {
                        LanguageHelp::Brief
                    }
                }
                other => other,
            },
            focus: p.focus.unwrap_or(Focus::Balanced),
            detail: p.detail.unwrap_or(Detail::Balanced),
            known_languages: canonical(p.known_languages, true),
            learning_languages: canonical(p.learning_languages, true),
            known_frameworks: canonical(p.known_frameworks, false),
            learning_frameworks: canonical(p.learning_frameworks, false),
            known_domains: canonical(p.known_domains, false),
            learning_domains: canonical(p.learning_domains, false),
            known_concepts: canonical(p.known_concepts, false),
            learning_concepts: canonical(p.learning_concepts, false),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LearningInspection {
    pub revision: String,
    pub config: LearningConfig,
}
#[derive(Debug, PartialEq, Eq)]
pub enum LearningSaveConflict {
    Busy,
    Changed,
}

impl std::fmt::Display for LearningSaveConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => write!(
                f,
                "learning preferences are being edited; retry after the other save completes"
            ),
            Self::Changed => write!(f, "learning preferences changed; reload before saving"),
        }
    }
}

impl std::error::Error for LearningSaveConflict {}

pub struct LearningStore {
    path: PathBuf,
}
fn read_root(path: &std::path::Path) -> Result<toml::Value> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let root: toml::Value = toml::from_str(&text)
                .with_context(|| format!("invalid configuration {}", path.display()))?;
            if root.get("reader").is_some() {
                bail!("[reader] is unsupported; use [learning.defaults] or git explain learning to customize explanations");
            }
            Ok(root)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(toml::Value::Table(Default::default()))
        }
        Err(e) => Err(e.into()),
    }
}
impl LearningConfig {
    pub fn validate(&self) -> Result<()> {
        if let Some(name) = &self.profile {
            if !self.profiles.contains_key(name) {
                bail!("unknown learning profile '{name}'");
            }
        }
        for (name, profile) in &self.profiles {
            if name.trim().is_empty() {
                bail!("learning profile name cannot be empty");
            }
            let mut seen = std::collections::BTreeSet::new();
            for language in profile.languages.keys() {
                let normalized = normalize_language(language);
                if normalized.is_empty() || !seen.insert(normalized.clone()) {
                    bail!(
                        "invalid or duplicate language '{normalized}' in learning profile '{name}'"
                    );
                }
            }
        }
        Ok(())
    }
}
impl LearningStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn inspect(&self) -> Result<LearningInspection> {
        let root = read_root(&self.path)?;
        let config: LearningConfig = root
            .get("learning")
            .map(|v| v.clone().try_into())
            .transpose()?
            .unwrap_or_default();
        config.validate()?;
        Ok(LearningInspection {
            revision: hash(&serde_json::to_vec(&root.get("learning"))?),
            config,
        })
    }
    pub fn save(
        &self,
        expected_revision: &str,
        config: LearningConfig,
    ) -> Result<LearningInspection> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let lock_path = self.path.with_extension("learning.lock");
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        match lock.try_lock_exclusive() {
            Ok(()) => {}
            Err(err) => {
                let is_contended = err.kind() == std::io::ErrorKind::WouldBlock
                    || (err.raw_os_error().is_some()
                        && err.raw_os_error() == fs2::lock_contended_error().raw_os_error());
                if is_contended {
                    return Err(LearningSaveConflict::Busy.into());
                } else {
                    return Err(err).context("could not lock learning preferences");
                }
            }
        }
        let _lock = lock;
        if self.inspect()?.revision != expected_revision {
            return Err(LearningSaveConflict::Changed.into());
        }
        config.validate()?;
        let mut root = read_root(&self.path)?;
        root.as_table_mut()
            .context("configuration must be a table")?
            .insert("learning".into(), toml::Value::try_from(config)?);
        let temp = self
            .path
            .with_extension(format!("learning-{}.tmp", std::process::id()));
        fs::write(&temp, toml::to_string_pretty(&root)?)?;
        fs::rename(&temp, &self.path).context("could not atomically save learning preferences")?;
        self.inspect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> LearningContext {
        LearningContext {
            config: LearningConfig {
                defaults: LearningPreferences {
                    known_languages: Some(vec![" Python ".into(), "python".into()]),
                    ..Default::default()
                },
                ..Default::default()
            },
            repository_defaults: None,
            reader: ReaderConfig {
                experience: "expert".into(),
                known_languages: vec![" Python ".into(), "python".into()],
                learning_languages: vec![],
                known_frameworks: vec![],
                learning_frameworks: vec![],
            },
        }
    }
    #[test]
    fn canonical_identity_and_experience_is_advisory() {
        let c = context();
        let p = c.resolve("RS", &Default::default()).unwrap();
        assert_eq!(p.language_help, LanguageHelp::Brief);
        assert_eq!(p.known_languages, vec!["python"]);
        assert_eq!(p.id(), c.resolve("rust", &Default::default()).unwrap().id());
    }
    #[test]
    fn repository_requires_consent_and_overrides_win() {
        let mut c = context();
        c.repository_defaults = Some(LearningPreferences {
            detail: Some(Detail::Detailed),
            ..Default::default()
        });
        assert_eq!(
            c.resolve("rust", &Default::default()).unwrap().detail,
            Detail::Balanced
        );
        let mut s = LearningSelection {
            use_repository_defaults: true,
            ..Default::default()
        };
        assert_eq!(c.resolve("rust", &s).unwrap().detail, Detail::Detailed);
        s.overrides.detail = Some(Detail::Balanced);
        assert_eq!(c.resolve("rust", &s).unwrap().detail, Detail::Balanced);
    }
    #[test]
    fn store_preserves_unrelated_and_rejects_stale_save() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        fs::write(&p, "[model]\nprofile = 'local'\n").unwrap();
        let store = LearningStore::new(p.clone());
        let initial = store.inspect().unwrap();
        let mut config = initial.config;
        config.defaults.focus = Some(Focus::Execution);
        store.save(&initial.revision, config.clone()).unwrap();
        assert!(store.save(&initial.revision, config).is_err());
        assert!(fs::read_to_string(p).unwrap().contains("local"));
    }
    #[test]
    fn unknown_fields_are_rejected() {
        assert!(toml::from_str::<LearningConfig>("consent = true").is_err());
        assert!(toml::from_str::<RepositoryLearningConfig>("profile='x'").is_err());
    }
    #[test]
    fn language_profile_session_and_current_precedence() {
        let mut c = context();
        let mut profile = LearningProfile::default();
        profile.preferences.focus = Some(Focus::Syntax);
        profile.languages.insert(
            "RS".into(),
            LearningPreferences {
                focus: Some(Focus::Execution),
                ..Default::default()
            },
        );
        c.config.profiles.insert("rust learner".into(), profile);
        c.config.profile = Some("rust learner".into());
        let mut selection = LearningSelection::default();
        assert_eq!(
            c.resolve("rust", &selection).unwrap().focus,
            Focus::Execution
        );
        selection.session.focus = Some(Focus::Reasoning);
        assert_eq!(
            c.resolve("rust", &selection).unwrap().focus,
            Focus::Reasoning
        );
        selection.overrides.focus = Some(Focus::Architecture);
        assert_eq!(
            c.resolve("rust", &selection).unwrap().focus,
            Focus::Architecture
        );
        assert_eq!(
            c.provenance("rust", &selection).unwrap()["focus"],
            "current view"
        );
    }
    #[test]
    fn automatic_and_explicit_effective_preferences_share_identity() {
        let c = context();
        let mut selection = LearningSelection::default();
        let automatic = c.resolve("python", &selection).unwrap();
        assert_eq!(automatic.language_help, LanguageHelp::Skip);
        selection.overrides.language_help = Some(LanguageHelp::Skip);
        assert_eq!(automatic.id(), c.resolve("py", &selection).unwrap().id());
        selection.session.learning_languages = Some(vec!["Python".into()]);
        selection.overrides.language_help = None;
        assert_eq!(
            c.resolve("python", &selection).unwrap().language_help,
            LanguageHelp::Explain
        );
    }
    #[test]
    fn every_layer_overrides_only_fields_it_supplies() {
        let mut c = context();
        c.config.defaults = LearningPreferences {
            focus: Some(Focus::Syntax),
            known_domains: Some(vec!["HTTP".into()]),
            ..Default::default()
        };
        let mut selected = LearningSelection::default();
        assert_eq!(c.resolve("rust", &selected).unwrap().focus, Focus::Syntax);
        c.repository_defaults = Some(LearningPreferences {
            focus: Some(Focus::Execution),
            ..Default::default()
        });
        assert_eq!(c.resolve("rust", &selected).unwrap().focus, Focus::Syntax);
        selected.use_repository_defaults = true;
        assert_eq!(
            c.resolve("rust", &selected).unwrap().focus,
            Focus::Execution
        );
        let mut profile = LearningProfile {
            preferences: LearningPreferences {
                focus: Some(Focus::Reasoning),
                ..Default::default()
            },
            ..Default::default()
        };
        c.config.profiles.insert("p".into(), profile.clone());
        selected.profile = Some("p".into());
        assert_eq!(
            c.resolve("rust", &selected).unwrap().focus,
            Focus::Reasoning
        );
        profile.languages.insert(
            "rs".into(),
            LearningPreferences {
                focus: Some(Focus::Architecture),
                ..Default::default()
            },
        );
        c.config.profiles.insert("p".into(), profile);
        assert_eq!(
            c.resolve("rust", &selected).unwrap().focus,
            Focus::Architecture
        );
        selected.session.focus = Some(Focus::Balanced);
        assert_eq!(c.resolve("rust", &selected).unwrap().focus, Focus::Balanced);
        selected.overrides.focus = Some(Focus::Syntax);
        selected.overrides.known_languages = Some(vec![]);
        let policy = c.resolve("rust", &selected).unwrap();
        assert_eq!(policy.focus, Focus::Syntax);
        assert_eq!(policy.known_domains, vec!["http"]);
        assert!(policy.known_languages.is_empty());
    }
    #[test]
    fn same_effective_policy_has_same_identity_despite_provenance() {
        let mut c = context();
        c.config.defaults.focus = Some(Focus::Execution);
        c.config.defaults.known_concepts =
            Some(vec![" Mutex ".into(), "await".into(), "mutex".into()]);
        let first = c.resolve("RS", &Default::default()).unwrap();
        let mut selected = LearningSelection::default();
        selected.overrides.focus = Some(Focus::Execution);
        selected.overrides.known_concepts = Some(vec!["AWAIT".into(), "mutex".into()]);
        assert_eq!(first.id(), c.resolve("rust", &selected).unwrap().id());
        assert_ne!(
            c.provenance("rust", &Default::default()).unwrap(),
            c.provenance("rust", &selected).unwrap()
        );
    }
    #[test]
    fn saved_profile_can_be_explicitly_deselected() {
        let mut c = context();
        c.config.profiles.insert(
            "p".into(),
            LearningProfile {
                preferences: LearningPreferences {
                    detail: Some(Detail::Detailed),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        c.config.profile = Some("p".into());
        assert_eq!(
            c.resolve("rust", &Default::default()).unwrap().detail,
            Detail::Detailed
        );
        let mut selected = LearningSelection {
            clear_profile: true,
            ..Default::default()
        };
        assert_eq!(
            c.resolve("rust", &selected).unwrap().detail,
            Detail::Balanced
        );
        selected.profile = Some("p".into());
        assert!(c.resolve("rust", &selected).is_err());
    }
    #[test]
    fn load_uses_learning_defaults_and_repository_requires_consent() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user.toml");
        let repo = dir.path().join("repo.toml");
        fs::write(&user, "[learning.defaults]\nknown_languages=['Python']\n").unwrap();
        fs::write(
            &repo,
            "[learning.defaults]\nlearning_languages=['Rust']\nfocus='execution'\n",
        )
        .unwrap();
        let c = LearningContext::load(
            &ConfigPaths {
                user,
                repository: Some(repo.clone()),
            },
            &context().reader,
        )
        .unwrap();
        let p = c.resolve("rust", &Default::default()).unwrap();
        assert_eq!(p.language_help, LanguageHelp::Brief);
        assert_eq!(p.known_languages, vec!["python"]);
        let selected = LearningSelection {
            use_repository_defaults: true,
            ..Default::default()
        };
        assert_eq!(
            c.resolve("rust", &selected).unwrap().language_help,
            LanguageHelp::Explain
        );
        fs::write(repo, "[learning.profiles.bad]\n").unwrap();
        assert!(LearningContext::load(
            &ConfigPaths {
                user: dir.path().join("user.toml"),
                repository: Some(dir.path().join("repo.toml"))
            },
            &context().reader
        )
        .is_err());
    }
    #[test]
    fn concurrent_saves_have_one_winner_and_leave_valid_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("user.toml");
        fs::write(&path, "[model]\nprofile='private-local'\n").unwrap();
        let initial = LearningStore::new(path.clone()).inspect().unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut threads = Vec::new();
        for focus in [Focus::Execution, Focus::Syntax] {
            let path = path.clone();
            let revision = initial.revision.clone();
            let barrier = barrier.clone();
            threads.push(std::thread::spawn(move || {
                let mut config = LearningConfig::default();
                config.defaults.focus = Some(focus);
                barrier.wait();
                LearningStore::new(path).save(&revision, config).is_ok()
            }));
        }
        barrier.wait();
        assert_eq!(
            threads
                .into_iter()
                .filter_map(|t| t.join().ok())
                .filter(|ok| *ok)
                .count(),
            1
        );
        assert!(LearningStore::new(path.clone())
            .inspect()
            .unwrap()
            .config
            .defaults
            .focus
            .is_some());
        assert_eq!(
            read_root(&path).unwrap()["model"]["profile"].as_str(),
            Some("private-local")
        );
    }
    #[test]
    fn malformed_or_missing_profiles_never_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("user.toml");
        let store = LearningStore::new(path.clone());
        let before = store.inspect().unwrap();
        let mut config = LearningConfig {
            profile: Some("missing".into()),
            ..Default::default()
        };
        assert!(store.save(&before.revision, config.clone()).is_err());
        assert!(!path.exists());
        config.profile = None;
        let mut profile = LearningProfile::default();
        profile.languages.insert("rs".into(), Default::default());
        profile.languages.insert("Rust".into(), Default::default());
        config.profiles.insert("p".into(), profile);
        assert!(store.save(&before.revision, config).is_err());
        assert!(!path.exists());
        fs::write(&path, "[learning]\nprofile='missing'\n").unwrap();
        assert!(store.inspect().is_err());
        fs::write(&path, "[learning.defaults]\nfocus='invalid'\n").unwrap();
        assert!(store.inspect().is_err());
    }
    #[test]
    fn legacy_constructor_reader_does_not_affect_policy_or_identity() {
        let mut c = context();
        c.config = LearningConfig::default();
        let before = c.resolve("rust", &Default::default()).unwrap();
        c.reader.experience = "beginner".into();
        c.reader.known_languages = vec!["Rust".into()];
        c.reader.learning_languages = vec!["Rust".into()];
        assert_eq!(before, c.resolve("rust", &Default::default()).unwrap());
        assert_eq!(
            before,
            ResolvedExplanationPolicy {
                language: "rust".into(),
                ..Default::default()
            }
        );
    }
    #[test]
    fn old_reader_configuration_is_rejected_with_learning_guidance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[reader]\nknown_languages=['Rust']\n").unwrap();
        let error = LearningStore::new(path).inspect().unwrap_err().to_string();
        assert!(error.contains("[learning.defaults]"));
    }
}
