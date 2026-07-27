use crate::{
    facts::{CommandFact, Fact, FactKind, StoredFact},
    scanner::{detected_from_facts, metadata, ScannerInput, ScannerOutput},
    CommandPurpose, CommandSpec, Detected, ProjectPath,
};

pub(crate) struct CommandProjection {
    pub(crate) development: Vec<Detected<CommandSpec>>,
    pub(crate) validation: Vec<Detected<CommandSpec>>,
}

pub(crate) fn scan(input: &ScannerInput<'_>) -> ScannerOutput<CommandProjection> {
    let commands = input
        .facts()
        .primary_facts_of_kind(FactKind::Command)
        .into_iter()
        .filter_map(|stored| {
            let Fact::Command(command) = &stored.fact else {
                return None;
            };
            Some((stored, command))
        })
        .collect::<Vec<_>>();
    let consolidated = consolidate_workspace_commands(&commands);
    let mut development = Vec::new();
    let mut validation = Vec::new();
    for (index, (stored, command)) in commands.iter().enumerate() {
        if consolidated[index].is_some() {
            continue;
        }
        let evidence = std::iter::once(*stored)
            .chain(
                commands
                    .iter()
                    .zip(&consolidated)
                    .filter_map(|((candidate, _), owner)| {
                        (*owner == Some(index)).then_some(*candidate)
                    }),
            )
            .collect::<Vec<_>>();
        let detected = command_spec(command, &evidence);
        if command.purpose == CommandPurpose::Develop {
            development.push(detected);
        } else {
            validation.push(detected);
        }
    }
    sort_commands(&mut development);
    sort_commands(&mut validation);
    let findings = development.len() + validation.len();
    ScannerOutput::complete(
        metadata(
            "validation",
            1,
            "Projects argv-based development and validation commands without executing them",
        ),
        CommandProjection {
            development,
            validation,
        },
        findings,
    )
}

fn consolidate_workspace_commands(commands: &[(&StoredFact, &CommandFact)]) -> Vec<Option<usize>> {
    let workspaces = commands
        .iter()
        .enumerate()
        .filter(|(_, (_, command))| workspace_arguments(command).is_some())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut consolidated = vec![None; commands.len()];

    for (candidate_index, (_, candidate)) in commands.iter().enumerate() {
        if candidate.executable != "cargo" || workspace_arguments(candidate).is_some() {
            continue;
        }
        consolidated[candidate_index] = workspaces
            .iter()
            .copied()
            .filter(|workspace_index| {
                let workspace = commands[*workspace_index].1;
                workspace.purpose == candidate.purpose
                    && workspace.executable == candidate.executable
                    && workspace_arguments(workspace).as_deref()
                        == Some(candidate.arguments.as_slice())
                    && is_descendant(&workspace.working_directory, &candidate.working_directory)
            })
            .max_by_key(|workspace_index| commands[*workspace_index].1.working_directory.len());
    }

    consolidated
}

fn workspace_arguments(command: &CommandFact) -> Option<Vec<String>> {
    (command.executable == "cargo"
        && command
            .arguments
            .iter()
            .any(|argument| argument == "--workspace"))
    .then(|| {
        command
            .arguments
            .iter()
            .filter(|argument| argument.as_str() != "--workspace")
            .cloned()
            .collect()
    })
}

fn is_descendant(parent: &str, child: &str) -> bool {
    if parent == "." {
        return child != ".";
    }
    child
        .strip_prefix(parent)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn command_spec(command: &CommandFact, facts: &[&StoredFact]) -> Detected<CommandSpec> {
    detected_from_facts(
        CommandSpec {
            executable: command.executable.clone(),
            arguments: command.arguments.clone(),
            working_directory: ProjectPath(command.working_directory.clone()),
            purpose: command.purpose,
        },
        facts,
    )
}

fn sort_commands(commands: &mut [Detected<CommandSpec>]) {
    commands.sort_by(|left, right| {
        left.value
            .working_directory
            .cmp(&right.value.working_directory)
            .then_with(|| left.value.purpose.cmp(&right.value.purpose))
            .then_with(|| left.value.executable.cmp(&right.value.executable))
            .then_with(|| left.value.arguments.cmp(&right.value.arguments))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        facts::{FactGraphBuilder, FactProvenance},
        scope::SemanticScope,
        Confidence, Evidence, EvidenceSource,
    };

    fn add_command(
        builder: &mut FactGraphBuilder,
        arguments: &[&str],
        working_directory: &str,
        scope: SemanticScope,
    ) {
        builder.add_fact(
            Fact::Command(CommandFact {
                executable: "cargo".to_string(),
                arguments: arguments
                    .iter()
                    .map(|argument| argument.to_string())
                    .collect(),
                working_directory: working_directory.to_string(),
                purpose: CommandPurpose::Test,
                source_path: format!("{working_directory}/Cargo.toml"),
            }),
            FactProvenance {
                scanner: "test".to_string(),
                scope,
                confidence: Confidence::High,
                evidence: vec![Evidence {
                    source: EvidenceSource::Manifest,
                    path: Some(ProjectPath(format!("{working_directory}/Cargo.toml"))),
                    locator: None,
                    rule: "cargo.command".to_string(),
                }],
            },
        );
    }

    #[test]
    fn workspace_commands_replace_equivalent_package_commands_and_merge_evidence() {
        let mut builder = FactGraphBuilder::new();
        add_command(
            &mut builder,
            &["test", "--workspace"],
            ".",
            SemanticScope::Primary,
        );
        add_command(
            &mut builder,
            &["test"],
            "crates/app",
            SemanticScope::Primary,
        );
        add_command(
            &mut builder,
            &["test", "--all-features"],
            "crates/app",
            SemanticScope::Primary,
        );
        let graph = builder.finish().expect("graph");

        let output = scan(&ScannerInput::new(&graph));

        assert_eq!(output.value.validation.len(), 2);
        let workspace = output
            .value
            .validation
            .iter()
            .find(|command| command.value.arguments == ["test", "--workspace"])
            .expect("workspace command");
        assert_eq!(workspace.evidence.len(), 2);
        assert!(output.value.validation.iter().any(|command| {
            command.value.arguments == ["test", "--all-features"]
                && command.value.working_directory.as_str() == "crates/app"
        }));
        assert!(!output.value.validation.iter().any(|command| {
            command.value.arguments == ["test"]
                && command.value.working_directory.as_str() == "crates/app"
        }));
    }

    #[test]
    fn fixture_commands_are_not_project_validation_commands() {
        let mut builder = FactGraphBuilder::new();
        add_command(
            &mut builder,
            &["test"],
            "tests/fixtures/demo",
            SemanticScope::Fixture,
        );
        let graph = builder.finish().expect("graph");

        let output = scan(&ScannerInput::new(&graph));

        assert!(output.value.validation.is_empty());
    }
}
