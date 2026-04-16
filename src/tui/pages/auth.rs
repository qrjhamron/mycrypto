use super::*;

pub(super) fn render_auth_page(
    f: &mut Frame,
    area: Rect,
    state: &AppState,
    theme: &Theme,
    auth_view: Option<&AuthPageView>,
    spinner_frame: usize,
) {
    let fallback = AuthPageView {
        selected_index: 0,
        selected_provider: AuthProvider::GitHub,
        input_mode: AuthInputModeView::Select,
        error: None,
        info: None,
        input_focused: false,
        cursor_visible: false,
    };

    let view = auth_view.unwrap_or(&fallback);

    let shell = Block::default()
        .title(Span::styled(" Authentication ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let content = shell.inner(area);
    f.render_widget(shell, area);

    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(content);

    let provider_block = Block::default()
        .title(Span::styled(" Providers ", theme.panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.panel_border());
    let provider_inner = provider_block.inner(split[0]);
    f.render_widget(provider_block, split[0]);

    let detail_border_style = if view.input_focused {
        theme
            .border_active()
            .fg(theme.chat_user_name)
            .add_modifier(Modifier::BOLD)
    } else {
        theme.panel_border()
    };

    let detail_block = Block::default()
        .title(Span::styled(
            if view.input_focused {
                " Instructions · EDITING "
            } else {
                " Instructions "
            },
            theme.panel_title(),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(detail_border_style);
    let detail_inner = detail_block.inner(split[1]);
    f.render_widget(detail_block, split[1]);

    let mut provider_lines: Vec<Line<'static>> = Vec::new();
    provider_lines.push(Line::from(Span::styled(
        "◈ AUTHENTICATION CENTER",
        theme.section_header(),
    )));
    provider_lines.push(Line::from(""));

    for (idx, provider) in AuthProvider::ALL.iter().enumerate() {
        let selected = idx == view.selected_index;
        let marker = if selected {
            chars::ARROW_RIGHT.to_string()
        } else {
            " ".to_string()
        };
        let row_style = if selected {
            Style::default()
                .fg(theme.text_primary)
                .bg(theme.bg_selected)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.text()
        };
        let (badge, badge_style) = provider_status_badge(state, *provider, spinner_frame, theme);

        provider_lines.push(Line::from(vec![
            Span::styled(format!("{} ", marker), row_style),
            Span::styled(format!("{:<15}", provider.display_name()), row_style),
            Span::styled(badge, badge_style),
        ]));
    }

    provider_lines.push(Line::from(""));
    provider_lines.push(Line::from(Span::styled(
        "[↑↓] Select  [Enter] Auth  [D] Delete  [Esc] Back",
        theme.text_muted(),
    )));

    f.render_widget(
        Paragraph::new(provider_lines)
            .style(theme.text())
            .wrap(Wrap { trim: false }),
        provider_inner,
    );

    let mut detail_lines: Vec<Line<'static>> = Vec::new();
    detail_lines.push(Line::from(Span::styled(
        format!("Provider: {}", view.selected_provider.display_name()),
        theme.text_accent_bold(),
    )));
    detail_lines.push(Line::from(Span::styled(
        format!("Method: {}", view.selected_provider.auth_method()),
        theme.text_secondary(),
    )));
    detail_lines.push(Line::from(Span::styled(
        format!(
            "Status: {}",
            llm_provider_current_masked_status(
                state,
                &auth_to_llm_provider(view.selected_provider)
            )
        ),
        theme.text_secondary(),
    )));
    detail_lines.push(Line::from(""));

    match &view.input_mode {
        AuthInputModeView::Select => {
            detail_lines.push(Line::from(Span::styled("Step 1", theme.table_header())));
            detail_lines.push(Line::from(Span::styled(
                "Use ↑/↓ to choose provider on the left panel.",
                theme.text_secondary(),
            )));
            detail_lines.push(Line::from(Span::styled("Step 2", theme.table_header())));
            detail_lines.push(Line::from(Span::styled(
                "Press Enter to start provider setup flow.",
                theme.text_secondary(),
            )));
            detail_lines.push(Line::from(Span::styled("Step 3", theme.table_header())));
            detail_lines.push(Line::from(Span::styled(
                "Press D to remove stored auth for selected provider.",
                theme.text_secondary(),
            )));
        }
        AuthInputModeView::ApiKey {
            provider,
            masked_input,
        } => {
            detail_lines.push(Line::from(Span::styled(
                format!("{} API key setup", provider.display_name()),
                theme.text_accent_bold(),
            )));
            detail_lines.push(Line::from(Span::styled(
                "Paste key in the input field and press Enter to save.",
                theme.text_secondary(),
            )));
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled("Input", theme.table_header())));
            let placeholder = "Paste API key here...";
            let display = if masked_input.is_empty() {
                placeholder.to_string()
            } else {
                masked_input.clone()
            };
            let cursor = if view.cursor_visible { "_" } else { "" };
            detail_lines.push(Line::from(Span::styled(
                format!("[ {}{} ]", display, cursor),
                if masked_input.is_empty() {
                    Style::default().fg(theme.text_muted).bg(theme.bg_selected)
                } else {
                    Style::default()
                        .fg(theme.text_primary)
                        .bg(theme.bg_selected)
                },
            )));
            detail_lines.push(Line::from(Span::styled(
                format!(
                    "Current saved: {}",
                    llm_provider_current_masked_status(state, &auth_to_llm_provider(*provider))
                ),
                theme.text_secondary(),
            )));
            detail_lines.push(Line::from(Span::styled(
                "Saved to ~/.mycrypto/keys.json with 0600 permissions. Ctrl+V supported.",
                theme.text_muted(),
            )));
        }
        AuthInputModeView::GradioUrl { input } => {
            detail_lines.push(Line::from(Span::styled(
                "Gradio setup",
                theme.text_accent_bold(),
            )));
            detail_lines.push(Line::from(Span::styled(
                "Enter Space URL then press Enter.",
                theme.text_secondary(),
            )));
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled("Space URL", theme.table_header())));
            let cursor = if view.cursor_visible { "_" } else { "" };
            detail_lines.push(Line::from(Span::styled(
                format!("[ {}{} ]", input, cursor),
                Style::default()
                    .fg(theme.text_primary)
                    .bg(theme.bg_selected),
            )));
        }
        AuthInputModeView::GradioToken {
            space_url,
            masked_input,
        } => {
            detail_lines.push(Line::from(Span::styled(
                "Gradio token (optional)",
                theme.text_accent_bold(),
            )));
            detail_lines.push(Line::from(Span::styled(
                format!("Space: {}", space_url),
                theme.text_secondary(),
            )));
            detail_lines.push(Line::from(Span::styled(
                "Leave blank for public spaces.",
                theme.text_secondary(),
            )));
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled("Token", theme.table_header())));
            let display = if masked_input.is_empty() {
                "(optional)".to_string()
            } else {
                masked_input.clone()
            };
            let cursor = if view.cursor_visible { "_" } else { "" };
            detail_lines.push(Line::from(Span::styled(
                format!("[ {}{} ]", display, cursor),
                Style::default()
                    .fg(theme.text_primary)
                    .bg(theme.bg_selected),
            )));
        }
    }

    if let Some(AuthStatus::PendingDevice {
        user_code,
        verification_uri,
        expires_at,
        ..
    }) = state.auth_state.get(&AuthProvider::GitHub)
    {
        let remaining = expires_at.saturating_duration_since(Instant::now());
        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from(Span::styled(
            "GitHub device flow",
            theme.text_accent_bold(),
        )));
        detail_lines.push(Line::from(Span::styled(
            format!("Code: {}", user_code),
            Style::default()
                .fg(theme.text_primary)
                .bg(theme.bg_selected),
        )));
        detail_lines.push(Line::from(Span::styled(
            format!("Verify URL: {}", verification_uri),
            Style::default()
                .fg(theme.text_primary)
                .bg(theme.bg_selected),
        )));
        detail_lines.push(Line::from(Span::styled(
            format!("Expires in {}", format_duration(remaining)),
            Style::default().fg(theme.signal_wait),
        )));
    }

    if let Some(message) = &view.info {
        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from(Span::styled(
            message.clone(),
            Style::default().fg(theme.profit),
        )));
    }

    if let Some(message) = &view.error {
        detail_lines.push(Line::from(""));
        detail_lines.push(Line::from(Span::styled(message.clone(), theme.error())));
    }

    f.render_widget(
        Paragraph::new(detail_lines)
            .style(theme.text())
            .wrap(Wrap { trim: false }),
        detail_inner,
    );
}
