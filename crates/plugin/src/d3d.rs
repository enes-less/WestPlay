//! D3D11/DXGI hooks.
//!
//! Hooks `ID3D11Device::CreateTexture2D` (texture inventory for diagnostics), `CreateShaderResourceView` (the
//! tablet's screen texture as the game makes its view: see `screens`),
//! `ID3D11DeviceContext::OMSetRenderTargets*` and `PSSetShaderResources` (screen takeover, see `screens`)
//! and `IDXGISwapChain::Present`/`Present1` (frame clock, heartbeat, renderer info).
//!
//! Function addresses come from the vtables of a throwaway device + swap chain. MinHook patches the
//! function bodies inside d3d11.dll/dxgi.dll, so the hooks apply to the game's own device too.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

use windows::Win32::Foundation::{HMODULE, HWND, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC,
    D3D11CreateDeviceAndSwapChain, ID3D11Device, ID3D11DeviceContext,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    DXGI_PRESENT_PARAMETERS, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
    IDXGIDevice, IDXGISwapChain,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetForegroundWindow, GetWindowRect, IsIconic, IsWindowVisible, WINDOW_EX_STYLE,
    WS_OVERLAPPEDWINDOW,
};
use windows::core::{BOOL, HRESULT, Interface, w};

use crate::hooks::{guarded, hook};
use crate::{bridge, log, screens};

// vtable slots (IUnknown = 0..2)
const VTBL_DEVICE_CREATE_TEXTURE2D: usize = 5;
const VTBL_DEVICE_CREATE_SHADER_RESOURCE_VIEW: usize = 7;
const VTBL_CONTEXT_PS_SET_SHADER_RESOURCES: usize = 8;
const VTBL_SWAPCHAIN_PRESENT: usize = 8;
const VTBL_SWAPCHAIN1_PRESENT1: usize = 22;
const VTBL_CONTEXT_OM_SET_RENDER_TARGETS: usize = 33;
const VTBL_CONTEXT_OM_SET_RENDER_TARGETS_AND_UAVS: usize = 34;

type CreateTexture2DFn = unsafe extern "system" fn(
    *mut c_void,
    *const D3D11_TEXTURE2D_DESC,
    *const D3D11_SUBRESOURCE_DATA,
    *mut *mut c_void,
) -> HRESULT;
type CreateShaderResourceViewFn =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *const c_void, *mut *mut c_void) -> HRESULT;
type PSSetShaderResourcesFn = unsafe extern "system" fn(*mut c_void, u32, u32, *const *mut c_void);
type PresentFn = unsafe extern "system" fn(*mut c_void, u32, u32) -> HRESULT;
type Present1Fn = unsafe extern "system" fn(*mut c_void, u32, u32, *const DXGI_PRESENT_PARAMETERS) -> HRESULT;
type OMSetRenderTargetsFn = unsafe extern "system" fn(*mut c_void, u32, *const *mut c_void, *mut c_void);
type OMSetRenderTargetsAndUavsFn = unsafe extern "system" fn(
    *mut c_void,
    u32,
    *const *mut c_void,
    *mut c_void,
    u32,
    u32,
    *const *mut c_void,
    *const u32,
);

static ORIG_CREATE_TEXTURE2D: AtomicUsize = AtomicUsize::new(0);
static ORIG_CREATE_SHADER_RESOURCE_VIEW: AtomicUsize = AtomicUsize::new(0);
static ORIG_PS_SET_SHADER_RESOURCES: AtomicUsize = AtomicUsize::new(0);
static ORIG_PRESENT: AtomicUsize = AtomicUsize::new(0);
static ORIG_PRESENT1: AtomicUsize = AtomicUsize::new(0);
static ORIG_OM_SET_RENDER_TARGETS: AtomicUsize = AtomicUsize::new(0);
static ORIG_OM_SET_RENDER_TARGETS_AND_UAVS: AtomicUsize = AtomicUsize::new(0);

static FRAME: AtomicU64 = AtomicU64::new(0);
static FIRST_PRESENT_LOGGED: AtomicBool = AtomicBool::new(false);
/// How often (game frames) the game's window is looked at, and how it was as last logged (0: not yet;
/// [`window_mode`]'s).
const WINDOW_EVERY_FRAMES: u64 = 120;
static WINDOW_MODE: AtomicU8 = AtomicU8::new(0);
/// The game's window (its swap chain's), as last looked at (0: not yet).
static GAME_WINDOW: AtomicIsize = AtomicIsize::new(0);
static TEXTURES_DIRTY: AtomicBool = AtomicBool::new(false);

/// How often (in presented frames) the texture table is rewritten to disk.
const DUMP_EVERY_FRAMES: u64 = 300;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct TexKey {
    width: u32,
    height: u32,
    mips: u32,
    array: u32,
    format: i32,
    samples: u32,
    usage: i32,
    bind: u32,
    cpu: u32,
    misc: u32,
    initial_data: bool,
}

#[derive(Clone, Copy)]
struct TexStat {
    count: u64,
    first_frame: u64,
    last_frame: u64,
}

static TEXTURES: LazyLock<Mutex<HashMap<TexKey, TexStat>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// # Safety
/// In the game's process, before the hooks are switched on ([`crate::hooks::enable_all`]).
pub unsafe fn install() -> Result<(), String> {
    // SAFETY: the vtables are d3d11.dll's and dxgi.dll's own (`dummy_vtables`), each slot within them (the
    // slot numbers are the interfaces' fixed layout); hooked with detours of their exact signatures (below).
    unsafe {
        let DummyVtables { device: device_vtbl, context: context_vtbl, swapchain: swapchain_vtbl } = dummy_vtables()?;

        let create_texture2d = *device_vtbl.add(VTBL_DEVICE_CREATE_TEXTURE2D);
        let present = *swapchain_vtbl.add(VTBL_SWAPCHAIN_PRESENT);
        // IDXGISwapChain1 shares the object's vtable; slot 22 exists on DXGI 1.2+ swap chains.
        let present1 = *swapchain_vtbl.add(VTBL_SWAPCHAIN1_PRESENT1);

        hook(create_texture2d, create_texture2d_hook as *mut c_void, &ORIG_CREATE_TEXTURE2D, "CreateTexture2D")?;
        hook(
            *device_vtbl.add(VTBL_DEVICE_CREATE_SHADER_RESOURCE_VIEW),
            create_shader_resource_view_hook as *mut c_void,
            &ORIG_CREATE_SHADER_RESOURCE_VIEW,
            "CreateShaderResourceView",
        )?;
        hook(
            *context_vtbl.add(VTBL_CONTEXT_PS_SET_SHADER_RESOURCES),
            ps_set_shader_resources_hook as *mut c_void,
            &ORIG_PS_SET_SHADER_RESOURCES,
            "PSSetShaderResources",
        )?;
        hook(present, present_hook as *mut c_void, &ORIG_PRESENT, "Present")?;
        hook(present1, present1_hook as *mut c_void, &ORIG_PRESENT1, "Present1")?;
        hook(
            *context_vtbl.add(VTBL_CONTEXT_OM_SET_RENDER_TARGETS),
            om_set_render_targets_hook as *mut c_void,
            &ORIG_OM_SET_RENDER_TARGETS,
            "OMSetRenderTargets",
        )?;
        hook(
            *context_vtbl.add(VTBL_CONTEXT_OM_SET_RENDER_TARGETS_AND_UAVS),
            om_set_render_targets_and_uavs_hook as *mut c_void,
            &ORIG_OM_SET_RENDER_TARGETS_AND_UAVS,
            "OMSetRenderTargetsAndUnorderedAccessViews",
        )?;
        Ok(())
    }
}

/// Diagnostics on unload (the hooks themselves are removed by `hooks::remove_all`).
pub fn on_unload() {
    dump_textures();
    screens::log_stats();
}

struct DummyVtables {
    device: *const usize,
    context: *const usize,
    swapchain: *const usize,
}

/// Creates a hidden window + D3D11 device + swap chain and returns their vtable pointers.
///
/// # Safety
/// On a thread that may own a window (the game's main thread, at init).
unsafe fn dummy_vtables() -> Result<DummyVtables, String> {
    // SAFETY: a hidden window of the system's own `STATIC` class, made and destroyed here, its strings static.
    unsafe {
        let hinstance = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("westplay-dummy"),
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

        let result = create_dummy_device(hwnd);
        let _ = DestroyWindow(hwnd);
        result
    }
}

/// # Safety
/// `hwnd` is a live window of this thread's.
unsafe fn create_dummy_device(hwnd: HWND) -> Result<DummyVtables, String> {
    // SAFETY: a throwaway device and swap chain on `hwnd` (the caller's, alive through the call), their
    // descriptions locals; their vtables stay valid after they go (they're the DLLs', not the objects').
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

        let device = device.ok_or("no dummy device")?;
        let context = context.ok_or("no dummy context")?;
        let swapchain = swapchain.ok_or("no dummy swap chain")?;
        // vtables live in d3d11.dll/dxgi.dll and stay valid after the dummy objects are released.
        Ok(DummyVtables {
            device: vtable(device.as_raw()),
            context: vtable(context.as_raw()),
            swapchain: vtable(swapchain.as_raw()),
        })
    }
}

/// # Safety
/// `object` is a live COM object.
unsafe fn vtable(object: *mut c_void) -> *const usize {
    // SAFETY: a COM object starts with its vtable's address.
    unsafe { *(object as *const *const usize) }
}

/// # Safety
/// Called in place of `ID3D11Device::CreateTexture2D`, with its arguments.
unsafe extern "system" fn create_texture2d_hook(
    this: *mut c_void,
    desc: *const D3D11_TEXTURE2D_DESC,
    initial_data: *const D3D11_SUBRESOURCE_DATA,
    texture: *mut *mut c_void,
) -> HRESULT {
    // SAFETY: the trampoline was stored before the hooks were switched on (`install`, then `enable_all`), so it's
    // the original function, called with the game's arguments as they came; the description is
    // looked at only when set.
    unsafe {
        let original: CreateTexture2DFn = std::mem::transmute(ORIG_CREATE_TEXTURE2D.load(Ordering::Acquire));
        let result = original(this, desc, initial_data, texture);
        if let Some(desc) = desc.as_ref() {
            guarded(|| record_texture(desc, !initial_data.is_null()));
        }
        result
    }
}

/// # Safety
/// Called in place of `ID3D11Device::CreateShaderResourceView`, with its arguments.
unsafe extern "system" fn create_shader_resource_view_hook(
    this: *mut c_void,
    resource: *mut c_void,
    desc: *const c_void,
    view: *mut *mut c_void,
) -> HRESULT {
    // SAFETY: the trampoline was stored before the hooks were switched on (`install`, then `enable_all`), so it's
    // the original function, called with the game's arguments as they came; the view it made (and the resource
    // the game gave) are looked at only when it made one.
    unsafe {
        let original: CreateShaderResourceViewFn =
            std::mem::transmute(ORIG_CREATE_SHADER_RESOURCE_VIEW.load(Ordering::Acquire));
        let result = original(this, resource, desc, view);
        if result.is_ok() && !resource.is_null() && !view.is_null() && !(*view).is_null() {
            let made = *view;
            guarded(|| screens::on_view_created(resource, made));
        }
        result
    }
}

/// # Safety
/// Called in place of `ID3D11DeviceContext::PSSetShaderResources`, with its arguments.
unsafe extern "system" fn ps_set_shader_resources_hook(
    this: *mut c_void,
    start_slot: u32,
    count: u32,
    views: *const *mut c_void,
) {
    // SAFETY: the game's own arguments, looked at before they're bound (`on_set_shader_resources`); then
    // the trampoline was stored before the hooks were switched on (`install`, then `enable_all`), so it's
    // the original function, called with the game's arguments as they came.
    unsafe {
        guarded(|| screens::on_set_shader_resources(this, count, views));
        let original: PSSetShaderResourcesFn =
            std::mem::transmute(ORIG_PS_SET_SHADER_RESOURCES.load(Ordering::Acquire));
        original(this, start_slot, count, views)
    }
}

/// # Safety
/// Called in place of `ID3D11DeviceContext::OMSetRenderTargets`, with its arguments.
unsafe extern "system" fn om_set_render_targets_hook(
    this: *mut c_void,
    count: u32,
    views: *const *mut c_void,
    depth_stencil: *mut c_void,
) {
    // SAFETY: the game's own arguments, looked at first (`on_set_render_targets`); then
    // the trampoline was stored before the hooks were switched on (`install`, then `enable_all`), so it's
    // the original function, called with the game's arguments as they came.
    unsafe {
        guarded(|| screens::on_set_render_targets(count, views));
        let original: OMSetRenderTargetsFn = std::mem::transmute(ORIG_OM_SET_RENDER_TARGETS.load(Ordering::Acquire));
        original(this, count, views, depth_stencil)
    }
}

/// # Safety
/// Called in place of `OMSetRenderTargetsAndUnorderedAccessViews`, with its arguments.
#[allow(clippy::too_many_arguments)]
unsafe extern "system" fn om_set_render_targets_and_uavs_hook(
    this: *mut c_void,
    rtv_count: u32,
    views: *const *mut c_void,
    depth_stencil: *mut c_void,
    uav_start: u32,
    uav_count: u32,
    uavs: *const *mut c_void,
    uav_initial_counts: *const u32,
) {
    // SAFETY: the game's own arguments, looked at first unless it keeps its targets; then
    // the trampoline was stored before the hooks were switched on (`install`, then `enable_all`), so it's
    // the original function, called with the game's arguments as they came.
    unsafe {
        // D3D11_KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL leaves the current targets bound.
        if rtv_count != u32::MAX {
            guarded(|| screens::on_set_render_targets(rtv_count, views));
        }
        let original: OMSetRenderTargetsAndUavsFn =
            std::mem::transmute(ORIG_OM_SET_RENDER_TARGETS_AND_UAVS.load(Ordering::Acquire));
        original(this, rtv_count, views, depth_stencil, uav_start, uav_count, uavs, uav_initial_counts)
    }
}

/// # Safety
/// Called in place of `IDXGISwapChain::Present`, with its arguments.
unsafe extern "system" fn present_hook(this: *mut c_void, sync_interval: u32, flags: u32) -> HRESULT {
    // SAFETY: the game's swap chain, alive through its call; then
    // the trampoline was stored before the hooks were switched on (`install`, then `enable_all`), so it's
    // the original function, called with the game's arguments as they came.
    unsafe {
        guarded(|| on_present(this));
        let original: PresentFn = std::mem::transmute(ORIG_PRESENT.load(Ordering::Acquire));
        original(this, sync_interval, flags)
    }
}

/// # Safety
/// Called in place of `IDXGISwapChain1::Present1`, with its arguments.
unsafe extern "system" fn present1_hook(
    this: *mut c_void,
    sync_interval: u32,
    flags: u32,
    params: *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT {
    // SAFETY: the game's swap chain, alive through its call; then
    // the trampoline was stored before the hooks were switched on (`install`, then `enable_all`), so it's
    // the original function, called with the game's arguments as they came.
    unsafe {
        guarded(|| on_present(this));
        let original: Present1Fn = std::mem::transmute(ORIG_PRESENT1.load(Ordering::Acquire));
        original(this, sync_interval, flags, params)
    }
}

fn record_texture(desc: &D3D11_TEXTURE2D_DESC, initial_data: bool) {
    let key = TexKey {
        width: desc.Width,
        height: desc.Height,
        mips: desc.MipLevels,
        array: desc.ArraySize,
        format: desc.Format.0,
        samples: desc.SampleDesc.Count,
        usage: desc.Usage.0,
        bind: desc.BindFlags,
        cpu: desc.CPUAccessFlags,
        misc: desc.MiscFlags,
        initial_data,
    };
    let frame = FRAME.load(Ordering::Relaxed);
    let mut textures = TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
    match textures.get_mut(&key) {
        Some(stat) => {
            stat.count += 1;
            stat.last_frame = frame;
        }
        None => {
            textures.insert(key, TexStat { count: 1, first_frame: frame, last_frame: frame });
            drop(textures);
            log!("new texture kind @frame {frame}: {}", describe(&key));
        }
    }
    TEXTURES_DIRTY.store(true, Ordering::Relaxed);
}

/// # Safety
/// `swapchain` is the game's, alive through its `Present`.
unsafe fn on_present(swapchain: *mut c_void) {
    // SAFETY: the game's live swap chain (the caller's contract).
    unsafe {
        let frame = FRAME.fetch_add(1, Ordering::Relaxed) + 1;
        screens::on_frame(frame);
        // The interaction key's toggle, then the keybinds (which may need it).
        crate::input::on_frame();
        crate::keybinds::on_frame();
        if let Some(bridge) = bridge::get() {
            bridge.plugin.heartbeat.store(frame, Ordering::Release);
        }
        if !FIRST_PRESENT_LOGGED.swap(true, Ordering::Relaxed) {
            log_renderer(swapchain);
        }
        if frame == 1 || frame.is_multiple_of(WINDOW_EVERY_FRAMES) {
            log_window_mode(swapchain);
        }
        if frame.is_multiple_of(DUMP_EVERY_FRAMES) {
            if TEXTURES_DIRTY.swap(false, Ordering::Relaxed) {
                dump_textures();
            }
            if frame.is_multiple_of(DUMP_EVERY_FRAMES * 10) {
                screens::log_stats();
            }
        }
    }
}

/// The game's window as it is now, logged when it changes: exclusive fullscreen, borderless (a window covering
/// its monitor), or a window; and whether it's in the background (another window in front). Windows composes
/// WestPlay's hidden screen differently in each, so a log of a slow or frozen screen says which it was.
///
/// # Safety
/// As [`on_present`].
unsafe fn log_window_mode(swapchain: *mut c_void) {
    // SAFETY: the game's live swap chain (the caller's contract), only asked about itself; the window and output
    // it names are asked about, not changed.
    unsafe {
        let Some(swapchain) = IDXGISwapChain::from_raw_borrowed(&swapchain) else { return };
        let mut fullscreen = BOOL::default();
        let exclusive = swapchain.GetFullscreenState(Some(&mut fullscreen), None).is_ok() && fullscreen.as_bool();
        let Ok(desc) = swapchain.GetDesc() else { return };
        GAME_WINDOW.store(desc.OutputWindow.0 as isize, Ordering::Relaxed);
        let mut rect = RECT::default();
        let covers = !exclusive
            && GetWindowRect(desc.OutputWindow, &mut rect).is_ok()
            && swapchain.GetContainingOutput().and_then(|output| output.GetDesc()).is_ok_and(|said| {
                let monitor = said.DesktopCoordinates;
                rect.left <= monitor.left
                    && rect.top <= monitor.top
                    && rect.right >= monitor.right
                    && rect.bottom >= monitor.bottom
            });
        let behind = GetForegroundWindow() != desc.OutputWindow;
        let mode = window_mode(exclusive, covers, behind);
        if WINDOW_MODE.swap(mode, Ordering::Relaxed) != mode {
            log!("game window: {}", window_said(mode));
        }
    }
}

/// Whether the game's window shows: shown and not minimised, in front or not (Lossless Scaling's window is in
/// front of the game it scales: owner 2026-10-28, whenever the game shows, WestPlay works). Not yet known: no.
pub fn game_shown() -> bool {
    let window = HWND(GAME_WINDOW.load(Ordering::Relaxed) as *mut c_void);
    // SAFETY: plain queries about a window handle (one gone answers no).
    !window.is_invalid() && unsafe { IsWindowVisible(window).as_bool() && !IsIconic(window).as_bool() }
}

/// The window's mode as one number (never 0): exclusive 1, borderless 2, a window 3; 4 added in the background.
fn window_mode(exclusive: bool, covers: bool, behind: bool) -> u8 {
    (if exclusive {
        1
    } else if covers {
        2
    } else {
        3
    }) | if behind { 4 } else { 0 }
}

fn window_said(mode: u8) -> String {
    let shape = match mode & 3 {
        1 => "exclusive fullscreen",
        2 => "borderless fullscreen",
        _ => "a window",
    };
    if mode & 4 != 0 { format!("{shape}, in the background") } else { shape.to_owned() }
}

/// # Safety
/// As [`on_present`].
unsafe fn log_renderer(swapchain: *mut c_void) {
    // SAFETY: the game's live swap chain (the caller's contract), only asked about itself.
    unsafe {
        let d3d12_loaded = GetModuleHandleW(w!("d3d12.dll")).is_ok();
        log!("first Present; d3d12.dll loaded: {d3d12_loaded}");
        let Some(swapchain) = IDXGISwapChain::from_raw_borrowed(&swapchain) else { return };
        if let Ok(desc) = swapchain.GetDesc() {
            log!(
                "swap chain: {}x{} format {} buffers {} swap effect {} windowed {}",
                desc.BufferDesc.Width,
                desc.BufferDesc.Height,
                desc.BufferDesc.Format.0,
                desc.BufferCount,
                desc.SwapEffect.0,
                desc.Windowed.as_bool()
            );
        }
        match swapchain.GetDevice::<ID3D11Device>() {
            Ok(device) => {
                log!("renderer: Direct3D 11 (feature level {:#x})", device.GetFeatureLevel().0);
                publish_adapter(&device);
            }
            Err(_) => log!("renderer: NOT Direct3D 11 (swap chain device is not an ID3D11Device)"),
        }
    }
}

/// Tells the app which GPU the game renders on, so it creates the shared frame texture there.
///
/// # Safety
/// `device` is the game's live device.
unsafe fn publish_adapter(device: &ID3D11Device) {
    // SAFETY: the game's live device, only asked which adapter it's on.
    unsafe {
        let Some(bridge) = bridge::get() else { return };
        let Ok(adapter) = device.cast::<IDXGIDevice>().and_then(|d| d.GetAdapter()) else { return };
        let Ok(desc) = adapter.GetDesc() else { return };
        let plugin = &bridge.plugin;
        plugin.adapter_luid_low.store(desc.AdapterLuid.LowPart, Ordering::Relaxed);
        plugin.adapter_luid_high.store(desc.AdapterLuid.HighPart, Ordering::Relaxed);
        plugin.pid.store(std::process::id(), Ordering::Release);
        let name = String::from_utf16_lossy(&desc.Description);
        log!(
            "adapter: {} (LUID {:#x}:{:#x})",
            name.trim_end_matches('\0'),
            desc.AdapterLuid.HighPart,
            desc.AdapterLuid.LowPart
        );
    }
}

/// Writes the texture kinds seen to `textures.txt`, on the log's thread (the table copied under its lock,
/// which texture creation takes too, the text made and written after).
fn dump_textures() {
    log::write_file_later("textures.txt", || {
        let mut rows: Vec<(TexKey, TexStat)> = {
            let textures = TEXTURES.lock().unwrap_or_else(|e| e.into_inner());
            textures.iter().map(|(key, stat)| (*key, *stat)).collect()
        };
        rows.sort_by_key(|(key, _)| *key);
        let mut out = String::from("# texture kinds seen by CreateTexture2D (sorted by size)\n");
        out += &format!("# frame {}\n", FRAME.load(Ordering::Relaxed));
        out += "#   count  first_frame  last_frame  description\n";
        for (key, stat) in rows {
            out +=
                &format!("{:>9}  {:>11}  {:>10}  {}\n", stat.count, stat.first_frame, stat.last_frame, describe(&key));
        }
        out
    });
}

fn describe(key: &TexKey) -> String {
    format!(
        "{}x{} mips={} array={} fmt={} samples={} usage={} bind={:#x} cpu={:#x} misc={:#x} init={}",
        key.width,
        key.height,
        key.mips,
        key.array,
        key.format,
        key.samples,
        key.usage,
        key.bind,
        key.cpu,
        key.misc,
        key.initial_data
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_the_games_window_as_it_is() {
        assert_eq!(window_said(window_mode(true, false, false)), "exclusive fullscreen");
        assert_eq!(window_said(window_mode(false, true, false)), "borderless fullscreen");
        assert_eq!(window_said(window_mode(false, false, true)), "a window, in the background");
        // Never 0, the mode not logged yet.
        assert!((0..8).all(|n| window_mode(n & 1 != 0, n & 2 != 0, n & 4 != 0) != 0));
    }
}
