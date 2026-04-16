//! Main TUI application with rich visual interface.

use std::borrow::Cow;
use std::collections::{hash_map::DefaultHasher, HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::io;
use std::process::Command as ProcessCommand;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame, Terminal,
};
use rust_decimal::Decimal;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio::time;
use tracing::{error, info, warn};

use crate::auth::{
    apply_keys_to_env, load_keys, remove_stored_provider, set_api_key, set_gradio, sync_auth_state,
    AuthEvent, AuthProvider, AuthStatus, GitHubAuth,
};
use crate::chat::engine::ChatEngine;
use crate::chat::llm::openrouter_models::fetch_openrouter_free_models;
use crate::chat::team::run_team_discussion;
use crate::config::{AgentStatus, LlmProvider};
use crate::data::sources::binance::fetch_binance_klines;
use crate::data::sources::cache::{
    load_chart_cache, load_news_cache, save_chart_cache, save_news_cache,
};
use crate::data::sources::news_refresh::fetch_news_headlines;
use crate::data::{spawn_market_feed_on, spawn_sources_aggregator_on};
use crate::engine::scheduler::spawn_signal_scheduler_on;
use crate::error::Result;
use crate::state::{
    AppState, CloseReason, ConnectionStatus, LogEntry, Position, PositionSide, StateUpdate,
    TeamActionKind, Timeframe,
};
use crate::tui::command::{parse_input, Command, InputResult};
use crate::tui::pages::{
    self, render_splash_with_offset, AuthInputModeView, AuthPageView, ModelPageView,
    NewsHistoryView, Page, TeamPopupOption,
};
use crate::tui::theme::{chars, Theme};
use crate::tui::widgets::{Autocomplete, ModelSelector};

#[path = "input.rs"]
mod input;
#[path = "shell.rs"]
mod shell;

const CUSTOMIZE_FIELD_COUNT: usize = 15;
const ACTIVITY_EVENT_CAP: usize = 20;
const RENDER_HASH_VERSION: u8 = 1;

/// Confirmation state for dangerous actions.
#[derive(Debug, Clone)]
enum ConfirmState {
    None,
    Reset,
    Exit,
    Buy { pair: String, size: Decimal },
    ClosePosition(String),
}

#[derive(Debug, Clone)]
enum AuthInputMode {
    Select,
    ApiKey {
        provider: AuthProvider,
        input: String,
    },
    GradioUrl {
        input: String,
    },
    GradioToken {
        space_url: String,
        input: String,
    },
}

#[derive(Debug, Clone)]
struct TeamActionPopupState {
    selected_index: usize,
    edit_buffer: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelInputMode {
    Browse,
    ApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ActiveInputTarget {
    Chat,
    AuthApiKey,
    AuthGradioUrl,
    AuthGradioToken,
    ModelSearch,
    NewsFilter,
}

#[derive(Debug, Clone, Default)]
struct PriceFlashState {
    last_price: Option<Decimal>,
    ticks_remaining: u8,
    is_up: bool,
}

#[derive(Debug, Clone, Copy)]
struct FrameRects {
    header: Rect,
    tabs: Rect,
    main: Rect,
    activity: Rect,
    input: Rect,
    footer: Rect,
}

/// Main TUI application.
pub struct App {
    /// Application state.
    pub state: AppState,
    /// Current theme.
    pub theme: Theme,
    /// Current page being displayed.
    pub page: Page,
    /// Input buffer.
    pub input: String,
    /// Whether the app should quit.
    pub should_quit: bool,
    /// Page scroll position.
    pub scroll: usize,
    /// History count for /history command.
    pub history_count: usize,
    /// Error message to display (clears on next input).
    pub error_message: Option<String>,
    /// Confirmation state.
    confirm: ConfirmState,
    /// Show startup logo.
    show_logo: bool,
    /// Logo display start time.
    logo_start: Instant,
    /// Chat engine for LLM interactions.
    chat_engine: Arc<ChatEngine>,
    /// Tokio runtime handle for async operations.
    runtime_handle: Handle,
    /// Receiver for state updates from background tasks.
    update_rx: mpsc::Receiver<StateUpdate>,
    /// Sender for state updates (cloned to background tasks).
    update_tx: mpsc::Sender<StateUpdate>,
    /// Watch sender for immutable app snapshots consumed by engine scheduler.
    engine_snapshot_tx: tokio::sync::watch::Sender<AppState>,
    /// Autocomplete widget state.
    autocomplete: Autocomplete,
    /// Model selector state.
    model_selector: ModelSelector,
    /// Spinner animation frame.
    spinner_frame: usize,
    /// GitHub auth handler.
    github_auth: GitHubAuth,
    /// Selected auth provider index.
    auth_selected_index: usize,
    /// Auth page input mode.
    auth_input_mode: AuthInputMode,
    /// Auth page message.
    auth_message: Option<String>,
    /// Auth page error.
    auth_error: Option<String>,
    /// Inline API key input mode on model page.
    model_input_mode: ModelInputMode,
    /// Inline API key input buffer on model page.
    model_api_key_input: String,
    /// Current focused keyboard/paste target.
    active_input_target: ActiveInputTarget,
    /// Chat view auto-follows newest message when true.
    chat_auto_scroll: bool,
    /// Selected field index in /customize.
    customize_selected: usize,
    /// Whether customize values were changed but not saved.
    customize_dirty: bool,
    /// Snapshot used for discard in /customize.
    customize_snapshot: Option<crate::config::Config>,
    /// Pending team action popup state.
    team_popup: Option<TeamActionPopupState>,
    /// Generic keybind popup visibility.
    show_keybind_popup: bool,
    /// Pulse state for websocket indicator animation.
    ws_pulse_on: bool,
    /// Last price flash state by pair (BTCUSDT/ETHUSDT).
    price_flash: HashMap<String, PriceFlashState>,
    /// Rolling activity messages for ticker strip.
    activity_events: VecDeque<String>,
    /// Memoized joined activity strip payload.
    activity_joined: String,
    /// Memoized sorted source names for status/news rendering.
    source_names_sorted: Vec<String>,
    /// Memoized source status preview used by news footer.
    source_status_preview: String,
    /// Character offset for activity strip scrolling.
    activity_offset: usize,
    /// Last frame hash used for render diff guard.
    last_render_hash: Option<u64>,
    /// Prevents re-entrant render calls.
    render_guard: bool,
    /// Splash logo vertical offset.
    logo_offset: i16,
    /// News history inline filter/search mode.
    news_history_search_mode: bool,
    /// News history filter query.
    news_history_query: String,
    /// Background tasks started by App::new.
    background_tasks: Vec<tokio::task::JoinHandle<()>>,
    /// Broadcast sender used to request worker shutdown.
    shutdown_tx: watch::Sender<bool>,
    /// Whether background task shutdown already started.
    shutdown_started: bool,
    /// Last rendered frame size for scroll calculations.
    last_frame_size: Rect,
}

impl App {
    /// Create a new App with the given state and runtime handle.
    pub fn new(mut state: AppState, runtime_handle: Handle) -> Self {
        let theme = Theme::from_name(&state.config.tui.theme);

        if let Ok(Some(cache)) = load_news_cache() {
            state.apply_update(StateUpdate::NewsHistoryLoaded {
                headlines: cache.headlines,
                last_fetch_at: cache.last_fetch_at,
            });
        }

        if let Ok(Some(cache)) = load_chart_cache() {
            state.apply_update(StateUpdate::ChartCacheLoaded(cache));
        }

        if let Ok(keys) = load_keys() {
            apply_keys_to_env_preserving_existing(&keys);
            sync_auth_state(&keys, &mut state.auth_state);
        }

        if let Ok(Some(stored)) = GitHubAuth::load_stored() {
            std::env::set_var("GITHUB_TOKEN", &stored.access_token);
            state.auth_state.insert(
                AuthProvider::GitHub,
                AuthStatus::AuthenticatedGitHub {
                    username: stored.username,
                    token: stored.access_token,
                    created_at: stored.created_at,
                },
            );
        }

        let chat_engine = Arc::new(ChatEngine::new(&state.config.llm));
        info!("Chat engine using {} provider", chat_engine.provider_name());

        let (update_tx, update_rx) = mpsc::channel(256);
        let (engine_snapshot_tx, engine_snapshot_rx) = tokio::sync::watch::channel(state.clone());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut background_tasks = Vec::new();

        if !cfg!(test) {
            background_tasks.push(spawn_market_feed_on(
                &runtime_handle,
                &state.config.data,
                state.config.pairs.watchlist.clone(),
                update_tx.clone(),
                shutdown_rx.clone(),
            ));
            background_tasks.push(spawn_sources_aggregator_on(
                &runtime_handle,
                state.config.data.clone(),
                update_tx.clone(),
                shutdown_rx.clone(),
            ));
            background_tasks.push(spawn_signal_scheduler_on(
                &runtime_handle,
                engine_snapshot_rx,
                update_tx.clone(),
                shutdown_rx.clone(),
            ));

            let news_refresh_tx = update_tx.clone();
            let news_config = state.config.data.clone();
            let mut news_shutdown_rx = shutdown_rx.clone();
            background_tasks.push(runtime_handle.spawn(async move {
                let client = match reqwest::Client::builder()
                    .user_agent("mycrypto/0.1")
                    .timeout(Duration::from_secs(20))
                    .build()
                {
                    Ok(c) => c,
                    Err(err) => {
                        warn!("news refresh client init failed: {}", err);
                        return;
                    }
                };

                let mut interval = time::interval(Duration::from_secs(180));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {}
                        changed = news_shutdown_rx.changed() => {
                            if changed.is_err() || *news_shutdown_rx.borrow() {
                                break;
                            }
                            continue;
                        }
                    }

                    if news_refresh_tx
                        .send(StateUpdate::NewsRefreshStarted)
                        .await
                        .is_err()
                    {
                        break;
                    }

                    let headlines = fetch_news_headlines(&client, &news_config).await;
                    if !headlines.is_empty() {
                        let fetched_at = Utc::now();
                        if news_refresh_tx
                            .send(StateUpdate::NewsUpdate(headlines.clone()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                        if news_refresh_tx
                            .send(StateUpdate::NewsRefreshCompleted { fetched_at })
                            .await
                            .is_err()
                        {
                            break;
                        }

                        if let Err(err) = save_news_cache(headlines, Some(fetched_at)) {
                            warn!("failed to persist news cache in refresh worker: {}", err);
                        }
                    }
                }
            }));

            let chart_refresh_tx = update_tx.clone();
            let chart_pairs = state.config.pairs.watchlist.clone();
            let mut chart_shutdown_rx = shutdown_rx.clone();
            background_tasks.push(runtime_handle.spawn(async move {
                let client = match reqwest::Client::builder()
                    .user_agent("mycrypto/0.1")
                    .timeout(Duration::from_secs(20))
                    .build()
                {
                    Ok(c) => c,
                    Err(err) => {
                        warn!("chart refresh client init failed: {}", err);
                        return;
                    }
                };

                let timeframes = [
                    Timeframe::H1,
                    Timeframe::H4,
                    Timeframe::D1,
                    Timeframe::W1,
                    Timeframe::MO1,
                ];
                let mut last_polled: HashMap<String, Instant> = HashMap::new();
                let mut ticker = time::interval(Duration::from_secs(60));

                loop {
                    tokio::select! {
                        _ = ticker.tick() => {}
                        changed = chart_shutdown_rx.changed() => {
                            if changed.is_err() || *chart_shutdown_rx.borrow() {
                                break;
                            }
                            continue;
                        }
                    }

                    for pair in &chart_pairs {
                        for timeframe in timeframes {
                            let cache_key = format!("{}|{}", pair, timeframe.as_binance_interval());
                            let cadence = chart_refresh_cadence(timeframe);
                            let due = last_polled
                                .get(&cache_key)
                                .map(|t| t.elapsed() >= cadence)
                                .unwrap_or(true);

                            if !due {
                                continue;
                            }

                            match fetch_binance_klines(&client, pair, timeframe, 200).await {
                                Ok(candles) if !candles.is_empty() => {
                                    let fetched_at = Utc::now();
                                    if chart_refresh_tx
                                        .send(StateUpdate::ChartSeriesUpdate {
                                            pair: pair.clone(),
                                            timeframe,
                                            candles,
                                            fetched_at,
                                        })
                                        .await
                                        .is_err()
                                    {
                                        return;
                                    }
                                    last_polled.insert(cache_key, Instant::now());
                                }
                                Ok(_) => {}
                                Err(err) => {
                                    warn!(
                                        "chart refresh failed pair={} tf={}: {}",
                                        pair, timeframe, err
                                    );
                                }
                            }
                        }
                    }
                }
            }));

            let free_models_tx = update_tx.clone();
            let mut free_models_shutdown_rx = shutdown_rx;
            background_tasks.push(runtime_handle.spawn(async move {
                let mut ticker = time::interval(Duration::from_secs(30 * 60));

                loop {
                    tokio::select! {
                        _ = ticker.tick() => {}
                        changed = free_models_shutdown_rx.changed() => {
                            if changed.is_err() || *free_models_shutdown_rx.borrow() {
                                break;
                            }
                            continue;
                        }
                    }

                    if let Ok(models) = refresh_openrouter_free_models_update().await {
                        if free_models_tx
                            .send(StateUpdate::OpenRouterFreeModelsUpdated(models))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }));
        }

        let api_key_set = provider_authenticated(&state, &state.config.llm.provider);

        let mut model_selector = ModelSelector::from_config(
            state.config.llm.provider.clone(),
            &state.config.llm.model,
            api_key_set,
        );
        let selector_provider = model_selector.provider.clone();
        let provider_auth = provider_authenticated(&state, &selector_provider);
        model_selector.set_provider_authenticated(provider_auth);

        let selector_models = model_provider_models_for_state(
            &state,
            &model_selector.provider,
            &model_selector.model,
        );
        if let Some(index) = selector_models
            .iter()
            .position(|model| model == &model_selector.model)
        {
            model_selector.model_index = index;
        } else if let Some(first) = selector_models.first() {
            model_selector.model = first.clone();
            model_selector.model_index = 0;
        }

        let mut app = Self {
            state,
            theme,
            page: Page::Portfolio,
            input: String::new(),
            should_quit: false,
            scroll: 0,
            history_count: 20,
            error_message: None,
            confirm: ConfirmState::None,
            show_logo: true,
            logo_start: Instant::now(),
            chat_engine,
            runtime_handle,
            update_rx,
            update_tx,
            engine_snapshot_tx,
            autocomplete: Autocomplete::new(),
            model_selector,
            spinner_frame: 0,
            github_auth: GitHubAuth::new(),
            auth_selected_index: 0,
            auth_input_mode: AuthInputMode::Select,
            auth_message: None,
            auth_error: None,
            model_input_mode: ModelInputMode::Browse,
            model_api_key_input: String::new(),
            active_input_target: ActiveInputTarget::Chat,
            chat_auto_scroll: true,
            customize_selected: 0,
            customize_dirty: false,
            customize_snapshot: None,
            team_popup: None,
            show_keybind_popup: false,
            ws_pulse_on: true,
            price_flash: HashMap::new(),
            activity_events: VecDeque::new(),
            activity_joined: String::new(),
            source_names_sorted: Vec::new(),
            source_status_preview: String::new(),
            activity_offset: 0,
            last_render_hash: None,
            render_guard: false,
            logo_offset: 0,
            news_history_search_mode: false,
            news_history_query: String::new(),
            background_tasks,
            shutdown_tx,
            shutdown_started: false,
            last_frame_size: Rect::new(0, 0, 120, 40),
        };

        app.refresh_active_input_target();
        app.refresh_source_health_cache();
        app
    }

    /// Run the TUI main loop.
    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.run_loop(&mut terminal);
        self.shutdown_background_tasks();

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        let (tick_tx, mut tick_rx) = mpsc::channel::<()>(256);
        let tick_task = self.runtime_handle.spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                match tick_tx.try_send(()) {
                    Ok(()) => {}
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        warn!("tick channel full; dropping UI tick");
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => break,
                }
            }
        });

        let loop_result = (|| -> Result<()> {
            loop {
                self.process_updates();

                let mut ticked = false;
                while tick_rx.try_recv().is_ok() {
                    self.tick();
                    ticked = true;
                }

                if event::poll(Duration::from_millis(16))? {
                    match event::read()? {
                        Event::Key(key) if key.kind == KeyEventKind::Press => {
                            self.handle_key(key.code, key.modifiers);
                        }
                        Event::Paste(text) => self.handle_paste_text(&text),
                        _ => {}
                    }

                    while event::poll(Duration::from_millis(0))? {
                        match event::read()? {
                            Event::Key(key) if key.kind == KeyEventKind::Press => {
                                self.handle_key(key.code, key.modifiers);
                            }
                            Event::Paste(text) => self.handle_paste_text(&text),
                            _ => {}
                        }
                    }
                }

                if self.should_quit {
                    return Ok(());
                }

                if !ticked {
                    continue;
                }

                let size = terminal.size()?;
                let frame_rect = Rect::new(0, 0, size.width, size.height);
                self.last_frame_size = frame_rect;
                let frame_hash = self.render_state_hash(frame_rect);
                if self.last_render_hash == Some(frame_hash) {
                    continue;
                }
                if self.render_guard {
                    continue;
                }

                self.render_guard = true;
                let draw_result = terminal.draw(|f| self.render(f));
                self.render_guard = false;
                draw_result?;
                self.last_render_hash = Some(frame_hash);
            }
        })();

        tick_task.abort();
        loop_result
    }

    fn shutdown_background_tasks(&mut self) {
        if self.shutdown_started {
            return;
        }
        self.shutdown_started = true;

        let _ = self.shutdown_tx.send(true);
        for task in &self.background_tasks {
            task.abort();
        }
        self.background_tasks.clear();
    }

    fn tick(&mut self) {
        if self.show_logo {
            let elapsed = self.logo_start.elapsed();
            let progress = (elapsed.as_millis() as i64).saturating_sub(1800);
            self.logo_offset = if progress > 0 {
                -(progress / 120) as i16
            } else {
                0
            };
            if elapsed >= Duration::from_millis(2500) {
                self.show_logo = false;
                self.logo_offset = 0;
            }
        }

        self.spinner_frame = (self.spinner_frame + 1) % chars::SPINNER.len();
        self.ws_pulse_on = !self.ws_pulse_on;
        if !self.activity_events.is_empty() && !self.state.team_discussion.active {
            self.activity_offset = self.activity_offset.saturating_add(1);
        }

        self.refresh_price_flash();

        for state in self.price_flash.values_mut() {
            if state.ticks_remaining > 0 {
                state.ticks_remaining -= 1;
            }
        }
    }

    fn process_updates(&mut self) {
        let mut needs_chat_engine_refresh = false;
        let mut persist_news_cache = false;
        let mut persist_chart_cache = false;
        let mut source_health_changed = false;
        let mut chat_stream_changed = false;
        while let Ok(update) = self.update_rx.try_recv() {
            if let StateUpdate::AuthStateChanged { provider, status } = &update {
                if *provider == AuthProvider::GitHub {
                    match status {
                        AuthStatus::PendingDevice {
                            user_code,
                            verification_uri,
                            interval_secs,
                            ..
                        } => {
                            self.auth_message = Some(format!(
                                "GitHub device code {} ready at {} (poll {}s)",
                                user_code, verification_uri, interval_secs
                            ));
                            self.auth_error = None;
                            self.page = Page::Auth;
                            self.auth_input_mode = AuthInputMode::Select;
                        }
                        AuthStatus::AuthenticatedGitHub { username, .. } => {
                            self.auth_message =
                                Some(format!("GitHub authentication complete for {}", username));
                            self.auth_error = None;
                            if self.page == Page::Auth {
                                self.page = Page::Model;
                            }
                        }
                        AuthStatus::Error(message) => {
                            self.auth_error = Some(message.clone());
                            warn!("GitHub authentication state error: {}", message);
                        }
                        _ => {}
                    }
                }
            }

            if matches!(
                &update,
                StateUpdate::AuthStateChanged { .. } | StateUpdate::ConfigChanged(_)
            ) {
                needs_chat_engine_refresh = true;
            }

            if matches!(
                &update,
                StateUpdate::NewsUpdate(_)
                    | StateUpdate::NewsHistoryLoaded { .. }
                    | StateUpdate::NewsRefreshCompleted { .. }
            ) {
                persist_news_cache = true;
            }

            if matches!(
                &update,
                StateUpdate::ChartSeriesUpdate { .. } | StateUpdate::ChartCacheLoaded(_)
            ) {
                persist_chart_cache = true;
            }

            if matches!(
                &update,
                StateUpdate::ChatToken(_) | StateUpdate::ChatDone | StateUpdate::ChatError(_)
            ) {
                chat_stream_changed = true;
            }

            if matches!(&update, StateUpdate::SourceHealthChanged(_)) {
                source_health_changed = true;
            }

            self.capture_activity_event(&update);
            self.state.apply_update(update);
        }

        if source_health_changed {
            self.refresh_source_health_cache();
        }

        if persist_news_cache {
            if let Err(err) = save_news_cache(
                self.state.news_history.iter().cloned().collect(),
                self.state.news_last_fetch_at,
            ) {
                warn!("failed to persist news cache: {}", err);
            }
        }

        if persist_chart_cache {
            if let Err(err) = save_chart_cache(
                self.state.chart_cache.clone(),
                self.state.chart_last_fetch_at.clone(),
                self.state.chart_cache_lru.iter().cloned().collect(),
            ) {
                warn!("failed to persist chart cache: {}", err);
            }
        }

        let _ = self.engine_snapshot_tx.send(self.state.clone());

        if self.state.team_discussion.pending_action.is_some() && self.team_popup.is_none() {
            self.team_popup = Some(TeamActionPopupState {
                selected_index: 0,
                edit_buffer: String::new(),
            });
        }
        if self.state.team_discussion.pending_action.is_none() {
            self.team_popup = None;
        }

        if needs_chat_engine_refresh {
            self.rebuild_chat_engine();
        }

        if self.page == Page::Chat && chat_stream_changed && self.chat_auto_scroll {
            self.scroll = self.chat_max_scroll();
        }

        self.sync_model_selector_auth();
        self.sync_model_selector_model();
        self.refresh_active_input_target();
    }

    fn capture_activity_event(&mut self, update: &StateUpdate) {
        let msg = match update {
            StateUpdate::NewSignal(signal) => Some(format!(
                "signal {} {} {}%",
                signal.pair, signal.direction, signal.confidence
            )),
            StateUpdate::TeamSessionStarted { prompt, .. } => {
                Some(format!("team started: {}", prompt))
            }
            StateUpdate::TeamSummary { summary, .. } => {
                Some(format!("team verdict: {}", summary.leader_verdict))
            }
            StateUpdate::PositionOpened(pos) => {
                Some(format!("trade open {} {}", pos.side, pos.pair))
            }
            StateUpdate::PositionClosed { position_id, .. } => {
                Some(format!("trade closed {}", position_id))
            }
            StateUpdate::SourceHealthChanged(source) => {
                Some(format!("source {} {}", source.name, source.level))
            }
            StateUpdate::EngineStatusUpdated(status) => {
                Some(format!("engine tick errors:{}", status.consecutive_errors))
            }
            _ => None,
        };

        if let Some(event) = msg {
            self.activity_events.push_back(event);
            while self.activity_events.len() > ACTIVITY_EVENT_CAP {
                self.activity_events.pop_front();
            }
            self.refresh_activity_joined();
        }
    }

    fn refresh_activity_joined(&mut self) {
        self.activity_joined = self
            .activity_events
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("   ✦   ");
    }

    fn refresh_source_health_cache(&mut self) {
        let mut names: Vec<String> = self.state.source_health.keys().cloned().collect();
        names.sort();

        self.source_status_preview = if names.is_empty() {
            "source status not available yet".to_string()
        } else {
            names
                .iter()
                .take(4)
                .map(|name| {
                    self.state
                        .source_health
                        .get(name)
                        .map(|source| format!("{}:{}", source.name, source.level))
                        .unwrap_or_else(|| name.clone())
                })
                .collect::<Vec<_>>()
                .join("  ·  ")
        };

        self.source_names_sorted = names;
    }

    fn refresh_price_flash(&mut self) {
        for pair in ["BTCUSDT", "ETHUSDT"] {
            let Some(ticker) = self.state.get_ticker(pair) else {
                continue;
            };
            let entry = self.price_flash.entry(pair.to_string()).or_default();
            if let Some(last) = entry.last_price {
                if ticker.price != last {
                    entry.ticks_remaining = 2;
                    entry.is_up = ticker.price > last;
                }
            }
            entry.last_price = Some(ticker.price);
        }
    }

    fn adjust_customize_value(&mut self, delta: i32) -> bool {
        match self.customize_selected {
            0 => {
                let v = self.state.config.agent.min_confidence as i32 + delta;
                self.state.config.agent.min_confidence = v.clamp(0, 100) as u8;
                true
            }
            1 => {
                let v = self.state.config.agent.max_open_trades as i32 + delta;
                self.state.config.agent.max_open_trades = v.clamp(1, 20) as u8;
                true
            }
            2 => {
                let v = self.state.config.agent.scan_interval_sec as i32 + (delta * 30);
                self.state.config.agent.scan_interval_sec = v.clamp(60, 3600) as u64;
                true
            }
            3 => {
                let v = self.state.config.risk.risk_per_trade_pct + Decimal::new(delta as i64, 1);
                self.state.config.risk.risk_per_trade_pct =
                    v.max(Decimal::new(1, 1)).min(Decimal::new(1000, 1));
                true
            }
            4 => {
                let v =
                    self.state.config.risk.max_daily_drawdown_pct + Decimal::new(delta as i64, 1);
                self.state.config.risk.max_daily_drawdown_pct =
                    v.max(Decimal::new(1, 1)).min(Decimal::new(1000, 1));
                true
            }
            13 => {
                let options = ["1h", "4h", "1d", "1w", "1M"];
                let current = options
                    .iter()
                    .position(|v| *v == self.state.config.tui.chart_default_timeframe)
                    .unwrap_or(1);
                let mut next = current as i32 + delta;
                if next < 0 {
                    next = options.len() as i32 - 1;
                }
                if next >= options.len() as i32 {
                    next = 0;
                }
                let selected = options[next as usize].to_string();
                self.state.config.tui.chart_default_timeframe = selected.clone();
                self.state.chart_timeframe =
                    Timeframe::from_chart_label(&selected).unwrap_or(self.state.chart_timeframe);
                true
            }
            14 => {
                let v = self.state.config.tui.log_lines as i32 + (delta * 10);
                self.state.config.tui.log_lines = v.clamp(50, 2000) as usize;
                true
            }
            _ => false,
        }
    }

    fn toggle_customize_source(&mut self) -> bool {
        match self.customize_selected {
            5 => {
                self.state.config.data.yahoo_enabled = !self.state.config.data.yahoo_enabled;
                true
            }
            6 => {
                self.state.config.data.coingecko_enabled =
                    !self.state.config.data.coingecko_enabled;
                true
            }
            7 => {
                self.state.config.data.fear_greed_enabled =
                    !self.state.config.data.fear_greed_enabled;
                true
            }
            8 => {
                self.state.config.data.reddit_enabled = !self.state.config.data.reddit_enabled;
                true
            }
            9 => {
                self.state.config.data.twitter_enabled = !self.state.config.data.twitter_enabled;
                true
            }
            10 => {
                self.state.config.data.reuters_rss_enabled =
                    !self.state.config.data.reuters_rss_enabled;
                true
            }
            11 => {
                self.state.config.data.bloomberg_rss_enabled =
                    !self.state.config.data.bloomberg_rss_enabled;
                true
            }
            12 => {
                self.state.config.data.finnhub_enabled = !self.state.config.data.finnhub_enabled;
                true
            }
            _ => false,
        }
    }

    fn save_customize_snapshot(&mut self) {
        let config_path = std::env::var("MYCRYPTO_CONFIG")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "config.toml".to_string());

        match toml::to_string_pretty(&self.state.config) {
            Ok(contents) => {
                if let Err(err) = std::fs::write(&config_path, contents) {
                    self.error_message = Some(format!("Failed to write {}: {}", config_path, err));
                    return;
                }
                self.customize_snapshot = Some(self.state.config.clone());
            }
            Err(err) => {
                self.error_message = Some(format!("Failed to serialize config: {}", err));
            }
        }
    }

    fn restore_customize_snapshot(&mut self) {
        if let Some(snapshot) = self.customize_snapshot.clone() {
            self.state.config = snapshot;
            self.state.chart_timeframe =
                Timeframe::from_chart_label(&self.state.config.tui.chart_default_timeframe)
                    .unwrap_or(self.state.chart_timeframe);
        }
    }

    fn sync_model_selector_auth(&mut self) {
        let authenticated = provider_authenticated(&self.state, &self.model_selector.provider);
        self.model_selector
            .set_provider_authenticated(authenticated);
    }

    fn sync_model_selector_model(&mut self) {
        let options = model_provider_models_for_state(
            &self.state,
            &self.model_selector.provider,
            &self.model_selector.model,
        );

        let fallback = options
            .first()
            .cloned()
            .unwrap_or_else(|| self.model_selector.model.clone());

        if !options
            .iter()
            .any(|item| item == &self.model_selector.model)
        {
            self.model_selector.model = fallback;
        }

        self.model_selector.model_index = options
            .iter()
            .position(|item| item == &self.model_selector.model)
            .unwrap_or(0);
    }

    fn cycle_model_for_current_provider(&mut self) {
        let options = model_provider_models_for_state(
            &self.state,
            &self.model_selector.provider,
            &self.model_selector.model,
        );

        if options.is_empty() {
            return;
        }

        let current = options
            .iter()
            .position(|item| item == &self.model_selector.model)
            .unwrap_or(0);
        let next = (current + 1) % options.len();

        self.model_selector.model = options[next].clone();
        self.model_selector.model_index = next;
    }

    fn rebuild_chat_engine(&mut self) {
        self.chat_engine = Arc::new(ChatEngine::new(&self.state.config.llm));
        info!(
            "Chat engine switched to {} provider",
            self.chat_engine.provider_name()
        );
    }

    fn refresh_active_input_target(&mut self) {
        self.active_input_target = if self.page == Page::Auth {
            match self.auth_input_mode {
                AuthInputMode::ApiKey { .. } => ActiveInputTarget::AuthApiKey,
                AuthInputMode::GradioUrl { .. } => ActiveInputTarget::AuthGradioUrl,
                AuthInputMode::GradioToken { .. } => ActiveInputTarget::AuthGradioToken,
                AuthInputMode::Select => ActiveInputTarget::Chat,
            }
        } else if self.page == Page::Model && self.model_input_mode == ModelInputMode::ApiKey {
            ActiveInputTarget::ModelSearch
        } else if self.page == Page::NewsHistory && self.news_history_search_mode {
            ActiveInputTarget::NewsFilter
        } else {
            ActiveInputTarget::Chat
        };
    }

    fn chat_max_scroll(&self) -> usize {
        let rects = Self::compute_frame_rects(self.last_frame_size);
        let content_width = rects.main.width.saturating_sub(2) as usize;
        let viewport_height = rects.main.height.saturating_sub(2) as usize;
        let total_lines = pages::chat_line_count(
            &self.state,
            &self.theme,
            self.spinner_frame,
            content_width,
            self.chat_auto_scroll,
        );
        total_lines.saturating_sub(viewport_height.max(1))
    }

    fn perform_exit_shutdown(&mut self) {
        self.should_quit = true;
        self.confirm = ConfirmState::None;
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.shutdown_background_tasks();
    }
}

fn provider_authenticated(state: &AppState, provider: &LlmProvider) -> bool {
    let configured_in_state = match provider {
        LlmProvider::Claude => state
            .auth_state
            .get(&AuthProvider::Anthropic)
            .map(|s| s.is_configured())
            .unwrap_or(false),
        LlmProvider::OpenAI => state
            .auth_state
            .get(&AuthProvider::OpenAI)
            .map(|s| s.is_configured())
            .unwrap_or(false),
        LlmProvider::Gemini => state
            .auth_state
            .get(&AuthProvider::Gemini)
            .map(|s| s.is_configured())
            .unwrap_or(false),
        LlmProvider::OpenRouter => state
            .auth_state
            .get(&AuthProvider::OpenRouter)
            .map(|s| s.is_configured())
            .unwrap_or(false),
        LlmProvider::Gradio => state
            .auth_state
            .get(&AuthProvider::Gradio)
            .map(|s| s.is_configured())
            .unwrap_or(false),
        LlmProvider::Copilot => state
            .auth_state
            .get(&AuthProvider::GitHub)
            .map(|s| s.is_configured())
            .unwrap_or(false),
        LlmProvider::Mock => true,
    };

    configured_in_state || provider_has_env_credentials(provider)
}

fn provider_has_env_credentials(provider: &LlmProvider) -> bool {
    match provider {
        LlmProvider::Claude => env_var_has_value("CLAUDE_API_KEY"),
        LlmProvider::OpenAI => env_var_has_value("OPENAI_API_KEY"),
        LlmProvider::Gemini => {
            env_var_has_value("GEMINI_API_KEY") || env_var_has_value("GOOGLE_API_KEY")
        }
        LlmProvider::OpenRouter => env_var_has_value("OPENROUTER_API_KEY"),
        LlmProvider::Gradio => {
            env_var_has_value("GRADIO_SPACE_URL") || env_var_has_value("GRADIO_API_KEY")
        }
        LlmProvider::Copilot => env_var_has_value("GITHUB_TOKEN"),
        LlmProvider::Mock => true,
    }
}

fn env_var_has_value(name: &str) -> bool {
    std::env::var(name)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn apply_keys_to_env_preserving_existing(store: &crate::auth::apikey::ApiKeyStore) {
    apply_key_if_missing("GITHUB_TOKEN", store.github_token.as_deref());
    apply_key_if_missing("CLAUDE_API_KEY", store.anthropic.as_deref());
    apply_key_if_missing("OPENAI_API_KEY", store.openai.as_deref());
    apply_key_if_missing("GEMINI_API_KEY", store.gemini.as_deref());
    apply_key_if_missing("OPENROUTER_API_KEY", store.openrouter.as_deref());
    apply_key_if_missing("GRADIO_API_KEY", store.gradio_token.as_deref());
    apply_key_if_missing("GRADIO_SPACE_URL", store.gradio_space_url.as_deref());
}

fn apply_key_if_missing(name: &str, value: Option<&str>) {
    if env_var_has_value(name) {
        return;
    }

    if let Some(v) = value {
        if !v.trim().is_empty() {
            std::env::set_var(name, v);
        }
    }
}

fn read_clipboard_text() -> Option<String> {
    let candidates: [(&str, &[&str]); 3] = [
        ("wl-paste", &["-n"]),
        ("xclip", &["-selection", "clipboard", "-o"]),
        ("pbpaste", &[]),
    ];

    for (bin, args) in candidates {
        let output = ProcessCommand::new(bin).args(args).output();
        let Ok(output) = output else {
            continue;
        };
        if !output.status.success() {
            continue;
        }

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

fn auth_provider_from_action(action: &str) -> Option<AuthProvider> {
    match action.to_lowercase().as_str() {
        "github" | "copilot" => Some(AuthProvider::GitHub),
        "anthropic" | "claude" => Some(AuthProvider::Anthropic),
        "openai" => Some(AuthProvider::OpenAI),
        "gemini" | "google" => Some(AuthProvider::Gemini),
        "openrouter" | "or" => Some(AuthProvider::OpenRouter),
        "gradio" => Some(AuthProvider::Gradio),
        _ => None,
    }
}

fn llm_provider_auth_provider(provider: &LlmProvider) -> Option<AuthProvider> {
    match provider {
        LlmProvider::Claude => Some(AuthProvider::Anthropic),
        LlmProvider::OpenAI => Some(AuthProvider::OpenAI),
        LlmProvider::Gemini => Some(AuthProvider::Gemini),
        LlmProvider::OpenRouter => Some(AuthProvider::OpenRouter),
        LlmProvider::Gradio => Some(AuthProvider::Gradio),
        LlmProvider::Copilot => Some(AuthProvider::GitHub),
        LlmProvider::Mock => None,
    }
}

fn model_provider_models_for_state(
    state: &AppState,
    provider: &LlmProvider,
    current_model: &str,
) -> Vec<String> {
    let defaults: &[&str] = match provider {
        LlmProvider::Claude => &["claude-opus-4-5", "claude-sonnet-4-5", "claude-haiku-4-5"],
        LlmProvider::OpenAI => &["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "o1", "o1-mini"],
        LlmProvider::Gemini => &["gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"],
        LlmProvider::OpenRouter => &[
            "anthropic/claude-sonnet-4-5",
            "openai/gpt-4o",
            "google/gemini-2.0-flash-001",
        ],
        LlmProvider::Gradio => &[
            "https://huggingface.co/spaces/HuggingFaceH4/zephyr-chat",
            "https://huggingface.co/spaces/Qwen/Qwen2.5-Max-Demo",
            "https://huggingface.co/spaces/microsoft/Phi-4-multimodal-instruct",
        ],
        LlmProvider::Copilot => &["gpt-4o"],
        LlmProvider::Mock => &["mock-v1"],
    };

    let mut models: Vec<String> = defaults.iter().map(|value| (*value).to_string()).collect();
    if matches!(provider, LlmProvider::OpenRouter) {
        for model in &state.openrouter_free_models {
            if !models.iter().any(|item| item == model) {
                models.push(model.clone());
            }
        }
    }

    if !current_model.trim().is_empty() && !models.iter().any(|item| item == current_model) {
        models.insert(0, current_model.to_string());
    }

    if models.is_empty() {
        models.push("(no model presets)".to_string());
    }
    models
}

async fn refresh_openrouter_free_models_update() -> Result<Vec<String>> {
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .map(|value| value.trim().to_string())
        .ok()
        .filter(|value| !value.is_empty());

    let Some(api_key) = api_key else {
        return Ok(Vec::new());
    };

    fetch_openrouter_free_models(&api_key)
        .await
        .map_err(|err| match err {
            crate::error::MycryptoError::ApiError { .. }
            | crate::error::MycryptoError::Http(_)
            | crate::error::MycryptoError::Json(_) => crate::error::MycryptoError::LlmRequest(
                format!("Failed to refresh OpenRouter free models: {}", err),
            ),
            _ => err,
        })
}

fn build_tab_labels_for_width(width: usize) -> Vec<String> {
    const FULL: [&str; 7] = [
        "Portfolio",
        "Signals",
        "Chart",
        "Team",
        "News",
        "Heatmap",
        "Status",
    ];
    const MEDIUM: [&str; 7] = [
        "Portf", "Signals", "Chart", "Team", "News", "Heat", "Status",
    ];
    const COMPACT: [&str; 7] = ["Port", "Sig", "Chart", "Team", "News", "Heat", "Stat"];
    const TINY: [&str; 7] = ["P", "S", "C", "T", "N", "H", "S"];

    let candidates = [
        format_tab_labels(&FULL, "  "),
        format_tab_labels(&MEDIUM, "  "),
        format_tab_labels(&FULL, " "),
        format_tab_labels(&MEDIUM, " "),
        format_tab_labels(&COMPACT, " "),
        format_tab_labels(&COMPACT, ""),
        format_tab_labels(&TINY, ""),
    ];

    for labels in candidates {
        let used: usize = labels.iter().map(|label| label.chars().count()).sum();
        if used <= width {
            return labels;
        }
    }

    format_tab_labels(&TINY, "")
}

fn format_tab_labels(names: &[&str; 7], side_padding: &str) -> Vec<String> {
    names
        .iter()
        .enumerate()
        .map(|(idx, name)| format!("{pad}[{}] {name}{pad}", idx + 1, pad = side_padding))
        .collect()
}

/// Run the TUI with the given state.
pub fn run(state: AppState) -> Result<()> {
    let handle = Handle::current();
    let mut app = App::new(state, handle);
    app.run()
}

/// Format a price value compactly.
fn format_price_compact(price: Decimal) -> String {
    if price >= Decimal::from(10000) {
        format!("{:.0}", price)
    } else if price >= Decimal::from(100) {
        format!("{:.1}", price)
    } else if price >= Decimal::from(1) {
        format!("{:.2}", price)
    } else {
        format!("{:.4}", price)
    }
}

fn marquee_text(source: &str, width: usize, offset: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let chars: Vec<char> = source.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() <= width {
        return source.to_string();
    }
    let mut out = String::with_capacity(width);
    for i in 0..width {
        let idx = (offset + i) % chars.len();
        out.push(chars[idx]);
    }
    out
}

/// Parse a timeframe string.
fn chart_refresh_cadence(timeframe: Timeframe) -> Duration {
    match timeframe {
        Timeframe::H1 | Timeframe::H4 => Duration::from_secs(5 * 60),
        Timeframe::D1 => Duration::from_secs(30 * 60),
        Timeframe::W1 | Timeframe::MO1 => Duration::from_secs(2 * 60 * 60),
        _ => Duration::from_secs(30 * 60),
    }
}

fn hash_agent_status(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Running => 0,
        AgentStatus::Paused => 1,
    }
}

fn hash_llm_provider(provider: &LlmProvider) -> u8 {
    match provider {
        LlmProvider::Claude => 0,
        LlmProvider::OpenAI => 1,
        LlmProvider::Gemini => 2,
        LlmProvider::OpenRouter => 3,
        LlmProvider::Gradio => 4,
        LlmProvider::Copilot => 5,
        LlmProvider::Mock => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_state() -> AppState {
        AppState::new(Config::default())
    }

    fn test_handle() -> Handle {
        if let Ok(handle) = Handle::try_current() {
            return handle;
        }

        static TEST_RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> =
            std::sync::OnceLock::new();
        let runtime =
            TEST_RUNTIME.get_or_init(|| {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(err) => panic!("failed to build test runtime: {}", err),
                }
            });
        runtime.handle().clone()
    }

    #[test]
    fn test_app_creation() {
        let app = App::new(test_state(), test_handle());
        assert!(!app.should_quit);
        assert!(matches!(app.page, Page::Portfolio));
    }

    #[test]
    fn test_command_portfolio() {
        let mut app = App::new(test_state(), test_handle());
        app.page = Page::Help;
        app.execute_command(Command::Portfolio);
        assert!(matches!(app.page, Page::Portfolio));
    }

    #[test]
    fn test_command_pause_resume() {
        let mut app = App::new(test_state(), test_handle());
        assert!(matches!(app.state.agent_status, AgentStatus::Running));

        app.execute_command(Command::Pause);
        assert!(matches!(app.state.agent_status, AgentStatus::Paused));

        app.execute_command(Command::Resume);
        assert!(matches!(app.state.agent_status, AgentStatus::Running));
    }

    #[test]
    fn test_command_unknown() {
        let mut app = App::new(test_state(), test_handle());
        app.execute_command(Command::Unknown {
            input: "test error".to_string(),
        });
        assert!(app.error_message.is_some());
    }

    #[test]
    fn test_command_buy_sets_confirmation() {
        let mut app = App::new(test_state(), test_handle());
        app.execute_command(Command::Buy {
            pair: "BTCUSDT".to_string(),
            size: Decimal::new(1, 1),
        });

        assert!(matches!(app.confirm, ConfirmState::Buy { .. }));
    }

    #[test]
    fn test_confirm_buy_opens_position() {
        let mut app = App::new(test_state(), test_handle());

        let mut ticker = crate::state::Ticker::new("BTCUSDT".to_string());
        ticker.price = Decimal::from(50000);
        ticker.bid_price = Decimal::from(49999);
        ticker.ask_price = Decimal::from(50001);
        app.state.apply_update(StateUpdate::MarketTick(ticker));

        app.execute_command(Command::Buy {
            pair: "BTCUSDT".to_string(),
            size: Decimal::new(1, 2),
        });

        app.handle_key(KeyCode::Char('y'), KeyModifiers::empty());

        assert!(app.state.portfolio.has_position("BTCUSDT"));
    }

    #[test]
    fn test_parse_timeframe() {
        use crate::state::Timeframe;
        assert_eq!(Timeframe::from_chart_label("1m"), Some(Timeframe::M1));
        assert_eq!(Timeframe::from_chart_label("1H"), Some(Timeframe::H1));
        assert_eq!(Timeframe::from_chart_label("4h"), Some(Timeframe::H4));
        assert_eq!(Timeframe::from_chart_label("1w"), Some(Timeframe::W1));
        assert_eq!(Timeframe::from_chart_label("7d"), Some(Timeframe::W1));
        assert_eq!(Timeframe::from_chart_label("1M"), Some(Timeframe::MO1));
        assert_eq!(Timeframe::from_chart_label("invalid"), None);
    }

    #[test]
    fn test_chart_refresh_cadence_mapping() {
        assert_eq!(
            chart_refresh_cadence(Timeframe::H1),
            Duration::from_secs(300)
        );
        assert_eq!(
            chart_refresh_cadence(Timeframe::D1),
            Duration::from_secs(1800)
        );
        assert_eq!(
            chart_refresh_cadence(Timeframe::MO1),
            Duration::from_secs(7200)
        );
    }

    #[test]
    fn test_autocomplete() {
        let mut app = App::new(test_state(), test_handle());
        assert!(!app.autocomplete.visible);

        app.handle_key(KeyCode::Char('/'), KeyModifiers::empty());
        assert!(app.autocomplete.visible);

        app.handle_key(KeyCode::Esc, KeyModifiers::empty());
        assert!(!app.autocomplete.visible);
    }

    #[test]
    fn test_provider_authenticated_accepts_env_var() {
        std::env::set_var("OPENROUTER_API_KEY", "test-openrouter-key");

        let state = test_state();
        assert!(provider_authenticated(&state, &LlmProvider::OpenRouter));

        std::env::remove_var("OPENROUTER_API_KEY");
    }

    #[test]
    fn test_model_selection_rebuilds_chat_engine_provider() {
        std::env::set_var("OPENAI_API_KEY", "test-openai-key");

        let mut config = Config::default();
        config.llm.provider = LlmProvider::Mock;
        config.llm.model = "mock-v1".to_string();

        let mut app = App::new(AppState::new(config), test_handle());
        assert_eq!(app.chat_engine.provider_name(), "mock");

        app.page = Page::Model;
        app.model_selector.provider = LlmProvider::OpenAI;
        app.model_selector.model = "gpt-4o".to_string();
        app.model_selector.api_key_set = true;

        app.handle_key(KeyCode::Enter, KeyModifiers::empty());

        assert_eq!(app.state.config.llm.provider, LlmProvider::OpenAI);
        assert_eq!(app.chat_engine.provider_name(), "openai");

        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_auth_command_provider_shortcut_opens_provider_input() {
        let mut app = App::new(test_state(), test_handle());

        app.execute_command(Command::Auth {
            action: Some("openrouter".to_string()),
        });

        assert_eq!(app.page, Page::Auth);
        match &app.auth_input_mode {
            AuthInputMode::ApiKey { provider, .. } => {
                assert_eq!(*provider, AuthProvider::OpenRouter);
            }
            _ => panic!("expected OpenRouter API key auth mode"),
        }
    }

    #[test]
    fn test_tab_index_for_heatmap() {
        let mut app = App::new(test_state(), test_handle());
        app.page = Page::Heatmap;
        assert_eq!(app.active_tab_index(), 5);
    }

    #[test]
    fn test_tab_labels_fit_width_above_80_cols() {
        for width in [80usize, 96, 120] {
            let labels = build_tab_labels_for_width(width);
            assert_eq!(labels.len(), 7);
            let used: usize = labels.iter().map(|label| label.chars().count()).sum();
            assert!(used <= width);
            for idx in 0..7 {
                assert!(labels[idx].contains(&format!("[{}]", idx + 1)));
            }
        }

        let roomy = build_tab_labels_for_width(140);
        assert_eq!(roomy[0], "  [1] Portfolio  ");
    }

    #[test]
    fn test_model_page_m_key_cycles_model() {
        let mut app = App::new(test_state(), test_handle());
        app.page = Page::Model;
        app.model_selector.provider = LlmProvider::OpenAI;
        app.model_selector.provider_index = 1;
        app.model_selector.model_index = 0;
        app.model_selector.model = "gpt-4o".to_string();

        app.handle_key(KeyCode::Char('m'), KeyModifiers::empty());

        assert_eq!(app.model_selector.model, "gpt-4o-mini");
    }

    #[test]
    fn test_model_page_m_key_cycles_openrouter_dynamic_free_models() {
        let mut app = App::new(test_state(), test_handle());
        app.state.openrouter_free_models = vec![
            "openrouter/free-a".to_string(),
            "openrouter/free-b".to_string(),
        ];

        app.page = Page::Model;
        app.model_selector.provider = LlmProvider::OpenRouter;
        app.model_selector.provider_index = 3;
        app.model_selector.model = "google/gemini-2.0-flash-001".to_string();

        app.handle_key(KeyCode::Char('m'), KeyModifiers::empty());
        assert_eq!(app.model_selector.model, "openrouter/free-a");

        app.handle_key(KeyCode::Char('m'), KeyModifiers::empty());
        assert_eq!(app.model_selector.model, "openrouter/free-b");
    }

    #[test]
    fn test_model_page_a_key_opens_auth_for_device_flow_provider() {
        let mut app = App::new(test_state(), test_handle());
        app.page = Page::Model;
        app.model_selector.provider = LlmProvider::Copilot;
        app.model_selector.provider_index = 5;

        app.handle_key(KeyCode::Char('a'), KeyModifiers::empty());

        assert_eq!(app.page, Page::Auth);
        match app.auth_input_mode {
            AuthInputMode::Select => {}
            _ => panic!("expected select auth mode for device flow provider"),
        }
    }

    #[test]
    fn test_model_page_a_key_enters_inline_api_key_input_for_key_providers() {
        let mut app = App::new(test_state(), test_handle());
        app.page = Page::Model;
        app.model_selector.provider = LlmProvider::OpenAI;
        app.model_selector.provider_index = 1;

        app.handle_key(KeyCode::Char('a'), KeyModifiers::empty());

        assert_eq!(app.model_input_mode, ModelInputMode::ApiKey);
    }

    #[test]
    fn test_paste_text_appends_to_auth_api_key_input() {
        let mut app = App::new(test_state(), test_handle());
        app.page = Page::Auth;
        app.auth_input_mode = AuthInputMode::ApiKey {
            provider: AuthProvider::OpenAI,
            input: String::new(),
        };

        app.handle_paste_text("sk-test-123");

        match &app.auth_input_mode {
            AuthInputMode::ApiKey { input, .. } => assert_eq!(input, "sk-test-123"),
            _ => panic!("expected api key mode"),
        }
    }

    #[test]
    fn test_paste_text_appends_to_model_inline_input() {
        let mut app = App::new(test_state(), test_handle());
        app.page = Page::Model;
        app.model_input_mode = ModelInputMode::ApiKey;

        app.handle_paste_text("openai-key-abc");

        assert_eq!(app.model_api_key_input, "openai-key-abc");
    }

    #[test]
    fn test_active_input_target_tracks_context() {
        let mut app = App::new(test_state(), test_handle());

        app.page = Page::Auth;
        app.auth_input_mode = AuthInputMode::ApiKey {
            provider: AuthProvider::OpenAI,
            input: String::new(),
        };
        app.refresh_active_input_target();
        assert!(matches!(
            app.active_input_target,
            ActiveInputTarget::AuthApiKey
        ));

        app.page = Page::NewsHistory;
        app.news_history_search_mode = true;
        app.refresh_active_input_target();
        assert!(matches!(
            app.active_input_target,
            ActiveInputTarget::NewsFilter
        ));
    }

    #[test]
    fn test_paste_text_routes_to_news_filter_buffer() {
        let mut app = App::new(test_state(), test_handle());
        app.page = Page::NewsHistory;
        app.news_history_search_mode = true;

        app.handle_paste_text("btc");

        assert_eq!(app.news_history_query, "btc");
        assert!(app.input.is_empty());
    }

    #[test]
    fn test_chat_manual_scroll_disables_auto_scroll() {
        let mut app = App::new(test_state(), test_handle());
        app.page = Page::Chat;
        app.chat_auto_scroll = true;

        app.handle_key(KeyCode::Up, KeyModifiers::empty());

        assert!(!app.chat_auto_scroll);
    }

    #[test]
    fn test_github_auth_success_transitions_back_to_model_page() {
        let mut app = App::new(test_state(), test_handle());
        app.page = Page::Auth;
        app.update_tx
            .try_send(StateUpdate::AuthStateChanged {
                provider: AuthProvider::GitHub,
                status: AuthStatus::AuthenticatedGitHub {
                    username: "octocat".to_string(),
                    token: "tok".to_string(),
                    created_at: Utc::now(),
                },
            })
            .expect("queue auth update");

        app.process_updates();

        assert_eq!(app.page, Page::Model);
        assert!(app
            .auth_message
            .as_deref()
            .unwrap_or_default()
            .contains("octocat"));
    }

    #[test]
    fn test_activity_events_capped_at_20() {
        let mut app = App::new(test_state(), test_handle());

        for i in 0..25 {
            app.capture_activity_event(&StateUpdate::TeamSessionStarted {
                prompt: format!("p{}", i),
                session_id: i as u64,
            });
        }

        assert_eq!(app.activity_events.len(), 20);
        assert_eq!(
            app.activity_events.front().map(String::as_str),
            Some("team started: p5")
        );
        assert_eq!(
            app.activity_events.back().map(String::as_str),
            Some("team started: p24")
        );
    }

    #[test]
    fn test_render_hash_stable_without_state_change() {
        let app = App::new(test_state(), test_handle());
        let area = Rect::new(0, 0, 120, 40);

        let a = app.render_state_hash(area);
        let b = app.render_state_hash(area);

        assert_eq!(a, b);
    }
}
