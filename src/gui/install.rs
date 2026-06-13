//! Desktop integration — on first launch parasite installs itself into the
//! system application menu (like a "real" installed app), so the user can launch
//! it from their desktop environment afterwards. Linux/freedesktop only; a no-op
//! elsewhere. Idempotent and quiet; controllable via CLI flags.

#[cfg(target_os = "linux")]
use std::path::PathBuf;

/// The user's virus logo, with the CSS-variable fallback colours baked into the
/// brand palette so desktop environments render it consistently.
#[cfg(target_os = "linux")]
fn icon_svg() -> String {
    include_str!("../../assets/logo.svg")
        .replace("rgb(127, 44, 40)", "rgb(217,119,87)")   // body / spots → coral
        .replace("rgb(20, 20, 19)",  "rgb(60,52,46)")     // spikes
        .replace("rgb(61, 61, 58)",  "rgb(120,108,96)")   // connections
        .replace("rgb(255, 255, 255)", "rgb(20,17,15)")   // hollow core → dark
}

pub const BANNER: &str = r#"
   ╔═══════════════════════════════════════════════╗
       ◍  p a r a s i t e   —   OSINT graph
       a free, open-source Maltego alternative
   ╚═══════════════════════════════════════════════╝
        \\   ◍╍╍◍        infect · expand · pivot
         \\ ◍╍◍ ◍╍◍
          ◍╍◍ ◍ ◍╍◍     74+ transforms · 11 machines
         ◍╍◍ ◍╍◍ ◍
"#;

pub fn print_banner() {
    println!("{BANNER}");
    println!("  tip: run `parasitephp --setup` to auto-install the OSINT CLI tools");
    println!("       (holehe, sherlock, maigret, …) used by some transforms.\n");
}

/// Handle install-related CLI flags. Returns `true` if the program should exit
/// immediately (e.g. `--install` / `--uninstall` / `--setup` were handled).
pub fn handle_cli() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--setup") {
        setup();
        return true;
    }
    if args.iter().any(|a| a == "--uninstall") {
        uninstall();
        return true;
    }
    if args.iter().any(|a| a == "--install") {
        match install(true) {
            Ok(p)  => println!("✓  installed: {p}"),
            Err(e) => eprintln!("✗  install failed: {e}"),
        }
        return true;
    }
    // Auto-install on normal launch unless suppressed.
    if !args.iter().any(|a| a == "--no-install") {
        let _ = install(false);
    }
    false
}

#[cfg(target_os = "linux")]
fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// True if `a` is newer than `b` (or `b` can't be read — treat as needs update).
#[cfg(target_os = "linux")]
fn is_newer(a: &std::path::Path, b: &std::path::Path) -> bool {
    use std::fs;
    match (fs::metadata(a).and_then(|m| m.modified()), fs::metadata(b).and_then(|m| m.modified())) {
        (Ok(ma), Ok(mb)) => ma > mb,
        _ => true,
    }
}

/// Download & install the optional OSINT CLI tools the transforms can use.
pub fn setup() {
    use std::process::Command;
    print_banner();
    println!("⇣  Installing OSINT tools (this can take a minute)…\n");

    // Python tools via pip (user site).
    let pip_tools = ["holehe", "maigret", "sherlock-project"];
    let pip = if Command::new("pip").arg("--version").output().is_ok() { "pip" } else { "pip3" };
    for t in pip_tools {
        print!("  • {t} … ");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let ok = Command::new(pip)
            .args(["install", "--user", "--break-system-packages", "--upgrade", t])
            .status().map(|s| s.success()).unwrap_or(false);
        println!("{}", if ok { "✓" } else { "✗ (install Python/pip and retry)" });
    }

    // Go tools (optional) — only if `go` is present.
    if Command::new("go").arg("version").output().is_ok() {
        for t in ["github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest",
                  "github.com/tomnomnom/waybackurls@latest"] {
            let short = t.rsplit('/').next().unwrap_or(t).split('@').next().unwrap_or(t);
            print!("  • {short} (go) … ");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let ok = Command::new("go").args(["install", t]).status().map(|s| s.success()).unwrap_or(false);
            println!("{}", if ok { "✓" } else { "✗" });
        }
    } else {
        println!("  • subfinder/waybackurls: install Go then `go install …` (skipped)");
    }

    // Make sure the parasite engine + desktop entry are in place.
    match install(false) {
        Ok(p) => println!("\n✓  desktop entry: {p}"),
        Err(e) => println!("\n✗  desktop install: {e}"),
    }
    println!("\n✓  setup done. Launch with `parasitephp` or from your app menu.");
}

/// Install desktop entry + icon, copying the binaries to a stable location.
/// `force` rewrites everything; otherwise it's skipped if already present.
#[cfg(target_os = "linux")]
pub fn install(force: bool) -> std::io::Result<String> {
    use std::fs;

    let home = home().ok_or_else(|| err("no HOME"))?;
    let apps_dir  = home.join(".local/share/applications");
    let icon_dir  = home.join(".local/share/icons/hicolor/scalable/apps");
    let inst_dir  = home.join(".local/share/parasite");
    let desktop   = apps_dir.join("parasite.desktop");
    let exe       = std::env::current_exe()?;
    let gui_dst   = inst_dir.join("parasitephp");

    // (Re)install when forced, when not yet installed, or when this binary is
    // newer than the installed copy — so launching a freshly-built version
    // automatically updates the one in the app menu.
    let need = force
        || !desktop.exists()
        || (exe != gui_dst && is_newer(&exe, &gui_dst));
    if !need {
        return Ok(desktop.display().to_string());
    }
    if desktop.exists() && !force {
        println!("↻  newer build detected — updating the installed parasite copy…");
    }

    fs::create_dir_all(&apps_dir)?;
    fs::create_dir_all(&icon_dir)?;
    fs::create_dir_all(&inst_dir)?;

    // Copy the GUI binary (and the recon engine beside it, if present) into the
    // stable install dir so the menu entry keeps working after a `cargo clean`.
    if exe != gui_dst {
        let _ = fs::copy(&exe, &gui_dst);
    }
    if let Some(dir) = exe.parent() {
        let engine = dir.join("parasite");
        if engine.exists() {
            let _ = fs::copy(&engine, inst_dir.join("parasite"));
        }
    }

    fs::write(icon_dir.join("parasite.svg"), icon_svg())?;

    let entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=parasite\n\
         GenericName=OSINT Graph\n\
         Comment=Open-source graph-based OSINT & reconnaissance — a Maltego alternative\n\
         Exec={} --no-install\n\
         Icon=parasite\n\
         Terminal=false\n\
         Categories=Network;Security;Utility;\n\
         Keywords=osint;maltego;recon;security;graph;\n",
        gui_dst.display()
    );
    fs::write(&desktop, entry)?;

    // Best-effort refresh of the menu database.
    let _ = std::process::Command::new("update-desktop-database")
        .arg(&apps_dir).status();

    Ok(desktop.display().to_string())
}

#[cfg(target_os = "linux")]
pub fn uninstall() {
    use std::fs;
    if let Some(home) = home() {
        let _ = fs::remove_file(home.join(".local/share/applications/parasite.desktop"));
        let _ = fs::remove_file(home.join(".local/share/icons/hicolor/scalable/apps/parasite.svg"));
        let _ = fs::remove_dir_all(home.join(".local/share/parasite"));
        println!("✓  uninstalled parasite desktop entry");
    }
}

#[cfg(target_os = "linux")]
fn err(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, msg)
}

// ── Non-Linux stubs ────────────────────────────────────────────────────────────
#[cfg(not(target_os = "linux"))]
pub fn install(_force: bool) -> std::io::Result<String> {
    Ok("desktop install is only implemented on Linux".into())
}
#[cfg(not(target_os = "linux"))]
pub fn uninstall() {
    println!("desktop uninstall is only implemented on Linux");
}
