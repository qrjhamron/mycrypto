use super::*;

impl App {
    pub(super) fn render_state_hash(&self, size: Rect) -> u64 {
        let mut hasher = DefaultHasher::new();

        RENDER_HASH_VERSION.hash(&mut hasher);

        size.width.hash(&mut hasher);
        size.height.hash(&mut hasher);
        std::mem::discriminant(&self.page).hash(&mut hasher);
        self.show_logo.hash(&mut hasher);
        self.logo_offset.hash(&mut hasher);
        self.scroll.hash(&mut hasher);
        self.input.hash(&mut hasher);
        self.spinner_frame.hash(&mut hasher);
        self.ws_pulse_on.hash(&mut hasher);
        self.activity_joined.hash(&mut hasher);
        self.show_keybind_popup.hash(&mut hasher);
        self.customize_selected.hash(&mut hasher);
        self.customize_dirty.hash(&mut hasher);
        self.history_count.hash(&mut hasher);
        self.news_history_search_mode.hash(&mut hasher);
        self.news_history_query.hash(&mut hasher);
        std::mem::discriminant(&self.active_input_target).hash(&mut hasher);
        self.chat_auto_scroll.hash(&mut hasher);
        self.state.updated_at.timestamp_millis().hash(&mut hasher);
        self.state
            .portfolio
            .updated_at
            .timestamp_millis()
            .hash(&mut hasher);
        self.state.chat_messages.len().hash(&mut hasher);
        self.state.chart_timeframe.hash(&mut hasher);
        self.state.chart_pair.hash(&mut hasher);
        self.state.chart_offset.hash(&mut hasher);
        self.state.chart_zoom.hash(&mut hasher);
        self.state.chart_show_indicators.hash(&mut hasher);
        self.state.chart_show_sentiment.hash(&mut hasher);
        hash_agent_status(self.state.agent_status).hash(&mut hasher);

        self.state.config.agent.min_confidence.hash(&mut hasher);
        self.state.config.agent.max_open_trades.hash(&mut hasher);
        self.state.config.agent.scan_interval_sec.hash(&mut hasher);
        self.state.config.risk.risk_per_trade_pct.hash(&mut hasher);
        self.state
            .config
            .risk
            .max_daily_drawdown_pct
            .hash(&mut hasher);
        self.state.config.risk.max_position_pct.hash(&mut hasher);
        self.state.config.data.yahoo_enabled.hash(&mut hasher);
        self.state.config.data.coingecko_enabled.hash(&mut hasher);
        self.state.config.data.fear_greed_enabled.hash(&mut hasher);
        self.state.config.data.reddit_enabled.hash(&mut hasher);
        self.state.config.data.twitter_enabled.hash(&mut hasher);
        self.state.config.data.reuters_rss_enabled.hash(&mut hasher);
        self.state
            .config
            .data
            .bloomberg_rss_enabled
            .hash(&mut hasher);
        self.state.config.data.finnhub_enabled.hash(&mut hasher);
        self.state
            .config
            .tui
            .chart_default_timeframe
            .hash(&mut hasher);
        self.state.config.tui.log_lines.hash(&mut hasher);
        self.state.config.tui.theme.hash(&mut hasher);
        hash_llm_provider(&self.state.config.llm.provider).hash(&mut hasher);
        self.state.config.llm.model.hash(&mut hasher);
        self.state.config.pairs.watchlist.hash(&mut hasher);

        std::mem::discriminant(&self.confirm).hash(&mut hasher);
        match &self.confirm {
            ConfirmState::Buy { pair, size } => {
                pair.hash(&mut hasher);
                size.hash(&mut hasher);
            }
            ConfirmState::ClosePosition(pair) => {
                pair.hash(&mut hasher);
            }
            _ => {}
        }

        std::mem::discriminant(&self.auth_input_mode).hash(&mut hasher);
        match &self.auth_input_mode {
            AuthInputMode::ApiKey { provider, input } => {
                provider.hash(&mut hasher);
                input.hash(&mut hasher);
            }
            AuthInputMode::GradioUrl { input } => {
                input.hash(&mut hasher);
            }
            AuthInputMode::GradioToken { space_url, input } => {
                space_url.hash(&mut hasher);
                input.hash(&mut hasher);
            }
            AuthInputMode::Select => {}
        }

        self.autocomplete.visible.hash(&mut hasher);
        self.autocomplete.filter.hash(&mut hasher);
        self.autocomplete.selected_index.hash(&mut hasher);
        self.autocomplete.scroll_offset.hash(&mut hasher);

        hash_llm_provider(&self.model_selector.provider).hash(&mut hasher);
        self.model_selector.model.hash(&mut hasher);
        self.model_selector.provider_index.hash(&mut hasher);
        self.model_selector.model_index.hash(&mut hasher);
        self.model_selector.api_key_set.hash(&mut hasher);
        self.model_selector.connected.hash(&mut hasher);
        std::mem::discriminant(&self.model_input_mode).hash(&mut hasher);
        self.model_api_key_input.hash(&mut hasher);

        self.team_popup.is_some().hash(&mut hasher);
        if let Some(popup) = &self.team_popup {
            popup.selected_index.hash(&mut hasher);
            popup.edit_buffer.hash(&mut hasher);
        }

        for pair in ["BTCUSDT", "ETHUSDT"] {
            self.price_flash.contains_key(pair).hash(&mut hasher);
            if let Some(flash) = self.price_flash.get(pair) {
                flash.last_price.hash(&mut hasher);
                flash.ticks_remaining.hash(&mut hasher);
                flash.is_up.hash(&mut hasher);
            }
        }

        hasher.finish()
    }

    pub(super) fn compute_frame_rects(size: Rect) -> FrameRects {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Min(5),
                Constraint::Length(1),
                Constraint::Length(4),
            ])
            .split(size);

        let input_footer_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(1)])
            .split(layout[4]);

        FrameRects {
            header: layout[0],
            tabs: layout[1],
            main: layout[2],
            activity: layout[3],
            input: input_footer_layout[0],
            footer: input_footer_layout[1],
        }
    }

    pub(super) fn rects_overlap(a: Rect, b: Rect) -> bool {
        let a_right = a.x.saturating_add(a.width);
        let b_right = b.x.saturating_add(b.width);
        let a_bottom = a.y.saturating_add(a.height);
        let b_bottom = b.y.saturating_add(b.height);

        a.x < b_right && a_right > b.x && a.y < b_bottom && a_bottom > b.y
    }

    pub(super) fn assert_non_overlapping_rects(rects: &FrameRects) {
        let all = [
            rects.header,
            rects.tabs,
            rects.main,
            rects.activity,
            rects.input,
            rects.footer,
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                debug_assert!(!Self::rects_overlap(all[i], all[j]));
            }
        }
    }

    pub(super) fn paint_frame_background(&self, f: &mut Frame, area: Rect) {
        let style = Style::default()
            .fg(self.theme.text_primary)
            .bg(self.theme.bg_primary);
        let buf = f.buffer_mut();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_symbol(" ").set_style(style);
            }
        }
    }

    pub(super) fn render(&self, f: &mut Frame) {
        let size = f.area();
        self.paint_frame_background(f, size);

        if size.width < 70 || size.height < 16 {
            let msg = format!(
                "Terminal too small: {}x{} (need 70x16)",
                size.width, size.height
            );
            let paragraph = Paragraph::new(msg).style(self.theme.error());
            f.render_widget(paragraph, size);
            return;
        }

        if self.show_logo {
            render_splash_with_offset(f, size, &self.theme, self.logo_offset);
            return;
        }

        let rects = Self::compute_frame_rects(size);
        Self::assert_non_overlapping_rects(&rects);

        self.render_shell_header(f, rects.header);
        self.render_tabs(f, rects.tabs);
        self.render_main_area(f, rects.main);
        self.render_activity_strip(f, rects.activity);
        self.render_input_bar(f, rects.input);
        self.render_footer(f, rects.footer);

        if self.page == Page::Team {
            if let (Some(card), Some(popup)) = (
                self.state.team_discussion.pending_action.as_ref(),
                self.team_popup.as_ref(),
            ) {
                let mut summary = card.summary.clone();
                if !popup.edit_buffer.is_empty() {
                    summary = format!("{} (edited to {}%)", summary, popup.edit_buffer);
                }
                pages::render_team_action_popup(
                    f,
                    size,
                    &self.theme,
                    &summary,
                    popup.selected_index,
                );
            }
        }

        if self.show_keybind_popup {
            pages::render_keybind_popup(f, size, &self.theme, &self.page);
        }

        if self.autocomplete.visible && !self.show_keybind_popup {
            self.autocomplete
                .render(rects.input, f.buffer_mut(), &self.theme);
        }
    }

    pub(super) fn render_shell_header(&self, f: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let header_line = Rect::new(area.x, area.y, area.width, 1);
        let has_divider = area.height > 1;
        let divider_line = Rect::new(area.x, area.y.saturating_add(1), area.width, 1);

        let buf = f.buffer_mut();
        for x in header_line.x..header_line.x + header_line.width {
            buf[(x, header_line.y)]
                .set_symbol(" ")
                .set_style(self.theme.shell_header());
        }
        if has_divider {
            for x in divider_line.x..divider_line.x + divider_line.width {
                buf[(x, divider_line.y)]
                    .set_symbol(chars::LINE_H_DOUBLE)
                    .set_style(
                        Style::default()
                            .fg(self.theme.border_inactive)
                            .bg(self.theme.bg_primary),
                    );
            }
        }

        let mut spans = Vec::new();
        spans.push(Span::styled(" ", self.theme.shell_header()));
        spans.push(Span::styled("◈ ", self.theme.brand_bright()));
        for (idx, ch) in "mYcrypto".chars().enumerate() {
            let style = if idx % 2 == 0 {
                self.theme.brand_bright()
            } else {
                self.theme.brand_dim()
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
        spans.push(Span::styled("  ", self.theme.shell_header()));

        for pair in ["BTCUSDT", "ETHUSDT"] {
            let short = pair.replace("USDT", "");
            spans.push(Span::styled(
                format!("{} ", short),
                Style::default()
                    .fg(self.theme.text_accent)
                    .bg(self.theme.bg_elevated)
                    .add_modifier(Modifier::BOLD),
            ));

            if let Some(t) = self.state.get_ticker(pair) {
                let flash_style = self
                    .price_flash
                    .get(pair)
                    .filter(|s| s.ticks_remaining > 0)
                    .map(|s| {
                        if s.is_up {
                            self.theme.value_flash_up()
                        } else {
                            self.theme.value_flash_down()
                        }
                    });

                let price_style = flash_style.unwrap_or(
                    Style::default()
                        .fg(self.theme.text_primary)
                        .bg(self.theme.bg_elevated),
                );
                spans.push(Span::styled(
                    format!("{} ", format_price_compact(t.price)),
                    price_style,
                ));

                let up = t.price_change_pct_24h >= Decimal::ZERO;
                spans.push(Span::styled(
                    format!(
                        "{}{:.2}%  ",
                        if up {
                            chars::ARROW_UP
                        } else {
                            chars::ARROW_DOWN
                        },
                        t.price_change_pct_24h.abs()
                    ),
                    self.theme.price_change(up),
                ));
            } else {
                spans.push(Span::styled("--  ", self.theme.shell_footer()));
            }
        }

        let (sentiment_text, sentiment_style) = self.sentiment_pill();
        spans.push(Span::styled(" ", self.theme.shell_header()));
        spans.push(Span::styled(
            format!(" {} ", sentiment_text),
            sentiment_style,
        ));
        spans.push(Span::styled("  ", self.theme.shell_header()));

        let ws_connected = matches!(self.state.feed_status, ConnectionStatus::Connected);
        let ws_symbol = if ws_connected {
            if self.ws_pulse_on {
                chars::BULLET
            } else {
                chars::BULLET_EMPTY
            }
        } else {
            chars::BULLET
        };
        let ws_style = if ws_connected {
            self.theme.ws_connected()
        } else {
            self.theme.ws_disconnected()
        };
        spans.push(Span::styled("WS ", self.theme.shell_footer()));
        spans.push(Span::styled(ws_symbol, ws_style));
        spans.push(Span::styled("  ", self.theme.shell_header()));

        spans.push(Span::styled(
            Utc::now().format("%H:%M:%S UTC").to_string(),
            self.theme.shell_footer(),
        ));

        let mut used_chars = spans
            .iter()
            .map(|s| s.content.chars().count())
            .sum::<usize>();
        let width_chars = header_line.width as usize;
        if used_chars < width_chars {
            spans.push(Span::styled(
                " ".repeat(width_chars - used_chars),
                self.theme.shell_header(),
            ));
            used_chars = width_chars;
        }
        if used_chars > width_chars {
            let mut trimmed = Vec::new();
            let mut consumed = 0usize;
            for span in spans {
                if consumed >= width_chars {
                    break;
                }
                let text: Cow<'_, str> = match span.content {
                    Cow::Borrowed(s) => Cow::Borrowed(s),
                    Cow::Owned(ref s) => Cow::Borrowed(s.as_str()),
                };
                let remaining = width_chars - consumed;
                let count = text.chars().count();
                if count <= remaining {
                    consumed += count;
                    trimmed.push(span);
                } else {
                    let clipped: String = text.chars().take(remaining).collect();
                    consumed += clipped.chars().count();
                    trimmed.push(Span::styled(clipped, span.style));
                }
            }
            spans = trimmed;
        }

        let para = Paragraph::new(Line::from(spans)).style(self.theme.shell_header());
        f.render_widget(para, header_line);
    }

    pub(super) fn render_tabs(&self, f: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let line_area = Rect::new(area.x, area.y, area.width, 1);

        let row_style = self.theme.shell_footer();
        {
            let buf = f.buffer_mut();
            for x in line_area.x..line_area.x + line_area.width {
                buf[(x, line_area.y)].set_symbol(" ").set_style(row_style);
            }
        }

        let active_idx = self.active_tab_index();
        let labels = build_tab_labels_for_width(line_area.width as usize);
        let mut spans = Vec::new();
        for (idx, label) in labels.iter().enumerate() {
            let style = if idx == active_idx {
                self.theme.tab_active()
            } else {
                self.theme.tab_inactive()
            };
            spans.push(Span::styled(label.clone(), style));
        }

        let width_used = labels
            .iter()
            .map(|label| label.chars().count())
            .sum::<usize>();
        let full_width = line_area.width as usize;
        if width_used < full_width {
            spans.push(Span::styled(" ".repeat(full_width - width_used), row_style));
        }

        let para = Paragraph::new(Line::from(spans)).style(row_style);
        f.render_widget(para, line_area);
    }

    pub(super) fn render_activity_strip(&self, f: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let strip_area = Rect::new(area.x, area.y, area.width, 1);
        let indicator = "⚡ ";
        let content_width = (area.width as usize).saturating_sub(4);
        let text = if self.state.team_discussion.active {
            format!(
                "Agent Team Thinking... {}",
                chars::SPINNER[self.spinner_frame % chars::SPINNER.len()]
            )
        } else if self.activity_events.is_empty() {
            "activity: waiting for market + engine events".to_string()
        } else {
            marquee_text(&self.activity_joined, content_width, self.activity_offset)
        };

        let clipped_text: String = text.chars().take(content_width).collect();

        let line = Line::from(vec![
            Span::styled(indicator, self.theme.spinner()),
            Span::styled(clipped_text, self.theme.activity_strip()),
        ]);
        f.render_widget(
            Paragraph::new(line).style(self.theme.activity_strip()),
            strip_area,
        );
    }

    pub(super) fn render_footer(&self, f: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let line_area = Rect::new(area.x, area.y, area.width, 1);
        let hints = self.footer_hints();
        let max_hint_width = line_area.width.saturating_sub(2) as usize;
        let clipped: String = hints.chars().take(max_hint_width).collect();
        let padding_len = max_hint_width.saturating_sub(clipped.chars().count());
        let line = Line::from(vec![
            Span::styled(" ", self.theme.shell_footer()),
            Span::styled(clipped, self.theme.footer_hint()),
            Span::styled(" ".repeat(padding_len), self.theme.shell_footer()),
        ]);
        f.render_widget(
            Paragraph::new(line).style(self.theme.shell_footer()),
            line_area,
        );
    }

    pub(super) fn active_tab_index(&self) -> usize {
        match self.page {
            Page::Portfolio => 0,
            Page::Signals => 1,
            Page::Chart => 2,
            Page::Team => 3,
            Page::News => 4,
            Page::Heatmap => 5,
            Page::Status => 6,
            _ => 0,
        }
    }

    pub(super) fn set_tab_by_index(&mut self, idx: usize) {
        self.page = match idx {
            0 => Page::Portfolio,
            1 => Page::Signals,
            2 => Page::Chart,
            3 => Page::Team,
            4 => Page::News,
            5 => Page::Heatmap,
            6 => Page::Status,
            _ => self.page.clone(),
        };
        self.scroll = 0;
    }

    pub(super) fn navigate_tabs(&mut self, delta: isize) {
        let current = self.active_tab_index() as isize;
        let mut next = current + delta;
        if next < 0 {
            next = 6;
        }
        if next > 6 {
            next = 0;
        }
        self.set_tab_by_index(next as usize);
    }

    pub(super) fn footer_hints(&self) -> String {
        if self.show_keybind_popup {
            return "Esc/? close help".to_string();
        }
        if self.page == Page::Team && self.team_popup.is_some() {
            return "Y Execute  N Dismiss  E Edit Amount  D Re-analyze  Enter confirm".to_string();
        }
        if self.autocomplete.visible {
            return "↑/↓ select  Tab complete  Enter run  Esc close".to_string();
        }
        match self.page {
            Page::Portfolio => {
                "[1-7] tabs  ←/→ tabs  / command  ↑/↓ scroll  ? help  Ctrl+C exit".to_string()
            }
            Page::Signals => "[1-7] tabs  ↑/↓ scroll  Enter detail  /signals  ? help".to_string(),
            Page::Chart => {
                "[1-7] tabs  Tab pair  1/2/3/4/5 tf  [/] pan  +/- zoom  i indicators  s sentiment"
                    .to_string()
            }
            Page::Team => "[1-7] tabs  /team <prompt>  ↑/↓ scroll  ? help".to_string(),
            Page::News => "[1-7] tabs  ↑/↓ scroll  H history  /news".to_string(),
            Page::NewsHistory => "↑/↓ scroll  / search  H back  Esc back".to_string(),
            Page::Heatmap => "[1-7] tabs  auto heatmap from watchlist  ? help".to_string(),
            Page::Status => "[1-7] tabs  ↑/↓ scroll  /status".to_string(),
            _ => "[1-7] tabs  ←/→ tabs  / command  ? help".to_string(),
        }
    }

    pub(super) fn sentiment_pill(&self) -> (String, Style) {
        if let Some(sentiment) = &self.state.sentiment_score {
            if sentiment.composite >= 0.25 {
                (
                    format!("🟢 +{:.2}", sentiment.composite),
                    self.theme.sentiment_positive(),
                )
            } else if sentiment.composite <= -0.25 {
                (
                    format!("🔴 {:.2}", sentiment.composite),
                    self.theme.sentiment_negative(),
                )
            } else {
                (
                    format!("🟡 {:+.2}", sentiment.composite),
                    self.theme.sentiment_neutral(),
                )
            }
        } else {
            ("🟡 n/a".to_string(), self.theme.sentiment_neutral())
        }
    }

    pub(super) fn render_main_area(&self, f: &mut Frame, area: Rect) {
        let auth_view = self.auth_view_state();
        let model_view = self.model_view_state();
        let news_history_view = self.news_history_view_state();
        pages::render_page(
            f,
            area,
            pages::RenderPageParams {
                page: &self.page,
                state: &self.state,
                theme: &self.theme,
                scroll: self.scroll,
                history_count: self.history_count,
                model_selector: Some(&self.model_selector),
                model_view: model_view.as_ref(),
                auth_view: auth_view.as_ref(),
                news_history_view: news_history_view.as_ref(),
                spinner_frame: self.spinner_frame,
                customize_selected: self.customize_selected,
                customize_dirty: self.customize_dirty,
                source_names_sorted: Some(&self.source_names_sorted),
                source_status_preview: Some(&self.source_status_preview),
                chat_auto_scroll: self.chat_auto_scroll,
            },
        );
    }

    pub(super) fn auth_view_state(&self) -> Option<AuthPageView> {
        if self.page != Page::Auth {
            return None;
        }

        let selected_provider = AuthProvider::ALL
            .get(self.auth_selected_index)
            .copied()
            .unwrap_or(AuthProvider::GitHub);

        let input_mode = match &self.auth_input_mode {
            AuthInputMode::Select => AuthInputModeView::Select,
            AuthInputMode::ApiKey { provider, input } => AuthInputModeView::ApiKey {
                provider: *provider,
                masked_input: "●".repeat(input.chars().count()),
            },
            AuthInputMode::GradioUrl { input } => AuthInputModeView::GradioUrl {
                input: input.clone(),
            },
            AuthInputMode::GradioToken { space_url, input } => AuthInputModeView::GradioToken {
                space_url: space_url.clone(),
                masked_input: "●".repeat(input.chars().count()),
            },
        };

        Some(AuthPageView {
            selected_index: self.auth_selected_index,
            selected_provider,
            input_mode,
            error: self.auth_error.clone(),
            info: self.auth_message.clone(),
            input_focused: !matches!(self.auth_input_mode, AuthInputMode::Select),
            cursor_visible: self.spinner_frame.is_multiple_of(2),
        })
    }

    pub(super) fn model_view_state(&self) -> Option<ModelPageView> {
        if self.page != Page::Model {
            return None;
        }

        let focused = self.model_input_mode == ModelInputMode::ApiKey;
        let masked = if self.model_api_key_input.is_empty() {
            String::new()
        } else {
            "•".repeat(self.model_api_key_input.chars().count())
        };

        Some(ModelPageView {
            api_key_masked_input: masked,
            api_key_input_focused: focused,
            cursor_visible: focused && self.spinner_frame.is_multiple_of(2),
            api_key_placeholder: "Paste API key here...".to_string(),
        })
    }

    pub(super) fn news_history_view_state(&self) -> Option<NewsHistoryView> {
        if self.page != Page::NewsHistory {
            return None;
        }

        Some(NewsHistoryView {
            query: self.news_history_query.clone(),
            search_active: self.news_history_search_mode,
        })
    }

    pub(super) fn render_input_bar(&self, f: &mut Frame, area: Rect) {
        let buf = f.buffer_mut();

        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf[(x, y)].set_bg(self.theme.bg_elevated);
            }
        }

        let command_mode = self.input.starts_with('/');
        let chat_focused =
            self.page == Page::Chat && self.active_input_target == ActiveInputTarget::Chat;
        let border_style = if chat_focused {
            Style::default()
                .fg(self.theme.chat_user_name)
                .bg(self.theme.bg_elevated)
                .add_modifier(Modifier::BOLD)
        } else {
            self.theme.border_inactive().bg(self.theme.bg_elevated)
        };

        let inner_width = area.width.saturating_sub(2);

        buf[(area.x, area.y)]
            .set_symbol(chars::CORNER_TL)
            .set_style(border_style);
        for x in area.x + 1..area.x + area.width - 1 {
            buf[(x, area.y)]
                .set_symbol(chars::LINE_H)
                .set_style(border_style);
        }
        buf[(area.x + area.width - 1, area.y)]
            .set_symbol(chars::CORNER_TR)
            .set_style(border_style);

        let label = if chat_focused {
            " input · EDITING "
        } else {
            " input "
        };
        for (i, ch) in label.chars().enumerate() {
            let x = area.x + 2 + i as u16;
            if x < area.x + area.width - 1 {
                buf[(x, area.y)].set_char(ch).set_style(
                    Style::default()
                        .fg(self.theme.text_accent)
                        .bg(self.theme.bg_elevated),
                );
            }
        }

        let input_y = area.y + 1;
        buf[(area.x, input_y)]
            .set_symbol(chars::LINE_V)
            .set_style(border_style);
        buf[(area.x + area.width - 1, input_y)]
            .set_symbol(chars::LINE_V)
            .set_style(border_style);

        let mode_hint = if self.active_input_target != ActiveInputTarget::Chat {
            "routed elsewhere"
        } else if command_mode {
            "command mode"
        } else {
            "chat mode"
        };
        let input_char_count = self.input.chars().count();
        let char_counter = format!("{} chars", input_char_count);
        let hint_x = area.x
            + area
                .width
                .saturating_sub(mode_hint.chars().count() as u16 + 3);
        let counter_x = area
            .x
            .saturating_add(area.width)
            .saturating_sub(char_counter.chars().count() as u16 + 3);

        buf[(area.x + 2, input_y)].set_symbol("◈").set_style(
            Style::default()
                .fg(self.theme.chat_agent_name)
                .bg(self.theme.bg_elevated),
        );
        buf[(area.x + 4, input_y)].set_symbol(">").set_style(
            Style::default()
                .fg(self.theme.chat_user_name)
                .bg(self.theme.bg_elevated)
                .add_modifier(Modifier::BOLD),
        );

        let input_area_width = inner_width.saturating_sub(8) as usize;
        let display_input: Cow<'_, str> = if input_char_count > input_area_width {
            Cow::Owned(
                self.input
                    .chars()
                    .skip(input_char_count - input_area_width)
                    .collect::<String>(),
            )
        } else {
            Cow::Borrowed(self.input.as_str())
        };

        for (i, ch) in display_input.chars().enumerate() {
            let x = area.x + 6 + i as u16;
            if x < hint_x.saturating_sub(2) {
                buf[(x, input_y)]
                    .set_char(ch)
                    .set_style(self.theme.input_bg().bg(self.theme.bg_elevated));
            }
        }

        let cursor_x = area.x + 6 + display_input.chars().count() as u16;
        if cursor_x < hint_x.saturating_sub(1) {
            buf[(cursor_x, input_y)]
                .set_char('_')
                .set_style(self.theme.prompt().bg(self.theme.bg_elevated));
        }

        for (i, ch) in mode_hint.chars().enumerate() {
            let x = hint_x + i as u16;
            if x < counter_x.saturating_sub(1) {
                buf[(x, input_y)].set_char(ch).set_style(
                    Style::default()
                        .fg(self.theme.text_muted)
                        .bg(self.theme.bg_elevated),
                );
            }
        }

        let counter_y = area.y + 2;
        for (i, ch) in char_counter.chars().enumerate() {
            let x = counter_x + i as u16;
            if x < area.x + area.width - 1 {
                buf[(x, counter_y)].set_char(ch).set_style(
                    Style::default()
                        .fg(self.theme.text_muted)
                        .bg(self.theme.bg_elevated),
                );
            }
        }

        buf[(area.x, area.y + 2)]
            .set_symbol(chars::CORNER_BL)
            .set_style(border_style);
        for x in area.x + 1..area.x + area.width - 1 {
            buf[(x, area.y + 2)]
                .set_symbol(chars::LINE_H)
                .set_style(border_style);
        }
        buf[(area.x + area.width - 1, area.y + 2)]
            .set_symbol(chars::CORNER_BR)
            .set_style(border_style);

        if let Some(error) = &self.error_message {
            let overlay = Paragraph::new(Line::from(Span::styled(
                format!("Error: {}", error),
                self.theme.error(),
            )));
            let err_area = Rect::new(
                area.x + 2,
                area.y.saturating_sub(1),
                area.width.saturating_sub(4),
                1,
            );
            f.render_widget(overlay, err_area);
        }

        let confirm_message = match &self.confirm {
            ConfirmState::Exit => Some("Exit mYcrypto? [y/N]".to_string()),
            ConfirmState::Reset => Some("Reset paper portfolio? [y/N]".to_string()),
            ConfirmState::Buy { pair, size } => {
                Some(format!("Buy {} {} at market? [y/N]", pair, size))
            }
            ConfirmState::ClosePosition(_) => Some("Close selected position? [y/N]".to_string()),
            ConfirmState::None => None,
        };
        if let Some(message) = confirm_message {
            let overlay = Paragraph::new(Line::from(Span::styled(message, self.theme.warning())));
            let msg_area = Rect::new(
                area.x + 2,
                area.y.saturating_sub(1),
                area.width.saturating_sub(4),
                1,
            );
            f.render_widget(overlay, msg_area);
        }
    }
}
