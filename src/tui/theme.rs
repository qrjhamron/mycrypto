//! Color theme system for the TUI.
//!
//! Provides a consistent, professional color palette for the terminal interface.
//! All colors are carefully chosen for readability and visual hierarchy.

use ratatui::style::{Color, Modifier, Style};

/// Complete theme definition with all colors and styles.
#[derive(Debug, Clone)]
pub struct Theme {
    // ─────────────────────────────────────────────────────────────
    // Background Colors
    // ─────────────────────────────────────────────────────────────
    /// Primary background color.
    pub bg_primary: Color,
    /// Elevated/secondary background (cards, popups).
    pub bg_elevated: Color,
    /// Selected/highlighted row background.
    pub bg_selected: Color,

    // ─────────────────────────────────────────────────────────────
    // Text Colors
    // ─────────────────────────────────────────────────────────────
    /// Primary text - high contrast.
    pub text_primary: Color,
    /// Secondary text - medium contrast.
    pub text_secondary: Color,
    /// Muted text - low contrast (timestamps, hints).
    pub text_muted: Color,
    /// Accent text - highlighted information.
    pub text_accent: Color,

    // ─────────────────────────────────────────────────────────────
    // Profit/Loss Colors
    // ─────────────────────────────────────────────────────────────
    /// Strong profit - bright green.
    pub profit_strong: Color,
    /// Normal profit.
    pub profit: Color,
    /// Mild/secondary profit.
    pub profit_mild: Color,
    /// Strong loss - bright red.
    pub loss_strong: Color,
    /// Normal loss.
    pub loss: Color,
    /// Mild/secondary loss.
    pub loss_mild: Color,

    // ─────────────────────────────────────────────────────────────
    // Signal Colors
    // ─────────────────────────────────────────────────────────────
    /// LONG signal - green.
    pub signal_long: Color,
    /// SHORT signal - red.
    pub signal_short: Color,
    /// WAIT signal - amber/yellow.
    pub signal_wait: Color,
    /// SKIP signal - gray.
    pub signal_skip: Color,

    // ─────────────────────────────────────────────────────────────
    // UI Element Colors
    // ─────────────────────────────────────────────────────────────
    /// Active border (focused elements).
    pub border_active: Color,
    /// Inactive border.
    pub border_inactive: Color,
    /// Status indicators - connected/running.
    pub status_ok: Color,
    /// Status indicators - warning/paused.
    pub status_warn: Color,
    /// Status indicators - error/disconnected.
    pub status_error: Color,
    /// Paper trading badge background.
    pub status_paper: Color,

    // ─────────────────────────────────────────────────────────────
    // Chat Colors
    // ─────────────────────────────────────────────────────────────
    /// Chat user name color (cyan).
    pub chat_user_name: Color,
    /// Chat agent name color (purple).
    pub chat_agent_name: Color,

    // ─────────────────────────────────────────────────────────────
    // Logo Gradient Colors (specific for splash)
    // ─────────────────────────────────────────────────────────────
    /// Logo 'm' color.
    pub logo_m: Color,
    /// Logo 'Y' color.
    pub logo_y_upper: Color,
    /// Logo 'c' color.
    pub logo_c: Color,
    /// Logo 'r' color.
    pub logo_r: Color,
    /// Logo 'y' color.
    pub logo_y_lower: Color,
    /// Logo 'p' color.
    pub logo_p: Color,
    /// Logo 't' color.
    pub logo_t: Color,
    /// Logo 'o' color.
    pub logo_o: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Creates the dark theme (default).
    pub fn dark() -> Self {
        Self {
            // Background colors - deep blue-gray
            bg_primary: Color::Rgb(13, 17, 23),
            bg_elevated: Color::Rgb(22, 27, 34),
            bg_selected: Color::Rgb(33, 38, 45),

            // Text colors
            text_primary: Color::Rgb(230, 237, 243),
            text_secondary: Color::Rgb(139, 148, 158),
            text_muted: Color::Rgb(89, 98, 108),
            text_accent: Color::Rgb(88, 166, 255),

            // Profit/Loss
            profit_strong: Color::Rgb(63, 185, 80),
            profit: Color::Rgb(46, 160, 67),
            profit_mild: Color::Rgb(111, 214, 141),
            loss_strong: Color::Rgb(248, 81, 73),
            loss: Color::Rgb(218, 54, 51),
            loss_mild: Color::Rgb(244, 134, 128),

            // Signals
            signal_long: Color::Rgb(63, 185, 80),
            signal_short: Color::Rgb(248, 81, 73),
            signal_wait: Color::Rgb(210, 153, 34),
            signal_skip: Color::Rgb(139, 148, 158),

            // UI Elements
            border_active: Color::Rgb(88, 166, 255),
            border_inactive: Color::Rgb(48, 54, 61),
            status_ok: Color::Rgb(63, 185, 80),
            status_warn: Color::Rgb(210, 153, 34),
            status_error: Color::Rgb(248, 81, 73),
            status_paper: Color::Rgb(130, 80, 223),

            // Chat
            chat_user_name: Color::Rgb(57, 211, 203),
            chat_agent_name: Color::Rgb(163, 113, 247),

            // Logo gradient
            logo_m: Color::Rgb(57, 211, 203), // cyan (chat_user_name)
            logo_y_upper: Color::Rgb(88, 166, 255), // blue (text_accent)
            logo_c: Color::Rgb(63, 185, 80),  // green (signal_long)
            logo_r: Color::Rgb(63, 185, 80),  // profit_strong
            logo_y_lower: Color::Rgb(210, 153, 34), // amber (signal_wait)
            logo_p: Color::Rgb(163, 113, 247), // purple (chat_agent_name)
            logo_t: Color::Rgb(248, 81, 73),  // red (loss_strong)
            logo_o: Color::Rgb(230, 237, 243), // white (text_primary)
        }
    }

    /// Creates a light theme variant.
    pub fn light() -> Self {
        Self {
            // Background colors - light
            bg_primary: Color::Rgb(255, 255, 255),
            bg_elevated: Color::Rgb(246, 248, 250),
            bg_selected: Color::Rgb(234, 238, 242),

            // Text colors
            text_primary: Color::Rgb(36, 41, 47),
            text_secondary: Color::Rgb(87, 96, 106),
            text_muted: Color::Rgb(139, 148, 158),
            text_accent: Color::Rgb(9, 105, 218),

            // Profit/Loss
            profit_strong: Color::Rgb(26, 127, 55),
            profit: Color::Rgb(31, 136, 61),
            profit_mild: Color::Rgb(87, 171, 113),
            loss_strong: Color::Rgb(207, 34, 46),
            loss: Color::Rgb(164, 14, 38),
            loss_mild: Color::Rgb(215, 102, 111),

            // Signals
            signal_long: Color::Rgb(26, 127, 55),
            signal_short: Color::Rgb(207, 34, 46),
            signal_wait: Color::Rgb(154, 103, 0),
            signal_skip: Color::Rgb(87, 96, 106),

            // UI Elements
            border_active: Color::Rgb(9, 105, 218),
            border_inactive: Color::Rgb(208, 215, 222),
            status_ok: Color::Rgb(26, 127, 55),
            status_warn: Color::Rgb(154, 103, 0),
            status_error: Color::Rgb(207, 34, 46),
            status_paper: Color::Rgb(130, 80, 223),

            // Chat
            chat_user_name: Color::Rgb(8, 134, 127),
            chat_agent_name: Color::Rgb(130, 80, 223),

            // Logo gradient (same hues, adjusted for light bg)
            logo_m: Color::Rgb(8, 134, 127),
            logo_y_upper: Color::Rgb(9, 105, 218),
            logo_c: Color::Rgb(26, 127, 55),
            logo_r: Color::Rgb(26, 127, 55),
            logo_y_lower: Color::Rgb(154, 103, 0),
            logo_p: Color::Rgb(130, 80, 223),
            logo_t: Color::Rgb(207, 34, 46),
            logo_o: Color::Rgb(36, 41, 47),
        }
    }

    /// Get theme by name.
    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "light" => Self::light(),
            _ => Self::dark(),
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Pre-composed Styles
    // ═══════════════════════════════════════════════════════════════

    /// Default text style.
    pub fn text(&self) -> Style {
        Style::default().fg(self.text_primary).bg(self.bg_primary)
    }

    /// Secondary text style.
    pub fn text_dim(&self) -> Style {
        Style::default().fg(self.text_secondary).bg(self.bg_primary)
    }

    /// Secondary text style alias.
    pub fn text_secondary(&self) -> Style {
        self.text_dim()
    }

    /// Muted text style.
    pub fn text_muted(&self) -> Style {
        Style::default().fg(self.text_muted).bg(self.bg_primary)
    }

    /// Muted italic text style (for timestamps).
    pub fn text_muted_italic(&self) -> Style {
        Style::default()
            .fg(self.text_muted)
            .bg(self.bg_primary)
            .add_modifier(Modifier::ITALIC)
    }

    /// Accent text style.
    pub fn text_accent(&self) -> Style {
        Style::default().fg(self.text_accent).bg(self.bg_primary)
    }

    /// Accent bold text style (for pair names).
    pub fn text_accent_bold(&self) -> Style {
        Style::default()
            .fg(self.text_accent)
            .bg(self.bg_primary)
            .add_modifier(Modifier::BOLD)
    }

    /// Bold text style.
    pub fn text_bold(&self) -> Style {
        Style::default()
            .fg(self.text_primary)
            .bg(self.bg_primary)
            .add_modifier(Modifier::BOLD)
    }

    /// Section header style (accent, bold, underlined).
    pub fn section_header(&self) -> Style {
        Style::default()
            .fg(self.text_accent)
            .bg(self.bg_primary)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    }

    /// Table header style.
    pub fn table_header(&self) -> Style {
        Style::default()
            .fg(self.text_secondary)
            .bg(self.bg_primary)
            .add_modifier(Modifier::BOLD)
    }

    /// Strong profit style.
    pub fn profit_style(&self) -> Style {
        Style::default().fg(self.profit_strong).bg(self.bg_primary)
    }

    /// Strong loss style.
    pub fn loss_style(&self) -> Style {
        Style::default().fg(self.loss_strong).bg(self.bg_primary)
    }

    /// Warning/amber style.
    pub fn warning(&self) -> Style {
        Style::default().fg(self.signal_wait).bg(self.bg_primary)
    }

    /// Divider style.
    pub fn divider(&self) -> Style {
        Style::default()
            .fg(self.border_inactive)
            .bg(self.bg_primary)
    }

    /// Active border style.
    pub fn border_active(&self) -> Style {
        Style::default().fg(self.border_active).bg(self.bg_primary)
    }

    /// Inactive border style.
    pub fn border_inactive(&self) -> Style {
        Style::default()
            .fg(self.border_inactive)
            .bg(self.bg_primary)
    }

    /// Top bar style.
    pub fn bar(&self) -> Style {
        Style::default()
            .fg(self.text_secondary)
            .bg(self.bg_elevated)
    }

    /// Input bar background style.
    pub fn input_bg(&self) -> Style {
        Style::default().fg(self.text_primary).bg(self.bg_elevated)
    }

    /// Prompt style (cyan "> ").
    pub fn prompt(&self) -> Style {
        Style::default()
            .fg(self.chat_user_name)
            .bg(self.bg_elevated)
    }

    /// App name style (purple, bold).
    pub fn app_name(&self) -> Style {
        Style::default()
            .fg(self.chat_agent_name)
            .bg(self.bg_elevated)
            .add_modifier(Modifier::BOLD)
    }

    /// Paper badge style.
    pub fn paper_badge(&self) -> Style {
        Style::default()
            .fg(self.text_primary)
            .bg(self.status_paper)
            .add_modifier(Modifier::BOLD)
    }

    /// Status dot - OK (green).
    pub fn status_ok(&self) -> Style {
        Style::default().fg(self.status_ok).bg(self.bg_elevated)
    }

    /// Status dot - Warning (amber).
    pub fn status_warn(&self) -> Style {
        Style::default().fg(self.status_warn).bg(self.bg_elevated)
    }

    /// Status dot - Error (red).
    pub fn status_error(&self) -> Style {
        Style::default().fg(self.status_error).bg(self.bg_elevated)
    }

    /// PnL style based on value.
    pub fn pnl(&self, value: rust_decimal::Decimal) -> Style {
        if value > rust_decimal::Decimal::ZERO {
            self.profit_style()
        } else if value < rust_decimal::Decimal::ZERO {
            self.loss_style()
        } else {
            self.text_dim()
        }
    }

    /// Price change style with arrow.
    pub fn price_change(&self, is_positive: bool) -> Style {
        if is_positive {
            Style::default().fg(self.profit_strong).bg(self.bg_elevated)
        } else {
            Style::default().fg(self.loss_strong).bg(self.bg_elevated)
        }
    }

    /// Signal direction style.
    pub fn signal_direction(&self, direction: &crate::state::SignalDirection) -> Style {
        use crate::state::SignalDirection;
        match direction {
            SignalDirection::Long => Style::default()
                .fg(self.signal_long)
                .bg(self.bg_primary)
                .add_modifier(Modifier::BOLD),
            SignalDirection::Short => Style::default()
                .fg(self.signal_short)
                .bg(self.bg_primary)
                .add_modifier(Modifier::BOLD),
            SignalDirection::Wait => Style::default().fg(self.signal_wait).bg(self.bg_primary),
        }
    }

    /// Signal action style.
    pub fn signal_action(&self, action: &crate::state::SignalAction, executed: bool) -> Style {
        use crate::state::SignalAction;
        if executed {
            return Style::default().fg(self.profit_strong).bg(self.bg_primary);
        }
        match action {
            SignalAction::Execute => Style::default().fg(self.profit_strong).bg(self.bg_primary),
            SignalAction::Watch => Style::default().fg(self.signal_wait).bg(self.bg_primary),
            SignalAction::Skip => Style::default().fg(self.signal_skip).bg(self.bg_primary),
        }
    }

    /// Confidence bar filled portion.
    pub fn confidence_filled(&self) -> Style {
        Style::default().fg(self.signal_long).bg(self.bg_primary)
    }

    /// Confidence bar empty portion.
    pub fn confidence_empty(&self) -> Style {
        Style::default().fg(self.bg_elevated).bg(self.bg_primary)
    }

    /// Chat user message style.
    pub fn chat_user(&self) -> Style {
        Style::default()
            .fg(self.chat_user_name)
            .bg(self.bg_primary)
            .add_modifier(Modifier::BOLD)
    }

    /// Chat agent message style.
    pub fn chat_agent(&self) -> Style {
        Style::default()
            .fg(self.chat_agent_name)
            .bg(self.bg_primary)
            .add_modifier(Modifier::BOLD)
    }

    /// Error message style.
    pub fn error(&self) -> Style {
        Style::default().fg(self.loss_strong).bg(self.bg_primary)
    }

    /// Autocomplete popup background.
    pub fn popup_bg(&self) -> Style {
        Style::default().fg(self.text_primary).bg(self.bg_elevated)
    }

    /// Autocomplete selected row.
    pub fn popup_selected(&self) -> Style {
        Style::default()
            .fg(self.text_primary)
            .bg(self.border_active)
            .add_modifier(Modifier::BOLD)
    }

    /// Autocomplete description text.
    pub fn popup_description(&self) -> Style {
        Style::default().fg(self.text_muted).bg(self.bg_elevated)
    }

    /// Model selector - radio selected.
    pub fn radio_selected(&self) -> Style {
        Style::default()
            .fg(self.text_accent)
            .bg(self.bg_primary)
            .add_modifier(Modifier::BOLD)
    }

    /// Model selector - radio unselected.
    pub fn radio_unselected(&self) -> Style {
        Style::default().fg(self.text_secondary).bg(self.bg_primary)
    }

    /// Spinner style.
    pub fn spinner(&self) -> Style {
        Style::default().fg(self.text_accent).bg(self.bg_primary)
    }

    /// Header shell style.
    pub fn shell_header(&self) -> Style {
        Style::default()
            .fg(self.text_primary)
            .bg(self.bg_elevated)
            .add_modifier(Modifier::BOLD)
    }

    /// Footer shell style.
    pub fn shell_footer(&self) -> Style {
        Style::default()
            .fg(self.text_secondary)
            .bg(self.bg_elevated)
    }

    /// Bright brand accent style.
    pub fn brand_bright(&self) -> Style {
        Style::default()
            .fg(Color::LightCyan)
            .bg(self.bg_elevated)
            .add_modifier(Modifier::BOLD)
    }

    /// Secondary brand accent style.
    pub fn brand_dim(&self) -> Style {
        Style::default()
            .fg(Color::Cyan)
            .bg(self.bg_elevated)
            .add_modifier(Modifier::BOLD)
    }

    /// Active tab style.
    pub fn tab_active(&self) -> Style {
        Style::default()
            .fg(Color::White)
            .bg(Color::Blue)
            .add_modifier(Modifier::BOLD)
    }

    /// Inactive tab style.
    pub fn tab_inactive(&self) -> Style {
        Style::default().fg(self.text_muted).bg(self.bg_primary)
    }

    /// Activity strip style.
    pub fn activity_strip(&self) -> Style {
        Style::default().fg(self.text_secondary).bg(self.bg_primary)
    }

    /// Footer hint style.
    pub fn footer_hint(&self) -> Style {
        Style::default().fg(self.text_muted).bg(self.bg_elevated)
    }

    /// Header value flash style for upward move.
    pub fn value_flash_up(&self) -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(self.profit_strong)
            .add_modifier(Modifier::BOLD)
    }

    /// Header value flash style for downward move.
    pub fn value_flash_down(&self) -> Style {
        Style::default()
            .fg(Color::White)
            .bg(self.loss_strong)
            .add_modifier(Modifier::BOLD)
    }

    /// Positive sentiment pill style.
    pub fn sentiment_positive(&self) -> Style {
        Style::default().fg(Color::Black).bg(self.profit_strong)
    }

    /// Neutral sentiment pill style.
    pub fn sentiment_neutral(&self) -> Style {
        Style::default().fg(Color::Black).bg(self.signal_wait)
    }

    /// Negative sentiment pill style.
    pub fn sentiment_negative(&self) -> Style {
        Style::default().fg(Color::White).bg(self.loss_strong)
    }

    /// Connected websocket pulse style.
    pub fn ws_connected(&self) -> Style {
        Style::default().fg(self.status_ok).bg(self.bg_elevated)
    }

    /// Disconnected websocket style.
    pub fn ws_disconnected(&self) -> Style {
        Style::default()
            .fg(self.status_error)
            .bg(self.bg_elevated)
            .add_modifier(Modifier::BOLD)
    }

    /// Shared panel border style.
    pub fn panel_border(&self) -> Style {
        Style::default()
            .fg(self.border_inactive)
            .bg(self.bg_primary)
    }

    /// Shared panel title style.
    pub fn panel_title(&self) -> Style {
        Style::default()
            .fg(self.text_accent)
            .bg(self.bg_primary)
            .add_modifier(Modifier::BOLD)
    }

    /// Overlay dim style for popups.
    pub fn overlay_dim(&self) -> Style {
        Style::default().bg(self.bg_selected)
    }

    /// Popup frame style.
    pub fn popup_frame(&self) -> Style {
        Style::default().fg(self.text_accent).bg(self.bg_elevated)
    }

    /// Heatmap cell style by 24h change percentage.
    pub fn heatmap_cell(&self, change_pct_24h: f64) -> Style {
        if change_pct_24h >= 4.0 {
            Style::default().fg(Color::Black).bg(self.profit_strong)
        } else if change_pct_24h >= 1.0 {
            Style::default().fg(Color::Black).bg(self.profit)
        } else if change_pct_24h > -1.0 {
            Style::default().fg(self.text_primary).bg(self.bg_selected)
        } else if change_pct_24h > -4.0 {
            Style::default().fg(Color::White).bg(self.loss)
        } else {
            Style::default().fg(Color::White).bg(self.loss_strong)
        }
    }
}

/// Special characters for the TUI.
pub mod chars {
    /// Filled block for progress bars.
    pub const BLOCK_FULL: &str = "█";
    /// Light shade for empty bars.
    pub const BLOCK_LIGHT: &str = "░";
    /// Medium shade.
    pub const BLOCK_MED: &str = "▒";
    /// Horizontal line.
    pub const LINE_H: &str = "─";
    /// Vertical line.
    pub const LINE_V: &str = "│";
    /// Corner: top-left rounded.
    pub const CORNER_TL: &str = "╭";
    /// Corner: top-right rounded.
    pub const CORNER_TR: &str = "╮";
    /// Corner: bottom-left rounded.
    pub const CORNER_BL: &str = "╰";
    /// Corner: bottom-right rounded.
    pub const CORNER_BR: &str = "╯";
    /// Arrow up.
    pub const ARROW_UP: &str = "▲";
    /// Arrow down.
    pub const ARROW_DOWN: &str = "▼";
    /// Arrow right (selection indicator).
    pub const ARROW_RIGHT: &str = "▶";
    /// Bullet point.
    pub const BULLET: &str = "●";
    /// Empty bullet.
    pub const BULLET_EMPTY: &str = "○";
    /// Check mark.
    pub const CHECK: &str = "✓";
    /// Cross mark.
    pub const CROSS: &str = "✗";
    /// Spinner frames.
    pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    /// Dashed horizontal line.
    pub const LINE_H_DASHED: &str = "╌";
    /// Double horizontal line.
    pub const LINE_H_DOUBLE: &str = "═";
    /// Double vertical line.
    pub const LINE_V_DOUBLE: &str = "║";
    /// Double corner: top-left.
    pub const CORNER_TL_DOUBLE: &str = "╔";
    /// Double corner: top-right.
    pub const CORNER_TR_DOUBLE: &str = "╗";
    /// Double corner: bottom-left.
    pub const CORNER_BL_DOUBLE: &str = "╚";
    /// Double corner: bottom-right.
    pub const CORNER_BR_DOUBLE: &str = "╝";
}

/// Signal icons with semantic meaning.
pub mod icons {
    /// Execute signal.
    pub const EXECUTE: &str = "⚡";
    /// Watch signal.
    pub const WATCH: &str = "🔍";
    /// Wait signal.
    pub const WAIT: &str = "⏳";
    /// Skip signal.
    pub const SKIP: &str = "🚫";
    /// Success/completed.
    pub const SUCCESS: &str = "✅";
    /// Warning.
    pub const WARNING: &str = "⚠️";
    /// Long position.
    pub const LONG: &str = "📈";
    /// Short position.
    pub const SHORT: &str = "📉";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_theme_creation() {
        let theme = Theme::dark();
        assert_eq!(theme.bg_primary, Color::Rgb(13, 17, 23));
    }

    #[test]
    fn test_light_theme_creation() {
        let theme = Theme::light();
        assert_eq!(theme.bg_primary, Color::Rgb(255, 255, 255));
    }

    #[test]
    fn test_theme_from_name() {
        let dark = Theme::from_name("dark");
        let light = Theme::from_name("light");
        let unknown = Theme::from_name("unknown");

        assert_eq!(dark.bg_primary, Theme::dark().bg_primary);
        assert_eq!(light.bg_primary, Theme::light().bg_primary);
        assert_eq!(unknown.bg_primary, Theme::dark().bg_primary);
    }

    #[test]
    fn test_confidence_styles() {
        let theme = Theme::dark();
        let filled = theme.confidence_filled();
        let empty = theme.confidence_empty();
        assert_ne!(format!("{:?}", filled), format!("{:?}", empty));
    }
}
