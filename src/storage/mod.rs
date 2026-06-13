use anyhow::Result;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::parser::PageType;

/// Результат обхода одной страницы — экспортируется в JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlResult {
    pub url: String,
    pub title: Option<String>,
    pub page_type: PageType,
    pub depth: u32,
    pub status_code: u16,
    pub links_found: usize,
    pub crawled_at: DateTime<Utc>,
    pub response_time_ms: u64,
}

/// Хранилище состояния краулера.
/// Разделяется между всеми воркерами через Arc.
pub struct Storage {
    /// Множество посещённых URL — O(1) поиск через DashMap
    visited: DashMap<String, ()>,
    /// Накопленные результаты для экспорта
    results: RwLock<Vec<CrawlResult>>,
}

impl Storage {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            visited: DashMap::new(),
            results: RwLock::new(Vec::new()),
        })
    }

    // ─── Visited set ─────────────────────────────────────────────────────────

    #[inline]
    pub fn is_visited(&self, url: &str) -> bool {
        self.visited.contains_key(url)
    }

    /// Атомарно пометить URL посещённым.
    /// Возвращает true если URL был добавлен (т.е. не был посещён ранее).
    pub fn mark_visited(&self, url: String) -> bool {
        self.visited.insert(url, ()).is_none()
    }

    pub fn visited_count(&self) -> usize {
        self.visited.len()
    }

    // ─── Results ─────────────────────────────────────────────────────────────

    pub async fn store_result(&self, result: CrawlResult) {
        self.results.write().await.push(result);
    }

    pub async fn results_count(&self) -> usize {
        self.results.read().await.len()
    }

    // ─── Persistence ─────────────────────────────────────────────────────────

    /// Сохранить результаты в JSON-файл
    pub async fn save_results(&self, path: &str) -> Result<()> {
        let results = self.results.read().await;
        let json = serde_json::to_string_pretty(&*results)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    /// Сохранить список посещённых URL для возобновления после перезапуска
    pub async fn save_visited(&self, path: &str) -> Result<()> {
        let visited: Vec<String> = self.visited.iter().map(|e| e.key().clone()).collect();
        let json = serde_json::to_string(&visited)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    /// Загрузить сохранённое состояние (visited URLs)
    pub async fn load_visited(&self, path: &str) -> Result<()> {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                let visited: Vec<String> = serde_json::from_str(&content)?;
                let count = visited.len();
                for url in visited {
                    self.visited.insert(url, ());
                }
                tracing::info!("Loaded {} visited URLs from state file", count);
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
