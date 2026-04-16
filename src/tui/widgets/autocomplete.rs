//! Command autocomplete widget.
//!
//! Floating suggestion box that appears when user types "/".

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Modifier,
    widgets::{Clear, Widget},
};

use crate::tui::theme::{chars, Theme};

/// A command suggestion entry.
#[derive(Debug, Clone)]
pub struct CommandSuggestion {
    /// The command (e.g., "/portfolio").
    pub command: &'static str,
    /// Short description.
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct SuggestionRow {
    x: u16,
    y: u16,
    width: u16,
}

/// All available commands for autocomplete.
pub const COMMANDS: &[CommandSuggestion] = &[
    CommandSuggestion {
        command: "/portfolio",
        description: "View portfolio & trades",
    },
    CommandSuggestion {
        command: "/signals",
        description: "Latest signals",
    },
    CommandSuggestion {
        command: "/chart",
        description: "Price chart",
    },
    CommandSuggestion {
        command: "/history",
        description: "Closed trade history",
    },
    CommandSuggestion {
        command: "/stats",
        description: "Performance metrics",
    },
    CommandSuggestion {
        command: "/customize",
        description: "Interactive config editor",
    },
    CommandSuggestion {
        command: "/pairs",
        description: "Watchlist manager",
    },
    CommandSuggestion {
        command: "/log",
        description: "Recent logs",
    },
    CommandSuggestion {
        command: "/status",
        description: "System + source health",
    },
    CommandSuggestion {
        command: "/heatmap",
        description: "24h market heatmap",
    },
    CommandSuggestion {
        command: "/news",
        description: "News + history (H)",
    },
    CommandSuggestion {
        command: "/sentiment",
        description: "Sentiment breakdown",
    },
    CommandSuggestion {
        command: "/macro",
        description: "Macro context",
    },
    CommandSuggestion {
        command: "/pause",
        description: "Pause agent",
    },
    CommandSuggestion {
        command: "/resume",
        description: "Resume agent",
    },
    CommandSuggestion {
        command: "/add [pair]",
        description: "Add to watchlist",
    },
    CommandSuggestion {
        command: "/remove [pair]",
        description: "Remove from watchlist",
    },
    CommandSuggestion {
        command: "/close [pair]",
        description: "Close position",
    },
    CommandSuggestion {
        command: "/risk [%]",
        description: "Set risk per trade",
    },
    CommandSuggestion {
        command: "/confidence [%]",
        description: "Set min confidence",
    },
    CommandSuggestion {
        command: "/model",
        description: "Select AI model",
    },
    CommandSuggestion {
        command: "/auth",
        description: "GitHub authentication",
    },
    CommandSuggestion {
        command: "/team [prompt]",
        description: "Run AI Agent Team",
    },
    CommandSuggestion {
        command: "/team status",
        description: "Team discussion status",
    },
    CommandSuggestion {
        command: "/team history",
        description: "Last 5 team sessions",
    },
    CommandSuggestion {
        command: "/reset",
        description: "Reset paper portfolio",
    },
    CommandSuggestion {
        command: "/clear",
        description: "Clear current page",
    },
    CommandSuggestion {
        command: "/help",
        description: "Full help page",
    },
    CommandSuggestion {
        command: "/exit",
        description: "Quit mYcrypto",
    },
];

/// Autocomplete widget state.
#[derive(Debug, Clone)]
pub struct Autocomplete {
    /// Whether the autocomplete popup is visible.
    pub visible: bool,
    /// Current filter text (what user has typed after "/").
    pub filter: String,
    /// Currently selected filtered index.
    pub selected_index: usize,
    /// Scroll offset into the filtered list.
    pub scroll_offset: usize,
    /// Filtered suggestions.
    filtered: Vec<usize>,
}

impl Default for Autocomplete {
    fn default() -> Self {
        Self::new()
    }
}

impl Autocomplete {
    /// Create a new autocomplete state.
    pub fn new() -> Self {
        Self {
            visible: false,
            filter: String::new(),
            selected_index: 0,
            scroll_offset: 0,
            filtered: (0..COMMANDS.len()).collect(),
        }
    }

    /// Show the autocomplete popup.
    pub fn show(&mut self) {
        self.visible = true;
        self.filter.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.update_filtered();
    }

    /// Hide the autocomplete popup.
    pub fn hide(&mut self) {
        self.visible = false;
        self.filter.clear();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Update the filter text.
    pub fn set_filter(&mut self, filter: &str) {
        // Remove leading "/" if present
        self.filter = filter.trim_start_matches('/').to_lowercase();
        self.update_filtered();
        // Keep selection in bounds
        if self.selected_index >= self.filtered.len() {
            self.selected_index = self.filtered.len().saturating_sub(1);
        }
        if self.scroll_offset > self.selected_index {
            self.scroll_offset = self.selected_index;
        }
        if self.scroll_offset >= self.filtered.len() {
            self.scroll_offset = 0;
        }
    }

    /// Update filtered suggestions based on current filter.
    fn update_filtered(&mut self) {
        if self.filter.is_empty() {
            self.filtered = (0..COMMANDS.len()).collect();
        } else {
            self.filtered = COMMANDS
                .iter()
                .enumerate()
                .filter(|(_, cmd)| {
                    cmd.command
                        .trim_start_matches('/')
                        .split_whitespace()
                        .next()
                        .map(|c| c.starts_with(&self.filter))
                        .unwrap_or(false)
                })
                .map(|(i, _)| i)
                .collect();
        }
    }

    /// Move selection up.
    pub fn prev(&mut self) {
        if !self.filtered.is_empty() {
            if self.selected_index > 0 {
                self.selected_index -= 1;
            }
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    /// Move selection down.
    pub fn next(&mut self) {
        if !self.filtered.is_empty() {
            if self.selected_index + 1 < self.filtered.len() {
                self.selected_index += 1;
            }
            let max_visible = Self::max_visible_items();
            if self.selected_index >= self.scroll_offset + max_visible {
                self.scroll_offset = self.selected_index + 1 - max_visible;
            }
        }
    }

    /// Get the currently selected command.
    pub fn selected_command(&self) -> Option<&'static str> {
        self.filtered
            .get(self.selected_index)
            .and_then(|&i| COMMANDS.get(i))
            .map(|cmd| {
                // Return just the command part without arguments
                cmd.command.split_whitespace().next().unwrap_or(cmd.command)
            })
    }

    /// Check if there are any suggestions.
    pub fn has_suggestions(&self) -> bool {
        !self.filtered.is_empty()
    }

    /// Get the number of visible suggestions.
    pub fn suggestion_count(&self) -> usize {
        self.filtered.len()
    }

    /// Render the autocomplete popup.
    pub fn render(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        if !self.visible || self.filtered.is_empty() {
            return;
        }
        if area.width < 6 || area.height < 3 {
            return;
        }

        // Calculate popup dimensions
        let max_cmd_width = COMMANDS.iter().map(|c| c.command.len()).max().unwrap_or(15);
        let max_desc_width = COMMANDS
            .iter()
            .map(|c| c.description.len())
            .max()
            .unwrap_or(20);

        let popup_width = (max_cmd_width + max_desc_width + 7)
            .min((area.width as usize).saturating_sub(4)) as u16;
        let max_visible = Self::max_visible_items();
        let visible_rows = std::cmp::min(
            max_visible,
            self.filtered.len().saturating_sub(self.scroll_offset),
        );
        let has_more = self.scroll_offset + visible_rows < self.filtered.len();
        let popup_height = (visible_rows + 2 + usize::from(has_more)).min(20) as u16;

        // Position popup above input bar, left-aligned with input
        let popup_x = area.x + 2;
        let popup_y = if area.y > popup_height {
            area.y - popup_height
        } else {
            area.y
        };

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the area
        Clear.render(popup_area, buf);

        // Draw border
        self.draw_border(popup_area, buf, theme);

        // Draw suggestions
        let inner_area = Rect::new(
            popup_area.x + 1,
            popup_area.y + 1,
            popup_area.width.saturating_sub(2),
            popup_area.height.saturating_sub(2),
        );

        for i in 0..visible_rows {
            let filtered_idx = self.scroll_offset + i;
            let cmd_idx = self.filtered[filtered_idx];

            let cmd = &COMMANDS[cmd_idx];
            let y = inner_area.y + i as u16;
            let is_selected = filtered_idx == self.selected_index;

            self.draw_suggestion_row(
                SuggestionRow {
                    x: inner_area.x,
                    y,
                    width: inner_area.width,
                },
                cmd,
                is_selected,
                buf,
                theme,
            );
        }

        if has_more {
            let remaining = self.filtered.len() - (self.scroll_offset + visible_rows);
            let indicator_y = inner_area.y + visible_rows as u16;
            let indicator_text = format!("↓ {} more", remaining);
            self.draw_scroll_indicator(
                inner_area.x,
                indicator_y,
                inner_area.width,
                &indicator_text,
                buf,
                theme,
            );
        }
    }

    /// Draw the popup border.
    fn draw_border(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let border_style = theme.border_active();
        let bg_style = theme.popup_bg();

        // Fill background
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_style(bg_style);
            }
        }

        // Top border with title
        buf[(area.x, area.y)]
            .set_symbol(chars::CORNER_TL)
            .set_style(border_style);

        buf[(area.x + 1, area.y)]
            .set_symbol(chars::LINE_H)
            .set_style(border_style);

        // Title
        let title = " Commands ";
        for (i, ch) in title.chars().enumerate() {
            if area.x + 2 + (i as u16) < area.x + area.width - 1 {
                buf[(area.x + 2 + i as u16, area.y)]
                    .set_char(ch)
                    .set_style(theme.text_accent_bold());
            }
        }

        // Rest of top border
        for x in (area.x + 2 + title.len() as u16)..area.x + area.width - 1 {
            buf[(x, area.y)]
                .set_symbol(chars::LINE_H)
                .set_style(border_style);
        }

        buf[(area.x + area.width - 1, area.y)]
            .set_symbol(chars::CORNER_TR)
            .set_style(border_style);

        // Side borders
        for y in area.y + 1..area.y + area.height - 1 {
            buf[(area.x, y)]
                .set_symbol(chars::LINE_V)
                .set_style(border_style);
            buf[(area.x + area.width - 1, y)]
                .set_symbol(chars::LINE_V)
                .set_style(border_style);
        }

        // Bottom border
        buf[(area.x, area.y + area.height - 1)]
            .set_symbol(chars::CORNER_BL)
            .set_style(border_style);

        for x in area.x + 1..area.x + area.width - 1 {
            buf[(x, area.y + area.height - 1)]
                .set_symbol(chars::LINE_H)
                .set_style(border_style);
        }

        buf[(area.x + area.width - 1, area.y + area.height - 1)]
            .set_symbol(chars::CORNER_BR)
            .set_style(border_style);
    }

    /// Draw a single suggestion row.
    fn draw_suggestion_row(
        &self,
        row: SuggestionRow,
        cmd: &CommandSuggestion,
        selected: bool,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        // Clear the row with appropriate background
        let row_style = if selected {
            theme.popup_selected()
        } else {
            theme.popup_bg()
        };

        for col in row.x..row.x + row.width {
            buf[(col, row.y)].set_style(row_style);
        }

        // Selection indicator
        let indicator = if selected { chars::ARROW_RIGHT } else { " " };
        let indicator_style = if selected {
            theme.popup_selected()
        } else {
            theme.popup_bg()
        };

        buf[(row.x, row.y)]
            .set_symbol(indicator)
            .set_style(indicator_style);

        // Command text
        let cmd_style = if selected {
            theme.popup_selected()
        } else {
            theme.text_accent()
        };

        let cmd_text = cmd.command;
        for (i, ch) in cmd_text.chars().enumerate() {
            if row.x + 2 + (i as u16) < row.x + row.width {
                let cell = &mut buf[(row.x + 2 + i as u16, row.y)];
                cell.set_char(ch);
                // Highlight matching prefix
                if i < self.filter.len() {
                    cell.set_style(cmd_style.add_modifier(Modifier::BOLD));
                } else {
                    cell.set_style(cmd_style);
                }
            }
        }

        // Description (right-aligned or after command)
        let desc_start = row.x + 2 + cmd_text.len() as u16 + 2;
        let desc_style = if selected {
            theme.popup_selected().remove_modifier(Modifier::BOLD)
        } else {
            theme.popup_description()
        };

        for (i, ch) in cmd.description.chars().enumerate() {
            if desc_start + (i as u16) < row.x + row.width {
                buf[(desc_start + i as u16, row.y)]
                    .set_char(ch)
                    .set_style(desc_style);
            }
        }
    }

    fn draw_scroll_indicator(
        &self,
        x: u16,
        y: u16,
        width: u16,
        text: &str,
        buf: &mut Buffer,
        theme: &Theme,
    ) {
        for col in x..x + width {
            buf[(col, y)].set_style(theme.popup_bg());
        }

        for (i, ch) in text.chars().enumerate() {
            let cell_x = x + i as u16;
            if cell_x >= x + width {
                break;
            }
            buf[(cell_x, y)]
                .set_char(ch)
                .set_style(theme.popup_description());
        }
    }

    const fn max_visible_items() -> usize {
        10
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autocomplete_creation() {
        let ac = Autocomplete::new();
        assert!(!ac.visible);
        assert_eq!(ac.selected_index, 0);
        assert_eq!(ac.scroll_offset, 0);
    }

    #[test]
    fn test_autocomplete_show_hide() {
        let mut ac = Autocomplete::new();
        ac.show();
        assert!(ac.visible);
        ac.hide();
        assert!(!ac.visible);
    }

    #[test]
    fn test_autocomplete_filter() {
        let mut ac = Autocomplete::new();
        ac.show();
        ac.set_filter("/po");
        assert!(ac.suggestion_count() > 0);
        assert!(ac.selected_command().is_some());
    }

    #[test]
    fn test_autocomplete_navigation() {
        let mut ac = Autocomplete::new();
        ac.show();
        ac.next();
        assert_eq!(ac.selected_index, 1);

        ac.prev();
        assert_eq!(ac.selected_index, 0);
    }

    #[test]
    fn test_autocomplete_no_wrap() {
        let mut ac = Autocomplete::new();
        ac.show();
        ac.prev();
        assert_eq!(ac.selected_index, 0);

        for _ in 0..500 {
            ac.next();
        }
        assert_eq!(ac.selected_index, ac.suggestion_count() - 1);
    }

    #[test]
    fn test_autocomplete_scroll_offset() {
        let mut ac = Autocomplete::new();
        ac.show();

        for _ in 0..11 {
            ac.next();
        }

        assert!(ac.selected_index >= 10);
        assert!(ac.scroll_offset > 0);
    }
}
