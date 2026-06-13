use anyhow::Result;
use base64::{Engine, engine::general_purpose};
use crate::ui::{self, color::*};

pub async fn run_encode() -> Result<()> {
    ui::section("encode — кодирование");
    println!();

    let input = ui::prompt("строка для кодирования:");
    if input.is_empty() { ui::warn("пустой ввод"); return Ok(()); }

    let b64_std    = general_purpose::STANDARD.encode(&input);
    let b64_url    = general_purpose::URL_SAFE_NO_PAD.encode(&input);
    let hex        = to_hex(input.as_bytes());
    let url_enc    = url_encode(&input);
    let html_ent   = html_encode(&input);

    println!();
    println!("  {DRED}╔══════════════ {BRED}{BOLD}encode{RESET}{DRED} ══════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}input:       {WHITE}{}{RESET}", shorten(&input, 50));
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {RED}base64:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}{b64_std}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {RED}base64 url-safe:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}{b64_url}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {RED}hex:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}{hex}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {RED}url encoding:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}{url_enc}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {RED}html entities:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}{html_ent}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}╚═══════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

pub async fn run_decode() -> Result<()> {
    ui::section("decode — декодирование");
    println!();

    let input = ui::prompt("строка для декодирования:");
    if input.is_empty() { ui::warn("пустой ввод"); return Ok(()); }

    println!();
    println!("  {DRED}╔══════════════ {BRED}{BOLD}decode{RESET}{DRED} ══════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}input: {WHITE}{}{RESET}", shorten(&input, 54));
    println!("  {DRED}║{RESET}");

    // Try base64
    let b64 = general_purpose::STANDARD.decode(&input)
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(&input))
        .or_else(|_| general_purpose::URL_SAFE.decode(&input));
    match b64 {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes).to_string();
            println!("  {DRED}║{RESET}  {BRED}✓ base64:{RESET}");
            println!("  {DRED}║{RESET}    {WHITE}{}{RESET}", shorten(&text, 66));
            println!("  {DRED}║{RESET}");
        }
        Err(_) => println!("  {DRED}║{RESET}  {GRAY}✗ не base64{RESET}"),
    }

    // Try hex
    if input.len() % 2 == 0 && input.chars().all(|c| c.is_ascii_hexdigit()) {
        let bytes: Vec<u8> = (0..input.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&input[i..i+2], 16).unwrap_or(0))
            .collect();
        let text = String::from_utf8_lossy(&bytes).to_string();
        println!("  {DRED}║{RESET}  {BRED}✓ hex:{RESET}");
        println!("  {DRED}║{RESET}    {WHITE}{}{RESET}", shorten(&text, 66));
        println!("  {DRED}║{RESET}");
    } else {
        println!("  {DRED}║{RESET}  {GRAY}✗ не hex{RESET}");
    }

    // Try URL decode
    let url_dec = url_decode(&input);
    if url_dec != input {
        println!("  {DRED}║{RESET}  {BRED}✓ url-decode:{RESET}");
        println!("  {DRED}║{RESET}    {WHITE}{}{RESET}", shorten(&url_dec, 66));
        println!("  {DRED}║{RESET}");
    } else {
        println!("  {DRED}║{RESET}  {GRAY}✗ url-decode без изменений{RESET}");
    }

    println!("  {DRED}╚═══════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn url_encode(s: &str) -> String {
    s.chars().flat_map(|c| {
        if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
            vec![c]
        } else {
            format!("%{:02X}", c as u32).chars().collect()
        }
    }).collect()
}

fn url_decode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().unwrap_or('0');
            let h2 = chars.next().unwrap_or('0');
            if let Ok(b) = u8::from_str_radix(&format!("{h1}{h2}"), 16) {
                result.push(b as char);
            } else {
                result.push('%'); result.push(h1); result.push(h2);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

fn html_encode(s: &str) -> String {
    s.chars().map(|c| match c {
        '&'  => "&amp;".to_string(),
        '<'  => "&lt;".to_string(),
        '>'  => "&gt;".to_string(),
        '"'  => "&quot;".to_string(),
        '\'' => "&#x27;".to_string(),
        c    => c.to_string(),
    }).collect()
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
