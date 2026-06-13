use anyhow::Result;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("spawn larvae — генератор вариантов");
    println!();

    let word    = ui::prompt("базовое слово (напр. admin):");
    if word.is_empty() { ui::err("слово обязательно"); return Ok(()); }
    let outfile = ui::prompt_default("сохранить в файл (enter — только экран):", "");

    let mut variants: Vec<String> = vec![];

    // base
    variants.push(word.clone());
    variants.push(word.to_uppercase());
    variants.push(capitalize(&word));

    // common suffixes
    let suffixes = ["1","2","3","123","1234","12345","2024","2025","!","@","#",
                    "_1","_2","_admin","_test","_dev","_prod","_old","_new",
                    ".php",".html",".asp",".aspx",".jsp",".txt",".bak",".zip"];
    for suf in &suffixes {
        variants.push(format!("{word}{suf}"));
        variants.push(format!("{}{suf}", word.to_uppercase()));
    }

    // common prefixes
    let prefixes = ["test_","dev_","admin_","old_","new_","backup_","_"];
    for pre in &prefixes {
        variants.push(format!("{pre}{word}"));
    }

    // leet speak
    let leet = leet_speak(&word);
    if leet != word { variants.push(leet.clone()); }

    // reversed
    variants.push(word.chars().rev().collect::<String>());

    // common separators
    for sep in &["-","_",".","/"] {
        variants.push(format!("{word}{sep}admin"));
        variants.push(format!("admin{sep}{word}"));
    }

    // URL-path variants
    variants.push(format!("/{word}"));
    variants.push(format!("/{word}/"));
    variants.push(format!("/api/{word}"));
    variants.push(format!("/api/v1/{word}"));

    variants.dedup();
    let total = variants.len();

    if !outfile.is_empty() {
        let content = variants.join("\n");
        tokio::fs::write(&outfile, content).await?;
    }

    println!();
    println!("  {DRED}╔════════════ {BRED}{BOLD}spawn larvae{RESET}{DRED} ════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}слово:{RESET}  {WHITE}{word}{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}вариантов:{RESET}  {BRED}{BOLD}{total}{RESET}");
    if !outfile.is_empty() {
        println!("  {DRED}║{RESET}  {BRED}✓{RESET}  сохранено в {WHITE}{outfile}{RESET}");
    }
    println!("  {DRED}║{RESET}");

    for (i, v) in variants.iter().enumerate().take(50) {
        println!("  {DRED}║{RESET}  {GRAY}{:>3}{RESET}  {WHITE}{v}{RESET}", i+1);
    }
    if total > 50 {
        println!("  {DRED}║{RESET}  {GRAY}  … ещё {} вариантов{RESET}", total-50);
    }

    println!("  {DRED}║{RESET}");
    println!("  {DRED}╚═══════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None    => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn leet_speak(s: &str) -> String {
    s.chars().map(|c| match c {
        'a' | 'A' => '4', 'e' | 'E' => '3', 'i' | 'I' => '1',
        'o' | 'O' => '0', 's' | 'S' => '5', 't' | 'T' => '7',
        c          => c,
    }).collect()
}
