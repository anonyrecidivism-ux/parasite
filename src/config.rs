use serde::{Deserialize, Serialize};

/// Полная конфигурация краулера.
/// Все поля имеют разумные дефолты — менять только то, что нужно.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlerConfig {
    /// Начальные URL для обхода
    pub seed_urls: Vec<String>,
    /// Максимальная глубина ссылок от seed
    pub max_depth: u32,
    /// Максимальное количество страниц
    pub max_pages: usize,
    /// Количество параллельных воркеров
    pub num_workers: usize,
    /// Таймаут HTTP-запроса (секунды)
    pub request_timeout_secs: u64,
    /// Минимальная задержка между запросами к одному домену (мс)
    pub domain_delay_ms: u64,
    /// User-Agent заголовок
    pub user_agent: String,
    /// Путь для экспорта результатов (JSON)
    pub output_file: String,
    /// Путь для сохранения/загрузки состояния
    pub state_file: String,
    /// Соблюдать robots.txt
    pub respect_robots: bool,
    /// Следовать внешним ссылкам (другие домены)
    pub follow_external: bool,
    /// Максимальный размер ответа (байт) — защита от огромных страниц
    pub max_body_size: usize,
}

impl Default for CrawlerConfig {
    fn default() -> Self {
        Self {
            seed_urls: vec![
                "https://example.com".to_string(),
            ],
            max_depth: 4,
            max_pages: 5_000,
            num_workers: 12,
            request_timeout_secs: 20,
            domain_delay_ms: 1_000,
            user_agent: "RustCrawler/1.0 (+https://github.com/example/rustcrawler)".to_string(),
            output_file: "results.json".to_string(),
            state_file: "crawler_state.json".to_string(),
            respect_robots: true,
            follow_external: false,
            max_body_size: 5 * 1024 * 1024, // 5 МБ
        }
    }
}

impl CrawlerConfig {
    /// Загрузить конфиг из файла или вернуть дефолт
    pub fn load_or_default(path: &str) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
}
