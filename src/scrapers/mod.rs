//! Torrent scrapers for various sites

pub mod x1337;
pub mod tpb;
pub mod bitsearch;
pub mod yts;
pub mod ilcorsaronero;
pub mod log;

use anyhow::Result;
use reqwest::Client;
use std::time::Duration;

pub use log::{init_log, log_error, log_info};

pub use x1337::scrape_1337x;
pub use tpb::scrape_tpb;
pub use bitsearch::scrape_bitsearch;
pub use yts::scrape_yts;
pub use ilcorsaronero::scrape_ilcorsaronero;

/// Torrent search result
#[derive(Debug, Clone)]
pub struct TorrentResult {
    pub name: String,
    pub size: String,
    pub seeders: i64,
    pub leechers: i64,
    pub magnet: String,
    pub source: String,
    pub url: Option<String>,
    pub category: Option<String>,
}

impl TorrentResult {
    pub fn seeders_str(&self) -> String {
        self.seeders.to_string()
    }

    pub fn size_str(&self) -> String {
        self.size.clone()
    }

    pub fn source_str(&self) -> String {
        self.source.clone()
    }
}

/// HTTP client with standard headers
pub fn create_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(Into::into)
}

/// Fetch URL and return HTML
pub async fn fetch(client: &Client, url: &str) -> Option<String> {
    client
        .get(url)
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()
}

/// Clean and trim text
pub fn clean_text(text: &str) -> String {
    text.trim().to_string()
}

/// Available scrapers
pub const SCRAPERS: &[&str] = &["1337x", "tpb", "bitsearch", "yts", "ilcorsaronero"];

/// Search all sites in parallel
pub async fn search_all(query: &str, page: u32) -> Vec<TorrentResult> {
    let client = match create_client() {
        Ok(c) => c,
        Err(e) => {
            log_error("client", &format!("Failed to create HTTP client: {}", e));
            return Vec::new();
        }
    };

    log_info("search", &format!("Searching for '{}' (page {})", query, page));

    // Run all scrapers in parallel
    let (r1337x, rtpb, rbitsearch, ryts, rilcorsaronero) = tokio::join!(
        scrape_1337x(&client, query, page),
        scrape_tpb(&client, query, page),
        scrape_bitsearch(&client, query, page),
        scrape_yts(&client, query, page),
        scrape_ilcorsaronero(&client, query, page),
    );

    let mut results = Vec::new();

    // Collect results with logging
    match r1337x {
        Some(ref r) if !r.is_empty() => {
            log_info("1337x", &format!("Found {} results", r.len()));
            results.extend(r.clone());
        }
        Some(_) => log_info("1337x", "No results found"),
        None => log_error("1337x", "Scraper failed (returned None)"),
    }

    match rtpb {
        Some(ref r) if !r.is_empty() => {
            log_info("tpb", &format!("Found {} results", r.len()));
            results.extend(r.clone());
        }
        Some(_) => log_info("tpb", "No results found"),
        None => log_error("tpb", "Scraper failed (returned None)"),
    }

    match rbitsearch {
        Some(ref r) if !r.is_empty() => {
            log_info("bitsearch", &format!("Found {} results", r.len()));
            results.extend(r.clone());
        }
        Some(_) => log_info("bitsearch", "No results found"),
        None => log_error("bitsearch", "Scraper failed (returned None)"),
    }

    match ryts {
        Some(ref r) if !r.is_empty() => {
            log_info("yts", &format!("Found {} results", r.len()));
            results.extend(r.clone());
        }
        Some(_) => log_info("yts", "No results found"),
        None => log_error("yts", "Scraper failed (returned None)"),
    }

    match rilcorsaronero {
        Some(ref r) if !r.is_empty() => {
            log_info("ilcorsaronero", &format!("Found {} results", r.len()));
            results.extend(r.clone());
        }
        Some(_) => log_info("ilcorsaronero", "No results found"),
        None => log_error("ilcorsaronero", "Scraper failed (returned None)"),
    }

    log_info("search", &format!("Total: {} results from all sources", results.len()));

    // Sort by seeders (descending)
    results.sort_by(|a, b| b.seeders.cmp(&a.seeders));

    results
}
