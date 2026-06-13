//! A Sherlock-style username hunter: probe a curated list of social networks for
//! a given username. Inspired by sherlock-project/sherlock — the site list is a
//! self-contained Rust table so no Python or external data files are needed.
//!
//! Each `Site` is checked by substituting `{}` in `url` with the username.
//! Detection: HTTP 200 means the profile exists, unless `absent` is set, in which
//! case a 200 whose body contains the `absent` marker is treated as "not found".

use std::time::Duration;

pub struct Site {
    pub name:   &'static str,
    pub url:    &'static str,
    pub absent: Option<&'static str>,
}

const S: fn(&'static str, &'static str) -> Site =
    |name, url| Site { name, url, absent: None };
const A: fn(&'static str, &'static str, &'static str) -> Site =
    |name, url, absent| Site { name, url, absent: Some(absent) };

pub fn sites() -> Vec<Site> {
    vec![
        S("GitHub",        "https://github.com/{}"),
        S("GitLab",        "https://gitlab.com/{}"),
        S("Bitbucket",     "https://bitbucket.org/{}/"),
        S("Reddit",        "https://www.reddit.com/user/{}"),
        S("Telegram",      "https://t.me/{}"),
        S("Instagram",     "https://www.instagram.com/{}"),
        S("TikTok",        "https://www.tiktok.com/@{}"),
        S("Twitch",        "https://m.twitch.tv/{}"),
        S("Steam",         "https://steamcommunity.com/id/{}"),
        S("Pinterest",     "https://www.pinterest.com/{}/"),
        S("Medium",        "https://medium.com/@{}"),
        S("Dev.to",        "https://dev.to/{}"),
        S("HackerNews",    "https://news.ycombinator.com/user?id={}"),
        S("Keybase",       "https://keybase.io/{}"),
        S("Replit",        "https://replit.com/@{}"),
        S("PyPI",          "https://pypi.org/user/{}/"),
        S("npm",           "https://www.npmjs.com/~{}"),
        S("DockerHub",     "https://hub.docker.com/u/{}"),
        S("SoundCloud",    "https://soundcloud.com/{}"),
        S("Vimeo",         "https://vimeo.com/{}"),
        S("Dailymotion",   "https://www.dailymotion.com/{}"),
        S("Patreon",       "https://www.patreon.com/{}"),
        S("Gravatar",      "https://en.gravatar.com/{}"),
        S("About.me",      "https://about.me/{}"),
        S("Pastebin",      "https://pastebin.com/u/{}"),
        S("Tumblr",        "https://{}.tumblr.com"),
        S("Wordpress",     "https://{}.wordpress.com"),
        S("Flickr",        "https://www.flickr.com/people/{}"),
        S("500px",         "https://500px.com/p/{}"),
        S("Behance",       "https://www.behance.net/{}"),
        S("Dribbble",      "https://dribbble.com/{}"),
        S("DeviantArt",    "https://www.deviantart.com/{}"),
        S("Last.fm",       "https://www.last.fm/user/{}"),
        S("Letterboxd",    "https://letterboxd.com/{}/"),
        S("Chess.com",     "https://www.chess.com/member/{}"),
        S("Lichess",       "https://lichess.org/@/{}"),
        S("CodePen",       "https://codepen.io/{}"),
        S("Kaggle",        "https://www.kaggle.com/{}"),
        S("HackerOne",     "https://hackerone.com/{}"),
        S("Bugcrowd",      "https://bugcrowd.com/{}"),
        S("ProductHunt",   "https://www.producthunt.com/@{}"),
        S("VK",            "https://vk.com/{}"),
        S("Telegram(t.me)","https://t.me/s/{}"),
        S("Mastodon(.social)", "https://mastodon.social/@{}"),
        S("YouTube",       "https://www.youtube.com/@{}"),
        S("Spotify",       "https://open.spotify.com/user/{}"),
        S("Linktree",      "https://linktr.ee/{}"),
        A("Snapchat",      "https://www.snapchat.com/add/{}", "Sorry, we"),
        A("Tinder",        "https://tinder.com/@{}", "Page not found"),
    ]
}

/// Check a single site for `username`. Returns the profile URL if it likely exists.
pub async fn check(client: &reqwest::Client, site: &Site, username: &str) -> Option<String> {
    let url = site.url.replace("{}", username);
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(12))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let final_url = resp.url().to_string();
    if let Some(marker) = site.absent {
        let body = resp.text().await.unwrap_or_default();
        if body.contains(marker) {
            return None;
        }
    }
    Some(final_url)
}
