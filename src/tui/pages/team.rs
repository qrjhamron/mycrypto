use super::*;

pub(super) fn render_team_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
    spinner_frame: usize,
) {
    const TEAM_SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    let shell = Block::default()
        .title(Span::styled(" Team Discussion ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let shell_inner = shell.inner(area);
    f.render_widget(shell, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(26),
            Constraint::Percentage(46),
            Constraint::Percentage(28),
        ])
        .split(shell_inner);

    let left_block = Block::default()
        .title(Span::styled(" Team Agents ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let left_inner = left_block.inner(cols[0]);
    f.render_widget(left_block, cols[0]);

    let middle_block = Block::default()
        .title(Span::styled(" Conversation Thread ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let middle_inner = middle_block.inner(cols[1]);
    f.render_widget(middle_block, cols[1]);

    let right_outer = Block::default()
        .title(Span::styled(" Graph + Summary ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let right_inner = right_outer.inner(cols[2]);
    f.render_widget(right_outer, cols[2]);

    // Left panel: agent statuses.
    let mut left_lines: Vec<Line<'static>> = Vec::new();
    left_lines.push(Line::from(Span::styled(
        "◈ TEAM AGENTS",
        theme.section_header(),
    )));
    left_lines.push(Line::from(""));

    for agent in &state.team_discussion.agents {
        let (badge, badge_style) = match agent.status {
            TeamAgentStatus::Idle => ("IDLE", theme.text_muted()),
            TeamAgentStatus::Thinking => ("THINKING", Style::default().fg(theme.signal_wait)),
            TeamAgentStatus::Done => ("DONE", Style::default().fg(theme.status_ok)),
        };

        let spinner = if agent.status == TeamAgentStatus::Thinking {
            TEAM_SPINNER[spinner_frame % TEAM_SPINNER.len()]
        } else {
            " "
        };

        let role_style = team_role_style(theme, agent.role).add_modifier(Modifier::BOLD);
        left_lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", spinner),
                Style::default().fg(theme.signal_wait),
            ),
            Span::styled(
                format!("{} {}", agent.role.emoji(), agent.role.label()),
                role_style,
            ),
        ]));
        left_lines.push(Line::from(vec![
            Span::styled("   ", theme.text()),
            Span::styled(format!("[{}]", badge), badge_style),
            Span::styled(
                format!("  {}", agent.updated_at.format("%H:%M:%S")),
                theme.text_muted(),
            ),
        ]));
        left_lines.push(Line::from(""));
    }

    left_lines.push(Line::from(Span::styled(
        if state.team_discussion.active {
            "Session: ACTIVE"
        } else {
            "Session: IDLE"
        },
        if state.team_discussion.active {
            Style::default().fg(theme.signal_wait)
        } else {
            Style::default().fg(theme.text_secondary)
        },
    )));

    if let Some(err) = &state.team_discussion.last_error {
        left_lines.push(Line::from(""));
        left_lines.push(Line::from(Span::styled(
            format!("Error: {}", err),
            theme.error(),
        )));
    }

    f.render_widget(
        Paragraph::new(left_lines)
            .style(theme.text())
            .wrap(Wrap { trim: false }),
        left_inner,
    );

    // Middle panel: conversation thread.
    let mut middle_lines: Vec<Line<'static>> = Vec::new();
    middle_lines.push(Line::from(Span::styled(
        "◈ CONVERSATION THREAD",
        theme.section_header(),
    )));
    middle_lines.push(Line::from(Span::styled(
        format!(
            "Prompt: {}",
            state
                .team_discussion
                .prompt
                .clone()
                .unwrap_or_else(|| "(none)".to_string())
        ),
        theme.text_secondary(),
    )));
    middle_lines.push(Line::from(""));

    if state.team_discussion.thread.is_empty() {
        middle_lines.push(Line::from(Span::styled(
            "No messages yet. Run /team <prompt>",
            theme.text_muted(),
        )));
    } else {
        for entry in state
            .team_discussion
            .thread
            .iter()
            .rev()
            .take(TEAM_THREAD_RENDER_CAP)
            .rev()
        {
            let role_style = team_role_style(theme, entry.role).add_modifier(Modifier::BOLD);
            middle_lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}] ", entry.timestamp.format("%H:%M:%S")),
                    theme.text_muted_italic(),
                ),
                Span::styled(
                    format!("{} {}", entry.role.emoji(), entry.role.label()),
                    role_style,
                ),
                Span::styled(format!("  (phase {})", entry.phase), theme.text_muted()),
            ]));
            middle_lines.push(Line::from(Span::styled(
                format!("  {}", entry.content),
                theme.text(),
            )));
            middle_lines.push(Line::from(""));
        }
    }

    if let Some(card) = &state.team_discussion.pending_action {
        middle_lines.push(Line::from(Span::styled(
            "─ Action Card ─",
            theme.table_header(),
        )));
        middle_lines.push(Line::from(Span::styled(
            card.summary.clone(),
            team_action_style(theme, card.kind).add_modifier(Modifier::BOLD),
        )));
        middle_lines.push(Line::from(Span::styled(
            card.rationale.clone(),
            theme.text_secondary(),
        )));
        middle_lines.push(Line::from(Span::styled(
            "Use popup controls: [Y] Execute [N] Dismiss [E] Edit Amount [D] Re-analyze",
            theme.text_muted(),
        )));
    }

    let visible_middle: Vec<Line<'static>> = middle_lines.into_iter().skip(scroll).collect();
    f.render_widget(
        Paragraph::new(visible_middle)
            .style(theme.text())
            .wrap(Wrap { trim: false }),
        middle_inner,
    );

    let middle_height = middle_inner.height as usize;
    let middle_len = state.team_discussion.thread.len().saturating_mul(3) + 10;
    if middle_len > middle_height {
        let mut scroll_state =
            ScrollbarState::new(middle_len).position(scroll.min(middle_len.saturating_sub(1)));
        f.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            middle_inner,
            &mut scroll_state,
        );
    }

    if let Some(summary) = &state.team_discussion.session_summary {
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(6), Constraint::Min(6)])
            .split(right_inner);

        let mut right_lines: Vec<Line<'static>> = Vec::new();
        right_lines.push(Line::from(Span::styled(
            "◈ SESSION SUMMARY",
            theme.section_header(),
        )));
        right_lines.push(Line::from(Span::styled(
            format!("Topic: {}", summary.topic),
            theme.text_secondary(),
        )));
        right_lines.push(Line::from(Span::styled(
            format!("Session: {}", summary.timestamp.format("%H:%M:%S")),
            theme.text_secondary(),
        )));
        right_lines.push(Line::from(Span::styled(
            format!("Verdict: {}", summary.leader_verdict),
            theme.text_accent_bold(),
        )));
        right_lines.push(Line::from(""));
        f.render_widget(
            Paragraph::new(right_lines).style(theme.text()),
            right_chunks[0],
        );

        let header = Row::new(vec![
            Cell::from("Role"),
            Cell::from("Stance"),
            Cell::from("Conf"),
            Cell::from("Words"),
        ])
        .style(theme.table_header())
        .bottom_margin(0);

        let rows: Vec<Row> = summary
            .scorecard
            .iter()
            .map(|row| {
                let stance = match row.stance {
                    crate::state::TeamStance::Bullish => "Bullish",
                    crate::state::TeamStance::Bearish => "Bearish",
                    crate::state::TeamStance::Neutral => "Neutral",
                };
                Row::new(vec![
                    Cell::from(row.role.short().to_string())
                        .style(team_role_style(theme, row.role)),
                    Cell::from(stance),
                    Cell::from(team_confidence_bar(row.confidence)),
                    Cell::from(row.word_count.to_string()),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(6),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(6),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .title(" Session Summary ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_inactive)),
        )
        .style(theme.text());

        f.render_widget(table, right_chunks[1]);
    } else {
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(8), Constraint::Length(9)])
            .split(right_inner);

        let graph_block = Block::default()
            .title(Span::styled(" Relation Graph ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border());
        let graph_inner = graph_block.inner(right_chunks[0]);
        f.render_widget(graph_block, right_chunks[0]);

        let graph_lines = if graph_inner.width < 18 || graph_inner.height < 7 {
            let mut fallback = vec![Line::from(Span::styled("compact mode", theme.text_muted()))];
            let max_rows = (graph_inner.height as usize).saturating_sub(1);
            for line in build_ascii_graph(state).into_iter().take(max_rows) {
                fallback.push(Line::from(Span::styled(line, theme.text())));
            }
            fallback
        } else {
            build_braille_graph_lines(
                state,
                graph_inner.width as usize,
                graph_inner.height as usize,
                theme,
            )
        };
        f.render_widget(Paragraph::new(graph_lines).style(theme.text()), graph_inner);

        let edge_block = Block::default()
            .title(Span::styled(" Edge Flow ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border());
        let edge_inner = edge_block.inner(right_chunks[1]);
        f.render_widget(edge_block, right_chunks[1]);

        let mut edge_lines: Vec<Line<'static>> = vec![Line::from(vec![
            Span::styled("agree ", team_edge_style(theme, TeamEdgeKind::Agree)),
            Span::styled("counter ", team_edge_style(theme, TeamEdgeKind::Counter)),
            Span::styled("node influence", theme.text_muted()),
        ])];

        let mut sorted_edges = state.team_discussion.edges.clone();
        sorted_edges.sort_by(|a, b| b.weight.cmp(&a.weight));

        if sorted_edges.is_empty() {
            edge_lines.push(Line::from(Span::styled(
                "No links yet. Run /team <prompt>",
                theme.text_muted(),
            )));
        } else {
            let max_rows =
                ((edge_inner.height.saturating_sub(1)) as usize).min(MAX_RENDER_ITEMS_PER_LIST);
            for edge in sorted_edges.into_iter().take(max_rows) {
                let kind_label = match edge.kind {
                    TeamEdgeKind::Agree => "[agree]",
                    TeamEdgeKind::Counter => "[counter]",
                };
                let bar_len = (edge.weight as usize).clamp(1, 8);
                let bar = "▮".repeat(bar_len);
                edge_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{}→{} ", edge.from.short(), edge.to.short()),
                        team_role_style(theme, edge.from).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(kind_label, team_edge_style(theme, edge.kind)),
                    Span::styled(format!(" {}", bar), team_edge_style(theme, edge.kind)),
                    Span::styled(format!(" x{}", edge.weight), theme.text_secondary()),
                ]));
            }
        }

        f.render_widget(Paragraph::new(edge_lines).style(theme.text()), edge_inner);
    }
}

pub(super) fn render_team_history_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
) {
    let shell = Block::default()
        .title(Span::styled(" Team History ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let inner = shell.inner(area);
    f.render_widget(shell, area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Last 5 sessions",
        theme.section_header(),
    )));
    lines.push(Line::from(""));

    if state.team_discussion.history.is_empty() {
        lines.push(Line::from(Span::styled(
            "No history yet. Run /team <prompt> first.",
            theme.text_muted(),
        )));
    } else {
        for (idx, entry) in state.team_discussion.history.iter().enumerate() {
            lines.push(Line::from(Span::styled(
                format!("{} ) {}", idx + 1, entry.topic),
                theme.text_accent_bold(),
            )));
            lines.push(Line::from(vec![
                Span::styled("   Time: ", theme.text_muted()),
                Span::styled(
                    entry.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                    theme.text_secondary(),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("   Leader: ", theme.text_muted()),
                Span::styled(entry.leader_verdict.clone(), theme.text()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("   Decision: ", theme.text_muted()),
                Span::styled(entry.user_decision.clone(), theme.text()),
            ]));
            lines.push(Line::from(""));
        }
    }

    let visible: Vec<Line<'static>> = lines.into_iter().skip(scroll).collect();
    f.render_widget(
        Paragraph::new(visible)
            .style(theme.text())
            .wrap(Wrap { trim: false }),
        inner,
    );

    let line_count = if state.team_discussion.history.is_empty() {
        4
    } else {
        state.team_discussion.history.len() * 6 + 2
    };
    let viewport = inner.height as usize;
    if line_count > viewport {
        let mut scroll_state =
            ScrollbarState::new(line_count).position(scroll.min(line_count.saturating_sub(1)));
        f.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            inner,
            &mut scroll_state,
        );
    }
}

pub(super) fn render_team_action_popup(
    f: &mut Frame,
    area: Rect,
    theme: &Theme,
    summary: &str,
    selected_idx: usize,
) {
    let popup_width = area.width.min(74);
    let popup_height = 8;
    let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup = Rect::new(popup_x, popup_y, popup_width, popup_height);

    render_popup_overlay(f, area, theme);

    f.render_widget(Clear, popup);
    let block = Block::default()
        .title(Span::styled(
            " Team Action Review ",
            theme.text_accent_bold(),
        ))
        .borders(Borders::ALL)
        .border_set(symbols::border::DOUBLE)
        .border_style(Style::default().fg(theme.text_accent).bg(theme.bg_elevated));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        summary.to_string(),
        theme.text_bold(),
    )));
    lines.push(Line::from(""));

    let mut button_spans = Vec::new();
    for (idx, option) in TeamPopupOption::ALL.iter().enumerate() {
        let selected = idx == selected_idx;
        let style = if selected {
            Style::default()
                .fg(theme.bg_primary)
                .bg(theme.text_accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary).bg(theme.bg_primary)
        };
        button_spans.push(Span::styled(format!(" {} ", option.label()), style));
        button_spans.push(Span::styled("  ", theme.text()));
    }
    lines.push(Line::from(button_spans));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Arrow keys + Enter, or press Y/N/E/D",
        theme.text_muted(),
    )));

    f.render_widget(Paragraph::new(lines).style(theme.text()), inner);
}
