//! Splash screen logo widget.
//!
//! Renders the mYcrypto ASCII art logo with neon semantic colors.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::tui::theme::Theme;

/// Full-size splash logo block.
const SPLASH_LOGO: [&str; 6] = [
    "██╗  ██╗ ██╗   ██╗ ██████╗ ██████╗ ██╗   ██╗██████╗ ████████╗ ██████╗",
    "████████╗╚██╗ ██╔╝██╔════╝ ██╔══██╗╚██╗ ██╔╝██╔══██╗╚══██╔══╝██╔═══██╗",
    "██╔██╔██║ ╚████╔╝ ██║      ██████╔╝ ╚████╔╝ ██████╔╝   ██║   ██║   ██║",
    "██║╚═╝██║  ╚██╔╝  ██║      ██╔══██╗  ╚██╔╝  ██╔═══╝    ██║   ██║   ██║",
    "██║   ██║   ██║   ╚██████╗ ██║  ██║   ██║   ██║        ██║   ╚██████╔╝",
    "╚═╝   ╚═╝   ╚═╝    ╚═════╝ ╚═╝  ╚═╝   ╚═╝   ╚═╝        ╚═╝    ╚═════╝",
];

const FALLBACK_LINE: &str = "mYcrypto ◈";

/// Splash logo renderer.
pub struct Logo<'a> {
    theme: &'a Theme,
}

impl<'a> Logo<'a> {
    /// Create a logo renderer.
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }

    /// Render splash with vertical offset.
    pub fn render_splash(&self, area: Rect, buf: &mut Buffer, offset_y: i16) {
        self.fill_bg(area, buf);

        let use_fallback = area.width < 86 || area.height < 16;

        if use_fallback {
            self.render_fallback(area, buf, offset_y);
            return;
        }

        self.render_full_logo(area, buf, offset_y);
    }

    fn fill_bg(&self, area: Rect, buf: &mut Buffer) {
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_bg(self.theme.bg_primary);
            }
        }
    }

    fn render_fallback(&self, area: Rect, buf: &mut Buffer, offset_y: i16) {
        let y = centered_y(area, 1, offset_y);
        let x = centered_x(area, FALLBACK_LINE.chars().count() as u16);

        let spans = FALLBACK_LINE
            .chars()
            .enumerate()
            .map(|(i, ch)| Span::styled(ch.to_string(), self.color_for_index(i)))
            .collect::<Vec<_>>();

        buf.set_line(
            x,
            y,
            &Line::from(spans),
            area.width.saturating_sub(x - area.x),
        );
    }

    fn render_full_logo(&self, area: Rect, buf: &mut Buffer, offset_y: i16) {
        let content_height = SPLASH_LOGO.len() as u16 + 6;
        let mut y = centered_y(area, content_height, offset_y);

        for line in SPLASH_LOGO {
            let x = centered_x(area, line.chars().count() as u16);
            let spans = line
                .chars()
                .enumerate()
                .map(|(i, ch)| {
                    let mut style = self.color_for_index(i);
                    if ch != ' ' {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    Span::styled(ch.to_string(), style)
                })
                .collect::<Vec<_>>();
            buf.set_line(
                x,
                y,
                &Line::from(spans),
                area.width.saturating_sub(x - area.x),
            );
            y = y.saturating_add(1);
        }

        y = y.saturating_add(1);
        self.center_text(
            area,
            buf,
            y,
            "─────────────────────────────────────────────────────────",
            Style::default().fg(self.theme.border_inactive),
        );
        y = y.saturating_add(1);
        self.center_text(
            area,
            buf,
            y,
            "AI-Powered Crypto Paper Trading  ·  Terminal Edition",
            Style::default().fg(self.theme.text_secondary),
        );
        y = y.saturating_add(1);
        self.center_text(
            area,
            buf,
            y,
            "github.com/qrjhamron/mYcrypto",
            Style::default().fg(self.theme.text_muted),
        );
        y = y.saturating_add(1);
        self.center_text(
            area,
            buf,
            y,
            "─────────────────────────────────────────────────────────",
            Style::default().fg(self.theme.border_inactive),
        );
        y = y.saturating_add(1);
        self.center_text(
            area,
            buf,
            y,
            "v0.1.0   ·   paper trading only   ·   no real funds",
            Style::default().fg(self.theme.text_muted),
        );
    }

    fn center_text(&self, area: Rect, buf: &mut Buffer, y: u16, text: &str, style: Style) {
        let x = centered_x(area, text.chars().count() as u16);
        let spans = vec![Span::styled(text.to_string(), style)];
        buf.set_line(
            x,
            y,
            &Line::from(spans),
            area.width.saturating_sub(x - area.x),
        );
    }

    fn color_for_index(&self, idx: usize) -> Style {
        let palette = [
            self.theme.chat_agent_name,
            self.theme.text_accent,
            self.theme.signal_long,
            self.theme.profit_strong,
            self.theme.signal_wait,
            self.theme.chat_user_name,
        ];

        Style::default()
            .fg(palette[idx % palette.len()])
            .bg(self.theme.bg_primary)
    }
}

fn centered_x(area: Rect, width: u16) -> u16 {
    if area.width > width {
        area.x + (area.width - width) / 2
    } else {
        area.x
    }
}

fn centered_y(area: Rect, height: u16, offset_y: i16) -> u16 {
    let base = if area.height > height {
        area.y + (area.height - height) / 2
    } else {
        area.y
    };

    if offset_y < 0 {
        base.saturating_sub(offset_y.unsigned_abs())
    } else {
        base.saturating_add(offset_y as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_centering_helpers() {
        let area = Rect::new(0, 0, 100, 40);
        assert_eq!(centered_x(area, 20), 40);
        assert_eq!(centered_y(area, 10, 0), 15);
    }
}
