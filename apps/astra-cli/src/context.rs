use astra_context::{render_json, render_text, render_tree, ProjectAnalyzer};
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
    let report = ProjectAnalyzer::default()
        .analyze(path)
        .map_err(|error| error.to_string())?;
    let rendered = match format {
        OutputFormat::Text => render_text(&report),
        OutputFormat::Json => render_json(&report).map_err(|error| error.to_string())?,
        OutputFormat::Tree => render_tree(&report),
    };
    match io::stdout().lock().write_all(rendered.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(format!("failed to write context output: {error}")),
    }
}
