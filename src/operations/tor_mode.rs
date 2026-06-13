use anyhow::Result;
use std::process::Stdio;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("tor mode — анонимный прокси");
    println!();

    let tor_port: u16 = 9050;
    let control_port: u16 = 9051;

    println!("  {GRAY}проверяем tor...{RESET}");

    // Check if tor binary exists
    let tor_exists = tokio::process::Command::new("which")
        .arg("tor")
        .output().await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !tor_exists {
        ui::cursor_up(1);
        println!();
        println!("  {DRED}╔══════════════════ {BRED}{BOLD}tor mode{RESET}{DRED} ══════════════════╗{RESET}");
        println!("  {DRED}║{RESET}");
        println!("  {DRED}║{RESET}  {RED}✗  tor не установлен{RESET}");
        println!("  {DRED}║{RESET}");
        println!("  {DRED}║{RESET}  {GRAY}установите:{RESET}");
        println!("  {DRED}║{RESET}  {WHITE}  sudo apt install tor{RESET}         {GRAY}(Debian/Ubuntu){RESET}");
        println!("  {DRED}║{RESET}  {WHITE}  sudo pacman -S tor{RESET}            {GRAY}(Arch/Manjaro){RESET}");
        println!("  {DRED}║{RESET}  {WHITE}  sudo dnf install tor{RESET}          {GRAY}(Fedora/RHEL){RESET}");
        println!("  {DRED}║{RESET}  {WHITE}  brew install tor{RESET}              {GRAY}(macOS){RESET}");
        println!("  {DRED}║{RESET}");
        println!("  {DRED}╚══════════════════════════════════════════════════════════════╝{RESET}");
        return Ok(());
    }

    // Check if tor is already listening on 9050
    let already_running = is_port_open(tor_port).await;

    let mut child_handle: Option<tokio::process::Child> = None;

    if already_running {
        ui::cursor_up(1);
        println!("  {BRED}✓{RESET}  {GRAY}tor уже запущен на порту {tor_port}{RESET}");
    } else {
        ui::cursor_up(1);
        println!("  {GRAY}запускаем tor...{RESET}");

        let child = tokio::process::Command::new("tor")
            .arg("--quiet")
            .arg("--SocksPort")
            .arg(tor_port.to_string())
            .arg("--ControlPort")
            .arg(control_port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match child {
            Ok(c) => { child_handle = Some(c); }
            Err(e) => {
                ui::cursor_up(1);
                ui::err(&format!("не удалось запустить tor: {e}"));
                return Ok(());
            }
        }

        // Wait for port to become available (max 30 seconds)
        let mut ready = false;
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if is_port_open(tor_port).await {
                ready = true;
                break;
            }
        }
        ui::cursor_up(1);

        if !ready {
            println!("  {RED}✗{RESET}  tor не смог запуститься в течение 30 сек");
            if let Some(mut c) = child_handle { let _ = c.kill().await; }
            return Ok(());
        }

        println!("  {BRED}✓{RESET}  {GRAY}tor запущен{RESET}");
    }

    // Get real IP
    let real_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let real_ip = fetch_ip(&real_client).await.unwrap_or("неизвестно".to_string());

    // Get TOR IP
    let tor_proxy = format!("socks5://127.0.0.1:{tor_port}");
    let tor_client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(&tor_proxy)?)
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("Mozilla/5.0")
        .build()?;

    println!("  {GRAY}проверяем внешний ip через tor...{RESET}");
    let tor_ip = fetch_ip(&tor_client).await.unwrap_or("недоступно".to_string());
    ui::cursor_up(1);

    let anonymized = real_ip != tor_ip && tor_ip != "недоступно";

    println!();
    println!("  {DRED}╔══════════════════ {BRED}{BOLD}tor mode{RESET}{DRED} ══════════════════╗{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}реальный ip:{RESET}    {WHITE}{real_ip}{RESET}");
    println!("  {DRED}║{RESET}  {DRED}tor ip:{RESET}         {BRED}{BOLD}{tor_ip}{RESET}");
    println!("  {DRED}║{RESET}");

    if anonymized {
        println!("  {DRED}║{RESET}  {BRED}✓{RESET}  {BRED}анонимизация активна{RESET}");
    } else {
        println!("  {DRED}║{RESET}  {RED}✗{RESET}  {RED}tor недоступен или ip совпадает{RESET}");
    }

    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {RED}── настройка браузера ──{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}firefox:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}Параметры → Сеть → Настроить...{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}SOCKS5:{RESET}  {BRED}127.0.0.1{RESET}  порт {BRED}{tor_port}{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}☑ Proxy DNS when using SOCKS5{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}chrome:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}google-chrome --proxy-server=socks5://127.0.0.1:{tor_port}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}curl:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}curl --socks5-hostname 127.0.0.1:{tor_port} https://example.com{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}proxychains:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}echo 'socks5 127.0.0.1 {tor_port}' >> /etc/proxychains.conf{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}proxychains <команда>{RESET}");
    println!("  {DRED}║{RESET}");

    if child_handle.is_some() {
        println!("  {DRED}║{RESET}  {GRAY}нажмите enter чтобы остановить tor и выйти...{RESET}");
    } else {
        println!("  {DRED}║{RESET}  {GRAY}нажмите enter чтобы выйти (tor останется работать)...{RESET}");
    }
    println!("  {DRED}╚══════════════════════════════════════════════════════════════╝{RESET}");

    // Wait for Enter
    let mut buf = String::new();
    let _ = std::io::stdin().read_line(&mut buf);

    // Kill tor only if we launched it
    if let Some(mut c) = child_handle {
        println!("  {GRAY}останавливаем tor...{RESET}");
        let _ = c.kill().await;
        let _ = c.wait().await;
        println!("  {BRED}✓{RESET}  {GRAY}tor остановлен{RESET}");
    }

    ui::divider();
    Ok(())
}

async fn is_port_open(port: u16) -> bool {
    tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
        .await.is_ok()
}

async fn fetch_ip(client: &reqwest::Client) -> Option<String> {
    let endpoints = ["https://api.ipify.org", "https://ifconfig.me/ip", "https://icanhazip.com"];
    for ep in &endpoints {
        if let Ok(r) = client.get(*ep).send().await {
            if let Ok(t) = r.text().await {
                let ip = t.trim().to_string();
                if !ip.is_empty() { return Some(ip); }
            }
        }
    }
    None
}
