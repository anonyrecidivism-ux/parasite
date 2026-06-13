use anyhow::Result;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use url::Url;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("necrosis check — broken link hijacking");
    println!();

    let target    = ui::prompt("target url:");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }
    let pages_s   = ui::prompt_default("страниц для сканирования:", "15");
    let max_pages = pages_s.parse::<usize>().unwrap_or(15);

    let root = match Url::parse(&target) {
        Ok(u) => u, Err(e) => { ui::err(&format!("{e}")); return Ok(()); }
    };
    let own_host = root.host_str().unwrap_or("").to_string();

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(12))
        .build()?;

    // Crawl internal pages to collect external links
    let mut visited: HashSet<String>  = HashSet::new();
    let mut queue                     = vec![root.to_string()];
    let mut external_links: HashMap<String, Vec<String>> = HashMap::new(); // domain → [found_on_pages]
    let link_sel = Selector::parse("a[href]").unwrap();

    while !queue.is_empty() && visited.len() < max_pages {
        let url_str = queue.remove(0);
        if visited.contains(&url_str) { continue; }
        visited.insert(url_str.clone());

        let short = if url_str.len() > 60 { format!("{}…", &url_str[..59]) } else { url_str.clone() };
        println!("  {GRAY}  [{:>2}/{}] {short}{RESET}", visited.len(), max_pages);
        ui::flush();

        let html = match client.get(&url_str).send().await {
            Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
            _ => { ui::cursor_up(1); continue; }
        };
        ui::cursor_up(1);

        let doc = Html::parse_document(&html);
        for el in doc.select(&link_sel) {
            if let Some(href) = el.value().attr("href") {
                if let Ok(u) = root.join(href) {
                    let h = u.host_str().unwrap_or("").to_string();
                    if h.is_empty() || h == own_host { continue; }
                    external_links.entry(h).or_default().push(url_str.clone());
                    // Also queue internal pages
                } else if !href.starts_with('#') && !href.starts_with("javascript") {
                    if let Ok(abs) = root.join(href) {
                        if abs.host_str().unwrap_or("") == own_host {
                            let key = abs.to_string();
                            if !visited.contains(&key) { queue.push(key); }
                        }
                    }
                }
            }
        }
    }

    println!("  {GRAY}проверяем {} внешних доменов...{RESET}", external_links.len());
    ui::flush();

    let mut dead:  Vec<(String, Vec<String>)> = vec![];
    let mut alive: Vec<String>                = vec![];

    let check_futs: Vec<_> = external_links.iter().map(|(domain, pages)| {
        let client  = client.clone();
        let domain  = domain.clone();
        let pages   = pages.clone();
        async move {
            let url = format!("https://{domain}/");
            match client.head(&url)
                .timeout(std::time::Duration::from_secs(6))
                .send().await
            {
                Ok(r) if r.status().as_u16() < 500 => (domain, true,  pages),
                _                                    => (domain, false, pages),
            }
        }
    }).collect();

    for (domain, is_alive, pages) in futures::future::join_all(check_futs).await {
        if is_alive { alive.push(domain); }
        else        { dead.push((domain, pages)); }
    }

    ui::cursor_up(1);
    println!();
    println!("  {DRED}╔══════════════════ {BRED}{BOLD}necrosis check{RESET}{DRED} ══════════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}страниц скан:{RESET}  {WHITE}{}{RESET}   {GRAY}внешних доменов:{RESET}  {WHITE}{}{RESET}", visited.len(), external_links.len());
    println!("  {DRED}║{RESET}  {BRED}живых:{RESET}  {BRED}{}{RESET}   {RED}мёртвых:{RESET}  {RED}{BOLD}{}{RESET}", alive.len(), dead.len());
    println!("  {DRED}║{RESET}");

    if dead.is_empty() {
        println!("  {DRED}║{RESET}  {BRED}✓{RESET}  мёртвых ссылок не обнаружено");
    } else {
        println!("  {DRED}║{RESET}  {RED}⚠  потенциальные цели для hijacking:{RESET}");
        println!("  {DRED}║{RESET}");
        for (domain, pages) in &dead {
            println!("  {DRED}║{RESET}  {RED}✗{RESET}  {WHITE}{domain}{RESET}");
            println!("  {DRED}║{RESET}     {GRAY}упоминается на {} странице(ах){RESET}", pages.len());
            for page in pages.iter().take(2) {
                println!("  {DRED}║{RESET}     {DRED}▸{RESET}  {DRED}{}{RESET}", shorten(page, 62));
            }
            println!("  {DRED}║{RESET}");
        }
    }

    println!("  {DRED}╚══════════════════════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
