use anyhow::Result;
use std::time::Duration;
use crate::ui::{self, color::*};

const API_PATHS: &[(&str, &str)] = &[
    ("/api",                   "api root"),
    ("/api/v1",                "api v1"),
    ("/api/v2",                "api v2"),
    ("/api/v3",                "api v3"),
    ("/graphql",               "graphql"),
    ("/graphiql",              "graphiql ui"),
    ("/playground",            "graphql playground"),
    ("/swagger",               "swagger ui"),
    ("/swagger-ui.html",       "swagger ui"),
    ("/api-docs",              "api docs"),
    ("/openapi.json",          "openapi spec"),
    ("/openapi.yaml",          "openapi spec"),
    ("/api/swagger.json",      "swagger spec"),
    ("/api/swagger.yaml",      "swagger spec"),
    ("/api/openapi.json",      "openapi spec"),
    ("/api/spec",              "api spec"),
    ("/api/schema",            "api schema"),
    ("/api/health",            "health endpoint"),
    ("/api/status",            "status endpoint"),
    ("/api/version",           "version endpoint"),
    ("/api/info",              "info endpoint"),
    ("/api/ping",              "ping endpoint"),
    ("/api/users",             "users resource"),
    ("/api/user",              "user resource"),
    ("/api/auth",              "auth endpoint"),
    ("/api/login",             "login endpoint"),
    ("/api/token",             "token endpoint"),
    ("/api/refresh",           "refresh token"),
    ("/api/me",                "current user"),
    ("/api/profile",           "profile endpoint"),
    ("/api/admin",             "admin api"),
    ("/api/metrics",           "metrics"),
    ("/api/logs",              "logs"),
    ("/api/search",            "search"),
    ("/api/products",          "products"),
    ("/api/orders",            "orders"),
    ("/rest",                  "rest api"),
    ("/rest/v1",               "rest v1"),
    ("/.well-known/openid-configuration", "oidc config"),
    ("/.well-known/jwks.json", "jwks keys"),
    ("/oauth/token",           "oauth token"),
    ("/oauth/authorize",       "oauth authorize"),
    ("/metrics",               "prometheus metrics"),
    ("/health",                "health check"),
    ("/healthz",               "health check k8s"),
    ("/readyz",                "readiness probe"),
    ("/debug/pprof",           "go pprof"),
    ("/actuator",              "spring actuator"),
    ("/actuator/health",       "spring health"),
    ("/actuator/env",          "spring env"),
];

pub async fn run() -> Result<()> {
    ui::section("symbiosis — поиск api эндпоинтов");
    println!();

    let target  = ui::prompt("target url (https://example.com):");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }
    let base    = target.trim_end_matches('/').to_string();
    let threads = ui::prompt_default("одновременных запросов:", "15")
        .parse::<usize>().unwrap_or(15);

    println!("  {GRAY}проверяем {} эндпоинтов...{RESET}", API_PATHS.len());
    ui::flush();

    let client = reqwest::Client::builder()
        .user_agent("parasite/1.0")
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let mut found: Vec<(String, u16, String)> = vec![];
    let chunks: Vec<&[(&str, &str)]> = API_PATHS.chunks(threads).collect();

    for chunk in &chunks {
        let futs: Vec<_> = chunk.iter().map(|(path, desc)| {
            let client = client.clone();
            let url    = format!("{base}{path}");
            let desc   = desc.to_string();
            async move {
                match client.get(&url).send().await {
                    Ok(r) => {
                        let s = r.status().as_u16();
                        if s != 404 { Some((url, s, desc)) } else { None }
                    }
                    Err(_) => None,
                }
            }
        }).collect();

        for r in futures::future::join_all(futs).await.into_iter().flatten() {
            found.push(r);
        }
    }

    ui::cursor_up(1);
    println!();
    println!("  {DRED}╔════════════════ {BRED}{BOLD}symbiosis{RESET}{DRED} ════════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}проверено:{RESET}  {WHITE}{}{RESET}   {BRED}найдено:{RESET}  {BRED}{BOLD}{}{RESET}", API_PATHS.len(), found.len());
    println!("  {DRED}║{RESET}");

    if found.is_empty() {
        println!("  {DRED}║{RESET}  {GRAY}api эндпоинты не обнаружены{RESET}");
    } else {
        found.sort_by_key(|r| r.1);
        for (url, status, desc) in &found {
            let sc = match status {
                200..=299 => BRED,
                301..=308 => RED,
                401 | 403 => RED,
                _         => GRAY,
            };
            println!("  {DRED}║{RESET}  {sc}{status}{RESET}  {GRAY}{desc:<24}{RESET}  {WHITE}{}{RESET}", shorten(url, 42));
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
