use anyhow::Result;
use scraper::{Html, Selector};
use url::Url;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("leech — скачать ресурсы страницы");
    println!();

    let target = ui::prompt("target url:");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }

    println!("  {GRAY}загружаем страницу...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    let resp = match client.get(&target).send().await {
        Ok(r)  => r,
        Err(e) => { ui::cursor_up(1); ui::err(&format!("{e}")); return Ok(()); }
    };
    let base = resp.url().clone();
    let html = resp.text().await.unwrap_or_default();
    ui::cursor_up(1);

    let doc = Html::parse_document(&html);

    let mut images:  Vec<String> = vec![];
    let mut scripts: Vec<String> = vec![];
    let mut styles:  Vec<String> = vec![];
    let mut fonts:   Vec<String> = vec![];
    let mut other:   Vec<String> = vec![];

    let collect = |sel: &str, attr: &str, doc: &Html, base: &Url| -> Vec<String> {
        let s = Selector::parse(sel).unwrap();
        doc.select(&s)
            .filter_map(|el| el.value().attr(attr))
            .filter_map(|href| base.join(href).ok())
            .map(|u| u.to_string())
            .collect()
    };

    images  = collect("img",    "src",  &doc, &base);
    scripts = collect("script", "src",  &doc, &base);
    styles  = collect("link[rel='stylesheet']", "href", &doc, &base);

    // fonts via link preload
    if let Ok(s) = Selector::parse("link[as='font']") {
        for el in doc.select(&s) {
            if let Some(href) = el.value().attr("href") {
                if let Ok(u) = base.join(href) { fonts.push(u.to_string()); }
            }
        }
    }

    // audio/video/source
    for tag in &["source", "audio", "video"] {
        if let Ok(s) = Selector::parse(tag) {
            for el in doc.select(&s) {
                for attr in &["src", "data-src"] {
                    if let Some(v) = el.value().attr(attr) {
                        if let Ok(u) = base.join(v) { other.push(u.to_string()); }
                    }
                }
            }
        }
    }

    let total = images.len() + scripts.len() + styles.len() + fonts.len() + other.len();

    println!();
    println!("  {DRED}╔══════════════════════ {BRED}{BOLD}leech results{RESET}{DRED} ══════════════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}url:{RESET}  {WHITE}{}{RESET}", shorten(&target, 58));
    println!("  {DRED}║{RESET}  {BRED}{BOLD}{total}{RESET} ресурсов обнаружено");
    println!("  {DRED}╚═════════════════════════════════════════════════════════╝{RESET}");
    println!();

    print_section("images",  &images,  BRED);
    print_section("scripts", &scripts, RED);
    print_section("styles",  &styles,  DRED);
    print_section("fonts",   &fonts,   GRAY);
    print_section("other",   &other,   GRAY);

    ui::divider();
    Ok(())
}

fn print_section(label: &str, items: &[String], color: &str) {
    if items.is_empty() { return; }
    println!("  {DRED}── {color}{BOLD}{label}{RESET} ({}) ─────────────────────────────────────────", items.len());
    for (i, url) in items.iter().enumerate().take(30) {
        println!("  {GRAY}{:>3}{RESET}  {color}{}{RESET}", i+1, shorten(url, 68));
    }
    if items.len() > 30 {
        println!("  {GRAY}  … и ещё {} ресурсов{RESET}", items.len()-30);
    }
    println!();
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
