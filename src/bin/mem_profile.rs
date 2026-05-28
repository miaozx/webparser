//! Memory profiler for rs-trafilatura.
//! Reads benchmark JSONL, processes each entry single-threaded,
//! and reports RSS at each phase of extraction.

use rs_trafilatura::{extract_with_options, Options};
use serde::Deserialize;
use std::fs;
use std::io::{self, BufRead};
use std::time::Instant;

#[derive(Deserialize)]
struct BenchItem {
    track_id: String,
    url: Option<String>,
    html: String,
    meta: Option<serde_json::Value>,
}

fn get_rss_kb() -> u64 {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                return parts[1].parse().unwrap_or(0);
            }
        }
    }
    0
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: mem_profile <jsonl_file> [max_pages]");
        std::process::exit(1);
    }
    let filepath = &args[1];
    let max_pages: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);

    let file = fs::File::open(filepath).expect("Failed to open file");
    let reader = io::BufReader::new(file);

    let mut total_rss_peak: u64 = 0;
    let mut total_elapsed = std::time::Duration::ZERO;
    let mut count = 0usize;
    let mut slowest = (0usize, 0.0f64, String::new(), 0usize);

    println!("{:>4} | {:>10} | {:>8} | {:40} | track_id", "#", "time", "html_size", "content_info");
    println!("{}", "-".repeat(90));

    for line in reader.lines() {
        if count >= max_pages {
            break;
        }
        let line = line.unwrap();
        let item: BenchItem = match serde_json::from_str(&line) {
            Ok(item) => item,
            Err(e) => {
                eprintln!("Skipping invalid JSON: {e}");
                continue;
            }
        };

        let html_len = item.html.len();

        let t_extract = Instant::now();
        let options = Options {
            url: item.url.clone(),
            output_markdown: true,
            include_tables: true,
            include_links: true,
            include_formatting: true,
            ..Options::default()
        };
        let result = extract_with_options(&item.html, &options);
        let elapsed = t_extract.elapsed();
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;

        let mut content_info = String::new();
        match result {
            Ok(extracted) => {
                let content_len = extracted.content_text.len();
                let md_len = extracted.content_markdown.as_ref().map(|s| s.len()).unwrap_or(0);
                content_info = format!("text={content_len} md={md_len}");
            }
            Err(e) => {
                content_info = format!("ERROR: {e}");
            }
        }

        println!("{count:>4} | {elapsed_ms:>8.2}ms | {html_len:>8} | {:40} | {}",
                 content_info, item.track_id);

        if elapsed_ms > slowest.1 {
            slowest = (count, elapsed_ms, item.track_id.clone(), html_len);
        }

        let rss_peak = get_rss_kb();
        if rss_peak > total_rss_peak {
            total_rss_peak = rss_peak;
        }
        total_elapsed += elapsed;
        count += 1;
    }

    println!("\n\n========================================");
    println!("Summary ({count} pages):");
    println!("  Total time: {total_elapsed:?}");
    println!("  Peak RSS:  {total_rss_peak} KB ({:.2} MB)", total_rss_peak as f64 / 1024.0);
    if count > 0 {
        println!("  Avg time/page: {:?}", total_elapsed / count as u32);
    }
    println!();
    println!("SLOWEST PAGE:");
    println!("  Page #{}: {} ({} bytes)", slowest.0, slowest.2, slowest.3);
    println!("  Time: {:.2}ms", slowest.1);
}
