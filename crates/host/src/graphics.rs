//! Which GPU the cab is drawn on. WebView2 draws on the GPU Windows lists first, which on most laptops is
//! the integrated one, while the game runs on the other: every frame was then drawn on the weak GPU and
//! copied across to the game's. So WestPlay picks the GPU and tells WebView2 (Chromium's
//! `--use-adapter-luid`, its LUID's two halves in decimal: measured with Edge, the same engine, moving its
//! drawing from one GPU to another): the game's once it runs, before that Windows' high-performance GPU; or
//! the one the player chose in the companion, if it's there. A different GPU than the one WebView2 was
//! started on takes starting WebView2 again (lib.rs ends the host's run with [`GpuChanged`]).

use windows::Win32::Foundation::LUID;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, IDXGIAdapter1, IDXGIFactory1,
    IDXGIFactory6,
};
use windows::core::Interface;

/// A GPU: its id (what the player's choice is kept as: its PCI ids, the same from boot to boot, where a
/// LUID isn't), its name, and its LUID this boot.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Adapter {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub luid: LUID,
}

/// The PC's GPUs (not Windows' software one), the high-performance ones first, as Windows ranks them.
pub fn adapters() -> Vec<Adapter> {
    // SAFETY: plain DXGI calls with locals; the factory and adapters are released as they go.
    let listed: Vec<IDXGIAdapter1> = unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return Vec::new() };
        match factory.cast::<IDXGIFactory6>() {
            Ok(ranked) => (0..)
                .map_while(|i| ranked.EnumAdapterByGpuPreference(i, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE).ok())
                .collect(),
            Err(_) => (0..).map_while(|i| factory.EnumAdapters1(i).ok()).collect(),
        }
    };
    let mut adapters: Vec<Adapter> = Vec::new();
    for adapter in listed {
        // SAFETY: the adapter is alive; the description is a local.
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else { continue };
        if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        let pci = format!("{:04x}:{:04x}:{:08x}:{}", desc.VendorId, desc.DeviceId, desc.SubSysId, desc.Revision);
        // Two of the same card: the second is told apart by its place.
        let twins = adapters.iter().filter(|a| a.id.split('#').next() == Some(pci.as_str())).count();
        let id = if twins == 0 { pci } else { format!("{pci}#{}", twins + 1) };
        let name = String::from_utf16_lossy(&desc.Description).trim_end_matches('\0').trim().to_string();
        adapters.push(Adapter { id, name, luid: desc.AdapterLuid });
    }
    adapters
}

pub fn same(a: LUID, b: LUID) -> bool {
    (a.HighPart, a.LowPart) == (b.HighPart, b.LowPart)
}

/// The GPU to draw the cab on: the player's `chosen` one if it's among `adapters`; otherwise the `game`'s
/// once it's known; otherwise the one it's drawn on now (`current`), so the game closing changes nothing;
/// otherwise the first (Windows' high-performance GPU). `None` without any GPU listed.
pub fn choose(chosen: Option<&str>, game: Option<LUID>, current: Option<LUID>, adapters: &[Adapter]) -> Option<LUID> {
    let listed = |luid: LUID| adapters.iter().any(|a| same(a.luid, luid)).then_some(luid);
    chosen
        .and_then(|id| adapters.iter().find(|a| a.id == id))
        .map(|a| a.luid)
        .or_else(|| game.and_then(listed))
        .or_else(|| current.and_then(listed))
        .or_else(|| adapters.first().map(|a| a.luid))
}

/// The browser argument that has WebView2 draw on `luid`.
pub fn browser_argument(luid: LUID) -> String {
    format!("--use-adapter-luid={},{}", luid.HighPart, luid.LowPart)
}

/// The GPU's name, among `adapters`.
pub fn name_of(luid: LUID, adapters: &[Adapter]) -> Option<String> {
    adapters.iter().find(|a| same(a.luid, luid)).map(|a| a.name.clone())
}

/// The host's run ended to draw the cab on another GPU: start it again (once WebView2's browsers have gone,
/// `webview::wait_for_browsers`).
#[derive(Debug)]
pub struct GpuChanged;

impl std::fmt::Display for GpuChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the cab moves to another GPU")
    }
}

impl std::error::Error for GpuChanged {}

#[cfg(test)]
mod tests {
    use super::*;

    fn luid(low: u32) -> LUID {
        LUID { LowPart: low, HighPart: 0 }
    }

    fn gpus() -> Vec<Adapter> {
        vec![
            Adapter {
                id: "10de:2702:00000000:161".into(),
                name: "NVIDIA GeForce RTX 4080 SUPER".into(),
                luid: luid(0x10e61),
            },
            Adapter {
                id: "1002:164e:00000000:197".into(),
                name: "AMD Radeon(TM) Graphics".into(),
                luid: luid(0x11deb),
            },
        ]
    }

    #[test]
    fn picks_the_players_choice_then_the_games_then_keeps_its_own() {
        let gpus = gpus();
        let (nvidia, radeon) = (Some(luid(0x10e61)), Some(luid(0x11deb)));
        let pick = |chosen, game, current| choose(chosen, game, current, &gpus).map(|l| l.LowPart);
        // Before the game: Windows' high-performance GPU.
        assert_eq!(pick(None, None, None), Some(0x10e61));
        // The game on the Radeon: there too.
        assert_eq!(pick(None, radeon, nvidia), Some(0x11deb));
        // The game gone: where it is.
        assert_eq!(pick(None, None, radeon), Some(0x11deb));
        // The player's choice wins, even over the game's.
        assert_eq!(pick(Some("1002:164e:00000000:197"), nvidia, nvidia), Some(0x11deb));
        // A chosen GPU that's gone (unplugged), or a LUID not listed (a stale one): WestPlay picks.
        assert_eq!(pick(Some("10de:0000:00000000:1"), radeon, None), Some(0x11deb));
        assert_eq!(pick(None, Some(luid(0x99)), Some(luid(0x98))), Some(0x10e61));
        assert_eq!(choose(None, None, None, &[]), None);
    }

    #[test]
    fn tells_webview2_the_gpu_in_decimal_halves() {
        // As measured with Edge: `0,73195` drew on the Radeon (LUID 0x0:0x11deb).
        assert_eq!(browser_argument(luid(0x11deb)), "--use-adapter-luid=0,73195");
    }

    #[test]
    fn lists_this_pcs_gpus_with_ids_that_tell_them_apart() {
        let adapters = adapters();
        let mut ids: Vec<&str> = adapters.iter().map(|a| a.id.as_str()).collect();
        ids.dedup();
        assert_eq!(ids.len(), adapters.len());
        assert!(adapters.iter().all(|a| !a.name.is_empty() && !a.name.contains("Basic Render")));
    }
}
