//! YTS scraper with Firecrawl support

use super::{clean_text, TorrentResult};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

/// YTS domains to try
const YTS_DOMAINS: &[&str] = &["yts.mx", "yts.lt"];

/// Standard trackers for YTS magnets
const YTS_TRACKERS: &[&str] = &[
    "udp://open.demonii.com:1337/announce",
    "udp://tracker.openbittorrent.com:80",
    "udp://tracker.coppersurfer.tk:6969",
    "udp://glotorrents.pw:6969/announce",
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://torrent.gresille.org:80/announce",
    "udp://p4p.arenabg.com:1337",
    "udp://tracker.leechers-paradise.org:6969",
];

/// Firecrawl scrape request
#[derive(Serialize)]
struct FirecrawlRequest {
    url: String,
    formats: Vec<String>,
}

/// Firecrawl scrape response
#[derive(Deserialize)]
struct FirecrawlResponse {
    html: Option<String>,
}

/// Convert info hash to magnet link
fn hash_to_magnet(info_hash: &str, name: &str) -> String {
    let hash = info_hash.to_uppercase();
    let encoded_name = urlencoding::encode(name);
    let trackers: String = YTS_TRACKERS
        .iter()
        .map(|t| format!("&tr={}", urlencoding::encode(t)))
        .collect();

    format!("magnet:?xt=urn:btih:{}&dn={}{}", hash, encoded_name, trackers)
}

/// Extract info hash from YTS torrent download URL
fn extract_hash_from_url(url: &str) -> Option<String> {
    let re = Regex::new(r"/torrent/download/([A-Fa-f0-9]{40})").ok()?;
    re.captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Fetch URL using Firecrawl API (for bypassing anti-bot)
async fn fetch_with_firecrawl(client: &Client, url: &str) -> Option<String> {
    let api_key = std::env::var("FIRECRAWL_API_KEY").ok()?;

    let request = FirecrawlRequest {
        url: url.to_string(),
        formats: vec!["html".to_string()],
    };

    let response = client
        .post("https://api.firecrawl.dev/v1/scrape")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await
        .ok()?;

    let data: serde_json::Value = response.json().await.ok()?;

    // Extract HTML from response - structure is { data: { html: "..." } }
    data.get("data")
        .and_then(|d| d.get("html"))
        .and_then(|h| h.as_str())
        .map(String::from)
}

/// Fetch URL - tries Firecrawl first, falls back to regular fetch
async fn fetch_with_fallback(client: &Client, url: &str) -> Option<String> {
    // Try Firecrawl first (better for YTS anti-bot)
    if let Some(html) = fetch_with_firecrawl(client, url).await {
        if !html.is_empty() {
            return Some(html);
        }
    }

    // Fall back to regular fetch
    client
        .get(url)
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()
}

/// Parse movie page and extract torrent info
fn parse_movie_page(html: &str, movie_name: &str) -> Vec<TorrentResult> {
    let document = Html::parse_document(html);
    let mut results = Vec::new();

    let torrent_sel = match Selector::parse("a[href*='/torrent/download/']") {
        Ok(s) => s,
        Err(_) => return results,
    };

    let quality_re = match Regex::new(r"(\d+p(?:\.\w+)*)") {
        Ok(r) => r,
        Err(_) => return results,
    };

    let size_re = match Regex::new(r"(\d+(?:\.\d+)?\s*(?:GB|MB|GiB|MiB))") {
        Ok(r) => r,
        Err(_) => return results,
    };

    let mut seen_hashes = std::collections::HashSet::new();
    let mut quality_list = Vec::new();

    // Collect quality options with hashes
    for link in document.select(&torrent_sel) {
        let href = link.value().attr("href").unwrap_or("");
        let info_hash = match extract_hash_from_url(href) {
            Some(h) => h,
            None => continue,
        };

        if seen_hashes.contains(&info_hash) {
            continue;
        }

        // Get quality from link text or title
        let link_text = link.text().collect::<String>();
        let title = link.value().attr("title").unwrap_or("");

        let quality = quality_re
            .find(&link_text)
            .or_else(|| quality_re.find(title))
            .map(|m| m.as_str().to_string());

        if let Some(quality) = quality {
            seen_hashes.insert(info_hash.clone());
            quality_list.push((quality, info_hash));
        }
    }

    // Collect sizes (simplified - just use page text)
    let page_text: String = document.root_element().text().collect();
    let sizes: Vec<String> = size_re
        .find_iter(&page_text)
        .map(|m| m.as_str().to_string())
        .collect();

    // Create results for each quality
    for (i, (quality, info_hash)) in quality_list.into_iter().enumerate() {
        let size = sizes.get(i).cloned().unwrap_or_default();
        let full_name = format!("{} [{}]", movie_name, quality);
        let magnet = hash_to_magnet(&info_hash, &full_name);

        results.push(TorrentResult {
            name: full_name,
            size,
            seeders: 0, // YTS doesn't show seeders
            leechers: 0,
            magnet,
            source: "yts".to_string(),
            url: None,
            category: Some("Movies".to_string()),
        });
    }

    results
}

/// Scrape YTS for movies
pub async fn scrape_yts(client: &Client, query: &str, page: u32) -> Option<Vec<TorrentResult>> {
    let encoded = urlencoding::encode(query);

    let mut html = None;

    // Try each domain with Firecrawl
    for domain in YTS_DOMAINS {
        let url = if page > 1 {
            format!(
                "https://{}/browse-movies/{}/all/all/0/latest/0/all?page={}",
                domain, encoded, page
            )
        } else {
            format!(
                "https://{}/browse-movies/{}/all/all/0/latest/0/all",
                domain, encoded
            )
        };

        if let Some(h) = fetch_with_fallback(client, &url).await {
            if h.contains("browse-movie-wrap") {
                html = Some(h);
                break;
            }
        }
    }

    let html = html?;

    // Parse search page and extract movie links (synchronously)
    let movies = {
        let document = Html::parse_document(&html);

        let movie_sel = Selector::parse("div.browse-movie-wrap").ok()?;
        let link_sel = Selector::parse("a.browse-movie-link").ok()?;
        let title_sel = Selector::parse("a.browse-movie-title").ok()?;
        let year_sel = Selector::parse("div.browse-movie-year").ok()?;

        let mut movies = Vec::new();

        for movie in document.select(&movie_sel) {
            let link = movie.select(&link_sel).next();
            let title = movie.select(&title_sel).next();
            let year = movie.select(&year_sel).next();

            if let (Some(link), Some(title)) = (link, title) {
                let movie_url = link.value().attr("href").unwrap_or("").to_string();
                let name = clean_text(&title.text().collect::<String>());
                let year_str = year
                    .map(|y| clean_text(&y.text().collect::<String>()))
                    .unwrap_or_default();

                if !name.is_empty() && !movie_url.is_empty() {
                    let movie_name = if year_str.is_empty() {
                        name
                    } else {
                        format!("{} ({})", name, year_str)
                    };
                    movies.push((movie_url, movie_name));
                }
            }
        }

        movies
    }; // document dropped here

    // Fetch details for each movie (limit to 10)
    let movies: Vec<_> = movies.into_iter().take(10).collect();
    let mut results = Vec::new();

    for (url, name) in movies {
        if let Some(html) = fetch_with_fallback(client, &url).await {
            let movie_results = parse_movie_page(&html, &name);
            results.extend(movie_results);
        }
    }

    Some(results)
}
