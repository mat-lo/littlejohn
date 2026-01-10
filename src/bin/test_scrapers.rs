//! Test all scrapers with a live query

use littlejohn::scrapers::{self, TorrentResult};

fn print_results(name: &str, results: &Option<Vec<TorrentResult>>) {
    println!("\n============================================================");
    println!("  {}", name);
    println!("============================================================");

    match results {
        Some(items) if !items.is_empty() => {
            println!("  ✓ Found {} results:", items.len());
            for (i, r) in items.iter().take(5).enumerate() {
                println!("    {}. {} | {} | {} seeds", i + 1, truncate(&r.name, 45), r.size, r.seeders);
            }
            if items.len() > 5 {
                println!("    ... and {} more", items.len() - 5);
            }
        }
        Some(_) => println!("  ⚠ No results found (empty list)"),
        None => println!("  ✗ FAILED - returned None (network/parse error)"),
    }
}

#[tokio::main]
async fn main() {
    // Load .env file - check current directory first, then config directory
    if dotenvy::dotenv().is_err() {
        if let Some(config_dir) = dirs::config_dir() {
            let config_env = config_dir.join("littlejohn").join(".env");
            dotenvy::from_path(&config_env).ok();
        }
    }

    let query = std::env::args().nth(1).unwrap_or_else(|| "matrix 1999".to_string());
    println!("\n🔍 Testing scrapers with query: \"{}\"", query);

    let client = match scrapers::create_client() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create HTTP client: {}", e);
            return;
        }
    };

    // Test each scraper individually
    println!("\n--- Testing 1337x ---");
    let r1337x = scrapers::x1337::scrape_1337x(&client, &query, 1).await;
    print_results("1337x", &r1337x);

    println!("\n--- Testing TPB ---");
    let tpb = scrapers::tpb::scrape_tpb(&client, &query, 1).await;
    print_results("TPB", &tpb);

    println!("\n--- Testing BitSearch ---");
    let bitsearch = scrapers::bitsearch::scrape_bitsearch(&client, &query, 1).await;
    print_results("BitSearch", &bitsearch);

    println!("\n--- Testing YTS ---");
    let yts = scrapers::yts::scrape_yts(&client, &query, 1).await;
    print_results("YTS", &yts);

    println!("\n--- Testing ilCorsaroNero ---");
    let ilcorsaronero = scrapers::ilcorsaronero::scrape_ilcorsaronero(&client, &query, 1).await;
    print_results("ilCorsaroNero", &ilcorsaronero);

    // Test combined search
    println!("\n\n============================================================");
    println!("  COMBINED SEARCH (search_all)");
    println!("============================================================");
    let all = scrapers::search_all(&query, 1).await;
    println!("  Total results: {}", all.len());

    // Group by source
    let mut by_source: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in &all {
        *by_source.entry(r.source.clone()).or_insert(0) += 1;
    }
    println!("  By source: {:?}", by_source);

    println!("\n  Top 10 by seeders:");
    for (i, r) in all.iter().take(10).enumerate() {
        println!("    {}. [{}] {} | {} | {} seeds",
            i + 1, r.source, truncate(&r.name, 40), r.size, r.seeders);
    }

    // Summary
    println!("\n============================================================");
    println!("  SUMMARY");
    println!("============================================================");
    let success_count = [&r1337x, &tpb, &bitsearch, &yts, &ilcorsaronero]
        .iter()
        .filter(|r| r.as_ref().map(|v| !v.is_empty()).unwrap_or(false))
        .count();
    println!("  {} of 5 scrapers returned results", success_count);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() }
    else { format!("{}...", &s[..max-3]) }
}
