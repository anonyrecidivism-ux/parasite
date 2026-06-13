/// SCORE TARGETS — ранжировать цели по приоритету атаки
use anyhow::Result;
use url::Url;

use crate::parser::HtmlParser;
use crate::scoring::ScoringEngine;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("SCORE TARGETS — ранжирование целей");
    println!();
    println!("  {GRAY}Введите URL по одному (пустая строка — завершить):{RESET}\n");

    let mut urls: Vec<String> = vec![];
    loop {
        let line = ui::prompt(&format!("[{:02}]", urls.len() + 1));
        if line.is_empty() { break; }
        urls.push(line);
    }

    if urls.is_empty() { ui::warn("Список пуст"); return Ok(()); }

    let mut scored: Vec<(i32, String, String, bool)> = urls.iter().map(|raw| {
        match Url::parse(raw) {
            Ok(url) => {
                let s = ScoringEngine::score(&url, 0);
                let m = ScoringEngine::is_media(url.path());
                let t = format!("{}", HtmlParser::detect_page_type(&url, ""));
                (s, raw.clone(), t, m)
            }
            Err(_) => (-9999, raw.clone(), "INVALID".into(), false),
        }
    }).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    println!();
    ui::divider();
    println!("  {BRED}{BOLD}Ранжировано {}{RESET} целей:\n", scored.len());

    println!("  {GRAY}  {:>4}  {:>6}  {:^5}  {:<12}  {}{RESET}", "Rank", "Score", "Stars", "Type", "URL");
    println!("  {GRAY}  {}  {}  {}  {}  {}{RESET}", "─".repeat(4), "─".repeat(6), "─".repeat(5), "─".repeat(12), "─".repeat(50));

    for (rank, (score, url, ptype, is_media)) in scored.iter().enumerate() {
        let sc = if *is_media { GRAY }
            else if *score > 500 { BRED }
            else if *score > 200 { RED }
            else if *score > 0   { DRED }
            else                 { GRAY };

        let stars = if *is_media          { "SKIP " }
            else if *score > 500          { "★★★  " }
            else if *score > 200          { "★★   " }
            else if *score > 0            { "★    " }
            else                          { "     " };

        let u = if url.len() > 52 { format!("{}…", &url[..51]) } else { url.clone() };

        println!(
            "  {GRAY}  {rank:>4}{RESET}  {sc}{score:>6}{RESET}  {RED}{stars}{RESET}  {sc}{ptype:<12}{RESET}  {WHITE}{u}{RESET}",
            rank = rank + 1,
        );
    }

    let high   = scored.iter().filter(|(s,_,_,_)| *s > 500).count();
    let medium = scored.iter().filter(|(s,_,_,_)| *s > 0 && *s <= 500).count();
    let skip   = scored.iter().filter(|(s,_,_,_)| *s <= 0).count();

    println!();
    ui::divider();
    println!("  {BRED}HIGH:{RESET} {high}   {RED}MEDIUM:{RESET} {medium}   {GRAY}SKIP/LOW:{RESET} {skip}");
    ui::divider();
    Ok(())
}
