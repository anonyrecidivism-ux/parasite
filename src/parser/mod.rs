use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

/// Тип страницы, определяемый эвристически по URL и содержимому
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PageType {
    Api,
    Documentation,
    Blog,
    UserProfile,
    Catalog,
    Product,
    Unknown,
}

impl std::fmt::Display for PageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api           => write!(f, "API"),
            Self::Documentation => write!(f, "Docs"),
            Self::Blog          => write!(f, "Blog"),
            Self::UserProfile   => write!(f, "Profile"),
            Self::Catalog       => write!(f, "Catalog"),
            Self::Product       => write!(f, "Product"),
            Self::Unknown       => write!(f, "Page"),
        }
    }
}

/// Парсер HTML-страниц: извлекает ссылки, заголовок, определяет тип страницы
pub struct HtmlParser;

impl HtmlParser {
    /// Извлечь все абсолютные HTTP(S)-ссылки из HTML
    pub fn extract_links(html: &str, base_url: &Url) -> Vec<Url> {
        let document = Html::parse_document(html);

        // Ленивая инициализация не нужна — Selector::parse дешевый
        let a_sel  = Selector::parse("a[href]").expect("valid selector");
        let from_href = document
            .select(&a_sel)
            .filter_map(|el| el.value().attr("href"))
            .filter_map(|href| base_url.join(href).ok());

        // Некоторые SPA-фреймворки используют <link href="...">
        let link_sel = Selector::parse("link[href]").expect("valid selector");
        let from_link = document
            .select(&link_sel)
            .filter_map(|el| el.value().attr("href"))
            .filter_map(|href| base_url.join(href).ok());

        let mut links: Vec<Url> = from_href
            .chain(from_link)
            .filter(|u| u.scheme() == "http" || u.scheme() == "https")
            .collect();

        links.dedup_by(|a, b| a.as_str() == b.as_str());
        links
    }

    /// Определить тип страницы по URL и мета-тегам
    pub fn detect_page_type(url: &Url, html: &str) -> PageType {
        let path = url.path().to_lowercase();

        // URL-эвристики (быстрая ветка, без парсинга HTML)
        if path.contains("/api/") || path.contains("/swagger") || path.contains("/openapi")
            || path.contains("/graphql") || path.ends_with("/api")
        {
            return PageType::Api;
        }
        if path.contains("/docs/") || path.contains("/documentation") || path.contains("/wiki/")
            || path.contains("/reference/") || path.contains("/manual/")
        {
            return PageType::Documentation;
        }
        if path.contains("/blog/") || path.contains("/posts/") || path.contains("/articles/")
            || path.contains("/news/") || path.contains("/press/")
        {
            return PageType::Blog;
        }
        if path.contains("/users/") || path.contains("/profile/") || path.contains("/u/")
            || path.contains("/@")
        {
            return PageType::UserProfile;
        }
        if path.contains("/product/") || path.contains("/products/") || path.contains("/item/")
            || path.contains("/sku/")
        {
            return PageType::Product;
        }
        if path.contains("/catalog/") || path.contains("/category/") || path.contains("/shop/")
            || path.contains("/store/") || path.contains("/collection/")
        {
            return PageType::Catalog;
        }

        // Fallback: заглянуть в HTML (только для Unknown пока)
        let document = Html::parse_document(html);
        if let Ok(sel) = Selector::parse("meta[name='description'], meta[property='og:type']") {
            for el in document.select(&sel) {
                let content = el.value().attr("content").unwrap_or("").to_lowercase();
                if content.contains("article") || content.contains("blog") {
                    return PageType::Blog;
                }
                if content.contains("product") {
                    return PageType::Product;
                }
            }
        }

        PageType::Unknown
    }

    /// Извлечь <title> страницы
    pub fn extract_title(html: &str) -> Option<String> {
        let document = Html::parse_document(html);
        let sel = Selector::parse("title").ok()?;
        let title = document
            .select(&sel)
            .next()?
            .text()
            .collect::<String>();
        let trimmed = title.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    }
}

/// Нормализовать URL: убрать фрагмент, схлопнуть trailing slash
pub fn normalize_url(mut url: Url) -> Url {
    url.set_fragment(None);
    let path = url.path().to_owned();
    if path.len() > 1 && path.ends_with('/') {
        url.set_path(path.trim_end_matches('/'));
    }
    url
}
