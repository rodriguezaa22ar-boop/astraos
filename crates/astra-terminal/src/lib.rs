mod error;
mod plan;
mod process;
mod wezterm;

pub use error::TerminalError;
pub use plan::{
    build_launch_plan, describe_layout, LaunchPlan, MAX_SPLIT_PERCENT, MIN_SPLIT_PERCENT,
};
pub use wezterm::{launch, LaunchSummary};
