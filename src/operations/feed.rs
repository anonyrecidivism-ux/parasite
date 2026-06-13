/// FEED — поглощение всех данных со страницы:
/// ссылки, email-адреса, телефоны, мета-теги, скрипты.

use anyhow::Result;
use regex::Regex;
use url::Url;

use crate::parser::{normalize_url, HtmlParser};
use crate::scoring::ScoringEngine;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("FEED — поглощение данных");
    println!();

    let target = ui::prompt("Target URL:");
    if target.is_empty() { ui::err("URL обязателен"); return Ok(()); }

    let base = match Url::parse(&target) {
        Ok(u) => u,
        Err(e) => { ui::err(&format!("Некорректный URL: {e}")); return Ok(()); }
    };

    println!();
    println!("  {GRAY}Поглощаем...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(20))
        .gzip(true)
        .build()?;

    let resp = match client.get(&target).send().await {
        Ok(r)  => r,
        Err(e) => { ui::cursor_up(1); ui::err(&format!("Ошибка: {e}")); return Ok(()); }
    };

    let status = resp.status().as_u16();
    let html   = resp.text().await.unwrap_or_default();

    ui::cursor_up(1);
    println!("  {BRED}✓{RESET}  HTTP {status}  |  {WHITE}{} байт{RESET}", html.len());

    // ── Ссылки ────────────────────────────────────────────────────────────────
    let links: Vec<Url> = HtmlParser::extract_links(&html, &base)
        .into_iter().map(normalize_url).collect();

    let mut scored: Vec<(i32, &Url)> = links.iter()
        .filter(|u| !ScoringEngine::is_media(u.path()))
        .map(|u| (ScoringEngine::score(u, 1), u))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    // ── Email-адреса ─────────────────────────────────────────────────────────
    let email_re = Regex::new(r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}")?;
    let mut emails: Vec<&str> = email_re.find_iter(&html).map(|m| m.as_str()).collect();
    emails.dedup();

    // ── Телефоны ─────────────────────────────────────────────────────────────
    let phone_re = Regex::new(r"[\+]?[0-9]{1,3}[\s\-\.]?[\(\s]?[0-9]{3}[\)\s\-\.]?[0-9]{3}[\s\-\.]?[0-9]{2,4}")?;
    let mut phones: Vec<&str> = phone_re.find_iter(&html).map(|m| m.as_str()).collect();
    phones.dedup();
    phones.retain(|p| p.len() >= 7);

    // ── Мета-теги ────────────────────────────────────────────────────────────
    let doc   = scraper::Html::parse_document(&html);
    let meta_sel  = scraper::Selector::parse("meta").unwrap();
    let metas: Vec<(String, String)> = doc.select(&meta_sel)
        .filter_map(|el| {
            let name = el.value().attr("name")
                .or(el.value().attr("property"))
                .unwrap_or("").to_string();
            let content = el.value().attr("content").unwrap_or("").to_string();
            if name.is_empty() || content.is_empty() { None } else { Some((name, content)) }
        })
        .take(12)
        .collect();

    // ── Скрипты ──────────────────────────────────────────────────────────────
    let script_sel = scraper::Selector::parse("script[src]").unwrap();
    let scripts: Vec<String> = doc.select(&script_sel)
        .filter_map(|el| el.value().attr("src").map(|s| s.to_string()))
        .take(8)
        .collect();

    // ── Вывод ────────────────────────────────────────────────────────────────
    println!();

    // ── Ссылки
    println!("  {DRED}┌─── {BRED}{BOLD}LINKS  ({count}){RESET}{DRED} ─────────────────────────────────────────┐{RESET}",
        count = scored.len());
    println!("  {DRED}│{RESET}  {GRAY}{:>4}  {:>5}  {:<8}  {}{RESET}  {DRED}│{RESET}", "#", "Score", "Type", "URL");
    println!("  {DRED}│{RESET}  {GRAY}────  ─────  ────────  {}  │{RESET}", "─".repeat(42));

    for (i, (sc, url)) in scored.iter().enumerate().take(15) {
        let sc_c = if *sc > 500 { BRED } else if *sc > 0 { RED } else { GRAY };
        let ptype = format!("{}", crate::parser::HtmlParser::detect_page_type(url, ""));
        let u = url.as_str();
        let us = if u.len() > 44 { format!("{}…", &u[..43]) } else { u.to_string() };
        println!(
            "  {DRED}│{RESET}  {GRAY}{:>4}{RESET}  {sc_c}{:>5}{RESET}  {RED}{:<8}{RESET}  {WHITE}{us}{RESET}  {DRED}│{RESET}",
            i+1, sc, ptype
        );
    }
    if scored.len() > 15 {
        println!("  {DRED}│{RESET}  {GRAY}  … и ещё {} ссылок{RESET}  {DRED}│{RESET}", scored.len()-15);
    }
    println!("  {DRED}└──────────────────────────────────────────────────────────────────┘{RESET}");

    // ── Email
    if !emails.is_empty() {
        println!();
        println!("  {DRED}┌─── {BRED}{BOLD}EMAILS  ({count}){RESET}{DRED} ─────────────────────────────────────────┐{RESET}",
            count = emails.len());
        for e in &emails {
            println!("  {DRED}│{RESET}  {BRED}◉{RESET}  {WHITE}{e}{RESET}");
        }
        println!("  {DRED}└──────────────────────────────────────────────────────────────────┘{RESET}");
    }

    // ── Phones
    if !phones.is_empty() {
        println!();
        println!("  {DRED}┌─── {BRED}{BOLD}PHONES  ({count}){RESET}{DRED} ─────────────────────────────────────────┐{RESET}",
            count = phones.len());
        for p in phones.iter().take(8) {
            println!("  {DRED}│{RESET}  {RED}☎{RESET}  {WHITE}{p}{RESET}");
        }
        println!("  {DRED}└──────────────────────────────────────────────────────────────────┘{RESET}");
    }

    // ── Meta
    if !metas.is_empty() {
        println!();
        println!("  {DRED}┌─── {BRED}{BOLD}META TAGS{RESET}{DRED} ──────────────────────────────────────────────────┐{RESET}");
        for (k, v) in &metas {
            let vs = if v.len() > 48 { format!("{}…", &v[..47]) } else { v.clone() };
            println!("  {DRED}│{RESET}  {GRAY}{k:<20}{RESET}  {WHITE}{vs}{RESET}");
        }
        println!("  {DRED}└──────────────────────────────────────────────────────────────────┘{RESET}");
    }

    // ── Scripts
    if !scripts.is_empty() {
        println!();
        println!("  {DRED}┌─── {BRED}{BOLD}SCRIPTS  ({count}){RESET}{DRED} ────────────────────────────────────────────┐{RESET}",
            count = scripts.len());
        for s in &scripts {
            let ss = if s.len() > 60 { format!("{}…", &s[..59]) } else { s.clone() };
            println!("  {DRED}│{RESET}  {RED}⟨/⟩{RESET}  {WHITE}{ss}{RESET}");
        }
        println!("  {DRED}└──────────────────────────────────────────────────────────────────┘{RESET}");
    }

    ui::divider();
    Ok(())
}
