use crate::{Diagnostic, Insight, ProjectContext, ScannerResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Version of the serialized [`ScanReport`] contract.
///
/// Existing serialized fields must not be renamed or removed without
/// incrementing this value.
pub const PROJECT_CONTEXT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanReport {
    pub schema_version: u32,
    pub context: ProjectContext,
    pub scanners: Vec<ScannerResult>,
    pub diagnostics: Vec<Diagnostic>,
    pub insights: Vec<crate::Detected<Insight>>,

    #[serde(skip, default)]
    pub duration: Duration,
}

impl ScanReport {
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == crate::DiagnosticSeverity::Warning)
    }
}
