use crate::ContextError;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOptions {
    pub max_entries: usize,
    pub max_files: usize,
    pub max_depth: usize,
    pub max_file_read_bytes: u64,
    pub max_git_output_bytes: usize,
    pub git_timeout: Duration,
    pub recent_commit_limit: usize,
}

impl ScanOptions {
    pub(crate) fn validate(&self) -> Result<(), ContextError> {
        if self.max_entries == 0 {
            return Err(ContextError::InvalidOptions(
                "max_entries must be greater than zero".to_string(),
            ));
        }
        if self.max_files == 0 {
            return Err(ContextError::InvalidOptions(
                "max_files must be greater than zero".to_string(),
            ));
        }
        if self.max_depth == 0 {
            return Err(ContextError::InvalidOptions(
                "max_depth must be greater than zero".to_string(),
            ));
        }
        if self.max_file_read_bytes == 0 {
            return Err(ContextError::InvalidOptions(
                "max_file_read_bytes must be greater than zero".to_string(),
            ));
        }
        if self.max_git_output_bytes == 0 {
            return Err(ContextError::InvalidOptions(
                "max_git_output_bytes must be greater than zero".to_string(),
            ));
        }
        if self.git_timeout.is_zero() {
            return Err(ContextError::InvalidOptions(
                "git_timeout must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_entries: 200_000,
            max_files: 100_000,
            max_depth: 64,
            max_file_read_bytes: 1_048_576,
            max_git_output_bytes: 1_048_576,
            git_timeout: Duration::from_secs(2),
            recent_commit_limit: 5,
        }
    }
}
