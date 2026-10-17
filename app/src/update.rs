//! WestPlay's updates, from its releases on GitHub: each release's `latest.json` (the updater's endpoint in
//! tauri.conf.json) names its installer and the installer's signature, made with WestPlay's private key and
//! checked here against its public key. An update is looked for a little after WestPlay starts, every few
//! hours after, and when the companion asks (its About page). One found is offered in the companion
//! (`update-found`), with its notes (the changelog's), and installed only when the player says so
//! ([`install_update`]), with the game closed: downloaded (`update-progress`), checked, and then Windows'
//! installer runs on its own, without questions, and starts WestPlay again.
//!
//! The plugin and the mod in the game are the new WestPlay's once it's started again: the companion puts
//! them in (setup's `updated`).
//!
//! A development build is never updated: it looks for nothing.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use westplay_host::log;

/// The first look, once WestPlay has started (not to slow its start).
const FIRST_LOOK: Duration = Duration::from_secs(30);
/// Then this often.
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// After a look that couldn't be made (no network, GitHub not answering, no release yet): again this soon.
const RETRY: Duration = Duration::from_secs(30 * 60);

/// The update found, if one was.
#[derive(Default)]
pub struct Updates(Mutex<Option<Update>>);

/// An update, as the companion offers it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    version: String,
    /// What's new: the changelog's notes for it (Markdown, a list).
    notes: String,
    /// When it was released, `YYYY-MM-DD`.
    date: Option<String>,
}

impl From<&Update> for Offer {
    fn from(update: &Update) -> Self {
        Self {
            version: update.version.clone(),
            notes: update.body.clone().unwrap_or_default(),
            date: update.date.map(|date| date.date().to_string()),
        }
    }
}

/// How far the download is, for the companion.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    done: u64,
    total: Option<u64>,
}

/// Looks for updates in the background, from now on: a little after the start, then every few hours.
pub fn watch(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    let looking = std::thread::Builder::new().name("westplay-updates".into()).spawn(move || {
        std::thread::sleep(FIRST_LOOK);
        loop {
            let looked = tauri::async_runtime::block_on(look(&app));
            std::thread::sleep(if looked.is_ok() { EVERY } else { RETRY });
        }
    });
    if let Err(error) = looking {
        log!("[update] couldn't start looking for updates: {error}");
    }
}

/// Looks for an update: one found is kept for [`install_update`] and told to the companion (`update-found`).
/// `Err` when the look couldn't be made.
async fn look(app: &AppHandle) -> Result<Option<Offer>, ()> {
    // The host stopped before the installer runs, as when WestPlay quits: the installer replaces its files.
    let handle = app.clone();
    let updater = match app.updater_builder().on_before_exit(move || crate::stop_host(&handle)).build() {
        Ok(updater) => updater,
        Err(error) => {
            log!("[update] the updater isn't set up: {error}");
            return Err(());
        }
    };
    let found = match updater.check().await {
        Ok(found) => found,
        Err(error) => {
            log!("[update] couldn't look for an update: {error}");
            return Err(());
        }
    };
    let offer = found.as_ref().map(Offer::from);
    // Kept before the companion's told, so an Update straight away finds it.
    *app.state::<Updates>().0.lock().unwrap_or_else(|e| e.into_inner()) = found;
    if let Some(offer) = &offer {
        log!("[update] WestPlay {} is available", offer.version);
        let _ = app.emit("update-found", offer);
    }
    Ok(offer)
}

/// The update found so far, if there's one (the companion asks when it opens: `update-found` may have come
/// before it was listening).
#[tauri::command]
pub fn update_offer(updates: State<Updates>) -> Option<Offer> {
    updates.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(Offer::from)
}

/// Looks for an update now (the companion, as it opens, and its About page). A development build has none.
#[tauri::command]
pub async fn look_for_update(app: AppHandle) -> Option<Offer> {
    if cfg!(debug_assertions) {
        return None;
    }
    look(&app).await.ok().flatten()
}

/// Installs the update found: downloaded, its signature checked, then Windows' installer runs, and WestPlay
/// quits for it (the installer starts it again). Only with the game closed: the plugin the game loaded is
/// the old WestPlay's. Comes back only if it couldn't be installed.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    if westplay_setup::game::running() {
        return Err("gameRunning".into());
    }
    let update = app.state::<Updates>().0.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let update = update.ok_or("noUpdate")?;
    log!("[update] installing WestPlay {}", update.version);
    let mut done = 0u64;
    let mut shown = None;
    let bytes = update
        .download(
            |chunk, total| {
                done += chunk as u64;
                // Told in whole percents, not for every chunk.
                let percent = total.map(|total| done * 100 / total.max(1));
                if percent != shown || total.is_none() {
                    shown = percent;
                    let _ = app.emit("update-progress", Progress { done, total });
                }
            },
            || {},
        )
        .await
        .map_err(|error| {
            log!("[update] couldn't download it: {error}");
            error.to_string()
        })?;
    update.install(bytes).map_err(|error| {
        log!("[update] couldn't install it: {error}");
        error.to_string()
    })
}
