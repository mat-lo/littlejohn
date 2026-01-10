//! BitSearch scraper

use super::{clean_text, fetch, TorrentResult};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};

/// Scrape BitSearch for torrents
pub async fn scrape_bitsearch(client: &Client, query: &str, page: u32) -> Option<Vec<TorrentResult>> {
    let encoded = urlencoding::encode(query);
    let url = format!(
        "https://bitsearch.to/search?q={}&page={}&sort=seeders",
        encoded, page
    );

    let html = fetch(client, &url).await?;
    let document = Html::parse_document(&html);

    // Result items are divs with this class combo
    let item_sel = Selector::parse("div.bg-white.rounded-lg.shadow-sm.border").ok()?;
    let title_sel = Selector::parse("h3 a").ok()?;
    let magnet_sel = Selector::parse("a[href^='magnet:']").ok()?;
    let green_span_sel = Selector::parse("span.text-green-600 span.font-medium").ok()?;
    let red_span_sel = Selector::parse("span.text-red-600 span.font-medium").ok()?;
    let size_re = Regex::new(r"([\d.]+\s*(?:GB|MB|KB|TB|GiB|MiB))").ok()?;

    let mut results = Vec::new();
    let mut seen_magnets = std::collections::HashSet::new();

    for item in document.select(&item_sel) {
        // Find magnet link
        let magnet_raw = match item.select(&magnet_sel).next() {
            Some(el) => el.value().attr("href").unwrap_or(""),
            None => continue,
        };

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

        // Find title
        let name = item
            .select(&title_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();

        if name.is_empty() {
            continue;
        }

        // Extract seeders from green span
        let seeders: i64 = item
            .select(&green_span_sel)
            .next()
            .and_then(|el| el.text().collect::<String>().trim().parse().ok())
            .unwrap_or(0);

        // Extract leechers from red span
        let leechers: i64 = item
            .select(&red_span_sel)
            .next()
            .and_then(|el| el.text().collect::<String>().trim().parse().ok())
            .unwrap_or(0);

        // Extract size from item text
        let text: String = item.text().collect();
        let size = size_re
            .find(&text)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();

        results.push(TorrentResult {
            name,
            size,
            seeders,
            leechers,
            magnet,
            source: "bitsearch".to_string(),
            url: None,
            category: None,
        });
    }

    Some(results)
}
