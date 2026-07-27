mod model;
mod plan;
mod policy;
mod resolver;

/// Version of the executable action policy contract.
pub const ACTION_POLICY_VERSION: u32 = 1;

pub use model::{
    ActionId, ActionSource, CommandSpec, ProjectAction, ProjectActionReport, ProjectReference,
    PROJECT_ACTION_SCHEMA_VERSION,
};
pub use plan::{
    DryRunExecutionState, DryRunMode, DryRunReport, ExecutionPlan, ACTION_DRY_RUN_SCHEMA_VERSION,
};
pub use policy::{ActionPolicy, PolicyDecision, PolicyEvaluation, PolicyRejectionReason};
pub use resolver::{resolve_actions, select_action};
