use anyhow::Result;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::config::CrawlerConfig;
use crate::rate_limiter::RateLimiter;
use crate::robots::RobotsManager;
use crate::scheduler::{UrlEntry, UrlQueue};
use crate::scoring::ScoringEngine;
use crate::spider::Spider;
use crate::stats::CrawlStats;
use crate::storage::Storage;

/// Координатор краулера: инициализирует подсистемы, засевает очередь,
/// запускает воркеров и ждёт завершения.
pub struct Crawler {
    pub config:   CrawlerConfig,
    pub queue:    Arc<UrlQueue>,
    pub storage:  Arc<Storage>,
    pub stats:    Arc<CrawlStats>,
    rate_limiter: Arc<RateLimiter>,
    robots:       Arc<RobotsManager>,
    cancel:       CancellationToken,
}

impl Crawler {
    pub fn new(config: CrawlerConfig, cancel: CancellationToken) -> Arc<Self> {
        let http = Arc::new(
            Client::builder()
                .user_agent(&config.user_agent)
                .timeout(Duration::from_secs(config.request_timeout_secs))
                .gzip(true)
                .build()
                .expect("HTTP client build failed"),
        );

        Arc::new(Self {
            rate_limiter: RateLimiter::new(config.domain_delay_ms),
            robots:       Arc::new(RobotsManager::new(http, config.user_agent.clone())),
            queue:        UrlQueue::new(),
            storage:      Storage::new(),
            stats:        CrawlStats::new(),
            cancel,
            config,
        })
    }

    pub async fn run(self: Arc<Self>) -> Result<()> {
        // Загрузить предыдущее состояние
        let _ = self.storage.load_visited(&self.config.state_file).await;

        // Посеять начальные URL
        for raw in &self.config.seed_urls {
            if let Ok(url) = Url::parse(raw) {
                let priority = ScoringEngine::score(&url, 0);
                self.queue.push(UrlEntry { url, depth: 0, priority, parent: None }).await;
            }
        }

        // Запустить воркеров
        let mut handles = Vec::with_capacity(self.config.num_workers);
        for id in 0..self.config.num_workers {
            let spider = Spider::new(
                id,
                self.queue.clone(),
                self.storage.clone(),
                self.rate_limiter.clone(),
                self.robots.clone(),
                self.stats.clone(),
                self.cancel.clone(),
                self.config.clone(),
            );
            handles.push(tokio::spawn(async move { spider.run().await }));
        }

        // Монитор завершения
        {
            let cancel  = self.cancel.clone();
            let queue   = self.queue.clone();
            let storage = self.storage.clone();
            let max     = self.config.max_pages;

            tokio::spawn(async move {
                let mut idle = 0u32;
                let mut interval = tokio::time::interval(Duration::from_millis(400));
                loop {
                    interval.tick().await;
                    if cancel.is_cancelled() { break; }
                    if storage.visited_count() >= max {
                        cancel.cancel();
                        queue.notify_all_waiters();
                        break;
                    }
                    if queue.is_empty().await {
                        idle += 1;
                        if idle >= 5 {
                            cancel.cancel();
                            queue.notify_all_waiters();
                            break;
                        }
                    } else {
                        idle = 0;
                    }
                }
            });
        }

        for h in handles { let _ = h.await; }

        self.storage.save_results(&self.config.output_file).await?;
        self.storage.save_visited(&self.config.state_file).await?;
        Ok(())
    }
}
