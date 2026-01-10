//! The Pirate Bay scraper - uses proxy sites

use super::{clean_text, TorrentResult};
use reqwest::Client;
use scraper::{Html, Selector};

/// List of TPB proxy domains to try
const TPB_PROXIES: &[&str] = &[
    "thepiratebay10.org",
    "piratebay.live",
    "thepiratebay.zone",
    "tpb.party",
];

/// Try fetching from TPB proxies until one works
async fn try_fetch_tpb(client: &Client, path: &str) -> Option<(String, String)> {
    for domain in TPB_PROXIES {
        let url = format!("https://{}{}", domain, path);
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(html) = resp.text().await {
                if html.contains("searchResult") {
                    return Some((html, domain.to_string()));
                }
            }
        }
    }
    None
}

/// Parse search results from TPB HTML
fn parse_search_results(html: &str) -> Vec<TorrentResult> {
    let document = Html::parse_document(html);
    let mut results = Vec::new();

    let table_sel = match Selector::parse("table#searchResult") {
        Ok(s) => s,
        Err(_) => return results,
    };

    let row_sel = Selector::parse("tr").unwrap();
    let cell_sel = Selector::parse("td").unwrap();
    let link_sel = Selector::parse("a").unwrap();
    let magnet_sel = Selector::parse("a[href^='magnet:']").unwrap();

    let table = match document.select(&table_sel).next() {
        Some(t) => t,
        None => return results,
    };

    for row in table.select(&row_sel).skip(1) {
        let cells: Vec<_> = row.select(&cell_sel).collect();
        if cells.len() < 4 {
            continue;
        }

        // Cell 1: Name link
        let name_link = cells.get(1).and_then(|c| c.select(&link_sel).next());
        let name = name_link
            .map(|l| clean_text(&l.text().collect::<String>()))
            .unwrap_or_default();

        if name.is_empty() {
            continue;
        }

        // Magnet link (usually in cell 1 or 3)
        let magnet = row
            .select(&magnet_sel)
            .next()
            .and_then(|l| l.value().attr("href"))
            .map(String::from)
            .unwrap_or_default();

        if magnet.is_empty() {
            continue;
        }

        // Size (cell 4)
        let size = cells
            .get(4)
            .map(|c| clean_text(&c.text().collect::<String>()))
            .unwrap_or_default();

        // Seeders (cell 5)
        let seeders: i64 = cells
            .get(5)
            .and_then(|c| c.text().collect::<String>().trim().parse().ok())
            .unwrap_or(0);

        // Leechers (cell 6)
        let leechers: i64 = cells
            .get(6)
            .and_then(|c| c.text().collect::<String>().trim().parse().ok())
            .unwrap_or(0);

        results.push(TorrentResult {
            name,
            size,
            seeders,
            leechers,
            magnet,
            source: "tpb".to_string(),
            url: None,
            category: None,
        });
    }

    results
}

/// Scrape The Pirate Bay for torrents
pub async fn scrape_tpb(client: &Client, query: &str, page: u32) -> Option<Vec<TorrentResult>> {
    let encoded = urlencoding::encode(query);
    // TPB pages are 0-indexed, sort by seeders (99)
    let tpb_page = if page > 0 { page - 1 } else { 0 };
    let path = format!("/search/{}/{}/99/0", encoded, tpb_page);

    let (html, _domain) = try_fetch_tpb(client, &path).await?;
    let results = parse_search_results(&html);

    Some(results)
}
