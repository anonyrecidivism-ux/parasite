# Parasite OSINT — Android build

The desktop app runs on `eframe`; on Android there is no eframe, so the GUI is
driven by our own `winit` event loop with a `wgpu` backend. The **entire GUI is
reused verbatim** (`src/gui/`) — only the windowing shell differs:

| Layer            | Desktop                     | Android                                   |
|------------------|-----------------------------|-------------------------------------------|
| Window / loop    | `eframe::run_native`        | `winit` + `android-activity` (`android_main`) |
| Renderer         | eframe (glow/wgpu)          | `egui-wgpu` + `egui-winit`                |
| Entry file       | `src/gui_main.rs` (bin)     | `src/android_lib.rs` (cdylib)             |
| GUI code         | `src/gui/**` (shared)       | `src/gui/**` (shared)                     |

`src/android_lib.rs` is gated with `#![cfg(target_os = "android")]`, so on any
other target it compiles to an empty library and the desktop build is unaffected.

## What works on the phone

Everything network-driven — i.e. the heart of the tool:
- the graph canvas (pan / zoom / drag / select), all entity kinds,
- the ~400 transforms, the rule/insight engine, cases, dossier, export/import.

Disabled on Android (desktop-only): the embedded **WebKitGTK browser**
(ParasiteGoogle), spawning sibling processes, and native desktop file dialogs.
These are `cfg`-gated and degrade to no-ops, so nothing crashes.

## Prerequisites (one-time)

```bash
rustup target add aarch64-linux-android
cargo install cargo-apk

# Android NDK r26d + SDK (build-tools 34, platform 34) — already present here at:
export ANDROID_HOME="$HOME/android/sdk"
export ANDROID_NDK_ROOT="$HOME/android/android-ndk-r26d"
export ANDROID_NDK_HOME="$HOME/android/android-ndk-r26d"
```

## Build

```bash
# Debug APK (fast, ~65 MB, unstripped):
cargo apk build --lib

# Release APK (optimized + stripped, much smaller — slower first build):
cargo apk build --lib --release
```

Output: `target/debug/apk/parasite-osint.apk`
(or `target/release/apk/…` for `--release`).

`--lib` is required so cargo-apk packages the cdylib and skips the desktop bins
(`parasite`, `parasitephp`, `parasitegoogle`), which are Linux-only.

## Install / run on a device

```bash
adb install -r target/debug/apk/parasite-osint.apk
adb shell am start -n com.parasite.osint/android.app.NativeActivity
adb logcat -s parasite RustStdoutStderr   # logs (android_logger)
```

## Notes

- App id: `com.parasite.osint`, label **Parasite OSINT**, minSdk 24, targetSdk 34.
- Only `arm64-v8a` is built (covers all modern phones). To also target an x86_64
  emulator, add `x86_64-linux-android` to `build_targets` in
  `[package.metadata.android]` and `rustup target add x86_64-linux-android`.
- The debug APK is signed with the auto-generated `~/.android/debug.keystore`.
  For Play/distribution, sign the release APK with your own keystore via
  `apksigner`.
