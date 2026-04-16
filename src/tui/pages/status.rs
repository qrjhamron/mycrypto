use super::*;

pub(super) fn render_status_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
    source_names_sorted: Option<&[String]>,
) {
    let shell = Block::default()
        .title(Span::styled(" System Status ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let inner = shell.inner(area);
    f.render_widget(shell, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(36),
            Constraint::Percentage(20),
            Constraint::Percentage(22),
            Constraint::Percentage(22),
        ])
        .split(inner);

    let source_header = Row::new(vec![
        Cell::from("Source"),
        Cell::from("Level"),
        Cell::from("Detail"),
    ])
    .style(theme.table_header());

    let detail_width = chunks[0].width.saturating_sub(30) as usize;
    let mut source_rows_all: Vec<Row> = if state.source_health.is_empty() {
        vec![Row::new(vec![
            Cell::from("n/a").style(theme.text_muted()),
            Cell::from("waiting").style(theme.text_muted()),
            Cell::from("Awaiting first source poll").style(theme.text_muted()),
        ])]
    } else {
        if let Some(cached) = source_names_sorted {
            cached
                .iter()
                .filter_map(|name| state.source_health.get(name))
                .map(|source| {
                    let level_style = match source.level {
                        crate::state::SourceStatusLevel::Connected
                        | crate::state::SourceStatusLevel::Ok => theme.status_ok(),
                        crate::state::SourceStatusLevel::Warn => theme.status_warn(),
                        _ => theme.status_error(),
                    };
                    Row::new(vec![
                        Cell::from(source.name.as_str()).style(theme.text_secondary()),
                        Cell::from(format!("{}", source.level)).style(level_style),
                        Cell::from(truncate_ellipsis(&source.detail, detail_width.max(10)))
                            .style(theme.text()),
                    ])
                })
                .collect()
        } else {
            let mut fallback: Vec<&String> = state.source_health.keys().collect();
            fallback.sort();
            fallback
                .into_iter()
                .filter_map(|name| state.source_health.get(name))
                .map(|source| {
                    let level_style = match source.level {
                        crate::state::SourceStatusLevel::Connected
                        | crate::state::SourceStatusLevel::Ok => theme.status_ok(),
                        crate::state::SourceStatusLevel::Warn => theme.status_warn(),
                        _ => theme.status_error(),
                    };
                    Row::new(vec![
                        Cell::from(source.name.as_str()).style(theme.text_secondary()),
                        Cell::from(format!("{}", source.level)).style(level_style),
                        Cell::from(truncate_ellipsis(&source.detail, detail_width.max(10)))
                            .style(theme.text()),
                    ])
                })
                .collect()
        }
    };

    let source_viewport = chunks[0].height.saturating_sub(3) as usize;
    let source_total = source_rows_all.len();
    let source_start = scroll.min(source_total.saturating_sub(1));
    let source_rows: Vec<Row> = source_rows_all
        .drain(source_start..)
        .take(source_viewport.max(1))
        .collect();

    let source_table = Table::new(
        source_rows,
        [
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Min(12),
        ],
    )
    .header(source_header)
    .block(
        Block::default()
            .title(Span::styled(" Data Sources ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(source_table, chunks[0]);

    if source_total > source_viewport.max(1) {
        let mut scroll_state =
            ScrollbarState::new(source_total).position(source_start.min(source_total - 1));
        f.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            chunks[0],
            &mut scroll_state,
        );
    }

    let quality_rows: Vec<Row> = if state.config.pairs.watchlist.is_empty() {
        vec![Row::new(vec![
            Cell::from("n/a").style(theme.text_muted()),
            Cell::from("No watchlist pairs").style(theme.text_muted()),
            Cell::from("n/a").style(theme.text_muted()),
        ])]
    } else {
        state
            .config
            .pairs
            .watchlist
            .iter()
            .map(|pair| {
                let score = state
                    .data_quality
                    .get(pair)
                    .copied()
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);
                let change = state
                    .get_ticker(pair)
                    .map(|t| t.price_change_pct_24h)
                    .unwrap_or(Decimal::ZERO);
                Row::new(vec![
                    Cell::from(pair.replace("USDT", "")).style(theme.text_accent_bold()),
                    Cell::from(format!(
                        "{} {:>3}%",
                        quality_bar(score),
                        (score * 100.0).round() as i32
                    ))
                    .style(quality_style(score, theme)),
                    Cell::from(format!("{:+.2}%", change))
                        .style(theme.price_change(change >= Decimal::ZERO)),
                ])
            })
            .collect()
    };

    let quality_table = Table::new(
        quality_rows,
        [
            Constraint::Length(10),
            Constraint::Length(18),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec![
            Cell::from("Pair"),
            Cell::from("Quality"),
            Cell::from("24h"),
        ])
        .style(theme.table_header()),
    )
    .block(
        Block::default()
            .title(Span::styled(" Data Quality ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(quality_table, chunks[1]);

    let engine_rows = vec![
        Row::new(vec![
            Cell::from("Agent Status").style(theme.table_header()),
            Cell::from(format!("{}", state.agent_status)).style(theme.text()),
        ]),
        Row::new(vec![
            Cell::from("Open Trades").style(theme.table_header()),
            Cell::from(format!(
                "{} / {}",
                state.portfolio.positions.len(),
                state.config.agent.max_open_trades
            ))
            .style(theme.text()),
        ]),
        Row::new(vec![
            Cell::from("Uptime").style(theme.table_header()),
            Cell::from(format_duration(
                Utc::now()
                    .signed_duration_since(state.started_at)
                    .to_std()
                    .unwrap_or(Duration::from_secs(0)),
            ))
            .style(theme.text()),
        ]),
        Row::new(vec![
            Cell::from("Circuit Breaker").style(theme.table_header()),
            Cell::from(if state.engine_status.circuit_breaker_open {
                "OPEN"
            } else {
                "CLOSED"
            })
            .style(if state.engine_status.circuit_breaker_open {
                theme.status_error()
            } else {
                theme.status_ok()
            }),
        ]),
        Row::new(vec![
            Cell::from("WS Uptime").style(theme.table_header()),
            Cell::from(format!(
                "{:.0}%",
                state.engine_status.ws_uptime_ratio * 100.0
            ))
            .style(theme.text_secondary()),
        ]),
    ];

    let engine_table = Table::new(engine_rows, [Constraint::Length(18), Constraint::Min(12)])
        .block(
            Block::default()
                .title(Span::styled(" Engine Telemetry ", theme.panel_title()))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.panel_border()),
        )
        .style(theme.text());
    f.render_widget(engine_table, chunks[2]);

    let auth_rows: Vec<Row> = crate::auth::AuthProvider::ALL
        .iter()
        .map(|provider| {
            let status = state.auth_state.get(provider);
            let (text, style) = match status {
                Some(crate::auth::AuthStatus::AuthenticatedGitHub {
                    username,
                    created_at,
                    ..
                }) => {
                    let remaining_days = (*created_at + chrono::Duration::days(90))
                        .signed_duration_since(Utc::now())
                        .num_days()
                        .max(0);
                    (
                        format!("@{} expires {}d", username, remaining_days),
                        theme.profit_style(),
                    )
                }
                Some(s) if s.is_configured() => ("configured".to_string(), theme.profit_style()),
                Some(crate::auth::AuthStatus::Error(err)) => {
                    (truncate_ellipsis(err, 34), theme.loss_style())
                }
                _ => ("not set".to_string(), theme.text_muted()),
            };

            Row::new(vec![
                Cell::from(provider.display_name()).style(theme.text_secondary()),
                Cell::from(text).style(style),
            ])
        })
        .collect();

    let auth_table = Table::new(auth_rows, [Constraint::Length(14), Constraint::Min(14)])
        .header(
            Row::new(vec![Cell::from("Provider"), Cell::from("Status")])
                .style(theme.table_header()),
        )
        .block(
            Block::default()
                .title(Span::styled(" Authentication ", theme.panel_title()))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.panel_border()),
        )
        .style(theme.text());
    f.render_widget(auth_table, chunks[3]);
}
