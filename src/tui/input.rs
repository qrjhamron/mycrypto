use super::*;

impl App {
    pub(super) fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.refresh_active_input_target();
        self.error_message = None;

        if self.show_keybind_popup {
            if code == KeyCode::Esc || code == KeyCode::Char('?') {
                self.show_keybind_popup = false;
            }
            return;
        }

        if !matches!(self.confirm, ConfirmState::None) {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let confirm = std::mem::replace(&mut self.confirm, ConfirmState::None);
                    match confirm {
                        ConfirmState::Exit => self.perform_exit_shutdown(),
                        ConfirmState::Reset => self.do_reset(),
                        ConfirmState::Buy { pair, size } => self.do_buy_position(&pair, size),
                        ConfirmState::ClosePosition(pair) => self.do_close_position(&pair),
                        ConfirmState::None => {}
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.confirm = ConfirmState::None;
                }
                _ => {}
            }
            return;
        }

        if self.page == Page::Team && self.team_popup.is_some() {
            match code {
                KeyCode::Left => {
                    if let Some(popup) = self.team_popup.as_mut() {
                        if popup.selected_index == 0 {
                            popup.selected_index = TeamPopupOption::ALL.len() - 1;
                        } else {
                            popup.selected_index -= 1;
                        }
                    }
                    return;
                }
                KeyCode::Right | KeyCode::Tab => {
                    if let Some(popup) = self.team_popup.as_mut() {
                        popup.selected_index =
                            (popup.selected_index + 1) % TeamPopupOption::ALL.len();
                    }
                    return;
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.execute_team_action();
                    return;
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.dismiss_team_action();
                    return;
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    self.request_team_reanalysis();
                    return;
                }
                KeyCode::Char('e') | KeyCode::Char('E') => {
                    if let Some(popup) = self.team_popup.as_mut() {
                        popup.selected_index = 2;
                    }
                    return;
                }
                KeyCode::Char(c) => {
                    if let Some(popup) = self.team_popup.as_mut() {
                        if TeamPopupOption::ALL[popup.selected_index] == TeamPopupOption::EditAmount
                            && c.is_ascii_digit()
                        {
                            popup.edit_buffer.push(c);
                            return;
                        }
                    }
                }
                KeyCode::Backspace => {
                    if let Some(popup) = self.team_popup.as_mut() {
                        if TeamPopupOption::ALL[popup.selected_index] == TeamPopupOption::EditAmount
                        {
                            popup.edit_buffer.pop();
                            return;
                        }
                    }
                }
                KeyCode::Enter => {
                    if let Some(popup) = self.team_popup.as_ref() {
                        match TeamPopupOption::ALL[popup.selected_index] {
                            TeamPopupOption::Execute => self.execute_team_action(),
                            TeamPopupOption::Dismiss => self.dismiss_team_action(),
                            TeamPopupOption::EditAmount => self.apply_edited_team_amount(),
                            TeamPopupOption::Reanalyze => self.request_team_reanalysis(),
                        }
                    }
                    return;
                }
                KeyCode::Esc => {
                    self.dismiss_team_action();
                    return;
                }
                _ => {}
            }
        }

        if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            self.confirm = ConfirmState::Exit;
            return;
        }

        if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('l') {
            self.execute_command(Command::Clear);
            return;
        }

        if modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('v')) {
            if !self.handle_ctrl_v_paste() {
                self.error_message = Some(
                    "Clipboard paste unavailable here. Use terminal paste or bracketed paste."
                        .to_string(),
                );
            }
            return;
        }

        if self.handle_targeted_input_key(code) {
            return;
        }

        if code == KeyCode::F(1) || code == KeyCode::Char('?') {
            self.show_keybind_popup = true;
            return;
        }

        if code == KeyCode::F(5) {
            self.auth_message = Some("Refresh requested".to_string());
            return;
        }

        if self.autocomplete.visible {
            match code {
                KeyCode::Up => {
                    self.autocomplete.prev();
                    return;
                }
                KeyCode::Down => {
                    self.autocomplete.next();
                    return;
                }
                KeyCode::Tab | KeyCode::Right => {
                    if let Some(cmd) = self.autocomplete.selected_command() {
                        self.input = cmd.to_string();
                        self.autocomplete.hide();
                    }
                    return;
                }
                KeyCode::Enter => {
                    if let Some(cmd) = self.autocomplete.selected_command() {
                        self.input = cmd.to_string();
                        self.autocomplete.hide();
                        self.submit_input();
                    }
                    return;
                }
                KeyCode::Esc | KeyCode::Char(' ') => {
                    self.autocomplete.hide();
                    if code == KeyCode::Char(' ') {
                        self.input.push(' ');
                    }
                    return;
                }
                _ => {}
            }
        }

        if self.page == Page::Chart && self.input.is_empty() && modifiers.is_empty() {
            match code {
                KeyCode::Char('1') => {
                    self.state.chart_timeframe = Timeframe::H1;
                    return;
                }
                KeyCode::Char('2') => {
                    self.state.chart_timeframe = Timeframe::H4;
                    return;
                }
                KeyCode::Char('3') => {
                    self.state.chart_timeframe = Timeframe::D1;
                    return;
                }
                KeyCode::Char('4') => {
                    self.state.chart_timeframe = Timeframe::W1;
                    return;
                }
                KeyCode::Char('5') => {
                    self.state.chart_timeframe = Timeframe::MO1;
                    return;
                }
                _ => {}
            }
        }

        if self.input.is_empty() && modifiers.is_empty() {
            match code {
                KeyCode::Char('1') => {
                    self.set_tab_by_index(0);
                    return;
                }
                KeyCode::Char('2') => {
                    self.set_tab_by_index(1);
                    return;
                }
                KeyCode::Char('3') => {
                    self.set_tab_by_index(2);
                    return;
                }
                KeyCode::Char('4') => {
                    self.set_tab_by_index(3);
                    return;
                }
                KeyCode::Char('5') => {
                    self.set_tab_by_index(4);
                    return;
                }
                KeyCode::Char('6') => {
                    self.set_tab_by_index(5);
                    return;
                }
                KeyCode::Char('7') => {
                    self.set_tab_by_index(6);
                    return;
                }
                KeyCode::Left => {
                    self.navigate_tabs(-1);
                    return;
                }
                KeyCode::Right => {
                    self.navigate_tabs(1);
                    return;
                }
                _ => {}
            }
        }

        if self.page == Page::Model {
            match code {
                KeyCode::Up => {
                    self.model_selector.prev_provider();
                    self.sync_model_selector_auth();
                    self.sync_model_selector_model();
                }
                KeyCode::Down => {
                    self.model_selector.next_provider();
                    self.sync_model_selector_auth();
                    self.sync_model_selector_model();
                }
                KeyCode::Char('m') | KeyCode::Char('M') | KeyCode::Tab => {
                    self.cycle_model_for_current_provider();
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    if matches!(
                        self.model_selector.provider,
                        LlmProvider::Claude
                            | LlmProvider::OpenAI
                            | LlmProvider::Gemini
                            | LlmProvider::OpenRouter
                    ) {
                        self.model_input_mode = ModelInputMode::ApiKey;
                        self.model_api_key_input.clear();
                    } else if let Some(auth_provider) =
                        llm_provider_auth_provider(&self.model_selector.provider)
                    {
                        if let Some(index) = AuthProvider::ALL
                            .iter()
                            .position(|provider| *provider == auth_provider)
                        {
                            self.auth_selected_index = index;
                            self.auth_input_mode = AuthInputMode::Select;
                            self.page = Page::Auth;
                            self.start_auth_for_selected();
                        }
                    } else {
                        self.error_message =
                            Some("Mock provider does not require authentication".to_string());
                    }
                }
                KeyCode::Enter => {
                    self.state.config.llm.provider = self.model_selector.provider.clone();
                    self.state.config.llm.model = self.model_selector.model.clone();
                    if self.model_selector.provider != LlmProvider::Mock
                        && !self.model_selector.api_key_set
                    {
                        self.error_message = Some(
                            "Provider selected; authenticate via /auth to enable it".to_string(),
                        );
                    } else {
                        self.error_message = Some(format!(
                            "Activated {} / {}",
                            self.model_selector.provider, self.model_selector.model
                        ));
                    }
                    self.rebuild_chat_engine();
                }
                KeyCode::Esc => self.page = Page::Portfolio,
                _ => {}
            }
            return;
        }

        if self.page == Page::Auth {
            let _ = self.handle_auth_key(code);
            self.refresh_active_input_target();
            return;
        }

        if self.page == Page::Customize {
            match code {
                KeyCode::Up => {
                    self.customize_selected = self.customize_selected.saturating_sub(1);
                    return;
                }
                KeyCode::Down => {
                    self.customize_selected =
                        (self.customize_selected + 1).min(CUSTOMIZE_FIELD_COUNT.saturating_sub(1));
                    return;
                }
                KeyCode::Left => {
                    if self.adjust_customize_value(-1) {
                        self.customize_dirty = true;
                    }
                    return;
                }
                KeyCode::Right => {
                    if self.adjust_customize_value(1) {
                        self.customize_dirty = true;
                    }
                    return;
                }
                KeyCode::Char(' ') => {
                    if self.toggle_customize_source() {
                        self.customize_dirty = true;
                    }
                    return;
                }
                KeyCode::Enter => {
                    if self.adjust_customize_value(1) {
                        self.customize_dirty = true;
                    }
                    return;
                }
                KeyCode::Char('s') if modifiers.contains(KeyModifiers::CONTROL) => {
                    self.save_customize_snapshot();
                    self.customize_dirty = false;
                    self.error_message = Some("Saved config.toml".to_string());
                    return;
                }
                KeyCode::Esc => {
                    if self.customize_dirty {
                        self.restore_customize_snapshot();
                        self.customize_dirty = false;
                    }
                    self.page = Page::Portfolio;
                    return;
                }
                _ => {}
            }
        }

        if self.page == Page::Chart {
            match code {
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    self.state.chart_timeframe = crate::state::Timeframe::H4;
                    return;
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    self.state.chart_timeframe = crate::state::Timeframe::D1;
                    return;
                }
                KeyCode::Char('w') | KeyCode::Char('W') => {
                    self.state.chart_timeframe = crate::state::Timeframe::W1;
                    return;
                }
                KeyCode::Tab => {
                    if !self.state.config.pairs.watchlist.is_empty() {
                        let current = self
                            .state
                            .config
                            .pairs
                            .watchlist
                            .iter()
                            .position(|p| p == &self.state.chart_pair)
                            .unwrap_or(0);
                        let next = (current + 1) % self.state.config.pairs.watchlist.len();
                        self.state.chart_pair = self.state.config.pairs.watchlist[next].clone();
                    }
                    return;
                }
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    self.state.chart_show_indicators = !self.state.chart_show_indicators;
                    return;
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    self.state.chart_show_sentiment = !self.state.chart_show_sentiment;
                    return;
                }
                KeyCode::Char('[') => {
                    self.state.chart_offset = self.state.chart_offset.saturating_add(1);
                    return;
                }
                KeyCode::Char(']') => {
                    self.state.chart_offset = self.state.chart_offset.saturating_sub(1);
                    return;
                }
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    self.state.chart_zoom = self.state.chart_zoom.saturating_sub(8).max(16);
                    return;
                }
                KeyCode::Char('-') => {
                    self.state.chart_zoom = (self.state.chart_zoom + 8).min(240);
                    return;
                }
                KeyCode::Esc => {
                    self.page = Page::Portfolio;
                    return;
                }
                _ => {}
            }
        }

        if self.page == Page::News {
            match code {
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    self.page = Page::NewsHistory;
                    self.scroll = 0;
                    self.news_history_search_mode = false;
                    self.refresh_active_input_target();
                    return;
                }
                _ => {}
            }
        }

        if self.page == Page::NewsHistory {
            match code {
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    self.page = Page::News;
                    self.scroll = 0;
                    self.news_history_search_mode = false;
                    self.refresh_active_input_target();
                    return;
                }
                KeyCode::Char('/') => {
                    self.news_history_search_mode = true;
                    self.refresh_active_input_target();
                    return;
                }
                KeyCode::Esc => {
                    self.page = Page::News;
                    self.scroll = 0;
                    self.news_history_search_mode = false;
                    self.refresh_active_input_target();
                    return;
                }
                _ => {}
            }
        }

        if self.page == Page::Team && code == KeyCode::Esc {
            self.page = Page::Portfolio;
            return;
        }

        if self.page == Page::TeamHistory && code == KeyCode::Esc {
            self.page = Page::Portfolio;
            return;
        }

        match code {
            KeyCode::Char(c) => {
                self.input.push(c);
                if self.input.starts_with('/') {
                    self.autocomplete.show();
                    self.autocomplete.set_filter(&self.input);
                } else {
                    self.autocomplete.hide();
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
                if self.input.starts_with('/') {
                    self.autocomplete.set_filter(&self.input);
                } else {
                    self.autocomplete.hide();
                }
            }
            KeyCode::Enter => {
                self.autocomplete.hide();
                self.submit_input();
            }
            KeyCode::Esc => {
                self.input.clear();
                self.autocomplete.hide();
            }
            KeyCode::Up => {
                if self.page == Page::Chat {
                    self.chat_auto_scroll = false;
                }
                self.scroll = self.scroll.saturating_sub(1);
            }
            KeyCode::Down => {
                self.scroll += 1;
                if self.page == Page::Chat {
                    let max_scroll = self.chat_max_scroll();
                    if self.scroll >= max_scroll {
                        self.chat_auto_scroll = true;
                        self.scroll = max_scroll;
                    }
                }
            }
            KeyCode::PageUp => {
                if self.page == Page::Chat {
                    self.chat_auto_scroll = false;
                }
                self.scroll = self.scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.scroll += 10;
                if self.page == Page::Chat {
                    let max_scroll = self.chat_max_scroll();
                    if self.scroll >= max_scroll {
                        self.chat_auto_scroll = true;
                        self.scroll = max_scroll;
                    }
                }
            }
            KeyCode::Home => {
                self.scroll = 0;
                if self.page == Page::Chat {
                    self.chat_auto_scroll = false;
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_auth_key(&mut self, code: KeyCode) -> bool {
        self.auth_error = None;
        self.auth_message = None;

        match &mut self.auth_input_mode {
            AuthInputMode::Select => match code {
                KeyCode::Up => {
                    if self.auth_selected_index == 0 {
                        self.auth_selected_index = AuthProvider::ALL.len() - 1;
                    } else {
                        self.auth_selected_index -= 1;
                    }
                    true
                }
                KeyCode::Down => {
                    self.auth_selected_index =
                        (self.auth_selected_index + 1) % AuthProvider::ALL.len();
                    true
                }
                KeyCode::Enter => {
                    self.start_auth_for_selected();
                    true
                }
                KeyCode::Char('d') | KeyCode::Char('D') => {
                    self.delete_auth_for_selected();
                    true
                }
                KeyCode::Esc => {
                    self.page = Page::Portfolio;
                    true
                }
                KeyCode::Tab => {
                    self.auth_selected_index =
                        (self.auth_selected_index + 1) % AuthProvider::ALL.len();
                    true
                }
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    if self.selected_auth_provider() == AuthProvider::GitHub {
                        self.start_github_auth();
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            },
            AuthInputMode::ApiKey { provider, input } => match code {
                KeyCode::Char(c) => {
                    input.push(c);
                    true
                }
                KeyCode::Backspace => {
                    input.pop();
                    true
                }
                KeyCode::Enter => {
                    if input.trim().is_empty() {
                        self.auth_error = Some("API key cannot be empty".to_string());
                        return true;
                    }

                    let value = input.trim().to_string();
                    match set_api_key(*provider, value) {
                        Ok(store) => {
                            apply_keys_to_env(&store);
                            sync_auth_state(&store, &mut self.state.auth_state);
                            self.auth_message =
                                Some(format!("{} key saved", provider.display_name()));
                            self.auth_input_mode = AuthInputMode::Select;
                            self.rebuild_chat_engine();
                        }
                        Err(err) => {
                            self.auth_error = Some(format!("Failed to save key: {}", err));
                        }
                    }
                    true
                }
                KeyCode::Esc => {
                    self.auth_input_mode = AuthInputMode::Select;
                    true
                }
                _ => true,
            },
            AuthInputMode::GradioUrl { input } => match code {
                KeyCode::Char(c) => {
                    input.push(c);
                    true
                }
                KeyCode::Backspace => {
                    input.pop();
                    true
                }
                KeyCode::Enter => {
                    if input.trim().is_empty() {
                        self.auth_error = Some("Space URL is required".to_string());
                    } else {
                        let url = input.trim().to_string();
                        self.auth_input_mode = AuthInputMode::GradioToken {
                            space_url: url,
                            input: String::new(),
                        };
                    }
                    true
                }
                KeyCode::Esc => {
                    self.auth_input_mode = AuthInputMode::Select;
                    true
                }
                _ => true,
            },
            AuthInputMode::GradioToken { space_url, input } => match code {
                KeyCode::Char(c) => {
                    input.push(c);
                    true
                }
                KeyCode::Backspace => {
                    input.pop();
                    true
                }
                KeyCode::Enter => {
                    let token = if input.trim().is_empty() {
                        None
                    } else {
                        Some(input.trim().to_string())
                    };

                    match set_gradio(space_url.clone(), token) {
                        Ok(store) => {
                            apply_keys_to_env(&store);
                            sync_auth_state(&store, &mut self.state.auth_state);
                            self.auth_message = Some("Gradio credentials saved".to_string());
                            self.auth_input_mode = AuthInputMode::Select;
                            self.rebuild_chat_engine();
                        }
                        Err(err) => {
                            self.auth_error = Some(format!("Failed to save gradio auth: {}", err));
                        }
                    }
                    true
                }
                KeyCode::Esc => {
                    self.auth_input_mode = AuthInputMode::Select;
                    true
                }
                _ => true,
            },
        }
    }

    pub(super) fn selected_auth_provider(&self) -> AuthProvider {
        AuthProvider::ALL
            .get(self.auth_selected_index)
            .copied()
            .unwrap_or(AuthProvider::GitHub)
    }

    pub(super) fn start_auth_for_selected(&mut self) {
        let provider = self.selected_auth_provider();

        match provider {
            AuthProvider::GitHub => {
                self.start_github_auth();
            }
            AuthProvider::Gradio => {
                self.auth_input_mode = AuthInputMode::GradioUrl {
                    input: String::new(),
                };
            }
            p if p.is_api_key_provider() => {
                self.auth_input_mode = AuthInputMode::ApiKey {
                    provider: p,
                    input: String::new(),
                };
            }
            _ => {}
        }
    }

    pub(super) fn handle_paste_text(&mut self, pasted: &str) {
        self.refresh_active_input_target();
        let cleaned = pasted.replace('\r', "");
        if cleaned.is_empty() {
            return;
        }

        match self.active_input_target {
            ActiveInputTarget::AuthApiKey => {
                if let AuthInputMode::ApiKey { input, .. } = &mut self.auth_input_mode {
                    input.push_str(&cleaned);
                }
            }
            ActiveInputTarget::AuthGradioUrl => {
                if let AuthInputMode::GradioUrl { input } = &mut self.auth_input_mode {
                    input.push_str(&cleaned);
                }
            }
            ActiveInputTarget::AuthGradioToken => {
                if let AuthInputMode::GradioToken { input, .. } = &mut self.auth_input_mode {
                    input.push_str(&cleaned);
                }
            }
            ActiveInputTarget::ModelSearch => {
                self.model_api_key_input.push_str(&cleaned);
            }
            ActiveInputTarget::NewsFilter => {
                self.news_history_query.push_str(&cleaned);
            }
            ActiveInputTarget::Chat => {
                self.input.push_str(&cleaned);
                if self.input.starts_with('/') {
                    self.autocomplete.show();
                    self.autocomplete.set_filter(&self.input);
                } else {
                    self.autocomplete.hide();
                }
            }
        }
    }

    pub(super) fn handle_ctrl_v_paste(&mut self) -> bool {
        self.refresh_active_input_target();

        let Some(text) = read_clipboard_text() else {
            return false;
        };
        self.handle_paste_text(&text);
        true
    }

    fn handle_targeted_input_key(&mut self, code: KeyCode) -> bool {
        match self.active_input_target {
            ActiveInputTarget::AuthApiKey
            | ActiveInputTarget::AuthGradioUrl
            | ActiveInputTarget::AuthGradioToken => {
                let consumed = self.handle_auth_key(code);
                self.refresh_active_input_target();
                consumed
            }
            ActiveInputTarget::ModelSearch => match code {
                KeyCode::Char(c) => {
                    self.model_api_key_input.push(c);
                    true
                }
                KeyCode::Backspace => {
                    self.model_api_key_input.pop();
                    true
                }
                KeyCode::Enter => {
                    self.save_model_api_key_input();
                    self.refresh_active_input_target();
                    true
                }
                KeyCode::Esc => {
                    self.model_input_mode = ModelInputMode::Browse;
                    self.refresh_active_input_target();
                    true
                }
                _ => true,
            },
            ActiveInputTarget::NewsFilter => match code {
                KeyCode::Char(c) => {
                    self.news_history_query.push(c);
                    true
                }
                KeyCode::Backspace => {
                    self.news_history_query.pop();
                    true
                }
                KeyCode::Enter | KeyCode::Esc => {
                    self.news_history_search_mode = false;
                    self.refresh_active_input_target();
                    true
                }
                _ => true,
            },
            ActiveInputTarget::Chat => false,
        }
    }

    pub(super) fn save_model_api_key_input(&mut self) {
        let provider = self.model_selector.provider.clone();
        let Some(auth_provider) = llm_provider_auth_provider(&provider) else {
            self.model_input_mode = ModelInputMode::Browse;
            return;
        };

        let value = self.model_api_key_input.trim().to_string();
        if value.is_empty() {
            self.auth_error = Some("API key cannot be empty".to_string());
            return;
        }

        match set_api_key(auth_provider, value) {
            Ok(store) => {
                apply_keys_to_env(&store);
                sync_auth_state(&store, &mut self.state.auth_state);
                self.auth_message = Some(format!(
                    "{} key saved from Model page",
                    auth_provider.display_name()
                ));
                self.model_api_key_input.clear();
                self.model_input_mode = ModelInputMode::Browse;
                self.rebuild_chat_engine();
            }
            Err(err) => {
                self.auth_error = Some(format!("Failed to save key: {}", err));
            }
        }
    }

    pub(super) fn delete_auth_for_selected(&mut self) {
        let provider = self.selected_auth_provider();

        match provider {
            AuthProvider::GitHub => {
                if let Err(err) = GitHubAuth::logout() {
                    self.auth_error = Some(format!("Failed to remove GitHub auth: {}", err));
                    return;
                }
                self.state
                    .auth_state
                    .insert(AuthProvider::GitHub, AuthStatus::NotConfigured);
                std::env::remove_var("GITHUB_TOKEN");
                self.auth_message = Some("GitHub auth removed".to_string());
                self.rebuild_chat_engine();
            }
            _ => match remove_stored_provider(provider) {
                Ok(store) => {
                    apply_keys_to_env(&store);
                    sync_auth_state(&store, &mut self.state.auth_state);
                    self.state
                        .auth_state
                        .entry(provider)
                        .and_modify(|status| {
                            if !status.is_configured() {
                                *status = AuthStatus::NotConfigured;
                            }
                        })
                        .or_insert(AuthStatus::NotConfigured);
                    self.auth_message = Some(format!("{} auth removed", provider.display_name()));
                    self.rebuild_chat_engine();
                }
                Err(err) => {
                    self.auth_error = Some(format!("Failed to delete auth: {}", err));
                }
            },
        }
    }

    pub(super) fn submit_input(&mut self) {
        let input = std::mem::take(&mut self.input);

        match parse_input(&input) {
            InputResult::Command(cmd) => self.execute_command(cmd),
            InputResult::ChatMessage(msg) => self.send_chat_message(msg),
            InputResult::Empty => {}
        }
    }

    pub(super) fn execute_command(&mut self, cmd: Command) {
        self.scroll = 0;
        self.chat_auto_scroll = true;

        match cmd {
            Command::Portfolio => self.page = Page::Portfolio,
            Command::Signals => self.page = Page::Signals,
            Command::Chart { pair, timeframe } => {
                if let Some(p) = pair {
                    self.state.chart_pair = p;
                }
                if let Some(tf) = timeframe {
                    if let Some(t) = Timeframe::from_chart_label(&tf) {
                        self.state.chart_timeframe = t;
                    }
                }
                self.page = Page::Chart;
            }
            Command::History { count } => {
                self.history_count = count;
                self.page = Page::History;
            }
            Command::Stats => self.page = Page::Stats,
            Command::Customize => {
                self.customize_snapshot = Some(self.state.config.clone());
                self.customize_selected = 0;
                self.customize_dirty = false;
                self.page = Page::Customize;
            }
            Command::Help => self.page = Page::Help,
            Command::Exit => {
                self.confirm = ConfirmState::Exit;
            }

            Command::Pause => {
                self.state.agent_status = AgentStatus::Paused;
            }
            Command::Resume => {
                self.state.agent_status = AgentStatus::Running;
            }
            Command::Buy { pair, size } => {
                self.confirm = ConfirmState::Buy { pair, size };
            }
            Command::Close { pair } => {
                self.confirm = ConfirmState::ClosePosition(pair);
            }
            Command::Add { pair } => {
                if !self.state.config.pairs.watchlist.contains(&pair) {
                    self.state.config.pairs.watchlist.push(pair.clone());
                    self.state.market_data.insert(
                        pair.clone(),
                        crate::state::MarketData::new(pair, self.state.config.data.cache_candles),
                    );
                }
            }
            Command::Remove { pair } => {
                self.state.config.pairs.watchlist.retain(|p| p != &pair);
                self.state.market_data.remove(&pair);
            }
            Command::Risk { percent } => {
                self.state.config.risk.max_position_pct = percent;
            }
            Command::Confidence { threshold } => {
                self.state.config.agent.min_confidence = threshold;
            }
            Command::Reset => {
                self.confirm = ConfirmState::Reset;
            }

            Command::Model => {
                self.sync_model_selector_auth();
                self.page = Page::Model;
            }
            Command::Auth { action } => match action.as_deref() {
                Some("status") | None => {
                    self.page = Page::Auth;
                    self.auth_input_mode = AuthInputMode::Select;
                }
                Some("logout") => {
                    self.auth_selected_index = 0;
                    self.delete_auth_for_selected();
                    self.page = Page::Auth;
                }
                Some(provider_name) => {
                    if let Some(provider) = auth_provider_from_action(provider_name) {
                        self.page = Page::Auth;
                        self.auth_input_mode = AuthInputMode::Select;
                        if let Some(index) = AuthProvider::ALL.iter().position(|p| *p == provider) {
                            self.auth_selected_index = index;
                            self.start_auth_for_selected();
                        }
                    } else {
                        self.error_message = Some(
                            "Usage: /auth [status|github|anthropic|openai|gemini|openrouter|gradio|logout]"
                                .to_string(),
                        );
                    }
                }
            },
            Command::AuthDelete { provider } => {
                if let Some(name) = provider {
                    if let Some(index) = AuthProvider::ALL
                        .iter()
                        .position(|p| p.display_name().to_lowercase().starts_with(&name))
                    {
                        self.auth_selected_index = index;
                    }
                }
                self.delete_auth_for_selected();
                self.page = Page::Auth;
            }
            Command::Team { prompt } => {
                self.page = Page::Team;
                self.start_team_discussion(prompt);
            }
            Command::TeamStatus => {
                self.page = Page::Team;
            }
            Command::TeamHistory => {
                self.page = Page::TeamHistory;
            }

            Command::Clear => {
                if self.page == Page::Chat {
                    self.state.chat_messages.clear();
                    self.chat_auto_scroll = true;
                }
                self.scroll = 0;
            }
            Command::Status => self.page = Page::Status,
            Command::Heatmap => self.page = Page::Heatmap,
            Command::News => self.page = Page::News,
            Command::Sentiment => self.page = Page::Sentiment,
            Command::Macro => self.page = Page::Macro,
            Command::Log => self.page = Page::Log,
            Command::Pairs => self.page = Page::Pairs,

            Command::Unknown { input } => {
                self.error_message = Some(input);
            }
        }

        self.refresh_active_input_target();
    }

    pub(super) fn start_github_auth(&mut self) {
        self.auth_message = Some("Starting GitHub device flow...".to_string());
        self.auth_input_mode = AuthInputMode::Select;
        self.page = Page::Auth;

        let tx = self.update_tx.clone();
        let auth = self.github_auth.clone();

        self.runtime_handle.spawn(async move {
            match auth.start_device_flow().await {
                Ok(mut rx) => {
                    while let Some(event) = rx.recv().await {
                        match event {
                            AuthEvent::DeviceCode {
                                user_code,
                                verification_uri,
                                expires_in,
                                interval_secs,
                            } => {
                                let _ = tx
                                    .send(StateUpdate::AuthStateChanged {
                                        provider: AuthProvider::GitHub,
                                        status: AuthStatus::PendingDevice {
                                            user_code,
                                            verification_uri,
                                            expires_at: Instant::now() + expires_in,
                                            interval_secs,
                                        },
                                    })
                                    .await;
                            }
                            AuthEvent::Polling { .. } => {
                                // Display handled by pending state timer.
                            }
                            AuthEvent::Success { username, token } => {
                                std::env::set_var("GITHUB_TOKEN", &token);
                                let _ = tx
                                    .send(StateUpdate::AuthStateChanged {
                                        provider: AuthProvider::GitHub,
                                        status: AuthStatus::AuthenticatedGitHub {
                                            username,
                                            token,
                                            created_at: Utc::now(),
                                        },
                                    })
                                    .await;
                            }
                            AuthEvent::Error(err) => {
                                warn!("GitHub auth flow error: {}", err);
                                let _ = tx
                                    .send(StateUpdate::AuthStateChanged {
                                        provider: AuthProvider::GitHub,
                                        status: AuthStatus::Error(err),
                                    })
                                    .await;
                            }
                            AuthEvent::Expired => {
                                warn!("GitHub auth flow expired before completion");
                                let _ = tx
                                    .send(StateUpdate::AuthStateChanged {
                                        provider: AuthProvider::GitHub,
                                        status: AuthStatus::Error(
                                            "Device flow expired. Retry /auth github".to_string(),
                                        ),
                                    })
                                    .await;
                            }
                        }
                    }
                }
                Err(err) => {
                    warn!("Failed to start GitHub device flow: {}", err);
                    let _ = tx
                        .send(StateUpdate::AuthStateChanged {
                            provider: AuthProvider::GitHub,
                            status: AuthStatus::Error(format!(
                                "Failed to start GitHub device flow: {}",
                                err
                            )),
                        })
                        .await;
                }
            }
        });
    }

    pub(super) fn do_reset(&mut self) {
        self.state.portfolio = crate::state::Portfolio::new(
            self.state.config.portfolio.virtual_balance,
            self.state.config.portfolio.currency.clone(),
        );
        self.state
            .apply_update(StateUpdate::Log(LogEntry::info("Portfolio reset")));
        self.team_popup = None;
    }

    pub(super) fn do_close_position(&mut self, pair: &str) {
        if let Some(pos) = self
            .state
            .portfolio
            .positions
            .iter()
            .find(|p| p.pair == pair)
        {
            let pos_id = pos.id;
            let current_price = pos.current_price;
            let close_result =
                self.state
                    .portfolio
                    .close_position(pos_id, current_price, CloseReason::Manual);
            if close_result.is_none() {
                self.error_message = Some(format!("No position found for {}", pair));
            }
        } else {
            self.error_message = Some(format!("No position found for {}", pair));
        }
    }

    pub(super) fn do_buy_position(&mut self, pair: &str, size: Decimal) {
        if size <= Decimal::ZERO {
            self.error_message = Some("Buy size must be greater than 0".to_string());
            return;
        }

        let Some(ticker) = self.state.get_ticker(pair) else {
            self.error_message = Some(format!("No ticker data available for {}", pair));
            return;
        };

        if ticker.price <= Decimal::ZERO {
            self.error_message = Some(format!("Invalid market price for {}", pair));
            return;
        }

        let stop_loss = ticker.price * (Decimal::ONE - Decimal::new(2, 2));
        let take_profit = ticker.price * (Decimal::ONE + Decimal::new(3, 2));
        let position = Position::new(
            pair.to_string(),
            PositionSide::Long,
            ticker.price,
            size,
            stop_loss,
            take_profit,
            70,
        );

        match self.state.portfolio.open_position(position) {
            Ok(_) => {
                self.state
                    .apply_update(StateUpdate::Log(LogEntry::trade(format!(
                        "Manual BUY executed: {} {}",
                        pair, size
                    ))));
                self.error_message = Some(format!("Bought {} {}", pair, size));
                self.page = Page::Portfolio;
            }
            Err(err) => {
                self.error_message = Some(format!("Failed to buy {}: {}", pair, err));
            }
        }
    }

    pub(super) fn start_team_discussion(&mut self, prompt: String) {
        if prompt.trim().is_empty() {
            self.error_message = Some("Usage: /team <prompt>".to_string());
            return;
        }

        let tx = self.update_tx.clone();
        let state_snapshot = self.state.clone();
        let config = self.state.config.llm.clone();

        self.runtime_handle.spawn(async move {
            if let Err(err) = run_team_discussion(state_snapshot, prompt, config, tx.clone()).await
            {
                let session_id = crate::chat::team::current_team_session_id();
                let _ = tx
                    .send(StateUpdate::TeamSessionError {
                        error: err.to_string(),
                        session_id,
                    })
                    .await;
            }
        });
    }

    pub(super) fn execute_team_action(&mut self) {
        let Some(card) = self.state.team_discussion.pending_action.clone() else {
            self.error_message = Some("No pending Team action card".to_string());
            return;
        };

        let current_session_ts = self
            .state
            .team_discussion
            .session_summary
            .as_ref()
            .map(|s| s.timestamp);

        match card.kind {
            TeamActionKind::Buy | TeamActionKind::Sell => {
                let pair = card
                    .pair
                    .clone()
                    .unwrap_or_else(|| self.state.chart_pair.clone());
                let Some(ticker) = self.state.get_ticker(&pair) else {
                    self.error_message = Some(format!("No ticker data available for {}", pair));
                    return;
                };

                if ticker.price <= Decimal::ZERO {
                    self.error_message = Some(format!("Invalid market price for {}", pair));
                    return;
                }

                let pct = card
                    .allocation_pct
                    .max(Decimal::new(1, 1))
                    .min(Decimal::from(100));
                let notional = (self.state.portfolio.total_value() * pct) / Decimal::from(100);
                let size = notional / ticker.price;

                let side = if card.kind == TeamActionKind::Buy {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                };
                let (stop_loss, take_profit) = if side == PositionSide::Long {
                    (
                        ticker.price * (Decimal::ONE - Decimal::new(2, 2)),
                        ticker.price * (Decimal::ONE + Decimal::new(3, 2)),
                    )
                } else {
                    (
                        ticker.price * (Decimal::ONE + Decimal::new(2, 2)),
                        ticker.price * (Decimal::ONE - Decimal::new(3, 2)),
                    )
                };

                let position = Position::new(
                    pair.clone(),
                    side,
                    ticker.price,
                    size,
                    stop_loss,
                    take_profit,
                    70,
                );

                match self.state.portfolio.open_position(position) {
                    Ok(_) => {
                        self.state
                            .apply_update(StateUpdate::Log(LogEntry::trade(format!(
                                "Team action executed: {}",
                                card.summary
                            ))));
                        self.error_message = Some(format!("Executed: {}", card.summary));
                        if let Some(timestamp) = current_session_ts {
                            self.state
                                .apply_update(StateUpdate::TeamHistoryDecisionUpdated {
                                    timestamp,
                                    decision: "Executed".to_string(),
                                });
                        }
                    }
                    Err(err) => {
                        self.error_message = Some(format!("Failed to execute action: {}", err));
                        return;
                    }
                }
            }
            TeamActionKind::Close => {
                let Some(pair) = card.pair.clone() else {
                    self.error_message = Some("Close action missing pair".to_string());
                    return;
                };
                if self
                    .state
                    .portfolio
                    .close_position_by_pair(&pair, CloseReason::Manual)
                    .is_none()
                {
                    self.error_message = Some(format!("No open position for {}", pair));
                    return;
                }
                self.state
                    .apply_update(StateUpdate::Log(LogEntry::trade(format!(
                        "Team action executed: {}",
                        card.summary
                    ))));
                self.error_message = Some(format!("Executed: {}", card.summary));
                if let Some(timestamp) = current_session_ts {
                    self.state
                        .apply_update(StateUpdate::TeamHistoryDecisionUpdated {
                            timestamp,
                            decision: "Executed".to_string(),
                        });
                }
            }
            TeamActionKind::Hold => {
                self.state
                    .apply_update(StateUpdate::Log(LogEntry::info(format!(
                        "Team action acknowledged: {}",
                        card.summary
                    ))));
                self.error_message = Some("Team action acknowledged (HOLD)".to_string());
                if let Some(timestamp) = current_session_ts {
                    self.state
                        .apply_update(StateUpdate::TeamHistoryDecisionUpdated {
                            timestamp,
                            decision: "Executed".to_string(),
                        });
                }
            }
        }

        self.state.apply_update(StateUpdate::TeamActionCleared);
        self.team_popup = None;
    }

    pub(super) fn dismiss_team_action(&mut self) {
        if self.state.team_discussion.pending_action.is_none() {
            self.error_message = Some("No pending Team action card".to_string());
            return;
        }

        if let Some(timestamp) = self
            .state
            .team_discussion
            .session_summary
            .as_ref()
            .map(|s| s.timestamp)
        {
            self.state
                .apply_update(StateUpdate::TeamHistoryDecisionUpdated {
                    timestamp,
                    decision: "Dismissed".to_string(),
                });
        }

        self.state.apply_update(StateUpdate::TeamActionCleared);
        self.state.apply_update(StateUpdate::Log(LogEntry::info(
            "Team action rejected by user",
        )));
        self.error_message = Some("Team action rejected".to_string());
        self.team_popup = None;
    }

    pub(super) fn request_team_reanalysis(&mut self) {
        let Some(prompt) = self.state.team_discussion.prompt.clone() else {
            self.error_message = Some("No active Team prompt to re-analyze".to_string());
            return;
        };

        if let Some(timestamp) = self
            .state
            .team_discussion
            .session_summary
            .as_ref()
            .map(|s| s.timestamp)
        {
            self.state
                .apply_update(StateUpdate::TeamHistoryDecisionUpdated {
                    timestamp,
                    decision: "Re-analyzed".to_string(),
                });
        }

        self.state.apply_update(StateUpdate::TeamActionCleared);
        self.team_popup = None;
        self.start_team_discussion(prompt);
    }

    pub(super) fn apply_edited_team_amount(&mut self) {
        let Some(popup) = self.team_popup.as_mut() else {
            return;
        };

        if popup.edit_buffer.is_empty() {
            popup.edit_buffer = "10".to_string();
        }

        let Ok(percent) = popup.edit_buffer.parse::<u8>() else {
            self.error_message = Some("Edit Amount expects 1-100".to_string());
            return;
        };

        if percent == 0 || percent > 100 {
            self.error_message = Some("Edit Amount expects 1-100".to_string());
            return;
        }

        if let Some(card) = self.state.team_discussion.pending_action.as_mut() {
            card.allocation_pct = Decimal::from(percent);
            card.summary = match card.kind {
                TeamActionKind::Buy => format!(
                    "BUY {} {}% portfolio",
                    card.pair.clone().unwrap_or_else(|| "BTCUSDT".to_string()),
                    percent
                ),
                TeamActionKind::Sell => format!(
                    "SELL {} {}% portfolio",
                    card.pair.clone().unwrap_or_else(|| "BTCUSDT".to_string()),
                    percent
                ),
                _ => card.summary.clone(),
            };
        }

        self.error_message = Some(format!("Updated action amount to {}%", percent));
    }

    pub(super) fn send_chat_message(&mut self, message: String) {
        let state_snapshot = self.state.clone();
        self.state.send_user_message(message.clone());
        self.page = Page::Chat;
        self.chat_auto_scroll = true;
        self.scroll = self.chat_max_scroll();

        let engine = Arc::clone(&self.chat_engine);
        let tx = self.update_tx.clone();

        self.runtime_handle.spawn(async move {
            match engine.process_message(&state_snapshot, &message).await {
                Ok(mut rx) => {
                    use crate::chat::engine::ChatEvent;

                    while let Some(event) = rx.recv().await {
                        let update = match event {
                            ChatEvent::Token(text) => StateUpdate::ChatToken(text),
                            ChatEvent::Complete(_) => StateUpdate::ChatDone,
                            ChatEvent::Error(e) => StateUpdate::ChatError(e),
                            ChatEvent::CommandExecuted(result) => {
                                info!("Command executed: {}", result.message);
                                continue;
                            }
                        };

                        if tx.send(update).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Chat engine error: {}", e);
                    let _ = tx.send(StateUpdate::ChatError(e.to_string())).await;
                }
            }
        });
    }
}
