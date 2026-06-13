use anyhow::Result;
use regex::Regex;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use url::Url;
use crate::ui::{self, color::*};

#[derive(Default)]
struct ApiEndpoint {
    method:  String,
    path:    String,
    params:  Vec<String>,
    found_in: String,
}

pub async fn run() -> Result<()> {
    ui::section("api parasite — сборка api-карты");
    println!();

    let target = ui::prompt("target url:");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }

    let root = match Url::parse(&target) {
        Ok(u) => u, Err(e) => { ui::err(&format!("{e}")); return Ok(()); }
    };

    println!("  {GRAY}загружаем страницу...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    let html = match client.get(&target).send().await {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(e) => { ui::cursor_up(1); ui::err(&format!("{e}")); return Ok(()); }
    };
    ui::cursor_up(1);

    let doc    = Html::parse_document(&html);
    let js_sel = Selector::parse("script").unwrap();

    // Regex patterns to find API calls in JavaScript
    let re_fetch   = Regex::new(r#"fetch\s*\(\s*['"`]([^'"`\s]+)['"`]"#).unwrap();
    let re_axios   = Regex::new(r#"axios\s*\.\s*(get|post|put|delete|patch)\s*\(\s*['"`]([^'"`\s]+)['"`]"#).unwrap();
    let re_xhr     = Regex::new(r#"\.open\s*\(\s*['"`](GET|POST|PUT|DELETE|PATCH)['"`]\s*,\s*['"`]([^'"`\s]+)['"`]"#).unwrap();
    let re_jquery  = Regex::new(r#"\$\.\s*(get|post|ajax|getJSON)\s*\(\s*['"`]([^'"`\s]+)['"`]"#).unwrap();
    let re_baseurl = Regex::new(r#"(?i)base_?url\s*[:=]\s*['"`]([^'"`]+)['"`]"#).unwrap();
    let re_apipath = Regex::new(r#"['"`](/api/v?\d*/?[a-z][a-z0-9_/\-]{0,50})['"`]"#).unwrap();

    let mut endpoints: Vec<ApiEndpoint> = vec![];
    let mut base_urls: HashSet<String>  = HashSet::new();
    let mut seen: HashSet<String>       = HashSet::new();

    // Process inline scripts
    let mut all_js: Vec<(String, String)> = vec![]; // (content, source)
    for script_el in doc.select(&js_sel) {
        let content = script_el.text().collect::<String>();
        if !content.trim().is_empty() {
            all_js.push((content, "[inline]".to_string()));
        }
        if let Some(src) = script_el.value().attr("src") {
            if let Ok(abs) = root.join(src) {
                all_js.push(("".to_string(), abs.to_string()));
            }
        }
    }

    println!("  {GRAY}анализируем {} js источников...{RESET}", all_js.len());
    ui::flush();

    // Fetch external JS files
    for (content, src) in &mut all_js {
        if content.is_empty() && src != "[inline]" {
            *content = match client.get(src.as_str())
                .timeout(std::time::Duration::from_secs(8))
                .send().await
            {
                Ok(r) => r.text().await.unwrap_or_default(),
                Err(_) => continue,
            };
        }
    }

    ui::cursor_up(1);

    // Parse all JS
    for (content, src) in &all_js {
        if content.is_empty() { continue; }

        for m in re_baseurl.find_iter(content) {
            let val = re_baseurl.captures(m.as_str())
                .and_then(|c| c.get(1)).map(|m| m.as_str().to_string());
            if let Some(v) = val { base_urls.insert(v); }
        }

        for caps in re_fetch.captures_iter(content) {
            if let Some(path) = caps.get(1) {
                let p = path.as_str().to_string();
                if !seen.contains(&p) && looks_like_api(&p) {
                    seen.insert(p.clone());
                    endpoints.push(ApiEndpoint { method: "FETCH".to_string(), path: p, params: vec![], found_in: src.clone() });
                }
            }
        }

        for caps in re_axios.captures_iter(content) {
            let method = caps.get(1).map(|m| m.as_str().to_uppercase()).unwrap_or_default();
            let path   = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            if !seen.contains(&path) && looks_like_api(&path) {
                seen.insert(path.clone());
                endpoints.push(ApiEndpoint { method, path, params: vec![], found_in: src.clone() });
            }
        }

        for caps in re_xhr.captures_iter(content) {
            let method = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let path   = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            if !seen.contains(&path) && looks_like_api(&path) {
                seen.insert(path.clone());
                endpoints.push(ApiEndpoint { method, path, params: vec![], found_in: src.clone() });
            }
        }

        for caps in re_jquery.captures_iter(content) {
            let method = caps.get(1).map(|m| m.as_str().to_uppercase()).unwrap_or_default();
            let path   = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            if !seen.contains(&path) && looks_like_api(&path) {
                seen.insert(path.clone());
                endpoints.push(ApiEndpoint { method, path, params: vec![], found_in: src.clone() });
            }
        }

        for caps in re_apipath.captures_iter(content) {
            let path = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            if !seen.contains(&path) {
                seen.insert(path.clone());
                endpoints.push(ApiEndpoint { method: "?".to_string(), path, params: vec![], found_in: src.clone() });
            }
        }
    }

    // Group by method
    let mut by_method: HashMap<String, Vec<&ApiEndpoint>> = HashMap::new();
    for ep in &endpoints {
        by_method.entry(ep.method.clone()).or_default().push(ep);
    }

    println!();
    println!("  {DRED}╔═════════════════ {BRED}{BOLD}api parasite{RESET}{DRED} ═════════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}js источников:{RESET}  {WHITE}{}{RESET}   {BRED}эндпоинтов:{RESET}  {BRED}{BOLD}{}{RESET}", all_js.len(), endpoints.len());

    if !base_urls.is_empty() {
        println!("  {DRED}║{RESET}");
        println!("  {DRED}║{RESET}  {RED}── base urls ──{RESET}");
        for b in &base_urls {
            println!("  {DRED}║{RESET}  {WHITE}{}{RESET}", b);
        }
    }

    for method in &["GET","POST","PUT","DELETE","PATCH","FETCH","?"] {
        if let Some(eps) = by_method.get(*method) {
            println!("  {DRED}║{RESET}");
            let mc = match *method { "GET" => BRED, "POST"|"PUT"|"DELETE" => RED, _ => GRAY };
            println!("  {DRED}║{RESET}  {mc}── {method} ({}) ──{RESET}", eps.len());
            for ep in eps.iter().take(20) {
                println!("  {DRED}║{RESET}  {mc}▸{RESET}  {WHITE}{:<50}{RESET}  {GRAY}{}{RESET}",
                    shorten(&ep.path, 50), shorten(&ep.found_in, 16));
            }
            if eps.len() > 20 {
                println!("  {DRED}║{RESET}  {GRAY}  … ещё {}{RESET}", eps.len()-20);
            }
        }
    }

    println!("  {DRED}║{RESET}");
    println!("  {DRED}╚══════════════════════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

fn looks_like_api(path: &str) -> bool {
    if path.len() < 2 { return false; }
    if path.ends_with(".js") || path.ends_with(".css") || path.ends_with(".png") { return false; }
    path.starts_with('/') || path.starts_with("http") || path.contains("/api") || path.contains("/v1") || path.contains("/v2")
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
