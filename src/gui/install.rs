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

/// Handle install-related CLI flags. Returns `true` if the program should exit
/// immediately (e.g. `--install` / `--uninstall` were handled).
pub fn handle_cli() -> bool {
    let args: Vec<String> = std::env::args().collect();
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

    if desktop.exists() && !force {
        return Ok(desktop.display().to_string());
    }

    fs::create_dir_all(&apps_dir)?;
    fs::create_dir_all(&icon_dir)?;
    fs::create_dir_all(&inst_dir)?;

    // Copy the GUI binary (and the recon engine beside it, if present) into the
    // stable install dir so the menu entry keeps working after a `cargo clean`.
    let exe = std::env::current_exe()?;
    let gui_dst = inst_dir.join("parasitephp");
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
