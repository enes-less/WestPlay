//! WestPlay's probe (`westplay_shared::probe`): makes a throwaway D3D11 device and swap chain on a hidden window
//! of its own process, and prints where the functions the plugin hooks are, each in Windows' own `dxgi.dll` or
//! `d3d11.dll`, one line each. Run by the plugin as the game starts, so that nothing is made inside the game for it.
//!
//! Exit codes: 0 found; 2 a device couldn't be made; 3 a function found outside Windows' own DLLs (something
//! hooks this process's DirectX too: no place to give the plugin).

#![windows_subsystem = "windows"]

use std::io::Write;

use westplay_shared::probe::{
    self, Located, VTBL_CONTEXT_OM_SET_RENDER_TARGETS, VTBL_CONTEXT_OM_SET_RENDER_TARGETS_AND_UAVS,
    VTBL_CONTEXT_PS_SET_SHADER_RESOURCES, VTBL_DEVICE_CREATE_SHADER_RESOURCE_VIEW, VTBL_DEVICE_CREATE_TEXTURE2D,
    VTBL_SWAPCHAIN_PRESENT, VTBL_SWAPCHAIN1_PRESENT1,
};
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION, D3D11CreateDeviceAndSwapChain, ID3D11Device, ID3D11DeviceContext,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGISwapChain,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW};
use windows::core::{BOOL, Interface, w};

fn main() {
    let found = match find() {
        Ok(found) => found,
        Err((code, why)) => {
            eprintln!("westplay-probe: {why}");
            std::process::exit(code);
        }
    };
    let mut out = std::io::stdout().lock();
    for (name, located) in &found {
        let _ = writeln!(out, "{}", probe::line(name, located));
    }
    let _ = out.flush();
}

/// The functions, by name ([`probe::NAMES`]), each located in Windows' own DLL; else an exit code and why.
fn find() -> Result<Vec<(&'static str, Located)>, (i32, String)> {
    let addresses = addresses().map_err(|why| (2, why))?;
    let system = system_directory();
    let mut found = Vec::new();
    for (name, address) in addresses {
        let located = probe::locate(address).ok_or_else(|| (3, format!("{name} at {address:#x} in no module")))?;
        let own = located.module.to_lowercase();
        let in_system = system.as_ref().is_some_and(|system| own.starts_with(&format!("{system}\\")));
        if !in_system || !(own.ends_with("\\dxgi.dll") || own.ends_with("\\d3d11.dll")) {
            return Err((3, format!("{name} in {}, not Windows' own dxgi.dll or d3d11.dll", located.module)));
        }
        found.push((name, located));
    }
    Ok(found)
}

/// Windows' system folder (`C:\Windows\System32`), in lower case.
fn system_directory() -> Option<String> {
    let mut path = [0u16; 260];
    // SAFETY: a local buffer, its length given.
    let length = unsafe { GetSystemDirectoryW(Some(&mut path)) } as usize;
    (length > 0 && length < path.len()).then(|| String::from_utf16_lossy(&path[..length]).to_lowercase())
}

/// The functions' addresses, read from a throwaway device's, context's and swap chain's vtables while they live.
fn addresses() -> Result<Vec<(&'static str, usize)>, String> {
    // SAFETY: a hidden window of the system's own `STATIC` class, made and destroyed here, its strings static;
    // the throwaway objects live through the reads (each slot within its interface's fixed layout).
    unsafe {
        let hinstance = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("westplay-probe"),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            64,
            64,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| format!("CreateWindowExW: {e}"))?;
        let read = read_vtables(hwnd);
        let _ = DestroyWindow(hwnd);
        read
    }
}

/// # Safety
/// `hwnd` is a live window of this thread's.
unsafe fn read_vtables(hwnd: HWND) -> Result<Vec<(&'static str, usize)>, String> {
    // SAFETY: a throwaway device and swap chain on `hwnd` (alive through the call), their descriptions locals; each
    // function read from its live object's vtable within the interface's fixed layout.
    unsafe {
        let desc = DXGI_SWAP_CHAIN_DESC {
            BufferDesc: DXGI_MODE_DESC {
                Width: 64,
                Height: 64,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                ..Default::default()
            },
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 1,
            OutputWindow: hwnd,
            Windowed: BOOL::from(true),
            SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
            Flags: 0,
        };
        let mut swapchain: Option<IDXGISwapChain> = None;
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        D3D11CreateDeviceAndSwapChain(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&desc),
            Some(&mut swapchain),
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .map_err(|e| format!("D3D11CreateDeviceAndSwapChain: {e}"))?;
        let device = device.ok_or("no device")?;
        let context = context.ok_or("no context")?;
        let swapchain = swapchain.ok_or("no swap chain")?;
        let entry = |object: *mut core::ffi::c_void, slot: usize| *(*(object as *const *const usize)).add(slot);
        Ok(vec![
            ("create_texture2d", entry(device.as_raw(), VTBL_DEVICE_CREATE_TEXTURE2D)),
            ("create_shader_resource_view", entry(device.as_raw(), VTBL_DEVICE_CREATE_SHADER_RESOURCE_VIEW)),
            ("ps_set_shader_resources", entry(context.as_raw(), VTBL_CONTEXT_PS_SET_SHADER_RESOURCES)),
            ("om_set_render_targets", entry(context.as_raw(), VTBL_CONTEXT_OM_SET_RENDER_TARGETS)),
            ("om_set_render_targets_and_uavs", entry(context.as_raw(), VTBL_CONTEXT_OM_SET_RENDER_TARGETS_AND_UAVS)),
            ("present", entry(swapchain.as_raw(), VTBL_SWAPCHAIN_PRESENT)),
            ("present1", entry(swapchain.as_raw(), VTBL_SWAPCHAIN1_PRESENT1)),
            ("swapchain_vtable", *(swapchain.as_raw() as *const usize)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every function found, in Windows' own dxgi.dll or d3d11.dll, and printed lines the plugin reads back to the
    /// same places, resolving in this process to where they are.
    #[test]
    fn finds_every_function_in_windows_own_dlls() {
        let found = find().unwrap();
        assert_eq!(found.iter().map(|(name, _)| *name).collect::<Vec<_>>(), probe::NAMES);
        let output: String = found.iter().map(|(name, located)| probe::line(name, located) + "\n").collect();
        let read = probe::parse(&output).unwrap();
        for (name, located) in &found {
            assert_eq!(&read[*name], located);
            let address = probe::resolve(located).unwrap();
            assert_eq!(probe::locate(address).as_ref(), Some(located), "{name}");
        }
        assert!(read["present"].module.to_lowercase().ends_with("\\dxgi.dll"));
        assert!(read["create_texture2d"].module.to_lowercase().ends_with("\\d3d11.dll"));
    }
}
