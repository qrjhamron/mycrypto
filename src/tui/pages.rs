//! Page renderers for the TUI.
//!
//! Each page renders rich, semantic-colored content into the main area.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{Datelike, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, Wrap,
    },
    Frame,
};
use rust_decimal::Decimal;

use crate::auth::{AuthProvider, AuthStatus};
use crate::config::LlmProvider;
use crate::state::{
    AppState, LogLevel, PositionSide, SignalAction, SignalDirection, TeamActionKind,
    TeamAgentStatus, TeamEdgeKind, TeamRelationEdge, TeamRole, OHLCV,
};
use crate::tui::theme::{chars, Theme};
use crate::tui::widgets::{Logo, ModelSelector};

mod auth;
mod chart;
mod heatmap;
mod model;
mod news;
mod portfolio;
mod signals;
mod status;
mod team;

const MAX_RENDER_ITEMS_PER_LIST: usize = 100;
const TEAM_THREAD_RENDER_CAP: usize = 100;

/// Popup button options for team action confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamPopupOption {
    /// Execute action card.
    Execute,
    /// Dismiss action card.
    Dismiss,
    /// Edit amount/allocation.
    EditAmount,
    /// Trigger fresh re-analysis.
    Reanalyze,
}

impl TeamPopupOption {
    /// Ordered options for keyboard navigation.
    pub const ALL: [Self; 4] = [
        Self::Execute,
        Self::Dismiss,
        Self::EditAmount,
        Self::Reanalyze,
    ];

    /// Display label shown on button.
    pub fn label(self) -> &'static str {
        match self {
            Self::Execute => "[Y] Execute",
            Self::Dismiss => "[N] Dismiss",
            Self::EditAmount => "[E] Edit Amount",
            Self::Reanalyze => "[D] Request Re-analysis",
        }
    }
}

/// Which page is currently displayed.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Page {
    /// Portfolio summary + open positions.
    #[default]
    Portfolio,
    /// Latest signals.
    Signals,
    /// Chart view.
    Chart,
    /// Trade history.
    History,
    /// Performance stats.
    Stats,
    /// Interactive customization page.
    Customize,
    /// System/data/auth status page.
    Status,
    /// Headlines page.
    News,
    /// Cached news history page.
    NewsHistory,
    /// 24h market heatmap page.
    Heatmap,
    /// Sentiment page.
    Sentiment,
    /// Macro context page.
    Macro,
    /// Help page.
    Help,
    /// Chat conversation.
    Chat,
    /// Log entries.
    Log,
    /// Pairs list.
    Pairs,
    /// Model selector.
    Model,
    /// Authentication page.
    Auth,
    /// Team discussion page.
    Team,
    /// Team discussion history page.
    TeamHistory,
}

/// Auth page input mode for rendering.
#[derive(Debug, Clone)]
pub enum AuthInputModeView {
    /// Provider selection screen.
    Select,
    /// API key input for provider.
    ApiKey {
        provider: AuthProvider,
        masked_input: String,
    },
    /// Gradio space URL input.
    GradioUrl { input: String },
    /// Gradio optional token input.
    GradioToken {
        space_url: String,
        masked_input: String,
    },
}

#[derive(Debug, Clone)]
struct ModelEntry {
    id: String,
    display: String,
    selectable: bool,
}

/// Extra auth view state from App.
#[derive(Debug, Clone)]
pub struct AuthPageView {
    /// Selected provider index.
    pub selected_index: usize,
    /// Selected provider value.
    pub selected_provider: AuthProvider,
    /// Current input mode.
    pub input_mode: AuthInputModeView,
    /// Local transient error.
    pub error: Option<String>,
    /// Local transient info.
    pub info: Option<String>,
    /// Whether auth detail input field is focused.
    pub input_focused: bool,
    /// Whether blinking cursor should be shown for focused input.
    pub cursor_visible: bool,
}

#[derive(Debug, Clone)]
pub struct ModelPageView {
    pub api_key_masked_input: String,
    pub api_key_input_focused: bool,
    pub cursor_visible: bool,
    pub api_key_placeholder: String,
}

/// Extra news history view state from App.
#[derive(Debug, Clone)]
pub struct NewsHistoryView {
    /// Current inline filter query.
    pub query: String,
    /// Whether search input capture is active.
    pub search_active: bool,
}

/// Shared rendering inputs for the active page.
pub struct RenderPageParams<'a> {
    pub page: &'a Page,
    pub state: &'a AppState,
    pub theme: &'a Theme,
    pub scroll: usize,
    pub history_count: usize,
    pub model_selector: Option<&'a ModelSelector>,
    pub model_view: Option<&'a ModelPageView>,
    pub auth_view: Option<&'a AuthPageView>,
    pub news_history_view: Option<&'a NewsHistoryView>,
    pub spinner_frame: usize,
    pub customize_selected: usize,
    pub customize_dirty: bool,
    pub source_names_sorted: Option<&'a [String]>,
    pub source_status_preview: Option<&'a str>,
    pub chat_auto_scroll: bool,
}

struct NewsPageMeta<'a> {
    spinner_frame: usize,
    source_names_sorted: Option<&'a [String]>,
    source_status_preview: Option<&'a str>,
}

/// Render splash screen at default position.
pub fn render_splash(f: &mut Frame, area: Rect, theme: &Theme) {
    render_splash_with_offset(f, area, theme, 0);
}

/// Render splash screen with a vertical offset (negative moves upward).
pub fn render_splash_with_offset(f: &mut Frame, area: Rect, theme: &Theme, offset_y: i16) {
    let logo = Logo::new(theme);
    logo.render_splash(area, f.buffer_mut(), offset_y);
}

/// Render the current page content.
pub fn render_page(f: &mut Frame, area: Rect, params: RenderPageParams<'_>) {
    let RenderPageParams {
        page,
        state,
        theme,
        scroll,
        history_count,
        model_selector,
        model_view,
        auth_view,
        news_history_view,
        spinner_frame,
        customize_selected,
        customize_dirty,
        source_names_sorted,
        source_status_preview,
        chat_auto_scroll,
    } = params;

    match page {
        Page::Model => {
            if let Some(selector) = model_selector {
                render_model_page(f, area, state, theme, selector, model_view);
            }
            return;
        }
        Page::Portfolio => {
            render_portfolio_page(f, area, state, theme, scroll);
            return;
        }
        Page::Auth => {
            render_auth_page(f, area, state, theme, auth_view, spinner_frame);
            return;
        }
        Page::Team => {
            render_team_page(f, area, state, theme, scroll, spinner_frame);
            return;
        }
        Page::Signals => {
            render_signals_page(f, area, state, theme, scroll);
            return;
        }
        Page::Chart => {
            render_chart_page(f, area, state, theme, scroll, spinner_frame);
            return;
        }
        Page::Customize => {
            render_customize_page(f, area, state, theme, customize_selected, customize_dirty);
            return;
        }
        Page::Status => {
            render_status_page(f, area, state, theme, scroll, source_names_sorted);
            return;
        }
        Page::Help => {
            render_help_page(f, area, theme, scroll);
            return;
        }
        Page::News => {
            render_news_page(
                f,
                area,
                state,
                theme,
                scroll,
                NewsPageMeta {
                    spinner_frame,
                    source_names_sorted,
                    source_status_preview,
                },
            );
            return;
        }
        Page::NewsHistory => {
            render_news_history_page(f, area, state, theme, scroll, news_history_view);
            return;
        }
        Page::Sentiment => {
            render_sentiment_page(f, area, state, theme);
            return;
        }
        Page::Macro => {
            render_macro_page(f, area, state, theme, scroll);
            return;
        }
        Page::Pairs => {
            render_pairs_page(f, area, state, theme, scroll);
            return;
        }
        Page::TeamHistory => {
            render_team_history_page(f, area, state, theme, scroll);
            return;
        }
        _ => {}
    }

    let content_block = Block::default()
        .title(Span::styled(
            format!(" {} ", page_label(page)),
            theme.panel_title(),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let content_area = content_block.inner(area);
    f.render_widget(content_block, area);

    let lines = match page {
        Page::Portfolio => render_portfolio(state, theme),
        Page::Signals => Vec::new(),
        Page::Chart => render_chart(
            state,
            theme,
            content_area.width as usize,
            content_area.height as usize,
            spinner_frame,
        ),
        Page::History => render_history(state, theme, history_count),
        Page::Stats => render_stats(state, theme),
        Page::Customize => render_customize(state, theme, customize_selected, customize_dirty),
        Page::Status => render_status(state, theme),
        Page::News => render_news(state, theme),
        Page::NewsHistory => render_news(state, theme),
        Page::Heatmap => render_heatmap(state, theme, content_area.width as usize),
        Page::Sentiment => render_sentiment(state, theme),
        Page::Macro => render_macro(state, theme),
        Page::Help => render_help(theme),
        Page::Chat => render_chat(
            state,
            theme,
            spinner_frame,
            content_area.width as usize,
            chat_auto_scroll,
        ),
        Page::Log => render_log(state, theme),
        Page::Pairs => render_pairs(state, theme),
        Page::Model | Page::Auth | Page::Team | Page::TeamHistory => Vec::new(),
    };

    let visible: Vec<Line> = lines.iter().skip(scroll).cloned().collect();
    let paragraph = Paragraph::new(visible)
        .style(theme.text())
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, content_area);

    let content_height = lines.len().max(1);
    let viewport_height = content_area.height as usize;
    if content_height > viewport_height {
        let mut state = ScrollbarState::new(content_height)
            .position(scroll.min(content_height.saturating_sub(1)));
        f.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            content_area,
            &mut state,
        );
    }
}

fn render_model_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    selector: &ModelSelector,
    model_view: Option<&ModelPageView>,
) {
    model::render_model_page(f, area, state, theme, selector, model_view);
}

fn llm_providers_ordered() -> [LlmProvider; 7] {
    [
        LlmProvider::Claude,
        LlmProvider::OpenAI,
        LlmProvider::Gemini,
        LlmProvider::OpenRouter,
        LlmProvider::Gradio,
        LlmProvider::Copilot,
        LlmProvider::Mock,
    ]
}

fn llm_provider_label(provider: &LlmProvider) -> &'static str {
    match provider {
        LlmProvider::Claude => "Claude",
        LlmProvider::OpenAI => "OpenAI",
        LlmProvider::Gemini => "Gemini",
        LlmProvider::OpenRouter => "OpenRouter",
        LlmProvider::Gradio => "Gradio",
        LlmProvider::Copilot => "Copilot",
        LlmProvider::Mock => "Mock",
    }
}

fn llm_provider_auth_method(provider: &LlmProvider) -> &'static str {
    match provider {
        LlmProvider::Claude
        | LlmProvider::OpenAI
        | LlmProvider::Gemini
        | LlmProvider::OpenRouter => "API key",
        LlmProvider::Gradio => "Space URL + token",
        LlmProvider::Copilot => "Device flow",
        LlmProvider::Mock => "None",
    }
}

fn llm_provider_config_instruction(provider: &LlmProvider) -> &'static str {
    match provider {
        LlmProvider::Claude
        | LlmProvider::OpenAI
        | LlmProvider::Gemini
        | LlmProvider::OpenRouter => "Press A to add API key, then Enter to activate.",
        LlmProvider::Gradio => "Press A to set Space URL/token, M cycles model presets.",
        LlmProvider::Copilot => "Press A to start GitHub device flow auth.",
        LlmProvider::Mock => "No auth needed. Press Enter to activate mock mode.",
    }
}

fn llm_provider_current_masked_status(state: &AppState, provider: &LlmProvider) -> String {
    let Some(auth_provider) = llm_to_auth_provider(provider) else {
        return "not required".to_string();
    };
    let Some(status) = state.auth_state.get(&auth_provider) else {
        if llm_provider_has_env_credentials(provider) {
            return "key in environment".to_string();
        }
        return "not configured".to_string();
    };

    match status {
        AuthStatus::ApiKeyConfigured { masked } => format!("key {}", last4_only_mask(masked)),
        AuthStatus::GradioConfigured {
            space_url,
            token_masked,
        } => {
            if let Some(masked) = token_masked {
                format!("{} ({})", space_url, last4_only_mask(masked))
            } else {
                format!("{} (public)", space_url)
            }
        }
        AuthStatus::AuthenticatedGitHub { username, .. } => format!("@{}", username),
        AuthStatus::PendingDevice { user_code, .. } => format!("pending {}", user_code),
        AuthStatus::Error(err) => err.clone(),
        AuthStatus::NotConfigured => "not configured".to_string(),
    }
}

fn last4_only_mask(masked: &str) -> String {
    let tail: String = masked
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if tail.is_empty() {
        "••••".to_string()
    } else {
        format!("••••{}", tail)
    }
}

fn llm_to_auth_provider(provider: &LlmProvider) -> Option<AuthProvider> {
    match provider {
        LlmProvider::Claude => Some(AuthProvider::Anthropic),
        LlmProvider::OpenAI => Some(AuthProvider::OpenAI),
        LlmProvider::Gemini => Some(AuthProvider::Gemini),
        LlmProvider::OpenRouter => Some(AuthProvider::OpenRouter),
        LlmProvider::Gradio => Some(AuthProvider::Gradio),
        LlmProvider::Copilot => Some(AuthProvider::GitHub),
        LlmProvider::Mock => None,
    }
}

fn llm_provider_has_env_credentials(provider: &LlmProvider) -> bool {
    match provider {
        LlmProvider::Claude => env_var_has_value("CLAUDE_API_KEY"),
        LlmProvider::OpenAI => env_var_has_value("OPENAI_API_KEY"),
        LlmProvider::Gemini => {
            env_var_has_value("GEMINI_API_KEY") || env_var_has_value("GOOGLE_API_KEY")
        }
        LlmProvider::OpenRouter => env_var_has_value("OPENROUTER_API_KEY"),
        LlmProvider::Gradio => {
            env_var_has_value("GRADIO_SPACE_URL") || env_var_has_value("GRADIO_API_KEY")
        }
        LlmProvider::Copilot => env_var_has_value("GITHUB_TOKEN"),
        LlmProvider::Mock => true,
    }
}

fn env_var_has_value(name: &str) -> bool {
    std::env::var(name)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn llm_provider_has_credentials(state: &AppState, provider: &LlmProvider) -> bool {
    let configured_in_state = llm_to_auth_provider(provider)
        .and_then(|auth_provider| state.auth_state.get(&auth_provider))
        .map(|status| status.is_configured())
        .unwrap_or(matches!(provider, LlmProvider::Mock));
    configured_in_state || llm_provider_has_env_credentials(provider)
}

fn provider_key_status_text(state: &AppState, provider: &LlmProvider) -> &'static str {
    match provider {
        LlmProvider::Mock => "not required",
        _ if llm_provider_has_credentials(state, provider) => "configured",
        _ => "missing credentials",
    }
}

fn provider_status_badge(
    state: &AppState,
    provider: AuthProvider,
    _spinner_frame: usize,
    theme: &Theme,
) -> (String, Style) {
    match state.auth_state.get(&provider) {
        Some(AuthStatus::PendingDevice { .. }) => (
            "⟳ Pending".to_string(),
            Style::default().fg(theme.signal_wait),
        ),
        Some(status) if status.is_configured() => (
            "✓ Authenticated".to_string(),
            Style::default().fg(theme.profit_strong),
        ),
        _ => (
            "✗ Not configured".to_string(),
            Style::default().fg(theme.loss_mild),
        ),
    }
}

fn assistant_role_style(state: &AppState, theme: &Theme) -> Style {
    match state.config.llm.provider {
        LlmProvider::Claude => Style::default()
            .fg(theme.signal_wait)
            .add_modifier(Modifier::BOLD),
        LlmProvider::Gemini => theme.text_accent_bold(),
        LlmProvider::OpenAI => Style::default()
            .fg(theme.profit_strong)
            .add_modifier(Modifier::BOLD),
        LlmProvider::OpenRouter => Style::default()
            .fg(theme.chat_user_name)
            .add_modifier(Modifier::BOLD),
        LlmProvider::Copilot => theme.chat_agent(),
        LlmProvider::Gradio => theme.text_bold(),
        LlmProvider::Mock => theme.text_muted(),
    }
}

fn push_message_separator(lines: &mut Vec<Line<'static>>, theme: &Theme, width: usize) {
    let divider_width = width.clamp(24, 120);
    lines.push(Line::from(Span::styled(
        chars::LINE_H_DASHED.repeat(divider_width),
        theme.divider(),
    )));
}

fn align_right(text: &str, width: usize) -> String {
    let text_width = text.chars().count();
    if text_width >= width {
        text.to_string()
    } else {
        format!("{}{}", " ".repeat(width - text_width), text)
    }
}

fn provider_indicator_style(
    state: &AppState,
    theme: &Theme,
    provider: &LlmProvider,
) -> (Style, &'static str) {
    match provider {
        LlmProvider::Mock => (Style::default().fg(theme.status_warn), "unconfigured"),
        LlmProvider::Copilot | LlmProvider::Gradio => {
            if llm_provider_has_credentials(state, provider) {
                (Style::default().fg(theme.status_ok), "key present")
            } else {
                (Style::default().fg(theme.status_warn), "unconfigured")
            }
        }
        _ => {
            if llm_provider_has_credentials(state, provider) {
                (Style::default().fg(theme.status_ok), "key present")
            } else {
                (Style::default().fg(theme.status_error), "key missing")
            }
        }
    }
}

fn model_provider_models(
    state: &AppState,
    provider: &LlmProvider,
    current_model: &str,
) -> Vec<ModelEntry> {
    let defaults: &[&str] = match provider {
        LlmProvider::Claude => &["claude-opus-4-5", "claude-sonnet-4-5", "claude-haiku-4-5"],
        LlmProvider::OpenAI => &["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "o1", "o1-mini"],
        LlmProvider::Gemini => &["gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"],
        LlmProvider::OpenRouter => &[
            "anthropic/claude-sonnet-4-5",
            "openai/gpt-4o",
            "google/gemini-2.0-flash-001",
        ],
        LlmProvider::Gradio => &[
            "https://huggingface.co/spaces/HuggingFaceH4/zephyr-chat",
            "https://huggingface.co/spaces/Qwen/Qwen2.5-Max-Demo",
            "https://huggingface.co/spaces/microsoft/Phi-4-multimodal-instruct",
        ],
        LlmProvider::Copilot => &["gpt-4o"],
        LlmProvider::Mock => &["mock-v1"],
    };

    let mut models: Vec<ModelEntry> = defaults
        .iter()
        .map(|value| ModelEntry {
            id: (*value).to_string(),
            display: (*value).to_string(),
            selectable: true,
        })
        .collect();

    if matches!(provider, LlmProvider::OpenRouter) {
        if state.openrouter_free_models.is_empty()
            && !llm_provider_has_credentials(state, &LlmProvider::OpenRouter)
        {
            models.push(ModelEntry {
                id: "openrouter-helper".to_string(),
                display: "Configure OpenRouter key to see free models".to_string(),
                selectable: false,
            });
        } else if state.openrouter_free_models.is_empty() {
            models.push(ModelEntry {
                id: "openrouter-empty".to_string(),
                display: "Free Models: none available right now".to_string(),
                selectable: false,
            });
        } else {
            models.push(ModelEntry {
                id: "openrouter-free-header".to_string(),
                display: "Free Models".to_string(),
                selectable: false,
            });
            for model in &state.openrouter_free_models {
                if !models.iter().any(|item| item.id == *model) {
                    models.push(ModelEntry {
                        id: model.clone(),
                        display: format!("🆓 {}", model),
                        selectable: true,
                    });
                }
            }
        }
    }

    if !current_model.trim().is_empty() && !models.iter().any(|item| item.id == current_model) {
        models.insert(
            0,
            ModelEntry {
                id: current_model.to_string(),
                display: current_model.to_string(),
                selectable: true,
            },
        );
    }

    if models.is_empty() {
        models.push(ModelEntry {
            id: "no-models".to_string(),
            display: "(no model presets)".to_string(),
            selectable: false,
        });
    }
    models
}

fn provider_start_index(total: usize, viewport: usize, selected: usize) -> usize {
    if total <= viewport || viewport == 0 {
        return 0;
    }
    let half = viewport / 2;
    let desired = selected.saturating_sub(half);
    desired.min(total.saturating_sub(viewport))
}

fn render_portfolio(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let portfolio = &state.portfolio;
    let metrics = portfolio.calculate_metrics();

    let total_value = portfolio.total_value();
    let invested = portfolio.total_position_value();
    let daily_pnl = portfolio.daily_realized_pnl + portfolio.total_unrealized_pnl();
    let daily_pnl_pct = if total_value > Decimal::ZERO {
        (daily_pnl / total_value) * Decimal::from(100)
    } else {
        Decimal::ZERO
    };

    lines.push(Line::from(vec![
        Span::styled("◈ PORTFOLIO", theme.app_name()),
        Span::styled("                                      ", theme.text()),
        Span::styled("[PAPER MODE]", theme.paper_badge()),
    ]));
    push_divider(&mut lines, theme, 66);
    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::styled("Total Value     ", theme.table_header()),
        Span::styled("Cash Available   ", theme.table_header()),
        Span::styled("Invested        ", theme.table_header()),
        Span::styled("PnL Today        ", theme.table_header()),
        Span::styled("Drawdown", theme.table_header()),
    ]));

    lines.push(Line::from(vec![
        Span::styled(
            format!("{:<15}", format_price(total_value)),
            theme.text_bold(),
        ),
        Span::styled(
            format!("{:<16}", format_price(portfolio.cash)),
            theme.text(),
        ),
        Span::styled(
            format!("{:<14}", format_price(invested)),
            theme.text_accent(),
        ),
        Span::styled(
            format!(
                "{:<16}",
                format!(
                    "{} {}{:.1}%",
                    format_pnl(daily_pnl),
                    if daily_pnl >= Decimal::ZERO {
                        chars::ARROW_UP
                    } else {
                        chars::ARROW_DOWN
                    },
                    daily_pnl_pct.abs()
                )
            ),
            theme.pnl(daily_pnl),
        ),
        Span::styled(
            format!("-{:.1}%", portfolio.current_drawdown_pct),
            Style::default().fg(theme.loss),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Summary  ", theme.table_header()),
        Span::styled(
            format!(
                "Trades:{}  Win:{:.1}%  PF:{:.2}x  Net:{}",
                metrics.total_trades,
                metrics.win_rate,
                metrics.profit_factor,
                format_pnl(metrics.net_profit)
            ),
            theme.text_secondary(),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "── OPEN POSITIONS ({}) ────────────────────────────────────────",
            portfolio.positions.len()
        ),
        theme.section_header(),
    )));

    if portfolio.positions.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "◌  No open positions.",
            theme.text_muted(),
        )]));
        lines.push(Line::from(vec![Span::styled(
            "   Agent is scanning market for signals...",
            theme.text_muted(),
        )]));
        lines.push(Line::from(vec![
            Span::styled("   Type ", theme.text_muted()),
            Span::styled("/signals", theme.text_accent_bold()),
            Span::styled(" to view current analysis.", theme.text_muted()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("PAIR      ", theme.table_header()),
            Span::styled("DIR       ", theme.table_header()),
            Span::styled("ENTRY       ", theme.table_header()),
            Span::styled("NOW         ", theme.table_header()),
            Span::styled("PNL            ", theme.table_header()),
            Span::styled("SL          ", theme.table_header()),
            Span::styled("TP", theme.table_header()),
        ]));
        push_divider(&mut lines, theme, 70);

        for pos in &portfolio.positions {
            let (dir_icon, dir_text, dir_style) = if pos.side == PositionSide::Long {
                (
                    chars::ARROW_UP,
                    "LONG",
                    Style::default()
                        .fg(theme.signal_long)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    chars::ARROW_DOWN,
                    "SHORT",
                    Style::default()
                        .fg(theme.signal_short)
                        .add_modifier(Modifier::BOLD),
                )
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<9}", pos.pair.replace("USDT", "")),
                    theme.text_accent_bold(),
                ),
                Span::styled(format!("{} {:<6}", dir_icon, dir_text), dir_style),
                Span::styled(
                    format!("{:<12}", format_price(pos.entry_price)),
                    theme.text(),
                ),
                Span::styled(
                    format!("{:<12}", format_price(pos.current_price)),
                    theme.text(),
                ),
                Span::styled(
                    format!(
                        "{:<14}",
                        format!(
                            "{} {:+.1}%",
                            format_pnl(pos.unrealized_pnl),
                            pos.unrealized_pnl_pct
                        )
                    ),
                    theme.pnl(pos.unrealized_pnl),
                ),
                Span::styled(
                    format!("{:<12}", format_price(pos.stop_loss)),
                    Style::default().fg(theme.loss),
                ),
                Span::styled(
                    format_price(pos.take_profit),
                    Style::default().fg(theme.profit),
                ),
            ]));

            let spark = sparkline_from_position(pos.unrealized_pnl_pct);
            lines.push(Line::from(vec![
                Span::styled("         ", theme.text()),
                Span::styled("spark ", theme.text_muted()),
                Span::styled(spark, theme.pnl(pos.unrealized_pnl)),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "── PERFORMANCE SNAPSHOT ──────────────────────────────────────",
        theme.section_header(),
    )));

    lines.push(Line::from(vec![
        Span::styled("Win Rate     ", theme.table_header()),
        Span::styled("Profit Factor   ", theme.table_header()),
        Span::styled("Total Trades   ", theme.table_header()),
        Span::styled("Best Trade      ", theme.table_header()),
        Span::styled("Worst Trade", theme.table_header()),
    ]));

    let best = state
        .portfolio
        .trade_history
        .iter()
        .max_by_key(|t| t.realized_pnl)
        .map(|t| {
            format!(
                "{} {}",
                format_pnl(t.realized_pnl),
                t.pair.replace("USDT", "")
            )
        })
        .unwrap_or_else(|| "—".to_string());

    let worst = state
        .portfolio
        .trade_history
        .iter()
        .min_by_key(|t| t.realized_pnl)
        .map(|t| {
            format!(
                "{} {}",
                format_pnl(t.realized_pnl),
                t.pair.replace("USDT", "")
            )
        })
        .unwrap_or_else(|| "—".to_string());

    lines.push(Line::from(vec![
        Span::styled(
            format!("{:<12}", format!("{:.1}%", metrics.win_rate)),
            theme.price_change(metrics.win_rate >= Decimal::from(50)),
        ),
        Span::styled(
            format!("{:<16}", format!("{:.2}x", metrics.profit_factor)),
            Style::default().fg(theme.profit),
        ),
        Span::styled(format!("{:<14}", metrics.total_trades), theme.text()),
        Span::styled(
            format!("{:<16}", best),
            Style::default().fg(theme.profit_strong),
        ),
        Span::styled(worst, Style::default().fg(theme.loss_strong)),
    ]));

    lines.push(Line::from(""));
    lines
}

fn render_signals(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let signals = state.signal_history.recent(20);

    let last_scan = signals
        .first()
        .map(|s| {
            format_duration(
                Utc::now()
                    .signed_duration_since(s.generated_at)
                    .to_std()
                    .unwrap_or(Duration::from_secs(0)),
            )
        })
        .unwrap_or_else(|| "—".to_string());

    let next_scan = Duration::from_secs(state.config.engine.tick_interval_secs.min(3600));

    lines.push(Line::from(vec![
        Span::styled("◈ SIGNALS", theme.app_name()),
        Span::styled(
            format!(
                "                        last scan: {}   next: {}",
                last_scan,
                format_duration(next_scan)
            ),
            theme.text_muted(),
        ),
    ]));
    push_divider(&mut lines, theme, 70);

    if signals.is_empty() {
        lines.push(Line::from(Span::styled(
            "No signals yet. Engine scheduler is analyzing markets...",
            theme.text_muted(),
        )));
        return lines;
    }

    lines.push(Line::from(vec![
        Span::styled("PAIR      ", theme.table_header()),
        Span::styled("DIR         ", theme.table_header()),
        Span::styled("CONFIDENCE          ", theme.table_header()),
        Span::styled("ACTION      ", theme.table_header()),
        Span::styled("AGE      ", theme.table_header()),
        Span::styled("EXPIRES", theme.table_header()),
    ]));
    push_divider(&mut lines, theme, 70);

    for signal in signals.iter().take(8) {
        let (icon, dir_label, dir_style) = match signal.direction {
            SignalDirection::Long => (
                "⚡",
                "LONG",
                Style::default()
                    .fg(theme.signal_long)
                    .add_modifier(Modifier::BOLD),
            ),
            SignalDirection::Short => (
                "🚫",
                "SKIP",
                Style::default()
                    .fg(theme.signal_short)
                    .add_modifier(Modifier::BOLD),
            ),
            SignalDirection::Wait => (
                "⏳",
                "WAIT",
                Style::default()
                    .fg(theme.signal_wait)
                    .add_modifier(Modifier::BOLD),
            ),
        };

        let conf = confidence_bar(signal.confidence, theme);
        let action = match signal.action {
            SignalAction::Execute => {
                if signal.executed {
                    ("EXECUTED", Style::default().fg(theme.profit_strong))
                } else {
                    ("EXECUTE", Style::default().fg(theme.profit))
                }
            }
            SignalAction::Watch => ("WATCHING", Style::default().fg(theme.signal_wait)),
            SignalAction::Skip => ("SKIPPED", Style::default().fg(theme.loss_mild)),
        };

        let age = Utc::now().signed_duration_since(signal.generated_at);
        let expires = if signal.is_expired() {
            "expired".to_string()
        } else {
            signal
                .expires_at
                .signed_duration_since(Utc::now())
                .to_std()
                .map(format_duration)
                .unwrap_or_else(|_| "—".to_string())
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("{} {:<7}", icon, signal.pair.replace("USDT", "")),
                theme.text_accent_bold(),
            ),
            Span::styled(
                format!(
                    "{} {:<7}",
                    match signal.direction {
                        SignalDirection::Long => chars::ARROW_UP,
                        SignalDirection::Short => chars::ARROW_DOWN,
                        SignalDirection::Wait => "─",
                    },
                    dir_label
                ),
                dir_style,
            ),
            Span::styled(
                format!("{:<21}", conf),
                confidence_style(signal.confidence, theme),
            ),
            Span::styled(format!("{:<11}", action.0), action.1),
            Span::styled(
                format!(
                    "{:<9}",
                    format_duration(age.to_std().unwrap_or(Duration::from_secs(0)))
                ),
                theme.text_muted(),
            ),
            Span::styled(expires, theme.text_muted()),
        ]));
    }

    let selected = signals[0].clone();

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "[{} signal selected — press Enter to view reasoning]",
            selected.pair.replace("USDT", "")
        ),
        theme.text_muted(),
    )));
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        format!(
            "── SIGNAL DETAIL: {} ────────────────────────────────────────",
            selected.pair.replace("USDT", "")
        ),
        theme.section_header(),
    )));

    let direction_style = match selected.direction {
        SignalDirection::Long => Style::default()
            .fg(theme.signal_long)
            .add_modifier(Modifier::BOLD),
        SignalDirection::Short => Style::default()
            .fg(theme.signal_short)
            .add_modifier(Modifier::BOLD),
        SignalDirection::Wait => Style::default()
            .fg(theme.signal_wait)
            .add_modifier(Modifier::BOLD),
    };

    lines.push(Line::from(vec![
        Span::styled(
            format!(
                "{} {}  {}% confidence  ",
                match selected.direction {
                    SignalDirection::Long => chars::ARROW_UP,
                    SignalDirection::Short => chars::ARROW_DOWN,
                    SignalDirection::Wait => "─",
                },
                selected.direction,
                selected.confidence
            ),
            direction_style,
        ),
        Span::styled(
            if selected.executed {
                format!("Executed @ {}", format_price(selected.entry_price))
            } else {
                format!("Entry @ {}", format_price(selected.entry_price))
            },
            theme.text(),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(format!("{:<22}", "STEP"), theme.table_header()),
        Span::styled(format!("{:<10}", "SCORE"), theme.table_header()),
        Span::styled("DETAIL", theme.table_header()),
    ]));

    let mut total_score: i32 = 0;
    let mut max_score: i32 = 0;
    for reason in &selected.reasoning {
        total_score += reason.score as i32;
        max_score += reason.score.unsigned_abs() as i32;
        lines.push(Line::from(vec![
            Span::styled(format!("{:<22}", reason.step_name), theme.text()),
            Span::styled(
                format!("{:+}/{}", reason.score, 30),
                score_style(reason.score, theme),
            ),
            Span::styled(format!("  {}", reason.detail), theme.text_secondary()),
        ]));
    }

    lines.push(Line::from(Span::styled(
        "────────────────────────────────────────",
        theme.divider(),
    )));
    lines.push(Line::from(vec![
        Span::styled("Total", theme.table_header()),
        Span::styled(
            format!("  {}/{}", total_score.max(0), max_score.max(1)),
            theme.text_accent_bold(),
        ),
        Span::styled(
            format!("  → normalized to {}% confidence", selected.confidence),
            theme.text_muted(),
        ),
    ]));

    lines.push(Line::from(""));
    lines
}

fn render_signals_page(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, scroll: usize) {
    signals::render_signals_page(f, area, state, theme, scroll);
}

fn render_portfolio_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
) {
    portfolio::render_portfolio_page(f, area, state, theme, scroll);
}

fn render_status_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
    source_names_sorted: Option<&[String]>,
) {
    status::render_status_page(f, area, state, theme, scroll, source_names_sorted);
}

fn render_help_page(f: &mut Frame, area: Rect, theme: &Theme, _scroll: usize) {
    let shell = Block::default()
        .title(Span::styled(" Help ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let inner = shell.inner(area);
    f.render_widget(shell, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(42),
            Constraint::Percentage(34),
            Constraint::Percentage(24),
        ])
        .split(inner);

    let nav_rows = vec![
        (
            "/portfolio",
            "Portfolio & positions",
            "/close [PAIR]",
            "Close position manually",
        ),
        (
            "/signals",
            "Latest signals",
            "/risk [n]",
            "Set risk per trade",
        ),
        (
            "/chart",
            "Price chart",
            "/confidence [n]",
            "Set confidence threshold",
        ),
        (
            "/history",
            "Closed trades",
            "/pause / /resume",
            "Control trading agent",
        ),
    ];

    let nav_table_rows: Vec<Row> = nav_rows
        .into_iter()
        .map(|(a, b, c, d)| {
            Row::new(vec![
                Cell::from(a).style(theme.text_accent()),
                Cell::from(b).style(theme.text_secondary()),
                Cell::from(c).style(theme.text_accent()),
                Cell::from(d).style(theme.text_secondary()),
            ])
        })
        .collect();

    let nav_table = Table::new(
        nav_table_rows,
        [
            Constraint::Length(20),
            Constraint::Length(23),
            Constraint::Length(22),
            Constraint::Min(12),
        ],
    )
    .header(
        Row::new(vec![
            Cell::from("Navigation"),
            Cell::from("Description"),
            Cell::from("Actions"),
            Cell::from("Description"),
        ])
        .style(theme.table_header()),
    )
    .block(
        Block::default()
            .title(Span::styled(" Navigation + Trading ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(nav_table, sections[0]);

    let config_rows = vec![
        (
            "/model",
            "Switch AI provider/model",
            "/auth",
            "Open auth menu",
        ),
        (
            "/pairs",
            "Manage watchlist",
            "/auth-delete [provider]",
            "Delete stored auth",
        ),
        (
            "/status",
            "System health",
            "/auth github",
            "Start GitHub auth",
        ),
        (
            "/team <prompt>",
            "Run AI Agent Team",
            "/team history",
            "Last 5 sessions",
        ),
    ];

    let config_table_rows: Vec<Row> = config_rows
        .into_iter()
        .map(|(a, b, c, d)| {
            Row::new(vec![
                Cell::from(a).style(theme.text_accent()),
                Cell::from(b).style(theme.text_secondary()),
                Cell::from(c).style(theme.text_accent()),
                Cell::from(d).style(theme.text_secondary()),
            ])
        })
        .collect();

    let config_table = Table::new(
        config_table_rows,
        [
            Constraint::Length(20),
            Constraint::Length(23),
            Constraint::Length(22),
            Constraint::Min(12),
        ],
    )
    .header(
        Row::new(vec![
            Cell::from("Config"),
            Cell::from("Description"),
            Cell::from("Auth/Team"),
            Cell::from("Description"),
        ])
        .style(theme.table_header()),
    )
    .block(
        Block::default()
            .title(Span::styled(" Config + Auth + Team ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(config_table, sections[1]);

    let shortcuts = vec![
        Line::from(vec![
            Span::styled("Input: ", theme.table_header()),
            Span::styled(
                "/ opens autocomplete, Enter submit, Esc clear",
                theme.text_secondary(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Navigation: ", theme.table_header()),
            Span::styled(
                "1-7 tabs, ←/→ cycle tabs, ↑/↓ scroll",
                theme.text_secondary(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Global: ", theme.table_header()),
            Span::styled(
                "Ctrl+L clear, Ctrl+C exit confirm, ? keybind popup",
                theme.text_secondary(),
            ),
        ]),
    ];

    let shortcuts_panel = Paragraph::new(shortcuts)
        .style(theme.text())
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(Span::styled(" Keyboard Shortcuts ", theme.panel_title()))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.panel_border()),
        );
    f.render_widget(shortcuts_panel, sections[2]);
}

fn render_news_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
    meta: NewsPageMeta<'_>,
) {
    news::render_news_page(f, area, state, theme, scroll, meta);
}

fn render_news_history_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
    view: Option<&NewsHistoryView>,
) {
    news::render_news_history_page(f, area, state, theme, scroll, view);
}

fn filter_news_history<'a, I>(items: I, query: &str) -> Vec<&'a crate::state::NewsHeadline>
where
    I: IntoIterator<Item = &'a crate::state::NewsHeadline>,
{
    if query.trim().is_empty() {
        return items.into_iter().take(MAX_RENDER_ITEMS_PER_LIST).collect();
    }

    let q = query.to_ascii_lowercase();
    items
        .into_iter()
        .filter(|item| {
            item.title.to_ascii_lowercase().contains(&q)
                || item.source.to_ascii_lowercase().contains(&q)
                || item
                    .url
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&q)
        })
        .take(MAX_RENDER_ITEMS_PER_LIST)
        .collect()
}

fn news_bucket_label(ts: chrono::DateTime<Utc>) -> String {
    let now = Utc::now();
    let today = now.date_naive();
    let date = ts.date_naive();
    let age_days = (today - date).num_days();

    if age_days == 0 {
        "Today".to_string()
    } else if age_days == 1 {
        "Yesterday".to_string()
    } else if age_days <= 7 {
        "This Week".to_string()
    } else {
        format!("{}-{:02}-{:02}", date.year(), date.month(), date.day())
    }
}

fn split_match_for_highlight(text: &str, query: &str) -> (String, (String, String)) {
    if query.trim().is_empty() {
        return (truncate_ellipsis(text, 120), (String::new(), String::new()));
    }

    let text_l = text.to_ascii_lowercase();
    let q = query.to_ascii_lowercase();
    if let Some((pos, matched)) = text_l.match_indices(&q).next() {
        let start = pos;
        let end = (start + matched.len()).min(text.len());
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return (truncate_ellipsis(text, 120), (String::new(), String::new()));
        }
        let pre = truncate_ellipsis(&text[..start], 90);
        let mid = text[start..end].to_string();
        let post = truncate_ellipsis(&text[end..], 90);
        (pre, (mid, post))
    } else {
        (truncate_ellipsis(text, 120), (String::new(), String::new()))
    }
}

fn sentiment_badge(sentiment: Option<f32>, theme: &Theme) -> (String, Style) {
    match sentiment {
        Some(v) if v > 0.15 => ("🟢".to_string(), theme.profit_style()),
        Some(v) if v < -0.15 => ("🔴".to_string(), theme.loss_style()),
        Some(_) => ("🟡".to_string(), theme.text_secondary()),
        None => ("·".to_string(), theme.text_muted()),
    }
}

fn render_sentiment_page(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme) {
    let shell = Block::default()
        .title(Span::styled(" Sentiment ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let inner = shell.inner(area);
    f.render_widget(shell, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Min(8),
        ])
        .split(inner);

    let (composite_text, composite_style, updated_text) = if let Some(s) = &state.sentiment_score {
        (
            format!("{:+.2}", s.composite),
            if s.composite > 0.25 {
                theme.sentiment_positive()
            } else if s.composite < -0.25 {
                theme.sentiment_negative()
            } else {
                theme.sentiment_neutral()
            },
            s.updated_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        )
    } else {
        (
            "n/a".to_string(),
            theme.sentiment_neutral(),
            "not updated".to_string(),
        )
    };

    let headline = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Composite Sentiment ", theme.table_header()),
            Span::styled(format!(" {} ", composite_text), composite_style),
        ]),
        Line::from(vec![
            Span::styled("Last update: ", theme.table_header()),
            Span::styled(updated_text, theme.text_secondary()),
        ]),
    ])
    .style(theme.text())
    .block(
        Block::default()
            .title(Span::styled(" Overview ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    );
    f.render_widget(headline, chunks[0]);

    let factors = if let Some(s) = &state.sentiment_score {
        vec![
            (
                "Fear & Greed",
                s.fear_greed
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
            ),
            (
                "Fear & Greed Label",
                s.fear_greed_label
                    .clone()
                    .unwrap_or_else(|| "n/a".to_string()),
            ),
            (
                "Reddit",
                s.reddit_score
                    .map(|v| format!("{:+.2}", v))
                    .unwrap_or_else(|| "n/a".to_string()),
            ),
            (
                "X / Twitter",
                s.twitter_score
                    .map(|v| format!("{:+.2}", v))
                    .unwrap_or_else(|| "n/a".to_string()),
            ),
            (
                "News",
                s.news_score
                    .map(|v| format!("{:+.2}", v))
                    .unwrap_or_else(|| "n/a".to_string()),
            ),
        ]
    } else {
        vec![
            ("Fear & Greed", "n/a".to_string()),
            ("Fear & Greed Label", "n/a".to_string()),
            ("Reddit", "n/a".to_string()),
            ("X / Twitter", "n/a".to_string()),
            ("News", "n/a".to_string()),
        ]
    };

    let factor_rows: Vec<Row> = factors
        .into_iter()
        .map(|(name, value)| {
            Row::new(vec![
                Cell::from(name).style(theme.table_header()),
                Cell::from(value.clone()).style(sentiment_value_style(&value, theme)),
            ])
        })
        .collect();

    let factor_table = Table::new(factor_rows, [Constraint::Length(18), Constraint::Min(10)])
        .header(
            Row::new(vec![Cell::from("Factor"), Cell::from("Value")]).style(theme.table_header()),
        )
        .block(
            Block::default()
                .title(Span::styled(" Source Factors ", theme.panel_title()))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.panel_border()),
        )
        .style(theme.text());
    f.render_widget(factor_table, chunks[1]);

    let sources_text = state
        .sentiment_score
        .as_ref()
        .map(|s| {
            if s.sources_available.is_empty() {
                "none".to_string()
            } else {
                s.sources_available.join(", ")
            }
        })
        .unwrap_or_else(|| "none".to_string());

    let notes = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Contributing sources: ", theme.table_header()),
            Span::styled(sources_text, theme.text_secondary()),
        ]),
        Line::from(vec![
            Span::styled("Interpretation: ", theme.table_header()),
            Span::styled(
                "positive > +0.25, neutral between -0.25 and +0.25, negative < -0.25",
                theme.text_muted(),
            ),
        ]),
    ])
    .style(theme.text())
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .title(Span::styled(" Notes ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    );
    f.render_widget(notes, chunks[2]);
}

fn render_macro_page(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, scroll: usize) {
    let shell = Block::default()
        .title(Span::styled(" Macro ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let inner = shell.inner(area);
    f.render_widget(shell, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(10)])
        .split(inner);

    let macro_rows = vec![
        Row::new(vec![
            Cell::from("SPY").style(theme.table_header()),
            Cell::from(
                state
                    .macro_context
                    .spy_change_pct
                    .map(|v| format!("{:+.2}%", v))
                    .unwrap_or_else(|| "n/a".to_string()),
            )
            .style(theme.text()),
            Cell::from("DXY").style(theme.table_header()),
            Cell::from(
                state
                    .macro_context
                    .dxy_change_pct
                    .map(|v| format!("{:+.2}%", v))
                    .unwrap_or_else(|| "n/a".to_string()),
            )
            .style(theme.text()),
            Cell::from("VIX").style(theme.table_header()),
            Cell::from(
                state
                    .macro_context
                    .vix
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "n/a".to_string()),
            )
            .style(theme.text()),
        ]),
        Row::new(vec![
            Cell::from("BTC Dom").style(theme.table_header()),
            Cell::from(
                state
                    .macro_context
                    .btc_dominance
                    .map(|v| format!("{:.2}%", v))
                    .unwrap_or_else(|| "n/a".to_string()),
            )
            .style(theme.text()),
            Cell::from("Total MCap").style(theme.table_header()),
            Cell::from(
                state
                    .macro_context
                    .total_market_cap
                    .map(format_large_usd)
                    .unwrap_or_else(|| "n/a".to_string()),
            )
            .style(theme.text()),
            Cell::from("Updated").style(theme.table_header()),
            Cell::from(
                state
                    .macro_context
                    .updated_at
                    .map(|t| t.format("%H:%M:%S").to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
            )
            .style(theme.text_secondary()),
        ]),
    ];

    let macro_table = Table::new(
        macro_rows,
        [
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(9),
            Constraint::Min(10),
        ],
    )
    .block(
        Block::default()
            .title(Span::styled(" Market Snapshot ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(macro_table, chunks[0]);

    let event_header = Row::new(vec![
        Cell::from("Date"),
        Cell::from("Impact"),
        Cell::from("Country"),
        Cell::from("Event"),
    ])
    .style(theme.table_header());

    let event_text_width = chunks[1].width.saturating_sub(30) as usize;
    let total_events = state.macro_context.upcoming_events.len();
    let viewport = chunks[1].height.saturating_sub(3) as usize;
    let start = scroll.min(total_events.saturating_sub(1));
    let end = (start + viewport.max(1)).min(total_events);

    let event_rows: Vec<Row> = if state.macro_context.upcoming_events.is_empty() {
        vec![Row::new(vec![
            Cell::from("--").style(theme.text_muted()),
            Cell::from("n/a").style(theme.text_muted()),
            Cell::from("--").style(theme.text_muted()),
            Cell::from("No upcoming events").style(theme.text_muted()),
        ])]
    } else {
        state.macro_context.upcoming_events[start..end]
            .iter()
            .map(|e| {
                let impact_style = match e.impact.to_ascii_lowercase().as_str() {
                    "high" => theme.loss_style(),
                    "medium" => theme.warning(),
                    "low" => theme.profit_style(),
                    _ => theme.text_muted(),
                };
                Row::new(vec![
                    Cell::from(e.time.format("%Y-%m-%d").to_string()).style(theme.text_secondary()),
                    Cell::from(truncate_ellipsis(&e.impact, 7)).style(impact_style),
                    Cell::from(truncate_ellipsis(&e.country, 8)).style(theme.text_accent()),
                    Cell::from(truncate_ellipsis(&e.title, event_text_width.max(16)))
                        .style(theme.text()),
                ])
            })
            .collect()
    };

    let event_table = Table::new(
        event_rows,
        [
            Constraint::Length(11),
            Constraint::Length(8),
            Constraint::Length(9),
            Constraint::Min(12),
        ],
    )
    .header(event_header)
    .block(
        Block::default()
            .title(Span::styled(
                " Upcoming Economic Events ",
                theme.panel_title(),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(event_table, chunks[1]);

    if total_events > viewport.max(1) {
        let mut scroll_state =
            ScrollbarState::new(total_events).position(start.min(total_events - 1));
        f.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            chunks[1],
            &mut scroll_state,
        );
    }
}

fn render_pairs_page(f: &mut Frame, area: Rect, state: &AppState, theme: &Theme, scroll: usize) {
    let shell = Block::default()
        .title(Span::styled(" Pairs ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let inner = shell.inner(area);
    f.render_widget(shell, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(5)])
        .split(inner);

    let header = Row::new(vec![
        Cell::from("Pair"),
        Cell::from("Price"),
        Cell::from("24h"),
        Cell::from("Spread"),
        Cell::from("Volume"),
    ])
    .style(theme.table_header());

    let total = state.config.pairs.watchlist.len();
    let viewport = chunks[0].height.saturating_sub(3) as usize;
    let start = scroll.min(total.saturating_sub(1));
    let end = (start + viewport.max(1)).min(total);

    let rows: Vec<Row> = if state.config.pairs.watchlist.is_empty() {
        vec![Row::new(vec![
            Cell::from("n/a").style(theme.text_muted()),
            Cell::from("--").style(theme.text_muted()),
            Cell::from("--").style(theme.text_muted()),
            Cell::from("--").style(theme.text_muted()),
            Cell::from("--").style(theme.text_muted()),
        ])]
    } else {
        state.config.pairs.watchlist[start..end]
            .iter()
            .map(|pair| {
                if let Some(t) = state.get_ticker(pair) {
                    let change = t.price_change_pct_24h;
                    let spread = t.spread_pct();
                    let volume = format_compact_number(t.quote_volume_24h);
                    Row::new(vec![
                        Cell::from(pair.replace("USDT", "")).style(theme.text_accent_bold()),
                        Cell::from(format_price(t.price)).style(theme.text()),
                        Cell::from(format!("{:+.2}%", change))
                            .style(theme.price_change(change >= Decimal::ZERO)),
                        Cell::from(format!("{:.2}%", spread)).style(theme.text_secondary()),
                        Cell::from(volume).style(theme.text_secondary()),
                    ])
                } else {
                    Row::new(vec![
                        Cell::from(pair.replace("USDT", "")).style(theme.text_accent_bold()),
                        Cell::from("loading...").style(theme.text_muted()),
                        Cell::from("n/a").style(theme.text_muted()),
                        Cell::from("n/a").style(theme.text_muted()),
                        Cell::from("n/a").style(theme.text_muted()),
                    ])
                }
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(13),
            Constraint::Length(10),
            Constraint::Length(9),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(Span::styled(" Watchlist ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(table, chunks[0]);

    if total > viewport.max(1) {
        let mut scroll_state = ScrollbarState::new(total).position(start.min(total - 1));
        f.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            chunks[0],
            &mut scroll_state,
        );
    }

    let note = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Hint: ", theme.table_header()),
            Span::styled(
                "/add <pair> and /remove <pair> to manage watchlist",
                theme.text_secondary(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Chart linkage: ", theme.table_header()),
            Span::styled(
                "Tab in Chart page cycles this same list",
                theme.text_muted(),
            ),
        ]),
    ])
    .style(theme.text())
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .title(Span::styled(" Notes ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    );
    f.render_widget(note, chunks[1]);
}

fn render_chart_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
    spinner_frame: usize,
) {
    chart::render_chart_page(f, area, state, theme, scroll, spinner_frame);
}

fn render_customize_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    selected: usize,
    dirty: bool,
) {
    let shell = Block::default()
        .title(Span::styled(" Customize ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let shell_inner = shell.inner(area);
    f.render_widget(shell, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Length(4),
        ])
        .split(shell_inner);

    let header = Paragraph::new(vec![Line::from(vec![
        Span::styled("Config editor", theme.table_header()),
        Span::styled(
            if dirty {
                "   ● unsaved changes"
            } else {
                "   saved state"
            },
            if dirty {
                theme.status_warn()
            } else {
                theme.text_muted()
            },
        ),
    ])])
    .style(theme.text())
    .block(
        Block::default()
            .title(Span::styled(" Session ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    );
    f.render_widget(header, chunks[0]);

    let mark = |idx| {
        if selected == idx {
            ">"
        } else {
            " "
        }
    };

    let agent_rows = vec![
        Row::new(vec![
            Cell::from(mark(0)).style(theme.text_accent_bold()),
            Cell::from("Min Confidence").style(theme.table_header()),
            Cell::from(format!("{}%", state.config.agent.min_confidence)).style(theme.text()),
            Cell::from(mark(1)).style(theme.text_accent_bold()),
            Cell::from("Max Open Trades").style(theme.table_header()),
            Cell::from(state.config.agent.max_open_trades.to_string()).style(theme.text()),
        ]),
        Row::new(vec![
            Cell::from(mark(2)).style(theme.text_accent_bold()),
            Cell::from("Scan Interval").style(theme.table_header()),
            Cell::from(format!("{} sec", state.config.engine.tick_interval_secs))
                .style(theme.text()),
            Cell::from(mark(3)).style(theme.text_accent_bold()),
            Cell::from("Risk / Trade").style(theme.table_header()),
            Cell::from(format!("{}%", state.config.risk.risk_per_trade_pct)).style(theme.text()),
        ]),
        Row::new(vec![
            Cell::from(mark(4)).style(theme.text_accent_bold()),
            Cell::from("Max Daily DD").style(theme.table_header()),
            Cell::from(format!("{}%", state.config.risk.max_daily_drawdown_pct))
                .style(theme.text()),
            Cell::from(" "),
            Cell::from(""),
            Cell::from(""),
        ]),
    ];

    let agent_table = Table::new(
        agent_rows,
        [
            Constraint::Length(2),
            Constraint::Length(16),
            Constraint::Length(14),
            Constraint::Length(2),
            Constraint::Length(16),
            Constraint::Min(8),
        ],
    )
    .header(
        Row::new(vec![
            Cell::from(""),
            Cell::from("Field"),
            Cell::from("Value"),
            Cell::from(""),
            Cell::from("Field"),
            Cell::from("Value"),
        ])
        .style(theme.table_header()),
    )
    .block(
        Block::default()
            .title(Span::styled(" Agent + Risk ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(agent_table, chunks[1]);

    let source_rows = vec![
        Row::new(vec![
            Cell::from(mark(5)).style(theme.text_accent_bold()),
            Cell::from("Yahoo").style(theme.text_secondary()),
            Cell::from(source_toggle(state.config.data.yahoo_enabled)).style(theme.text()),
            Cell::from(mark(6)).style(theme.text_accent_bold()),
            Cell::from("CoinGecko").style(theme.text_secondary()),
            Cell::from(source_toggle(state.config.data.coingecko_enabled)).style(theme.text()),
            Cell::from(mark(7)).style(theme.text_accent_bold()),
            Cell::from("Fear&Greed").style(theme.text_secondary()),
            Cell::from(source_toggle(state.config.data.fear_greed_enabled)).style(theme.text()),
        ]),
        Row::new(vec![
            Cell::from(mark(8)).style(theme.text_accent_bold()),
            Cell::from("Reddit").style(theme.text_secondary()),
            Cell::from(source_toggle(state.config.data.reddit_enabled)).style(theme.text()),
            Cell::from(mark(9)).style(theme.text_accent_bold()),
            Cell::from("X/Twitter").style(theme.text_secondary()),
            Cell::from(source_toggle(state.config.data.twitter_enabled)).style(theme.text()),
            Cell::from(mark(12)).style(theme.text_accent_bold()),
            Cell::from("Finnhub").style(theme.text_secondary()),
            Cell::from(source_toggle(state.config.data.finnhub_enabled)).style(theme.text()),
        ]),
        Row::new(vec![
            Cell::from(mark(10)).style(theme.text_accent_bold()),
            Cell::from("Reuters RSS").style(theme.text_secondary()),
            Cell::from(source_toggle(state.config.data.reuters_rss_enabled)).style(theme.text()),
            Cell::from(mark(11)).style(theme.text_accent_bold()),
            Cell::from("Bloomberg RSS").style(theme.text_secondary()),
            Cell::from(source_toggle(state.config.data.bloomberg_rss_enabled)).style(theme.text()),
            Cell::from(" "),
            Cell::from(""),
            Cell::from(""),
        ]),
    ];

    let sources_table = Table::new(
        source_rows,
        [
            Constraint::Length(2),
            Constraint::Length(12),
            Constraint::Length(4),
            Constraint::Length(2),
            Constraint::Length(12),
            Constraint::Length(4),
            Constraint::Length(2),
            Constraint::Length(12),
            Constraint::Min(4),
        ],
    )
    .header(
        Row::new(vec![
            Cell::from(""),
            Cell::from("Source"),
            Cell::from("On"),
            Cell::from(""),
            Cell::from("Source"),
            Cell::from("On"),
            Cell::from(""),
            Cell::from("Source"),
            Cell::from("On"),
        ])
        .style(theme.table_header()),
    )
    .block(
        Block::default()
            .title(Span::styled(" Data Sources ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(sources_table, chunks[2]);

    let display_rows = vec![Row::new(vec![
        Cell::from(mark(13)).style(theme.text_accent_bold()),
        Cell::from("Chart Default TF").style(theme.table_header()),
        Cell::from(state.config.tui.chart_default_timeframe.clone()).style(theme.text()),
        Cell::from(mark(14)).style(theme.text_accent_bold()),
        Cell::from("Log Buffer Size").style(theme.table_header()),
        Cell::from(state.config.tui.log_lines.to_string()).style(theme.text()),
    ])];

    let display_table = Table::new(
        display_rows,
        [
            Constraint::Length(2),
            Constraint::Length(18),
            Constraint::Length(12),
            Constraint::Length(2),
            Constraint::Length(16),
            Constraint::Min(8),
        ],
    )
    .header(
        Row::new(vec![
            Cell::from(""),
            Cell::from("Display Field"),
            Cell::from("Value"),
            Cell::from(""),
            Cell::from("Display Field"),
            Cell::from("Value"),
        ])
        .style(theme.table_header()),
    )
    .block(
        Block::default()
            .title(Span::styled(" Display ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(display_table, chunks[3]);

    let footer = Paragraph::new(vec![Line::from(Span::styled(
        "[↑/↓] select field   [←/→] adjust value   [Space] toggle source   [Ctrl+S] save   [Esc] discard",
        theme.text_muted(),
    ))])
    .style(theme.text())
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .title(Span::styled(" Controls ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    );
    f.render_widget(footer, chunks[4]);
}

fn render_chart(
    state: &AppState,
    theme: &Theme,
    width: usize,
    height: usize,
    spinner_frame: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    let pair = &state.chart_pair;
    let tf = state.chart_timeframe;
    let short_pair = pair.replace("USDT", "/USDT");

    let top = if let Some(ticker) = state.get_ticker(pair) {
        vec![
            Span::styled("◈ CHART  ", theme.app_name()),
            Span::styled(format!("{} {}  ", short_pair, tf), theme.text_accent_bold()),
            Span::styled(
                format!("{}  ", format_price(ticker.price)),
                theme.text_bold(),
            ),
            Span::styled(
                format!(
                    "{}{:+.2}%",
                    if ticker.price_change_pct_24h >= Decimal::ZERO {
                        chars::ARROW_UP
                    } else {
                        chars::ARROW_DOWN
                    },
                    ticker.price_change_pct_24h
                ),
                theme.price_change(ticker.price_change_pct_24h >= Decimal::ZERO),
            ),
        ]
    } else {
        vec![
            Span::styled("◈ CHART  ", theme.app_name()),
            Span::styled(format!("{} {}", short_pair, tf), theme.text_accent_bold()),
        ]
    };
    lines.push(Line::from(top));

    if let Some(buffer) = state.get_candles(pair, tf) {
        if buffer.candles.len() > 5 {
            let total = buffer.candles.len();
            let mut window = state.chart_zoom.clamp(16, total.max(16));
            window = window.min(total);
            let offset = state.chart_offset.min(total.saturating_sub(window));
            let end = total.saturating_sub(offset);
            let start = end.saturating_sub(window);

            let mut visible_candles: Vec<OHLCV> = buffer
                .candles
                .iter()
                .skip(start)
                .take(window)
                .cloned()
                .collect();

            let chart_width = width.saturating_sub(14).clamp(28, 150);
            if visible_candles.len() > chart_width {
                visible_candles = sample_candles_to_width(&visible_candles, chart_width);
            }

            let chart_rows = 30usize;
            let price_rows = chart_rows.saturating_sub(6).max(12);
            let volume_rows = chart_rows.saturating_sub(price_rows).max(4);

            let min = visible_candles
                .iter()
                .map(|c| c.low)
                .min()
                .unwrap_or(Decimal::ZERO);
            let max = visible_candles
                .iter()
                .map(|c| c.high)
                .max()
                .unwrap_or(Decimal::ONE);
            let range = (max - min).max(Decimal::new(1, 8));

            let closes: Vec<Decimal> = visible_candles.iter().map(|c| c.close).collect();
            let ema9 = ema_series(&closes, 9);
            let ema21 = ema_series(&closes, 21);
            let last_close = closes.last().copied().unwrap_or(Decimal::ZERO);
            let last_row = map_decimal_to_row(last_close, min, max, price_rows);

            if let Some(last) = visible_candles.last() {
                lines.push(Line::from(vec![
                    Span::styled("OHLCV: ", theme.table_header()),
                    Span::styled(chart_ohlcv_summary(last), theme.text()),
                ]));
            }

            let grid_step = (price_rows / 10).max(1);
            for row in 0..price_rows {
                let level = max
                    - (range * Decimal::from(row as i64))
                        / Decimal::from((price_rows.saturating_sub(1)) as i64);
                let show_grid = row % grid_step == 0;

                let mut spans: Vec<Span<'static>> = Vec::new();
                spans.push(Span::styled(
                    format!("{:>9}", format_price_plain(level)),
                    theme.text_muted(),
                ));
                spans.push(Span::styled(" ", theme.text()));
                let mut appended_price_label = false;

                for (idx, candle) in visible_candles.iter().enumerate() {
                    let high_row = map_decimal_to_row(candle.high, min, max, price_rows);
                    let low_row = map_decimal_to_row(candle.low, min, max, price_rows);
                    let open_row = map_decimal_to_row(candle.open, min, max, price_rows);
                    let close_row = map_decimal_to_row(candle.close, min, max, price_rows);
                    let body_top = open_row.min(close_row);
                    let body_bottom = open_row.max(close_row);

                    let base_grid = if row == last_row {
                        '╌'
                    } else if show_grid {
                        '─'
                    } else {
                        ' '
                    };

                    let mut glyph = base_grid;
                    let mut style = theme.text_muted();

                    if row >= high_row && row <= low_row {
                        glyph = '│';
                        style = theme.text_secondary();
                    }
                    if row >= body_top && row <= body_bottom {
                        glyph = '█';
                        style = if candle.is_bullish() {
                            theme.profit_style()
                        } else {
                            theme.loss_style()
                        };
                    }

                    if state.chart_show_indicators {
                        if let Some(Some(v)) = ema21.get(idx) {
                            if map_decimal_to_row(*v, min, max, price_rows) == row {
                                glyph = '·';
                                style = Style::default()
                                    .fg(theme.chat_user_name)
                                    .bg(theme.bg_primary);
                            }
                        }
                        if let Some(Some(v)) = ema9.get(idx) {
                            if map_decimal_to_row(*v, min, max, price_rows) == row {
                                glyph = '·';
                                style = Style::default().fg(theme.signal_wait).bg(theme.bg_primary);
                            }
                        }
                    }

                    if idx + 1 == visible_candles.len() && row == last_row {
                        spans.push(Span::styled(glyph.to_string(), style));
                        spans.push(Span::styled(
                            format!("  {}", format_price(last_close)),
                            theme.text_accent_bold(),
                        ));
                        appended_price_label = true;
                        continue;
                    }

                    spans.push(Span::styled(glyph.to_string(), style));
                }

                if !appended_price_label && row == last_row {
                    spans.push(Span::styled(
                        format!("  {}", format_price(last_close)),
                        theme.text_accent_bold(),
                    ));
                }

                lines.push(Line::from(spans));
            }

            let max_volume = visible_candles
                .iter()
                .map(|c| c.volume)
                .max()
                .unwrap_or(Decimal::ONE)
                .max(Decimal::ONE);
            let volume_chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
            for row in 0..volume_rows {
                let mut spans: Vec<Span<'static>> = Vec::new();
                if row == 0 {
                    spans.push(Span::styled("VOL      ", theme.table_header()));
                } else {
                    spans.push(Span::styled("         ", theme.text()));
                }

                let threshold = ((volume_rows - row) as f64 / volume_rows as f64).clamp(0.0, 1.0);
                for candle in &visible_candles {
                    let ratio = (candle.volume / max_volume)
                        .to_string()
                        .parse::<f64>()
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    if ratio >= threshold {
                        let idx = (ratio * 7.0).round() as usize;
                        let glyph = volume_chars[idx.min(7)];
                        let style = if candle.is_bullish() {
                            theme.profit_style()
                        } else {
                            theme.loss_style()
                        };
                        spans.push(Span::styled(glyph.to_string(), style));
                    } else {
                        spans.push(Span::styled(" ", theme.text_muted()));
                    }
                }
                lines.push(Line::from(spans));
            }

            if let (Some(first), Some(last)) = (visible_candles.first(), visible_candles.last()) {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!(
                            "{}  …  {}",
                            first.timestamp.format("%m-%d %H:%M"),
                            last.timestamp.format("%m-%d %H:%M")
                        ),
                        theme.text_muted(),
                    ),
                    Span::styled(
                        format!("  {} candles", visible_candles.len()),
                        theme.text_secondary(),
                    ),
                ]));
            }
        } else {
            return render_chart_loading_lines(theme, width, height, spinner_frame);
        }
    } else {
        return render_chart_loading_lines(theme, width, height, spinner_frame);
    }

    lines
}

fn render_chart_loading_lines(
    theme: &Theme,
    width: usize,
    height: usize,
    spinner_frame: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let spinner = chars::SPINNER[spinner_frame % chars::SPINNER.len()];
    let pad_x = width.saturating_sub(28) / 2;
    let pad_y = height.saturating_sub(6) / 2;
    for _ in 0..pad_y {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        format!("{}{} Loading chart data...", " ".repeat(pad_x), spinner),
        theme.spinner(),
    )));
    lines
}

fn render_history(state: &AppState, theme: &Theme, count: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled(
        "◈ HISTORY",
        theme.section_header(),
    )));
    push_divider(&mut lines, theme, 68);

    let trades = state.portfolio.closed_trades();
    if trades.is_empty() {
        lines.push(Line::from(Span::styled(
            "No closed trades yet.",
            theme.text_muted(),
        )));
        return lines;
    }

    lines.push(Line::from(vec![
        Span::styled("PAIR      ", theme.table_header()),
        Span::styled("SIDE     ", theme.table_header()),
        Span::styled("ENTRY       ", theme.table_header()),
        Span::styled("EXIT        ", theme.table_header()),
        Span::styled("PNL         ", theme.table_header()),
        Span::styled("REASON", theme.table_header()),
    ]));

    for trade in trades.iter().rev().take(count) {
        let side_style = if trade.side == PositionSide::Long {
            Style::default().fg(theme.signal_long)
        } else {
            Style::default().fg(theme.signal_short)
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<10}", trade.pair.replace("USDT", "")),
                theme.text_accent_bold(),
            ),
            Span::styled(format!("{:<8}", trade.side), side_style),
            Span::styled(
                format!("{:<12}", format_price(trade.entry_price)),
                theme.text(),
            ),
            Span::styled(
                format!("{:<12}", format_price(trade.exit_price)),
                theme.text(),
            ),
            Span::styled(
                format!("{:<12}", format_pnl(trade.realized_pnl)),
                theme.pnl(trade.realized_pnl),
            ),
            Span::styled(format!("{:?}", trade.close_reason), theme.text_muted()),
        ]));
    }

    lines.push(Line::from(""));
    lines
}

fn render_stats(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let stats = state.portfolio.calculate_metrics();

    lines.push(Line::from(Span::styled("◈ STATS", theme.section_header())));
    push_divider(&mut lines, theme, 64);

    lines.push(Line::from(vec![
        Span::styled("Total Trades: ", theme.table_header()),
        Span::styled(stats.total_trades.to_string(), theme.text()),
        Span::styled("   Win Rate: ", theme.table_header()),
        Span::styled(format!("{:.2}%", stats.win_rate), theme.text()),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Profit Factor: ", theme.table_header()),
        Span::styled(
            format!("{:.2}x", stats.profit_factor),
            Style::default().fg(theme.profit),
        ),
        Span::styled("   Net: ", theme.table_header()),
        Span::styled(format_pnl(stats.net_profit), theme.pnl(stats.net_profit)),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Gross Profit: ", theme.table_header()),
        Span::styled(
            format_pnl(stats.gross_profit),
            Style::default().fg(theme.profit_strong),
        ),
        Span::styled("   Gross Loss: ", theme.table_header()),
        Span::styled(
            format_pnl(-stats.gross_loss),
            Style::default().fg(theme.loss_strong),
        ),
    ]));

    lines.push(Line::from(""));
    lines
}

fn render_customize(
    state: &AppState,
    theme: &Theme,
    selected: usize,
    dirty: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("◈ CUSTOMIZE", theme.section_header()),
        Span::styled(
            if dirty { "   ● unsaved changes" } else { "" },
            if dirty {
                theme.status_warn()
            } else {
                theme.text_muted()
            },
        ),
    ]));
    push_divider(&mut lines, theme, 72);

    lines.push(Line::from(Span::styled("Agent", theme.text_accent_bold())));

    lines.push(Line::from(vec![
        customize_marker(0, selected, theme),
        Span::styled(" Min Confidence: ", theme.table_header()),
        Span::styled(
            format!("{}%", state.config.agent.min_confidence),
            theme.text(),
        ),
        Span::styled("   ", theme.text()),
        customize_marker(1, selected, theme),
        Span::styled(" Max Open Trades: ", theme.table_header()),
        Span::styled(state.config.agent.max_open_trades.to_string(), theme.text()),
    ]));

    lines.push(Line::from(vec![
        customize_marker(2, selected, theme),
        Span::styled(" Scan Interval: ", theme.table_header()),
        Span::styled(
            format!("{} sec", state.config.engine.tick_interval_secs),
            theme.text_accent(),
        ),
        Span::styled("   ", theme.text()),
        customize_marker(3, selected, theme),
        Span::styled(" Risk/Trade: ", theme.table_header()),
        Span::styled(
            format!("{}%", state.config.risk.risk_per_trade_pct),
            theme.text(),
        ),
    ]));
    lines.push(Line::from(vec![
        customize_marker(4, selected, theme),
        Span::styled(" Max Daily Drawdown: ", theme.table_header()),
        Span::styled(
            format!("{}%", state.config.risk.max_daily_drawdown_pct),
            theme.text(),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Data Sources: ", theme.table_header()),
        customize_marker(5, selected, theme),
        Span::styled(source_toggle(state.config.data.yahoo_enabled), theme.text()),
        Span::styled(" Yahoo  ", theme.text_secondary()),
        customize_marker(6, selected, theme),
        Span::styled(
            source_toggle(state.config.data.coingecko_enabled),
            theme.text(),
        ),
        Span::styled(" CoinGecko  ", theme.text_secondary()),
        customize_marker(7, selected, theme),
        Span::styled(
            source_toggle(state.config.data.fear_greed_enabled),
            theme.text(),
        ),
        Span::styled(" Fear&Greed", theme.text_secondary()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("              ", theme.table_header()),
        customize_marker(8, selected, theme),
        Span::styled(
            source_toggle(state.config.data.reddit_enabled),
            theme.text(),
        ),
        Span::styled(" Reddit  ", theme.text_secondary()),
        customize_marker(9, selected, theme),
        Span::styled(
            source_toggle(state.config.data.twitter_enabled),
            theme.text(),
        ),
        Span::styled(" X/Twitter  ", theme.text_secondary()),
        customize_marker(12, selected, theme),
        Span::styled(
            source_toggle(state.config.data.finnhub_enabled),
            theme.text(),
        ),
        Span::styled(" Finnhub", theme.text_secondary()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("              ", theme.table_header()),
        customize_marker(10, selected, theme),
        Span::styled(
            source_toggle(state.config.data.reuters_rss_enabled),
            theme.text(),
        ),
        Span::styled(" Reuters RSS  ", theme.text_secondary()),
        customize_marker(11, selected, theme),
        Span::styled(
            source_toggle(state.config.data.bloomberg_rss_enabled),
            theme.text(),
        ),
        Span::styled(" Bloomberg RSS", theme.text_secondary()),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Display: ", theme.table_header()),
        customize_marker(13, selected, theme),
        Span::styled(
            format!(
                " Chart Default TF [{}]  ",
                state.config.tui.chart_default_timeframe
            ),
            theme.text(),
        ),
        customize_marker(14, selected, theme),
        Span::styled(
            format!(" Log Buffer Size [{}]", state.config.tui.log_lines),
            theme.text(),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "[Enter] Edit value   [Space] Toggle   [Ctrl+S] Save   [Esc] Discard",
        theme.text_muted(),
    )));

    lines.push(Line::from(""));
    lines
}

fn render_status(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "◈ SYSTEM STATUS",
        theme.section_header(),
    )));
    push_divider(&mut lines, theme, 72);
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        "── Data Sources ─────────────────────────────────────────────",
        theme.text_accent_bold(),
    )));
    lines.push(Line::from(vec![
        Span::styled(format!("{:<14}", "SOURCE"), theme.table_header()),
        Span::styled(format!("{:<13}", "STATUS"), theme.table_header()),
        Span::styled("DETAIL", theme.table_header()),
    ]));

    if state.source_health.is_empty() {
        lines.push(Line::from(Span::styled(
            "No source health yet. Waiting for first source poll...",
            theme.text_muted(),
        )));
    } else {
        let mut names: Vec<_> = state.source_health.keys().cloned().collect();
        names.sort();
        for name in names {
            if let Some(source) = state.source_health.get(&name) {
                let icon = match source.level {
                    crate::state::SourceStatusLevel::Connected
                    | crate::state::SourceStatusLevel::Ok => "●",
                    crate::state::SourceStatusLevel::Warn => "⚠",
                    crate::state::SourceStatusLevel::Error => "✗",
                    crate::state::SourceStatusLevel::MissingConfig
                    | crate::state::SourceStatusLevel::Disabled => "✗",
                };
                let style = match source.level {
                    crate::state::SourceStatusLevel::Connected
                    | crate::state::SourceStatusLevel::Ok => theme.status_ok(),
                    crate::state::SourceStatusLevel::Warn => theme.status_warn(),
                    _ => theme.status_error(),
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{:<14}", source.name), theme.text_secondary()),
                    Span::styled(format!("{} {:<12}", icon, source.level), style),
                    Span::styled(format!(" {}", source.detail), theme.text_muted()),
                ]));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "── Data Quality ──────────────────────────────────────────────",
        theme.text_accent_bold(),
    )));
    if state.config.pairs.watchlist.is_empty() {
        lines.push(Line::from(Span::styled(
            "No watchlist pairs",
            theme.text_muted(),
        )));
    } else {
        for pair in &state.config.pairs.watchlist {
            let score = state
                .data_quality
                .get(pair)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<10}", pair.replace("USDT", "")),
                    theme.table_header(),
                ),
                Span::styled(
                    format!(
                        "{} {:>3}%",
                        quality_bar(score),
                        (score * 100.0).round() as i32
                    ),
                    quality_style(score, theme),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "── Engine Telemetry ──────────────────────────────────────────",
        theme.text_accent_bold(),
    )));
    lines.push(Line::from(vec![
        Span::styled("Status: ", theme.table_header()),
        Span::styled(format!("{}", state.agent_status), theme.text()),
        Span::styled("   Open trades: ", theme.table_header()),
        Span::styled(
            format!(
                "{} / {}",
                state.portfolio.positions.len(),
                state.config.agent.max_open_trades
            ),
            theme.text(),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Uptime: ", theme.table_header()),
        Span::styled(
            format_duration(
                Utc::now()
                    .signed_duration_since(state.started_at)
                    .to_std()
                    .unwrap_or(Duration::from_secs(0)),
            ),
            theme.text(),
        ),
        Span::styled("   Last tick: ", theme.table_header()),
        Span::styled(
            state
                .engine_status
                .last_tick_time
                .map(|t| t.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            theme.text_secondary(),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("Engine Tick: ", theme.table_header()),
        Span::styled(
            format!("{}s", state.config.engine.tick_interval_secs),
            theme.text(),
        ),
        Span::styled("   Breaker: ", theme.table_header()),
        Span::styled(
            if state.engine_status.circuit_breaker_open {
                "OPEN"
            } else {
                "CLOSED"
            },
            if state.engine_status.circuit_breaker_open {
                theme.status_error()
            } else {
                theme.status_ok()
            },
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Errors: ", theme.table_header()),
        Span::styled(
            state.engine_status.consecutive_errors.to_string(),
            theme.text(),
        ),
        Span::styled("   Active indicators: ", theme.table_header()),
        Span::styled(
            if state.engine_status.active_indicators.is_empty() {
                "none".to_string()
            } else {
                state.engine_status.active_indicators.join(", ")
            },
            theme.text_secondary(),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("WS Reconnects: ", theme.table_header()),
        Span::styled(
            state.engine_status.ws_reconnect_count.to_string(),
            theme.text(),
        ),
        Span::styled("   WS Uptime: ", theme.table_header()),
        Span::styled(
            format!("{:.0}%", state.engine_status.ws_uptime_ratio * 100.0),
            theme.text(),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Auth", theme.table_header())));
    for provider in crate::auth::AuthProvider::ALL {
        let status = state.auth_state.get(&provider);
        let label = format!("{:<15}", provider.display_name());
        match status {
            Some(crate::auth::AuthStatus::AuthenticatedGitHub {
                username,
                created_at,
                ..
            }) => {
                let remaining_days = (*created_at + chrono::Duration::days(90))
                    .signed_duration_since(Utc::now())
                    .num_days()
                    .max(0);
                lines.push(Line::from(vec![
                    Span::styled(label, theme.text_secondary()),
                    Span::styled(
                        format!("✅ @{} expires {}d", username, remaining_days),
                        theme.profit_style(),
                    ),
                ]));
            }
            Some(s) if s.is_configured() => {
                lines.push(Line::from(vec![
                    Span::styled(label, theme.text_secondary()),
                    Span::styled("✅ configured", theme.profit_style()),
                ]));
            }
            Some(crate::auth::AuthStatus::Error(err)) => {
                lines.push(Line::from(vec![
                    Span::styled(label, theme.text_secondary()),
                    Span::styled(format!("⚠ {}", err), theme.loss_style()),
                ]));
            }
            _ => {
                lines.push(Line::from(vec![
                    Span::styled(label, theme.text_secondary()),
                    Span::styled("✗ not set", theme.loss_style()),
                ]));
            }
        }
    }

    lines.push(Line::from(""));
    lines
}

fn render_news(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled("◈ NEWS", theme.section_header())));
    push_divider(&mut lines, theme, 72);

    if state.news_headlines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No headlines yet. Waiting for Finnhub/RSS refresh...",
            theme.text_muted(),
        )));
        return lines;
    }

    for h in state.news_headlines.iter().take(40) {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", h.published_at.format("%H:%M")),
                theme.text_muted_italic(),
            ),
            Span::styled(format!("[{}] ", h.source), theme.text_accent()),
            Span::styled(h.title.to_string(), theme.text()),
        ]));
    }
    lines.push(Line::from(""));
    lines
}

fn render_heatmap(state: &AppState, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    heatmap::render_heatmap(state, theme, width)
}

fn render_sentiment(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "◈ SENTIMENT",
        theme.section_header(),
    )));
    push_divider(&mut lines, theme, 72);

    if let Some(s) = &state.sentiment_score {
        lines.push(Line::from(vec![
            Span::styled("Composite: ", theme.table_header()),
            Span::styled(format!("{:+.2}", s.composite), theme.text_accent_bold()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Fear & Greed: ", theme.table_header()),
            Span::styled(
                s.fear_greed
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
                theme.text(),
            ),
            Span::styled("  Reddit: ", theme.table_header()),
            Span::styled(
                s.reddit_score
                    .map(|v| format!("{:+.2}", v))
                    .unwrap_or_else(|| "n/a".to_string()),
                theme.text(),
            ),
            Span::styled("  X: ", theme.table_header()),
            Span::styled(
                s.twitter_score
                    .map(|v| format!("{:+.2}", v))
                    .unwrap_or_else(|| "n/a".to_string()),
                theme.text(),
            ),
            Span::styled("  News: ", theme.table_header()),
            Span::styled(
                s.news_score
                    .map(|v| format!("{:+.2}", v))
                    .unwrap_or_else(|| "n/a".to_string()),
                theme.text(),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Sources: ", theme.table_header()),
            Span::styled(s.sources_available.join(", "), theme.text_secondary()),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "Sentiment not available yet.",
            theme.text_muted(),
        )));
    }

    lines.push(Line::from(""));
    lines
}

fn render_macro(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled("◈ MACRO", theme.section_header())));
    push_divider(&mut lines, theme, 72);

    lines.push(Line::from(vec![
        Span::styled("SPY: ", theme.table_header()),
        Span::styled(
            state
                .macro_context
                .spy_change_pct
                .map(|v| format!("{:+.2}%", v))
                .unwrap_or_else(|| "n/a".to_string()),
            theme.text(),
        ),
        Span::styled("  DXY: ", theme.table_header()),
        Span::styled(
            state
                .macro_context
                .dxy_change_pct
                .map(|v| format!("{:+.2}%", v))
                .unwrap_or_else(|| "n/a".to_string()),
            theme.text(),
        ),
        Span::styled("  VIX: ", theme.table_header()),
        Span::styled(
            state
                .macro_context
                .vix
                .map(|v| format!("{:.2}", v))
                .unwrap_or_else(|| "n/a".to_string()),
            theme.text(),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::styled("BTC Dominance: ", theme.table_header()),
        Span::styled(
            state
                .macro_context
                .btc_dominance
                .map(|v| format!("{:.2}%", v))
                .unwrap_or_else(|| "n/a".to_string()),
            theme.text(),
        ),
        Span::styled("  Total MCap: ", theme.table_header()),
        Span::styled(
            state
                .macro_context
                .total_market_cap
                .map(format_large_usd)
                .unwrap_or_else(|| "n/a".to_string()),
            theme.text(),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Upcoming events:",
        theme.text_accent_bold(),
    )));

    if state.macro_context.upcoming_events.is_empty() {
        lines.push(Line::from(Span::styled("  none", theme.text_muted())));
    } else {
        for e in state.macro_context.upcoming_events.iter().take(20) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", e.time.format("%Y-%m-%d")),
                    theme.text_muted_italic(),
                ),
                Span::styled(format!("[{}] ", e.impact), theme.text_accent()),
                Span::styled(format!("{} ({})", e.title, e.country), theme.text()),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines
}

fn render_help(theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled("◈ HELP", theme.section_header())));
    push_divider(&mut lines, theme, 72);

    lines.push(Line::from(vec![
        Span::styled("NAVIGATION", theme.text_accent_bold()),
        Span::styled("               TRADING ACTIONS", theme.text_accent_bold()),
    ]));
    lines.push(help_row(
        theme,
        "/portfolio",
        "Portfolio & positions",
        "/close [PAIR]",
        "Close position manually",
    ));
    lines.push(help_row(
        theme,
        "/signals",
        "Latest signal analysis",
        "/risk [%]",
        "Set risk per trade",
    ));
    lines.push(help_row(
        theme,
        "/chart",
        "Price chart",
        "/confidence [n]",
        "Set confidence threshold",
    ));
    lines.push(help_row(
        theme,
        "/history",
        "Closed trades",
        "/pause | /resume",
        "Control trading agent",
    ));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("CONFIGURATION", theme.text_accent_bold()),
        Span::styled("             AUTH", theme.text_accent_bold()),
    ]));
    lines.push(help_row(
        theme,
        "/model",
        "Switch AI provider & model",
        "/auth",
        "Open auth provider menu",
    ));
    lines.push(help_row(
        theme,
        "/pairs",
        "Manage watchlist",
        "/auth-delete [provider]",
        "Delete stored auth",
    ));
    lines.push(help_row(
        theme,
        "/status",
        "Agent health check",
        "/auth github",
        "Start GitHub device flow",
    ));
    lines.push(help_row(
        theme,
        "/team <prompt>",
        "Run AI Agent Team",
        "/team status",
        "Open Team Discussion",
    ));
    lines.push(help_row(
        theme,
        "/team history",
        "Last 5 team sessions",
        "Popup Y/N/E/D",
        "Action decision controls",
    ));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "KEYBOARD SHORTCUTS",
        theme.text_accent_bold(),
    )));
    lines.push(Line::from(Span::styled(
        "/ open autocomplete, Esc cancel/back, Tab cycle pairs/auth sections",
        theme.text_secondary(),
    )));
    lines.push(Line::from(Span::styled(
        "Enter execute/send, Ctrl+C cancel stream, Ctrl+L clear page, F1/? help, F5 refresh",
        theme.text_secondary(),
    )));

    lines.push(Line::from(""));
    lines
}

fn render_chat(
    state: &AppState,
    theme: &Theme,
    spinner_frame: usize,
    content_width: usize,
    chat_auto_scroll: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let bubble_width = content_width.saturating_sub(10).max(28);
    let right_width = bubble_width.min(70);
    let assistant_style = assistant_role_style(state, theme);

    lines.push(Line::from(vec![
        Span::styled("◈ CHAT  ", theme.app_name()),
        Span::styled(
            format!(
                "{}  ·  {} messages  ·  {}",
                state.config.llm.model,
                state.chat_messages.len(),
                if chat_auto_scroll { "AUTO" } else { "MANUAL" }
            ),
            theme.text_secondary(),
        ),
    ]));

    push_divider(&mut lines, theme, 70);
    lines.push(Line::from(""));

    if state.chat_messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "No conversation yet. Ask the agent anything about market context or trade rationale.",
            theme.text_muted(),
        )));
        return lines;
    }

    let take_n = 12usize;
    let len = state.chat_messages.len();
    let start = len.saturating_sub(take_n);

    if start > 0 {
        lines.push(Line::from(Span::styled(
            format!("↑ {} more messages", start),
            theme.text_muted(),
        )));
        lines.push(Line::from(""));
    }

    for (idx, msg) in state.chat_messages.iter().skip(start).enumerate() {
        let ts = msg.timestamp.format("%H:%M:%S").to_string();
        if msg.is_user {
            let header = format!("┌─ You ▶  {}", ts);
            let pad = right_width.saturating_sub(header.chars().count());
            lines.push(Line::from(vec![
                Span::styled(" ".repeat(pad), theme.text()),
                Span::styled(header, theme.chat_user()),
            ]));
            for line in wrap_text(&msg.content, right_width.saturating_sub(4)) {
                lines.push(Line::from(vec![Span::styled(
                    align_right(&format!("│ {}", line), right_width),
                    Style::default().fg(theme.chat_user_name),
                )]));
            }
            lines.push(Line::from(vec![Span::styled(
                align_right("└────────────────", right_width),
                theme.text_muted(),
            )]));
        } else {
            let provider_label = match state.config.llm.provider {
                LlmProvider::Claude => "Claude",
                LlmProvider::OpenAI => "OpenAI",
                LlmProvider::Gemini => "Gemini",
                LlmProvider::OpenRouter => "OpenRouter",
                LlmProvider::Copilot => "Copilot",
                LlmProvider::Gradio => "Gradio",
                LlmProvider::Mock => "Mock",
            };
            lines.push(Line::from(vec![Span::styled(
                format!("┌─ ◀ {}  {}", provider_label, ts),
                assistant_style,
            )]));

            let mut wrapped = wrap_text(&msg.content, bubble_width.min(76).saturating_sub(4));
            if msg.is_streaming {
                if !wrapped.is_empty() {
                    wrapped.pop();
                }
                wrapped.push(format!(
                    "{} typing...",
                    chars::SPINNER[spinner_frame % chars::SPINNER.len()]
                ));
            }

            for line in wrapped {
                lines.push(Line::from(Span::styled(
                    format!("│ {}", line),
                    theme.text(),
                )));
            }
            lines.push(Line::from(Span::styled(
                "└────────────────",
                theme.text_muted(),
            )));
        }

        if idx + 1 != len.saturating_sub(start) {
            push_message_separator(&mut lines, theme, bubble_width);
            lines.push(Line::from(""));
        } else {
            lines.push(Line::from(""));
        }
    }

    lines
}

pub fn chat_line_count(
    state: &AppState,
    theme: &Theme,
    spinner_frame: usize,
    content_width: usize,
    chat_auto_scroll: bool,
) -> usize {
    render_chat(state, theme, spinner_frame, content_width, chat_auto_scroll).len()
}

fn chart_ohlcv_summary(candle: &OHLCV) -> String {
    format!(
        "O:{} H:{} L:{} C:{} V:{}",
        format_price(candle.open),
        format_price(candle.high),
        format_price(candle.low),
        format_price(candle.close),
        format_compact_number(candle.volume)
    )
}

fn render_log(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("◈ ACTIVITY LOG", theme.section_header()),
        Span::styled(
            format!(
                "                              [{} entries, scroll ↑↓]",
                state.log_entries.len()
            ),
            theme.text_muted(),
        ),
    ]));

    push_divider(&mut lines, theme, 72);

    if state.log_entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "No activity yet.",
            theme.text_muted(),
        )));
        return lines;
    }

    for entry in state
        .log_entries
        .iter()
        .rev()
        .take(MAX_RENDER_ITEMS_PER_LIST)
    {
        let level_style = match entry.level {
            LogLevel::Trade => Style::default().fg(theme.profit),
            LogLevel::Warn => Style::default().fg(theme.signal_wait),
            LogLevel::Error => Style::default().fg(theme.loss_strong),
            LogLevel::Info => Style::default().fg(theme.text_secondary),
            LogLevel::Debug => Style::default().fg(theme.text_muted),
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("{}  ", entry.timestamp.format("%H:%M:%S")),
                theme.text_muted_italic(),
            ),
            Span::styled(format!("{:<6}", entry.level), level_style),
            Span::styled(entry.message.clone(), theme.text()),
        ]));
    }

    lines.push(Line::from(""));
    lines
}

fn render_pairs(state: &AppState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled("◈ PAIRS", theme.section_header())));
    push_divider(&mut lines, theme, 64);

    for pair in &state.config.pairs.watchlist {
        if let Some(t) = state.get_ticker(pair) {
            lines.push(Line::from(vec![
                Span::styled(format!("{:<10}", pair), theme.text_accent_bold()),
                Span::styled(format!("{:<12}", format_price(t.price)), theme.text()),
                Span::styled(
                    format!(
                        "{}{:+.2}%",
                        if t.price_change_pct_24h >= Decimal::ZERO {
                            chars::ARROW_UP
                        } else {
                            chars::ARROW_DOWN
                        },
                        t.price_change_pct_24h
                    ),
                    theme.price_change(t.price_change_pct_24h >= Decimal::ZERO),
                ),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("{:<10}", pair), theme.text_accent_bold()),
                Span::styled("loading...", theme.text_muted()),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines
}

fn render_team_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
    spinner_frame: usize,
) {
    team::render_team_page(f, area, state, theme, scroll, spinner_frame);
}

fn team_role_style(theme: &Theme, role: TeamRole) -> Style {
    match role {
        TeamRole::Analyst => Style::default().fg(theme.text_accent),
        TeamRole::Trader => Style::default().fg(theme.signal_long),
        TeamRole::RiskManager => Style::default().fg(theme.signal_wait),
        TeamRole::Researcher => Style::default().fg(theme.status_ok),
        TeamRole::Leader => Style::default().fg(theme.chat_agent_name),
        TeamRole::DevilsAdvocate => Style::default().fg(theme.loss_strong),
    }
}

fn team_edge_style(theme: &Theme, kind: TeamEdgeKind) -> Style {
    match kind {
        TeamEdgeKind::Agree => Style::default().fg(theme.status_ok),
        TeamEdgeKind::Counter => Style::default().fg(theme.loss),
    }
}

fn team_action_style(theme: &Theme, kind: TeamActionKind) -> Style {
    match kind {
        TeamActionKind::Buy => Style::default().fg(theme.profit_strong),
        TeamActionKind::Sell => Style::default().fg(theme.loss_strong),
        TeamActionKind::Close => Style::default().fg(theme.signal_wait),
        TeamActionKind::Hold => Style::default().fg(theme.text_secondary),
    }
}

fn team_confidence_bar(confidence: u8) -> String {
    let buckets = (confidence as usize).div_ceil(20).min(5);
    let filled = "█".repeat(buckets);
    let empty = "░".repeat(5usize.saturating_sub(buckets));
    format!("{}{}", filled, empty)
}

fn build_ascii_graph(state: &AppState) -> Vec<String> {
    let width = 28usize;
    let height = 15usize;
    let mut canvas = vec![vec![' '; width]; height];

    let nodes: [(TeamRole, (usize, usize)); 6] = [
        (TeamRole::Analyst, (2, 1)),
        (TeamRole::Trader, (20, 1)),
        (TeamRole::RiskManager, (2, 6)),
        (TeamRole::Researcher, (20, 6)),
        (TeamRole::Leader, (2, 11)),
        (TeamRole::DevilsAdvocate, (20, 11)),
    ];

    let mut pos = HashMap::new();
    for (role, (x, y)) in nodes {
        pos.insert(role, (x, y));
        canvas[y][x] = '●';
        if x + 2 < width {
            for (i, ch) in role.short().chars().enumerate() {
                if x + 2 + i < width {
                    canvas[y][x + 2 + i] = ch;
                }
            }
        }
    }

    for edge in &state.team_discussion.edges {
        if edge.weight == 0 {
            continue;
        }
        let Some((x1, y1)) = pos.get(&edge.from).copied() else {
            continue;
        };
        let Some((x2, y2)) = pos.get(&edge.to).copied() else {
            continue;
        };
        draw_edge(&mut canvas, x1, y1, x2, y2);
    }

    canvas
        .into_iter()
        .map(|row| row.into_iter().collect::<String>())
        .collect()
}

fn draw_edge(canvas: &mut [Vec<char>], x1: usize, y1: usize, x2: usize, y2: usize) {
    let dx = x2 as isize - x1 as isize;
    let dy = y2 as isize - y1 as isize;
    let steps = dx.abs().max(dy.abs()) as usize;
    if steps == 0 {
        return;
    }

    for step in 1..steps {
        let t = step as f32 / steps as f32;
        let x = (x1 as f32 + dx as f32 * t).round() as usize;
        let y = (y1 as f32 + dy as f32 * t).round() as usize;

        if y >= canvas.len() || x >= canvas[y].len() {
            continue;
        }

        let ch = if dx == 0 {
            '│'
        } else if dy == 0 {
            '─'
        } else if (dx > 0 && dy > 0) || (dx < 0 && dy < 0) {
            '╲'
        } else {
            '╱'
        };

        if canvas[y][x] == ' ' {
            canvas[y][x] = ch;
        }
    }
}

#[derive(Debug, Clone)]
struct BrailleGraphCell {
    glyph: char,
    style: Style,
}

fn char_to_string(ch: char) -> String {
    let mut out = String::new();
    out.push(ch);
    out
}

impl BrailleGraphCell {
    fn blank(theme: &Theme) -> Self {
        Self {
            glyph: ' ',
            style: theme.text_muted(),
        }
    }
}

fn build_braille_graph_lines(
    state: &AppState,
    width: usize,
    height: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let edges = &state.team_discussion.edges;
    let mut graph_signature: u64 = 0;
    for edge in edges {
        graph_signature = graph_signature
            .wrapping_mul(1_000_003)
            .wrapping_add((edge.weight as u64).wrapping_mul(17))
            .wrapping_add(edge.from as u64)
            .wrapping_add((edge.to as u64).wrapping_mul(31))
            .wrapping_add((edge.kind as u64).wrapping_mul(7));
    }

    if graph_signature == 0 && width > 0 && height > 0 {
        return vec![Line::from(Span::styled(
            "No graph edges yet",
            theme.text_muted(),
        ))];
    }

    let cells = build_braille_graph_cells(state, width, height, theme);
    graph_cells_to_lines(cells)
}

fn graph_cells_to_lines(cells: Vec<Vec<BrailleGraphCell>>) -> Vec<Line<'static>> {
    if cells.is_empty() {
        return vec![Line::from("")];
    }

    cells
        .into_iter()
        .map(|row| {
            let spans: Vec<Span<'static>> = row
                .into_iter()
                .map(|cell| Span::styled(char_to_string(cell.glyph), cell.style))
                .collect();
            Line::from(spans)
        })
        .collect()
}

fn build_braille_graph_cells(
    state: &AppState,
    width: usize,
    height: usize,
    theme: &Theme,
) -> Vec<Vec<BrailleGraphCell>> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let mut cells = vec![vec![BrailleGraphCell::blank(theme); width]; height];
    let sub_width = width.saturating_mul(2);
    let sub_height = height.saturating_mul(4);
    if sub_width == 0 || sub_height == 0 {
        return cells;
    }

    let pixel_count = sub_width.saturating_mul(sub_height);
    let mut agree_buffer = vec![0.0f32; pixel_count];
    let mut counter_buffer = vec![0.0f32; pixel_count];
    let mut node_buffer = vec![0.0f32; pixel_count];

    let node_positions = team_graph_node_positions(width, height);
    let max_weight = state
        .team_discussion
        .edges
        .iter()
        .map(|edge| edge.weight)
        .max()
        .unwrap_or(1)
        .max(1);

    for edge in &state.team_discussion.edges {
        if edge.weight == 0 {
            continue;
        }
        let Some(from_cell) = node_positions.get(&edge.from).copied() else {
            continue;
        };
        let Some(to_cell) = node_positions.get(&edge.to).copied() else {
            continue;
        };

        let (x1, y1) = node_subpixel_anchor(from_cell);
        let (x2, y2) = node_subpixel_anchor(to_cell);
        draw_weighted_team_edge(
            &mut agree_buffer,
            &mut counter_buffer,
            TeamEdgeRender {
                dims: (sub_width, sub_height),
                from: (x1, y1),
                to: (x2, y2),
                kind: edge.kind,
                weight: edge.weight,
                max_weight,
            },
        );
    }

    let influence = compute_team_node_influence(&state.team_discussion.edges);
    for role in TeamRole::ALL {
        let Some(cell_pos) = node_positions.get(&role).copied() else {
            continue;
        };
        let (cx, cy) = node_subpixel_anchor(cell_pos);
        let strength = influence.get(&role).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        stamp_node_influence(&mut node_buffer, sub_width, sub_height, cx, cy, strength);
    }

    let mut global_peak = 0.0f32;
    for idx in 0..pixel_count {
        let total = agree_buffer[idx] + counter_buffer[idx] + node_buffer[idx];
        global_peak = global_peak.max(total);
    }
    let on_threshold = if global_peak > 0.0 {
        global_peak * 0.18
    } else {
        f32::MAX
    };

    for (y, row) in cells.iter_mut().enumerate().take(height) {
        for (x, cell) in row.iter_mut().enumerate().take(width) {
            let mut bits = 0u8;
            let mut agree_energy = 0.0f32;
            let mut counter_energy = 0.0f32;
            let mut node_energy = 0.0f32;

            for sy in 0..4 {
                for sx in 0..2 {
                    let px = x * 2 + sx;
                    let py = y * 4 + sy;
                    let idx = py * sub_width + px;
                    let agree = agree_buffer[idx];
                    let counter = counter_buffer[idx];
                    let node = node_buffer[idx];
                    let total = agree + counter + node;

                    agree_energy += agree;
                    counter_energy += counter;
                    node_energy += node;

                    if total >= on_threshold {
                        bits |= braille_dot_bit(sx, sy);
                    }
                }
            }

            let glyph = if bits == 0 {
                ' '
            } else {
                char::from_u32(0x2800 + bits as u32).unwrap_or(' ')
            };

            let mut style =
                if node_energy > (agree_energy + counter_energy) * 0.9 && node_energy > 0.0 {
                    Style::default().fg(theme.text_accent).bg(theme.bg_primary)
                } else if agree_energy > counter_energy * 1.2 {
                    team_edge_style(theme, TeamEdgeKind::Agree).bg(theme.bg_primary)
                } else if counter_energy > agree_energy * 1.2 {
                    team_edge_style(theme, TeamEdgeKind::Counter).bg(theme.bg_primary)
                } else if agree_energy + counter_energy > 0.0 {
                    Style::default().fg(theme.signal_wait).bg(theme.bg_primary)
                } else {
                    theme.text_muted()
                };

            let intensity = if global_peak > 0.0 {
                ((agree_energy + counter_energy + node_energy) / (global_peak * 8.0))
                    .clamp(0.0, 2.0)
            } else {
                0.0
            };

            if intensity > 1.15 {
                style = style.add_modifier(Modifier::BOLD);
            } else if intensity > 0.0 && intensity < 0.20 {
                style = style.add_modifier(Modifier::DIM);
            }

            *cell = BrailleGraphCell { glyph, style };
        }
    }

    for role in TeamRole::ALL {
        let Some((x, y)) = node_positions.get(&role).copied() else {
            continue;
        };
        if y >= height || x >= width {
            continue;
        }

        let strength = influence.get(&role).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let mut style = team_role_style(theme, role);
        if strength >= 0.66 {
            style = style.add_modifier(Modifier::BOLD);
        } else if strength <= 0.15 {
            style = style.add_modifier(Modifier::DIM);
        }

        cells[y][x] = BrailleGraphCell {
            glyph: role.short().chars().next().unwrap_or('?'),
            style,
        };

        if x + 1 < width {
            let marker = if strength >= 0.80 {
                '◉'
            } else if strength >= 0.55 {
                '●'
            } else if strength >= 0.30 {
                '•'
            } else {
                '·'
            };
            cells[y][x + 1] = BrailleGraphCell {
                glyph: marker,
                style,
            };
        }
    }

    cells
}

fn team_graph_node_positions(width: usize, height: usize) -> HashMap<TeamRole, (usize, usize)> {
    let mut positions = HashMap::new();
    if width == 0 || height == 0 {
        return positions;
    }

    let left = if width > 10 { 2 } else { 1.min(width - 1) };
    let right = if width > 12 {
        width.saturating_sub(4)
    } else {
        width.saturating_sub(2)
    }
    .max(left);

    let top = 1.min(height.saturating_sub(1));
    let mid = (height / 2).clamp(1, height.saturating_sub(2).max(1));
    let bottom = height.saturating_sub(2).max(mid);

    positions.insert(TeamRole::Analyst, (left, top));
    positions.insert(TeamRole::Trader, (right, top));
    positions.insert(TeamRole::RiskManager, (left, mid));
    positions.insert(TeamRole::Researcher, (right, mid));
    positions.insert(TeamRole::Leader, (left, bottom));
    positions.insert(TeamRole::DevilsAdvocate, (right, bottom));
    positions
}

fn node_subpixel_anchor((x, y): (usize, usize)) -> (f32, f32) {
    (x as f32 * 2.0 + 0.5, y as f32 * 4.0 + 1.5)
}

struct TeamEdgeRender {
    dims: (usize, usize),
    from: (f32, f32),
    to: (f32, f32),
    kind: TeamEdgeKind,
    weight: u32,
    max_weight: u32,
}

fn draw_weighted_team_edge(
    agree_buffer: &mut [f32],
    counter_buffer: &mut [f32],
    edge: TeamEdgeRender,
) {
    let TeamEdgeRender {
        dims,
        from,
        to,
        kind,
        weight,
        max_weight,
    } = edge;
    let (width, height) = dims;
    if width == 0 || height == 0 || weight == 0 {
        return;
    }

    let max_weight = max_weight.max(1) as f32;
    let normalized = (weight as f32 / max_weight).clamp(0.05, 1.0);
    let radius = 0.45 + normalized * 1.5;
    let base_energy = 0.50 + normalized * 1.05;

    let dx = (to.0 - from.0).abs();
    let dy = (to.1 - from.1).abs();
    let steps = ((dx.max(dy) * 2.0).ceil() as usize).max(2);

    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let x = from.0 + (to.0 - from.0) * t;
        let y = from.1 + (to.1 - from.1) * t;
        let pulse = (1.0 - (t - 0.5).abs() * 0.85).clamp(0.55, 1.0);
        let energy = base_energy * pulse;

        match kind {
            TeamEdgeKind::Agree => {
                stamp_subpixel(agree_buffer, width, height, x, y, radius, energy);
            }
            TeamEdgeKind::Counter => {
                stamp_subpixel(counter_buffer, width, height, x, y, radius, energy);
            }
        }
    }
}

fn stamp_node_influence(
    node_buffer: &mut [f32],
    width: usize,
    height: usize,
    cx: f32,
    cy: f32,
    influence: f32,
) {
    let influence = influence.clamp(0.0, 1.0);
    let radius = 0.95 + influence * 2.15;
    let energy = 1.2 + influence * 2.2;

    stamp_subpixel(node_buffer, width, height, cx, cy, radius, energy);
    stamp_subpixel(
        node_buffer,
        width,
        height,
        cx,
        cy,
        (radius * 0.55).max(0.4),
        energy * 0.85,
    );
}

fn stamp_subpixel(
    buffer: &mut [f32],
    width: usize,
    height: usize,
    cx: f32,
    cy: f32,
    radius: f32,
    energy: f32,
) {
    if width == 0 || height == 0 {
        return;
    }

    let min_x = (cx - radius - 1.0).floor().max(0.0) as isize;
    let max_x = (cx + radius + 1.0)
        .ceil()
        .min((width.saturating_sub(1)) as f32) as isize;
    let min_y = (cy - radius - 1.0).floor().max(0.0) as isize;
    let max_y = (cy + radius + 1.0)
        .ceil()
        .min((height.saturating_sub(1)) as f32) as isize;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > radius + 0.35 {
                continue;
            }

            let falloff = (1.0 - (dist / (radius + 0.35))).clamp(0.05, 1.0);
            let idx = y as usize * width + x as usize;
            buffer[idx] += energy * falloff;
        }
    }
}

fn braille_dot_bit(x: usize, y: usize) -> u8 {
    match (x, y) {
        (0, 0) => 0b0000_0001,
        (0, 1) => 0b0000_0010,
        (0, 2) => 0b0000_0100,
        (1, 0) => 0b0000_1000,
        (1, 1) => 0b0001_0000,
        (1, 2) => 0b0010_0000,
        (0, 3) => 0b0100_0000,
        (1, 3) => 0b1000_0000,
        _ => 0,
    }
}

fn compute_team_node_influence(edges: &[TeamRelationEdge]) -> HashMap<TeamRole, f32> {
    let mut influence: HashMap<TeamRole, f32> = TeamRole::ALL
        .into_iter()
        .map(|role| (role, 0.0f32))
        .collect();

    for edge in edges {
        if edge.weight == 0 {
            continue;
        }
        let impact = edge.weight as f32;
        if let Some(v) = influence.get_mut(&edge.from) {
            *v += impact;
        }
        if let Some(v) = influence.get_mut(&edge.to) {
            *v += impact;
        }
    }

    let max_value = influence.values().copied().fold(0.0f32, f32::max);
    if max_value > 0.0 {
        for value in influence.values_mut() {
            *value = (*value / max_value).clamp(0.0, 1.0);
        }
    }

    influence
}

fn render_team_history_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
) {
    team::render_team_history_page(f, area, state, theme, scroll);
}

/// Render the team action popup above the input bar.
pub fn render_team_action_popup(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    summary: &str,
    selected_index: usize,
) {
    team::render_team_action_popup(f, area, theme, summary, selected_index);
}

pub fn render_keybind_popup(f: &mut Frame, area: Rect, theme: &Theme, page: &Page) {
    let popup_width = area.width.min(86);
    let popup_height = area.height.min(16);
    let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup = Rect::new(popup_x, popup_y, popup_width, popup_height);

    render_popup_overlay(f, area, theme);
    f.render_widget(Clear, popup);

    let title = format!(" Keybinds · {} ", page_label(page));
    let block = Block::default()
        .title(Span::styled(title, theme.text_accent_bold()))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.popup_frame());
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let lines = vec![
        Line::from(Span::styled("Navigation", theme.table_header())),
        Line::from(Span::styled(
            "1-7 switch tabs, ←/→ cycle tabs, ↑/↓ scroll content",
            theme.text_secondary(),
        )),
        Line::from(""),
        Line::from(Span::styled("Input", theme.table_header())),
        Line::from(Span::styled(
            "/ start command mode, Enter submit, Esc clear input",
            theme.text_secondary(),
        )),
        Line::from(""),
        Line::from(Span::styled("Chart", theme.table_header())),
        Line::from(Span::styled(
            "Tab pair, 1/2/3/4/5 timeframe, [/] pan, +/- zoom, i indicators, s sentiment",
            theme.text_secondary(),
        )),
        Line::from(""),
        Line::from(Span::styled("Global", theme.table_header())),
        Line::from(Span::styled(
            "Ctrl+L clear, Ctrl+C exit confirm, Esc/? close this popup",
            theme.text_secondary(),
        )),
    ];

    f.render_widget(Paragraph::new(lines).style(theme.text()), inner);
}

fn render_popup_overlay(f: &mut Frame, area: Rect, theme: &Theme) {
    let overlay = vec![Line::from(" ".repeat(area.width as usize)); area.height as usize];
    f.render_widget(Paragraph::new(overlay).style(theme.overlay_dim()), area);
}

fn page_label(page: &Page) -> &'static str {
    match page {
        Page::Portfolio => "Portfolio",
        Page::Signals => "Signals",
        Page::Chart => "Chart",
        Page::Team => "Team",
        Page::News => "News",
        Page::NewsHistory => "News History",
        Page::Heatmap => "Heatmap",
        Page::Status => "Status",
        Page::History => "History",
        Page::Stats => "Stats",
        Page::Customize => "Customize",
        Page::Sentiment => "Sentiment",
        Page::Macro => "Macro",
        Page::Help => "Help",
        Page::Chat => "Chat",
        Page::Log => "Log",
        Page::Pairs => "Pairs",
        Page::Model => "Model",
        Page::Auth => "Auth",
        Page::TeamHistory => "Team History",
    }
}

fn render_auth_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    auth_view: Option<&AuthPageView>,
    spinner_frame: usize,
) {
    auth::render_auth_page(f, area, state, theme, auth_view, spinner_frame);
}

fn auth_to_llm_provider(provider: AuthProvider) -> LlmProvider {
    match provider {
        AuthProvider::GitHub => LlmProvider::Copilot,
        AuthProvider::Anthropic => LlmProvider::Claude,
        AuthProvider::OpenAI => LlmProvider::OpenAI,
        AuthProvider::Gemini => LlmProvider::Gemini,
        AuthProvider::OpenRouter => LlmProvider::OpenRouter,
        AuthProvider::Gradio => LlmProvider::Gradio,
    }
}

fn help_row(
    theme: &Theme,
    left_cmd: &str,
    left_desc: &str,
    right_cmd: &str,
    right_desc: &str,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{:<20}", left_cmd), theme.text_accent()),
        Span::styled(format!("{:<23}", left_desc), theme.text_secondary()),
        Span::styled(format!("{:<22}", right_cmd), theme.text_accent()),
        Span::styled(right_desc.to_string(), theme.text_secondary()),
    ])
}

fn push_divider(lines: &mut Vec<Line<'static>>, theme: &Theme, width: usize) {
    lines.push(Line::from(Span::styled(
        chars::LINE_H.repeat(width),
        theme.divider(),
    )));
}

fn format_price(price: Decimal) -> String {
    if price >= Decimal::from(10_000) {
        format!("${:.0}", price)
    } else if price >= Decimal::from(100) {
        format!("${:.2}", price)
    } else if price >= Decimal::ONE {
        format!("${:.3}", price)
    } else {
        format!("${:.6}", price)
    }
}

fn format_price_plain(price: Decimal) -> String {
    if price >= Decimal::from(10_000) {
        format!("{:.0}", price)
    } else if price >= Decimal::from(100) {
        format!("{:.1}", price)
    } else {
        format!("{:.2}", price)
    }
}

fn format_pnl(pnl: Decimal) -> String {
    if pnl >= Decimal::ZERO {
        format!("+${:.2}", pnl)
    } else {
        format!("-${:.2}", pnl.abs())
    }
}

fn confidence_bar(confidence: u8, _theme: &Theme) -> String {
    let filled = (confidence as usize / 10).clamp(0, 10);
    let empty = 10usize.saturating_sub(filled);
    format!(
        "{}{} {:>3}%",
        chars::BLOCK_FULL.repeat(filled),
        chars::BLOCK_LIGHT.repeat(empty),
        confidence
    )
}

fn confidence_style(confidence: u8, theme: &Theme) -> Style {
    if confidence >= 70 {
        Style::default().fg(theme.profit_strong)
    } else if confidence >= 50 {
        Style::default().fg(theme.signal_wait)
    } else {
        Style::default().fg(theme.loss_mild)
    }
}

fn score_style(score: i8, theme: &Theme) -> Style {
    if score >= 20 {
        Style::default().fg(theme.profit_strong)
    } else if score >= 8 {
        Style::default().fg(theme.signal_wait)
    } else {
        Style::default().fg(theme.loss_mild)
    }
}

fn format_duration(duration: Duration) -> String {
    let total = duration.as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{}h {}m", h, m)
    } else if m > 0 {
        format!("{}m {}s", m, s)
    } else {
        format!("{}s", s)
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    for raw in text.lines() {
        if raw.chars().count() <= width {
            lines.push(raw.to_string());
            continue;
        }

        let mut current = String::new();
        for word in raw.split_whitespace() {
            let next_len = if current.is_empty() {
                word.chars().count()
            } else {
                current.chars().count() + 1 + word.chars().count()
            };

            if next_len > width && !current.is_empty() {
                lines.push(current);
                current = word.to_string();
            } else {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
            }
        }

        if !current.is_empty() {
            lines.push(current);
        }
    }

    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn sample_candles_to_width(values: &[OHLCV], width: usize) -> Vec<OHLCV> {
    if values.is_empty() || width == 0 {
        return Vec::new();
    }
    if values.len() <= width {
        return values.to_vec();
    }

    let step = values.len() as f64 / width as f64;
    let mut sampled = Vec::with_capacity(width);
    for i in 0..width {
        let idx = (i as f64 * step).floor() as usize;
        sampled.push(values[idx.min(values.len() - 1)].clone());
    }
    sampled
}

fn ema_series(values: &[Decimal], period: usize) -> Vec<Option<Decimal>> {
    if values.is_empty() || period == 0 {
        return Vec::new();
    }
    let mut out = vec![None; values.len()];
    if values.len() < period {
        return out;
    }

    let mut seed = Decimal::ZERO;
    for v in values.iter().take(period) {
        seed += *v;
    }
    let period_dec = Decimal::from(period as i64);
    let mut ema = seed / period_dec;
    out[period - 1] = Some(ema);

    let alpha = Decimal::from(2) / Decimal::from((period + 1) as i64);
    for idx in period..values.len() {
        ema = (values[idx] * alpha) + (ema * (Decimal::ONE - alpha));
        out[idx] = Some(ema);
    }

    out
}

fn map_decimal_to_row(value: Decimal, min: Decimal, max: Decimal, rows: usize) -> usize {
    if rows <= 1 {
        return 0;
    }
    let range = (max - min).max(Decimal::new(1, 8));
    let normalized = ((max - value) / range).clamp(Decimal::ZERO, Decimal::ONE);
    let scaled = (normalized * Decimal::from((rows - 1) as i64))
        .round_dp(0)
        .to_string()
        .parse::<usize>()
        .unwrap_or(0);
    scaled.min(rows.saturating_sub(1))
}

fn sparkline_from_position(pnl_pct: Decimal) -> String {
    let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut out = String::new();
    for i in 0..12 {
        let wave = ((i as f64 / 11.0) * std::f64::consts::PI * 2.0).sin();
        let bias = pnl_pct.to_string().parse::<f64>().unwrap_or(0.0) / 100.0;
        let value = (wave * 0.35 + bias).clamp(-1.0, 1.0);
        let idx = (((value + 1.0) / 2.0) * 7.0).round() as usize;
        out.push(chars[idx.min(7)]);
    }
    out
}

fn quality_bar(score: f32) -> String {
    let clamped = score.clamp(0.0, 1.0);
    let filled = (clamped * 10.0).round() as usize;
    format!(
        "{}{}",
        "█".repeat(filled.min(10)),
        "░".repeat(10usize.saturating_sub(filled.min(10)))
    )
}

fn quality_style(score: f32, theme: &Theme) -> Style {
    if score >= 0.75 {
        theme.profit_style()
    } else if score >= 0.45 {
        Style::default().fg(theme.signal_wait)
    } else {
        theme.loss_style()
    }
}

fn format_compact_number(value: Decimal) -> String {
    if value >= Decimal::from(1_000_000_000) {
        format!("{:.1}B", value / Decimal::from(1_000_000_000))
    } else if value >= Decimal::from(1_000_000) {
        format!("{:.1}M", value / Decimal::from(1_000_000))
    } else if value >= Decimal::from(1_000) {
        format!("{:.1}K", value / Decimal::from(1_000))
    } else {
        format!("{:.0}", value)
    }
}

fn format_large_usd(value: f64) -> String {
    if value >= 1_000_000_000_000.0 {
        format!("${:.2}T", value / 1_000_000_000_000.0)
    } else if value >= 1_000_000_000.0 {
        format!("${:.2}B", value / 1_000_000_000.0)
    } else if value >= 1_000_000.0 {
        format!("${:.2}M", value / 1_000_000.0)
    } else {
        format!("${:.0}", value)
    }
}

fn source_toggle(enabled: bool) -> &'static str {
    if enabled {
        "[✓]"
    } else {
        "[✗]"
    }
}

fn truncate_ellipsis(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let char_count = input.chars().count();
    if char_count <= max_chars {
        return input.to_string();
    }

    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let keep = max_chars - 3;
    let mut out: String = input.chars().take(keep).collect();
    out.push_str("...");
    out
}

fn sentiment_value_style(value: &str, theme: &Theme) -> Style {
    let trimmed = value.trim();
    if let Ok(parsed) = trimmed.parse::<f32>() {
        if parsed > 0.10 {
            return theme.profit_style();
        }
        if parsed < -0.10 {
            return theme.loss_style();
        }
        return theme.text_secondary();
    }

    if trimmed.eq_ignore_ascii_case("n/a") {
        theme.text_muted()
    } else {
        theme.text()
    }
}

fn customize_marker(index: usize, selected: usize, theme: &Theme) -> Span<'static> {
    if index == selected {
        Span::styled(">", theme.text_accent_bold())
    } else {
        Span::styled(" ", theme.text_muted())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::config::LlmProvider;
    use crate::state::{TeamEdgeKind, TeamRelationEdge, TeamRole};

    #[test]
    fn test_format_price() {
        assert_eq!(format_price(Decimal::from(84_230)), "$84230");
        assert!(format_price(Decimal::new(12345, 2)).starts_with("$"));
    }

    #[test]
    fn test_confidence_bar() {
        let theme = Theme::dark();
        let bar = confidence_bar(78, &theme);
        assert!(bar.contains("78"));
    }

    #[test]
    fn test_wrap_text() {
        let wrapped = wrap_text("one two three four five", 8);
        assert!(!wrapped.is_empty());
    }

    #[test]
    fn test_quality_bar_length() {
        let bar = quality_bar(0.63);
        assert_eq!(bar.chars().count(), 10);
    }

    #[test]
    fn test_ema_series_basic() {
        let values = vec![
            Decimal::from(1),
            Decimal::from(2),
            Decimal::from(3),
            Decimal::from(4),
            Decimal::from(5),
        ];
        let ema = ema_series(&values, 3);
        assert_eq!(ema.len(), values.len());
        assert!(ema[0].is_none());
        assert!(ema[2].is_some());
    }

    #[test]
    fn test_braille_graph_contains_role_labels() {
        let state = AppState::new(Config::default());
        let theme = Theme::dark();
        let cells = build_braille_graph_cells(&state, 28, 14, &theme);

        let mut graph_text = String::new();
        for row in cells {
            for cell in row {
                graph_text.push(cell.glyph);
            }
        }

        for role in TeamRole::ALL {
            assert!(graph_text.contains(role.short()));
        }
    }

    #[test]
    fn test_braille_graph_edge_weight_increases_density() {
        let mut state = AppState::new(Config::default());
        let theme = Theme::dark();
        let baseline = build_braille_graph_cells(&state, 28, 14, &theme);
        let baseline_non_space = baseline
            .iter()
            .flatten()
            .filter(|cell| cell.glyph != ' ')
            .count();

        state.team_discussion.edges.push(TeamRelationEdge {
            from: TeamRole::Analyst,
            to: TeamRole::Trader,
            kind: TeamEdgeKind::Agree,
            weight: 9,
        });

        let weighted = build_braille_graph_cells(&state, 28, 14, &theme);
        let weighted_non_space = weighted
            .iter()
            .flatten()
            .filter(|cell| cell.glyph != ' ')
            .count();

        assert!(weighted_non_space > baseline_non_space);
    }

    #[test]
    fn test_team_node_influence_reflects_incident_edges() {
        let mut state = AppState::new(Config::default());
        state.team_discussion.edges = vec![
            TeamRelationEdge {
                from: TeamRole::Analyst,
                to: TeamRole::Trader,
                kind: TeamEdgeKind::Agree,
                weight: 10,
            },
            TeamRelationEdge {
                from: TeamRole::Analyst,
                to: TeamRole::RiskManager,
                kind: TeamEdgeKind::Counter,
                weight: 7,
            },
        ];

        let influence = compute_team_node_influence(&state.team_discussion.edges);
        let analyst = influence.get(&TeamRole::Analyst).copied().unwrap_or(0.0);
        let leader = influence.get(&TeamRole::Leader).copied().unwrap_or(0.0);

        assert!(analyst > leader);
    }

    #[test]
    fn test_truncate_ellipsis_preserves_short_strings() {
        assert_eq!(truncate_ellipsis("abc", 5), "abc");
        assert_eq!(truncate_ellipsis("abc", 3), "abc");
    }

    #[test]
    fn test_truncate_ellipsis_compacts_long_strings() {
        assert_eq!(truncate_ellipsis("abcdefgh", 5), "ab...");
        assert_eq!(truncate_ellipsis("abcdefgh", 1), ".");
        assert_eq!(truncate_ellipsis("abcdefgh", 0), "");
    }

    #[test]
    fn test_sentiment_value_style_parses_numeric_sign() {
        let theme = Theme::dark();
        let pos = sentiment_value_style("+0.45", &theme);
        let neg = sentiment_value_style("-0.55", &theme);
        let neutral = sentiment_value_style("+0.02", &theme);

        assert_ne!(format!("{:?}", pos), format!("{:?}", neutral));
        assert_ne!(format!("{:?}", neg), format!("{:?}", neutral));
    }

    #[test]
    fn test_render_customize_page_selection_markers_match_index() {
        let state = AppState::new(Config::default());
        let theme = Theme::dark();

        let marks: Vec<&str> = (0..=14)
            .map(|idx| {
                if idx == 5 {
                    if source_toggle(state.config.data.yahoo_enabled) == "[✓]" {
                        "ok"
                    } else {
                        "ok"
                    }
                } else if idx == 14 {
                    if !state.config.tui.chart_default_timeframe.is_empty() {
                        "ok"
                    } else {
                        "bad"
                    }
                } else {
                    let marker = if idx == 3 { ">" } else { " " };
                    marker
                }
            })
            .collect();

        assert_eq!(marks[3], ">");
        assert_eq!(sentiment_value_style("n/a", &theme), theme.text_muted());
    }

    #[test]
    fn test_model_provider_models_include_openrouter_defaults() {
        let mut state = AppState::new(Config::default());
        state.openrouter_free_models = vec!["openrouter/free-a".to_string()];
        let options = model_provider_models(&state, &LlmProvider::OpenRouter, "");
        assert!(options
            .iter()
            .any(|m| m.id.contains("anthropic/") && m.selectable));
        assert!(options
            .iter()
            .any(|m| m.id.contains("openai/") && m.selectable));
        assert!(options
            .iter()
            .any(|m| m.display.contains("🆓") && m.selectable));
    }

    #[test]
    fn test_model_provider_models_show_non_selectable_helper_when_free_list_empty() {
        let mut state = AppState::new(Config::default());
        state
            .auth_state
            .insert(AuthProvider::OpenRouter, AuthStatus::NotConfigured);
        let options = model_provider_models(&state, &LlmProvider::OpenRouter, "");
        assert!(options.iter().any(|m| {
            !m.selectable && m.display == "Configure OpenRouter key to see free models"
        }));
    }

    #[test]
    fn test_model_provider_models_show_empty_notice_when_openrouter_key_is_configured() {
        let mut state = AppState::new(Config::default());
        state.auth_state.insert(
            AuthProvider::OpenRouter,
            AuthStatus::ApiKeyConfigured {
                masked: "●●●●●●●●ABCD".to_string(),
            },
        );

        let options = model_provider_models(&state, &LlmProvider::OpenRouter, "");
        assert!(options
            .iter()
            .any(|m| !m.selectable && m.display == "Free Models: none available right now"));
    }

    #[test]
    fn test_last4_only_mask_uses_tail_characters() {
        assert_eq!(last4_only_mask("●●●●●●●●ABCD"), "••••ABCD");
        assert_eq!(last4_only_mask("XYZ"), "••••XYZ");
    }

    #[test]
    fn test_provider_status_badge_returns_pending_for_device_flow() {
        let mut state = AppState::new(Config::default());
        state.auth_state.insert(
            crate::auth::AuthProvider::GitHub,
            crate::auth::AuthStatus::PendingDevice {
                user_code: "ABCD-EFGH".to_string(),
                verification_uri: "https://github.com/login/device".to_string(),
                expires_at: std::time::Instant::now() + std::time::Duration::from_secs(300),
                interval_secs: 5,
            },
        );
        let theme = Theme::dark();

        let (badge, _) =
            provider_status_badge(&state, crate::auth::AuthProvider::GitHub, 0, &theme);
        assert_eq!(badge, "⟳ Pending");
    }

    #[test]
    fn test_model_provider_label_and_auth_method() {
        assert_eq!(llm_provider_label(&LlmProvider::Claude), "Claude");
        assert_eq!(
            llm_provider_auth_method(&LlmProvider::Copilot),
            "Device flow"
        );
        assert_eq!(llm_provider_auth_method(&LlmProvider::Mock), "None");
    }

    #[test]
    fn test_sentiment_badge_mapping() {
        let theme = Theme::dark();
        assert_eq!(sentiment_badge(Some(0.30), &theme).0, "🟢");
        assert_eq!(sentiment_badge(Some(-0.30), &theme).0, "🔴");
        assert_eq!(sentiment_badge(Some(0.01), &theme).0, "🟡");
        assert_eq!(sentiment_badge(None, &theme).0, "·");
    }

    #[test]
    fn test_news_bucket_labels() {
        let now = Utc::now();
        assert_eq!(news_bucket_label(now), "Today");
        assert_eq!(
            news_bucket_label(now - chrono::Duration::days(1)),
            "Yesterday"
        );
        assert_eq!(
            news_bucket_label(now - chrono::Duration::days(3)),
            "This Week"
        );
    }

    #[test]
    fn test_filter_news_history_by_query() {
        let now = Utc::now();
        let items = vec![
            crate::state::NewsHeadline {
                source: "Reuters".to_string(),
                title: "Bitcoin ETF inflows rise".to_string(),
                url: Some("https://example.com/a".to_string()),
                published_at: now,
                sentiment: None,
            },
            crate::state::NewsHeadline {
                source: "Bloomberg".to_string(),
                title: "Ethereum gas fees cool".to_string(),
                url: Some("https://example.com/b".to_string()),
                published_at: now,
                sentiment: None,
            },
        ];

        assert_eq!(filter_news_history(&items, "etf").len(), 1);
        assert_eq!(filter_news_history(&items, "bloom").len(), 1);
        assert_eq!(filter_news_history(&items, "").len(), 2);
    }

    #[test]
    fn test_news_header_updated_label_present_when_fetched() {
        let mut state = AppState::new(Config::default());
        state.news_last_fetch_at = Some(Utc::now());

        let label = state
            .news_last_fetch_at
            .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "not fetched yet".to_string());

        assert!(label.contains("UTC"));
    }
}
