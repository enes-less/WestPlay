//! WestPlay's probe (`westplay-probe.exe`, beside the plugin in the game's plugins folder): where the DirectX
//! functions the plugin hooks are, found in a process of its own.
//!
//! The plugin used to make a throwaway D3D11 device and swap chain inside the game to read them from. Tools that
//! watch the game's swap chains being made (OptiScaler wrapping each, the Steam overlay hooking its Present) took
//! ours in too, and once it was let go the game crashed on its first frame (a player's, 2026-11-07: test.7, the
//! throwaway made and let go and nothing hooked, crashed; test.6, without it, played). The probe makes it in its own
//! process instead, where none of them are, and says each function as its DLL's file and the offset in it: Windows'
//! own `dxgi.dll` and `d3d11.dll`, the same files the game has loaded (checked by their image's stamp and size).
//!
//! It prints one line per function: its name, then the DLL's path, the image's stamp, its size and the offset, tab
//! between ([`line`], [`parse`]).

use std::collections::HashMap;

use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW,
    GetModuleHandleExW, GetModuleHandleW,
};
use windows::core::{HSTRING, PCWSTR};

/// The probe's file, beside the plugin's.
pub const FILE_NAME: &str = "westplay-probe.exe";

/// The functions' slots in their interfaces' vtables (the interfaces' fixed layout): ID3D11Device's, then
/// ID3D11DeviceContext's, then IDXGISwapChain's and IDXGISwapChain1's.
pub const VTBL_DEVICE_CREATE_TEXTURE2D: usize = 5;
pub const VTBL_DEVICE_CREATE_SHADER_RESOURCE_VIEW: usize = 7;
pub const VTBL_CONTEXT_PS_SET_SHADER_RESOURCES: usize = 8;
pub const VTBL_CONTEXT_OM_SET_RENDER_TARGETS: usize = 33;
pub const VTBL_CONTEXT_OM_SET_RENDER_TARGETS_AND_UAVS: usize = 34;
pub const VTBL_SWAPCHAIN_PRESENT: usize = 8;
pub const VTBL_SWAPCHAIN1_PRESENT1: usize = 22;

/// What the probe finds, by name: the D3D11 device's and context's functions the plugin hooks, the swap chain's
/// Present and Present1, and the swap chain's vtable.
pub const NAMES: [&str; 8] = [
    "create_texture2d",
    "create_shader_resource_view",
    "ps_set_shader_resources",
    "om_set_render_targets",
    "om_set_render_targets_and_uavs",
    "present",
    "present1",
    "swapchain_vtable",
];

/// Where something is: the DLL it's in (its path), that image's stamp and size (its PE header's `TimeDateStamp` and
/// `SizeOfImage`: the same file loaded in another process has them the same), and its offset in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    pub module: String,
    pub stamp: u32,
    pub size: u32,
    pub offset: usize,
}

/// `address`, located in the DLL it's in (`None`: in none).
pub fn locate(address: usize) -> Option<Located> {
    let mut module = HMODULE::default();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    // SAFETY: `address` is only looked up; the module found is loaded (its count left as it was), its header read
    // in place and its path into a local.
    unsafe {
        GetModuleHandleExW(flags, PCWSTR(address as *const u16), &mut module).ok()?;
        let mut path = [0u16; 520];
        let length = GetModuleFileNameW(Some(module), &mut path) as usize;
        let (stamp, size) = identity(module.0 as usize)?;
        Some(Located {
            module: String::from_utf16_lossy(&path[..length]),
            stamp,
            size,
            offset: address.wrapping_sub(module.0 as usize),
        })
    }
}

/// Where `located` is in this process: the same DLL loaded here (by its path, its stamp and size the same), plus
/// the offset; why not, if not.
pub fn resolve(located: &Located) -> Result<usize, String> {
    // SAFETY: a loaded module looked up by its path (no count taken: it's the game's, loaded for good), its header
    // read in place.
    unsafe {
        let module = GetModuleHandleW(PCWSTR(HSTRING::from(&located.module).as_ptr()))
            .map_err(|_| format!("{} isn't loaded here", located.module))?;
        let (stamp, size) = identity(module.0 as usize).ok_or_else(|| format!("{}: no PE header", located.module))?;
        if (stamp, size) != (located.stamp, located.size) {
            return Err(format!(
                "{} here isn't the probe's (stamp {stamp:#x} size {size:#x}, the probe's {:#x} {:#x})",
                located.module, located.stamp, located.size
            ));
        }
        if located.offset >= size as usize {
            return Err(format!("{} offset {:#x} past its image", located.module, located.offset));
        }
        Ok(module.0 as usize + located.offset)
    }
}

/// A loaded image's PE header: its `TimeDateStamp` and `SizeOfImage`.
///
/// # Safety
/// `base` is a loaded module's base (its headers mapped).
unsafe fn identity(base: usize) -> Option<(u32, u32)> {
    // SAFETY: a loaded image's DOS header, then its NT headers where `e_lfanew` says (the caller's contract).
    unsafe {
        if std::ptr::read_unaligned(base as *const u16) != 0x5A4D {
            return None;
        }
        let nt = base + std::ptr::read_unaligned((base + 0x3C) as *const u32) as usize;
        if std::ptr::read_unaligned(nt as *const u32) != 0x0000_4550 {
            return None;
        }
        // FileHeader.TimeDateStamp at +8; OptionalHeader (at +24) SizeOfImage at +56.
        let stamp = std::ptr::read_unaligned((nt + 8) as *const u32);
        let size = std::ptr::read_unaligned((nt + 24 + 56) as *const u32);
        Some((stamp, size))
    }
}

/// One line of the probe's output.
pub fn line(name: &str, located: &Located) -> String {
    format!("{name}\t{}\t{:x}\t{:x}\t{:x}", located.module, located.stamp, located.size, located.offset)
}

/// The probe's output, every one of [`NAMES`] in it (else why not).
pub fn parse(output: &str) -> Result<HashMap<String, Located>, String> {
    let mut found = HashMap::new();
    for text in output.lines().filter(|text| !text.trim().is_empty()) {
        let fields: Vec<&str> = text.trim_end_matches('\r').split('\t').collect();
        let [name, module, stamp, size, offset] = fields[..] else { return Err(format!("not a line of it: {text}")) };
        let hex = |field: &str| u64::from_str_radix(field, 16).map_err(|_| format!("not a number: {text}"));
        found.insert(
            name.to_owned(),
            Located {
                module: module.to_owned(),
                stamp: hex(stamp)? as u32,
                size: hex(size)? as u32,
                offset: hex(offset)? as usize,
            },
        );
    }
    match NAMES.iter().find(|name| !found.contains_key(**name)) {
        Some(missing) => Err(format!("no {missing} in its output")),
        None => Ok(found),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A function located here, then resolved back: the same address; its line read back: the same place.
    #[test]
    fn locates_and_resolves_a_function_in_its_dll() {
        // SAFETY: kernel32's own export, looked up by name, its address only compared.
        let address = unsafe {
            let kernel32 = GetModuleHandleW(windows::core::w!("kernel32.dll")).unwrap();
            windows::Win32::System::LibraryLoader::GetProcAddress(kernel32, windows::core::s!("GetModuleFileNameW"))
                .unwrap() as usize
        };
        let located = locate(address).unwrap();
        assert!(located.module.to_lowercase().ends_with("kernel32.dll"), "{located:?}");
        assert_eq!(resolve(&located).unwrap(), address);
        // Another build of the file: refused.
        assert!(resolve(&Located { stamp: located.stamp ^ 1, ..located.clone() }).is_err());
        let all: String = NAMES.iter().map(|name| line(name, &located) + "\n").collect();
        let read = parse(&all).unwrap();
        assert_eq!(read["present"], located);
        assert!(parse("present\tx\t1\t2\t3\n").unwrap_err().contains("no create_texture2d"));
        assert!(parse("half a line\n").is_err());
    }
}
