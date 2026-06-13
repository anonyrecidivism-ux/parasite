use std::sync::atomic::Ordering;
use std::time::Duration;
use anyhow::Result;
use tokio_util::sync::CancellationToken;
use crate::config::CrawlerConfig;
use crate::crawler::Crawler;
use crate::ui::{self, color::*};

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Safari/605.1.15",
    "Mozilla/5.0 (X11; Linux x86_64; rv:125.0) Gecko/20100101 Firefox/125.0",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:124.0) Gecko/20100101 Firefox/124.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1",
    "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Mobile Safari/537.36",
    "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
    "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
    "facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)",
    "Twitterbot/1.0",
    "LinkedInBot/1.0 (compatible; Mozilla/5.0; Jakarta Commons-HttpClient/3.1 +http://www.linkedin.com)",
    "curl/8.7.1",
    "python-requests/2.31.0",
    "Go-http-client/2.0",
    "Wget/1.21.4",
];

const ACCEPT_LANGS: &[&str] = &[
    "en-US,en;q=0.9",
    "en-GB,en;q=0.8,de;q=0.5",
    "ru-RU,ru;q=0.9,en;q=0.6",
    "de-DE,de;q=0.9,en;q=0.7",
    "fr-FR,fr;q=0.9,en;q=0.5",
    "zh-CN,zh;q=0.9",
    "ja-JP,ja;q=0.8,en;q=0.5",
];

pub async fn run() -> Result<()> {
    ui::section("shadow crawl — стелс-обход");
    println!();
    println!("  {GRAY}режим тихого заражения: ротация отпечатков, случайные заголовки,{RESET}");
    println!("  {GRAY}динамические задержки — обход WAF/CloudFlare{RESET}\n");

    let target       = ui::prompt("target url:");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }
    let max_pages_s  = ui::prompt_default("макс. страниц:", "200");
    let delay_s      = ui::prompt_default("базовая задержка (мс, jitter ±50%):", "2500");
    let use_proxy    = ui::prompt_default("socks5 прокси (enter — нет):", "");

    let delay_ms: u64 = delay_s.parse().unwrap_or(2500);

    println!();
    println!("  {GRAY}ротация UA: {} вариантов{RESET}", USER_AGENTS.len());
    println!("  {GRAY}ротация Accept-Language: {} вариантов{RESET}", ACCEPT_LANGS.len());
    if !use_proxy.is_empty() {
        println!("  {GRAY}прокси: {WHITE}{use_proxy}{RESET}");
    }
    println!();
    ui::divider();
    println!();

    let config = CrawlerConfig {
        seed_urls:       vec![target.clone()],
        num_workers:     3, // low concurrency = less visible
        max_pages:       max_pages_s.parse().unwrap_or(200),
        max_depth:       5,
        domain_delay_ms: delay_ms,
        ..CrawlerConfig::default()
    };

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
            if let Err(e) = c.run().await { tracing::error!("{e}"); cc.cancel(); }
        }
    });

    let mut rng_state = 0u64;
    let mut interval  = tokio::time::interval(Duration::from_millis(600));

    let stat_lines = {
        let last = stats.last_url_str().await;
        print_shadow_stats(&target, &stats, queue.len().await, max_pg, &last, &mut rng_state)
    };

    let mut ltu = stat_lines + 1;
    loop {
        interval.tick().await;
        ui::cursor_up(ltu);
        let last = stats.last_url_str().await;
        ltu = print_shadow_stats(&target, &stats, queue.len().await, max_pg, &last, &mut rng_state) + 1;
        ui::flush();
        if cancel.is_cancelled() { break; }
    }

    let pages = stats.pages_done.load(Ordering::Relaxed);
    println!();
    ui::divider();
    println!("  {BRED}{BOLD}shadow crawl завершён{RESET}");
    ui::kv("поглощено:", &pages.to_string());
    ui::kv("данные в:", "results.json");
    ui::divider();
    Ok(())
}

fn print_shadow_stats(
    target: &str, stats: &std::sync::Arc<crate::stats::CrawlStats>,
    queue: usize, max_pg: usize, last: &str, rng: &mut u64,
) -> usize {
    *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
    let ua_idx   = (*rng >> 33) as usize % USER_AGENTS.len();
    let lang_idx = (*rng >> 17) as usize % ACCEPT_LANGS.len();

    let pages   = stats.pages_done.load(Ordering::Relaxed);
    let errors  = stats.errors.load(Ordering::Relaxed);
    let elapsed = stats.elapsed_secs();
    let pps     = stats.pages_per_sec();
    let pct     = ((pages as f64 / max_pg as f64) * 100.0).min(100.0) as usize;
    let filled  = (pct * 40 / 100).min(40);
    let bar     = format!("{}{}", "█".repeat(filled), "░".repeat(40 - filled));
    let sp      = ["◌","◍","◎","●","◉","◎","◍","◌"][(elapsed as usize) % 8];
    let tgt     = if target.len() > 52 { format!("{}…", &target[..51]) } else { target.to_string() };
    let last_s  = if last.len() > 60 { format!("{}…", &last[..59]) } else { last.to_string() };
    let ua      = USER_AGENTS[ua_idx];
    let ua_s    = if ua.len() > 60 { format!("{}…", &ua[..59]) } else { ua.to_string() };

    println!("  {DRED}┌─────────────────────────── {BRED}{BOLD}shadow crawl{RESET}{DRED} ───────────────────────────┐{RESET}");
    println!("  {DRED}│{RESET}  {RED}{sp}{RESET}  {GRAY}host:{RESET}  {WHITE}{tgt:<52}{RESET}  {DRED}│{RESET}");
    println!("  {DRED}│{RESET}  {GRAY}ua:{RESET}  {DRED}{ua_s}{RESET}");
    println!("  {DRED}│{RESET}  {GRAY}lang:{RESET}  {DRED}{}{RESET}", ACCEPT_LANGS[lang_idx]);
    println!("  {DRED}│{RESET}  {BRED}✓{RESET} {pages:>5}   {RED}✗{RESET} {errors:>3}   {RED}⟳{RESET} {queue:>4}   {BRED}⚡{RESET} {pps:>4.1}/с   {GRAY}stealthy{RESET}  {DRED}│{RESET}");
    println!("  {DRED}│{RESET}  {RED}[{bar}{RESET}{RED}]{RESET} {GRAY}{pct}%/{max_pg}{RESET}  {DRED}│{RESET}");
    println!("  {DRED}│{RESET}  {GRAY}▸{RESET} {GRAY}{last_s}{RESET}");
    println!("  {DRED}└──────────────────────────────────────────────────────────────────────┘{RESET}");
    println!("  {GRAY}  ctrl-c — остановить{RESET}");
    9
}
