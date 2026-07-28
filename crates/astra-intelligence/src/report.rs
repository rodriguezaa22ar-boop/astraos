use crate::{Availability, ProjectIntelligence, VerificationValidity};

/// Renders the same typed model serialized by the JSON command.
pub fn render_text(report: &ProjectIntelligence) -> String {
    let mut output = String::from("Project Understanding Report\n\n");
    output.push_str("Identity\n");
    line(&mut output, "Name", &report.project.name);
    line(
        &mut output,
        "Type",
        &availability(&report.identity.project_type),
    );
    line(
        &mut output,
        "Packages",
        &report.identity.package_count.to_string(),
    );
    list(&mut output, "Languages", &report.identity.languages);
    list(&mut output, "Build systems", &report.identity.build_systems);
    line(
        &mut output,
        "Workspace",
        &availability(&report.identity.workspace),
    );

    output.push_str("\nArchitecture\n");
    line(
        &mut output,
        "Workspace detected",
        &availability(&report.architecture.workspace_detected),
    );
    line(
        &mut output,
        "Package structure",
        &availability(&report.architecture.package_structure),
    );

    output.push_str("\nCapabilities\n");
    list(
        &mut output,
        "Discovered",
        &report.capabilities.discovered_actions,
    );
    list(
        &mut output,
        "Controlled execution",
        &report.capabilities.controlled_execution_actions,
    );
    list(
        &mut output,
        "Dry-run only",
        &report.capabilities.dry_run_only_actions,
    );

    output.push_str("\nVerification\n");
    line(
        &mut output,
        "Availability",
        &availability(&report.verification.availability),
    );
    line(
        &mut output,
        "Latest action",
        &availability(&report.verification.latest_action),
    );
    line(
        &mut output,
        "Verdict",
        &availability(&report.verification.verdict),
    );
    line(
        &mut output,
        "Validity",
        &availability(&report.verification.validity),
    );

    output.push_str("\nKnowledge\n");
    line(&mut output, "Facts", &report.knowledge.facts.to_string());
    line(
        &mut output,
        "Decisions",
        &report.knowledge.decisions.to_string(),
    );
    line(
        &mut output,
        "Verifications",
        &report.knowledge.verifications.to_string(),
    );
    line(&mut output, "Goals", &report.knowledge.goals.to_string());

    section(
        &mut output,
        "Derived Insights",
        report.insights.iter().map(|insight| {
            format!(
                "[{}] {}\n  Confidence: {}",
                insight.rule_id, insight.statement, insight.confidence
            )
        }),
    );
    section(
        &mut output,
        "Risks",
        report.risks.iter().map(|risk| risk.statement.clone()),
    );
    section(
        &mut output,
        "Limitations",
        report
            .limitations
            .iter()
            .map(|limitation| limitation.statement.clone()),
    );
    output
}

fn line(output: &mut String, label: &str, value: &str) {
    output.push_str("  ");
    output.push_str(label);
    output.push_str(": ");
    output.push_str(value);
    output.push('\n');
}

fn list(output: &mut String, label: &str, values: &[String]) {
    output.push_str("  ");
    output.push_str(label);
    output.push_str(":\n");
    if values.is_empty() {
        output.push_str("    - unavailable\n");
    } else {
        for value in values {
            output.push_str("    - ");
            output.push_str(value);
            output.push('\n');
        }
    }
}

fn section<I>(output: &mut String, title: &str, values: I)
where
    I: IntoIterator<Item = String>,
{
    output.push('\n');
    output.push_str(title);
    output.push('\n');
    let values = values.into_iter().collect::<Vec<_>>();
    if values.is_empty() {
        output.push_str("  None detected from available evidence.\n");
    } else {
        for value in values {
            output.push_str("  - ");
            output.push_str(&value);
            output.push('\n');
        }
    }
}

fn availability<T: ToString>(value: &Availability<T>) -> String {
    match value {
        Availability::Available(value) => value.to_string(),
        Availability::Unavailable => "unavailable".to_string(),
        Availability::Unknown => "unknown".to_string(),
    }
}

impl std::fmt::Display for VerificationValidity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Current => "current",
            Self::Stale => "stale",
            Self::Invalidated => "invalidated",
            Self::Unknown => "unknown",
        })
    }
}
