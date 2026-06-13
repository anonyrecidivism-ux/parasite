/// MAP COLONY — картографировать структуру сайта
/// Делает ограниченный обход и строит дерево путей.

use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};
use url::Url;

use crate::parser::{normalize_url, HtmlParser};
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("MAP COLONY — карта территории");
    println!();

    let target   = ui::prompt("Target URL (корень):");
    if target.is_empty() { ui::err("URL обязателен"); return Ok(()); }

    let max_s    = ui::prompt_default("Макс. страниц для картографии:", "80");
    let depth_s  = ui::prompt_default("Макс. глубина:", "3");

    let max_pages: usize = max_s.parse().unwrap_or(80);
    let max_depth: u32   = depth_s.parse().unwrap_or(3);

    let root = match Url::parse(&target) {
        Ok(u) => u,
        Err(e) => { ui::err(&format!("{e}")); return Ok(()); }
    };

    let host = root.host_str().unwrap_or("").to_string();

    println!();
    println!("  {GRAY}Картографируем колонию...{RESET}");
    println!();

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .gzip(true)
        .build()?;

    // BFS-обход
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(Url, u32)> = VecDeque::new();
    // path → [children paths]
    let mut tree: HashMap<String, Vec<String>> = HashMap::new();
    let mut page_count = 0usize;

    queue.push_back((root.clone(), 0));

    while let Some((url, depth)) = queue.pop_front() {
        if visited.len() >= max_pages || depth > max_depth { continue; }
        let key = url.to_string();
        if visited.contains(&key) { continue; }
        visited.insert(key.clone());

        // Вывести прогресс
        ui::cursor_up(1);
        let short = if key.len() > 65 { format!("{}…", &key[..64]) } else { key.clone() };
        println!("  {GRAY}  [{:>3}] {short}{RESET}", visited.len());

        // Получить страницу
        let html = match client.get(url.as_str()).send().await {
            Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
            _ => continue,
        };

        let path = url.path().to_string();
        page_count += 1;

        let links = HtmlParser::extract_links(&html, &url);
        let mut children = vec![];

        for link in links {
            let norm = normalize_url(link);
            if norm.host_str().unwrap_or("") != host { continue; }
            let child_path = norm.path().to_string();
            let child_key  = norm.to_string();
            if !visited.contains(&child_key) && depth + 1 <= max_depth {
                children.push(child_path.clone());
                queue.push_back((norm, depth + 1));
            }
        }
        children.dedup();
        tree.insert(path, children);
    }

    ui::cursor_up(1);
    println!("  {BRED}✓{RESET}  Обработано {BRED}{BOLD}{page_count}{RESET} страниц");
    println!();

    // ── Вывести дерево путей ─────────────────────────────────────────────────
    println!("  {DRED}┌─── {BRED}{BOLD}SITE MAP @ {host}{RESET}{DRED} ─────────────────────────────────────┐{RESET}");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");

    // Отсортировать пути по алфавиту, вывести дерево
    let mut paths: Vec<String> = tree.keys().cloned().collect();
    paths.sort();

    // Показать только первый уровень с количеством дочерних
    let mut shown = 0usize;
    for path in &paths {
        if shown >= 40 { break; }
        let children = tree.get(path).map(|c| c.len()).unwrap_or(0);
        let depth_indent = path.matches('/').count().saturating_sub(1);
        let indent = "  ".repeat(depth_indent);
        let arrow = if children > 0 { format!("{GRAY}[{children}→]{RESET}") } else { String::new() };
        let p = if path.len() > 50 { format!("{}…", &path[..49]) } else { path.clone() };
        println!(
            "  {DRED}│{RESET}  {RED}{indent}▸{RESET}  {WHITE}{p:<52}{RESET}  {arrow}  {DRED}│{RESET}"
        );
        shown += 1;
    }

    if paths.len() > 40 {
        println!("  {DRED}│{RESET}  {GRAY}  … и ещё {} путей{RESET}{: <48}{DRED}│{RESET}", paths.len()-40, "");
    }

    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");
    println!("  {DRED}│{RESET}  {GRAY}Итого:{RESET}  {BRED}{BOLD}{}{RESET} уникальных путей{: <38}{DRED}│{RESET}", paths.len(), "");
    println!("  {DRED}└──────────────────────────────────────────────────────────────────────┘{RESET}");

    ui::divider();
    Ok(())
}
