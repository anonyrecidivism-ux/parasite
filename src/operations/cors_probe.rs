use anyhow::Result;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("cors probe — misconfiguration check");
    println!();

    let target = ui::prompt("target url:");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }

    println!("  {GRAY}тестируем cors...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let test_origins = [
        "https://evil.parasite.com",
        "null",
        "https://localhost",
    ];

    ui::cursor_up(1);
    println!();
    println!("  {DRED}╔══════════════════ {BRED}{BOLD}cors probe{RESET}{DRED} ═══════════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}target:{RESET}  {WHITE}{}{RESET}", shorten(&target, 50));
    println!("  {DRED}║{RESET}");

    let mut vulns = 0usize;

    for origin in &test_origins {
        let resp = client.get(&target)
            .header("Origin", *origin)
            .header("Access-Control-Request-Method", "GET")
            .send().await;

        match resp {
            Ok(r) => {
                let acao = r.headers().get("access-control-allow-origin")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                let acac = r.headers().get("access-control-allow-credentials")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());

                let vuln = match (&acao, &acac) {
                    (Some(ao), Some(cred)) if ao == *origin && cred == "true" => {
                        vulns += 1; true
                    }
                    (Some(ao), _) if ao == "*" => { vulns += 1; true }
                    (Some(ao), _) if ao == *origin => { vulns += 1; true }
                    _ => false,
                };

                let (sym, col) = if vuln { ("✗ VULN", RED) } else { ("✓ safe", GRAY) };
                let ao_str = acao.as_deref().unwrap_or("—");
                let ac_str = acac.as_deref().unwrap_or("—");

                println!("  {DRED}║{RESET}  {col}{sym}{RESET}  {GRAY}origin:{RESET} {WHITE}{origin}");
                println!("  {DRED}║{RESET}       {GRAY}acao: {col}{ao_str}{RESET}   {GRAY}acac:{RESET} {ac_str}");
                println!("  {DRED}║{RESET}");
            }
            Err(e) => {
                println!("  {DRED}║{RESET}  {GRAY}⊘  {origin} — ошибка: {e}{RESET}");
                println!("  {DRED}║{RESET}");
            }
        }
    }

    let (verdict_c, verdict) = if vulns > 0 {
        (RED, format!("⚠  обнаружено {vulns} потенциальных уязвимостей cors"))
    } else {
        (BRED, "✓  cors настроен корректно".to_string())
    };

    println!("  {DRED}║{RESET}  {verdict_c}{verdict}{RESET}");
    println!("  {DRED}╚═══════════════════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
