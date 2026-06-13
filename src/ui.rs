pub mod color {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD:  &str = "\x1b[1m";
    pub const RED:   &str = "\x1b[31m";
    pub const BRED:  &str = "\x1b[91m";
    pub const DRED:  &str = "\x1b[38;5;88m";
    pub const WHITE: &str = "\x1b[97m";
    pub const GRAY:  &str = "\x1b[90m";
    pub const DGRAY: &str = "\x1b[38;5;236m";
}
use color::*;

pub fn clear()             { print!("\x1b[2J\x1b[H"); flush(); }
pub fn flush()             { let _ = std::io::Write::flush(&mut std::io::stdout()); }
pub fn cursor_up(n: usize) { if n > 0 { print!("\x1b[{}A", n); } }

// ─── banner ──────────────────────────────────────────────────────────────────

pub fn print_banner() {
    let art = [
        "██████╗  █████╗ ██████╗  █████╗ ███████╗██╗████████╗███████╗",
        "██╔══██╗██╔══██╗██╔══██╗██╔══██╗██╔════╝██║╚══██╔══╝██╔════╝",
        "██████╔╝███████║██████╔╝███████║███████╗██║   ██║   █████╗  ",
        "██╔═══╝ ██╔══██║██╔══██╗██╔══██║╚════██║██║   ██║   ██╔══╝  ",
        "██║     ██║  ██║██║  ██║██║  ██║███████║██║   ██║   ███████╗",
        "╚═╝     ╚═╝  ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝╚═╝   ╚═╝   ╚══════╝",
    ];
    let vein = "─".repeat(68);
    println!();
    println!("{DRED}▓▒░{vein}░▒▓{RESET}");
    println!("{DGRAY}╔══════════════════════════════════════════════════════════════════════╗{RESET}");
    println!("{DGRAY}║{RESET}                                                                      {DGRAY}║{RESET}");
    for (i, line) in art.iter().enumerate() {
        let c = if i < 2 { BRED } else if i < 4 { RED } else { DRED };
        println!("{DGRAY}║{RESET}   {c}{BOLD}{line}{RESET}   {DGRAY}║{RESET}");
    }
    println!("{DGRAY}║{RESET}                                                                      {DGRAY}║{RESET}");
    println!("{DGRAY}║{RESET}   {GRAY}[ infect · spread · mutate · exfiltrate · anonymize ]{RESET}         {DGRAY}║{RESET}");
    println!("{DGRAY}║{RESET}                                                                      {DGRAY}║{RESET}");
    println!("{DGRAY}╚══════════════════════════════════════════════════════════════════════╝{RESET}");
    println!("{DRED}░▒▓{vein}▓▒░{RESET}");
    println!();
}

// ─── menu ────────────────────────────────────────────────────────────────────

pub fn print_menu() {
    let sep = "─".repeat(68);
    macro_rules! cat { ($t:expr) => {{
        println!("  {DRED}│{RESET}  {DRED}{BOLD}{:<68}{RESET}  {DRED}│{RESET}", $t);
        println!("  {DRED}│{RESET}  {GRAY}{sep}{RESET}  {DRED}│{RESET}");
    }}}
    macro_rules! row { ($key:expr, $name:expr, $desc:expr) => {
        println!("  {DRED}│{RESET}   {BRED}{BOLD}[{:<2}]{RESET}  {RED}◈{RESET}  {WHITE}{BOLD}{:<22}{RESET}  {GRAY}{:<37}{RESET}  {DRED}│{RESET}",
            $key, $name, $desc);
    }}
    macro_rules! row0 { ($key:expr, $name:expr, $desc:expr) => {
        println!("  {DRED}│{RESET}   {GRAY}[{:<2}]  ◈  {:<22}  {:<37}{RESET}  {DRED}│{RESET}",
            $key, $name, $desc);
    }}

    println!("\n  {DRED}┌─────────────────────────── {BRED}{BOLD}select function{RESET}{DRED} ──────────────────────────┐{RESET}");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");

    cat!("web harvest");
    row!( "1",  "infect",              "полный рекурсивный обход сайта");
    row!( "2",  "feed",                "поглотить данные страницы");
    row!( "3",  "map colony",          "карта структуры сайта (bfs)");
    row!( "4",  "leech",               "скачать все ресурсы страницы");
    row!( "5",  "replicate",           "сохранить страницу локально");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");

    cat!("host analysis");
    row!( "6",  "analyze host",        "биопсия: тип, структура, контент");
    row!( "7",  "probe defenses",      "заголовки безопасности + robots");
    row!( "8",  "ssl inspect",         "ssl сертификат и цепочка");
    row!( "9",  "http methods",        "разрешённые http методы");
    row!("10",  "header dump",         "все заголовки http ответа");
    row!("11",  "cors probe",          "cors misconfiguration check");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");

    cat!("infection & spreading");
    row!("12",  "shadow crawl",        "стелс-обход: ротация отпечатков/заголовков");
    row!("13",  "backdoor hunter",     "поиск .env/.git/бэкапов/паролей");
    row!("14",  "form injector",       "сбор форм и генерация векторов фаззинга");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");

    cat!("parasite ops");
    row!("15",  "dna mutation",        "мутация вордлиста под цель");
    row!("16",  "necrosis check",      "поиск мёртвых ссылок для перехвата");
    row!("17",  "content exfiltration","умный сбор: ключи/токены/карты/пароли");
    row!("18",  "spawn larvae",        "генератор вариантов слова");
    row!("19",  "dormant check",       "пакетная проверка живости хостов");
    row!("20",  "burrow",              "перебор директорий (wordlist)");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");

    cat!("symbiosis & logic");
    row!("21",  "api parasite",        "сборка api-карты из js-трафика");
    row!("22",  "websocket leech",     "перехват websocket трафика");
    row!("23",  "symbiosis",           "перебор api эндпоинтов");
    row!("24",  "open redirect",       "проверка на open redirect");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");

    cat!("hash & encode");
    row!("25",  "hash generate",       "md5/sha1/sha256/sha512 хэш строки");
    row!("26",  "hash identify",       "определить тип хэша");
    row!("27",  "encode",              "base64/hex/url кодирование");
    row!("28",  "decode",              "base64/hex/url декодирование");
    row!("29",  "checksum file",       "хэш локального файла");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");

    cat!("special");
    row!("30",  "score targets",       "приоритет целей по url");
    row!("31",  "drain cache",         "просмотр сохранённых результатов");
    row!("32",  "tor mode",            "анонимизация через сеть TOR / proxy");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");

    row0!("0",  "evacuate",            "покинуть хост");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");
    println!("  {DRED}└──────────────────────────────────────────────────────────────────────┘{RESET}");
    println!();
}

// ─── input ───────────────────────────────────────────────────────────────────

pub fn prompt(label: &str) -> String {
    print!("  {RED}▸{RESET} {WHITE}{label}{RESET} ");
    flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap_or(0);
    line.trim().to_string()
}

pub fn prompt_default(label: &str, default: &str) -> String {
    print!("  {RED}▸{RESET} {WHITE}{label}{RESET} {GRAY}[{default}]{RESET} ");
    flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap_or(0);
    let t = line.trim().to_string();
    if t.is_empty() { default.to_string() } else { t }
}

pub fn pause() {
    print!("\n  {GRAY}━━  enter — продолжить  ━━{RESET}  ");
    flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap_or(0);
}

// ─── output helpers ──────────────────────────────────────────────────────────

pub fn section(title: &str) { println!("\n  {DRED}┌─{RESET} {BRED}{BOLD}{title}{RESET}"); }
pub fn divider() {
    let s = "─".repeat(66);
    println!("  {DRED}  {s}{RESET}");
}
pub fn err(msg: &str)       { println!("  {RED}✗{RESET}  {RED}{msg}{RESET}"); }
pub fn warn(msg: &str)      { println!("  {RED}⚠{RESET}  {RED}{msg}{RESET}"); }
pub fn ok(msg: &str)        { println!("  {BRED}✓{RESET}  {msg}"); }
pub fn kv(k: &str, v: &str) { println!("  {DRED}  {k:<24}{RESET}{WHITE}{v}{RESET}"); }

// ─── crawl stats ─────────────────────────────────────────────────────────────

pub fn print_crawl_stats(
    target: &str, pages: u64, errors: u64, queue: usize,
    skipped: u64, pps: f64, elapsed: u64, last_url: &str, max_pages: usize,
) -> usize {
    let h      = elapsed / 3600;
    let m      = (elapsed % 3600) / 60;
    let s      = elapsed % 60;
    let pct    = ((pages as f64 / max_pages as f64) * 100.0).min(100.0) as usize;
    let filled = (pct * 46 / 100).min(46);
    let bar    = format!("{}{}", "█".repeat(filled), "░".repeat(46 - filled));
    let sp     = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"][(elapsed as usize) % 10];
    let last   = if last_url.len() > 60 { format!("{}…", &last_url[..59]) } else { last_url.to_string() };
    let tgt    = if target.len()   > 56 { format!("{}…", &target[..55])   } else { target.to_string() };

    println!("  {DRED}┌────────────────────────── {BRED}{BOLD}infecting{RESET}{DRED} ──────────────────────────┐{RESET}");
    println!("  {DRED}│{RESET}  {RED}{sp}{RESET}  {GRAY}host:{RESET}  {WHITE}{tgt:<56}{RESET}  {DRED}│{RESET}");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");
    println!("  {DRED}│{RESET}  {BRED}✓{RESET} поглощено {BRED}{BOLD}{pages:>6}{RESET}   {RED}✗{RESET} ошибок  {RED}{errors:>4}{RESET}   {GRAY}⊘{RESET} пропущено {GRAY}{skipped:>4}{RESET}  {DRED}│{RESET}");
    println!("  {DRED}│{RESET}  {RED}⟳{RESET} очередь   {RED}{queue:>6}{RESET}   {BRED}⚡{RESET} скорость {BRED}{BOLD}{pps:>5.1}{RESET}/с   {GRAY}⏱{RESET} {h:02}:{m:02}:{s:02}  {DRED}│{RESET}");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");
    println!("  {DRED}│{RESET}  {RED}[{bar}{RESET}{RED}]{RESET} {RED}{pct:>3}%{RESET}  {GRAY}/{max_pages}{RESET}               {DRED}│{RESET}");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");
    println!("  {DRED}│{RESET}  {GRAY}▸{RESET} {GRAY}{last:<68}{RESET}  {DRED}│{RESET}");
    println!("  {DRED}│{RESET}                                                                      {DRED}│{RESET}");
    println!("  {DRED}└──────────────────────────────────────────────────────────────────────┘{RESET}");
    println!("  {GRAY}  ctrl-c — остановить и сохранить{RESET}");
    12
}
