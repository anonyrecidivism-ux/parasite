//! AI integration — lets Claude or Gemini reason over a target and propose a
//! graph of entities + relationships to investigate. Raw HTTP (Rust has no
//! official Anthropic SDK); the user's key lives in Settings → API keys and is
//! sent only to the chosen provider.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use super::model::Kind;

#[derive(Clone, Copy, PartialEq)]
enum Backend { Anthropic, Gemini, OpenAi }

/// The resolved AI configuration: which provider, its key, base URL and model.
#[derive(Clone)]
pub struct AiCfg { key: String, kind: Backend, base: String, pub model: String, name: String }

impl AiCfg {
    pub fn label(&self) -> String { format!("{} ({})", self.name, self.model) }
}

/// Every selectable AI provider: (id, display name, default model). The id is
/// also the API-key name in the keys store.
pub const PROVIDERS: &[(&str, &str, &str)] = &[
    ("claude",     "Claude",     "claude-opus-4-8"),
    ("gemini",     "Gemini",     "gemini-2.5-flash"),
    ("openai",     "OpenAI",     "gpt-4o"),
    ("mistral",    "Mistral",    "mistral-large-latest"),
    ("deepseek",   "DeepSeek",   "deepseek-chat"),
    ("groq",       "Groq",       "llama-3.3-70b-versatile"),
    ("openrouter", "OpenRouter", "openai/gpt-4o-mini"),
    ("xai",        "xAI Grok",   "grok-2-latest"),
];

fn provider_info(id: &str) -> Option<(Backend, &'static str, &'static str, &'static str)> {
    Some(match id {
        "claude"     => (Backend::Anthropic, "", "claude-opus-4-8", "Claude"),
        "gemini"     => (Backend::Gemini, "", "gemini-2.5-flash", "Gemini"),
        "openai"     => (Backend::OpenAi, "https://api.openai.com/v1", "gpt-4o", "OpenAI"),
        "mistral"    => (Backend::OpenAi, "https://api.mistral.ai/v1", "mistral-large-latest", "Mistral"),
        "deepseek"   => (Backend::OpenAi, "https://api.deepseek.com/v1", "deepseek-chat", "DeepSeek"),
        "groq"       => (Backend::OpenAi, "https://api.groq.com/openai/v1", "llama-3.3-70b-versatile", "Groq"),
        "openrouter" => (Backend::OpenAi, "https://openrouter.ai/api/v1", "openai/gpt-4o-mini", "OpenRouter"),
        "xai"        => (Backend::OpenAi, "https://api.x.ai/v1", "grok-2-latest", "xAI Grok"),
        _ => return None,
    })
}

/// Resolve the active AI config: the chosen provider (or the first with a key),
/// its key, base URL and model (settings override or provider default).
pub fn cfg() -> Option<AiCfg> {
    let chosen = super::keys::get("ai_provider");
    let id = if !chosen.trim().is_empty() && !super::keys::get(chosen.trim()).trim().is_empty() {
        chosen.trim().to_string()
    } else {
        PROVIDERS.iter().map(|p| p.0)
            .find(|p| !super::keys::get(p).trim().is_empty())?.to_string()
    };
    let key = super::keys::get(&id);
    if key.trim().is_empty() { return None; }
    let (kind, base, default_model, name) = provider_info(&id)?;
    let m = super::keys::get("ai_model");
    let model = if m.trim().is_empty() { default_model.to_string() } else { m.trim().to_string() };
    Some(AiCfg { key, kind, base: base.to_string(), model, name: name.to_string() })
}

fn client() -> reqwest::Client {
    super::net::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("client")
}

#[derive(Clone, Copy, PartialEq)]
pub enum ChatRole { User, Assistant }

/// One completion call (single user turn). Returns the model's text or an error.
pub async fn complete(cfg: &AiCfg, system: &str, user: &str) -> Result<String, String> {
    chat(cfg, system, &[(ChatRole::User, user.to_string())]).await
}

/// A multi-turn chat call. `history` is the full conversation (the API is
/// stateless, so we resend it each turn).
pub async fn chat(cfg: &AiCfg, system: &str, history: &[(ChatRole, String)]) -> Result<String, String> {
    match cfg.kind {
        Backend::Anthropic => anthropic_call(cfg, system, history).await,
        Backend::Gemini    => gemini_call(cfg, system, history).await,
        Backend::OpenAi    => openai_call(cfg, system, history).await,
    }
}

async fn anthropic_call(cfg: &AiCfg, system: &str, history: &[(ChatRole, String)]) -> Result<String, String> {
    let messages: Vec<Value> = history.iter().map(|(r, t)| json!({
        "role": if *r == ChatRole::User { "user" } else { "assistant" }, "content": t,
    })).collect();
    let body = json!({ "model": cfg.model, "max_tokens": 4096, "system": system, "messages": messages });
    let resp = client().post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &cfg.key).header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let j: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(j["error"]["message"].as_str().unwrap_or("Claude API error").to_string());
    }
    j["content"][0]["text"].as_str().map(|s| s.to_string())
        .ok_or_else(|| "Claude returned no text".to_string())
}

async fn gemini_call(cfg: &AiCfg, system: &str, history: &[(ChatRole, String)]) -> Result<String, String> {
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        cfg.model, cfg.key);
    let contents: Vec<Value> = history.iter().map(|(r, t)| json!({
        "role": if *r == ChatRole::User { "user" } else { "model" }, "parts": [{ "text": t }],
    })).collect();
    let body = json!({
        "system_instruction": { "parts": [{ "text": system }] },
        "contents": contents, "generationConfig": { "temperature": 0.5 },
    });
    let resp = client().post(&url).header("content-type", "application/json")
        .json(&body).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let j: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(j["error"]["message"].as_str().unwrap_or("Gemini API error").to_string());
    }
    j["candidates"][0]["content"]["parts"][0]["text"].as_str().map(|s| s.to_string())
        .ok_or_else(|| "Gemini returned no text".to_string())
}

/// OpenAI-compatible chat completions (OpenAI / Mistral / DeepSeek / Groq / …).
async fn openai_call(cfg: &AiCfg, system: &str, history: &[(ChatRole, String)]) -> Result<String, String> {
    let mut messages = vec![json!({ "role": "system", "content": system })];
    for (r, t) in history {
        messages.push(json!({ "role": if *r == ChatRole::User { "user" } else { "assistant" }, "content": t }));
    }
    let body = json!({ "model": cfg.model, "messages": messages, "temperature": 0.5 });
    let url = format!("{}/chat/completions", cfg.base);
    let resp = client().post(&url)
        .header("Authorization", format!("Bearer {}", cfg.key))
        .header("content-type", "application/json")
        .json(&body).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let j: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(j["error"]["message"].as_str().unwrap_or("API error").to_string());
    }
    j["choices"][0]["message"]["content"].as_str().map(|s| s.to_string())
        .ok_or_else(|| "model returned no text".to_string())
}

/// Fetch the text of a list of source URLs (HTML stripped, capped) so the chat
/// can analyze the actual pages, not just the dossier facts.
pub async fn fetch_sites(urls: Vec<(String, String)>) -> String {
    let cli = client();
    let mut out = String::new();
    for (label, url) in urls.into_iter().take(6) {
        if !url.starts_with("http") { continue; }
        match cli.get(&url).send().await {
            Ok(r) => match r.text().await {
                Ok(body) => {
                    let text = strip_html(&body);
                    let cap: String = text.chars().take(2800).collect();
                    out.push_str(&format!("\n=== {label} ({url}) ===\n{cap}\n"));
                }
                Err(_) => {}
            },
            Err(_) => {}
        }
    }
    out
}

/// Very rough HTML→text: drop script/style, strip tags, collapse whitespace.
fn strip_html(html: &str) -> String {
    // (the regex crate has no backreferences, so list the two tags explicitly)
    let re_block = regex::Regex::new(
        r"(?is)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>").unwrap();
    let no_block = re_block.replace_all(html, " ");
    let re_tag = regex::Regex::new(r"(?s)<[^>]+>").unwrap();
    let no_tags = re_tag.replace_all(&no_block, " ");
    let re_ws = regex::Regex::new(r"\s+").unwrap();
    re_ws.replace_all(&no_tags, " ").trim().to_string()
}

// ── Graph-building prompt + parse ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AiEntity {
    pub kind: String,
    pub value: String,
    #[serde(default)] pub note: String,
}

#[derive(Deserialize)]
pub struct AiEdge {
    pub from: usize,
    pub to: usize,
    #[serde(default)] pub label: String,
}

#[derive(Deserialize)]
pub struct AiPlan {
    #[serde(default)] pub entities: Vec<AiEntity>,
    #[serde(default)] pub edges: Vec<AiEdge>,
}

/// Build the (system, user) prompt asking the model for a graph plan.
pub fn graph_prompt(instruction: &str, context: &str) -> (String, String) {
    let kinds = Kind::ALL.iter().map(|k| k.label()).collect::<Vec<_>>().join(", ");
    let system = format!(
        "You are an OSINT analyst assistant inside 'parasite', a Maltego-style graph tool. \
         Given a target or instruction, design a graph of entities and the relationships \
         between them that an investigator should map and verify. Think like an analyst: \
         infer likely domains, usernames, emails, organizations, people, infrastructure, \
         social profiles and how they connect.\n\n\
         Use ONLY these entity kinds (use the exact label): {kinds}.\n\n\
         Respond with STRICT JSON and nothing else, in this shape:\n\
         {{\"entities\":[{{\"kind\":\"Person\",\"value\":\"...\",\"note\":\"why this matters\"}}],\
         \"edges\":[{{\"from\":0,\"to\":1,\"label\":\"relationship\"}}]}}\n\
         'from'/'to' are 0-based indices into the entities array. Keep it to ~6-18 entities. \
         These are leads to investigate, not confirmed facts — put any uncertainty in 'note'. \
         Do not include markdown fences or commentary."
    );
    let user = if context.trim().is_empty() {
        format!("Target / instruction:\n{instruction}")
    } else {
        format!("Target / instruction:\n{instruction}\n\n\
                 The graph already contains these entities — expand from them, don't repeat them:\n{context}")
    };
    (system, user)
}

/// Pull an `<<<ACTIONS … ACTIONS>>>` block out of a chat reply. Returns the
/// reply with the block removed, plus the parsed actions JSON if present.
pub fn extract_actions(text: &str) -> (String, Option<Value>) {
    const OPEN: &str = "<<<ACTIONS";
    const CLOSE: &str = "ACTIONS>>>";
    if let (Some(s), Some(e)) = (text.find(OPEN), text.find(CLOSE)) {
        if e > s {
            let inner = &text[s + OPEN.len()..e];
            let cleaned = format!("{}{}", &text[..s], &text[e + CLOSE.len()..])
                .trim().to_string();
            return (cleaned, parse_json(inner).ok());
        }
    }
    (text.to_string(), None)
}

/// Lenient JSON-object parse: tolerates ```json fences and surrounding prose.
pub fn parse_json(text: &str) -> Result<Value, String> {
    let t = text.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t);
    let slice = match (t.find('{'), t.rfind('}')) {
        (Some(a), Some(b)) if b > a => &t[a..=b],
        _ => t,
    };
    serde_json::from_str::<Value>(slice).map_err(|e| format!("could not parse AI JSON: {e}"))
}

/// Parse a model reply into an AiPlan, tolerating ```json fences and prose.
pub fn parse_plan(text: &str) -> Result<AiPlan, String> {
    let t = text.trim();
    // strip code fences if present
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t);
    // isolate the outermost JSON object
    let slice = match (t.find('{'), t.rfind('}')) {
        (Some(a), Some(b)) if b > a => &t[a..=b],
        _ => t,
    };
    serde_json::from_str::<AiPlan>(slice).map_err(|e| format!("could not parse AI JSON: {e}"))
}

/// Map an AI-provided kind label (lenient) to a Kind.
pub fn kind_from_label(label: &str) -> Kind {
    let l = label.trim().to_lowercase();
    Kind::ALL.into_iter()
        .find(|k| k.label().to_lowercase() == l)
        .unwrap_or_else(|| match l.as_str() {
            "domain" | "hostname"        => Kind::Domain,
            "url" | "site" | "web"       => Kind::Website,
            "ip" | "ipv4" | "ipv6"       => Kind::Ip,
            "mail" | "e-mail"            => Kind::Email,
            "tel" | "phone number"       => Kind::Phone,
            "name" | "individual"        => Kind::Person,
            "handle" | "alias" | "nick"  => Kind::Username,
            "social" | "profile"         => Kind::Social,
            "org" | "company" | "business" => Kind::Organization,
            "place" | "address" | "city" => Kind::Location,
            "btc" | "bitcoin"            => Kind::BtcAddress,
            "eth" | "ethereum"           => Kind::EthAddress,
            "tx" | "transaction hash"    => Kind::Transaction,
            "mac"                        => Kind::MacAddress,
            "gps" | "coordinates"        => Kind::Coordinate,
            "doc" | "file"               => Kind::Document,
            _                            => Kind::Phrase,
        })
}
