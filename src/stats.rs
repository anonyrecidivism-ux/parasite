use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Счётчики состояния краулера.
/// Разделяется между всеми воркерами через Arc.
/// Не содержит TUI-специфичной логики.
pub struct CrawlStats {
    pub pages_done: AtomicU64,
    pub errors:     AtomicU64,
    pub skipped:    AtomicU64,
    /// Последний обработанный URL (для отображения прогресса)
    pub last_url:   Mutex<String>,
    pub start_time: Instant,
}

impl CrawlStats {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            pages_done: AtomicU64::new(0),
            errors:     AtomicU64::new(0),
            skipped:    AtomicU64::new(0),
            last_url:   Mutex::new(String::new()),
            start_time: Instant::now(),
        })
    }

    pub fn inc_pages(&self) {
        self.pages_done.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_errors(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_skipped(&self) {
        self.skipped.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn set_last_url(&self, url: &str) {
        *self.last_url.lock().await = url.to_string();
    }

    pub async fn last_url_str(&self) -> String {
        self.last_url.lock().await.clone()
    }

    pub fn elapsed_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Страниц в секунду (приблизительно, на основе elapsed)
    pub fn pages_per_sec(&self) -> f64 {
        let pages = self.pages_done.load(Ordering::Relaxed);
        let secs = self.elapsed_secs().max(1);
        pages as f64 / secs as f64
    }
}
