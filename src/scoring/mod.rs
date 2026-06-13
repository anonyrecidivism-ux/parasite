use url::Url;

/// Движок скоринга URL.
/// Возвращает числовой приоритет: чем выше — тем раньше обрабатывается.
pub struct ScoringEngine;

impl ScoringEngine {
    /// Вычислить приоритет URL. Результат используется в BinaryHeap.
    pub fn score(url: &Url, depth: u32) -> i32 {
        let path = url.path().to_lowercase();

        // Сразу отбрасываем медиа — возвращаем i32::MIN как сигнал
        if Self::is_media(&path) {
            return i32::MIN;
        }

        let mut score: i32 = 1_000;

        // Высокий приоритет — API и документация
        if path.contains("/api") || path.contains("/v1") || path.contains("/v2") {
            score += 300;
        }
        if path.contains("/docs") || path.contains("/documentation") || path.contains("/wiki") {
            score += 250;
        }

        // Средний — блог, профили, каталог
        if path.contains("/blog") || path.contains("/post") || path.contains("/article") {
            score += 100;
        }
        if path.contains("/catalog") || path.contains("/category") || path.contains("/shop") {
            score += 80;
        }
        if path.contains("/product") || path.contains("/item") {
            score += 60;
        }
        if path.contains("/user") || path.contains("/profile") || path.contains("/@") {
            score += 40;
        }

        // Штраф за глубину — предпочитаем неглубокие URL
        score -= (depth as i32) * 80;

        // Штраф за query-параметры — ведут к дублированию контента
        let param_count = url.query_pairs().count() as i32;
        score -= param_count * 150;

        // Штраф за длинный path (обычно машинные URL)
        let segments = path.matches('/').count() as i32;
        if segments > 6 {
            score -= (segments - 6) * 30;
        }

        score
    }

    /// True если URL — медиафайл, который не нужно обходить
    pub fn is_media(path: &str) -> bool {
        const MEDIA_EXTENSIONS: &[&str] = &[
            ".jpg", ".jpeg", ".png", ".gif", ".svg", ".webp", ".ico",
            ".mp4", ".mp3", ".avi", ".mov", ".mkv", ".webm",
            ".pdf", ".zip", ".tar", ".gz", ".rar", ".7z",
            ".css", ".js", ".woff", ".woff2", ".ttf", ".eot",
            ".xml", ".rss", ".atom",
        ];
        let p = path.split('?').next().unwrap_or(path);
        MEDIA_EXTENSIONS.iter().any(|ext| p.ends_with(ext))
    }
}
