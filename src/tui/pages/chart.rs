use super::*;

pub(super) fn render_chart_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    _scroll: usize,
    spinner_frame: usize,
) {
    let shell = Block::default()
        .title(Span::styled(" Chart ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let shell_inner = shell.inner(area);
    f.render_widget(shell, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(4)])
        .split(shell_inner);

    let chart_lines = render_chart(
        state,
        theme,
        chunks[0].width as usize,
        chunks[0].height as usize,
        spinner_frame,
    );
    let visible: Vec<Line<'static>> = chart_lines
        .iter()
        .take(chunks[0].height as usize)
        .cloned()
        .collect();
    let chart_panel = Paragraph::new(visible)
        .style(theme.text())
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(Span::styled(" Market View ", theme.panel_title()))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.panel_border()),
        );
    f.render_widget(chart_panel, chunks[0]);

    let pair_label = state.chart_pair.replace("USDT", "");
    let control_lines = vec![
        Line::from(vec![
            Span::styled("Pair: ", theme.table_header()),
            Span::styled(pair_label, theme.text_accent_bold()),
            Span::styled("   TF: ", theme.table_header()),
            Span::styled(state.chart_timeframe.to_string(), theme.text_accent()),
            Span::styled("   Offset: ", theme.table_header()),
            Span::styled(state.chart_offset.to_string(), theme.text_secondary()),
            Span::styled("   Zoom: ", theme.table_header()),
            Span::styled(state.chart_zoom.to_string(), theme.text_secondary()),
        ]),
        Line::from(vec![
            Span::styled("Indicators: ", theme.table_header()),
            Span::styled(
                if state.chart_show_indicators {
                    "ON"
                } else {
                    "OFF"
                },
                if state.chart_show_indicators {
                    theme.profit_style()
                } else {
                    theme.text_muted()
                },
            ),
            Span::styled("   Sentiment: ", theme.table_header()),
            Span::styled(
                if state.chart_show_sentiment {
                    "ON"
                } else {
                    "OFF"
                },
                if state.chart_show_sentiment {
                    theme.profit_style()
                } else {
                    theme.text_muted()
                },
            ),
            Span::styled(
                "   Keys: Tab pair · 1/2/3/4/5 tf · i/s toggles · [/] pan · +/- zoom",
                theme.text_muted(),
            ),
        ]),
    ];

    let controls = Paragraph::new(control_lines)
        .style(theme.text())
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(Span::styled(" Controls ", theme.panel_title()))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.panel_border()),
        );
    f.render_widget(controls, chunks[1]);
}
