use anyhow::Result;
use std::time::{Duration, Instant};
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("dormant check — проверка живости хостов");
    println!();

    println!("  {GRAY}введите url'ы по одному (пустая строка — завершить):{RESET}\n");
    let mut urls: Vec<String> = vec![];
    loop {
        let line = ui::prompt(&format!("[{:02}]", urls.len()+1));
        if line.is_empty() { break; }
        let u = if line.starts_with("http") { line } else { format!("https://{line}") };
        urls.push(u);
    }

    if urls.is_empty() { ui::warn("список пуст"); return Ok(()); }

    let timeout = ui::prompt_default("таймаут (сек):", "8")
        .parse::<u64>().unwrap_or(8);

    println!("  {GRAY}проверяем {} хостов...{RESET}", urls.len());

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(Duration::from_secs(timeout))
        .build()?;

    let mut results: Vec<(String, bool, u16, u64)> = vec![];

    let futs: Vec<_> = urls.iter().map(|url| {
        let client = client.clone();
        let url    = url.clone();
        async move {
            let start = Instant::now();
            match client.head(&url).send().await {
                Ok(r) => {
                    let ms = start.elapsed().as_millis() as u64;
                    (url, true, r.status().as_u16(), ms)
                }
                Err(_) => (url, false, 0u16, 0u64),
            }
        }
    }).collect();

    results = futures::future::join_all(futs).await;

    ui::cursor_up(1);
    let alive  = results.iter().filter(|r| r.1).count();
    let dead   = results.len() - alive;

    println!();
    println!("  {DRED}╔══════════════ {BRED}{BOLD}dormant check{RESET}{DRED} ══════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {BRED}живых: {alive}{RESET}   {GRAY}мёртвых: {dead}{RESET}");
    println!("  {DRED}║{RESET}");

    for (url, alive, status, ms) in &results {
        let (sym, col, st) = if *alive {
            match status {
                200..=299 => ("✓", BRED, format!("{status}")),
                301..=308 => ("→", RED,  format!("{status} redirect")),
                _         => ("~", RED,  format!("{status}")),
            }
        } else {
            ("✗", GRAY, "timeout/error".to_string())
        };
        let short_url = if url.len() > 50 { format!("{}…", &url[..49]) } else { url.clone() };
        let ms_str = if *alive { format!("{ms}ms") } else { "—".to_string() };
        println!("  {DRED}║{RESET}  {col}{sym}{RESET}  {col}{st:<18}{RESET}  {GRAY}{ms_str:<8}{RESET}  {WHITE}{short_url}{RESET}");
    }

    println!("  {DRED}║{RESET}");
    println!("  {DRED}╚══════════════════════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}
