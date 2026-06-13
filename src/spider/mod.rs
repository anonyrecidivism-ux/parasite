use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use url::Url;

use crate::config::CrawlerConfig;
use crate::parser::{normalize_url, HtmlParser};
use crate::rate_limiter::RateLimiter;
use crate::robots::RobotsManager;
use crate::scheduler::{UrlEntry, UrlQueue};
use crate::scoring::ScoringEngine;
use crate::stats::CrawlStats;
use crate::storage::{CrawlResult, Storage};

pub struct Spider {
    pub id:       usize,
    queue:        Arc<UrlQueue>,
    storage:      Arc<Storage>,
    rate_limiter: Arc<RateLimiter>,
    robots:       Arc<RobotsManager>,
    stats:        Arc<CrawlStats>,
    cancel:       CancellationToken,
    config:       CrawlerConfig,
    client:       reqwest::Client,
}

impl Spider {
    pub fn new(
        id:           usize,
        queue:        Arc<UrlQueue>,
        storage:      Arc<Storage>,
        rate_limiter: Arc<RateLimiter>,
        robots:       Arc<RobotsManager>,
        stats:        Arc<CrawlStats>,
        cancel:       CancellationToken,
        config:       CrawlerConfig,
    ) -> Arc<Self> {
        let client = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .gzip(true)
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .expect("HTTP client build failed");

        Arc::new(Self { id, queue, storage, rate_limiter, robots, stats, cancel, config, client })
    }

    pub async fn run(self: Arc<Self>) {
        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => break,
                entry = self.queue.pop_or_wait() => {
                    if self.cancel.is_cancelled() { break; }
                    self.process(entry).await;
                }
            }
        }
    }

    async fn process(&self, entry: UrlEntry) {
        let url_str = entry.url.to_string();

        if !self.storage.mark_visited(url_str.clone()) {
            return;
        }
        if self.storage.visited_count() > self.config.max_pages {
            self.cancel.cancel();
            return;
        }
        if entry.depth > self.config.max_depth {
            return;
        }

        if self.config.respect_robots && !self.robots.is_allowed(&entry.url).await {
            self.stats.inc_skipped();
            return;
        }

        let domain = entry.url.host_str().unwrap_or("unknown").to_string();
        let crawl_delay = self.robots.crawl_delay_secs(&domain).map(Duration::from_secs);
        self.rate_limiter.acquire(&domain, crawl_delay).await;

        self.stats.set_last_url(&url_str).await;

        let t_start = Instant::now();
        match self.fetch(&entry.url).await {
            Ok((status, html)) => {
                let elapsed_ms = t_start.elapsed().as_millis() as u64;
                if (200..300).contains(&status) {
                    self.handle_success(&entry, &url_str, &html, status, elapsed_ms).await;
                } else {
                    self.stats.inc_errors();
                }
            }
            Err(_) => self.stats.inc_errors(),
        }
    }

    async fn fetch(&self, url: &Url) -> anyhow::Result<(u16, String)> {
        let resp = self.client.get(url.as_str()).send().await?;
        let status = resp.status().as_u16();
        let bytes = resp.bytes().await?;
        if bytes.len() > self.config.max_body_size {
            return Ok((status, String::new()));
        }
        Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
    }

    async fn handle_success(
        &self,
        entry: &UrlEntry,
        url_str: &str,
        html: &str,
        status: u16,
        elapsed_ms: u64,
    ) {
        let page_type  = HtmlParser::detect_page_type(&entry.url, html);
        let title      = HtmlParser::extract_title(html);
        let raw_links  = HtmlParser::extract_links(html, &entry.url);
        let links_count = raw_links.len();

        self.enqueue_links(raw_links, &entry.url, entry.depth).await;

        self.storage.store_result(CrawlResult {
            url:              url_str.to_string(),
            title,
            page_type,
            depth:            entry.depth,
            status_code:      status,
            links_found:      links_count,
            crawled_at:       chrono::Utc::now(),
            response_time_ms: elapsed_ms,
        }).await;

        self.stats.inc_pages();
    }

    async fn enqueue_links(&self, links: Vec<Url>, base: &Url, parent_depth: u32) {
        for link in links {
            let normalized = normalize_url(link);

            if ScoringEngine::is_media(normalized.path()) {
                continue;
            }
            if !self.config.follow_external && normalized.host_str() != base.host_str() {
                continue;
            }
            if self.storage.is_visited(normalized.as_str()) {
                continue;
            }

            let depth    = parent_depth + 1;
            let priority = ScoringEngine::score(&normalized, depth);
            if priority == i32::MIN {
                continue;
            }

            self.queue.push(UrlEntry { url: normalized, depth, priority, parent: Some(base.clone()) }).await;
        }
    }
}
