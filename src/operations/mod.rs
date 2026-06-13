// web harvest
pub mod infect;
pub mod feed;
pub mod map_colony;
pub mod leech;
pub mod replicate;
// host analysis
pub mod analyze;
pub mod probe;
pub mod ssl_inspect;
pub mod http_methods;
pub mod header_dump;
pub mod cors_probe;
// infection & spreading
pub mod shadow_crawl;
pub mod backdoor_hunter;
pub mod form_injector;
// parasite ops
pub mod dna_mutation;
pub mod necrosis_check;
pub mod content_exfil;
pub mod spawn_larvae;
pub mod dormant_check;
pub mod burrow;
// symbiosis & logic
pub mod api_parasite;
pub mod ws_leech;
pub mod symbiosis;
pub mod open_redirect;
// hash & encode
pub mod hash_tools;
pub mod encode_decode;
pub mod checksum;
// special
pub mod score;
pub mod drain;
pub mod tor_mode;
