//! 1337x scraper

use super::{clean_text, fetch, TorrentResult};
use reqwest::Client;
use scraper::{Html, Selector};

const BASE_URL: &str = "https://www.1337xx.to";

/// Fetch magnet link from detail page
async fn fetch_detail(client: &Client, url: &str) -> Option<String> {
    let html = fetch(client, url).await?;
    let document = Html::parse_document(&html);

    let magnet_sel = Selector::parse("a[href^='magnet:']").ok()?;
    document
        .select(&magnet_sel)
        .next()
        .and_then(|el| el.value().attr("href"))
        .map(String::from)
}

/// Scrape 1337x for torrents
pub async fn scrape_1337x(client: &Client, query: &str, page: u32) -> Option<Vec<TorrentResult>> {
    let encoded = urlencoding::encode(query);
    let url = format!("{}/search/{}/{}/", BASE_URL, encoded, page);

    let html = fetch(client, &url).await?;

    // Parse HTML and extract items synchronously (before any await)
    let items = {
        let document = Html::parse_document(&html);

        let row_sel = Selector::parse("table.table-list tbody tr").ok()?;
        let name_sel = Selector::parse("td.name a:nth-of-type(2)").ok()?;
        let seeds_sel = Selector::parse("td.seeds").ok()?;
        let leech_sel = Selector::parse("td.leeches").ok()?;
        let size_sel = Selector::parse("td.size").ok()?;

        let mut items = Vec::new();

        for row in document.select(&row_sel) {
            let name_el = row.select(&name_sel).next();
            let seeds_el = row.select(&seeds_sel).next();
            let leech_el = row.select(&leech_sel).next();
            let size_el = row.select(&size_sel).next();

            if let Some(name_el) = name_el {
                let name = clean_text(&name_el.text().collect::<String>());
                let href = name_el.value().attr("href").unwrap_or("");
                let detail_url = format!("{}{}", BASE_URL, href);

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

        items
    }; // document is dropped here, before any await

    // Limit to 15 for performance
    let items: Vec<_> = items.into_iter().take(15).collect();

    // Fetch magnets sequentially to avoid Send issues
    let mut results = Vec::new();
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
            }
        }
    }

    Some(results)
}
