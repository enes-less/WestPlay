fn main() {
    // The app bundles the built UI (`ui/dist`, tauri.conf.json's resources), which `tauri build` makes first
    // (`npm run build:release`) and which isn't in the repository. Without it, `cargo check` or `cargo test`
    // on a fresh clone would fail here: an empty folder stands in until the UI is built.
    let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/dist");
    if !dist.exists() {
        std::fs::create_dir_all(&dist).expect("creating ui/dist");
    }
    tauri_build::build()
}
