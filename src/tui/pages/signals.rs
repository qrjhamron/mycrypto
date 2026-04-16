use super::*;

pub(super) fn render_signals_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
) {
    let shell = Block::default()
        .title(Span::styled(" Signals ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let shell_inner = shell.inner(area);
    f.render_widget(shell, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(shell_inner);

    let feed_panel = Block::default()
        .title(Span::styled(" Signal Feed ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let feed_inner = feed_panel.inner(chunks[0]);
    f.render_widget(feed_panel, chunks[0]);

    let lines = render_signals(state, theme);
    let visible: Vec<Line<'static>> = lines.iter().skip(scroll).cloned().collect();
    f.render_widget(
        Paragraph::new(visible)
            .style(theme.text())
            .wrap(Wrap { trim: false }),
        feed_inner,
    );

    let content_height = lines.len().max(1);
    let viewport_height = feed_inner.height as usize;
    if content_height > viewport_height {
        let mut state_scroll = ScrollbarState::new(content_height)
            .position(scroll.min(content_height.saturating_sub(1)));
        f.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            feed_inner,
            &mut state_scroll,
        );
    }

    let panel = Block::default()
        .title(Span::styled(" Engine Status ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());

    let mut side = Vec::new();
    side.push(Line::from(vec![
        Span::styled("Agent: ", theme.table_header()),
        Span::styled(format!("{}", state.agent_status), theme.text()),
    ]));
    side.push(Line::from(vec![
        Span::styled("Tick: ", theme.table_header()),
        Span::styled(
            format!("{}s", state.config.engine.tick_interval_secs),
            theme.text_secondary(),
        ),
    ]));
    side.push(Line::from(vec![
        Span::styled("Breaker: ", theme.table_header()),
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
    side.push(Line::from(vec![
        Span::styled("Errors: ", theme.table_header()),
        Span::styled(
            state.engine_status.consecutive_errors.to_string(),
            theme.text_secondary(),
        ),
    ]));
    side.push(Line::from(vec![
        Span::styled("WS reconnects: ", theme.table_header()),
        Span::styled(
            state.engine_status.ws_reconnect_count.to_string(),
            theme.text_secondary(),
        ),
    ]));
    side.push(Line::from(vec![
        Span::styled("WS uptime: ", theme.table_header()),
        Span::styled(
            format!("{:.0}%", state.engine_status.ws_uptime_ratio * 100.0),
            theme.text_secondary(),
        ),
    ]));

    let inner = panel.inner(chunks[1]);
    f.render_widget(panel, chunks[1]);
    f.render_widget(Paragraph::new(side).style(theme.text()), inner);
}
