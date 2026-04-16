//! Team discussion orchestration.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{future::join_all, StreamExt};
use rust_decimal::Decimal;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::config::LlmConfig;
use crate::error::{MycryptoError, Result};
use crate::state::{
    AppState, StateUpdate, TeamActionCard, TeamActionKind, TeamAgentScore, TeamAgentStatus,
    TeamEdgeKind, TeamRelationEdge, TeamRole, TeamSessionSummary, TeamStance,
};

use super::roles::{hardcoded_roles, leader_role, AgentRole};
use crate::chat::llm::{create_provider, Message};

const TEAM_LLM_TIMEOUT_SECS: u64 = 30;
static TEAM_SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Returns the most recently issued team session identifier.
///
/// The first session starts at `1`; when no session has been started yet,
/// this returns `0`.
#[must_use]
pub fn current_team_session_id() -> u64 {
    TEAM_SESSION_COUNTER
        .load(Ordering::Relaxed)
        .saturating_sub(1)
}

/// Runs one full team discussion session (phase 1 debate + phase 2 synthesis).
#[must_use = "handle orchestration errors to avoid silent team-flow failures"]
pub async fn run_team_discussion(
    state: AppState,
    prompt: String,
    config: LlmConfig,
    tx: mpsc::Sender<StateUpdate>,
) -> Result<()> {
    let session_id = TEAM_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);

    send_update(
        &tx,
        StateUpdate::TeamSessionStarted {
            prompt: prompt.clone(),
            session_id,
        },
    )
    .await?;

    let context = market_context(&state);
    let roles = hardcoded_roles();

    for role in roles.iter().map(|r| r.role()) {
        send_update(
            &tx,
            StateUpdate::TeamAgentStatusChanged {
                role,
                status: TeamAgentStatus::Thinking,
                session_id,
            },
        )
        .await?;
    }

    let tasks = roles.into_iter().map(|role| {
        let prompt_clone = prompt.clone();
        let context_clone = context.clone();
        let config_clone = config.clone();

        async move {
            let role_key = role.role();
            let messages = phase1_messages(role.as_ref(), &prompt_clone, &context_clone);
            let result = complete_text(&config_clone, messages, TEAM_LLM_TIMEOUT_SECS).await;
            (role_key, result)
        }
    });

    let mut phase1_results = Vec::new();
    for (role, result) in join_all(tasks).await {
        match result {
            Ok(content) => {
                send_update(
                    &tx,
                    StateUpdate::TeamMessage {
                        role,
                        phase: 1,
                        content: content.clone(),
                        session_id,
                    },
                )
                .await?;
                phase1_results.push((role, content));
            }
            Err(err) => {
                send_update(
                    &tx,
                    StateUpdate::TeamMessage {
                        role,
                        phase: 1,
                        content: format!("[error] {}", err),
                        session_id,
                    },
                )
                .await?;
            }
        }

        send_update(
            &tx,
            StateUpdate::TeamAgentStatusChanged {
                role,
                status: TeamAgentStatus::Done,
                session_id,
            },
        )
        .await?;
    }

    let edges = infer_relationship_edges(&phase1_results);
    send_update(
        &tx,
        StateUpdate::TeamRelationshipsUpdated { edges, session_id },
    )
    .await?;

    send_update(
        &tx,
        StateUpdate::TeamAgentStatusChanged {
            role: TeamRole::Leader,
            status: TeamAgentStatus::Thinking,
            session_id,
        },
    )
    .await?;

    let leader = leader_role();
    let phase2 = complete_text(
        &config,
        leader_synthesis_messages(&leader, &prompt, &context, &phase1_results),
        TEAM_LLM_TIMEOUT_SECS,
    )
    .await;

    match phase2 {
        Ok(content) => {
            send_update(
                &tx,
                StateUpdate::TeamMessage {
                    role: TeamRole::Leader,
                    phase: 2,
                    content: content.clone(),
                    session_id,
                },
            )
            .await?;
            let summary = build_session_summary(&prompt, &phase1_results, &content);
            send_update(
                &tx,
                StateUpdate::TeamSummary {
                    summary,
                    session_id,
                },
            )
            .await?;
            let action_card = parse_action_card(&content);
            send_update(
                &tx,
                StateUpdate::TeamActionProposed {
                    card: action_card,
                    session_id,
                },
            )
            .await?;
        }
        Err(err) => {
            send_update(
                &tx,
                StateUpdate::TeamSessionError {
                    error: err.to_string(),
                    session_id,
                },
            )
            .await?;
            return Ok(());
        }
    }

    send_update(
        &tx,
        StateUpdate::TeamAgentStatusChanged {
            role: TeamRole::Leader,
            status: TeamAgentStatus::Done,
            session_id,
        },
    )
    .await?;

    send_update(&tx, StateUpdate::TeamSessionCompleted { session_id }).await?;
    Ok(())
}

async fn send_update(tx: &mpsc::Sender<StateUpdate>, update: StateUpdate) -> Result<()> {
    tx.send(update)
        .await
        .map_err(|_| MycryptoError::channel_send("team_update"))
}

async fn complete_text(
    config: &LlmConfig,
    messages: Vec<Message>,
    timeout_secs: u64,
) -> Result<String> {
    let provider = create_provider(config);
    let mut stream = timeout(
        Duration::from_secs(timeout_secs.max(5)),
        provider.stream_completion(messages, config),
    )
    .await
    .map_err(|_| MycryptoError::LlmRequest("timed out waiting for stream start".to_string()))??;
    let mut out = String::new();
    loop {
        let token_result = timeout(Duration::from_secs(timeout_secs.max(5)), stream.next())
            .await
            .map_err(|_| MycryptoError::LlmRequest("timed out waiting for token".to_string()))?;
        let Some(token_result) = token_result else {
            break;
        };
        let token = token_result?;
        if !token.is_final {
            out.push_str(&token.text);
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        return Err(MycryptoError::LlmResponseParse(
            "empty response from provider".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

fn phase1_messages(role: &dyn AgentRole, prompt: &str, context: &str) -> Vec<Message> {
    vec![
        Message::system(role.system_prompt()),
        Message::user(format!(
            "TEAM TASK\nUser prompt: {}\n\nMarket snapshot:\n{}\n\nRules:\n- Respond in <= 120 words.\n- Mention at least one other role key from [ANALYST,TRADER,RISK_MANAGER,RESEARCHER,LEADER,DEVILS_ADVOCATE].\n- Add explicit markers using this exact syntax when relevant: [AGREE:<ROLE_KEY>] [COUNTER:<ROLE_KEY>].\n- Include confidence 0-100 and one-line action bias.",
            prompt, context
        )),
    ]
}

fn leader_synthesis_messages(
    leader: &dyn AgentRole,
    prompt: &str,
    context: &str,
    phase1: &[(TeamRole, String)],
) -> Vec<Message> {
    let mut transcript = String::new();
    for (role, content) in phase1 {
        transcript.push_str(&format!(
            "- {} {}: {}\n",
            role.emoji(),
            role.label(),
            content.replace('\n', " ")
        ));
    }

    vec![
        Message::system(leader.system_prompt()),
        Message::user(format!(
            "PHASE 2 SYNTHESIS\nUser prompt: {}\n\nMarket snapshot:\n{}\n\nTeam phase-1 transcript:\n{}\n\nOutput format:\n1) FINAL_ACTION: <BUY|SELL|CLOSE|HOLD> <PAIR or NONE> <PCT or 0%>\n2) RATIONALE: <2-4 short lines>\n3) KEY_RISKS: <bullet-style short list>",
            prompt, context, transcript
        )),
    ]
}

fn market_context(state: &AppState) -> String {
    let mut lines = Vec::new();
    for pair in state.config.pairs.watchlist.iter().take(8) {
        if let Some(t) = state.get_ticker(pair) {
            lines.push(format!(
                "{} price={} chg24h={:+.2}%",
                pair, t.price, t.price_change_pct_24h
            ));
        }
    }

    if lines.is_empty() {
        lines.push("No ticker data yet".to_string());
    }
    lines.join("\n")
}

fn infer_relationship_edges(results: &[(TeamRole, String)]) -> Vec<TeamRelationEdge> {
    let mut weights: HashMap<(TeamRole, TeamRole, TeamEdgeKind), u32> = HashMap::new();

    for (source_role, content) in results {
        for target_role in TeamRole::ALL {
            if target_role == *source_role {
                continue;
            }

            let agree_marker = format!("[AGREE:{}]", target_role.key());
            let counter_marker = format!("[COUNTER:{}]", target_role.key());

            let agree_count = count_occurrences(content, &agree_marker);
            let counter_count = count_occurrences(content, &counter_marker);

            if agree_count > 0 {
                *weights
                    .entry((*source_role, target_role, TeamEdgeKind::Agree))
                    .or_insert(0) += agree_count;
            }
            if counter_count > 0 {
                *weights
                    .entry((*source_role, target_role, TeamEdgeKind::Counter))
                    .or_insert(0) += counter_count;
            }
        }
    }

    weights
        .into_iter()
        .map(|((from, to, kind), weight)| TeamRelationEdge {
            from,
            to,
            kind,
            weight,
        })
        .collect()
}

fn count_occurrences(haystack: &str, needle: &str) -> u32 {
    haystack.matches(needle).count() as u32
}

fn parse_action_card(content: &str) -> TeamActionCard {
    let final_action_line = content
        .lines()
        .find(|line| line.to_ascii_uppercase().contains("FINAL_ACTION"))
        .unwrap_or(content);

    let upper = final_action_line.to_ascii_uppercase();
    let tokens: Vec<String> = upper
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '%' && c != '_'))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
        .collect();

    let mut kind = TeamActionKind::Hold;
    let mut pair: Option<String> = None;
    let mut allocation_pct = Decimal::ZERO;

    for token in &tokens {
        match token.as_str() {
            "BUY" => kind = TeamActionKind::Buy,
            "SELL" => kind = TeamActionKind::Sell,
            "CLOSE" => kind = TeamActionKind::Close,
            "HOLD" => kind = TeamActionKind::Hold,
            "NONE" => {
                pair = None;
            }
            _ => {}
        }

        if token.ends_with("USDT") {
            pair = Some(token.to_string());
        }

        if let Some(raw_pct) = token.strip_suffix('%') {
            if let Ok(parsed) = Decimal::from_str(raw_pct) {
                allocation_pct = parsed.clamp(Decimal::ZERO, Decimal::from(100));
            }
        }
    }

    if matches!(kind, TeamActionKind::Buy | TeamActionKind::Sell) && allocation_pct <= Decimal::ZERO
    {
        allocation_pct = Decimal::from(10);
    }

    let summary = match kind {
        TeamActionKind::Buy => format!(
            "BUY {} {}% portfolio",
            pair.clone().unwrap_or_else(|| "BTCUSDT".to_string()),
            allocation_pct.round_dp(2)
        ),
        TeamActionKind::Sell => format!(
            "SELL {} {}% portfolio",
            pair.clone().unwrap_or_else(|| "BTCUSDT".to_string()),
            allocation_pct.round_dp(2)
        ),
        TeamActionKind::Close => format!(
            "CLOSE {}",
            pair.clone()
                .unwrap_or_else(|| "(pair unspecified)".to_string())
        ),
        TeamActionKind::Hold => "HOLD / No trade".to_string(),
    };

    TeamActionCard {
        kind,
        pair,
        allocation_pct,
        summary,
        rationale: content.to_string(),
    }
}

fn build_session_summary(
    topic: &str,
    phase1_results: &[(TeamRole, String)],
    leader_content: &str,
) -> TeamSessionSummary {
    let mut scorecard = Vec::new();

    for role in TeamRole::ALL {
        let content = phase1_results
            .iter()
            .find(|(r, _)| *r == role)
            .map(|(_, c)| c.as_str())
            .unwrap_or("");

        scorecard.push(TeamAgentScore {
            role,
            stance: infer_stance(content),
            confidence: infer_confidence(content),
            word_count: content.split_whitespace().count(),
        });
    }

    TeamSessionSummary {
        topic: topic.to_string(),
        timestamp: chrono::Utc::now(),
        leader_verdict: extract_leader_verdict(leader_content),
        scorecard,
    }
}

fn infer_stance(content: &str) -> TeamStance {
    let upper = content.to_ascii_uppercase();
    if upper.contains("BUY") || upper.contains("LONG") || upper.contains("BULL") {
        TeamStance::Bullish
    } else if upper.contains("SELL") || upper.contains("SHORT") || upper.contains("BEAR") {
        TeamStance::Bearish
    } else {
        TeamStance::Neutral
    }
}

fn infer_confidence(content: &str) -> u8 {
    let upper = content.to_ascii_uppercase();
    for token in upper.split_whitespace() {
        let clean = token.trim_matches(|c: char| !c.is_ascii_digit());
        if clean.is_empty() {
            continue;
        }
        if let Ok(v) = clean.parse::<u8>() {
            if v <= 100 {
                return v;
            }
        }
    }
    50
}

fn extract_leader_verdict(content: &str) -> String {
    for line in content.lines() {
        if line.to_ascii_uppercase().contains("FINAL_ACTION") {
            return line.trim().to_string();
        }
    }
    content.lines().next().unwrap_or("HOLD").trim().to_string()
}
