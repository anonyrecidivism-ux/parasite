use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Ограничитель скорости запросов с гранулярностью до домена.
///
/// Реализует sliding-window throttle: между двумя запросами к одному домену
/// выдерживается `min_delay`. Crawl-delay из robots.txt учитывается отдельно
/// через `acquire_with_delay`.
pub struct RateLimiter {
    /// Последний момент запроса к домену
    last_request: DashMap<String, Instant>,
    /// Базовая задержка между запросами к одному домену
    min_delay: Duration,
}

impl RateLimiter {
    pub fn new(min_delay_ms: u64) -> Arc<Self> {
        Arc::new(Self {
            last_request: DashMap::new(),
            min_delay: Duration::from_millis(min_delay_ms),
        })
    }

    /// Подождать необходимое время, затем зафиксировать момент запроса.
    /// `extra_delay` — дополнительная задержка (из Crawl-delay robots.txt).
    pub async fn acquire(&self, domain: &str, extra_delay: Option<Duration>) {
        let effective_delay = match extra_delay {
            Some(extra) => self.min_delay.max(extra),
            None        => self.min_delay,
        };

        let wait = {
            if let Some(last) = self.last_request.get(domain) {
                let elapsed = last.elapsed();
                if elapsed < effective_delay {
                    Some(effective_delay - elapsed)
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(w) = wait {
            sleep(w).await;
        }

        self.last_request.insert(domain.to_string(), Instant::now());
    }
}
