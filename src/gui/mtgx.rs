//! Import Maltego `.mtgx` graphs. An `.mtgx` is a ZIP archive containing one or
//! more GraphML files under `Graphs/`. Each `<node>` embeds a Maltego entity
//! (`<mtg:MaltegoEntity type="maltego.Domain">…<mtg:Value>…</mtg:Value>`); edges
//! reference node ids. We map Maltego entity types onto our own `Kind`s.

use std::io::{self, Read, Write};

use egui::Pos2;
use regex::Regex;

use super::model::{Graph, Kind};

fn to_io<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e.to_string())
}

fn map_type(t: &str) -> Kind {
    match t {
        "Domain" | "DNSName"                    => Kind::Domain,
        "Website" | "URL"                       => Kind::Website,
        "IPv4Address" | "IPv6Address"           => Kind::Ip,
        "Netblock"                              => Kind::Netblock,
        "AS"                                    => Kind::Asn,
        "EmailAddress"                          => Kind::Email,
        "PhoneNumber"                           => Kind::Phone,
        "Person"                                => Kind::Person,
        "Alias" | "Twit" | "Account"            => Kind::Username,
        "Company" | "Organization"              => Kind::Organization,
        "Location" | "GPS"                      => Kind::Location,
        "Hash"                                  => Kind::Hash,
        "Document" | "File"                     => Kind::Document,
        "Port"                                  => Kind::Port,
        "Service" | "Banner"                    => Kind::Service,
        "Vulnerability" | "CVE"                 => Kind::Cve,
        "Coordinate"                            => Kind::Coordinate,
        _                                       => Kind::Phrase,
    }
}

fn unescape(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
        .replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'")
        .trim().to_string()
}

/// Maltego entity type + main property name for a kind (used when exporting).
fn rev_type(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Domain       => ("maltego.Domain", "fqdn"),
        Kind::Website      => ("maltego.URL", "url"),
        Kind::Ip           => ("maltego.IPv4Address", "ipv4-address"),
        Kind::Netblock     => ("maltego.Netblock", "ipv4-range"),
        Kind::Asn          => ("maltego.AS", "as.number"),
        Kind::Email        => ("maltego.EmailAddress", "email"),
        Kind::Phone        => ("maltego.PhoneNumber", "phonenumber"),
        Kind::Person       => ("maltego.Person", "person.fullname"),
        Kind::Username     => ("maltego.Alias", "alias"),
        Kind::Social       => ("maltego.URL", "url"),
        Kind::Organization => ("maltego.Company", "company"),
        Kind::Location     => ("maltego.Location", "location.name"),
        Kind::Cve          => ("maltego.Vulnerability", "vulnerability.id"),
        Kind::Hash         => ("maltego.Hash", "properties.hash"),
        Kind::Port         => ("maltego.Port", "port.number"),
        Kind::Service      => ("maltego.Service", "service.name"),
        Kind::Document     => ("maltego.Document", "title"),
        _                  => ("maltego.Phrase", "text"),
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&apos;")
}

/// Export the graph as a Maltego `.mtgx` archive.
pub fn export(path: &str, graph: &Graph) -> io::Result<()> {
    let mut ids: Vec<u64> = graph.entities.keys().copied().collect();
    ids.sort_unstable();
    let index: std::collections::HashMap<u64, usize> =
        ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();

    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\" \
                  xmlns:mtg=\"http://maltego.paterva.com/xml/mtgx\">\n");
    xml.push_str("<key id=\"d0\" for=\"node\" yfiles.type=\"entity\"/>\n");
    xml.push_str("<graph edgedefault=\"directed\">\n");

    for (&id, &i) in &index {
        let e = &graph.entities[&id];
        let (etype, prop) = rev_type(e.kind);
        xml.push_str(&format!(
            "<node id=\"n{i}\"><data key=\"d0\"><mtg:MaltegoEntity type=\"{etype}\">\
             <mtg:Properties><mtg:Property name=\"{prop}\" displayName=\"{}\">\
             <mtg:Value>{}</mtg:Value></mtg:Property></mtg:Properties>\
             </mtg:MaltegoEntity></data></node>\n",
            e.kind.label(), xml_escape(&e.value)));
    }
    for e in &graph.edges {
        if let (Some(&a), Some(&b)) = (index.get(&e.from), index.get(&e.to)) {
            xml.push_str(&format!("<edge source=\"n{a}\" target=\"n{b}\"/>\n"));
        }
    }
    xml.push_str("</graph>\n</graphml>\n");

    let file = std::fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("Graphs/Graph1.graphml", opts).map_err(to_io)?;
    zip.write_all(xml.as_bytes())?;
    zip.finish().map_err(to_io)?;
    Ok(())
}

pub fn import(path: &str) -> io::Result<Graph> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file).map_err(to_io)?;

    let mut xml = String::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(to_io)?;
        if f.name().ends_with(".graphml") {
            let mut s = String::new();
            f.read_to_string(&mut s).ok();
            xml.push_str(&s);
        }
    }
    if xml.is_empty() {
        return Err(to_io("no .graphml found inside the .mtgx archive"));
    }
    Ok(parse_graphml(&xml))
}

fn parse_graphml(xml: &str) -> Graph {
    let mut g = Graph::new();

    let node_re  = Regex::new(r#"(?s)<node\b[^>]*\bid="([^"]+)"[^>]*>(.*?)</node>"#).unwrap();
    let type_re  = Regex::new(r#"type="maltego\.([^"]+)""#).unwrap();
    let value_re = Regex::new(r#"(?s)<mtg:Value>(.*?)</mtg:Value>"#).unwrap();
    let edge_re  = Regex::new(r#"<edge\b[^>]*>"#).unwrap();
    let src_re   = Regex::new(r#"source="([^"]+)""#).unwrap();
    let tgt_re   = Regex::new(r#"target="([^"]+)""#).unwrap();

    let mut id_map: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut idx = 0usize;

    for caps in node_re.captures_iter(xml) {
        let nid = caps[1].to_string();
        let body = &caps[2];
        let kind = type_re.captures(body).map(|c| map_type(&c[1])).unwrap_or(Kind::Phrase);
        let value = value_re.captures(body).map(|c| unescape(&c[1])).unwrap_or_default();
        let value = if value.is_empty() { kind.label().to_string() } else { value };

        // grid placement; user can re-layout afterwards
        let cols = 8.0;
        let pos = Pos2::new((idx as f32 % cols) * 150.0 - 525.0,
                            (idx as f32 / cols).floor() * 130.0 - 300.0);
        let eid = g.add(kind, value, pos);
        id_map.insert(nid, eid);
        idx += 1;
    }

    for m in edge_re.find_iter(xml) {
        let tag = m.as_str();
        let src = src_re.captures(tag).map(|c| c[1].to_string());
        let tgt = tgt_re.captures(tag).map(|c| c[1].to_string());
        if let (Some(s), Some(t)) = (src, tgt) {
            if let (Some(&a), Some(&b)) = (id_map.get(&s), id_map.get(&t)) {
                g.link(a, b, "");
            }
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_roundtrip() {
        use egui::Pos2;
        let mut g = Graph::new();
        let a = g.add(Kind::Domain, "example.com", Pos2::ZERO);
        let b = g.add(Kind::Ip, "93.184.216.34", Pos2::ZERO);
        g.link(a, b, "resolves");
        let path = std::env::temp_dir().join("parasite_test_roundtrip.mtgx");
        let p = path.to_str().unwrap();
        export(p, &g).expect("export");
        let g2 = import(p).expect("import");
        assert_eq!(g2.entities.len(), 2);
        assert_eq!(g2.edges.len(), 1);
        assert!(g2.find(Kind::Domain, "example.com").is_some());
        assert!(g2.find(Kind::Ip, "93.184.216.34").is_some());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parses_maltego_graphml() {
        let xml = r#"<graphml>
          <node id="n0"><data key="d"><mtg:MaltegoEntity type="maltego.Domain">
            <mtg:Properties><mtg:Property name="fqdn"><mtg:Value>example.com</mtg:Value></mtg:Property></mtg:Properties>
          </mtg:MaltegoEntity></data></node>
          <node id="n1"><data key="d"><mtg:MaltegoEntity type="maltego.IPv4Address">
            <mtg:Value>93.184.216.34</mtg:Value>
          </mtg:MaltegoEntity></data></node>
          <edge source="n0" target="n1"/>
        </graphml>"#;
        let g = parse_graphml(xml);
        assert_eq!(g.entities.len(), 2, "two nodes");
        assert_eq!(g.edges.len(), 1, "one edge");
        assert!(g.find(Kind::Domain, "example.com").is_some());
        assert!(g.find(Kind::Ip, "93.184.216.34").is_some());
    }
}
