use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Result;
use tokio_util::sync::CancellationToken;

use crate::config::CrawlerConfig;
use crate::crawler::Crawler;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("infect — захват хоста");
    println!();

    let target       = ui::prompt("target url:");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }
    let workers_s    = ui::prompt_default("воркеры:", "8");
    let max_pages_s  = ui::prompt_default("макс. страниц:", "500");
    let max_depth_s  = ui::prompt_default("макс. глубина:", "4");
    let delay_s      = ui::prompt_default("задержка домена (мс):", "1000");

    let config = CrawlerConfig {
        seed_urls:       vec![target.clone()],
        num_workers:     workers_s.parse().unwrap_or(8),
        max_pages:       max_pages_s.parse().unwrap_or(500),
        max_depth:       max_depth_s.parse().unwrap_or(4),
        domain_delay_ms: delay_s.parse().unwrap_or(1000),
        ..CrawlerConfig::default()
    };

    println!();
    ui::divider();
    println!();

    let cancel  = CancellationToken::new();
    let crawler = Crawler::new(config, cancel.clone());
    let stats   = crawler.stats.clone();
    let queue   = crawler.queue.clone();
    let max_pg  = crawler.config.max_pages;

    let cancel_sig = cancel.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        cancel_sig.cancel();
    });

    tokio::spawn({
        let c  = crawler.clone();
        let cc = cancel.clone();
        async move {
            if let Err(e) = c.run().await {
                tracing::error!("{e}");
                cc.cancel();
            }
        }
    });

    // stats loop — no eye animation to avoid overlap
    let mut interval = tokio::time::interval(Duration::from_millis(400));
    let stat_lines = {
        let last = stats.last_url_str().await;
        let n = ui::print_crawl_stats(
            &target, stats.pages_done.load(Ordering::Relaxed),
            stats.errors.load(Ordering::Relaxed), queue.len().await,
            stats.skipped.load(Ordering::Relaxed), stats.pages_per_sec(),
            stats.elapsed_secs(), &last, max_pg,
        );
        ui::flush();
        n
    };

    let mut lines_to_up = stat_lines + 1;
    loop {
        interval.tick().await;
        ui::cursor_up(lines_to_up);
        let last = stats.last_url_str().await;
        lines_to_up = ui::print_crawl_stats(
            &target, stats.pages_done.load(Ordering::Relaxed),
            stats.errors.load(Ordering::Relaxed), queue.len().await,
            stats.skipped.load(Ordering::Relaxed), stats.pages_per_sec(),
            stats.elapsed_secs(), &last, max_pg,
        ) + 1;
        ui::flush();
        if cancel.is_cancelled() { break; }
    }

    let pages  = stats.pages_done.load(Ordering::Relaxed);
    let errors = stats.errors.load(Ordering::Relaxed);
    let secs   = stats.elapsed_secs();

    println!();
    ui::divider();
    println!("  {BRED}{BOLD}заражение завершено{RESET}");
    ui::kv("поглощено страниц:", &pages.to_string());
    ui::kv("ошибок:", &errors.to_string());
    ui::kv("время:", &format!("{:02}:{:02}:{:02}", secs/3600, (secs%3600)/60, secs%60));
    ui::kv("данные в:", "results.json");
    ui::divider();

    Ok(())
}
