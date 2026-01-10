//! 1337x scraper

use super::{clean_text, log_error, log_info, TorrentResult};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;

const BASE_URL: &str = "https://www.1337xx.to";

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
            log_error("1337x", &format!("Firecrawl request failed: {}", e));
            return None;
        }
    };

    let data: serde_json::Value = match response.json().await {
        Ok(d) => d,
        Err(e) => {
            log_error("1337x", &format!("Firecrawl response parse error: {}", e));
            return None;
        }
    };

    data.get("data")
        .and_then(|d| d.get("html"))
        .and_then(|h| h.as_str())
        .map(String::from)
}

/// Fetch URL with Firecrawl fallback to direct fetch
async fn fetch_with_fallback(client: &Client, url: &str, context: &str) -> Option<String> {
    // Try Firecrawl first (needed for Cloudflare bypass)
    if let Some(html) = fetch_with_firecrawl(client, url).await {
        // Basic validation - check we got actual HTML content
        if !html.is_empty() && (html.contains("1337x") || html.contains("magnet:") || html.contains("torrent")) {
            log_info("1337x", &format!("{}: Using Firecrawl", context));
            return Some(html);
        }
    }

    // Fall back to direct fetch
    log_info("1337x", &format!("{}: Trying direct fetch", context));
    match client.get(url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if !status.is_success() {
                log_error("1337x", &format!("{}: HTTP {} for {}", context, status, url));
                return None;
            }
            match resp.text().await {
                Ok(text) => {
                    // Check for Cloudflare challenge
                    if text.contains("Just a moment") || text.contains("Enable JavaScript") {
                        log_error("1337x", &format!("{}: Cloudflare challenge detected - set FIRECRAWL_API_KEY to bypass", context));
                        return None;
                    }
                    Some(text)
                }
                Err(e) => {
                    log_error("1337x", &format!("{}: Failed to read body: {}", context, e));
                    None
                }
            }
        }
        Err(e) => {
            log_error("1337x", &format!("{}: Request failed: {}", context, e));
            None
        }
    }
}

/// Fetch magnet link from detail page
async fn fetch_detail(client: &Client, url: &str) -> Option<String> {
    let html = fetch_with_fallback(client, url, "detail page").await?;
    let document = Html::parse_document(&html);

    let magnet_sel = Selector::parse("a[href^='magnet:']").ok()?;
    let magnet = document
        .select(&magnet_sel)
        .next()
        .and_then(|el| el.value().attr("href"))
        .map(String::from);

    if magnet.is_none() {
        log_error("1337x", &format!("No magnet link found on detail page: {}", url));
    }

    magnet
}

/// Scrape 1337x for torrents
pub async fn scrape_1337x(client: &Client, query: &str, page: u32) -> Option<Vec<TorrentResult>> {
    let encoded = urlencoding::encode(query);
    let url = format!("{}/search/{}/{}/", BASE_URL, encoded, page);

    log_info("1337x", &format!("Fetching search: {}", url));
    let html = fetch_with_fallback(client, &url, "search page").await?;

    // Parse HTML and extract items synchronously (before any await)
    let items = {
        let document = Html::parse_document(&html);

        let row_sel = match Selector::parse("table.table-list tbody tr") {
            Ok(s) => s,
            Err(e) => {
                log_error("1337x", &format!("Failed to parse row selector: {:?}", e));
                return None;
            }
        };
        let name_sel = Selector::parse("td.name a:nth-of-type(2)").ok()?;
        let seeds_sel = Selector::parse("td.seeds").ok()?;
        let leech_sel = Selector::parse("td.leeches").ok()?;
        let size_sel = Selector::parse("td.size").ok()?;

        let mut items = Vec::new();
        let mut row_count = 0;

        for row in document.select(&row_sel) {
            row_count += 1;
            let name_el = row.select(&name_sel).next();
            let seeds_el = row.select(&seeds_sel).next();
            let leech_el = row.select(&leech_sel).next();
            let size_el = row.select(&size_sel).next();

            if let Some(name_el) = name_el {
                let name = clean_text(&name_el.text().collect::<String>());
                let href = name_el.value().attr("href").unwrap_or("");
                // Handle both relative and absolute URLs
                let detail_url = if href.starts_with("http") {
                    href.to_string()
                } else {
                    format!("{}{}", BASE_URL, href)
                };

                let seeders: i64 = seeds_el
                    .map(|e| e.text().collect::<String>())
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0);

                let leechers: i64 = leech_el
                    .map(|e| e.text().collect::<String>())
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0);

                let size = size_el
                    .map(|e| {
                        let text = e.text().collect::<String>();
                        // Size format: "1.5 GB1.5 GB" - take first part
                        let parts: Vec<&str> = text.split_whitespace().collect();
                        if parts.len() >= 2 {
                            format!("{} {}", parts[0], parts[1])
                        } else {
                            text.trim().to_string()
                        }
                    })
                    .unwrap_or_default();

                items.push((name, detail_url, seeders, leechers, size));
            }
        }

        if row_count == 0 {
            log_error("1337x", "No table rows found - selector 'table.table-list tbody tr' may be outdated");
        }

        items
    }; // document is dropped here, before any await

    if items.is_empty() {
        log_error("1337x", "No items parsed from search results - CSS selectors may need updating");
        return Some(Vec::new());
    }

    log_info("1337x", &format!("Found {} items, fetching magnet links...", items.len()));

    // Limit to 8 to avoid Firecrawl rate limits
    let items: Vec<_> = items.into_iter().take(8).collect();

    // Fetch magnets sequentially to avoid Send issues
    let mut results = Vec::new();
    let mut magnet_failures = 0;
    for (name, url, seeders, leechers, size) in items {
        if let Some(magnet) = fetch_detail(client, &url).await {
            if !magnet.is_empty() {
                results.push(TorrentResult {
                    name,
                    size,
                    seeders,
                    leechers,
                    magnet,
                    source: "1337x".to_string(),
                    url: Some(url),
                    category: None,
                });
            } else {
                magnet_failures += 1;
            }
        } else {
            magnet_failures += 1;
        }
    }

    if magnet_failures > 0 {
        log_info("1337x", &format!("{} magnet link fetches failed", magnet_failures));
    }

    Some(results)
}
