use astra_context::{
    render_json, render_text, render_tree, ProjectAnalyzer, ScanOptions, ScanReport,
};
use std::{
    io::{self, Write},
    path::Path,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Text,
    Json,
    Tree,
}

pub(crate) fn inspect(path: &Path, format: OutputFormat) -> Result<(), String> {
    let report = analyze(path)?;
    write_report(&report, format)
}

pub(crate) fn analyze(path: &Path) -> Result<ScanReport, String> {
    ProjectAnalyzer::default()
        .analyze(path)
        .map_err(|error| error.to_string())
}

pub(crate) fn analyze_without_processes(path: &Path) -> Result<ScanReport, String> {
    ProjectAnalyzer::without_processes(ScanOptions::default())
        .map_err(|error| error.to_string())?
        .analyze(path)
        .map_err(|error| error.to_string())
}

pub(crate) fn write_report(report: &ScanReport, format: OutputFormat) -> Result<(), String> {
    let rendered = match format {
        OutputFormat::Text => render_text(report),
        OutputFormat::Json => render_json(report).map_err(|error| error.to_string())?,
        OutputFormat::Tree => render_tree(report),
    };
    match io::stdout().lock().write_all(rendered.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(format!("failed to write context output: {error}")),
    }
}
