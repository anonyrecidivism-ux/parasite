use anyhow::Result;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("replicate — сохранить страницу локально");
    println!();

    let target  = ui::prompt("target url:");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }
    let outfile = ui::prompt_default("сохранить в файл:", "page.html");

    println!("  {GRAY}загружаем...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; parasite/1.0)")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let resp = match client.get(&target).send().await {
        Ok(r)  => r,
        Err(e) => { ui::cursor_up(1); ui::err(&format!("{e}")); return Ok(()); }
    };

    let status   = resp.status().as_u16();
    let final_url = resp.url().to_string();
    let ct = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("?")
        .to_string();
    let body = resp.text().await.unwrap_or_default();
    let size = body.len();

    tokio::fs::write(&outfile, &body).await?;
    ui::cursor_up(1);

    println!();
    println!("  {BRED}✓{RESET}  страница сохранена");
    println!();
    ui::kv("url:", &shorten(&final_url, 56));
    ui::kv("файл:", &outfile);
    ui::kv("статус:", &status.to_string());
    ui::kv("content-type:", &ct);
    ui::kv("размер:", &format!("{} байт ({:.1} кб)", size, size as f64 / 1024.0));
    ui::divider();
    Ok(())
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
