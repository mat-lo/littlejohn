//! UI rendering for littlejohn

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Row, Table, Wrap},
};

use crate::{App, AppMode, DownloadStatus, SettingsField, format_bytes, scrapers};

/// Main draw function
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Create main layout: header, content, footer
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Status bar
        ])
        .split(area);

    draw_header(frame, app, layout[0]);

    match &app.mode {
        AppMode::Setup => draw_setup(frame, app, layout[1]),
        AppMode::Settings => draw_settings(frame, app, layout[1]),
        AppMode::Search => draw_search(frame, app, layout[1]),
        AppMode::Results => draw_results(frame, app, layout[1]),
        AppMode::FileSelect => draw_file_select(frame, app, layout[1]),
        AppMode::SourceSelect => draw_source_select(frame, app, layout[1]),
        AppMode::Downloads => draw_downloads(frame, app, layout[1]),
        AppMode::Processing => draw_processing(frame, app, layout[1]),
        AppMode::Error(msg) => draw_error(frame, msg, layout[1]),
    }

    draw_status_bar(frame, app, layout[2]);
}

fn draw_header(frame: &mut Frame, _app: &App, area: Rect) {
    let title = Paragraph::new("LITTLEJOHN - Torrent Search with Real-Debrid")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));

    frame.render_widget(title, area);
}

fn draw_setup(frame: &mut Frame, app: &App, area: Rect) {
    draw_settings_form(frame, app, area, true);
}

fn draw_settings(frame: &mut Frame, app: &App, area: Rect) {
    draw_settings_form(frame, app, area, false);
}

fn draw_settings_form(frame: &mut Frame, app: &App, area: Rect, is_setup: bool) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title/Instructions
            Constraint::Length(3),  // RD Token field
            Constraint::Length(3),  // Firecrawl field
            Constraint::Length(3),  // Download Dir field
            Constraint::Min(0),     // Help text
        ])
        .margin(1)
        .split(area);

    // Title and instructions
    let title = if is_setup {
        "Welcome! Please configure your settings to get started."
    } else {
        "Settings - Edit your configuration"
    };
    let title_widget = Paragraph::new(title)
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center);
    frame.render_widget(title_widget, layout[0]);

    // Helper to draw a field
    let draw_field = |frame: &mut Frame, area: Rect, label: &str, value: &str, is_active: bool, is_secret: bool, cursor_pos: usize| {
        let display_value = if is_secret && !value.is_empty() {
            if is_active {
                value.to_string()
            } else {
                "*".repeat(value.len().min(20))
            }
        } else {
            value.to_string()
        };

        let style = if is_active {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::Gray)
        };

        let border_style = if is_active {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let field = Paragraph::new(display_value)
            .style(style)
            .block(
                Block::default()
                    .title(label)
                    .borders(Borders::ALL)
                    .border_style(border_style)
            );

        frame.render_widget(field, area);

        // Draw cursor if active
        if is_active {
            frame.set_cursor_position((
                area.x + 1 + cursor_pos as u16,
                area.y + 1,
            ));
        }
    };

    // RD Token field
    let is_rd_active = app.settings_field == SettingsField::RdApiToken;
    draw_field(
        frame,
        layout[1],
        "Real-Debrid API Token (required)",
        &app.settings_rd_token,
        is_rd_active,
        true,
        if is_rd_active { app.settings_cursor } else { 0 },
    );

    // Firecrawl field
    let is_fc_active = app.settings_field == SettingsField::FirecrawlApiKey;
    draw_field(
        frame,
        layout[2],
        "Firecrawl API Key (optional)",
        &app.settings_firecrawl_key,
        is_fc_active,
        true,
        if is_fc_active { app.settings_cursor } else { 0 },
    );

    // Download Dir field
    let is_dd_active = app.settings_field == SettingsField::DownloadDir;
    draw_field(
        frame,
        layout[3],
        "Download Directory (optional, defaults to ~/Downloads)",
        &app.settings_download_dir,
        is_dd_active,
        false,
        if is_dd_active { app.settings_cursor } else { 0 },
    );

    // Help text
    let help = if is_setup {
        vec![
            "",
            "Tab/Down: Next field   |   Shift+Tab/Up: Previous field",
            "Enter: Save and continue   |   Esc: Skip setup",
            "",
            "Get your Real-Debrid token from: https://real-debrid.com/apitoken",
            "Get your Firecrawl key from: https://firecrawl.dev (optional)",
        ]
    } else {
        vec![
            "",
            "Tab/Down: Next field   |   Shift+Tab/Up: Previous field",
            "Enter: Save   |   Esc: Cancel",
        ]
    };

    let help_text = help.join("\n");
    let help_widget = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help_widget, layout[4]);
}

fn draw_search(frame: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Search input
            Constraint::Min(0),     // Instructions
        ])
        .margin(1)
        .split(area);

    // Search input
    let input = Paragraph::new(app.search_input.as_str())
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .title("Search (or paste magnet link)")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
        );

    frame.render_widget(input, layout[0]);

    // Set cursor position
    frame.set_cursor_position((
        layout[0].x + 1 + app.cursor_pos as u16,
        layout[0].y + 1,
    ));

    // Build sources list
    let enabled_count = app.enabled_sources.len();
    let total_count = scrapers::SCRAPERS.len();
    let sources_str: Vec<&str> = scrapers::SCRAPERS
        .iter()
        .filter(|s| app.enabled_sources.contains(&s.to_string()))
        .copied()
        .collect();

    // Show downloads indicator
    let active_downloads = app.downloads.iter()
        .filter(|d| matches!(d.status, DownloadStatus::Downloading | DownloadStatus::Pending))
        .count();
    let downloads_line = if active_downloads > 0 {
        format!("\n  {} download(s) in progress - press 'd' to view", active_downloads)
    } else {
        String::new()
    };

    // Instructions
    let instructions = format!(
        r#"
Enter a search query to find torrents across multiple sites.
You can also paste a magnet link directly.

Enabled sources ({}/{}): {}
{}
Controls:
  [Enter]     Search / Process magnet
  [s]         Select sources
  [d]         View downloads
  [Esc]       Quit
"#,
        enabled_count,
        total_count,
        sources_str.join(", "),
        downloads_line,
    );

    let help = Paragraph::new(instructions)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::NONE));

    frame.render_widget(help, layout[1]);
}

fn draw_results(frame: &mut Frame, app: &App, area: Rect) {
    // Check for active downloads
    let active_downloads = app.downloads.iter()
        .filter(|d| matches!(d.status, DownloadStatus::Downloading | DownloadStatus::Pending))
        .count();

    // Adjust visible height if showing downloads indicator
    let has_downloads = active_downloads > 0;
    let visible_height = if has_downloads {
        area.height.saturating_sub(6) as usize
    } else {
        area.height.saturating_sub(4) as usize
    };

    // Create table rows
    let rows: Vec<Row> = app
        .results
        .iter()
        .skip(app.scroll_offset)
        .take(visible_height)
        .enumerate()
        .map(|(i, result)| {
            let actual_idx = app.scroll_offset + i;
            let is_selected = actual_idx == app.selected_index;

            let name = truncate(&result.name, 50);
            let size = truncate(&result.size_str(), 10);
            let seeds = result.seeders_str();
            let source = truncate(&result.source_str(), 12);

            let style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if is_selected { "> " } else { "  " };

            Row::new(vec![
                format!("{}{:3}", prefix, actual_idx + 1),
                name,
                size,
                seeds,
                source,
            ])
            .style(style)
        })
        .collect();

    let header = Row::new(vec!["  #", "Name", "Size", "Seeds", "Source"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .bottom_margin(1);

    // Build title with downloads indicator
    let title = if has_downloads {
        format!(
            "Results - Page {} ({} total) | {} downloads active",
            app.page,
            app.results.len(),
            active_downloads
        )
    } else {
        format!(
            "Results - Page {} ({} total)",
            app.page,
            app.results.len()
        )
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Min(30),
            Constraint::Length(12),
            Constraint::Length(7),
            Constraint::Length(14),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green)),
    );

    frame.render_widget(table, area);
}

fn draw_file_select(frame: &mut Frame, app: &App, area: Rect) {
    let visible_height = area.height.saturating_sub(6) as usize;

    // Create list items
    let items: Vec<ListItem> = app
        .files
        .iter()
        .skip(app.file_scroll_offset)
        .take(visible_height)
        .enumerate()
        .map(|(i, file)| {
            let actual_idx = app.file_scroll_offset + i;
            let is_cursor = actual_idx == app.file_cursor;
            let is_selected = app.selected_files.contains(&file.id);

            let checkbox = if is_selected { "[x]" } else { "[ ]" };
            let prefix = if is_cursor { "> " } else { "  " };

            let text = format!(
                "{}{} {} ({})",
                prefix,
                checkbox,
                truncate(file.name(), 50),
                file.size_str()
            );

            let style = if is_cursor {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if is_selected {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(text).style(style)
        })
        .collect();

    let title = if let Some(result) = app.results.get(app.selected_index) {
        format!("Select Files - {} ({} files)", truncate(&result.name, 40), app.files.len())
    } else {
        format!("Select Files ({} files)", app.files.len())
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );

    frame.render_widget(list, area);
}

fn draw_processing(frame: &mut Frame, app: &App, area: Rect) {
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 7.min(area.height.saturating_sub(4));

    let popup_area = Rect::new(
        (area.width - popup_width) / 2,
        (area.height - popup_height) / 2,
        popup_width,
        popup_height,
    );

    frame.render_widget(Clear, popup_area);

    let spinner_frames = ["[    ]", "[=   ]", "[==  ]", "[=== ]", "[ ===]", "[  ==]", "[   =]", "[    ]"];
    let frame_idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() / 100) as usize % spinner_frames.len();

    let text = format!(
        "\n{}\n\n{}",
        spinner_frames[frame_idx],
        app.processing_status
    );

    let processing = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .title("Processing")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );

    frame.render_widget(processing, popup_area);
}

fn draw_error(frame: &mut Frame, message: &str, area: Rect) {
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 9.min(area.height.saturating_sub(4));

    let popup_area = Rect::new(
        (area.width - popup_width) / 2,
        (area.height - popup_height) / 2,
        popup_width,
        popup_height,
    );

    frame.render_widget(Clear, popup_area);

    let text = format!("\n{}\n\n\nPress any key to continue...", message);

    let error = Paragraph::new(text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Red))
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .title("Error")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        );

    frame.render_widget(error, popup_area);
}

fn draw_source_select(frame: &mut Frame, app: &App, area: Rect) {
    // Create list items for each source
    let items: Vec<ListItem> = scrapers::SCRAPERS
        .iter()
        .enumerate()
        .map(|(i, source)| {
            let is_cursor = i == app.source_cursor;
            let is_enabled = app.enabled_sources.contains(&source.to_string());

            let checkbox = if is_enabled { "[x]" } else { "[ ]" };
            let prefix = if is_cursor { "> " } else { "  " };

            let text = format!("{}{} {}", prefix, checkbox, source);

            let style = if is_cursor {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if is_enabled {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Gray)
            };

            ListItem::new(text).style(style)
        })
        .collect();

    let title = format!(
        "Select Sources ({}/{} enabled)",
        app.enabled_sources.len(),
        scrapers::SCRAPERS.len()
    );

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        );

    frame.render_widget(list, area);
}

fn draw_downloads(frame: &mut Frame, app: &App, area: Rect) {
    if app.downloads.is_empty() {
        // Show empty state
        let text = Paragraph::new("\n\nNo downloads yet.\n\nStart by searching and selecting a torrent.")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray))
            .block(
                Block::default()
                    .title("Downloads")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            );

        frame.render_widget(text, area);
        return;
    }

    let visible_height = area.height.saturating_sub(4) as usize;

    // Create table rows
    let rows: Vec<Row> = app
        .downloads
        .iter()
        .take(visible_height)
        .enumerate()
        .map(|(i, dl)| {
            let is_selected = i == app.download_cursor;

            let (status_str, status_style) = match &dl.status {
                DownloadStatus::Pending => ("Wait", Style::default().fg(Color::Gray)),
                DownloadStatus::Downloading => ("Down", Style::default().fg(Color::Yellow)),
                DownloadStatus::Completed => ("Done", Style::default().fg(Color::Green)),
                DownloadStatus::Failed(_) => ("Fail", Style::default().fg(Color::Red)),
                DownloadStatus::Cancelled => ("Stop", Style::default().fg(Color::Magenta)),
            };

            let progress = if dl.total_bytes > 0 {
                format!("{:.1}%", dl.progress())
            } else {
                format_bytes(dl.downloaded_bytes as f64)
            };

            let speed = if dl.status == DownloadStatus::Downloading && dl.speed > 0.0 {
                dl.speed_str()
            } else {
                "-".to_string()
            };

            let style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                status_style
            };

            let prefix = if is_selected { "> " } else { "  " };

            Row::new(vec![
                format!("{}{:2}", prefix, i + 1),
                status_str.to_string(),
                truncate(&dl.filename, 40),
                progress,
                speed,
            ])
            .style(style)
        })
        .collect();

    let header = Row::new(vec!["  #", "Status", "Name", "Progress", "Speed"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .bottom_margin(1);

    let active = app.downloads.iter()
        .filter(|d| matches!(d.status, DownloadStatus::Downloading | DownloadStatus::Pending))
        .count();

    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Length(6),
            Constraint::Min(20),
            Constraint::Length(12),
            Constraint::Length(12),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!("Downloads ({} active)", active))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)),
    );

    frame.render_widget(table, area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let help_text = match app.mode {
        AppMode::Setup => "[Tab] Next  [Enter] Save  [Esc] Skip",
        AppMode::Settings => "[Tab] Next  [Enter] Save  [Esc] Cancel",
        AppMode::Search => "[Enter] Search  [s] Sources  [S] Settings  [d] Downloads  [Esc] Quit",
        AppMode::Results => "[j/k] Nav  [Enter] Select  [c] Copy  [s] Sources  [d] Downloads  [n/p] Page  [/] Search  [q] Quit",
        AppMode::FileSelect => "[j/k] Navigate  [Space] Toggle  [a] All  [Enter] Confirm  [Esc] Back",
        AppMode::SourceSelect => "[j/k] Navigate  [Space] Toggle  [a] All  [n] None  [Enter] Confirm  [Esc] Back",
        AppMode::Downloads => "[j/k] Nav  [s] Start  [S] Start All  [c] Cancel  [C] Cancel All  [x] Clear  [Esc] Back",
        AppMode::Processing => "[Esc] Cancel",
        AppMode::Error(_) => "Press any key...",
    };

    let status_text = if app.status.is_empty() {
        help_text.to_string()
    } else {
        format!("{} | {}", app.status, help_text)
    };

    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Gray))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

    frame.render_widget(status, area);
}

/// Truncate string with ellipsis (UTF-8 safe)
fn truncate(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else if max_len > 3 {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        s.chars().take(max_len).collect()
    }
}
