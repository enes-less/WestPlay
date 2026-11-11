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

use crate::hooks::{guarded, hook, vtable_entry};
use crate::{bridge, log, screens};

// vtable slots (IUnknown = 0..2), shared with the probe
use westplay_shared::probe::{
    VTBL_CONTEXT_OM_SET_RENDER_TARGETS, VTBL_CONTEXT_OM_SET_RENDER_TARGETS_AND_UAVS,
    VTBL_CONTEXT_PS_SET_SHADER_RESOURCES, VTBL_DEVICE_CREATE_SHADER_RESOURCE_VIEW, VTBL_DEVICE_CREATE_TEXTURE2D,
    VTBL_SWAPCHAIN_PRESENT, VTBL_SWAPCHAIN1_PRESENT1,
};

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
/// `Present` and `Present1` hooked in their code too (beside their vtable's entries): their trampolines.
static ORIG_PRESENT_CODE: AtomicUsize = AtomicUsize::new(0);
static ORIG_PRESENT1_CODE: AtomicUsize = AtomicUsize::new(0);
/// Those two functions, and their first bytes as our hooks left them (`note_present_code`), to see another's hook
/// written over ours since ([`keep_present_hooked`]).
static PRESENT_CODE: Mutex<Vec<PresentCode>> = Mutex::new(Vec::new());

struct PresentCode {
    target: usize,
    detour: usize,
    original: &'static AtomicUsize,
    name: &'static str,
    ours: Option<[u8; 8]>,
    hooked_again: u32,
}

/// How many times a function's hook is made again over another's written over it, at most: a tool writing its own
/// back each time isn't fought.
const HOOKED_AGAIN_MOST: u32 = 3;
/// How many game frames (the telemetry's) without one through `Present` before the hooks are looked at.
const FRAMES_WITHOUT_PRESENT: u64 = 60;
static GAME_FRAMES: AtomicU64 = AtomicU64::new(0);
static PRESENT_FRAME_SEEN: AtomicU64 = AtomicU64::new(0);
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
    // SAFETY: the functions are d3d11.dll's and dxgi.dll's own (`dummy_functions`), hooked with detours of their
    // exact signatures (below).
    unsafe {
        // Found outside the game by the probe (westplay_shared::probe): a throwaway swap chain made inside it broke
        // OptiScaler and the Steam overlay, the game crashing on its first frame. Without the probe (not installed
        // beside the plugin, or it couldn't say), as before: one made here.
        let functions = match probed_functions() {
            Ok(functions) => {
                log!("the functions to hook found by the probe, outside the game");
                functions
            }
            Err(why) => {
                log!("the probe couldn't find the functions to hook ({why}): found with a throwaway device here");
                dummy_functions()?
            }
        };

        hook(
            functions.create_texture2d,
            create_texture2d_hook as *mut c_void,
            &ORIG_CREATE_TEXTURE2D,
            "CreateTexture2D",
        )?;
        hook(
            functions.create_shader_resource_view,
            create_shader_resource_view_hook as *mut c_void,
            &ORIG_CREATE_SHADER_RESOURCE_VIEW,
            "CreateShaderResourceView",
        )?;
        hook(
            functions.ps_set_shader_resources,
            ps_set_shader_resources_hook as *mut c_void,
            &ORIG_PS_SET_SHADER_RESOURCES,
            "PSSetShaderResources",
        )?;
        // Present and Present1 in the swap chain's vtable when it's its class's (in a module's image), whatever else
        // hooks them (OptiScaler's wrapper, the Steam overlay, RTSS) left as it is (`hooks::hook_vtable`); and in
        // Windows' own functions too, which the probe found: a layer loaded with the game before the plugin may
        // have kept the functions' addresses and call them itself, never through the vtable (Snowy Moon's
        // dxgi.dll in the game's folder, owner 2026-11-08: no frame ever reached the vtable's hook, the truck's
        // screen black and the game taken for closed). A frame through both is the outer hook's alone
        // ([`PresentDepth`]). One in heap memory may be its object's own: then only the functions.
        if crate::hooks::in_a_module(functions.swapchain_vtable) {
            crate::hooks::hook_vtable(
                functions.swapchain_vtable,
                VTBL_SWAPCHAIN_PRESENT,
                present_hook as *mut c_void,
                &ORIG_PRESENT,
                "Present",
            )?;
            crate::hooks::hook_vtable(
                functions.swapchain_vtable,
                VTBL_SWAPCHAIN1_PRESENT1,
                present1_hook as *mut c_void,
                &ORIG_PRESENT1,
                "Present1",
            )?;
            hook(functions.present, present_code_hook as *mut c_void, &ORIG_PRESENT_CODE, "Present (its code)")?;
            hook(functions.present1, present1_code_hook as *mut c_void, &ORIG_PRESENT1_CODE, "Present1 (its code)")?;
            let mut code = PRESENT_CODE.lock().unwrap_or_else(|e| e.into_inner());
            for (target, detour, original, name) in [
                (functions.present, present_code_hook as *const () as usize, &ORIG_PRESENT_CODE, "Present (its code)"),
                (
                    functions.present1,
                    present1_code_hook as *const () as usize,
                    &ORIG_PRESENT1_CODE,
                    "Present1 (its code)",
                ),
            ] {
                code.push(PresentCode { target, detour, original, name, ours: None, hooked_again: 0 });
            }
        } else {
            log!(
                "the swap chain's vtable isn't in a module ({:#x}): Present hooked in its code",
                functions.swapchain_vtable
            );
            hook(functions.present, present_hook as *mut c_void, &ORIG_PRESENT, "Present")?;
            hook(functions.present1, present1_hook as *mut c_void, &ORIG_PRESENT1, "Present1")?;
        }
        hook(
            functions.om_set_render_targets,
            om_set_render_targets_hook as *mut c_void,
            &ORIG_OM_SET_RENDER_TARGETS,
            "OMSetRenderTargets",
        )?;
        hook(
            functions.om_set_render_targets_and_uavs,
            om_set_render_targets_and_uavs_hook as *mut c_void,
            &ORIG_OM_SET_RENDER_TARGETS_AND_UAVS,
            "OMSetRenderTargetsAndUnorderedAccessViews",
        )?;
        Ok(())
    }
}

/// Notes `Present`'s and `Present1`'s first bytes as the hooks left them, once switched on.
///
/// # Safety
/// After [`install`] and [`crate::hooks::enable_all`].
pub unsafe fn note_present_code() {
    for code in PRESENT_CODE.lock().unwrap_or_else(|e| e.into_inner()).iter_mut() {
        // SAFETY: a function `install` hooked.
        code.ours = Some(unsafe { crate::hooks::first_bytes(code.target) });
    }
}

/// Each game frame (the telemetry's frame end, on the game's thread): if none has come through `Present` for a while,
/// and another's hook was written over ours on `Present`'s code since (RTSS writes its own over any before it on the
/// game's first frame; with a layer keeping `Present`'s address, as Snowy Moon's dxgi.dll, the game's frames went
/// nowhere near the vtable's hook and the truck's screen stayed black, owner 2026-11-08), ours is made again in
/// front of it. At most [`HOOKED_AGAIN_MOST`] times a function.
pub fn keep_present_hooked() {
    let game_frames = GAME_FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
    let presented = FRAME.load(Ordering::Relaxed);
    if presented != PRESENT_FRAME_SEEN.swap(presented, Ordering::Relaxed) {
        GAME_FRAMES.store(0, Ordering::Relaxed);
        return;
    }
    if game_frames < FRAMES_WITHOUT_PRESENT {
        return;
    }
    GAME_FRAMES.store(0, Ordering::Relaxed);
    guarded(|| {
        for code in PRESENT_CODE.lock().unwrap_or_else(|e| e.into_inner()).iter_mut() {
            let Some(ours) = code.ours else { continue };
            // SAFETY: a function `install` hooked.
            let now = unsafe { crate::hooks::first_bytes(code.target) };
            if now == ours
                || code.hooked_again >= HOOKED_AGAIN_MOST
                || crate::hooks::jump_of(code.target, now).is_none()
            {
                continue;
            }
            code.hooked_again += 1;
            // SAFETY: the function and its detour of the same signature, which calls on through `original`.
            match unsafe { crate::hooks::hook_over_jump(code.target, code.detour, code.original, code.name) } {
                // SAFETY: as above.
                Ok(()) => code.ours = Some(unsafe { crate::hooks::first_bytes(code.target) }),
                Err(why) => log!("couldn't hook {} again: {why}", code.name),
            }
        }
    });
}

/// Diagnostics on unload (the hooks themselves are removed by `hooks::remove_all`).
pub fn on_unload() {
    dump_textures();
    screens::log_stats();
}

/// The functions to hook, as a throwaway device and swap chain have them.
#[derive(Debug)]
struct DummyFunctions {
    create_texture2d: usize,
    create_shader_resource_view: usize,
    ps_set_shader_resources: usize,
    om_set_render_targets: usize,
    om_set_render_targets_and_uavs: usize,
    present: usize,
    present1: usize,
    /// The swap chain's vtable: dxgi.dll's swap chain class's (a flip-model swap chain's, the game's, the same:
    /// measured 2026-11-07), or the class of a layer wrapping swap chains (OptiScaler's).
    swapchain_vtable: usize,
}

/// How long the probe may take to say where the functions are (it makes a device: well under a second).
const PROBE_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// The functions to hook, as the probe beside the plugin found them in its own process, each resolved in the game's
/// copy of its DLL (the same file: `westplay_shared::probe::resolve`).
fn probed_functions() -> Result<DummyFunctions, String> {
    use westplay_shared::probe;
    let plugin = probe::locate(probed_functions as *const () as usize).ok_or("the plugin's own file not found")?;
    let path = std::path::Path::new(&plugin.module).with_file_name(probe::FILE_NAME);
    if !path.is_file() {
        return Err(format!("no {} beside the plugin", probe::FILE_NAME));
    }
    probe_at(&path)
}

/// The functions to hook, as the probe at `path` finds them, resolved here.
fn probe_at(path: &std::path::Path) -> Result<DummyFunctions, String> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use westplay_shared::probe;
    /// No console window for it (Windows' `CREATE_NO_WINDOW`).
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = std::process::Command::new(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("it couldn't start: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("no output")?;
    let reading = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < PROBE_WAIT => std::thread::sleep(std::time::Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("it took over {} s", PROBE_WAIT.as_secs()));
            }
            Err(e) => return Err(format!("it couldn't be waited for: {e}")),
        }
    };
    let output = reading.join().unwrap_or_default();
    if !status.success() {
        let mut why = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = stderr.read_to_string(&mut why);
        }
        return Err(format!("it ended with {status}: {}", why.trim()));
    }
    let found = probe::parse(&output)?;
    let at = |name: &str| probe::resolve(&found[name]).map_err(|why| format!("{name}: {why}"));
    log!(
        "probe: dxgi.dll's Present at +{:#x}, its swap chain vtable at +{:#x}, in {:.0} ms",
        found["present"].offset,
        found["swapchain_vtable"].offset,
        started.elapsed().as_secs_f64() * 1000.0
    );
    Ok(DummyFunctions {
        create_texture2d: at("create_texture2d")?,
        create_shader_resource_view: at("create_shader_resource_view")?,
        ps_set_shader_resources: at("ps_set_shader_resources")?,
        om_set_render_targets: at("om_set_render_targets")?,
        om_set_render_targets_and_uavs: at("om_set_render_targets_and_uavs")?,
        present: at("present")?,
        present1: at("present1")?,
        swapchain_vtable: at("swapchain_vtable")?,
    })
}

/// Creates a hidden window + D3D11 device + swap chain and returns the functions to hook (without the probe: it
/// breaks tools watching the game's swap chains, `probed_functions`).
///
/// # Safety
/// On a thread that may own a window (the game's main thread, at init).
unsafe fn dummy_functions() -> Result<DummyFunctions, String> {
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
unsafe fn create_dummy_device(hwnd: HWND) -> Result<DummyFunctions, String> {
    // SAFETY: a throwaway device and swap chain on `hwnd` (the caller's, alive through the call), their
    // descriptions locals; each function read from its live object's vtable, its slot within it (the slot numbers
    // are the interfaces' fixed layout).
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
        // Read while the objects live: the functions stay, in d3d11.dll and dxgi.dll, but a context's vtable is its
        // own memory, freed with it (a start of the plugin in a small process fell reading it after, 25 times in 40).
        Ok(DummyFunctions {
            create_texture2d: vtable_entry(device.as_raw(), VTBL_DEVICE_CREATE_TEXTURE2D),
            create_shader_resource_view: vtable_entry(device.as_raw(), VTBL_DEVICE_CREATE_SHADER_RESOURCE_VIEW),
            ps_set_shader_resources: vtable_entry(context.as_raw(), VTBL_CONTEXT_PS_SET_SHADER_RESOURCES),
            om_set_render_targets: vtable_entry(context.as_raw(), VTBL_CONTEXT_OM_SET_RENDER_TARGETS),
            om_set_render_targets_and_uavs: vtable_entry(context.as_raw(), VTBL_CONTEXT_OM_SET_RENDER_TARGETS_AND_UAVS),
            present: vtable_entry(swapchain.as_raw(), VTBL_SWAPCHAIN_PRESENT),
            // IDXGISwapChain1 shares the object's vtable; slot 22 exists on DXGI 1.2+ swap chains.
            present1: vtable_entry(swapchain.as_raw(), VTBL_SWAPCHAIN1_PRESENT1),
            swapchain_vtable: *(swapchain.as_raw() as *const usize),
        })
    }
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

thread_local! {
    /// How many `Present` / `Present1` calls this thread is inside of, through the hooks ([`PresentDepth`]).
    static PRESENT_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// The deepest re-entry said so far ([`PresentDepth`]): each new depth up to [`PRESENT_DEPTH_SAID_MOST`] once.
static PRESENT_DEPTH_SAID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
const PRESENT_DEPTH_SAID_MOST: u32 = 8;

/// A `Present` (or `Present1`) call through the hooks on this thread, while it lasts. Only the outermost handles the
/// frame (`outermost`): one made inside another is the same frame passing a second hook (the vtable's entry, then
/// the function's code), or a layer over `Present` (an upscaler's or a frame generator's) calling back. A new depth
/// is said once, past the vtable's and the code's two: two players' games (2026-11-07: an upscaler's `d3d12.dll`
/// loaded, the swap chain smaller than the screen) crashed with their stack used up on entering it.
struct PresentDepth {
    outermost: bool,
}

impl PresentDepth {
    fn enter(which: &str) -> Self {
        let depth = PRESENT_DEPTH.with(|depth| {
            depth.set(depth.get() + 1);
            depth.get()
        });
        if depth > 2
            && depth <= PRESENT_DEPTH_SAID_MOST
            && PRESENT_DEPTH_SAID.fetch_max(depth, Ordering::AcqRel) < depth
        {
            guarded(|| {
                log!(
                    "{which} called again inside itself on one thread: {depth} deep (a layer over Present calling back)"
                )
            });
        }
        PresentDepth { outermost: depth == 1 }
    }
}

impl Drop for PresentDepth {
    fn drop(&mut self) {
        PRESENT_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// `Present` through a hook: the frame handled if it's the outermost on this thread, then on to `original`.
///
/// # Safety
/// `this` and the arguments are the game's call's; `original` holds a function of `Present`'s signature, stored
/// before the hooks were switched on.
unsafe fn present_through(this: *mut c_void, sync_interval: u32, flags: u32, original: &AtomicUsize) -> HRESULT {
    let depth = PresentDepth::enter("Present");
    // SAFETY: the game's swap chain, alive through its call; then the original function (the caller's contract),
    // called with the game's arguments as they came.
    unsafe {
        if depth.outermost {
            guarded(|| on_present(this));
        }
        let original: PresentFn = std::mem::transmute(original.load(Ordering::Acquire));
        original(this, sync_interval, flags)
    }
}

/// `Present1` through a hook, as [`present_through`].
///
/// # Safety
/// As [`present_through`], for `Present1`.
unsafe fn present1_through(
    this: *mut c_void,
    sync_interval: u32,
    flags: u32,
    params: *const DXGI_PRESENT_PARAMETERS,
    original: &AtomicUsize,
) -> HRESULT {
    let depth = PresentDepth::enter("Present1");
    // SAFETY: as `present_through`.
    unsafe {
        if depth.outermost {
            guarded(|| on_present(this));
        }
        let original: Present1Fn = std::mem::transmute(original.load(Ordering::Acquire));
        original(this, sync_interval, flags, params)
    }
}

/// # Safety
/// Called in place of `IDXGISwapChain::Present` (its vtable's entry, or its code without one), with its arguments.
unsafe extern "system" fn present_hook(this: *mut c_void, sync_interval: u32, flags: u32) -> HRESULT {
    // SAFETY: the game's call; its original stored before the hooks were switched on (`install`, `enable_all`).
    unsafe { present_through(this, sync_interval, flags, &ORIG_PRESENT) }
}

/// # Safety
/// Called in place of `IDXGISwapChain::Present`'s code (beside its vtable's entry), with its arguments.
unsafe extern "system" fn present_code_hook(this: *mut c_void, sync_interval: u32, flags: u32) -> HRESULT {
    // SAFETY: as `present_hook`, its trampoline.
    unsafe { present_through(this, sync_interval, flags, &ORIG_PRESENT_CODE) }
}

/// # Safety
/// Called in place of `IDXGISwapChain1::Present1` (its vtable's entry, or its code without one), with its
/// arguments.
unsafe extern "system" fn present1_hook(
    this: *mut c_void,
    sync_interval: u32,
    flags: u32,
    params: *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT {
    // SAFETY: as `present_hook`.
    unsafe { present1_through(this, sync_interval, flags, params, &ORIG_PRESENT1) }
}

/// # Safety
/// Called in place of `IDXGISwapChain1::Present1`'s code (beside its vtable's entry), with its arguments.
unsafe extern "system" fn present1_code_hook(
    this: *mut c_void,
    sync_interval: u32,
    flags: u32,
    params: *const DXGI_PRESENT_PARAMETERS,
) -> HRESULT {
    // SAFETY: as `present_hook`, its trampoline.
    unsafe { present1_through(this, sync_interval, flags, params, &ORIG_PRESENT1_CODE) }
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
        plugin.game.store(westplay_shared::game_number(crate::game_id()), Ordering::Relaxed);
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

    /// The probe's functions, found in its own process and resolved here, are exactly a throwaway device's made
    /// here (in a process with nothing hooking its DirectX, both are Windows' own).
    #[test]
    #[ignore = "runs the probe: build it first (cargo build -p westplay-probe)"]
    fn the_probe_finds_what_a_throwaway_device_here_has() {
        let probe = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/debug")
            .join(westplay_shared::probe::FILE_NAME);
        assert!(probe.is_file(), "no probe at {}: cargo build -p westplay-probe", probe.display());
        let probed = probe_at(&probe).unwrap();
        // SAFETY: a throwaway device of this test's, as the plugin made one in the game before the probe.
        let here = unsafe { dummy_functions() }.unwrap();
        assert_eq!(probed.create_texture2d, here.create_texture2d);
        assert_eq!(probed.create_shader_resource_view, here.create_shader_resource_view);
        assert_eq!(probed.ps_set_shader_resources, here.ps_set_shader_resources);
        assert_eq!(probed.om_set_render_targets, here.om_set_render_targets);
        assert_eq!(probed.om_set_render_targets_and_uavs, here.om_set_render_targets_and_uavs);
        assert_eq!(probed.present, here.present);
        assert_eq!(probed.present1, here.present1);
        assert_eq!(probed.swapchain_vtable, here.swapchain_vtable);
        // No probe there: why, not a guess.
        assert!(probe_at(std::path::Path::new("no-such-probe.exe")).unwrap_err().contains("couldn't start"));
    }

    /// A swap chain of the throwaway kind and the game's (flip model) share one vtable, in dxgi.dll's image; its
    /// Present swapped there, a real Present goes through the detour and on into dxgi's own, and taken out, the
    /// entry is as it was.
    #[test]
    fn present_hooked_in_the_shared_vtable_reaches_dxgis_own() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "system" fn counting(this: *mut c_void, sync: u32, flags: u32) -> HRESULT {
            CALLS.fetch_add(1, Ordering::Relaxed);
            // SAFETY: what the entry held, dxgi's Present, called with the caller's arguments.
            unsafe {
                let original: PresentFn = std::mem::transmute(ORIGINAL.load(Ordering::Acquire));
                original(this, sync, flags)
            }
        }
        let _one_at_a_time = crate::hooks::VTABLE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: throwaway windows and swap chains of this test's, alive through it; the vtable is dxgi's,
        // its entry put back before the test ends.
        unsafe {
            let make = |effect: windows::Win32::Graphics::Dxgi::DXGI_SWAP_EFFECT, count: u32| {
                let hinstance = GetModuleHandleW(None).unwrap();
                let hwnd = CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    w!("westplay-test"),
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
                .unwrap();
                let desc = DXGI_SWAP_CHAIN_DESC {
                    BufferDesc: DXGI_MODE_DESC {
                        Width: 64,
                        Height: 64,
                        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                        ..Default::default()
                    },
                    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                    BufferCount: count,
                    OutputWindow: hwnd,
                    Windowed: BOOL::from(true),
                    SwapEffect: effect,
                    Flags: 0,
                };
                let mut swapchain: Option<IDXGISwapChain> = None;
                D3D11CreateDeviceAndSwapChain(
                    None,
                    D3D_DRIVER_TYPE_HARDWARE,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_FLAG(0),
                    None,
                    D3D11_SDK_VERSION,
                    Some(&desc),
                    Some(&mut swapchain),
                    None,
                    None,
                    None,
                )
                .unwrap();
                (swapchain.unwrap(), hwnd)
            };
            let (throwaway, first) = make(DXGI_SWAP_EFFECT_DISCARD, 1);
            let (flip, second) = make(windows::Win32::Graphics::Dxgi::DXGI_SWAP_EFFECT_FLIP_DISCARD, 2);
            let vtable = *(throwaway.as_raw() as *const usize);
            assert_eq!(*(flip.as_raw() as *const usize), vtable, "one vtable for both kinds");
            assert!(crate::hooks::in_a_module(vtable));
            let entry = vtable + VTBL_SWAPCHAIN_PRESENT * size_of::<usize>();
            let before = *(entry as *const usize);
            crate::hooks::hook_vtable(vtable, VTBL_SWAPCHAIN_PRESENT, counting as *mut c_void, &ORIGINAL, "Present")
                .unwrap();
            assert!(flip.Present(0, windows::Win32::Graphics::Dxgi::DXGI_PRESENT(0)).is_ok());
            assert_eq!(CALLS.load(Ordering::Relaxed), 1);
            crate::hooks::remove_all();
            assert_eq!(*(entry as *const usize), before);
            assert!(flip.Present(0, windows::Win32::Graphics::Dxgi::DXGI_PRESENT(0)).is_ok());
            assert_eq!(CALLS.load(Ordering::Relaxed), 1, "no longer through the detour");
            drop((throwaway, flip));
            let _ = DestroyWindow(first);
            let _ = DestroyWindow(second);
        }
    }

    /// A layer that kept dxgi's `Present` before the plugin hooked (Snowy Moon's dxgi.dll in the game's folder) calls it
    /// itself, never through the vtable: hooked in both the vtable's entry and the function's code, as `install` does,
    /// its call is caught, and a frame through the vtable (then the code) is handled once, not twice; with another's
    /// hook written over ours on the first frame (RTSS's, where it runs), ours made again in front of it, and taken
    /// out, the other's left as it was.
    #[test]
    fn a_frame_is_handled_once_whether_through_the_vtable_or_kept_by_a_layer() {
        static HANDLED: AtomicUsize = AtomicUsize::new(0);
        static VIA_VTABLE: AtomicUsize = AtomicUsize::new(0);
        static VIA_CODE: AtomicUsize = AtomicUsize::new(0);
        /// As `present_through`, counting the frames handled rather than handling them.
        unsafe fn through(this: *mut c_void, sync: u32, flags: u32, original: &AtomicUsize) -> HRESULT {
            let depth = PresentDepth::enter("Present");
            if depth.outermost {
                HANDLED.fetch_add(1, Ordering::Relaxed);
            }
            // SAFETY: what the hook replaced, dxgi's Present (or its trampoline), with the caller's arguments.
            unsafe {
                let original: PresentFn = std::mem::transmute(original.load(Ordering::Acquire));
                original(this, sync, flags)
            }
        }
        unsafe extern "system" fn vtable_detour(this: *mut c_void, sync: u32, flags: u32) -> HRESULT {
            // SAFETY: the caller's call.
            unsafe { through(this, sync, flags, &VIA_VTABLE) }
        }
        unsafe extern "system" fn code_detour(this: *mut c_void, sync: u32, flags: u32) -> HRESULT {
            // SAFETY: the caller's call.
            unsafe { through(this, sync, flags, &VIA_CODE) }
        }
        let _one_at_a_time = crate::hooks::VTABLE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: a throwaway window and swap chain of this test's, alive through it; dxgi's vtable entry and code
        // hooked and put back before the test ends.
        unsafe {
            let hinstance = GetModuleHandleW(None).unwrap();
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("westplay-test"),
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
            .unwrap();
            let desc = DXGI_SWAP_CHAIN_DESC {
                BufferDesc: DXGI_MODE_DESC {
                    Width: 64,
                    Height: 64,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    ..Default::default()
                },
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                OutputWindow: hwnd,
                Windowed: BOOL::from(true),
                SwapEffect: windows::Win32::Graphics::Dxgi::DXGI_SWAP_EFFECT_FLIP_DISCARD,
                Flags: 0,
            };
            let mut swapchain: Option<IDXGISwapChain> = None;
            D3D11CreateDeviceAndSwapChain(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                None,
                D3D11_SDK_VERSION,
                Some(&desc),
                Some(&mut swapchain),
                None,
                None,
                None,
            )
            .unwrap();
            let swapchain = swapchain.unwrap();
            let vtable = *(swapchain.as_raw() as *const usize);
            // The layer, loaded first, keeps dxgi's Present as it is then.
            let kept: PresentFn =
                std::mem::transmute(crate::hooks::vtable_entry(swapchain.as_raw(), VTBL_SWAPCHAIN_PRESENT));
            crate::hooks::hook_vtable(
                vtable,
                VTBL_SWAPCHAIN_PRESENT,
                vtable_detour as *mut c_void,
                &VIA_VTABLE,
                "Present",
            )
            .unwrap();
            crate::hooks::hook(kept as usize, code_detour as *mut c_void, &VIA_CODE, "Present (its code)").unwrap();
            crate::hooks::enable_all().unwrap();
            let ours = crate::hooks::first_bytes(kept as *const () as usize);
            let present = windows::Win32::Graphics::Dxgi::DXGI_PRESENT(0);
            assert!(swapchain.Present(0, present).is_ok());
            assert_eq!(HANDLED.load(Ordering::Relaxed), 1, "through the vtable, then the code: one frame");
            // Another's hook written over ours on the first frame (RTSS, where it runs: it hooks this test's process
            // too): ours made again in front of it, as `keep_present_hooked` does in the game.
            let now = crate::hooks::first_bytes(kept as *const () as usize);
            if now != ours {
                assert!(crate::hooks::jump_of(kept as *const () as usize, now).is_some(), "another's plain jump");
                crate::hooks::hook_over_jump(
                    kept as *const () as usize,
                    code_detour as *const () as usize,
                    &VIA_CODE,
                    "Present (its code)",
                )
                .unwrap();
            }
            assert!(kept(swapchain.as_raw(), 0, 0).is_ok());
            assert_eq!(HANDLED.load(Ordering::Relaxed), 2, "the layer's own call: caught in the code");
            assert!(swapchain.Present(0, present).is_ok());
            assert_eq!(HANDLED.load(Ordering::Relaxed), 3, "and through the vtable still once");
            crate::hooks::remove_all();
            if now != ours {
                assert_eq!(crate::hooks::first_bytes(kept as *const () as usize), now, "the other's jump put back");
            }
            assert!(swapchain.Present(0, present).is_ok());
            assert!(kept(swapchain.as_raw(), 0, 0).is_ok());
            assert_eq!(HANDLED.load(Ordering::Relaxed), 3, "taken out, neither way through the hooks");
            drop(swapchain);
            let _ = DestroyWindow(hwnd);
        }
    }

    /// Present inside Present on one thread counts its depth, each new depth said once, back to none as each ends;
    /// another thread counts its own.
    #[test]
    fn counts_present_inside_present_on_its_thread() {
        let depth = || PRESENT_DEPTH.with(|depth| depth.get());
        {
            let outer = PresentDepth::enter("Present");
            assert_eq!(depth(), 1);
            assert!(outer.outermost, "the game's own Present handles the frame");
            assert_eq!(PRESENT_DEPTH_SAID.load(Ordering::Acquire), 1, "the game's own Present isn't said");
            {
                // The same frame through the function's code after its vtable's entry: not handled again, not said.
                let code = PresentDepth::enter("Present");
                assert!(!code.outermost);
                assert_eq!(PRESENT_DEPTH_SAID.load(Ordering::Acquire), 1);
                let innermost = PresentDepth::enter("Present1");
                assert!(!innermost.outermost);
                assert_eq!(depth(), 3);
                assert_eq!(PRESENT_DEPTH_SAID.load(Ordering::Acquire), 3);
                std::thread::spawn(|| {
                    let other = PresentDepth::enter("Present");
                    assert!(other.outermost);
                    assert_eq!(PRESENT_DEPTH.with(|depth| depth.get()), 1);
                })
                .join()
                .unwrap();
            }
            assert_eq!(depth(), 1);
        }
        assert_eq!(depth(), 0);
    }

    #[test]
    fn says_the_games_window_as_it_is() {
        assert_eq!(window_said(window_mode(true, false, false)), "exclusive fullscreen");
        assert_eq!(window_said(window_mode(false, true, false)), "borderless fullscreen");
        assert_eq!(window_said(window_mode(false, false, true)), "a window, in the background");
        // Never 0, the mode not logged yet.
        assert!((0..8).all(|n| window_mode(n & 1 != 0, n & 2 != 0, n & 4 != 0) != 0));
    }
}
