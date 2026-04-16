//! Hardcoded team roles for multi-agent discussion.

use crate::state::TeamRole;

/// Contract for a team agent role.
pub trait AgentRole: Send + Sync {
    /// Role identifier.
    fn role(&self) -> TeamRole;

    /// Persona text used for UI/reference.
    fn persona(&self) -> &'static str;

    /// System prompt used for this role.
    fn system_prompt(&self) -> &'static str;
}

/// Analyst role.
pub struct AnalystRole;
/// Trader role.
pub struct TraderRole;
/// Risk manager role.
pub struct RiskManagerRole;
/// Researcher role.
pub struct ResearcherRole;
/// Leader role.
pub struct LeaderRole;
/// Devil's Advocate role.
pub struct DevilsAdvocateRole;

impl AgentRole for AnalystRole {
    fn role(&self) -> TeamRole {
        TeamRole::Analyst
    }

    fn persona(&self) -> &'static str {
        "📊 Technical specialist: momentum, structure, support/resistance, and trend"
    }

    fn system_prompt(&self) -> &'static str {
        "You are Analyst (📊), technical specialist. Focus on price structure, trend, momentum, and invalidation levels. Be specific and concise. Always include probability and what would invalidate your view."
    }
}

impl AgentRole for TraderRole {
    fn role(&self) -> TeamRole {
        TeamRole::Trader
    }

    fn persona(&self) -> &'static str {
        "📈 Execution and timing specialist for entries/exits"
    }

    fn system_prompt(&self) -> &'static str {
        "You are Trader (📈), execution and timing specialist. Convert analysis into executable trade plans with entry style, timing bias, and tactical alternatives. Keep language direct."
    }
}

impl AgentRole for RiskManagerRole {
    fn role(&self) -> TeamRole {
        TeamRole::RiskManager
    }

    fn persona(&self) -> &'static str {
        "🛡 Downside, sizing, and risk-budget specialist"
    }

    fn system_prompt(&self) -> &'static str {
        "You are Risk Manager (🛡). Prioritize capital preservation, downside scenarios, drawdown control, and position sizing discipline. If setup quality is weak, explicitly recommend no trade."
    }
}

impl AgentRole for ResearcherRole {
    fn role(&self) -> TeamRole {
        TeamRole::Researcher
    }

    fn persona(&self) -> &'static str {
        "🔬 On-chain, fundamental, and catalyst specialist"
    }

    fn system_prompt(&self) -> &'static str {
        "You are Researcher (🔬). Focus on on-chain/fundamental context, catalysts, market narrative, and regime shifts. Highlight uncertainty when evidence is weak."
    }
}

impl AgentRole for LeaderRole {
    fn role(&self) -> TeamRole {
        TeamRole::Leader
    }

    fn persona(&self) -> &'static str {
        "👑 Team lead synthesizing a final decision"
    }

    fn system_prompt(&self) -> &'static str {
        "You are Leader (👑). Synthesize all team opinions into one final recommendation. Output a clear action headline in uppercase format like BUY BTCUSDT 10% or HOLD. Include concise rationale and key risks."
    }
}

impl AgentRole for DevilsAdvocateRole {
    fn role(&self) -> TeamRole {
        TeamRole::DevilsAdvocate
    }

    fn persona(&self) -> &'static str {
        "😈 Contrarian who challenges majority assumptions"
    }

    fn system_prompt(&self) -> &'static str {
        "You are Devil's Advocate (😈). Challenge majority assumptions, stress-test the thesis, and expose hidden risks. Deliberately seek alternative interpretations and failure modes."
    }
}

/// Returns all hardcoded role implementations.
pub fn hardcoded_roles() -> Vec<Box<dyn AgentRole>> {
    vec![
        Box::new(AnalystRole),
        Box::new(TraderRole),
        Box::new(RiskManagerRole),
        Box::new(ResearcherRole),
        Box::new(LeaderRole),
        Box::new(DevilsAdvocateRole),
    ]
}

/// Returns a fresh Leader role implementation.
pub fn leader_role() -> LeaderRole {
    LeaderRole
}
