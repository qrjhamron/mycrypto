//! Multi-agent team discussion engine.

pub mod orchestrator;
pub mod roles;

pub use orchestrator::{current_team_session_id, run_team_discussion};
pub use roles::{hardcoded_roles, AgentRole};
