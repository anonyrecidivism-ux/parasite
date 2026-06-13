use anyhow::Result;
use std::time::Duration;
use crate::ui::{self, color::*};

const WORDLIST: &[&str] = &[
    "admin","administrator","login","signin","signup","register","logout","dashboard",
    "panel","cpanel","wp-admin","wp-login.php","phpmyadmin","pma","adminer",
    "api","api/v1","api/v2","api/v3","rest","graphql","swagger","swagger-ui.html",
    "api-docs","openapi.json","openapi.yaml",
    "config","config.php","config.json","config.yaml","settings","setup",
    ".env",".env.local",".env.production","env","environment",
    "backup","backup.zip","backup.tar.gz","db.sql","database.sql","dump.sql",
    "uploads","upload","files","file","media","images","img","static","assets",
    "docs","documentation","wiki","help","support","faq",
    "test","testing","debug","dev","development","staging","beta","demo","sandbox",
    "old","new","bak","backup","archive","temp","tmp",
    "users","user","accounts","account","profile","profiles",
    "search","data","export","import","download","report","reports",
    "logs","log","error.log","access.log","debug.log",
    "index.php","index.html","robots.txt","sitemap.xml","sitemap.xml.gz",
    ".git/HEAD",".git/config","git/config",".svn/entries",
    "Dockerfile","docker-compose.yml","docker-compose.yaml",
    "package.json","composer.json","requirements.txt","Gemfile",
    "status","health","healthz","ping","metrics","monitor",
    "invoice","invoices","orders","order","payment","checkout","cart",
    "register","reset","forgot","password","change-password",
    "contact","about","privacy","terms","legal",
    "500.html","404.html","403.html","error",
    "cgi-bin","scripts","sh","cmd","exec",
    "server-status","server-info",
];

pub async fn run() -> Result<()> {
    ui::section("burrow — перебор директорий");
    println!();

    let target = ui::prompt("target url (https://example.com):");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }
    let base   = target.trim_end_matches('/').to_string();

    let threads = ui::prompt_default("одновременных запросов:", "20")
        .parse::<usize>().unwrap_or(20);
    let status_filter = ui::prompt_default("показать статусы (all/200/2xx):", "all").to_lowercase();

    println!("  {GRAY}бурение {} путей...{RESET}", WORDLIST.len());
    ui::flush();

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let mut found: Vec<(String, u16, usize)> = vec![];
    let chunks: Vec<&[&str]> = WORDLIST.chunks(threads).collect();
    let total = WORDLIST.len();
    let mut checked = 0usize;

    for chunk in &chunks {
        let futs: Vec<_> = chunk.iter().map(|path| {
            let client = client.clone();
            let url    = format!("{base}/{path}");
            async move {
                match client.head(&url).send().await {
                    Ok(r) => Some((url, r.status().as_u16(), 0usize)),
                    Err(_) => None,
                }
            }
        }).collect();

        for r in futures::future::join_all(futs).await.into_iter().flatten() {
            let show = match status_filter.as_str() {
                "200"  => r.1 == 200,
                "2xx"  => r.1 < 300,
                _      => r.1 != 404,
            };
            if show { found.push(r); }
        }

        checked += chunk.len();
        ui::cursor_up(1);
        let pct = checked * 100 / total;
        let bar = format!("{}{}", "█".repeat(pct * 30 / 100), "░".repeat(30 - pct * 30 / 100));
        println!("  {RED}[{bar}]{RESET} {GRAY}{checked}/{total}{RESET}  {BRED}найдено: {}{RESET}", found.len());
        ui::flush();
    }

    ui::cursor_up(1);
    println!();
    println!("  {DRED}╔═══════════════ {BRED}{BOLD}burrow results{RESET}{DRED} ═══════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}проверено:{RESET}  {WHITE}{total}{RESET}   {BRED}найдено:{RESET}  {BRED}{BOLD}{}{RESET}", found.len());
    println!("  {DRED}║{RESET}");

    if found.is_empty() {
        println!("  {DRED}║{RESET}  {GRAY}ничего интересного не найдено{RESET}");
    } else {
        found.sort_by_key(|r| r.1);
        for (url, status, _) in &found {
            let sc = match status {
                200..=299 => BRED,
                301..=308 => RED,
                401 | 403 => RED,
                _         => GRAY,
            };
            println!("  {DRED}║{RESET}  {sc}{status}{RESET}  {WHITE}{}{RESET}", shorten(url, 64));
        }
    }

    println!("  {DRED}║{RESET}");
    println!("  {DRED}╚══════════════════════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
