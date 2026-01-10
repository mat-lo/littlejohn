//! Test scrapers binary

use littlejohn::scrapers;

#[tokio::main]
async fn main() {
    // Load env
    dotenvy::dotenv().ok();

    scrapers::init_log();

    let query = std::env::args().nth(1).unwrap_or_else(|| "ubuntu".to_string());
    println!("Testing scrapers with query: {}", query);
    println!("---");

    let results = scrapers::search_all(&query, 1).await;

    println!("\nTotal results: {}", results.len());

    // Group by source
    let mut by_source: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in &results {
        *by_source.entry(r.source.clone()).or_insert(0) += 1;
    }

    println!("\nResults by source:");
    for source in scrapers::SCRAPERS {
        let count = by_source.get(*source).copied().unwrap_or(0);
        let status = if count > 0 { "OK" } else { "FAILED" };
        println!("  {:15} {:>3} results [{}]", source, count, status);
    }

    // Show first few results from each source
    println!("\nSample results:");
    for source in scrapers::SCRAPERS {
        let source_results: Vec<_> = results.iter().filter(|r| r.source == *source).take(2).collect();
        if source_results.is_empty() {
            println!("\n[{}] - No results", source);
        } else {
            println!("\n[{}]", source);
            for r in source_results {
                println!("  {} ({} seeders, {})",
                    if r.name.len() > 60 { format!("{}...", &r.name[..60]) } else { r.name.clone() },
                    r.seeders,
                    r.size
                );
            }
        }
    }
}
