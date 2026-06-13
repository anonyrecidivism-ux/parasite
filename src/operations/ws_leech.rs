use anyhow::Result;
use futures::{SinkExt, StreamExt};
use regex::Regex;
use scraper::{Html, Selector};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url as ReqUrl;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("websocket leech — перехват трафика");
    println!();

    let target = ui::prompt("target url (страница с websocket):");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }

    let listen_secs: u64 = ui::prompt_default("слушать секунд:", "30")
        .parse().unwrap_or(30);

    println!("  {GRAY}анализируем страницу...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    let html = match client.get(&target).send().await {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(e) => { ui::cursor_up(1); ui::err(&format!("{e}")); return Ok(()); }
    };
    ui::cursor_up(1);

    // Find WebSocket URLs in JS code and HTML
    let root   = ReqUrl::parse(&target).unwrap_or(ReqUrl::parse("http://localhost").unwrap());
    let re_ws  = Regex::new(r#"(wss?://[^\s'"<>]+)"#).unwrap();
    let re_rel = Regex::new(r#"(?:new WebSocket\s*\(\s*['"`])(/[^'"`\s]+)['"`]"#).unwrap();

    let doc    = Html::parse_document(&html);
    let js_sel = Selector::parse("script").unwrap();

    let mut ws_urls: Vec<String> = vec![];

    // Direct wss:// matches in HTML
    for m in re_ws.find_iter(&html) {
        let url = m.as_str().trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string();
        if !ws_urls.contains(&url) { ws_urls.push(url); }
    }

    // Relative WebSocket paths
    for caps in re_rel.captures_iter(&html) {
        if let Some(path) = caps.get(1) {
            let ws_scheme = if root.scheme() == "https" { "wss" } else { "ws" };
            let abs = format!("{}://{}{}", ws_scheme, root.host_str().unwrap_or(""), path.as_str());
            if !ws_urls.contains(&abs) { ws_urls.push(abs); }
        }
    }

    // Also scan inline scripts
    for script_el in doc.select(&js_sel) {
        let content = script_el.text().collect::<String>();
        for m in re_ws.find_iter(&content) {
            let url = m.as_str().trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string();
            if !ws_urls.contains(&url) { ws_urls.push(url); }
        }
        for caps in re_rel.captures_iter(&content) {
            if let Some(path) = caps.get(1) {
                let ws_scheme = if root.scheme() == "https" { "wss" } else { "ws" };
                let abs = format!("{}://{}{}", ws_scheme, root.host_str().unwrap_or(""), path.as_str());
                if !ws_urls.contains(&abs) { ws_urls.push(abs); }
            }
        }
    }

    println!();
    println!("  {DRED}╔══════════════ {BRED}{BOLD}websocket leech{RESET}{DRED} ══════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}найдено websocket url:{RESET}  {BRED}{BOLD}{}{RESET}", ws_urls.len());
    println!("  {DRED}║{RESET}");

    if ws_urls.is_empty() {
        println!("  {DRED}║{RESET}  {GRAY}websocket соединений не обнаружено{RESET}");
        println!("  {DRED}║{RESET}");

        // manual mode
        let manual = ui::prompt_default("ввести ws url вручную (wss://...):", "");
        if !manual.is_empty() { ws_urls.push(manual); }
    }

    for ws_url in &ws_urls {
        println!("  {DRED}║{RESET}  {GRAY}url:{RESET}  {WHITE}{}{RESET}", shorten(ws_url, 58));
        println!("  {DRED}║{RESET}  {GRAY}подключаемся...{RESET}");
        ui::flush();

        match connect_to_ws(ws_url, listen_secs).await {
            Ok(messages) => {
                println!("  {DRED}║{RESET}  {BRED}✓{RESET}  перехвачено {BRED}{}{RESET} сообщений", messages.len());
                println!("  {DRED}║{RESET}");
                for (i, msg) in messages.iter().enumerate().take(30) {
                    let short = if msg.len() > 66 { format!("{}…", &msg[..65]) } else { msg.clone() };
                    println!("  {DRED}║{RESET}  {GRAY}[{:>3}]{RESET}  {WHITE}{short}{RESET}", i+1);
                }
                if messages.len() > 30 {
                    println!("  {DRED}║{RESET}  {GRAY}  … ещё {} сообщений{RESET}", messages.len()-30);
                }
            }
            Err(e) => {
                println!("  {DRED}║{RESET}  {RED}✗{RESET}  ошибка: {RED}{e}{RESET}");
            }
        }
        println!("  {DRED}║{RESET}");
    }

    println!("  {DRED}╚══════════════════════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

async fn connect_to_ws(ws_url: &str, secs: u64) -> anyhow::Result<Vec<String>> {
    let (ws_stream, _) = connect_async(ws_url).await
        .map_err(|e| anyhow::anyhow!("ws connect: {e}"))?;

    let (mut write, mut read) = ws_stream.split();

    let mut messages: Vec<String> = vec![];
    let timeout = tokio::time::sleep(std::time::Duration::from_secs(secs));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = &mut timeout => break,
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(t)))   => messages.push(t.to_string()),
                    Some(Ok(Message::Binary(b))) => messages.push(format!("[binary {} bytes]", b.len())),
                    Some(Ok(Message::Ping(_)))   => { let _ = write.send(Message::Pong(vec![])).await; }
                    Some(Ok(Message::Close(_)))  => break,
                    None | Some(Err(_))          => break,
                    _ => {}
                }
            }
        }
    }

    Ok(messages)
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
