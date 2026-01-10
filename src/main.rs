//! littlejohn - Terminal UI for torrent search with Real-Debrid integration

#![allow(dead_code)]

mod realdebrid;
mod scrapers;
mod ui;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use std::path::PathBuf;
use tokio::sync::mpsc;

use realdebrid::{RealDebridClient, TorrentFile};
use scrapers::TorrentResult;

/// Download status
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Completed,
    Failed(String),
    Cancelled,
}

/// A download in progress
#[derive(Debug, Clone)]
pub struct Download {
    pub url: String,
    pub filename: String,
    pub dest_path: PathBuf,
    pub status: DownloadStatus,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub speed: f64, // bytes per second
}

impl Download {
    pub fn progress(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.downloaded_bytes as f64 / self.total_bytes as f64) * 100.0
        }
    }

    pub fn speed_str(&self) -> String {
        format_bytes(self.speed) + "/s"
    }
}

/// Format bytes to human readable
pub fn format_bytes(bytes: f64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes;
    for unit in UNITS {
        if size < 1024.0 {
            return format!("{:.1} {}", size, unit);
        }
        size /= 1024.0;
    }
    format!("{:.1} PB", size)
}

/// Format seconds to human readable
pub fn format_time(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{}s", seconds as u64)
    } else if seconds < 3600.0 {
        format!("{}m {}s", (seconds / 60.0) as u64, (seconds % 60.0) as u64)
    } else {
        format!("{}h {}m", (seconds / 3600.0) as u64, ((seconds % 3600.0) / 60.0) as u64)
    }
}

type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Application mode/screen
#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Setup,      // First-run setup wizard
    Settings,   // Settings screen (accessible anytime)
    Search,
    Results,
    FileSelect,
    SourceSelect,
    Downloads,
    Processing,
    Error(String),
}

/// Settings field being edited
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsField {
    RdApiToken,
    FirecrawlApiKey,
    DownloadDir,
}

/// Source priority order (matching Python implementation)
pub const SOURCE_PRIORITY: &[&str] = &["yts", "ilcorsaronero", "tpb", "bitsearch", "1337x", "extto"];

/// Application state
pub struct App {
    /// Current mode/screen
    pub mode: AppMode,
    /// Search input
    pub search_input: String,
    /// Cursor position in search input
    pub cursor_pos: usize,
    /// Search results
    pub results: Vec<TorrentResult>,
    /// Selected result index
    pub selected_index: usize,
    /// Scroll offset for results list
    pub scroll_offset: usize,
    /// Current page
    pub page: u32,
    /// Files in selected torrent
    pub files: Vec<TorrentFile>,
    /// Selected file IDs
    pub selected_files: std::collections::HashSet<u32>,
    /// File selector cursor
    pub file_cursor: usize,
    /// File selector scroll offset
    pub file_scroll_offset: usize,
    /// Torrent ID (for RD)
    pub torrent_id: Option<String>,
    /// Status message
    pub status: String,
    /// Should quit
    pub should_quit: bool,
    /// Real-Debrid client
    pub rd_client: Option<RealDebridClient>,
    /// Processing status
    pub processing_status: String,
    /// Enabled sources for searching
    pub enabled_sources: std::collections::HashSet<String>,
    /// Source selector cursor
    pub source_cursor: usize,
    /// Downloads list
    pub downloads: Vec<Download>,
    /// Download cursor
    pub download_cursor: usize,
    /// Current settings field being edited
    pub settings_field: SettingsField,
    /// Settings input: RD API Token
    pub settings_rd_token: String,
    /// Settings input: Firecrawl API Key
    pub settings_firecrawl_key: String,
    /// Settings input: Download Directory
    pub settings_download_dir: String,
    /// Cursor position in current settings input
    pub settings_cursor: usize,
}

impl App {
    pub fn new() -> Self {
        let rd_client = RealDebridClient::new().ok();

        // All sources enabled by default
        let enabled_sources: std::collections::HashSet<String> =
            scrapers::SCRAPERS.iter().map(|s| s.to_string()).collect();

        // Load current settings from env
        let settings_rd_token = std::env::var("RD_API_TOKEN").unwrap_or_default();
        let settings_firecrawl_key = std::env::var("FIRECRAWL_API_KEY").unwrap_or_default();
        let settings_download_dir = std::env::var("DOWNLOAD_DIR").unwrap_or_default();

        Self {
            mode: AppMode::Search,
            search_input: String::new(),
            cursor_pos: 0,
            results: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            page: 1,
            files: Vec::new(),
            selected_files: std::collections::HashSet::new(),
            file_cursor: 0,
            file_scroll_offset: 0,
            torrent_id: None,
            status: String::new(),
            should_quit: false,
            rd_client,
            processing_status: String::new(),
            enabled_sources,
            source_cursor: 0,
            downloads: Vec::new(),
            download_cursor: 0,
            settings_field: SettingsField::RdApiToken,
            settings_rd_token,
            settings_firecrawl_key,
            settings_download_dir,
            settings_cursor: 0,
        }
    }

    pub fn visible_height(&self) -> usize {
        20 // Approximate visible rows
    }

    /// Get the current settings field input
    pub fn current_settings_input(&self) -> &str {
        match self.settings_field {
            SettingsField::RdApiToken => &self.settings_rd_token,
            SettingsField::FirecrawlApiKey => &self.settings_firecrawl_key,
            SettingsField::DownloadDir => &self.settings_download_dir,
        }
    }

    /// Get the current settings field input mutably
    pub fn current_settings_input_mut(&mut self) -> &mut String {
        match self.settings_field {
            SettingsField::RdApiToken => &mut self.settings_rd_token,
            SettingsField::FirecrawlApiKey => &mut self.settings_firecrawl_key,
            SettingsField::DownloadDir => &mut self.settings_download_dir,
        }
    }

    /// Move to next settings field
    pub fn next_settings_field(&mut self) {
        self.settings_field = match self.settings_field {
            SettingsField::RdApiToken => SettingsField::FirecrawlApiKey,
            SettingsField::FirecrawlApiKey => SettingsField::DownloadDir,
            SettingsField::DownloadDir => SettingsField::RdApiToken,
        };
        self.settings_cursor = self.current_settings_input().len();
    }

    /// Move to previous settings field
    pub fn prev_settings_field(&mut self) {
        self.settings_field = match self.settings_field {
            SettingsField::RdApiToken => SettingsField::DownloadDir,
            SettingsField::FirecrawlApiKey => SettingsField::RdApiToken,
            SettingsField::DownloadDir => SettingsField::FirecrawlApiKey,
        };
        self.settings_cursor = self.current_settings_input().len();
    }

    /// Save settings to config file
    pub fn save_settings(&self) -> std::io::Result<()> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Config directory not found"))?
            .join("littlejohn");

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&config_dir)?;

        let config_path = config_dir.join(".env");

        let mut content = String::new();
        content.push_str("# littlejohn configuration\n\n");

        if !self.settings_rd_token.is_empty() {
            content.push_str(&format!("RD_API_TOKEN={}\n", self.settings_rd_token));
        }
        if !self.settings_firecrawl_key.is_empty() {
            content.push_str(&format!("FIRECRAWL_API_KEY={}\n", self.settings_firecrawl_key));
        }
        if !self.settings_download_dir.is_empty() {
            content.push_str(&format!("DOWNLOAD_DIR={}\n", self.settings_download_dir));
        }

        std::fs::write(&config_path, content)?;
        Ok(())
    }

    /// Reinitialize RD client with current token
    pub fn reinit_rd_client(&mut self) {
        if !self.settings_rd_token.is_empty() {
            std::env::set_var("RD_API_TOKEN", &self.settings_rd_token);
            self.rd_client = RealDebridClient::new().ok();
        }
    }
}

/// Messages for async operations
#[derive(Debug)]
pub enum AppMessage {
    SearchResults(Vec<TorrentResult>),
    SearchError(String),
    TorrentFiles(String, Vec<TorrentFile>),
    TorrentError(String),
    DownloadLinks(Vec<(String, String)>), // (filename, url)
    DownloadError(String),
    StatusUpdate(String),
    // Download manager messages
    DownloadProgress {
        index: usize,
        downloaded: u64,
        total: u64,
        speed: f64,
    },
    DownloadComplete(usize),
    DownloadFailed(usize, String),
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file - check current directory first, then config directory
    if dotenvy::dotenv().is_err() {
        if let Some(config_dir) = dirs::config_dir() {
            let config_env = config_dir.join("littlejohn").join(".env");
            dotenvy::from_path(&config_env).ok();
        }
    }

    // Initialize scraper logging
    scrapers::init_log();

    // Initialize terminal
    let mut terminal = ratatui::init();

    // Create app
    let mut app = App::new();

    // Show setup wizard if RD token is not set
    if app.rd_client.is_none() && app.settings_rd_token.is_empty() {
        app.mode = AppMode::Setup;
        app.settings_cursor = 0;
    }

    // Create channel for async messages
    let (tx, mut rx) = mpsc::unbounded_channel::<AppMessage>();

    // Run app
    let result = run_app(&mut terminal, &mut app, tx, &mut rx).await;

    // Restore terminal
    ratatui::restore();

    result
}

async fn run_app(
    terminal: &mut Tui,
    app: &mut App,
    tx: mpsc::UnboundedSender<AppMessage>,
    rx: &mut mpsc::UnboundedReceiver<AppMessage>,
) -> Result<()> {
    loop {
        // Draw UI
        terminal.draw(|frame| ui::draw(frame, app))?;

        // Handle events with timeout to allow processing async messages
        if crossterm::event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key_event(app, key.code, key.modifiers, tx.clone()).await;
                }
            }
        }

        // Process any pending async messages
        while let Ok(msg) = rx.try_recv() {
            handle_message(app, msg);
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

async fn handle_key_event(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
    tx: mpsc::UnboundedSender<AppMessage>,
) {
    // Global quit
    if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    match &app.mode {
        AppMode::Setup => handle_setup_keys(app, code),
        AppMode::Settings => handle_settings_keys(app, code),
        AppMode::Search => handle_search_keys(app, code, tx).await,
        AppMode::Results => handle_results_keys(app, code, tx).await,
        AppMode::FileSelect => handle_file_select_keys(app, code, tx).await,
        AppMode::SourceSelect => handle_source_select_keys(app, code),
        AppMode::Downloads => handle_downloads_keys(app, code, tx).await,
        AppMode::Processing => {
            // Only allow quit during processing
            if code == KeyCode::Esc {
                app.mode = AppMode::Results;
            }
        }
        AppMode::Error(_) => {
            // Any key returns to previous mode
            app.mode = AppMode::Search;
            app.status.clear();
        }
    }
}

/// Handle setup wizard keys
fn handle_setup_keys(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Tab | KeyCode::Down => {
            app.next_settings_field();
        }
        KeyCode::BackTab | KeyCode::Up => {
            app.prev_settings_field();
        }
        KeyCode::Char(c) => {
            let cursor = app.settings_cursor;
            app.current_settings_input_mut().insert(cursor, c);
            app.settings_cursor += 1;
        }
        KeyCode::Backspace => {
            if app.settings_cursor > 0 {
                app.settings_cursor -= 1;
                let cursor = app.settings_cursor;
                app.current_settings_input_mut().remove(cursor);
            }
        }
        KeyCode::Delete => {
            let len = app.current_settings_input().len();
            let cursor = app.settings_cursor;
            if cursor < len {
                app.current_settings_input_mut().remove(cursor);
            }
        }
        KeyCode::Left => {
            app.settings_cursor = app.settings_cursor.saturating_sub(1);
        }
        KeyCode::Right => {
            let len = app.current_settings_input().len();
            if app.settings_cursor < len {
                app.settings_cursor += 1;
            }
        }
        KeyCode::Home => {
            app.settings_cursor = 0;
        }
        KeyCode::End => {
            app.settings_cursor = app.current_settings_input().len();
        }
        KeyCode::Enter => {
            // Save settings and continue
            if app.settings_rd_token.is_empty() {
                app.status = "RD API Token is required".to_string();
            } else {
                match app.save_settings() {
                    Ok(_) => {
                        app.reinit_rd_client();
                        app.status = "Settings saved!".to_string();
                        app.mode = AppMode::Search;
                    }
                    Err(e) => {
                        app.status = format!("Failed to save: {}", e);
                    }
                }
            }
        }
        KeyCode::Esc => {
            // Skip setup (user can configure later)
            app.mode = AppMode::Search;
            app.status = "Setup skipped. Press Shift+S to configure settings.".to_string();
        }
        _ => {}
    }
}

/// Handle settings screen keys
fn handle_settings_keys(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Tab | KeyCode::Down => {
            app.next_settings_field();
        }
        KeyCode::BackTab | KeyCode::Up => {
            app.prev_settings_field();
        }
        KeyCode::Char(c) => {
            let cursor = app.settings_cursor;
            app.current_settings_input_mut().insert(cursor, c);
            app.settings_cursor += 1;
        }
        KeyCode::Backspace => {
            if app.settings_cursor > 0 {
                app.settings_cursor -= 1;
                let cursor = app.settings_cursor;
                app.current_settings_input_mut().remove(cursor);
            }
        }
        KeyCode::Delete => {
            let len = app.current_settings_input().len();
            let cursor = app.settings_cursor;
            if cursor < len {
                app.current_settings_input_mut().remove(cursor);
            }
        }
        KeyCode::Left => {
            app.settings_cursor = app.settings_cursor.saturating_sub(1);
        }
        KeyCode::Right => {
            let len = app.current_settings_input().len();
            if app.settings_cursor < len {
                app.settings_cursor += 1;
            }
        }
        KeyCode::Home => {
            app.settings_cursor = 0;
        }
        KeyCode::End => {
            app.settings_cursor = app.current_settings_input().len();
        }
        KeyCode::Enter => {
            // Save settings
            match app.save_settings() {
                Ok(_) => {
                    app.reinit_rd_client();
                    app.status = "Settings saved!".to_string();
                    app.mode = AppMode::Search;
                }
                Err(e) => {
                    app.status = format!("Failed to save: {}", e);
                }
            }
        }
        KeyCode::Esc => {
            // Cancel without saving
            // Reload settings from env
            app.settings_rd_token = std::env::var("RD_API_TOKEN").unwrap_or_default();
            app.settings_firecrawl_key = std::env::var("FIRECRAWL_API_KEY").unwrap_or_default();
            app.settings_download_dir = std::env::var("DOWNLOAD_DIR").unwrap_or_default();
            app.mode = AppMode::Search;
        }
        _ => {}
    }
}

async fn handle_search_keys(
    app: &mut App,
    code: KeyCode,
    tx: mpsc::UnboundedSender<AppMessage>,
) {
    match code {
        // Special shortcuts when input is empty
        KeyCode::Char('s') if app.search_input.is_empty() => {
            app.source_cursor = 0;
            app.mode = AppMode::SourceSelect;
            return;
        }
        KeyCode::Char('S') if app.search_input.is_empty() => {
            // Open settings (Shift+S)
            app.settings_field = SettingsField::RdApiToken;
            app.settings_cursor = app.settings_rd_token.len();
            app.mode = AppMode::Settings;
            return;
        }
        KeyCode::Char('d') if app.search_input.is_empty() => {
            app.download_cursor = 0;
            app.mode = AppMode::Downloads;
            return;
        }
        KeyCode::Char(c) => {
            app.search_input.insert(app.cursor_pos, c);
            app.cursor_pos += 1;
        }
        KeyCode::Backspace => {
            if app.cursor_pos > 0 {
                app.cursor_pos -= 1;
                app.search_input.remove(app.cursor_pos);
            }
        }
        KeyCode::Delete => {
            if app.cursor_pos < app.search_input.len() {
                app.search_input.remove(app.cursor_pos);
            }
        }
        KeyCode::Left => {
            app.cursor_pos = app.cursor_pos.saturating_sub(1);
        }
        KeyCode::Right => {
            if app.cursor_pos < app.search_input.len() {
                app.cursor_pos += 1;
            }
        }
        KeyCode::Home => {
            app.cursor_pos = 0;
        }
        KeyCode::End => {
            app.cursor_pos = app.search_input.len();
        }
        KeyCode::Enter => {
            // Check if input is a magnet link
            if app.search_input.starts_with("magnet:") {
                let magnet = app.search_input.clone();
                if let Some(rd_client) = &app.rd_client {
                    let rd_client = rd_client.clone();
                    let tx = tx.clone();

                    app.mode = AppMode::Processing;
                    app.processing_status = "Adding magnet to Real-Debrid...".to_string();

                    tokio::spawn(async move {
                        let _ = tx.send(AppMessage::StatusUpdate("Adding magnet...".to_string()));
                        match rd_client.get_torrent_files(&magnet).await {
                            Ok((torrent_id, files)) => {
                                let _ = tx.send(AppMessage::TorrentFiles(torrent_id, files));
                            }
                            Err(e) => {
                                let _ = tx.send(AppMessage::TorrentError(e.to_string()));
                            }
                        }
                    });
                } else {
                    app.status = "Real-Debrid not configured".to_string();
                }
            } else if app.search_input.len() >= 2 {
                // Start search
                let query = app.search_input.clone();
                let tx = tx.clone();
                let enabled_sources = app.enabled_sources.clone();

                app.page = 1; // Reset page on new search
                app.status = format!("Searching for '{}'...", query);
                app.mode = AppMode::Processing;
                app.processing_status = format!("Searching {} sites...", enabled_sources.len());

                tokio::spawn(async move {
                    let mut results = scrapers::search_all(&query, 1).await;

                    // Filter by enabled sources
                    results.retain(|r| enabled_sources.contains(&r.source));

                    // Sort by source priority, then by seeders
                    results.sort_by(|a, b| {
                        let a_priority = SOURCE_PRIORITY.iter().position(|&s| s == a.source).unwrap_or(999);
                        let b_priority = SOURCE_PRIORITY.iter().position(|&s| s == b.source).unwrap_or(999);
                        match a_priority.cmp(&b_priority) {
                            std::cmp::Ordering::Equal => b.seeders.cmp(&a.seeders),
                            other => other,
                        }
                    });

                    if results.is_empty() {
                        let _ = tx.send(AppMessage::SearchError("No results found".to_string()));
                    } else {
                        let _ = tx.send(AppMessage::SearchResults(results));
                    }
                });
            } else {
                app.status = "Query must be at least 2 characters".to_string();
            }
        }
        KeyCode::Esc => {
            app.should_quit = true;
        }
        _ => {}
    }
}

async fn handle_results_keys(
    app: &mut App,
    code: KeyCode,
    tx: mpsc::UnboundedSender<AppMessage>,
) {
    let visible_height = app.visible_height();

    match code {
        KeyCode::Up | KeyCode::Char('k') => {
            if app.selected_index > 0 {
                app.selected_index -= 1;
                if app.selected_index < app.scroll_offset {
                    app.scroll_offset = app.selected_index;
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.selected_index < app.results.len().saturating_sub(1) {
                app.selected_index += 1;
                if app.selected_index >= app.scroll_offset + visible_height {
                    app.scroll_offset = app.selected_index - visible_height + 1;
                }
            }
        }
        KeyCode::PageUp => {
            app.selected_index = app.selected_index.saturating_sub(visible_height);
            app.scroll_offset = app.scroll_offset.saturating_sub(visible_height);
        }
        KeyCode::PageDown => {
            app.selected_index = (app.selected_index + visible_height).min(app.results.len().saturating_sub(1));
            if app.selected_index >= app.scroll_offset + visible_height {
                app.scroll_offset = app.selected_index - visible_height + 1;
            }
        }
        KeyCode::Home => {
            app.selected_index = 0;
            app.scroll_offset = 0;
        }
        KeyCode::End => {
            app.selected_index = app.results.len().saturating_sub(1);
            if app.selected_index >= visible_height {
                app.scroll_offset = app.selected_index - visible_height + 1;
            }
        }
        KeyCode::Enter => {
            if let Some(result) = app.results.get(app.selected_index) {
                let magnet = &result.magnet;
                if !magnet.is_empty() {
                    if let Some(rd_client) = &app.rd_client {
                        let magnet = magnet.clone();
                        let rd_client = rd_client.clone();
                        let tx = tx.clone();

                        app.mode = AppMode::Processing;
                        app.processing_status = "Adding magnet to Real-Debrid...".to_string();

                        tokio::spawn(async move {
                            let _ = tx.send(AppMessage::StatusUpdate("Adding magnet...".to_string()));
                            match rd_client.get_torrent_files(&magnet).await {
                                Ok((torrent_id, files)) => {
                                    let _ = tx.send(AppMessage::TorrentFiles(torrent_id, files));
                                }
                                Err(e) => {
                                    let _ = tx.send(AppMessage::TorrentError(e.to_string()));
                                }
                            }
                        });
                    } else {
                        app.status = "Real-Debrid not configured".to_string();
                    }
                } else {
                    app.status = "No magnet link available".to_string();
                }
            }
        }
        KeyCode::Char('n') => {
            // Next page
            let query = app.search_input.clone();
            let tx = tx.clone();
            let next_page = app.page + 1;
            let enabled_sources = app.enabled_sources.clone();

            app.status = format!("Loading page {}...", next_page);
            app.mode = AppMode::Processing;
            app.processing_status = "Searching...".to_string();

            tokio::spawn(async move {
                let mut results = scrapers::search_all(&query, next_page).await;

                // Filter by enabled sources
                results.retain(|r| enabled_sources.contains(&r.source));

                // Sort by source priority, then by seeders
                results.sort_by(|a, b| {
                    let a_priority = SOURCE_PRIORITY.iter().position(|&s| s == a.source).unwrap_or(999);
                    let b_priority = SOURCE_PRIORITY.iter().position(|&s| s == b.source).unwrap_or(999);
                    match a_priority.cmp(&b_priority) {
                        std::cmp::Ordering::Equal => b.seeders.cmp(&a.seeders),
                        other => other,
                    }
                });

                if results.is_empty() {
                    let _ = tx.send(AppMessage::SearchError("No more results".to_string()));
                } else {
                    let _ = tx.send(AppMessage::SearchResults(results));
                }
            });

            app.page = next_page;
        }
        KeyCode::Char('p') => {
            // Previous page
            if app.page > 1 {
                let query = app.search_input.clone();
                let tx = tx.clone();
                let prev_page = app.page - 1;
                let enabled_sources = app.enabled_sources.clone();

                app.status = format!("Loading page {}...", prev_page);
                app.mode = AppMode::Processing;
                app.processing_status = "Searching...".to_string();

                tokio::spawn(async move {
                    let mut results = scrapers::search_all(&query, prev_page).await;

                    // Filter by enabled sources
                    results.retain(|r| enabled_sources.contains(&r.source));

                    // Sort by source priority, then by seeders
                    results.sort_by(|a, b| {
                        let a_priority = SOURCE_PRIORITY.iter().position(|&s| s == a.source).unwrap_or(999);
                        let b_priority = SOURCE_PRIORITY.iter().position(|&s| s == b.source).unwrap_or(999);
                        match a_priority.cmp(&b_priority) {
                            std::cmp::Ordering::Equal => b.seeders.cmp(&a.seeders),
                            other => other,
                        }
                    });

                    if results.is_empty() {
                        let _ = tx.send(AppMessage::SearchError("No results".to_string()));
                    } else {
                        let _ = tx.send(AppMessage::SearchResults(results));
                    }
                });

                app.page = prev_page;
            }
        }
        KeyCode::Char('s') => {
            // Open source selector
            app.source_cursor = 0;
            app.mode = AppMode::SourceSelect;
        }
        KeyCode::Char('d') => {
            // Open downloads viewer
            app.download_cursor = 0;
            app.mode = AppMode::Downloads;
        }
        KeyCode::Char('/') | KeyCode::Esc => {
            // Back to search
            app.mode = AppMode::Search;
        }
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        _ => {}
    }
}

async fn handle_file_select_keys(
    app: &mut App,
    code: KeyCode,
    tx: mpsc::UnboundedSender<AppMessage>,
) {
    let visible_height = app.visible_height();

    match code {
        KeyCode::Up | KeyCode::Char('k') => {
            if app.file_cursor > 0 {
                app.file_cursor -= 1;
                if app.file_cursor < app.file_scroll_offset {
                    app.file_scroll_offset = app.file_cursor;
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.file_cursor < app.files.len().saturating_sub(1) {
                app.file_cursor += 1;
                if app.file_cursor >= app.file_scroll_offset + visible_height {
                    app.file_scroll_offset = app.file_cursor - visible_height + 1;
                }
            }
        }
        KeyCode::Char(' ') => {
            // Toggle file selection
            if let Some(file) = app.files.get(app.file_cursor) {
                if app.selected_files.contains(&file.id) {
                    app.selected_files.remove(&file.id);
                } else {
                    app.selected_files.insert(file.id);
                }
            }
        }
        KeyCode::Char('a') => {
            // Toggle all
            if app.selected_files.len() == app.files.len() {
                app.selected_files.clear();
            } else {
                app.selected_files = app.files.iter().map(|f| f.id).collect();
            }
        }
        KeyCode::Enter => {
            // Confirm selection and get download links
            if !app.selected_files.is_empty() {
                if let (Some(rd_client), Some(torrent_id)) = (&app.rd_client, &app.torrent_id) {
                    let rd_client = rd_client.clone();
                    let torrent_id = torrent_id.clone();
                    let file_ids: Vec<u32> = app.selected_files.iter().copied().collect();
                    let tx = tx.clone();

                    app.mode = AppMode::Processing;
                    app.processing_status = "Getting download links...".to_string();

                    tokio::spawn(async move {
                        let tx_clone = tx.clone();
                        let result = rd_client.download_selected_files_with_callback(
                            &torrent_id,
                            &file_ids,
                            |status| {
                                let _ = tx_clone.send(AppMessage::StatusUpdate(status.to_string()));
                            }
                        ).await;

                        match result {
                            Ok(links) => {
                                let _ = tx.send(AppMessage::DownloadLinks(links));
                            }
                            Err(e) => {
                                let _ = tx.send(AppMessage::DownloadError(e.to_string()));
                            }
                        }
                    });
                }
            } else {
                app.status = "No files selected".to_string();
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            // Cancel and go back to results
            // Clean up torrent from RD
            if let (Some(rd_client), Some(torrent_id)) = (&app.rd_client, &app.torrent_id) {
                let rd_client = rd_client.clone();
                let torrent_id = torrent_id.clone();
                tokio::spawn(async move {
                    let _ = rd_client.delete_torrent(&torrent_id).await;
                });
            }
            app.torrent_id = None;
            app.files.clear();
            app.selected_files.clear();
            app.mode = AppMode::Results;
        }
        _ => {}
    }
}

fn handle_message(app: &mut App, msg: AppMessage) {
    match msg {
        AppMessage::SearchResults(results) => {
            app.results = results;
            app.selected_index = 0;
            app.scroll_offset = 0;
            app.status = format!("{} results found", app.results.len());
            app.mode = AppMode::Results;
        }
        AppMessage::SearchError(e) => {
            app.status = format!("Search error: {}", e);
            app.mode = AppMode::Error(e);
        }
        AppMessage::TorrentFiles(torrent_id, files) => {
            app.torrent_id = Some(torrent_id);

            // Filter to useful files (video/archive or >50MB)
            let video_exts = ["mkv", "mp4", "avi", "mov", "wmv", "flv", "webm", "m4v"];
            let archive_exts = ["rar", "zip", "7z", "tar", "gz"];

            let useful_files: Vec<_> = files.iter().filter(|f| {
                let name_lower = f.name().to_lowercase();

                // Check if video file
                let is_video = video_exts.iter().any(|ext| name_lower.ends_with(ext));

                // Check if archive file
                let is_archive = archive_exts.iter().any(|ext| name_lower.ends_with(ext));

                // Check if large file (>50MB)
                let is_large = f.bytes > 50_000_000;

                is_video || is_archive || is_large
            }).cloned().collect();

            // Use filtered files if any, otherwise use all
            app.files = if useful_files.is_empty() {
                files
            } else {
                useful_files
            };

            app.file_cursor = 0;
            app.file_scroll_offset = 0;
            app.selected_files.clear();

            // Auto-select if single file
            if app.files.len() == 1 {
                app.selected_files.insert(app.files[0].id);
            }

            app.status = format!("{} files in torrent", app.files.len());
            app.mode = AppMode::FileSelect;
        }
        AppMessage::TorrentError(e) => {
            app.status = format!("Torrent error: {}", e);
            app.mode = AppMode::Error(e);
        }
        AppMessage::DownloadLinks(links) => {
            // Add downloads to the download list
            let downloads_dir = std::env::var("DOWNLOAD_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| dirs::download_dir().unwrap_or_else(|| PathBuf::from(".")));

            for (filename, url) in links {
                let dest_path = downloads_dir.join(&filename);
                let download = Download {
                    url: url.clone(),
                    filename: filename.clone(),
                    dest_path,
                    status: DownloadStatus::Pending,
                    total_bytes: 0,
                    downloaded_bytes: 0,
                    speed: 0.0,
                };
                app.downloads.push(download);
            }

            app.status = format!("{} download(s) queued! Press 'd' to view", app.downloads.len());

            // Print links to console (they'll be visible after exit)
            for dl in &app.downloads {
                eprintln!("\n{}", dl.filename);
                eprintln!("{}", dl.url);
            }

            app.mode = AppMode::Results;
        }
        AppMessage::DownloadError(e) => {
            app.status = format!("Download error: {}", e);
            app.mode = AppMode::Error(e);
        }
        AppMessage::StatusUpdate(s) => {
            app.processing_status = s;
        }
        AppMessage::DownloadProgress { index, downloaded, total, speed } => {
            if let Some(dl) = app.downloads.get_mut(index) {
                dl.downloaded_bytes = downloaded;
                dl.total_bytes = total;
                dl.speed = speed;
                dl.status = DownloadStatus::Downloading;
            }
        }
        AppMessage::DownloadComplete(index) => {
            if let Some(dl) = app.downloads.get_mut(index) {
                dl.status = DownloadStatus::Completed;
            }
        }
        AppMessage::DownloadFailed(index, error) => {
            if let Some(dl) = app.downloads.get_mut(index) {
                dl.status = DownloadStatus::Failed(error);
            }
        }
    }
}

/// Handle source selector keys
fn handle_source_select_keys(app: &mut App, code: KeyCode) {
    let num_sources = scrapers::SCRAPERS.len();

    match code {
        KeyCode::Up | KeyCode::Char('k') => {
            if app.source_cursor > 0 {
                app.source_cursor -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.source_cursor < num_sources.saturating_sub(1) {
                app.source_cursor += 1;
            }
        }
        KeyCode::Char(' ') => {
            // Toggle source
            let source = scrapers::SCRAPERS[app.source_cursor].to_string();
            if app.enabled_sources.contains(&source) {
                app.enabled_sources.remove(&source);
            } else {
                app.enabled_sources.insert(source);
            }
        }
        KeyCode::Char('a') => {
            // Enable all
            app.enabled_sources = scrapers::SCRAPERS.iter().map(|s| s.to_string()).collect();
        }
        KeyCode::Char('n') => {
            // Disable all
            app.enabled_sources.clear();
        }
        KeyCode::Enter => {
            // Confirm and go back
            if !app.enabled_sources.is_empty() {
                app.status = format!("{} sources enabled", app.enabled_sources.len());
                app.mode = AppMode::Search;
            } else {
                app.status = "At least one source must be enabled".to_string();
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            // Cancel
            app.mode = AppMode::Search;
        }
        _ => {}
    }
}

/// Handle downloads viewer keys
async fn handle_downloads_keys(
    app: &mut App,
    code: KeyCode,
    tx: mpsc::UnboundedSender<AppMessage>,
) {
    let num_downloads = app.downloads.len();

    match code {
        KeyCode::Up | KeyCode::Char('k') => {
            if app.download_cursor > 0 {
                app.download_cursor -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.download_cursor < num_downloads.saturating_sub(1) {
                app.download_cursor += 1;
            }
        }
        KeyCode::Char('s') => {
            // Start selected pending download
            if let Some(dl) = app.downloads.get_mut(app.download_cursor) {
                if dl.status == DownloadStatus::Pending {
                    dl.status = DownloadStatus::Downloading;
                    let url = dl.url.clone();
                    let dest_path = dl.dest_path.clone();
                    let index = app.download_cursor;
                    let tx = tx.clone();

                    tokio::spawn(async move {
                        start_download(url, dest_path, index, tx).await;
                    });
                }
            }
        }
        KeyCode::Char('S') => {
            // Start all pending downloads
            for (index, dl) in app.downloads.iter_mut().enumerate() {
                if dl.status == DownloadStatus::Pending {
                    dl.status = DownloadStatus::Downloading;
                    let url = dl.url.clone();
                    let dest_path = dl.dest_path.clone();
                    let tx = tx.clone();

                    tokio::spawn(async move {
                        start_download(url, dest_path, index, tx).await;
                    });
                }
            }
        }
        KeyCode::Char('c') => {
            // Cancel selected download
            if let Some(dl) = app.downloads.get_mut(app.download_cursor) {
                if dl.status == DownloadStatus::Downloading || dl.status == DownloadStatus::Pending {
                    dl.status = DownloadStatus::Cancelled;
                }
            }
        }
        KeyCode::Char('C') => {
            // Cancel all active downloads
            for dl in &mut app.downloads {
                if dl.status == DownloadStatus::Downloading || dl.status == DownloadStatus::Pending {
                    dl.status = DownloadStatus::Cancelled;
                }
            }
        }
        KeyCode::Char('x') => {
            // Clear completed/failed/cancelled
            app.downloads.retain(|dl| {
                matches!(dl.status, DownloadStatus::Downloading | DownloadStatus::Pending)
            });
            if app.download_cursor >= app.downloads.len() {
                app.download_cursor = app.downloads.len().saturating_sub(1);
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            // Back to search or results
            if app.results.is_empty() {
                app.mode = AppMode::Search;
            } else {
                app.mode = AppMode::Results;
            }
        }
        _ => {}
    }
}

/// Start downloading a file in the background
async fn start_download(
    url: String,
    dest_path: PathBuf,
    index: usize,
    tx: mpsc::UnboundedSender<AppMessage>,
) {
    use futures::StreamExt;
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::new();

    // Start the download
    let response = match client.get(&url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            let _ = tx.send(AppMessage::DownloadFailed(index, e.to_string()));
            return;
        }
    };

    let total_size = response.content_length().unwrap_or(0);

    // Create the file
    let mut file = match tokio::fs::File::create(&dest_path).await {
        Ok(f) => f,
        Err(e) => {
            let _ = tx.send(AppMessage::DownloadFailed(index, e.to_string()));
            return;
        }
    };

    let mut downloaded: u64 = 0;
    let mut last_update = std::time::Instant::now();
    let mut last_downloaded: u64 = 0;

    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                // Write chunk to file
                if let Err(e) = file.write_all(&chunk).await {
                    let _ = tx.send(AppMessage::DownloadFailed(index, e.to_string()));
                    return;
                }

                downloaded += chunk.len() as u64;

                // Report progress every 100ms
                let now = std::time::Instant::now();
                if now.duration_since(last_update).as_millis() >= 100 {
                    let elapsed = now.duration_since(last_update).as_secs_f64();
                    let speed = (downloaded - last_downloaded) as f64 / elapsed;

                    let _ = tx.send(AppMessage::DownloadProgress {
                        index,
                        downloaded,
                        total: total_size,
                        speed,
                    });

                    last_update = now;
                    last_downloaded = downloaded;
                }
            }
            Err(e) => {
                let _ = tx.send(AppMessage::DownloadFailed(index, e.to_string()));
                return;
            }
        }
    }

    // Final sync
    if let Err(e) = file.sync_all().await {
        let _ = tx.send(AppMessage::DownloadFailed(index, e.to_string()));
        return;
    }

    let _ = tx.send(AppMessage::DownloadComplete(index));
}
