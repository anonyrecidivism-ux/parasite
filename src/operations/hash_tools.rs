use anyhow::Result;
use md5::Md5;
use sha1::Sha1;
use sha2::{Sha256, Sha512, Digest};
use crate::ui::{self, color::*};

pub async fn run_generate() -> Result<()> {
    ui::section("hash generate — вычислить хэши");
    println!();

    let input = ui::prompt("строка для хэширования:");
    if input.is_empty() { ui::warn("пустая строка"); return Ok(()); }

    let bytes = input.as_bytes();

    let md5_hash    = format!("{:x}", Md5::digest(bytes));
    let sha1_hash   = format!("{:x}", Sha1::digest(bytes));
    let sha256_hash = format!("{:x}", Sha256::digest(bytes));
    let sha512_hash = format!("{:x}", Sha512::digest(bytes));

    println!();
    println!("  {DRED}╔═══════════ {BRED}{BOLD}hash generate{RESET}{DRED} ═══════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}input:  {WHITE}{}{RESET}", shorten(&input, 50));
    println!("  {DRED}║{RESET}  {GRAY}bytes:  {WHITE}{}{RESET}", bytes.len());
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {RED}md5    {RESET}  {WHITE}{md5_hash}{RESET}");
    println!("  {DRED}║{RESET}  {RED}sha1   {RESET}  {WHITE}{sha1_hash}{RESET}");
    println!("  {DRED}║{RESET}  {RED}sha256 {RESET}  {WHITE}{sha256_hash}{RESET}");
    println!("  {DRED}║{RESET}  {RED}sha512 {RESET}  {WHITE}{sha512_hash}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}╚═══════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

pub async fn run_identify() -> Result<()> {
    ui::section("hash identify — тип хэша");
    println!();

    let hash = ui::prompt("хэш для анализа:");
    let hash = hash.trim();
    if hash.is_empty() { ui::warn("пустой ввод"); return Ok(()); }

    let is_hex = hash.chars().all(|c| c.is_ascii_hexdigit());
    let len    = hash.len();

    let (name, bits, color) = match (len, is_hex) {
        (32,  true)  => ("MD5",                "128", BRED),
        (40,  true)  => ("SHA-1",              "160", RED),
        (56,  true)  => ("SHA-224",            "224", RED),
        (64,  true)  => ("SHA-256 / SHA3-256", "256", BRED),
        (96,  true)  => ("SHA-384",            "384", RED),
        (128, true)  => ("SHA-512 / SHA3-512", "512", BRED),
        (60,  false) => ("bcrypt",             "?",   RED),
        _ if hash.starts_with("$2b$") || hash.starts_with("$2a$") =>
                        ("bcrypt",             "?",   RED),
        _ if hash.starts_with("$argon2") =>
                        ("Argon2",             "?",   BRED),
        _ if hash.starts_with("$pbkdf2") =>
                        ("PBKDF2",             "?",   BRED),
        _ if hash.starts_with("{SHA}") =>
                        ("LDAP SHA",           "?",   GRAY),
        _              => ("неизвестен",        "?",   GRAY),
    };

    println!();
    println!("  {DRED}╔═════════════ {BRED}{BOLD}hash identify{RESET}{DRED} ═════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}hash:{RESET}  {WHITE}{}{RESET}", shorten(hash, 54));
    println!("  {DRED}║{RESET}  {GRAY}длина символов:{RESET}  {WHITE}{len}{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}hex-строка:{RESET}  {WHITE}{is_hex}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {color}{BOLD}алгоритм: {name}{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}биты: {bits}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}╚═══════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
