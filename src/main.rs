// The engine carries some scaffolding (config helpers, scheduler internals) that
// isn't wired into every build path; silence those intentional dead-code notes.
#![allow(dead_code)]

mod config;
mod crawler;
mod eye;
mod operations;
mod parser;
mod rate_limiter;
mod robots;
mod scheduler;
mod scoring;
mod spider;
mod stats;
mod storage;
mod ui;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let gui = std::env::args().any(|a| a == "--gui");
    init_logging();
    if !gui { intro_animation().await; }

    loop {
        if !gui {
            ui::clear();
            ui::print_banner();
            ui::print_menu();
        }

        let choice = if gui {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).unwrap_or(0);
            line.trim().to_string()
        } else {
            ui::prompt("команда:")
        };
        println!();

        match choice.trim() {
            // web harvest
            "1"  => operations::infect::run().await?,
            "2"  => operations::feed::run().await?,
            "3"  => operations::map_colony::run().await?,
            "4"  => operations::leech::run().await?,
            "5"  => operations::replicate::run().await?,
            // host analysis
            "6"  => operations::analyze::run().await?,
            "7"  => operations::probe::run().await?,
            "8"  => operations::ssl_inspect::run().await?,
            "9"  => operations::http_methods::run().await?,
            "10" => operations::header_dump::run().await?,
            "11" => operations::cors_probe::run().await?,
            // infection & spreading
            "12" => operations::shadow_crawl::run().await?,
            "13" => operations::backdoor_hunter::run().await?,
            "14" => operations::form_injector::run().await?,
            // parasite ops
            "15" => operations::dna_mutation::run().await?,
            "16" => operations::necrosis_check::run().await?,
            "17" => operations::content_exfil::run().await?,
            "18" => operations::spawn_larvae::run().await?,
            "19" => operations::dormant_check::run().await?,
            "20" => operations::burrow::run().await?,
            // symbiosis & logic
            "21" => operations::api_parasite::run().await?,
            "22" => operations::ws_leech::run().await?,
            "23" => operations::symbiosis::run().await?,
            "24" => operations::open_redirect::run().await?,
            // hash & encode
            "25" => operations::hash_tools::run_generate().await?,
            "26" => operations::hash_tools::run_identify().await?,
            "27" => operations::encode_decode::run_encode().await?,
            "28" => operations::encode_decode::run_decode().await?,
            "29" => operations::checksum::run().await?,
            // special
            "30" => operations::score::run().await?,
            "31" => operations::drain::run().await?,
            "32" => operations::tor_mode::run().await?,
            // exit
            "0" | "q" | "Q" | "exit" | "evacuate" => {
                if !gui { evacuate_screen().await; }
                break;
            }
            _ => ui::err(&format!("неизвестная команда: '{}'", choice.trim())),
        }

        if !gui { ui::pause(); }
    }

    Ok(())
}

async fn intro_animation() {
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    ui::clear();
    println!("\n\n\n\n");
    for _ in 0..eye::EYE_LINES { println!(); }

    let cancel = CancellationToken::new();
    let cc     = cancel.clone();
    let task   = tokio::spawn(async move { eye::run_animation(cc).await });

    tokio::time::sleep(Duration::from_millis(3400)).await;
    cancel.cancel();
    let _ = task.await;
}

async fn evacuate_screen() {
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;
    use ui::color::*;

    ui::clear();
    println!("\n\n\n");
    for _ in 0..eye::EYE_LINES { println!(); }

    let cancel = CancellationToken::new();
    let cc     = cancel.clone();
    let task   = tokio::spawn(async move { eye::run_animation(cc).await });

    tokio::time::sleep(Duration::from_millis(700)).await;
    cancel.cancel();
    let _ = task.await;

    println!("\n");
    println!("          {DRED}╔═══════════════════════════════════════╗{RESET}");
    println!("          {DRED}║{RESET}                                       {DRED}║{RESET}");
    println!("          {DRED}║{RESET}   {RED}parasite{RESET} {GRAY}— withdrawing from host{RESET}   {DRED}║{RESET}");
    println!("          {DRED}║{RESET}                                       {DRED}║{RESET}");
    println!("          {DRED}╚═══════════════════════════════════════╝{RESET}");
    println!();
    tokio::time::sleep(Duration::from_millis(800)).await;
}

fn init_logging() {
    let file = std::fs::OpenOptions::new()
        .create(true).append(true)
        .open("parasite.log")
        .expect("cannot open log file");
    tracing_subscriber::fmt()
        .with_writer(file)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_ansi(false)
        .init();
}
