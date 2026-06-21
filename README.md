# parasite

**An open-source, graph-based OSINT & web-reconnaissance toolkit — a free alternative to Maltego.**

![Rust](https://img.shields.io/badge/built%20with-Rust-orange)
![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-blue)
[![Release](https://github.com/anonyrecidivism-ux/parasite/actions/workflows/release.yml/badge.svg)](../../actions/workflows/release.yml)
![License](https://img.shields.io/badge/license-MIT-green)

> 🚧 **Status: Beta.** Core features work, but expect rough edges, breaking
> changes and incomplete Maltego parity. Feedback and issues welcome.

## Quickstart

```bash
# 1. install Rust  →  https://rustup.rs
# 2. build & run the GUI
cargo run --release --bin parasitephp
```

Then: drop an entity from the left **palette** → **right-click** it (or `Ctrl+K`)
to run transforms → open **λ Insights** for next-step suggestions. Optional:
`ffmpeg` for MP4 export; add AI keys in **⚙ Settings** (everything also works
without them). On **Linux** also build the WebKitGTK browser:
`cargo build --release --bin parasitegoogle`.

Drop entities (domains, IPs, emails, hashes…) onto an infinite canvas and expand
them with **transforms** that discover related entities. Everything runs locally
and in-process — no servers, no API keys, no telemetry.

> ⚠️ **For authorized security testing, research and educational use only.**
> You are responsible for complying with all applicable laws and for only
> targeting systems you own or have explicit permission to test.

---

## Modes

Switch in the top bar between: **◇ Graph**, **◎ GEOINT**, **⏱ Monitor**,
**🗂 Dossier**, **🗃 Cases**, **📡 Watch**, **🧰 Toolbox** and **🌐 ParasiteGoogle**.

- **🗃 Cases** — manage several investigations; save the current graph as a named
  case and switch between them.
- **📡 Watch** — keep an eye on a domain (new certs/subdomains), GitHub user, or
  BTC address; it alerts you when anything changes. Keyless, on-demand or on a timer.
- **🧰 Toolbox** — offline utilities: Base64/Hex/URL encode-decode, MD5/SHA-1/SHA-256,
  a **JWT decoder**, username/name variant generator, Google-dork builder, coordinate
  converter. No network, no keys.
- **🗂 Dossier** — type a person/org/place and it assembles a readable profile from
  Wikipedia + Wikidata (and optionally your own JSON DB), exportable to Markdown/JSON/PNG/PDF.

### ⏱ Monitor — live crypto feed
A **live transaction feed** — no addresses to enter. It streams recent **BTC,
ETH & TON** transactions straight into a scrolling list, shows each one's value in
coin and ≈USD, and lets you filter to only the big "whale" moves. Click a row to
open it in a block explorer. A **PRICES · 24h** ticker tracks **rises (▲) and
falls (▼)** across BTC, ETH, TON, **USDT**, SOL, XRP, BNB and DOGE. All via free,
key-less public APIs.

### 🌐 ParasiteGoogle — a real browser
A **real, full web browser** built on **WebKitGTK** (the engine behind GNOME Web) —
real JavaScript, CSS, images, the lot. It ships as its own `parasitegoogle` binary
with a parasite-branded toolbar (logo + address bar + back/forward/reload). The
ParasiteGoogle tab is a branded home screen that opens it, and **every "open in
browser" action** across the graph and GEOINT launches it too.

It runs as a **separate process on purpose**: a web engine needs its own GTK event
loop and cannot share egui's, so keeping it out-of-process is what guarantees a
slow or heavy page can never freeze or crash the main parasite window. *(Linux only
— it uses the system `webkit2gtk-4.1`.)*

### ◎ GEOINT — geospatial intelligence  *(beta)*
A standalone geospatial workspace:
- A live **slippy map** (OpenStreetMap **+ satellite**) — pan, smooth cursor-anchored
  zoom, click to drop points, drag markers to move them.
- **EXIF GPS extraction** from images — point a photo at it and parasite plots the
  location and shows camera make/model and capture time.
- **Markers** with reverse geocoding (OpenStreetMap Nominatim), DMS coordinates,
  and **distance measuring** between points.
- One-click links into **Google Maps, Google Earth, Street View** and OSM.

## The graph workspace

One unified, Maltego-style window:
- An infinite, pannable / zoomable canvas with **8 layout algorithms**
  (force-directed, tree, radial, circle, spiral, grid, columns-by-type, scatter).
- An **entity palette** with **25 types** grouped into Maltego-style categories
  (Infrastructure, Personal, Locations, Malware & Files, Cryptocurrency, Other).
- **Right-click any node** for a context menu of transforms (the classic Maltego
  gesture), a **command palette** (`Ctrl+K`), or the details panel on the right.
- **Multi-select**: shift-drag a marquee or shift-click nodes; move or delete
  them together.
- **Machines** — one-click transform pipelines that run in waves and expand the
  graph automatically (e.g. *Domain Footprint*, *Username Recon*, *Email →
  Identity*).
- **Searchable entity list** in the sidebar — click to jump to a node.
- **Save / load** graphs as JSON, **import Maltego `.mtgx`**, **export** to CSV,
  **PNG**, **PDF**, a self-contained **HTML report**, and an **animated MP4** (via ffmpeg).
- **Auto-saved session** — your graph is restored after a restart or crash.
- **25 entity types** — every type has at least one transform.

### 🧠 λ Insights — a rule engine, not AI
A deterministic "smart graph" advisor with **no AI**: a tiny embedded **Lisp**
interpreter runs editable rules over facts computed from your graph and suggests
the next move — coverage gaps, shared infrastructure (co-hosting, common registrar),
cycles, hubs, duplicates, unchecked reputation. Each suggestion **explains which
facts fired it** (`∵ …`), can **highlight the nodes** it refers to and **run** the
relevant transform/machine in one click. **⚑ Auto-triage** actually checks services
(GreyNoise for IPs, HTTP liveness for hosts) and **sets flags**. Rules live in a
**live-reloaded in-app editor** and can be **exported / imported** as packs.
Per-node **risk scores** (0–100) are drawn as coloured rings. Fully offline,
explainable, free.

Both the in-process transforms **and** the 28 recon **operations** (the old
`parasite` engine: crawling, host analysis, fuzzing, wordlists…) live in the
same right-click menu. Operations stream their output into the log and harvest
any URLs / emails / IPs they print back onto the graph. Double-click a node to
run its default transform.

### Look & customization
A first-run **welcome screen** walks you through the basics. Open **⚙ Settings**
(top-right) to customize:
- **3 interface designs**: **Stock** (the original dark Parasite look), **Cupertino**
  (clean, light, soft — Apple-like, flat, no glassmorphism), and **Retro Unix** — a
  grey **Motif/CDE** old-Linux workstation with 3D bevels, square corners, a
  monospace UI font and **vector entity pictograms** (globe, monitor, person,
  red map-pins…). The retro design pins its own palette.
- **Built-in themes** (for Stock/Cupertino): Anthropic, Midnight, Matrix, Dracula,
  Nord, Solarized, Cyberpunk, Ocean, Rosé, Amber, Mono, Light, Paper, Cupertino…
- **Custom accent colour**, node shape/style, curved edges, background style,
  node size, font scale, edge-label toggle, and a **custom UI font** (`.ttf/.otf`).
- **Network**: a **proxy** (SOCKS5/HTTP) for all traffic, **DuckDuckGo/Google**
  search choice, and a **block-insecure-HTTP** switch.

Localized in **English / Russian / Ukrainian**. Panels are resizable; plus a
**table view**, a **graph-analytics** panel (degree & **betweenness** centrality,
connected components, density), and a **Coverage board** (which checks have run per
entity). Everything persists to `~/.config/parasite/settings.json`.

The installed menu copy **auto-updates**: launch a freshly-built version and it
refreshes the binary in `~/.local/share/parasite/` automatically.

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

## Transforms

**320+ transforms** — ~110 in-process ones (pure Rust, mostly **no API keys**)
plus **200+ one-click "pivots"** that open an entity in the relevant OSINT service
(VirusTotal, Shodan, Censys, crt.sh, GreyNoise, AbuseIPDB, HIBP, Hunter, IntelX,
Etherscan, Blockchair, GitHub/Telegram/Reddit…). **19 machines** chain them into
one-click pipelines (Domain Deep Recon, IP Full Profile, Email Breach Sweep,
Username 360, Phone Profile, BTC/ETH Wallet Trace…).

**Optional AI** (natural-language graph building, chat over the graph/dossier) via
**8 providers** — Claude, Gemini, OpenAI, Mistral, DeepSeek, Groq, OpenRouter,
xAI Grok — with a model picker. Keys are local and optional; the rule-based λ
Insights engine needs no AI at all.

The in-process transforms below all run locally:

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
The Operations tab shells out to the `parasite` binary, so build both — keeping
them side by side in the same directory is enough.

### Platforms
- **Linux** — full functionality, including the WebKitGTK browser
  (`parasitegoogle`). Needs `libgtk-3-dev` + `libwebkit2gtk-4.1-dev`.
- **macOS / Windows** — the GUI (`parasitephp`) and engine (`parasite`) build and
  run; "open in browser" actions fall back to your system browser (the embedded
  WebKitGTK browser is Linux-only). Build with
  `cargo build --release --bin parasitephp --bin parasite`.

Pre-built binaries for **Linux, macOS (Apple Silicon; Intel runs via Rosetta 2)
and Windows** are produced by the GitHub Actions release workflow — push a `v*`
tag (or run it manually) and the platform bundles are attached to the release.

Optional: `parasitephp --setup` auto-installs OSINT CLI tools (holehe, sherlock,
maigret, subfinder, waybackurls) that some transforms shell out to. `ffmpeg` is
needed for **MP4 video export**.

---

## Adding a transform

1. Add a `TransformDef { id, name, applies, desc }` to the `TRANSFORMS` slice.
2. Add a `match` arm for your `id` in `transforms::run` that pushes `NewItem`s
   (children) and/or `props` (key/values merged onto the source entity).

That's it — the UI, edges and layout pick it up automatically.

---

## License

MIT — see [LICENSE](LICENSE).
