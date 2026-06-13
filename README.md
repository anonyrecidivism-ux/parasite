# parasite

**An open-source, graph-based OSINT & web-reconnaissance toolkit — a free alternative to Maltego.**

> 🚧 **Status: Beta.** Core features work, but expect rough edges, breaking
> changes and incomplete Maltego parity. Feedback and issues welcome.

Drop entities (domains, IPs, emails, hashes…) onto an infinite canvas and expand
them with **transforms** that discover related entities. Everything runs locally
and in-process — no servers, no API keys, no telemetry.

> ⚠️ **For authorized security testing, research and educational use only.**
> You are responsible for complying with all applicable laws and for only
> targeting systems you own or have explicit permission to test.

---

## The graph workspace

One unified, Maltego-style window:
- An infinite, pannable / zoomable canvas with force-directed auto-layout.
- An **entity palette** with 13 types: Domain, Website, IP, Email, Phone,
  Person, **Username**, **Social Profile**, File, Hash, Port, Netblock, Phrase.
- **Right-click any node** for a context menu of transforms (the classic Maltego
  gesture), or use the details panel on the right.
- **Multi-select**: shift-drag a marquee or shift-click nodes; move or delete
  them together.
- **Machines** — one-click transform pipelines that run in waves and expand the
  graph automatically (e.g. *Domain Footprint*, *Username Recon*, *Email →
  Identity*).
- **Searchable entity list** in the sidebar — click to jump to a node.
- **Save / load** graphs as JSON, **export** to CSV, **PNG** and **PDF**.
- **17 entity types** incl. Organization, Location, ASN and CVE.

Both the in-process transforms **and** the 28 recon **operations** (the old
`parasite` engine: crawling, host analysis, fuzzing, wordlists…) live in the
same right-click menu. Operations stream their output into the log and harvest
any URLs / emails / IPs they print back onto the graph. Double-click a node to
run its default transform.

### Themes & customization
A first-run **welcome screen** walks you through the basics. Open **⚙ Settings**
(top-right) to customize:
- **3 interface layouts**: Standard, Compact, Focus (canvas-only, no palette).
- **12 built-in themes**: Anthropic, Midnight, Matrix, Dracula, Nord, Solarized,
  Cyberpunk, Ocean, Rosé, Amber, Mono, Light.
- **Custom accent colour** (full picker + quick swatches).
- **Node shape** (circle / square / diamond / hexagon), **curved edges**,
  **background style** (grid / dots / plain).
- **Node size**, **font scale** and **edge-label** toggles.

- **8 node shapes** (circle / square / diamond / triangle / pentagon / hexagon /
  octagon / **by type** — a distinct shape per entity kind).
- **Edge thickness**, curved edges, node-label toggle.

Panels are **resizable** (drag their edges). Plus **5 layout algorithms**
(force-directed / circle / grid / tree / radial), a **table view** of all
entities, and **PNG/PDF/CSV** export. Everything persists to
`~/.config/parasite/settings.json`.

Run `parasitephp --setup` to auto-install the optional OSINT CLI tools
(holehe, sherlock, maigret, subfinder, waybackurls).

### Self-install
On first launch (Linux), parasite installs a desktop entry + icon into your
application menu so you can start it like any installed app. Control it with:

```bash
parasitephp --install     # force (re)install the menu entry
parasitephp --uninstall   # remove it
parasitephp --no-install  # run without touching the menu
```

---

## Built-in transforms

Everything runs locally in pure Rust — **no API keys**, no external services to
sign up for.

| Entity    | Transforms |
|-----------|------------|
| Domain    | To Website · Resolve to IP · Subdomains (crt.sh) · Subdomains (HackerTarget) · DNS Records · WHOIS · Wayback URLs · Google Dorks · Typosquats · Harvest Emails · **Hunter.io** · **VirusTotal** · Search Links |
| Website   | Fetch & Fingerprint · Extract Links / Emails / Phones · To Domain · Find Exposed Files · robots.txt & Sitemap · Security Headers grade |
| Email     | To Domain · To Username · Gravatar · **HaveIBeenPwned breaches** · **holehe (CLI)** · Search Links |
| Person    | To Username Guesses · Search Links |
| Username  | **Hunt Accounts** (Sherlock-style, ~50 sites) · **GitHub Profile** · Search Links |
| Social    | Fetch & Fingerprint |
| IP        | Scan Ports · Reverse DNS · Geo/ASN · Reverse IP (HackerTarget) · **Shodan** · **VirusTotal** · **AbuseIPDB** · To Website |
| ASN       | Announced Prefixes (RIPEstat) |
| Phone     | Country / Region |
| Hash      | Identify Algorithm · Dictionary Lookup |

**API integrations** (add keys in ⚙ Settings → keys stay local): Shodan,
VirusTotal (host/domain/file), Have I Been Pwned, Hunter.io, AbuseIPDB. Many
transforms work with **no key** (GitHub, HackerTarget, crt.sh, CertSpotter,
RIPEstat, NVD, Wayback…).

**External tool integration** — if you have these OSINT tools installed, parasite
shells out to them and folds the results back onto the graph:
[holehe](https://github.com/megadose/holehe) (email→accounts),
[maigret](https://github.com/soxoj/maigret) (username),
[subfinder](https://github.com/projectdiscovery/subfinder) &
[theHarvester](https://github.com/laramies/theHarvester) (domain). Missing tools
just log an install hint.

**Search Links** transforms hand you ready-made queries on dozens of OSINT
services (Shodan, Censys, urlscan, IntelX, LinkedIn…) as nodes you can **open in
the browser**.

Highlights:
- **Hunt Accounts** is a self-contained, parallel re-implementation of
  [Sherlock](https://github.com/sherlock-project/sherlock)'s idea — it probes a
  bundled list of ~50 sites for a username, all in Rust, concurrently.
- **Subdomains (crt.sh)** pulls real data from Certificate Transparency logs.
- **WHOIS** talks the raw port-43 protocol with IANA referral chasing.
- **DNS Records** / **Reverse DNS** use a real resolver
  ([hickory-dns](https://github.com/hickory-dns/hickory-dns)).

All transforms live in [`src/gui/transforms.rs`](src/gui/transforms.rs)
(+ the site list in [`src/gui/sherlock.rs`](src/gui/sherlock.rs)) — adding a new
one is a single `match` arm plus a `TransformDef` entry.

---

## Build & run

Requires a recent [Rust toolchain](https://rustup.rs).

```bash
# GUI (the Maltego replacement)
cargo run --release --bin parasitephp

# Headless recon engine (TUI)
cargo run --release --bin parasite
```

Release binaries land in `target/release/` (`parasitephp` and `parasite`).
The Operations tab shells out to the `parasite` binary, so build both for full
functionality — keeping them side by side in the same directory is enough.

---

## Adding a transform

1. Add a `TransformDef { id, name, applies, desc }` to the `TRANSFORMS` slice.
2. Add a `match` arm for your `id` in `transforms::run` that pushes `NewItem`s
   (children) and/or `props` (key/values merged onto the source entity).

That's it — the UI, edges and layout pick it up automatically.

---

## License

MIT — see [LICENSE](LICENSE).
