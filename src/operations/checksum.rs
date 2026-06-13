use anyhow::Result;
use md5::Md5;
use sha1::Sha1;
use sha2::{Sha256, Sha512, Digest};
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("checksum file — хэш файла");
    println!();

    let path = ui::prompt("путь к файлу:");
    if path.is_empty() { ui::err("путь обязателен"); return Ok(()); }

    println!("  {GRAY}читаем файл...{RESET}");

    let bytes = match tokio::fs::read(&path).await {
        Ok(b)  => b,
        Err(e) => { ui::cursor_up(1); ui::err(&format!("{e}")); return Ok(()); }
    };

    let size = bytes.len();

    let md5_hash    = format!("{:x}", Md5::digest(&bytes));
    let sha1_hash   = format!("{:x}", Sha1::digest(&bytes));
    let sha256_hash = format!("{:x}", Sha256::digest(&bytes));
    let sha512_hash = format!("{:x}", Sha512::digest(&bytes));

    ui::cursor_up(1);

    println!();
    println!("  {DRED}╔════════════ {BRED}{BOLD}checksum{RESET}{DRED} ════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}файл:{RESET}  {WHITE}{path}{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}размер:{RESET}  {WHITE}{size} байт ({:.2} кб){RESET}", size as f64 / 1024.0);
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {RED}md5:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}{md5_hash}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {RED}sha1:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}{sha1_hash}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {RED}sha256:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}{sha256_hash}{RESET}");
    println!("  {DRED}║{RESET}");
    println!("  {DRED}║{RESET}  {RED}sha512:{RESET}");
    println!("  {DRED}║{RESET}    {WHITE}{sha512_hash}{RESET}");
    println!("  {DRED}║{RESET}");

    // compare with known hash
    let compare = ui::prompt_default("сравнить с известным хэшем (enter — пропустить):", "");
    if !compare.is_empty() {
        let cmp_clean = compare.trim().to_lowercase();
        let matches = cmp_clean == md5_hash || cmp_clean == sha1_hash
                   || cmp_clean == sha256_hash || cmp_clean == sha512_hash;
        if matches {
            println!("  {BRED}✓  хэш совпадает{RESET}");
        } else {
            println!("  {RED}✗  хэш НЕ совпадает — файл изменён или неверный хэш{RESET}");
        }
    }

    println!("  {DRED}╚════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}
