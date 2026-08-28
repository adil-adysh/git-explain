use crate::change_set::FileChange;
use serde::{Deserialize, Serialize};
use sha2::Digest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SnapshotGeneration(pub u64);
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SnapshotIdentity {
    WorkingTree { fingerprint: String },
    Commit { oid: String },
}
pub fn working_tree_fingerprint(head: &str, diff_target: &str, files: &[FileChange]) -> String {
    let mut input = format!("head={head}\ndiff_target={diff_target}\n");
    for file in files {
        input.push_str(&format!("old={:?}\nnew={:?}\nkind={:?}\nsimilarity={:?}\nbinary={}\nmode={:?}->{:?}\npatch={}\n", file.old_path, file.new_path, file.kind, file.similarity, file.binary, file.old_mode, file.new_mode, file.patch.as_ref().map_or("", |patch| patch.text.as_str())));
    }
    sha2::Sha256::digest(input.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
