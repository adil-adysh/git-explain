//! Shared read-only transport projection. These operations never resolve learning
//! preferences, restore explanations, or invoke a model.
use crate::workspace::{Workspace, WorkspaceError};
use axum::{http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn repository_label(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Git repository".into())
}

#[derive(Deserialize)]
pub struct SectionQuery {
    pub snapshot_id: Option<String>,
}

pub fn error(
    status: StatusCode,
    code: &str,
    message: &str,
    retryable: bool,
) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({"ok": false, "code": code, "error": message, "retryable": retryable})),
    )
}

pub fn summary(
    workspace: &Workspace,
    session: Option<&str>,
    model: &crate::config::ResolvedProfile,
    repository: &str,
) -> Value {
    let index = workspace.index();
    json!({
        "schema_version": index.schema_version,
        "mode": index.mode,
        "session_id": session,
        "generation": index.generation,
        "snapshot_id": index.snapshot_id,
        "revision_label": index.revision_label,
        "capabilities": index.capabilities,
        "file_count": index.files.len(),
        "runtime": { "version": env!("CARGO_PKG_VERSION"), "model": model.model },
        "repository_label": repository,
    })
}

pub fn index(
    workspace: &Workspace,
    session: Option<&str>,
    model: &crate::config::ResolvedProfile,
    repository: &str,
) -> Value {
    let mut value = serde_json::to_value(workspace.index()).expect("serializable workspace index");
    value["session_id"] = json!(session);
    value["repository_label"] = json!(repository);
    value["runtime"] = json!({"version": env!("CARGO_PKG_VERSION"), "model": model.model});
    value
}

pub fn section(workspace: &Workspace, id: &str, query: SectionQuery) -> (StatusCode, Json<Value>) {
    let Some(snapshot) = query.snapshot_id else {
        return error(
            StatusCode::BAD_REQUEST,
            "snapshot_required",
            "Reload the file list before opening this code section.",
            true,
        );
    };
    match workspace.section(id, &snapshot) {
        Ok(detail) => (
            StatusCode::OK,
            Json(serde_json::to_value(detail).expect("serializable source detail")),
        ),
        Err(WorkspaceError::UnknownSection(_)) => error(
            StatusCode::NOT_FOUND,
            "section_not_found",
            "This code section is unavailable. Choose a section from the file list.",
            false,
        ),
        Err(WorkspaceError::StaleSnapshot { .. }) => {
            let (status, Json(mut value)) = error(
                StatusCode::CONFLICT,
                "source_changed",
                "The code changed. Reload the file list before continuing.",
                true,
            );
            value["snapshot_id"] = json!(workspace.index().snapshot_id);
            (status, Json(value))
        }
    }
}
