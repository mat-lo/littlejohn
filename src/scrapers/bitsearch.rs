//! BitSearch scraper

use super::{clean_text, log_error, log_info, TorrentResult};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;

/// Firecrawl scrape request
#[derive(Serialize)]
struct FirecrawlRequest {
    url: String,
    formats: Vec<String>,
}

/// Fetch URL using Firecrawl API (for bypassing Cloudflare)
async fn fetch_with_firecrawl(client: &Client, url: &str) -> Option<String> {
    let api_key = match std::env::var("FIRECRAWL_API_KEY") {
        Ok(key) if !key.is_empty() => key,
        _ => return None,
    };

    let request = FirecrawlRequest {
        url: url.to_string(),
        formats: vec!["html".to_string()],
    };

    let response = match client
        .post("https://api.firecrawl.dev/v1/scrape")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log_error("bitsearch", &format!("Firecrawl request failed: {}", e));
            return None;
        }
    };

    let data: serde_json::Value = match response.json().await {
        Ok(d) => d,
        Err(e) => {
            log_error("bitsearch", &format!("Firecrawl response parse error: {}", e));
            return None;
        }
    };

    data.get("data")
        .and_then(|d| d.get("html"))
        .and_then(|h| h.as_str())
        .map(String::from)
}

/// Fetch URL with Firecrawl fallback to direct fetch
async fn fetch_with_fallback(client: &Client, url: &str) -> Option<String> {
    // Try Firecrawl first (needed for Cloudflare bypass)
    if let Some(html) = fetch_with_firecrawl(client, url).await {
        if !html.is_empty() && (html.contains("search-result") || html.contains("card")) {
            log_info("bitsearch", "Using Firecrawl");
            return Some(html);
        }
    }

    // Fall back to direct fetch
    log_info("bitsearch", "Trying direct fetch");
    match client.get(url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if !status.is_success() {
                log_error("bitsearch", &format!("HTTP {} for {}", status, url));
                return None;
            }
            match resp.text().await {
                Ok(text) => {
                    // Check for Cloudflare challenge
                    if text.contains("Just a moment") || text.contains("Enable JavaScript") {
                        log_error("bitsearch", "Cloudflare challenge detected - set FIRECRAWL_API_KEY to bypass");
                        return None;
                    }
                    Some(text)
                }
                Err(e) => {
                    log_error("bitsearch", &format!("Failed to read body: {}", e));
                    None
                }
            }
        }
        Err(e) => {
            log_error("bitsearch", &format!("Request failed: {}", e));
            None
        }
    }
}

/// Scrape BitSearch for torrents
pub async fn scrape_bitsearch(client: &Client, query: &str, page: u32) -> Option<Vec<TorrentResult>> {
    let encoded = urlencoding::encode(query);
    let url = format!(
        "https://bitsearch.to/search?q={}&page={}&sort=seeders",
        encoded, page
    );

    log_info("bitsearch", &format!("Fetching: {}", url));
    let html = fetch_with_fallback(client, &url).await?;
    let document = Html::parse_document(&html);

    // Find all magnet links first (like the working Python version)
    let magnet_sel = Selector::parse("a[href^='magnet:']").ok()?;
    let title_selectors = [
        "h5 a",
        "a.text-sky-600",
        "a[href*='/torrent/']",
    ];
    let green_sel = Selector::parse("span.text-green-600, span.text-emerald-600").ok();
    let red_sel = Selector::parse("span.text-red-600, span.text-rose-600").ok();
    let size_re = Regex::new(r"([\d.]+\s*(?:GB|MB|KB|TB|GiB|MiB))").ok()?;

    let mut results = Vec::new();
    let mut seen_magnets = std::collections::HashSet::new();

    // Iterate through all magnet links and work backwards to find containers
    for magnet_el in document.select(&magnet_sel) {
        let magnet_raw = magnet_el.value().attr("href").unwrap_or("");
        if magnet_raw.is_empty() {
            continue;
        }

        // Decode HTML entities in magnet
        let magnet = magnet_raw
            .replace("&amp;", "&")
            .replace("&#x3D;", "=")
            .replace("&#x3A;", ":");

        // Skip duplicates
        if seen_magnets.contains(&magnet) {
            continue;
        }
        seen_magnets.insert(magnet.clone());

        // Find the parent card container (div.bg-white or similar)
        // We'll search ancestors of the magnet link element
        let mut card = None;
        let mut current = magnet_el.parent();
        while let Some(parent) = current {
            if let Some(parent_el) = parent.value().as_element() {
                let classes = parent_el.attr("class").unwrap_or("");
                if classes.contains("bg-white") || classes.contains("card") {
                    card = Some(scraper::ElementRef::wrap(parent).unwrap());
                    break;
                }
            }
            current = parent.parent();
        }

        let card = match card {
            Some(c) => c,
            None => continue,
        };

        // Find title using multiple selectors
        let mut name = String::new();
        let mut detail_url = String::new();
        for sel_str in &title_selectors {
            if let Ok(sel) = Selector::parse(sel_str) {
                if let Some(title_el) = card.select(&sel).next() {
                    let href = title_el.value().attr("href").unwrap_or("");
                    // Skip if this is the magnet link itself
                    if href.starts_with("magnet:") {
                        continue;
                    }
                    name = clean_text(&title_el.text().collect::<String>());
                    if !href.is_empty() && href.contains("/torrent/") {
                        detail_url = if href.starts_with("http") {
                            href.to_string()
                        } else {
                            format!("https://bitsearch.to{}", href)
                        };
                    }
                    if !name.is_empty() {
                        break;
                    }
                }
            }
        }

        if name.is_empty() {
            continue;
        }

        // Extract size from card text
        let card_text: String = card.text().collect();
        let size = size_re
            .find(&card_text)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();

        // Extract seeders (green text)
        let seeders: i64 = green_sel
            .as_ref()
            .and_then(|sel| card.select(sel).next())
            .map(|el| {
                let text = el.text().collect::<String>();
                text.trim().replace(",", "").parse().unwrap_or(0)
            })
            .unwrap_or(0);

        // Extract leechers (red text)
        let leechers: i64 = red_sel
            .as_ref()
            .and_then(|sel| card.select(sel).next())
            .map(|el| {
                let text = el.text().collect::<String>();
                text.trim().replace(",", "").parse().unwrap_or(0)
            })
            .unwrap_or(0);

        results.push(TorrentResult {
            name,
            size,
            seeders,
            leechers,
            magnet,
            source: "bitsearch".to_string(),
            url: if detail_url.is_empty() { None } else { Some(detail_url) },
            category: None,
        });
    }

    if results.is_empty() {
        log_info("bitsearch", "No results found");
    } else {
        log_info("bitsearch", &format!("Found {} results", results.len()));
    }

    Some(results)
}
