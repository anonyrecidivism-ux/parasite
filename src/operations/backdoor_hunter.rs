use anyhow::Result;
use std::time::Duration;
use crate::ui::{self, color::*};

// (path, description, severity)
const TARGETS: &[(&str, &str, &str)] = &[
    // Environment files
    (".env",                     ".env файл (credentials!)",          "CRITICAL"),
    (".env.local",               ".env.local",                         "CRITICAL"),
    (".env.production",          ".env.production",                    "CRITICAL"),
    (".env.backup",              ".env backup",                        "CRITICAL"),
    (".env.old",                 ".env старый",                        "HIGH"),
    (".env.dev",                 ".env development",                   "HIGH"),
    (".env.staging",             ".env staging",                       "HIGH"),
    // Git repos
    (".git/HEAD",                "git репозиторий открыт",             "CRITICAL"),
    (".git/config",              "git config с credentials",           "CRITICAL"),
    (".git/COMMIT_EDITMSG",      "git commit message",                 "HIGH"),
    (".git/logs/HEAD",           "git log история",                    "HIGH"),
    (".gitignore",               ".gitignore (раскрывает структуру)",  "LOW"),
    // DB backups
    ("backup.sql",               "SQL дамп базы данных",               "CRITICAL"),
    ("dump.sql",                 "SQL dump",                           "CRITICAL"),
    ("database.sql",             "database dump",                      "CRITICAL"),
    ("db.sql",                   "db dump",                            "CRITICAL"),
    ("backup.tar.gz",            "backup архив",                       "CRITICAL"),
    ("backup.zip",               "backup ZIP",                         "CRITICAL"),
    ("site.tar.gz",              "архив сайта",                        "HIGH"),
    // Config files
    ("config.php",               "PHP конфиг",                         "HIGH"),
    ("wp-config.php",            "WordPress конфиг",                   "CRITICAL"),
    ("wp-config.php.bak",        "WordPress конфиг backup",            "CRITICAL"),
    ("configuration.php",        "Joomla конфиг",                      "HIGH"),
    ("app/config/parameters.yml","Symfony параметры",                  "HIGH"),
    (".htpasswd",                ".htpasswd файл паролей",              "CRITICAL"),
    (".htaccess",                ".htaccess правила",                   "MEDIUM"),
    // Debug / info
    ("phpinfo.php",              "phpinfo() открыт",                   "HIGH"),
    ("info.php",                 "phpinfo alias",                      "HIGH"),
    ("test.php",                 "тестовый файл",                      "MEDIUM"),
    ("debug.php",                "debug скрипт",                       "HIGH"),
    ("server-status",            "Apache server-status",               "MEDIUM"),
    ("server-info",              "Apache server-info",                  "MEDIUM"),
    // Logs
    ("error.log",                "лог ошибок",                         "MEDIUM"),
    ("access.log",               "access log",                         "MEDIUM"),
    ("debug.log",                "debug log",                          "MEDIUM"),
    ("laravel.log",              "Laravel лог",                        "MEDIUM"),
    ("storage/logs/laravel.log", "Laravel storage log",                "MEDIUM"),
    // Source code archives
    ("source.zip",               "исходный код ZIP",                   "CRITICAL"),
    ("src.zip",                  "src архив",                          "HIGH"),
    ("website.zip",              "website архив",                      "HIGH"),
    ("old.zip",                  "old version архив",                  "HIGH"),
    // API / tokens
    ("api.key",                  "API ключ файл",                      "CRITICAL"),
    ("private.key",              "приватный ключ",                     "CRITICAL"),
    ("id_rsa",                   "SSH приватный ключ",                 "CRITICAL"),
    (".npmrc",                   ".npmrc с токенами",                  "HIGH"),
    (".pypirc",                  "PyPI credentials",                   "HIGH"),
    ("credentials",              "файл credentials",                   "HIGH"),
    // Docker / k8s
    ("docker-compose.yml",       "docker-compose конфиг",              "MEDIUM"),
    ("Dockerfile",               "Dockerfile",                         "LOW"),
    ("k8s.yaml",                 "Kubernetes конфиг",                  "MEDIUM"),
];

pub async fn run() -> Result<()> {
    ui::section("backdoor hunter — поиск открытых файлов");
    println!();

    let target   = ui::prompt("target url (https://example.com):");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }
    let base     = target.trim_end_matches('/').to_string();
    let threads  = ui::prompt_default("одновременных запросов:", "20")
        .parse::<usize>().unwrap_or(20);

    println!("  {GRAY}охота на {} целей...{RESET}", TARGETS.len());
    ui::flush();

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; Googlebot/2.1)")
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let mut found: Vec<(&str, &str, &str, u16, usize)> = vec![]; // path, desc, sev, status, len
    let total = TARGETS.len();
    let mut checked = 0usize;

    for chunk in TARGETS.chunks(threads) {
        let futs: Vec<_> = chunk.iter().map(|(path, desc, sev)| {
            let client = client.clone();
            let url    = format!("{base}/{path}");
            async move {
                match client.get(&url).send().await {
                    Ok(r) if r.status().as_u16() != 404 => {
                        let s = r.status().as_u16();
                        let len = r.content_length().unwrap_or(0) as usize;
                        Some((*path, *desc, *sev, s, len))
                    }
                    _ => None,
                }
            }
        }).collect();

        for r in futures::future::join_all(futs).await.into_iter().flatten() {
            found.push(r);
        }

        checked += chunk.len();
        ui::cursor_up(1);
        let pct = checked * 100 / total;
        let bar = format!("{}{}", "█".repeat(pct * 28 / 100), "░".repeat(28 - pct * 28 / 100));
        println!("  {RED}[{bar}]{RESET} {GRAY}{checked}/{total}{RESET}  {RED}⚠ найдено: {}{RESET}", found.len());
        ui::flush();
    }

    ui::cursor_up(1);
    println!();
    println!("  {DRED}╔═════════════════ {BRED}{BOLD}backdoor hunter{RESET}{DRED} ═════════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}проверено:{RESET}  {WHITE}{total}{RESET}   {RED}найдено:{RESET}  {RED}{BOLD}{}{RESET}", found.len());
    println!("  {DRED}║{RESET}");

    if found.is_empty() {
        println!("  {DRED}║{RESET}  {BRED}✓{RESET}  уязвимых файлов не обнаружено");
    } else {
        for (path, desc, sev, status, len) in &found {
            let (sc, bullet) = match *sev {
                "CRITICAL" => (BRED, "██ CRITICAL"),
                "HIGH"     => (RED,  "▓▓ HIGH    "),
                "MEDIUM"   => (RED,  "▒▒ MEDIUM  "),
                _          => (GRAY, "░░ LOW     "),
            };
            println!("  {DRED}║{RESET}  {sc}{bullet}{RESET}  {WHITE}{status}{RESET}  {sc}{path}{RESET}");
            println!("  {DRED}║{RESET}           {GRAY}{desc}  ({len} байт){RESET}");
            println!("  {DRED}║{RESET}           {DRED}{base}/{path}{RESET}");
            println!("  {DRED}║{RESET}");
        }
    }

    println!("  {DRED}╚══════════════════════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}
