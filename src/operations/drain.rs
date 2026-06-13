/// DRAIN CACHE — извлечь накопленные данные
use anyhow::Result;
use std::collections::HashMap;

use crate::storage::CrawlResult;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("DRAIN CACHE — накопленные данные");
    println!();

    let path = ui::prompt_default("Файл результатов:", "results.json");
    println!();
    println!("  {GRAY}Извлекаем {path}...{RESET}");

    let content = match tokio::fs::read_to_string(&path).await {
        Ok(c)  => c,
        Err(e) => { ui::cursor_up(1); ui::err(&format!("{e}")); return Ok(()); }
    };

    let results: Vec<CrawlResult> = match serde_json::from_str(&content) {
        Ok(r)  => r,
        Err(e) => { ui::cursor_up(1); ui::err(&format!("Некорректный JSON: {e}")); return Ok(()); }
    };

    ui::cursor_up(1);

    if results.is_empty() { ui::warn("Кэш пуст"); return Ok(()); }

    let mut by_type: HashMap<String, usize> = HashMap::new();
    let mut total_links = 0usize;
    let mut total_ms    = 0u64;

    for r in &results {
        *by_type.entry(format!("{}", r.page_type)).or_insert(0) += 1;
        total_links += r.links_found;
        total_ms    += r.response_time_ms;
    }

    let avg_ms = if results.is_empty() { 0 } else { total_ms / results.len() as u64 };

    println!();
    println!("  {DRED}╔═══════════════════════════ {BRED}{BOLD}CACHE CONTENTS{RESET}{DRED} ══════════════════════════╗{RESET}");
    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}Страниц сохранено :{RESET}  {BRED}{BOLD}{:<50}{RESET}  {DRED}║{RESET}", results.len());
    println!("  {DRED}║{RESET}  {GRAY}Ссылок извлечено  :{RESET}  {RED}{total_links:<50}{RESET}  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}Среднее время     :{RESET}  {DRED}{avg_ms} мс{RESET}{: <43}{DRED}║{RESET}", "");
    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}Распределение по типам:{RESET}{: <48}{DRED}║{RESET}", "");
    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");

    let mut tv: Vec<(String, usize)> = by_type.into_iter().collect();
    tv.sort_by(|a, b| b.1.cmp(&a.1));
    for (pt, cnt) in &tv {
        let b = (cnt * 32 / results.len()).min(32);
        let bar = format!("{}{}", "█".repeat(b), "░".repeat(32-b));
        let pct = cnt * 100 / results.len();
        println!(
            "  {DRED}║{RESET}  {RED}  {pt:<12}{RESET}  {BRED}{bar}{RESET}  {WHITE}{cnt:>4}  {pct:>3}%{RESET}  {DRED}║{RESET}"
        );
    }

    println!("  {DRED}║{RESET}                                                                      {DRED}║{RESET}");
    println!("  {DRED}╚══════════════════════════════════════════════════════════════════════╝{RESET}");

    // Последние записи
    println!();
    println!("  {BRED}{BOLD}Последние поглощённые:{RESET}\n");
    println!("  {GRAY}  {:>4}  {:<10}  {:>6}  {:>7}  {}{RESET}", "#", "Type", "HTTP", "ms", "URL");
    println!("  {GRAY}  {}  {}  {}  {}  {}{RESET}", "─".repeat(4), "─".repeat(10), "─".repeat(6), "─".repeat(7), "─".repeat(50));

    for (i, r) in results.iter().rev().take(20).enumerate() {
        let sc = if r.status_code < 300 { BRED } else { RED };
        let u = if r.url.len() > 52 { format!("{}…", &r.url[..51]) } else { r.url.clone() };
        println!(
            "  {GRAY}  {:>4}{RESET}  {RED}{:<10}{RESET}  {sc}{:>6}{RESET}  {GRAY}{:>6}ms{RESET}  {WHITE}{u}{RESET}",
            i+1, format!("{}", r.page_type), r.status_code, r.response_time_ms,
        );
    }
    if results.len() > 20 {
        println!("  {GRAY}  … и ещё {} записей в {path}{RESET}", results.len()-20);
    }

    ui::divider();
    Ok(())
}
