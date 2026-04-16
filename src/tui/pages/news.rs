use super::*;

pub(super) fn render_news_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
    meta: NewsPageMeta<'_>,
) {
    let shell = Block::default()
        .title(Span::styled(" News ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let inner = shell.inner(area);
    f.render_widget(shell, area);

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(4),
        ])
        .split(inner);

    let header = Row::new(vec![
        Cell::from("Time"),
        Cell::from("Source"),
        Cell::from("Sent"),
        Cell::from("Headline"),
    ])
    .style(theme.table_header());

    let headline_width = body[0].width.saturating_sub(24) as usize;
    let total = state.news_headlines.len();
    let viewport = body[0].height.saturating_sub(3) as usize;
    let start = scroll.min(total.saturating_sub(1));
    let end = (start + viewport.max(1)).min(total);

    let rows: Vec<Row> = if state.news_loading && state.news_headlines.is_empty() {
        let spinner = chars::SPINNER[meta.spinner_frame % chars::SPINNER.len()];
        vec![Row::new(vec![
            Cell::from("--").style(theme.text_muted()),
            Cell::from("feed").style(theme.text_muted()),
            Cell::from("…").style(theme.text_muted()),
            Cell::from(format!("{} Loading news headlines...", spinner)).style(theme.spinner()),
        ])]
    } else if state.news_headlines.is_empty() {
        vec![Row::new(vec![
            Cell::from("--").style(theme.text_muted()),
            Cell::from("feed").style(theme.text_muted()),
            Cell::from("·").style(theme.text_muted()),
            Cell::from("No headlines yet. Waiting for sources...").style(theme.text_muted()),
        ])]
    } else {
        state.news_headlines[start..end]
            .iter()
            .map(|h| {
                let (sent_text, sent_style) = sentiment_badge(h.sentiment, theme);

                Row::new(vec![
                    Cell::from(h.published_at.format("%H:%M").to_string())
                        .style(theme.text_muted()),
                    Cell::from(truncate_ellipsis(&h.source, 10)).style(theme.text_accent()),
                    Cell::from(sent_text).style(sent_style),
                    Cell::from(truncate_ellipsis(&h.title, headline_width.max(18)))
                        .style(theme.text()),
                ])
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(11),
            Constraint::Length(7),
            Constraint::Min(12),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(Span::styled(" Headlines ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .style(theme.text());
    f.render_widget(table, body[0]);

    let source_health_summary = if let Some(preview) = meta.source_status_preview {
        preview.to_string()
    } else {
        "source status not available yet".to_string()
    };

    let last_updated = state
        .news_last_fetch_at
        .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "not fetched yet".to_string());

    let header_bar = Paragraph::new(Line::from(vec![
        Span::styled("Count ", theme.table_header()),
        Span::styled(format!("{}", total), theme.text_secondary()),
        Span::styled("  |  Updated ", theme.table_header()),
        Span::styled(last_updated, theme.text_secondary()),
        Span::styled("  |  Sources ", theme.table_header()),
        Span::styled(source_health_summary, theme.text_secondary()),
    ]))
    .style(theme.text())
    .block(
        Block::default()
            .title(Span::styled(" Feed Header ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    )
    .wrap(Wrap { trim: false });
    f.render_widget(header_bar, body[1]);

    if total > viewport.max(1) {
        let mut scroll_state = ScrollbarState::new(total).position(start.min(total - 1));
        f.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            body[0],
            &mut scroll_state,
        );
    }

    let status_text = if let Some(preview) = meta.source_status_preview {
        preview.to_string()
    } else {
        if let Some(cached) = meta.source_names_sorted {
            if cached.is_empty() {
                "source status not available yet".to_string()
            } else {
                cached
                    .iter()
                    .take(4)
                    .map(|name| {
                        state
                            .source_health
                            .get(name)
                            .map(|s| format!("{}:{}", s.name, s.level))
                            .unwrap_or_else(|| name.clone())
                    })
                    .collect::<Vec<_>>()
                    .join("  ·  ")
            }
        } else {
            let mut fallback: Vec<&String> = state.source_health.keys().collect();
            fallback.sort();

            if fallback.is_empty() {
                "source status not available yet".to_string()
            } else {
                fallback
                    .into_iter()
                    .take(4)
                    .map(|name| {
                        state
                            .source_health
                            .get(name)
                            .map(|s| format!("{}:{}", s.name, s.level))
                            .unwrap_or_else(|| name.clone())
                    })
                    .collect::<Vec<_>>()
                    .join("  ·  ")
            }
        }
    };

    let footer = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Feed status: ", theme.table_header()),
            Span::styled(status_text, theme.text_secondary()),
        ]),
        Line::from(vec![
            Span::styled("Tip: ", theme.table_header()),
            Span::styled("H News History, ↑/↓ scroll", theme.text_muted()),
        ]),
    ])
    .style(theme.text())
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .title(Span::styled(" Feed Notes ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    );
    f.render_widget(footer, body[2]);
}

pub(super) fn render_news_history_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    scroll: usize,
    view: Option<&NewsHistoryView>,
) {
    let shell = Block::default()
        .title(Span::styled(" News History ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let inner = shell.inner(area);
    f.render_widget(shell, area);

    let query = view.map(|v| v.query.as_str()).unwrap_or("");
    let search_active = view.map(|v| v.search_active).unwrap_or(false);

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(8),
            Constraint::Length(4),
        ])
        .split(inner);

    let search_style = if search_active {
        theme.text_accent_bold()
    } else {
        theme.text_secondary()
    };

    let search_box = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Filter (/): ", theme.table_header()),
            Span::styled(
                if query.is_empty() {
                    "(none)".to_string()
                } else {
                    query.to_string()
                },
                search_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("Mode: ", theme.table_header()),
            Span::styled(
                if search_active { "typing" } else { "browse" },
                search_style,
            ),
        ]),
    ])
    .style(theme.text())
    .block(
        Block::default()
            .title(Span::styled(
                if search_active {
                    " Search · EDITING "
                } else {
                    " Search "
                },
                theme.panel_title(),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if search_active {
                theme
                    .border_active()
                    .fg(theme.chat_user_name)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme.panel_border()
            }),
    )
    .wrap(Wrap { trim: false });
    f.render_widget(search_box, body[0]);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_bucket = String::new();
    let filtered = filter_news_history(state.news_history.iter(), query);

    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matching cached history.",
            theme.text_muted(),
        )));
    } else {
        for item in filtered {
            let bucket = news_bucket_label(item.published_at);
            if bucket != current_bucket {
                current_bucket = bucket;
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    current_bucket.clone(),
                    theme.section_header(),
                )));
            }

            let (sent_text, sent_style) = sentiment_badge(item.sentiment, theme);
            let (title_prefix, title_suffix) = split_match_for_highlight(&item.title, query);
            let (match_text, suffix_text) = if query.is_empty() {
                (String::new(), String::new())
            } else {
                (title_suffix.0, title_suffix.1)
            };
            let match_style = if query.is_empty() {
                theme.text()
            } else {
                Style::default()
                    .fg(theme.text_primary)
                    .bg(theme.bg_selected)
                    .add_modifier(Modifier::BOLD)
            };
            lines.push(Line::from(vec![
                Span::styled(
                    item.published_at.format("%H:%M ").to_string(),
                    theme.text_muted(),
                ),
                Span::styled(
                    format!("[{:<10}] ", truncate_ellipsis(&item.source, 10)),
                    theme.text_accent(),
                ),
                Span::styled(format!("{} ", sent_text), sent_style),
                Span::styled(title_prefix, theme.text()),
                Span::styled(match_text, match_style),
                Span::styled(suffix_text, theme.text()),
            ]));
        }
    }

    let visible: Vec<Line<'static>> = lines.iter().skip(scroll).cloned().collect();
    let list = Paragraph::new(visible)
        .style(theme.text())
        .block(
            Block::default()
                .title(Span::styled(
                    " Cached Headlines (max 500) ",
                    theme.panel_title(),
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme.panel_border()),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(list, body[1]);

    let content_height = lines.len().max(1);
    let viewport = body[1].height.saturating_sub(2) as usize;
    if content_height > viewport {
        let mut scroll_state = ScrollbarState::new(content_height)
            .position(scroll.min(content_height.saturating_sub(1)));
        f.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            body[1],
            &mut scroll_state,
        );
    }

    let footer = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Keys: ", theme.table_header()),
            Span::styled("/ filter · H back to News · Esc back", theme.text_muted()),
        ]),
        Line::from(vec![
            Span::styled("Last fetch: ", theme.table_header()),
            Span::styled(
                state
                    .news_last_fetch_at
                    .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| "not fetched yet".to_string()),
                theme.text_secondary(),
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
    f.render_widget(footer, body[2]);
}
