use anyhow::Result;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("http methods — разрешённые методы");
    println!();

    let target = ui::prompt("target url:");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }

    println!("  {GRAY}отправляем OPTIONS...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let methods_to_test = ["GET","POST","PUT","DELETE","PATCH","HEAD","OPTIONS","TRACE","CONNECT"];

    // Try OPTIONS first
    let options_result = client.request(reqwest::Method::OPTIONS, &target)
        .send().await;

    ui::cursor_up(1);
    println!();

    let mut allowed_from_header: Vec<String> = vec![];
    if let Ok(resp) = &options_result {
        if let Some(allow) = resp.headers().get("allow").and_then(|v| v.to_str().ok()) {
            allowed_from_header = allow.split(',').map(|s| s.trim().to_uppercase()).collect();
        }
    }

    println!("  {DRED}╔════════════════ {BRED}{BOLD}http methods{RESET}{DRED} ════════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}url:{RESET}  {WHITE}{}{RESET}", shorten(&target, 48));
    println!("  {DRED}║{RESET}");

    if !allowed_from_header.is_empty() {
        println!("  {DRED}║{RESET}  {GRAY}allow header:{RESET}  {WHITE}{}{RESET}", allowed_from_header.join(", "));
        println!("  {DRED}║{RESET}");
    }

    println!("  {DRED}║{RESET}  {GRAY}прямое тестирование методов:{RESET}");
    println!("  {DRED}║{RESET}");

    for method_str in &methods_to_test {
        let method = reqwest::Method::from_bytes(method_str.as_bytes())
            .unwrap_or(reqwest::Method::GET);
        let result = client.request(method, &target)
            .timeout(std::time::Duration::from_secs(6))
            .send().await;

        let (symbol, color, info) = match result {
            Ok(r) => {
                let s = r.status().as_u16();
                match s {
                    200..=299 => ("✓", BRED, format!("{s} ok")),
                    301..=308 => ("→", RED,  format!("{s} redirect")),
                    400..=403 => ("✗", GRAY, format!("{s} denied")),
                    405       => ("⊘", GRAY, format!("{s} not allowed")),
                    _         => ("?", GRAY, format!("{s}")),
                }
            }
            Err(_) => ("✗", GRAY, "error / timeout".to_string()),
        };

        let danger = matches!(*method_str, "PUT" | "DELETE" | "TRACE" | "CONNECT");
        let nc = if danger && color == BRED { RED } else { color };
        println!("  {DRED}║{RESET}  {nc}{symbol}{RESET}  {nc}{method_str:<8}{RESET}  {GRAY}{info}{RESET}");
    }

    println!("  {DRED}║{RESET}");
    println!("  {DRED}╚══════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
