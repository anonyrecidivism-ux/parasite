# Changelog

All notable changes to **parasite** — the open-source, graph-based OSINT toolkit
(a free Maltego alternative).

The project went through five rapid development iterations that together formed
the first public beta (**Beta 1**). This release consolidates and greatly extends
them into **Beta 2**.

---

## [1.0.0-beta.4] — Beta 4 — 2026-06-16

The "real browser, real intel" release: a genuine embedded web browser, a live
price ticker, and a big batch of new transforms & tool integrations.

### Added — 🌐 ParasiteGoogle, a real embedded browser
- A **real web browser** built on **WebKitGTK** (the engine behind GNOME Web) —
  real JavaScript, CSS, images, video. Shipped as its own `parasitegoogle` binary
  with a parasite-branded toolbar (logo, address bar, back / forward / reload).
- It is **embedded into the ParasiteGoogle tab**: on Hyprland the browser window is
  floated and tracked **exactly over the browser panel** so it reads as in-app,
  while running **out-of-process** (a web engine needs its own GTK loop and cannot
  share egui's). Falls back to a standalone window on other compositors.
- **Theme-aware** — the browser chrome inherits the active parasite palette
  (background, accent, text, borders) via the theme you've selected.
- **Every "open in browser" action** across the graph & GEOINT now opens here.

### Added — Monitor
- **Price rises & falls** — a *PRICES · 24h* ticker showing each tracked coin's
  price and 24-hour change (▲ green / ▼ red): BTC, ETH, **TON**, **USDT**, SOL,
  XRP, BNB, DOGE.
- **TON** added to the live transaction feed (toncenter) with its own filter.

### Added — Transforms (now 99) & tool integrations
- New key-less API transforms: **RapidDNS** subdomains, **RDAP** for domains & IPs
  (registrar / netblock / org), **SPF** include parsing, **DMARC** policy,
  **RIPEstat** BGP (announcing ASN, prefix & holder), **Keybase** identity proofs,
  **GitLab** profile, **Hacker News** profile.
- New GitHub CLI tool integrations (shell-out, like sherlock/subfinder):
  **assetfinder**, **amass** (passive), **gau**, **httpx** (probe), **katana**
  (crawl). `parasitephp --setup` now installs them via `go install`.

### Fixed
- **"Open in browser" links** no longer rely on a flaky `xdg-open`.
- **"Application not responding" / freezes** — all compositor (Hyprland) I/O for the
  browser overlay was moved to a **background thread**, so the egui UI thread is
  never blocked waiting on `hyprctl`.
- **OOM / fork-storm crash** — the old positioning failed silently on Hyprland's new
  Lua API and span up an endless resize→reposition loop. Reworked to the new API,
  hard-throttled, single-instance (a stray browser can never accumulate WebKit
  processes), and killed cleanly on exit.
- **Browser opening a stale site on startup / respawning when closed** — the browser
  now opens only on an explicit click, clears its control file at launch, and closing
  its window returns you to the launcher instead of relaunching.
- Hardened a potential string-slice panic in the Monitor feed.

### Repo
- Stopped tracking scratch/export artifacts (`*.json`, `*.pdf`, `*.png`, `*.mp4`,
  `*.txt`, `*.csv`, logs) and removed the committed `graph.mp4` / `johndoe.txt`.

---

## [1.0.0-beta.3] — Beta 3 — 2026-06-14

### Added
- **GEOINT mode** ⚠️ *experimental / very buggy / unfinished* — a standalone
  geospatial workspace: live OpenStreetMap **and satellite** (Esri) slippy map
  with smooth fractional zoom, markers, EXIF-GPS extraction from images, reverse
  geocoding, distance measuring, and Google Maps/Earth/Street View links. (No
  in-app 3D / Street View — that needs Google's proprietary SDK; use the links.)
- **Video export (MP4)** — record an animated reveal of the graph to an H.264
  video with a theme-adaptive parasite logo + name watermark. Length is unlimited
  (records until the whole reveal animates in + 2 s) and the output filename is
  configurable. Requires `ffmpeg`.
- **Manual linking** — Ctrl-drag node→node to connect; click an edge + Delete to
  disconnect; Link/Unlink for two selected nodes; editable edge labels.
- **Undo/Redo** (Ctrl+Z/Y), **notes & flags** on nodes, **minimap**, **CSV
  import**, **Maltego `.mtgx` export**, **spawn + edge-draw animations**.
- **Node styles** (Flat / Material You / Neon / Outline) and **cluster colouring**.
- **Graph analytics**: degree **and betweenness** centrality, components, density.
- Transforms for **every** entity type (80 total): BTC, MAC, geocoding, file
  hashing, **Shodan InternetDB** (free), **ip-api**, OTX, urlscan, CIRCL, NVD…

### Fixed
- Right-click context menu (Run-all / Delete) not registering.
- Video cutting off half-way on large graphs; node-style rendering glitches.
- Map zoom jerkiness (now smooth fractional zoom).

---

## [1.0.0-beta.2] — Beta 2 — 2026-06-14

The "make it a real Maltego competitor" release.

### Added
- **GEOINT mode** — a standalone geospatial workspace with a live OpenStreetMap
  slippy map (pan/zoom/markers), **EXIF GPS extraction** from images, reverse
  geocoding, distance measuring, and Google Maps/Earth/Street View links.
- **Spawn animation** — nodes pop in (ease-out-back) when created.
- **Manual linking** — Ctrl-drag node→node to connect; click an edge + Delete to
  disconnect; Link/Unlink buttons for two selected nodes; editable edge labels.
- **Undo/Redo** (Ctrl+Z/Y), **notes & flags** on nodes, **minimap**,
  **CSV import**, **Maltego `.mtgx` export**, and keyboard shortcuts.
- **Maltego `.mtgx` import** — open Maltego graphs (ZIP/GraphML); entity types
  are mapped onto parasite kinds.
- **Transforms for every entity type** — BTC (balance via blockchain.info), MAC
  vendor lookup, Coordinate/Location geocoding (OpenStreetMap), File hashing, and
  search/pivot links for Document/OS/Service/Netblock/Port/Phrase. 74 in-process
  transforms total.
- **Graph analytics** panel — degree **and betweenness** centrality, connected
  components/clusters, density, average degree, isolates (with one-click select),
  and a by-type breakdown.
- **Cluster colouring** — tint nodes by connected component.
- More customization — label size, node-icon toggle, cluster colouring toggle.
- **Auto-update** — launching a newer build refreshes the installed copy in
  `~/.local/share/parasite/` automatically.
- **Interface variants** — 3 layouts selectable in Settings: Standard, Compact,
  Focus (canvas-only, no palette) + a quick **＋ New** entity popup.
- **Themes** — now 12: Anthropic, Midnight, Matrix, Dracula, Nord, Solarized,
  Cyberpunk, Ocean, Rosé, Amber, Mono, Light.
- **Entity types** — now 23: added BTC Address, MAC Address, Coordinate,
  Document, Service, OS.
- **Node shapes** — now 8: circle, square, diamond, triangle, pentagon, hexagon,
  octagon, and **By type** (a distinct shape per entity kind).
- **Layout algorithms** — now 5: force-directed, circle, grid, **tree/hierarchical**,
  **radial/concentric** (in the `⊹ Layout ▾` menu).
- **Resizable panels** — drag the palette / details / log edges.
- **Customization** — edge thickness, node-label toggle, plus existing accent,
  node size, font scale, grid/dots/plain background, curved edges.
- **Transforms** — Phone (Normalize, Country, Search Links), Organization
  (Guess Domain, Google Dorks, Search Links). 60 in-process transforms total.
- **Startup ASCII banner** and a **`--setup`** command that auto-installs the
  optional OSINT CLI tools (holehe, sherlock, maigret, subfinder, waybackurls).
- **PATH augmentation** so user-installed CLI tools resolve from desktop launches.

### Fixed
- **Sherlock false positives** — "HTTP 200 == exists" was wrong (many sites 200
  for missing users). Detection now requires the username to remain in the final
  URL after redirects, uses a curated/verified site list and a browser UA, and
  drops PyPI/Replit/Pinterest/WordPress.
- **holehe / maigret** integration — added `python3 -m <tool>` fallbacks; tools
  are now installable via `--setup`.
- **Crash with many nodes** — the force-directed layout produced NaN from a
  zero-length normalize on coincident nodes, crashing the renderer. Hardened with
  a safe direction helper and finite guards (regression test added).
- **Compact layout** — the details panel could overflow ("slide off") because
  long property values (e.g. CVE descriptions) didn't wrap; values now wrap.
- **All compiler warnings** removed across both binaries.

---

## Beta 1 — iterative development (2026-06-13 → 06-14)

The five increments that built the foundation:

### Stage 5 — competitor polish
- Multi-select with shift-drag marquee; move/delete selected together.
- **Machines** — one-click transform pipelines that expand the graph in waves
  (Domain Footprint, Website Recon, Username Recon, Email→Identity, …; 11 total).
- Unified app: the 28 recon **operations** of the `parasite` engine surfaced as
  transforms in the same right-click menu, streaming output and harvesting
  URLs/emails/IPs back onto the graph.
- API integrations: Shodan, VirusTotal (host/domain/file), Have I Been Pwned,
  Hunter.io, AbuseIPDB, CertSpotter, NVD, RIPEstat, GitHub, HackerTarget.
- External CLI tools: holehe, maigret, sherlock, subfinder, theHarvester,
  waybackurls. "Search Links" pivots for dozens of OSINT services.

### Stage 4 — theme-adaptive logo
- The placeholder diamond replaced everywhere with the parasite "virus" logo,
  ported to direct egui painting so it adopts the active theme's colours.

### Stage 3 — API integrations & more transforms
- First API integrations and the keyless real APIs (GitHub, HackerTarget,
  crt.sh, RIPEstat). awesome-osint-style "Search Links".

### Stage 2 — entity types, customization, export
- Added Organization/Location/ASN/CVE entity types.
- Node shape / curved edges / background style customization.
- Export the canvas to **PNG** and **PDF**.

### Stage 1 — first public beta
- Graph workspace: entity palette, infinite canvas (pan/zoom/drag), force-directed
  layout, details/properties panel, save/load JSON, CSV export.
- In-process transforms with no API keys: crt.sh, WHOIS, DNS/PTR, Wayback, dorks,
  security headers, robots, IP geo/ASN, Gravatar, hash tools, and a Sherlock-style
  username account hunter.
- Right-click context menu of transforms; searchable entity list.
- 7 themes + accent/customization, first-run welcome, Linux desktop self-install.
- Two binaries: `parasitephp` (GUI) and `parasite` (recon engine / TUI).
