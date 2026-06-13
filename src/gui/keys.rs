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
        _ => String::new(),
    }
}
