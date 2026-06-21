//! API keys for the optional integrations. Stored in a process-global lock so
//! transforms running on tokio worker threads can read them (a thread-local
//! wouldn't be visible there).

use serde::{Deserialize, Serialize};
use std::sync::{OnceLock, RwLock};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ApiKeys {
    #[serde(default)] pub shodan:     String,
    #[serde(default)] pub virustotal: String,
    #[serde(default)] pub hibp:       String,
    #[serde(default)] pub hunter:     String,
    #[serde(default)] pub abuseipdb:  String,
    #[serde(default)] pub claude:     String,
    #[serde(default)] pub gemini:     String,
    // extended OSINT providers
    #[serde(default)] pub securitytrails: String,
    #[serde(default)] pub greynoise:  String,
    #[serde(default)] pub ipinfo:     String,
    #[serde(default)] pub binaryedge: String,
    #[serde(default)] pub fullhunt:   String,
    #[serde(default)] pub leakix:     String,
    #[serde(default)] pub intelx:     String,
    #[serde(default)] pub urlscan:    String,
    #[serde(default)] pub zoomeye:    String,
    #[serde(default)] pub builtwith:  String,
    #[serde(default)] pub numverify:  String,
    #[serde(default)] pub whoisxml:   String,
    #[serde(default)] pub censys_id:  String,
    #[serde(default)] pub censys_secret: String,
    // AI / LLM providers
    #[serde(default)] pub openai:     String,
    #[serde(default)] pub mistral:    String,
    #[serde(default)] pub deepseek:   String,
    #[serde(default)] pub groq:       String,
    #[serde(default)] pub openrouter: String,
    #[serde(default)] pub xai:        String,
    /// which AI provider to use ("" = auto-pick the first with a key)
    #[serde(default)] pub ai_provider: String,
    /// optional model-id override (empty = the provider's default)
    #[serde(default)] pub ai_model:   String,
}

fn store() -> &'static RwLock<ApiKeys> {
    static S: OnceLock<RwLock<ApiKeys>> = OnceLock::new();
    S.get_or_init(|| RwLock::new(ApiKeys::default()))
}

pub fn set(k: ApiKeys) {
    *store().write().unwrap() = k;
}

pub fn get(name: &str) -> String {
    let k = store().read().unwrap();
    match name {
        "shodan"     => k.shodan.clone(),
        "virustotal" => k.virustotal.clone(),
        "hibp"       => k.hibp.clone(),
        "hunter"     => k.hunter.clone(),
        "abuseipdb"  => k.abuseipdb.clone(),
        "claude"     => k.claude.clone(),
        "gemini"     => k.gemini.clone(),
        "securitytrails" => k.securitytrails.clone(),
        "greynoise"  => k.greynoise.clone(),
        "ipinfo"     => k.ipinfo.clone(),
        "binaryedge" => k.binaryedge.clone(),
        "fullhunt"   => k.fullhunt.clone(),
        "leakix"     => k.leakix.clone(),
        "intelx"     => k.intelx.clone(),
        "urlscan"    => k.urlscan.clone(),
        "zoomeye"    => k.zoomeye.clone(),
        "builtwith"  => k.builtwith.clone(),
        "numverify"  => k.numverify.clone(),
        "whoisxml"   => k.whoisxml.clone(),
        "censys_id"  => k.censys_id.clone(),
        "censys_secret" => k.censys_secret.clone(),
        "openai"     => k.openai.clone(),
        "mistral"    => k.mistral.clone(),
        "deepseek"   => k.deepseek.clone(),
        "groq"       => k.groq.clone(),
        "openrouter" => k.openrouter.clone(),
        "xai"        => k.xai.clone(),
        "ai_provider"=> k.ai_provider.clone(),
        "ai_model"   => k.ai_model.clone(),
        _ => String::new(),
    }
}
