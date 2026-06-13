use anyhow::Result;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("header dump — все заголовки ответа");
    println!();

    let target = ui::prompt("target url:");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }

    let method_s = ui::prompt_default("метод (GET/HEAD):", "HEAD");
    println!("  {GRAY}отправляем запрос...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let method = if method_s.to_uppercase() == "GET" {
        reqwest::Method::GET
    } else {
        reqwest::Method::HEAD
    };

    let resp = match client.request(method, &target).send().await {
        Ok(r)  => r,
        Err(e) => { ui::cursor_up(1); ui::err(&format!("{e}")); return Ok(()); }
    };

    let status  = resp.status();
    let version = format!("{:?}", resp.version());
    let hdrs    = resp.headers().clone();

    ui::cursor_up(1);
    println!();
    println!("  {DRED}┌──── {BRED}{BOLD}headers @ {}{RESET}", shorten(&target, 50));
    println!("  {DRED}│{RESET}  {GRAY}статус:{RESET}  {BRED}{BOLD}{status}{RESET}   {GRAY}версия:{RESET}  {WHITE}{version}{RESET}");
    println!("  {DRED}│{RESET}  {GRAY}всего заголовков:{RESET}  {BRED}{}{RESET}", hdrs.len());
    println!("  {DRED}│{RESET}");

    // Security-relevant headers highlighted
    let security = ["strict-transport-security","content-security-policy","x-frame-options",
                    "x-content-type-options","x-xss-protection","referrer-policy",
                    "permissions-policy","cross-origin-opener-policy","cross-origin-embedder-policy"];

    let mut sorted_hdrs: Vec<(String, String)> = hdrs.iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("?").to_string()))
        .collect();
    sorted_hdrs.sort_by(|a, b| a.0.cmp(&b.0));

    for (k, v) in &sorted_hdrs {
        let is_sec = security.contains(&k.as_str());
        let (kc, vc) = if is_sec { (BRED, WHITE) } else { (DRED, GRAY) };
        let val = if v.len() > 58 { format!("{}…", &v[..57]) } else { v.clone() };
        let sec_mark = if is_sec { format!("{RED}●{RESET} ") } else { "  ".to_string() };
        println!("  {DRED}│{RESET}  {sec_mark}{kc}{k:<36}{RESET}  {vc}{val}{RESET}");
    }

    println!("  {DRED}│{RESET}");
    println!("  {DRED}└──────────────────────────────────────────────────────────────────────{RESET}");
    ui::divider();
    Ok(())
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
