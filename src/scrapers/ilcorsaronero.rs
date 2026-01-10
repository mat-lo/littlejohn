//! ilCorsaroNero scraper - Italian torrent site, requires Firecrawl

use super::{clean_text, log_error, log_info, TorrentResult};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;

const BASE_URL: &str = "https://ilcorsaronero.link";

/// Firecrawl scrape request
#[derive(Serialize)]
struct FirecrawlRequest {
    url: String,
    formats: Vec<String>,
}

/// Fetch URL using Firecrawl API
async fn fetch_with_firecrawl(url: &str) -> Option<String> {
    let api_key = match std::env::var("FIRECRAWL_API_KEY") {
        Ok(key) if !key.is_empty() => key,
        _ => {
            log_error("ilcorsaronero", "FIRECRAWL_API_KEY not set - this source requires Firecrawl");
            return None;
        }
    };

    // Create client with longer timeout for Firecrawl
    let client = match Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log_error("ilcorsaronero", &format!("Failed to create client: {}", e));
            return None;
        }
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
            log_error("ilcorsaronero", &format!("Firecrawl request failed: {}", e));
            return None;
        }
    };

    let data: serde_json::Value = match response.json().await {
        Ok(d) => d,
        Err(e) => {
            log_error("ilcorsaronero", &format!("Firecrawl response parse error: {}", e));
            return None;
        }
    };

    // Extract HTML from response - structure is { data: { html: "..." } }
    data.get("data")
        .and_then(|d| d.get("html"))
        .and_then(|h| h.as_str())
        .map(String::from)
}

/// Extract magnet link from detail page HTML
fn extract_magnet(html: &str) -> Option<String> {
    // Check for deleted torrent
    if html.contains("Eliminato") || html.to_lowercase().contains("non esiste") {
        return None;
    }

    // Look for magnet link directly in HTML with regex
    let re = Regex::new(r#"(magnet:\?xt=urn:btih:[a-fA-F0-9]{40}[^"<>\s]*)"#).ok()?;
    if let Some(cap) = re.captures(html) {
        let magnet = cap.get(1)?.as_str().replace("&amp;", "&");
        return Some(magnet);
    }

    // Fallback: parse with scraper
    let document = Html::parse_document(html);
    let magnet_sel = Selector::parse(r#"a[href^="magnet:"]"#).ok()?;

    document
        .select(&magnet_sel)
        .next()
        .and_then(|el| el.value().attr("href"))
        .map(String::from)
}

/// Parse search results from ilcorsaronero HTML
fn parse_search_results(html: &str) -> Vec<(String, String, String, String, String)> {
    // Returns: (name, url, seeders, leechers, size)
    let document = Html::parse_document(html);
    let mut results = Vec::new();

    let row_sel = match Selector::parse("tbody tr") {
        Ok(s) => s,
        Err(_) => return results,
    };

    let title_sel = Selector::parse("th a").ok();
    let cell_sel = Selector::parse("td, th").ok();

    for row in document.select(&row_sel) {
        // Get title and URL
        let (name, href) = match &title_sel {
            Some(sel) => match row.select(sel).next() {
                Some(link) => {
                    let name = clean_text(&link.text().collect::<String>());
                    let mut href = link.value().attr("href").unwrap_or("").to_string();
                    if !href.starts_with("http") {
                        href = format!("{}{}", BASE_URL, href);
                    }
                    (name, href)
                }
                None => continue,
            },
            None => continue,
        };

        if name.is_empty() {
            continue;
        }

        // Get cells: [0]=Category, [1]=Title, [2]=Seeders, [3]=Leechers, [4]=Size, [5]=Date
        let cells: Vec<_> = match &cell_sel {
            Some(sel) => row.select(sel).collect(),
            None => continue,
        };

        let seeders = if cells.len() > 2 {
            clean_text(&cells[2].text().collect::<String>())
        } else {
            "0".to_string()
        };

        let leechers = if cells.len() > 3 {
            clean_text(&cells[3].text().collect::<String>())
        } else {
            "0".to_string()
        };

        let size = if cells.len() > 4 {
            clean_text(&cells[4].text().collect::<String>())
        } else {
            String::new()
        };

        results.push((name, href, seeders, leechers, size));
    }

    results
}

/// Scrape ilcorsaronero.link for torrents
pub async fn scrape_ilcorsaronero(
    _client: &Client,
    query: &str,
    page: u32,
) -> Option<Vec<TorrentResult>> {
    // Build search URL
    let encoded = urlencoding::encode(query);
    let url = if page > 1 {
        format!("{}/search?q={}&page={}", BASE_URL, encoded, page)
    } else {
        format!("{}/search?q={}", BASE_URL, encoded)
    };

    log_info("ilcorsaronero", &format!("Fetching: {}", url));

    // Fetch search page with Firecrawl
    let html = match fetch_with_firecrawl(&url).await {
        Some(h) => h,
        None => {
            log_error("ilcorsaronero", "Failed to fetch search page");
            return None;
        }
    };

    // Parse search results
    let items = parse_search_results(&html);

    if items.is_empty() {
        log_info("ilcorsaronero", "No items found in search results");
        return Some(Vec::new());
    }

    log_info("ilcorsaronero", &format!("Found {} items, fetching details...", items.len()));

    // Fetch magnet links from detail pages (limit to 10)
    let mut results = Vec::new();
    let mut magnet_failures = 0;

    for (name, detail_url, seeders, leechers, size) in items.into_iter().take(10) {
        if let Some(detail_html) = fetch_with_firecrawl(&detail_url).await {
            if let Some(magnet) = extract_magnet(&detail_html) {
                let seeders_num = seeders.parse::<i64>().unwrap_or(0);
                let leechers_num = leechers.parse::<i64>().unwrap_or(0);

                results.push(TorrentResult {
                    name,
                    size,
                    seeders: seeders_num,
                    leechers: leechers_num,
                    magnet,
                    source: "ilcorsaronero".to_string(),
                    url: Some(detail_url),
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
        log_info("ilcorsaronero", &format!("{} magnet fetches failed", magnet_failures));
    }

    log_info("ilcorsaronero", &format!("Returning {} results", results.len()));

    Some(results)
}
