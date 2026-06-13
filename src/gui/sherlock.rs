//! A Sherlock-style username hunter — but tuned to avoid the false positives the
//! naive "HTTP 200 == exists" check produces. Many platforms return 200 (a login
//! wall or soft-404) for missing users, so we additionally require that the
//! username still appears in the *final* URL after redirects, and we only keep
//! sites whose missing-user behaviour is reliable (real 404 or a known marker).

use std::time::Duration;

pub struct Site {
    pub name:   &'static str,
    pub url:    &'static str,
    /// If set, a 200 whose body contains this marker means "not found".
    pub absent: Option<&'static str>,
}

const fn s(name: &'static str, url: &'static str) -> Site { Site { name, url, absent: None } }
const fn a(name: &'static str, url: &'static str, absent: &'static str) -> Site {
    Site { name, url, absent: Some(absent) }
}

/// Curated list — every entry either returns a real 404 for missing users or has
/// a reliable "not found" marker. Login-walled / bot-blocked platforms
/// (Instagram, TikTok, Facebook, X, Snapchat…) are deliberately excluded because
/// they can't be checked reliably without logging in.
pub fn sites() -> Vec<Site> {
    vec![
        // Each of these returns a real 404 for missing users (verified) — no
        // false positives. PyPI / Replit / Pinterest / WordPress were dropped
        // because they answer 200 for nonexistent accounts.
        s("GitHub",      "https://github.com/{}"),
        s("GitLab",      "https://gitlab.com/{}"),
        s("Codeberg",    "https://codeberg.org/{}"),
        s("npm",         "https://www.npmjs.com/~{}"),
        s("DockerHub",   "https://hub.docker.com/u/{}"),
        s("Keybase",     "https://keybase.io/{}"),
        s("HackerOne",   "https://hackerone.com/{}"),
        s("Bugcrowd",    "https://bugcrowd.com/{}"),
        s("Kaggle",      "https://www.kaggle.com/{}"),
        s("Dev.to",      "https://dev.to/{}"),
        s("Medium",      "https://medium.com/@{}"),
        s("Gravatar",    "https://en.gravatar.com/{}"),
        s("About.me",    "https://about.me/{}"),
        s("Behance",     "https://www.behance.net/{}"),
        s("Dribbble",    "https://dribbble.com/{}"),
        s("Last.fm",     "https://www.last.fm/user/{}"),
        s("Letterboxd",  "https://letterboxd.com/{}/"),
        s("Lichess",     "https://lichess.org/@/{}"),
        s("Chess.com",   "https://www.chess.com/member/{}"),
        s("CodePen",     "https://codepen.io/{}"),
        s("SoundCloud",  "https://soundcloud.com/{}"),
        s("Vimeo",       "https://vimeo.com/{}"),
        s("Patreon",     "https://www.patreon.com/{}"),
        s("ProductHunt", "https://www.producthunt.com/@{}"),
        s("Mastodon",    "https://mastodon.social/@{}"),
        s("Tumblr",      "https://{}.tumblr.com"),
        a("HackerNews",  "https://news.ycombinator.com/user?id={}", "No such user."),
        a("Reddit",      "https://www.reddit.com/user/{}/about.json", "\"is_suspended\""),
    ]
}

/// Check a single site for `username`. Returns the profile URL if it likely
/// exists. Conservative: prefers a miss over a false hit.
pub async fn check(client: &reqwest::Client, site: &Site, username: &str) -> Option<String> {
    let url = site.url.replace("{}", username);
    let resp = client.get(&url).timeout(Duration::from_secs(12)).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let final_url = resp.url().to_string();
    // The username must still be present in the final URL — this rejects sites
    // that 200/redirect to a login or home page for missing accounts.
    if !final_url.to_lowercase().contains(&username.to_lowercase()) {
        return None;
    }
    let display = final_url.replace("/about.json", "");
    if let Some(marker) = site.absent {
        let body = resp.text().await.unwrap_or_default();
        // Reddit's about.json: a real account returns is_suspended/created_utc.
        if site.name == "Reddit" {
            return if body.contains("\"created_utc\"") { Some(display) } else { None };
        }
        if body.contains(marker) {
            return None;
        }
    }
    Some(display)
}
