use astra_actions::ActionId;

/// Read-only description of the current controlled-execution boundary.
///
/// This does not plan or launch a process. It lets integration layers describe
/// the boundary without duplicating the execution engine's allowlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlledExecutionCapability {
    Allowed,
    DryRunOnly,
}

pub fn controlled_execution_capability(action: ActionId) -> ControlledExecutionCapability {
    match action {
        ActionId::Check => ControlledExecutionCapability::Allowed,
        ActionId::Build | ActionId::Test => ControlledExecutionCapability::DryRunOnly,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_check_is_available_for_controlled_execution() {
        assert_eq!(
            controlled_execution_capability(ActionId::Check),
            ControlledExecutionCapability::Allowed
        );
        assert_eq!(
            controlled_execution_capability(ActionId::Build),
            ControlledExecutionCapability::DryRunOnly
        );
    }
}
