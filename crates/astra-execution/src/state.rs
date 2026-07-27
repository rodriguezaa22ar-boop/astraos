use crate::fingerprint::{hash_fields, Fingerprint};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

/// Version of the serialized project source-state binding contract.
pub const STATE_FINGERPRINT_SCHEMA_VERSION: u32 = 1;

const MAX_UNTRACKED_FILE_BYTES: u64 = 1_048_576;
const MAX_UNTRACKED_TOTAL_BYTES: u64 = 16 * 1_048_576;
const MAX_UNTRACKED_FILES: usize = 1_000;
pub(crate) const MAX_GIT_DIFF_BYTES: usize = 64 * 1_048_576;
pub(crate) const MAX_GIT_ERROR_BYTES: usize = 64 * 1_024;
pub(crate) const MAX_GIT_COMMAND_DURATION: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StateCaptureLimits {
    pub(crate) max_untracked_file_bytes: u64,
    pub(crate) max_untracked_total_bytes: u64,
    pub(crate) max_untracked_files: usize,
    pub(crate) max_git_diff_bytes: usize,
    pub(crate) max_git_error_bytes: usize,
}

impl Default for StateCaptureLimits {
    fn default() -> Self {
        Self {
            max_untracked_file_bytes: MAX_UNTRACKED_FILE_BYTES,
            max_untracked_total_bytes: MAX_UNTRACKED_TOTAL_BYTES,
            max_untracked_files: MAX_UNTRACKED_FILES,
            max_git_diff_bytes: MAX_GIT_DIFF_BYTES,
            max_git_error_bytes: MAX_GIT_ERROR_BYTES,
        }
    }
}

/// The exact Git-relevant source state bound to an executable plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectStateBinding {
    pub schema_version: u32,
    pub canonical_root: PathBuf,
    pub repository_root: PathBuf,
    pub repository_head: String,
    pub index_fingerprint: Fingerprint,
    pub worktree_fingerprint: Fingerprint,
    pub untracked_fingerprint: Fingerprint,
    pub combined_fingerprint: Fingerprint,
}

impl ProjectStateBinding {
    pub(crate) fn new(
        canonical_root: PathBuf,
        repository_root: PathBuf,
        repository_head: String,
        index_fingerprint: Fingerprint,
        worktree_fingerprint: Fingerprint,
        untracked_fingerprint: Fingerprint,
    ) -> Self {
        let schema_version = STATE_FINGERPRINT_SCHEMA_VERSION.to_string();
        let canonical_root_text = canonical_root.to_string_lossy().into_owned();
        let repository_root_text = repository_root.to_string_lossy().into_owned();
        let combined_fingerprint = hash_fields(
            "astra-project-state-v1",
            &[
                ("schema_version", schema_version.as_bytes()),
                ("canonical_root", canonical_root_text.as_bytes()),
                ("repository_root", repository_root_text.as_bytes()),
                ("repository_head", repository_head.as_bytes()),
                ("index", index_fingerprint.as_str().as_bytes()),
                ("worktree", worktree_fingerprint.as_str().as_bytes()),
                ("untracked", untracked_fingerprint.as_str().as_bytes()),
            ],
        );

        Self {
            schema_version: STATE_FINGERPRINT_SCHEMA_VERSION,
            canonical_root,
            repository_root,
            repository_head,
            index_fingerprint,
            worktree_fingerprint,
            untracked_fingerprint,
            combined_fingerprint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fingerprint::hash_fields;
    use std::path::PathBuf;

    fn state(index: &str, worktree: &str, untracked: &str) -> ProjectStateBinding {
        ProjectStateBinding::new(
            PathBuf::from("/project"),
            PathBuf::from("/repo"),
            "0123456789abcdef".to_string(),
            hash_fields(index, &[("state", b"index")]),
            hash_fields(worktree, &[("state", b"worktree")]),
            hash_fields(untracked, &[("state", b"untracked")]),
        )
    }

    #[test]
    fn combined_state_fingerprint_is_deterministic() {
        assert_eq!(state("a", "b", "c"), state("a", "b", "c"));
        assert_ne!(state("a", "b", "c"), state("different", "b", "c"));
    }

    #[test]
    fn state_serialization_contains_fingerprints_but_not_source_content() {
        let state = state("a", "b", "c");
        let json = serde_json::to_string(&state).expect("state JSON");
        assert!(json.contains("combined_fingerprint"));
        assert!(!json.contains("Cargo.toml"));
        let restored: ProjectStateBinding = serde_json::from_str(&json).expect("state round trip");
        assert_eq!(restored, state);
    }
}
