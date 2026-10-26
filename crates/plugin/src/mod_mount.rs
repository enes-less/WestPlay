//! WestPlay's screen mod, mounted by the plugin itself (PLAN.md: the plugin mounts `westplay.scs` in every
//! mode, never shown in the Mod Manager), through the game's own mount: the function that mounts one device
//! from a request, as the game mounts its user folder with it. The archive goes into the mod pool, first. The
//! game's functions are found in its code by what they are (`locate`), not where one version had them, so other
//! builds work too (1.60.1.7, the VR branch's 1.61.1.1009s); a game whose code doesn't match is left alone.
//!
//! Mounted once a game process (an `sdk reload` doesn't mount it again), from `Documents\WestPlay\westplay.scs`
//! (written there by WestPlay's setup, from the player's own game), when the plugin loads, before the game
//! mounts the profile's mods; it stays through them and through a profile switch (tested 2026-10-20). Not while
//! WestPlay 1.2's copy is still in the game's `mod` folder (the app takes it out when it installs): with both,
//! the game has the screens' scripts twice and stops when a screen starts.
//!
//! The game finds a pool's files by the pool's index, which its mount doesn't update: the game rebuilds it after
//! its own mounts (the function that logs `[ufs] mod pool: …`). In single player the profile's mods, mounted
//! after the plugin, rebuild it with ours in; under TruckersMP the mod pool is indexed before the plugin loads
//! and nothing rebuilds it after, so a file only ours has (a screen's material) couldn't be opened: a white
//! screen (2026-10-21). So the mod pool's index is rebuilt right after the mount, as the game does.

use std::ffi::{CString, c_char, c_void};
use std::path::Path;

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};
use windows::core::PCWSTR;

use crate::log;
use crate::navigation::Image;

/// The game's mount, found by its start (`+0xF17E0` in 1.61.1.9, `+0xECB20` in 1.60.1.7, the same in both but
/// for the bytes masked: the addresses of what it calls and of the game's globals, and a jump's length). Kept:
/// its frame, and the request's fields it reads (`+0x8`, `+0x10`, `+0x0`), so a request laid out otherwise
/// doesn't match. Every build known reads the request's fields at the same offsets throughout (1.61.1.9,
/// 1.60.1.7, the VR branch's 1.61.1.1009 at `+0xF1A30`: checked 2026-10-23).
const MOUNT: &str = "48 89 54 24 10 53 48 81 ec 90 01 00 00 4c 8b 0d ?? ?? ?? ?? 48 8b d9 41 80 39 00 74 ?? 48 8b 41 08 \
                     4c 8d 05 ?? ?? ?? ?? 48 8d 4c 24 50 48 89 44 24 20 33 d2 e8 ?? ?? ?? ?? 48 8b 43 10 48 8d 15 \
                     ?? ?? ?? ?? 8b 0b 4c 8d 8c 24 a8 01 00 00 48 85 c0 c7 44 24 40 ff ff ff ff 4c 8d 44 24 58";
/// The game's pool by index (the mount takes the pool with it: `+0x153780` in 1.61.1.9, `+0x14F2A0` in
/// 1.60.1.7), its bounds check and the assert after it (its line number masked).
const POOL: &str = "48 83 ec 48 48 63 d1 48 3b 15 ?? ?? ?? ?? 73 10 48 8b 05 ?? ?? ?? ?? 48 8b 04 d0 48 83 c4 48 c3 \
                    48 8d 0d ?? ?? ?? ?? e8 ?? ?? ?? ?? 48 89 44 24 30 4c 8d 0d ?? ?? ?? ?? 48 89 54 24 28 48 8d 05 \
                    ?? ?? ?? ?? ba ?? ?? 00 00 48 89 44 24 20";
/// The rebuild of a pool's index (`+0x158BC0` in 1.61.1.9, `+0x1546E0` in 1.60.1.7, compiled differently in
/// each): the function holding the one load of the line it logs, which reads the pool's parts at these offsets
/// as it starts (`lea r, [rcx + E8h / 138h / 160h]`, in both builds).
const INDEX_LOG: &[u8] = b"[ufs] %s pool: %Iu packs, %Iu files, %Iu dirs\0";
const INDEX_READS: [[u8; 4]; 3] = [[0xe8, 0x00, 0x00, 0x00], [0x38, 0x01, 0x00, 0x00], [0x60, 0x01, 0x00, 0x00]];

/// A pattern's bytes, `??` any byte.
fn pattern(text: &str) -> Vec<Option<u8>> {
    text.split_whitespace().map(|byte| u8::from_str_radix(byte, 16).ok()).collect()
}

/// The lines a game with a pool index logs about it. A game logging neither (1.58.1.4) has none: it looks a
/// pool's files up device by device (its mount logs `[fs] device %s mounted.`, no pool), so there's no index to
/// rebuild after the mount.
const INDEX_SIGNS: [&[u8]; 2] = [INDEX_LOG, b"[ufs] disabling pool lookup"];

/// What the mount needs of the game, as found: its mount and, in a game with a pool index (1.60, 1.61), its pool
/// by index and its rebuild of a pool's index.
struct Found {
    mount: usize,
    index: Option<(usize, usize)>,
}

/// What the mount needs in `image`: `None` unless each is found exactly once (another game whose code differs is
/// left alone, as is one with a pool index whose rebuild isn't found).
fn locate(image: &Image) -> Option<Found> {
    let mount = image.unique_masked(&pattern(MOUNT))?;
    if INDEX_SIGNS.iter().all(|sign| image.constant(sign).is_none()) {
        return Some(Found { mount, index: None });
    }
    let pool = image.unique_masked(&pattern(POOL))?;
    let index = image.function_start(image.unique_load(image.constant(INDEX_LOG)?)?)?;
    let start = image.code_at(index, 0x60)?;
    INDEX_READS
        .iter()
        .all(|read| start.windows(4).any(|w| w == read))
        .then_some(Found { mount, index: Some((pool, index)) })
}

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
    let Some(path) = path.clone().filter(|path| path.is_file()) else {
        // The folder looked in whole, and what's in it: a player's "it's there" is then checked by this log.
        let looked = path.as_deref().map_or("no Documents folder".into(), |path| path.display().to_string());
        let beside: Vec<String> = path
            .as_deref()
            .and_then(Path::parent)
            .and_then(|dir| std::fs::read_dir(dir).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        log!("mod mount: no {looked}: nothing mounted (beside it: {beside:?})");
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
        let Some(found) = Image::of(base).as_ref().and_then(locate) else {
            log!("mod mount: the game's mount isn't as read (another game version?): nothing mounted");
            return;
        };
        match found.index {
            Some((pool_at, index_at)) => log!(
                "mod mount: the game's mount at +{:#x}, its pools at +{:#x}, their index at +{:#x}",
                found.mount - base,
                pool_at - base,
                index_at - base
            ),
            None => log!("mod mount: the game's mount at +{:#x} (its pools have no index)", found.mount - base),
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
        let mount: MountFn = std::mem::transmute(found.mount);
        let device = mount(&request);
        log!(
            "mod mount: {} into the mod pool at priority {PRIORITY}: {}",
            text.to_string_lossy(),
            if device.is_null() { "the game refused it".to_string() } else { format!("device {:#x}", device as usize) }
        );
        if device.is_null() {
            return;
        }
        // The pool the mount just took (so it's there), its index rebuilt as the game does after its own mounts;
        // a game with no index has nothing to rebuild.
        let Some((pool_at, index_at)) = found.index else { return };
        let pool: PoolFn = std::mem::transmute(pool_at);
        let index: IndexFn = std::mem::transmute(index_at);
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

    /// A game executable laid out as Windows maps it: its headers, then each section at its address.
    fn mapped(file: &[u8]) -> Vec<u8> {
        let u16_at = |at: usize| u16::from_le_bytes([file[at], file[at + 1]]) as usize;
        let u32_at = |at: usize| u32::from_le_bytes(file[at..at + 4].try_into().unwrap()) as usize;
        let header = u32_at(0x3c);
        let (count, optional) = (u16_at(header + 6), u16_at(header + 20));
        let mut image = vec![0u8; u32_at(header + 24 + 56)];
        let headers = u32_at(header + 24 + 60);
        image[..headers].copy_from_slice(&file[..headers]);
        for i in 0..count {
            let at = header + 24 + optional + i * 40;
            let (address, raw_size, raw) = (u32_at(at + 12), u32_at(at + 16), u32_at(at + 20));
            let size = raw_size.min(image.len() - address).min(file.len() - raw);
            image[address..address + size].copy_from_slice(&file[raw..raw + size]);
        }
        image
    }

    /// The mount, the pools and their index found in each game build named by `WESTPLAY_GAME_EXES` (paths to
    /// `eurotrucks2.exe`, separated by `;`), at the places they were read in the builds known: 1.61.1.9, 1.60.1.7,
    /// the VR branch's 1.61.1.1009 (the same functions, at other addresses) and 1.58.1.4 (its mount alone: its
    /// pools have no index).
    #[test]
    #[ignore = "needs game executables: WESTPLAY_GAME_EXES"]
    fn finds_the_mount_in_each_game_build() {
        const KNOWN: [(usize, Option<(usize, usize)>); 4] = [
            (0xF17E0, Some((0x153780, 0x158BC0))),
            (0xECB20, Some((0x14F2A0, 0x1546E0))),
            (0xF1A30, Some((0x1539D0, 0x158E10))),
            (0x21BB30, None),
        ];
        let exes = std::env::var("WESTPLAY_GAME_EXES").expect("WESTPLAY_GAME_EXES");
        for exe in exes.split(';').filter(|exe| !exe.is_empty()) {
            let image = mapped(&std::fs::read(exe).unwrap());
            let base = image.as_ptr() as usize;
            // SAFETY: `image` is laid out as a mapped PE image, alive through the search.
            let found = unsafe { Image::of(base) }
                .as_ref()
                .and_then(locate)
                .map(|found| (found.mount - base, found.index.map(|(pool, index)| (pool - base, index - base))));
            println!("{exe}: {found:x?}");
            let found = found.expect("the mount isn't found");
            if let Some(known) = KNOWN.iter().find(|known| known.0 == found.0) {
                assert_eq!(found, *known);
            }
        }
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
