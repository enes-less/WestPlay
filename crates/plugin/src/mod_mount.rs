//! WestPlay's screen mod, mounted by the plugin itself (PLAN.md: the plugin mounts `westplay.scs` in every
//! mode, never shown in the Mod Manager), through the game's own mount: the function that mounts one device
//! from a request (`eurotrucks2.exe+0xF17E0` in 1.61.1.9, its first bytes checked: another game version is
//! left alone), as the game mounts its user folder with it. The archive goes into the mod pool, first.
//!
//! Mounted once a game process (an `sdk reload` doesn't mount it again), from `Documents\WestPlay\westplay.scs`
//! (written there by WestPlay's setup, from the player's own game), when the plugin loads, before the game
//! mounts the profile's mods; it stays through them and through a profile switch (tested 2026-10-20). Not while
//! WestPlay 1.2's copy is still in the game's `mod` folder (the app takes it out when it installs): with both,
//! the game has the screens' scripts twice and stops when a screen starts.
//!
//! The game finds a pool's files by the pool's index, which its mount doesn't update: the game rebuilds it after
//! its own mounts (`+0x158BC0`, which logs `[ufs] mod pool: …`). In single player the profile's mods, mounted
//! after the plugin, rebuild it with ours in; under TruckersMP the mod pool is indexed before the plugin loads
//! and nothing rebuilds it after, so a file only ours has (a screen's material) couldn't be opened: a white
//! screen (2026-10-21). So the mod pool's index is rebuilt right after the mount, as the game does.

use std::ffi::{CString, c_char, c_void};

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};
use windows::core::PCWSTR;

use crate::log;

/// The game's mount, and its first bytes (1.61.1.9).
const MOUNT_RVA: usize = 0xF17E0;
const MOUNT_BYTES: [u8; 20] = [
    0x48, 0x89, 0x54, 0x24, 0x10, 0x53, 0x48, 0x81, 0xEC, 0x90, 0x01, 0x00, 0x00, 0x4C, 0x8B, 0x0D, 0xA4, 0x19, 0x64,
    0x02,
];
/// The game's pool by index (`+0x153780`, which the mount takes the pool with), and the rebuild of a pool's
/// index (`+0x158BC0`), with their first bytes (1.61.1.9).
const POOL_RVA: usize = 0x153780;
const POOL_BYTES: [u8; 16] =
    [0x48, 0x83, 0xEC, 0x48, 0x48, 0x63, 0xD1, 0x48, 0x3B, 0x15, 0xF2, 0x04, 0x5E, 0x02, 0x73, 0x10];
const INDEX_RVA: usize = 0x158BC0;
const INDEX_BYTES: [u8; 20] = [
    0x48, 0x8B, 0xC4, 0x48, 0x89, 0x48, 0x08, 0x57, 0x48, 0x81, 0xEC, 0x90, 0x00, 0x00, 0x00, 0x48, 0x89, 0x58, 0xF0,
    0x48,
];
/// A device kind the game opens as an archive (HashFS, else zip), the mod pool, and the highest priority a mod
/// can have there (the game keeps mods' within -100000..=99999).
const KIND_ARCHIVE: u32 = 3;
const POOL_MOD: u32 = 2;
const PRIORITY: i32 = 99_999;

/// The game's mount request, as its mount reads it.
#[repr(C)]
struct Request {
    kind: u32,
    pool: u32,
    path: *const c_char,
    /// The device's name (none: the game's empty one).
    name: *const c_char,
    flag_a: u8,
    priority: i32,
    flag_b: u8,
    /// Must be under 3 for an archive (the game refuses it otherwise).
    extra: u32,
}

type MountFn = unsafe extern "system" fn(*const Request) -> *mut c_void;
type PoolFn = unsafe extern "system" fn(i32) -> *mut c_void;
type IndexFn = unsafe extern "system" fn(*mut c_void);

/// The game's folder in Documents, by the game's id (as the SDK gives it).
fn game_folder(game_id: &str) -> Option<&'static str> {
    match game_id {
        "eut2" => Some("Euro Truck Simulator 2"),
        "ats" => Some("American Truck Simulator"),
        _ => None,
    }
}

/// Mounts WestPlay's mod into the game `game_id` (the SDK's), once a game process.
///
/// # Safety
/// On the game's main thread, from the plugin's init.
pub unsafe fn mount(game_id: &str) {
    let path = crate::log::log_dir().parent().map(|dir| dir.join("westplay.scs"));
    let Some(path) = path.filter(|path| path.is_file()) else {
        log!("mod mount: no Documents\\WestPlay\\westplay.scs: nothing mounted");
        return;
    };
    let old =
        game_folder(game_id).map(|folder| crate::log::documents_dir().join(folder).join("mod").join("westplay.scs"));
    if let Some(old) = old.filter(|old| old.is_file()) {
        log!("mod mount: WestPlay 1.2's mod is still in {} (two would stop the game): nothing mounted", old.display());
        return;
    }
    // SAFETY: the game's own module, mapped for the process's life; its code read where the mount was found.
    // A mutex named for this process tells whether an earlier load of the plugin mounted already (it's left
    // open, so it lasts as long as the process).
    unsafe {
        let name: Vec<u16> =
            format!("Local\\WestPlay.ModMounted.{}", GetCurrentProcessId()).encode_utf16().chain([0]).collect();
        if CreateMutexW(None, false, PCWSTR(name.as_ptr())).is_err() || GetLastError() == ERROR_ALREADY_EXISTS {
            log!("mod mount: already mounted in this game process");
            return;
        }
        let Ok(module) = GetModuleHandleW(None) else { return };
        let base = module.0 as usize;
        let code = |rva: usize, len: usize| std::slice::from_raw_parts((base + rva) as *const u8, len);
        if code(MOUNT_RVA, MOUNT_BYTES.len()) != MOUNT_BYTES
            || code(POOL_RVA, POOL_BYTES.len()) != POOL_BYTES
            || code(INDEX_RVA, INDEX_BYTES.len()) != INDEX_BYTES
        {
            log!("mod mount: the game's mount isn't as read (another game version?): nothing mounted");
            return;
        }
        // The game writes paths with forward slashes; the path is kept for the process's life (the device may
        // keep it).
        let text = path.to_string_lossy().replace('\\', "/");
        let Ok(text) = CString::new(text) else { return };
        let text: &'static CString = Box::leak(Box::new(text));
        let request = Request {
            kind: KIND_ARCHIVE,
            pool: POOL_MOD,
            path: text.as_ptr(),
            name: std::ptr::null(),
            flag_a: 0,
            priority: PRIORITY,
            flag_b: 0,
            extra: 0,
        };
        let mount: MountFn = std::mem::transmute(base + MOUNT_RVA);
        let device = mount(&request);
        log!(
            "mod mount: {} into the mod pool at priority {PRIORITY}: {}",
            text.to_string_lossy(),
            if device.is_null() { "the game refused it".to_string() } else { format!("device {:#x}", device as usize) }
        );
        if device.is_null() {
            return;
        }
        // The pool the mount just took (so it's there), its index rebuilt as the game does after its own mounts.
        let pool: PoolFn = std::mem::transmute(base + POOL_RVA);
        let index: IndexFn = std::mem::transmute(base + INDEX_RVA);
        let pool = pool(POOL_MOD as i32);
        if pool.is_null() {
            log!("mod mount: no mod pool to index");
            return;
        }
        index(pool);
        log!("mod mount: the mod pool's index rebuilt");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_games_folder_is_known_by_its_id() {
        assert_eq!(game_folder("eut2"), Some("Euro Truck Simulator 2"));
        assert_eq!(game_folder("ats"), Some("American Truck Simulator"));
        assert_eq!(game_folder("other"), None);
    }

    #[test]
    fn the_request_is_laid_out_as_the_games_mount_reads_it() {
        assert_eq!(std::mem::offset_of!(Request, kind), 0x0);
        assert_eq!(std::mem::offset_of!(Request, pool), 0x4);
        assert_eq!(std::mem::offset_of!(Request, path), 0x8);
        assert_eq!(std::mem::offset_of!(Request, name), 0x10);
        assert_eq!(std::mem::offset_of!(Request, flag_a), 0x18);
        assert_eq!(std::mem::offset_of!(Request, priority), 0x1C);
        assert_eq!(std::mem::offset_of!(Request, flag_b), 0x20);
        assert_eq!(std::mem::offset_of!(Request, extra), 0x24);
    }
}
