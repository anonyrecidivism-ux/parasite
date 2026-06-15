//! ParasiteGoogle — a real, full web browser.
//!
//! It is **not** a re-implementation: it embeds **WebKitGTK** (the same engine
//! that powers GNOME Web / Epiphany) through the open-source `gtk` + `webkit2gtk`
//! bindings. So it renders real pages with real JavaScript, CSS and images. We
//! only wrap it in parasite branding — a logo, a name and a coral toolbar.
//!
//! It ships as a separate binary on purpose: a web engine needs its own GTK event
//! loop, which cannot share the egui/winit loop of the main `parasitephp` window.
//! Running it as its own process is what keeps parasite from ever freezing or
//! crashing because of the browser.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("ParasiteGoogle currently runs on Linux (WebKitGTK) only.");
}

#[cfg(target_os = "linux")]
fn main() {
    use gtk::prelude::*;
    use webkit2gtk::{WebView, WebViewExt};

    // Stable Wayland app_id / X11 class so the parasite window can target this
    // window in the compositor (Hyprland) and overlay it onto the browser panel.
    gtk::glib::set_prgname(Some("parasitegoogle"));
    gtk::glib::set_application_name("ParasiteGoogle");

    if gtk::init().is_err() {
        eprintln!("ParasiteGoogle: failed to initialise GTK.");
        return;
    }

    // first non-flag argument = URL to open; otherwise an existing nav request
    let start = std::env::args().skip(1).find(|a| !a.starts_with('-'));
    let home = "https://duckduckgo.com/";
    let start_url = start.map(|q| resolve(&q))
        .or_else(|| read_nav().map(|(_, u)| u))
        .unwrap_or_else(|| home.to_string());

    // drop the logo onto disk so GTK can load it as the window icon + toolbar mark
    let logo_path = std::env::temp_dir().join("parasitegoogle-logo.svg");
    let _ = std::fs::write(&logo_path, include_str!("../assets/logo.svg"));

    // ── theme: inherit the active parasite palette (passed via env) ─────────────
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    let bg      = env("PG_BG",      "#17120f");
    let bar     = env("PG_BAR",     "#1f1916");
    let input   = env("PG_INPUT",   "#251e1a");
    let accent  = env("PG_ACCENT",  "#e87a54");
    let text    = env("PG_TEXT",    "#f0e9e4");
    let textsec = env("PG_TEXTSEC", "#cdbfb6");
    let border  = env("PG_BORDER",  "#3a2f29");
    let css = gtk::CssProvider::new();
    let _ = css.load_from_data(format!(
        "window {{ background-color:{bg}; }}
          .pg-bar {{ background-color:{bar}; border-bottom:1px solid {border}; padding:5px; }}
          entry {{ background-color:{input}; color:{text}; border:1px solid {border};
                  border-radius:7px; padding:5px 9px; caret-color:{accent}; }}
          entry:focus {{ border-color:{accent}; }}
          button.pg {{ background:{input}; color:{textsec}; border:1px solid {border};
                      border-radius:6px; min-width:30px; padding:2px 8px; }}
          button.pg:hover {{ background:{accent}; color:{bg}; }}").as_bytes());
    if let Some(screen) = gtk::gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    // ── window ──────────────────────────────────────────────────────────────────
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("ParasiteGoogle");
    window.set_default_size(1200, 820);
    if let Ok(pb) = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&logo_path, 64, 64, true) {
        window.set_icon(Some(&pb));
    }

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // ── toolbar ─────────────────────────────────────────────────────────────────
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    bar.style_context().add_class("pg-bar");

    if let Ok(pb) = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&logo_path, 22, 22, true) {
        bar.pack_start(&gtk::Image::from_pixbuf(Some(&pb)), false, false, 4);
    }
    let name = gtk::Label::new(None);
    name.set_markup(&format!(
        "<span foreground='{text}' weight='bold'>Parasite</span><span foreground='{accent}' weight='bold'>Google</span>"));
    bar.pack_start(&name, false, false, 2);

    let btn_back   = pg_button("‹");
    let btn_fwd    = pg_button("›");
    let btn_reload = pg_button("⟳");
    bar.pack_start(&btn_back,   false, false, 0);
    bar.pack_start(&btn_fwd,    false, false, 0);
    bar.pack_start(&btn_reload, false, false, 0);

    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some("search ParasiteGoogle or type a URL"));
    entry.set_text(&start_url);
    bar.pack_start(&entry, true, true, 4);

    let btn_go = pg_button("Go");
    bar.pack_start(&btn_go, false, false, 2);

    vbox.pack_start(&bar, false, false, 0);

    // ── the real web engine ─────────────────────────────────────────────────────
    let webview = WebView::new();
    webview.load_uri(&start_url);
    vbox.pack_start(&webview, true, true, 0);

    window.add(&vbox);

    // ── wiring ──────────────────────────────────────────────────────────────────
    {
        let wv = webview.clone();
        let e = entry.clone();
        let nav = move || { wv.load_uri(&resolve(&e.text())); };
        let n1 = nav.clone();
        entry.connect_activate(move |_| n1());
        btn_go.connect_clicked(move |_| nav());
    }
    {
        let wv = webview.clone();
        btn_back.connect_clicked(move |_| if wv.can_go_back() { wv.go_back(); });
    }
    {
        let wv = webview.clone();
        btn_fwd.connect_clicked(move |_| if wv.can_go_forward() { wv.go_forward(); });
    }
    {
        let wv = webview.clone();
        btn_reload.connect_clicked(move |_| wv.reload());
    }
    {
        // keep the address bar in sync with the page, and the window title too
        let e = entry.clone();
        let win = window.clone();
        webview.connect_load_changed(move |wv, _| {
            if let Some(u) = wv.uri() { if !e.has_focus() { e.set_text(&u); } }
            let t = wv.title().map(|t| t.to_string()).unwrap_or_default();
            let title = if t.is_empty() { "ParasiteGoogle".to_string() } else { format!("{t} — ParasiteGoogle") };
            win.set_title(&title);
        });
    }

    // Watch the control file: parasite writes a URL there when a link is clicked,
    // so the single overlay window navigates instead of spawning new windows.
    {
        let wv = webview.clone();
        let last = std::rc::Rc::new(std::cell::RefCell::new(
            read_nav().map(|(t, _)| t).unwrap_or_default()));
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            if let Some((token, url)) = read_nav() {
                if token != *last.borrow() {
                    *last.borrow_mut() = token;
                    wv.load_uri(&url);
                }
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    gtk::main();
}

/// Path of the parasite↔ParasiteGoogle navigation control file.
#[cfg(target_os = "linux")]
fn nav_path() -> std::path::PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join("parasitegoogle.nav")
}

/// Read the control file as `(token, url)`. The token changes on every request
/// (even for the same URL) so a repeat click still re-navigates.
#[cfg(target_os = "linux")]
fn read_nav() -> Option<(String, String)> {
    let s = std::fs::read_to_string(nav_path()).ok()?;
    let line = s.trim();
    let (token, url) = line.split_once(' ')?;
    if url.is_empty() { return None; }
    Some((token.to_string(), url.to_string()))
}

/// Turn a bar entry (URL or search text) into a loadable URL.
#[cfg(target_os = "linux")]
fn resolve(input: &str) -> String {
    let q = input.trim();
    if q.is_empty() { return "https://duckduckgo.com/".into(); }
    if q.starts_with("http://") || q.starts_with("https://") {
        q.to_string()
    } else if q.contains('.') && !q.contains(' ') {
        format!("https://{q}")
    } else {
        let enc: String = q.bytes().map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        }).collect();
        format!("https://duckduckgo.com/?q={enc}")
    }
}

#[cfg(target_os = "linux")]
fn pg_button(label: &str) -> gtk::Button {
    use gtk::prelude::*;
    let b = gtk::Button::with_label(label);
    b.style_context().add_class("pg");
    b
}
