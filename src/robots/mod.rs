use dashmap::DashMap;
use reqwest::Client;
use std::sync::Arc;
use url::Url;

struct Rules {
    /// Пути, запрещённые для нашего User-Agent (или *)
    disallowed: Vec<String>,
    /// Crawl-delay из robots.txt (секунды)
    pub crawl_delay_secs: Option<u64>,
}

/// Потокобезопасный менеджер robots.txt с кэшированием по хосту.
/// Один объект на всё приложение, разделяется между воркерами через Arc.
pub struct RobotsManager {
    cache: DashMap<String, Rules>,
    client: Arc<Client>,
    user_agent: String,
}

impl RobotsManager {
    pub fn new(client: Arc<Client>, user_agent: String) -> Self {
        Self {
            cache: DashMap::new(),
            client,
            user_agent,
        }
    }

    /// Проверить, разрешён ли URL для обхода
    pub async fn is_allowed(&self, url: &Url) -> bool {
        let host = match url.host_str() {
            Some(h) => h.to_string(),
            None    => return true,
        };

        if !self.cache.contains_key(&host) {
            self.fetch_and_cache(url, &host).await;
        }

        match self.cache.get(&host) {
            Some(rules) => {
                let path = url.path();
                !rules.disallowed.iter().any(|d| path.starts_with(d.as_str()))
            }
            None => true,
        }
    }

    /// Получить Crawl-delay для домена (если указан в robots.txt)
    pub fn crawl_delay_secs(&self, host: &str) -> Option<u64> {
        self.cache.get(host)?.crawl_delay_secs
    }

    async fn fetch_and_cache(&self, url: &Url, host: &str) {
        let robots_url = format!("{}://{}/robots.txt", url.scheme(), host);
        let rules = match self.client.get(&robots_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.text().await {
                    Ok(body) => parse_robots_txt(&body, &self.user_agent),
                    Err(_)   => empty_rules(),
                }
            }
            _ => empty_rules(),
        };
        self.cache.insert(host.to_string(), rules);
    }
}

fn empty_rules() -> Rules {
    Rules { disallowed: vec![], crawl_delay_secs: None }
}

/// Минимальный RFC-совместимый парсер robots.txt.
/// Обрабатывает User-agent, Disallow, Crawl-delay.
fn parse_robots_txt(text: &str, our_ua: &str) -> Rules {
    let our_ua_lower = our_ua.to_lowercase();
    let mut disallowed = vec![];
    // Применяем правила из секции нашего UA ИЛИ из секции *
    // Приоритет: специфичный UA > *
    let mut specific_disallowed: Vec<String> = vec![];
    let mut specific_delay: Option<u64> = None;
    let mut wildcard_disallowed: Vec<String> = vec![];
    let mut wildcard_delay: Option<u64> = None;

    // Разбиваем на секции по пустым строкам или смене User-agent
    let mut current_agents: Vec<String> = vec![];
    let mut in_specific = false;
    let mut in_wildcard = false;

    for line in text.lines() {
        let line = line.trim();
        // Игнорировать комментарии
        let line = match line.split('#').next() {
            Some(l) => l.trim(),
            None    => continue,
        };

        if line.is_empty() {
            current_agents.clear();
            in_specific = false;
            in_wildcard = false;
            continue;
        }

        if let Some(ua) = line.strip_prefix("User-agent:") {
            let ua = ua.trim();
            current_agents.push(ua.to_lowercase());
            in_specific = current_agents.iter().any(|a| a == &our_ua_lower);
            in_wildcard = current_agents.iter().any(|a| a == "*");
            continue;
        }

        if let Some(path) = line.strip_prefix("Disallow:") {
            let path = path.trim();
            if !path.is_empty() {
                if in_specific { specific_disallowed.push(path.to_string()); }
                if in_wildcard { wildcard_disallowed.push(path.to_string()); }
            }
            continue;
        }

        if let Some(delay) = line.strip_prefix("Crawl-delay:") {
            if let Ok(d) = delay.trim().parse::<u64>() {
                if in_specific { specific_delay = Some(d); }
                if in_wildcard { wildcard_delay = Some(d); }
            }
        }
    }

    // Если есть специфичные правила — используем их, иначе wildcard
    let crawl_delay_secs = if !specific_disallowed.is_empty() || specific_delay.is_some() {
        disallowed = specific_disallowed;
        specific_delay
    } else {
        disallowed = wildcard_disallowed;
        wildcard_delay
    };

    Rules { disallowed, crawl_delay_secs }
}
