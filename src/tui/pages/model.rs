use super::*;

pub(super) fn render_model_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    selector: &ModelSelector,
    model_view: Option<&ModelPageView>,
) {
    let shell = Block::default()
        .title(Span::styled(" AI Model + Provider ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let shell_inner = shell.inner(area);
    f.render_widget(shell, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(3)])
        .split(shell_inner);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
        .split(rows[0]);

    let provider_block = Block::default()
        .title(Span::styled(" Providers ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let provider_inner = provider_block.inner(cols[0]);
    f.render_widget(provider_block, cols[0]);

    let providers = llm_providers_ordered();
    let selected_idx = providers
        .iter()
        .position(|provider| *provider == selector.provider)
        .unwrap_or(0);
    let provider_total = providers.len();
    let provider_viewport = provider_inner.height as usize;
    let provider_start = provider_start_index(provider_total, provider_viewport, selected_idx);

    let mut provider_lines: Vec<Line<'static>> = Vec::new();
    for (idx, provider) in providers
        .iter()
        .enumerate()
        .skip(provider_start)
        .take(provider_viewport)
    {
        let (dot_style, status_text) = provider_indicator_style(state, theme, provider);
        let selected = idx == selected_idx;
        let row_style = if selected {
            Style::default()
                .fg(theme.text_primary)
                .bg(theme.bg_selected)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.text_secondary()
        };

        provider_lines.push(Line::from(vec![
            Span::styled(
                if selected {
                    format!("{} ", chars::ARROW_RIGHT)
                } else {
                    "  ".to_string()
                },
                row_style,
            ),
            Span::styled("●", dot_style),
            Span::styled(" ", row_style),
            Span::styled(format!("{:<11}", llm_provider_label(provider)), row_style),
            Span::styled(format!(" {}", status_text), row_style),
        ]));
    }

    f.render_widget(
        Paragraph::new(provider_lines)
            .style(theme.text())
            .wrap(Wrap { trim: false }),
        provider_inner,
    );

    if provider_total > provider_viewport.max(1) {
        let mut provider_scroll =
            ScrollbarState::new(provider_total).position(provider_start.min(provider_total - 1));
        f.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            provider_inner,
            &mut provider_scroll,
        );
    }

    let detail_block = Block::default()
        .title(Span::styled(" Provider Detail ", theme.text_accent_bold()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.border_active().add_modifier(Modifier::BOLD));
    let detail_inner = detail_block.inner(cols[1]);
    f.render_widget(detail_block, cols[1]);

    let detail_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(5),
            Constraint::Length(3),
        ])
        .split(detail_inner);

    let selected_provider = selector.provider.clone();
    let provider_name = llm_provider_label(&selected_provider).to_uppercase();
    let title_panel = Paragraph::new(vec![
        Line::from(Span::styled(
            format!("◈ {}", provider_name),
            theme.text_accent_bold().add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("Current model: ", theme.table_header()),
            Span::styled(
                truncate_ellipsis(
                    &selector.model,
                    detail_rows[0].width.saturating_sub(18) as usize,
                ),
                theme.text(),
            ),
        ]),
    ])
    .style(theme.text())
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .title(Span::styled(" Selection ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    );
    f.render_widget(title_panel, detail_rows[0]);

    let (dot_style, status_text) = provider_indicator_style(state, theme, &selected_provider);
    let key_status = provider_key_status_text(state, &selected_provider);
    let info_panel = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Auth method: ", theme.table_header()),
            Span::styled(
                llm_provider_auth_method(&selected_provider),
                theme.text_secondary(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Key status: ", theme.table_header()),
            Span::styled("●", dot_style),
            Span::styled(format!(" {}", status_text), theme.text_secondary()),
            Span::styled("  ", theme.text()),
            Span::styled(key_status, theme.text()),
        ]),
    ])
    .style(theme.text())
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .title(Span::styled(" Authentication ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    );
    f.render_widget(info_panel, detail_rows[1]);

    let models_block = Block::default()
        .title(Span::styled(" Available Models ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let models_inner = models_block.inner(detail_rows[2]);
    f.render_widget(models_block, detail_rows[2]);

    let model_entries = model_provider_models(state, &selected_provider, &selector.model);
    let model_selected = model_entries
        .iter()
        .position(|entry| entry.selectable && entry.id == selector.model)
        .unwrap_or(0);
    let model_total = model_entries.len();
    let model_viewport = models_inner.height as usize;
    let model_start = provider_start_index(model_total, model_viewport, model_selected);

    let mut model_lines: Vec<Line<'static>> = Vec::new();
    for model in model_entries.iter().skip(model_start).take(model_viewport) {
        let selected = model.selectable && model.id == selector.model;
        let style = if selected {
            Style::default()
                .fg(theme.text_accent)
                .bg(theme.bg_selected)
                .add_modifier(Modifier::BOLD)
        } else if model.selectable {
            theme.text_secondary()
        } else {
            theme.text_muted()
        };
        model_lines.push(Line::from(vec![
            Span::styled(
                if selected {
                    format!("{} ", chars::ARROW_RIGHT)
                } else {
                    "  ".to_string()
                },
                style,
            ),
            Span::styled(
                truncate_ellipsis(
                    &model.display,
                    models_inner.width.saturating_sub(3) as usize,
                ),
                style,
            ),
        ]));
    }

    f.render_widget(
        Paragraph::new(model_lines)
            .style(theme.text())
            .wrap(Wrap { trim: false }),
        models_inner,
    );

    if model_total > model_viewport.max(1) {
        let mut model_scroll =
            ScrollbarState::new(model_total).position(model_start.min(model_total - 1));
        f.render_stateful_widget(
            Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight),
            models_inner,
            &mut model_scroll,
        );
    }

    let fallback_model_view = ModelPageView {
        api_key_masked_input: String::new(),
        api_key_input_focused: false,
        cursor_visible: false,
        api_key_placeholder: "Paste API key here...".to_string(),
    };
    let inline_key_view = model_view.unwrap_or(&fallback_model_view);

    let key_border = if inline_key_view.api_key_input_focused {
        theme
            .border_active()
            .fg(theme.chat_user_name)
            .add_modifier(Modifier::BOLD)
    } else {
        theme.panel_border()
    };

    let mut key_lines: Vec<Line<'static>> = Vec::new();
    key_lines.push(Line::from(vec![
        Span::styled("Inline API key: ", theme.table_header()),
        Span::styled(
            if inline_key_view.api_key_input_focused {
                "focused"
            } else {
                "idle"
            },
            if inline_key_view.api_key_input_focused {
                theme.text_accent_bold().fg(theme.chat_user_name)
            } else {
                theme.text_secondary()
            },
        ),
    ]));

    let display = if inline_key_view.api_key_masked_input.is_empty() {
        inline_key_view.api_key_placeholder.clone()
    } else {
        inline_key_view.api_key_masked_input.clone()
    };
    let display_style = if inline_key_view.api_key_masked_input.is_empty() {
        theme.text_muted()
    } else {
        theme.text()
    };
    let cursor = if inline_key_view.api_key_input_focused && inline_key_view.cursor_visible {
        "_"
    } else {
        ""
    };

    key_lines.push(Line::from(vec![
        Span::styled("[ ", theme.text_secondary()),
        Span::styled(display, display_style),
        Span::styled(cursor, theme.prompt()),
        Span::styled(" ]", theme.text_secondary()),
    ]));
    key_lines.push(Line::from(Span::styled(
        "Press A to focus, Ctrl+V or terminal paste, Enter save, Esc cancel",
        theme.text_muted(),
    )));

    let key_panel = Paragraph::new(key_lines)
        .style(theme.text())
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(Span::styled(
                    if inline_key_view.api_key_input_focused {
                        " Quick Key Paste · EDITING "
                    } else {
                        " Quick Key Paste "
                    },
                    theme.panel_title(),
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(key_border),
        );
    f.render_widget(key_panel, detail_rows[3]);

    let detail_instruction = llm_provider_config_instruction(&selected_provider);
    let instruction_panel = Paragraph::new(Line::from(vec![
        Span::styled("Configure: ", theme.table_header()),
        Span::styled(
            truncate_ellipsis(
                detail_instruction,
                detail_rows[4].width.saturating_sub(13) as usize,
            ),
            theme.text_secondary(),
        ),
    ]))
    .style(theme.text())
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .title(Span::styled(" Quick Setup ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    );
    f.render_widget(instruction_panel, detail_rows[4]);

    let controls = Paragraph::new(Line::from(vec![
        Span::styled("[Enter] Activate provider   ", theme.text_accent_bold()),
        Span::styled("[M] Change model   ", theme.text_secondary()),
        Span::styled("[A] Auth / Key paste   ", theme.text_secondary()),
        Span::styled("[Esc] Back", theme.text_secondary()),
    ]))
    .style(theme.text())
    .block(
        Block::default()
            .title(Span::styled(" Keybinds ", theme.panel_title()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.panel_border()),
    );
    f.render_widget(controls, rows[1]);
}
