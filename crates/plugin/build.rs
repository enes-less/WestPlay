//! The Visual C++ runtime linked into the plugin, as Tauri links it into the app: a player's PC without the Visual
//! C++ Redistributable has no `VCRUNTIME140.dll`, and the game couldn't load the plugin at all ("unable to load:
//! 0000007E" in its game.log, a player's 2026-10-26: no plugin.log, no screens), while the app ran. The C runtime
//! proper (the UCRT) stays Windows' own, part of Windows 10 and 11.
//!
//! Taken from ChrisDenton's `static_vcruntime` (MIT/Apache-2.0, <https://github.com/ChrisDenton/static_vcruntime/>),
//! as `tauri-build` takes it, for debug builds too.

use std::fs;
use std::io::Write;
use std::path::Path;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    override_msvcrt_lib();
    // The libraries Rust doesn't hard-code that would clash.
    for lib in [
        "libvcruntimed.lib",
        "vcruntime.lib",
        "vcruntimed.lib",
        "libcmtd.lib",
        "msvcrt.lib",
        "msvcrtd.lib",
        "libucrt.lib",
        "libucrtd.lib",
    ] {
        println!("cargo:rustc-link-arg=/NODEFAULTLIB:{lib}");
    }
    // The ones wanted: the runtime's start-up and the VC runtime static, the UCRT Windows' own.
    for lib in ["libcmt.lib", "libvcruntime.lib", "ucrt.lib"] {
        println!("cargo:rustc-link-arg=/DEFAULTLIB:{lib}");
    }
}

/// Rust hard-codes `msvcrt.lib`: an (almost) empty one in its place, found first.
fn override_msvcrt_lib() {
    let machine: &[u8] = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => &[0x64, 0x86],
        Ok("x86") => &[0x4C, 0x01],
        _ => return,
    };
    let bytes: &[u8] = &[
        1, 0, 94, 3, 96, 98, 60, 0, 0, 0, 1, 0, 0, 0, 0, 0, 132, 1, 46, 100, 114, 101, 99, 116, 118, 101, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 60, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 16, 0, 46, 100, 114, 101, 99,
        116, 118, 101, 0, 0, 0, 0, 1, 0, 0, 0, 3, 0, 4, 0, 0, 0,
    ];
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let path = Path::new(&out_dir).join("msvcrt.lib");
    if let Ok(mut file) = fs::OpenOptions::new().write(true).create_new(true).open(path) {
        file.write_all(machine).expect("writing the empty msvcrt.lib");
        file.write_all(bytes).expect("writing the empty msvcrt.lib");
    }
    println!("cargo:rustc-link-search=native={out_dir}");
}
