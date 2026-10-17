//! The companion's Customize: the player's own wallpapers and icon sets (`westplay_host::customize`), added
//! through Windows' pickers (`customize-adding` once one's closed with a choice: the copying and checking
//! begins, which a large photo or .zip makes take a while), listed, and removed. Each change is told to the host, which tells the cabin page
//! (`customize_changed`). Which wallpaper and icon set are shown are settings (`look`): removing the one
//! shown goes back to WestPlay's own.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::{AppHandle, Emitter, State, WebviewWindow};
use westplay_host::customize::{self, Added, Customizations};
use westplay_setup::pick;

use crate::SettingsStore;

/// The `customize` folder, and the count the host watches for changes in it.
pub struct Customize {
    pub dir: PathBuf,
    pub changed: Arc<AtomicU64>,
}

impl Customize {
    fn changed(&self) {
        self.changed.fetch_add(1, Ordering::AcqRel);
    }
}

/// The image kinds a wallpaper can be, for the picker.
const WALLPAPER_TYPES: [&str; 7] = ["jpg", "jpeg", "png", "webp", "gif", "bmp", "svg"];

/// The player's wallpapers and icon sets.
#[tauri::command]
pub async fn customize_list(customize: State<'_, Customize>) -> Result<Customizations, String> {
    let dir = customize.dir.clone();
    blocking(move || Ok(customize::list(&dir))).await
}

/// Asks for images (Windows' picker: `title`, its filter called `kinds`), or (`folder`) a folder of them, and
/// adds them as wallpapers, each with its other look (`silk.jpg`, `silk.dark.jpg`). None: the player cancelled.
#[tauri::command]
pub async fn add_wallpapers(
    window: WebviewWindow,
    folder: bool,
    title: String,
    kinds: String,
    customize: State<'_, Customize>,
) -> Result<Option<Added>, String> {
    let owner = owner(&window);
    let dir = customize.dir.clone();
    let added = blocking(move || {
        let added = if folder {
            let Some(source) = pick::folder(owner, &title) else { return Ok(None) };
            let _ = window.emit("customize-adding", ());
            customize::add_wallpapers_from_folder(&dir, &source)
        } else {
            let files = pick::files(owner, &title, &kinds, &WALLPAPER_TYPES, true);
            if files.is_empty() {
                return Ok(None);
            }
            let _ = window.emit("customize-adding", ());
            customize::add_wallpapers(&dir, &files)
        };
        added.map(Some).map_err(|e| e.to_string())
    })
    .await?;
    if added.as_ref().is_some_and(|added| !added.added.is_empty()) {
        customize.changed();
    }
    Ok(added)
}

/// Asks for an icon set (`zip`: a .zip, its filter called `kinds`; otherwise a folder) and adds it. None: the
/// player cancelled.
#[tauri::command]
pub async fn add_icon_set(
    window: WebviewWindow,
    zip: bool,
    title: String,
    kinds: String,
    customize: State<'_, Customize>,
) -> Result<Option<Added>, String> {
    let owner = owner(&window);
    let dir = customize.dir.clone();
    let added = blocking(move || {
        let source =
            if zip { pick::files(owner, &title, &kinds, &["zip"], false).pop() } else { pick::folder(owner, &title) };
        let Some(source) = source else { return Ok(None) };
        let _ = window.emit("customize-adding", ());
        let added = if zip {
            customize::add_icon_set_from_zip(&dir, &source)
        } else {
            customize::add_icon_set_from_folder(&dir, &source)
        };
        added.map(Some).map_err(|e| e.to_string())
    })
    .await?;
    if added.as_ref().is_some_and(|added| !added.added.is_empty()) {
        customize.changed();
    }
    Ok(added)
}

/// Removes a wallpaper; the one shown, removed, gives way to WestPlay's own.
#[tauri::command]
pub async fn remove_wallpaper(
    id: String,
    customize: State<'_, Customize>,
    store: State<'_, SettingsStore>,
    app: AppHandle,
) -> Result<(), String> {
    let dir = customize.dir.clone();
    let target = id.clone();
    blocking(move || customize::remove_wallpaper(&dir, &target).map_err(|e| e.to_string())).await?;
    customize.changed();
    forget(&store, &app, |look| {
        if look.wallpaper.as_deref() == Some(id.as_str()) {
            look.wallpaper = None;
        }
    })
}

/// Removes an icon set; the one shown, removed, gives way to WestPlay's own icons.
#[tauri::command]
pub async fn remove_icon_set(
    name: String,
    customize: State<'_, Customize>,
    store: State<'_, SettingsStore>,
    app: AppHandle,
) -> Result<(), String> {
    let dir = customize.dir.clone();
    let target = name.clone();
    blocking(move || customize::remove_icon_set(&dir, &target).map_err(|e| e.to_string())).await?;
    customize.changed();
    forget(&store, &app, |look| {
        if look.icon_set.as_deref() == Some(name.as_str()) {
            look.icon_set = None;
        }
    })
}

/// Changes the look settings, if `change` does, and keeps and announces them as `update_settings` does.
fn forget(
    store: &SettingsStore,
    app: &AppHandle,
    change: impl FnOnce(&mut westplay_host::settings::LookSettings),
) -> Result<(), String> {
    let mut settings = store.shared.get();
    let before = settings.look.clone();
    change(&mut settings.look);
    if settings.look == before {
        return Ok(());
    }
    store.file.save(&settings).map_err(|e| format!("Couldn't save the settings: {e}"))?;
    store.shared.set(settings.clone());
    let _ = app.emit("settings-changed", &settings);
    Ok(())
}

fn owner(window: &WebviewWindow) -> isize {
    window.hwnd().map(|hwnd| hwnd.0 as isize).unwrap_or(0)
}

/// Runs `work` off the async runtime's threads (pickers, files, decoding images).
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| e.to_string())?
}
