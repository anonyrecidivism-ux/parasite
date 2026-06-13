# Changelog

All notable changes to **parasite** — the open-source, graph-based OSINT toolkit
(a free Maltego alternative).

The project went through five rapid development iterations that together formed
the first public beta (**Beta 1**). This release consolidates and greatly extends
them into **Beta 2**.

---

## [1.0.0-beta.2] — Beta 2 — 2026-06-14

The "make it a real Maltego competitor" release.

### Added
- **Graph analytics** panel — degree centrality (most-connected nodes),
  connected components/clusters, density, average degree, isolates (with
  one-click select), and a by-type breakdown.
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
