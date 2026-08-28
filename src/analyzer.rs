use crate::{
    change_set::{ChangeSet, FileChange, SourceFile},
    config::{GitConfig, ResolvedConfig},
    explain::{
        AnalysisContext, AnalysisMode, GitCommitSourceProvider, GitIndexSourceProvider,
        SourceProvider, WorkingTreeSourceProvider,
    },
    file_analysis::FileAnalysis,
    git, semantic,
    snapshot::{working_tree_fingerprint, SnapshotGeneration, SnapshotIdentity},
};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct RepositoryAnalyzer {
    repo_root: PathBuf,
    git_config: GitConfig,
}
impl RepositoryAnalyzer {
    pub fn analyze_files(&self, revision: Option<&str>, paths: &[PathBuf]) -> Result<FileAnalysis> {
        let resolved_revision = revision
            .map(|value| git::resolve_revision(&self.repo_root, value))
            .transpose()?;
        let provider: Box<dyn SourceProvider> = match &resolved_revision {
            Some(oid) => Box::new(GitCommitSourceProvider::new(&self.repo_root, oid)),
            None => Box::new(WorkingTreeSourceProvider::new(&self.repo_root)),
        };
        let selected_files =
            git::files_at_revision(&self.repo_root, resolved_revision.as_deref(), paths)?;
        let mut documents = Vec::with_capacity(selected_files.len());
        for path in &selected_files {
            let source = SourceFile {
                path: path.clone(),
                content: provider
                    .read_file(path)
                    .with_context(|| format!("read selected file {}", path.display()))?,
            };
            documents.push(semantic::extract_document(&source)?);
        }
        Ok(FileAnalysis {
            revision: resolved_revision,
            documents,
            explanations: Default::default(),
            deep_explanations: Default::default(),
        })
    }
    pub fn new(repo_root: impl Into<PathBuf>, config: ResolvedConfig) -> Self {
        Self {
            repo_root: repo_root.into(),
            git_config: config.git,
        }
    }
    pub fn with_git_config(repo_root: impl Into<PathBuf>, git_config: GitConfig) -> Self {
        Self {
            repo_root: repo_root.into(),
            git_config,
        }
    }

    pub fn analyze_working_tree(&self, generation: SnapshotGeneration) -> Result<ChangeSet> {
        self.analyze_working_tree_with_paths(generation, &[])
    }

    pub fn analyze_working_tree_with_paths(
        &self,
        generation: SnapshotGeneration,
        paths: &[PathBuf],
    ) -> Result<ChangeSet> {
        let mut files = git::working_tree_files(&self.repo_root, &self.git_config)?;
        let after = WorkingTreeSourceProvider::new(&self.repo_root);
        let before: Box<dyn SourceProvider> = if self.git_config.include_staged {
            Box::new(GitCommitSourceProvider::new(
                &self.repo_root,
                self.git_config.diff_target.clone(),
            ))
        } else {
            Box::new(GitIndexSourceProvider::new(&self.repo_root))
        };
        filter_files(&mut files, paths);
        populate_semantics(&mut files, before.as_ref(), &after);
        let head = git::head_oid(&self.repo_root).unwrap_or_default();
        Ok(ChangeSet {
            generation,
            identity: SnapshotIdentity::WorkingTree {
                fingerprint: working_tree_fingerprint(&head, &self.git_config.diff_target, &files),
            },
            context: if paths.is_empty() {
                AnalysisContext::working_tree()
            } else {
                AnalysisContext {
                    mode: AnalysisMode::WorkingTree,
                    path_filters: paths.to_vec(),
                }
            },
            files,
        })
    }

    pub fn analyze_staged(
        &self,
        generation: SnapshotGeneration,
        paths: &[PathBuf],
    ) -> Result<ChangeSet> {
        let mut files = git::staged_files(&self.repo_root, &self.git_config)?;
        filter_files(&mut files, paths);
        let before =
            GitCommitSourceProvider::new(&self.repo_root, self.git_config.diff_target.clone());
        let after = GitIndexSourceProvider::new(&self.repo_root);
        populate_semantics(&mut files, &before, &after);
        let head = git::head_oid(&self.repo_root).unwrap_or_default();
        Ok(ChangeSet {
            generation,
            identity: SnapshotIdentity::WorkingTree {
                fingerprint: working_tree_fingerprint(&head, "staged", &files),
            },
            context: AnalysisContext {
                mode: AnalysisMode::WorkingTree,
                path_filters: paths.to_vec(),
            },
            files,
        })
    }

    pub fn analyze_commit(
        &self,
        revision: &str,
        generation: SnapshotGeneration,
    ) -> Result<ChangeSet> {
        self.analyze_commit_with_paths(revision, generation, &[])
    }

    pub fn analyze_commit_with_paths(
        &self,
        revision: &str,
        generation: SnapshotGeneration,
        paths: &[PathBuf],
    ) -> Result<ChangeSet> {
        let analysis = git::commit_analysis(&self.repo_root, revision)?;
        let mut files = analysis.files;
        filter_files(&mut files, paths);
        let after = GitCommitSourceProvider::new(&self.repo_root, analysis.oid.clone());
        let before: Box<dyn SourceProvider> = match &analysis.parent_oid {
            Some(parent) => Box::new(GitCommitSourceProvider::new(&self.repo_root, parent)),
            None => Box::new(EmptySourceProvider),
        };
        populate_semantics(&mut files, before.as_ref(), &after);
        Ok(ChangeSet {
            generation,
            identity: SnapshotIdentity::Commit {
                oid: analysis.oid.clone(),
            },
            context: AnalysisContext {
                mode: AnalysisMode::Commit {
                    oid: analysis.oid,
                    parent_oid: analysis.parent_oid,
                    subject: analysis.subject,
                    merge_parent_count: analysis.parent_count,
                },
                path_filters: paths.to_vec(),
            },
            files,
        })
    }

    pub fn analyze_range(
        &self,
        expression: &str,
        generation: SnapshotGeneration,
        paths: &[PathBuf],
    ) -> Result<ChangeSet> {
        let analysis = git::range_analysis(&self.repo_root, expression)?;
        let mut files = analysis.files;
        filter_files(&mut files, paths);
        let before = GitCommitSourceProvider::new(&self.repo_root, &analysis.from_oid);
        let after = GitCommitSourceProvider::new(&self.repo_root, &analysis.to_oid);
        populate_semantics(&mut files, &before, &after);
        Ok(ChangeSet {
            generation,
            identity: SnapshotIdentity::Range {
                from_oid: analysis.from_oid.clone(),
                to_oid: analysis.to_oid.clone(),
            },
            context: AnalysisContext {
                mode: AnalysisMode::Range {
                    expression: analysis.expression,
                    from_oid: analysis.from_oid,
                    to_oid: analysis.to_oid,
                },
                path_filters: paths.to_vec(),
            },
            files,
        })
    }

    pub fn analyze_revision_or_range(
        &self,
        revision: &str,
        generation: SnapshotGeneration,
        paths: &[PathBuf],
    ) -> Result<ChangeSet> {
        if revision.contains("..") {
            self.analyze_range(revision, generation, paths)
        } else if paths.is_empty() {
            self.analyze_commit(revision, generation)
        } else {
            self.analyze_commit_with_paths(revision, generation, paths)
        }
    }
}

fn filter_files(files: &mut Vec<FileChange>, paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }
    files.retain(|file| {
        paths.iter().any(|filter| {
            file.old_path
                .as_ref()
                .is_some_and(|path| path_matches(path, filter))
                || file
                    .new_path
                    .as_ref()
                    .is_some_and(|path| path_matches(path, filter))
        })
    });
}

fn path_matches(path: &Path, filter: &Path) -> bool {
    path == filter || path.starts_with(filter)
}

fn populate_semantics(
    files: &mut [FileChange],
    before: &dyn SourceProvider,
    after: &dyn SourceProvider,
) {
    for file in files {
        if file.binary {
            continue;
        }
        file.before = file
            .old_path
            .as_ref()
            .and_then(|path| read_source(before, path));
        file.after = file
            .new_path
            .as_ref()
            .and_then(|path| read_source(after, path));
        let before_document = file
            .before
            .as_ref()
            .map(semantic::extract_document)
            .transpose()
            .unwrap_or_else(|error| {
                eprintln!(
                    "{}: semantic extraction failed: {error:#}",
                    file.display_path().display()
                );
                None
            });
        let after_document = file
            .after
            .as_ref()
            .map(semantic::extract_document)
            .transpose()
            .unwrap_or_else(|error| {
                eprintln!(
                    "{}: semantic extraction failed: {error:#}",
                    file.display_path().display()
                );
                None
            });
        file.semantic_changes = semantic::match_documents(
            before_document.as_ref(),
            after_document.as_ref(),
            file.patch
                .as_ref()
                .map(|patch| patch.text.clone())
                .unwrap_or_default(),
        );
    }
}
fn read_source(provider: &dyn SourceProvider, path: &Path) -> Option<SourceFile> {
    provider.read_file(path).ok().map(|content| SourceFile {
        path: path.into(),
        content,
    })
}
struct EmptySourceProvider;
impl SourceProvider for EmptySourceProvider {
    fn read_file(&self, _: &Path) -> Result<String> {
        anyhow::bail!("empty tree has no source")
    }
}
