//! In-cabin screen takeover.
//!
//! The WestPlay mod (`westplay_setup::game_mod`, built from the player's own game) patches the game's GPS UI scripts: each draws one full-screen
//! image, the *screen texture* of its layout (B8G8R8A8 sRGB, one mip), as the top layer of the script's
//! "screen on" group. The game renders it like any other UI image, through its own render-target pooling,
//! post-processing, flipping and mip generation, and switches the screen on and off with the truck's
//! electrics.
//!
//! Every layout (`crates/shared/screen_layouts.txt`) has its own texture, [`SCREEN_WIDTH`] wide and
//! [`SCREEN_HEIGHT`] plus the layout's index high, so its size tells which kind of screen the game is
//! drawing; we publish that for the app, which picks its screen profile by it. App frames are
//! [`SCREEN_WIDTH`] x [`SCREEN_HEIGHT`]; a texture's extra rows repeat the frame's bottom row.
//!
//! We never touch the game's render targets; we only keep the screen texture filled: when the game binds it
//! to draw a screen, we copy the newest app frame into it first. When WestPlay is switched off or the app is
//! not running, it is made fully transparent, so the game's original screen shows through.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use westplay_shared::{SCREEN_HEIGHT, SCREEN_WIDTH, ScreenLayout, screen_layout_for_texture, screen_layouts};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_IMMUTABLE, ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView, ID3D11ShaderResourceView,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_TYPELESS, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM,
    DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_SAMPLE_DESC,
};
use windows::core::Interface;

use crate::frame::FrameSource;
use crate::{bridge, log};

/// Screen textures the game has not drawn for this many frames are released (it reloads them e.g. when
/// the truck changes; we hold references, so without this they would accumulate).
const STALE_AFTER_FRAMES: u64 = 600;

/// `ScreenTexture::version` when the texture holds transparent pixels.
const TRANSPARENT: u64 = u64::MAX;

/// One loaded instance of the screen texture.
struct ScreenTexture {
    texture: ID3D11Texture2D,
    height: u32,
    /// Version of the app frame last copied into it, [`TRANSPARENT`], or 0 for the mod's initial content.
    version: u64,
    last_used: u64,
}

#[derive(Default)]
struct State {
    screens: Vec<ScreenTexture>,
    frames: FrameSource,
    /// All-zero texture copied in while WestPlay is off, as high as the highest screen texture.
    transparent: Option<ID3D11Texture2D>,
    /// The device `frames` and `transparent` were made on (its address; 0 before any).
    device: usize,
}

impl State {
    /// Makes `device` (its address) the one this state's textures belong to: if the game has a new one (it
    /// recreated it), the old device's textures are let go, to be made again on the new one (a copy between
    /// two devices' textures is undefined). Holding them keeps the old device alive, so its address can't
    /// come back as the new one's meanwhile. Returns whether it changed.
    fn adopt_device(&mut self, device: usize) -> bool {
        if self.device == device {
            return false;
        }
        self.device = device;
        self.frames = FrameSource::default();
        self.transparent = None;
        true
    }
}

struct Shared(State);
// SAFETY: its COM pointers are only used on the game's render thread, and only through the mutex it's kept in.
unsafe impl Send for Shared {}

static STATE: LazyLock<Mutex<Shared>> = LazyLock::new(|| Mutex::new(Shared(State::default())));
/// Whether the bound render target looks like a screen UI target (worth inspecting the bound textures).
static CURRENT_IS_CANDIDATE: AtomicBool = AtomicBool::new(false);
/// Current game frame, advanced by `on_frame`.
static FRAME: AtomicU64 = AtomicU64::new(0);

fn state() -> std::sync::MutexGuard<'static, Shared> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Called at the start of `OMSetRenderTargets*`.
///
/// # Safety
/// From the `OMSetRenderTargets*` hooks, with the game's arguments, on its render thread.
pub unsafe fn on_set_render_targets(count: u32, views: *const *mut c_void) {
    // SAFETY: the game's own arguments to `OMSetRenderTargets` (the caller's contract).
    unsafe {
        let candidate = first_target(count, views).is_some_and(|desc| is_screen_target(&desc));
        CURRENT_IS_CANDIDATE.store(candidate, Ordering::Relaxed);
    }
}

/// Called at the start of `PSSetShaderResources`, before the textures are bound. If the game is about
/// to draw the screen texture, refresh it with the newest app frame.
///
/// # Safety
/// From the `PSSetShaderResources` hook, with the game's arguments, on its render thread.
pub unsafe fn on_set_shader_resources(context: *mut c_void, count: u32, views: *const *mut c_void) {
    // SAFETY: the game's own arguments to `PSSetShaderResources` (the caller's contract): `count` views (checked
    // for a sane number), each a live view or null, and its live context.
    unsafe {
        if !CURRENT_IS_CANDIDATE.load(Ordering::Relaxed) || views.is_null() || count == 0 || count > 128 {
            return;
        }
        for view in std::slice::from_raw_parts(views, count as usize) {
            let Some(view) = ID3D11ShaderResourceView::from_raw_borrowed(view) else { continue };
            let Ok(resource) = view.GetResource() else { continue };
            let Ok(texture) = resource.cast::<ID3D11Texture2D>() else { continue };
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            if let Some(layout) = screen_layout(&desc)
                && let Some(context) = ID3D11DeviceContext::from_raw_borrowed(&context)
            {
                refresh(context, texture, desc.Height, layout);
            }
        }
    }
}

/// # Safety
/// On the game's render thread, with its live context and a screen texture of its.
unsafe fn refresh(context: &ID3D11DeviceContext, texture: ID3D11Texture2D, height: u32, layout: usize) {
    // SAFETY: the game's live context and screen texture (the caller's contract); copies stay within both
    // textures (whole frames, or the box of the screen's height).
    unsafe {
        let frame = FRAME.load(Ordering::Relaxed);
        let mut state = state();
        let state = &mut state.0;
        let index = match state.screens.iter().position(|s| s.texture.as_raw() == texture.as_raw()) {
            Some(index) => index,
            None => {
                let id = screen_layouts()[layout].id;
                log!("screen texture loaded: {id} ({} in use)", state.screens.len() + 1);
                state.screens.push(ScreenTexture { texture, height, version: 0, last_used: frame });
                state.screens.len() - 1
            }
        };
        let screen = &mut state.screens[index];
        screen.last_used = frame;

        let Ok(device) = screen.texture.GetDevice() else { return };
        if state.adopt_device(device.as_raw() as usize) && state.device != 0 {
            log!("screens: on device {:#x}", state.device);
        }
        let screen = &mut state.screens[index];
        let enabled = bridge::get().is_some_and(|b| b.host.screen_enabled.load(Ordering::Acquire) == 1);
        let latest = if enabled { state.frames.update(&device, context) } else { None };
        match latest {
            Some((latest, version)) => {
                if screen.version != version {
                    copy_frame(context, &screen.texture, screen.height, &latest);
                    screen.version = version;
                }
            }
            None => {
                if screen.version != TRANSPARENT {
                    if state.transparent.is_none() {
                        state.transparent = create_transparent(&device);
                    }
                    if let Some(transparent) = &state.transparent {
                        let all = D3D11_BOX {
                            left: 0,
                            top: 0,
                            front: 0,
                            right: SCREEN_WIDTH,
                            bottom: screen.height,
                            back: 1,
                        };
                        context.CopySubresourceRegion(&screen.texture, 0, 0, 0, 0, transparent, 0, Some(&all));
                        screen.version = TRANSPARENT;
                    }
                }
            }
        }

        if let Some(bridge) = bridge::get() {
            let plugin = &bridge.plugin;
            let published = layout as u32 + 1;
            if plugin.screen_layout.swap(published, Ordering::AcqRel) != published {
                log!("screen layout: {}", screen_layouts()[layout].id);
            }
            plugin.screen_frame.store(plugin.heartbeat.load(Ordering::Relaxed), Ordering::Release);
        }
    }
}

/// Copies an app frame ([`SCREEN_WIDTH`] x [`SCREEN_HEIGHT`]) into a screen texture of the given height,
/// repeating the frame's bottom row in the texture's extra rows.
///
/// # Safety
/// As [`refresh`]: live textures of the game's device, `height` the screen texture's.
unsafe fn copy_frame(context: &ID3D11DeviceContext, screen: &ID3D11Texture2D, height: u32, frame: &ID3D11Texture2D) {
    // SAFETY: the game's live context and textures (the caller's contract); the frame is the screen's width,
    // and rows past it repeat its last row, within the screen's height.
    unsafe {
        context.CopySubresourceRegion(screen, 0, 0, 0, 0, frame, 0, None);
        let bottom_row = D3D11_BOX {
            left: 0,
            top: SCREEN_HEIGHT - 1,
            front: 0,
            right: SCREEN_WIDTH,
            bottom: SCREEN_HEIGHT,
            back: 1,
        };
        for row in SCREEN_HEIGHT..height {
            context.CopySubresourceRegion(screen, 0, 0, row, 0, frame, 0, Some(&bottom_row));
        }
    }
}

/// # Safety
/// The game's live device, on its render thread.
unsafe fn create_transparent(device: &ID3D11Device) -> Option<ID3D11Texture2D> {
    // SAFETY: the game's live device (the caller's contract); `zeros` covers the whole texture and lives
    // through the call.
    unsafe {
        let height = ScreenLayout::texture_height(screen_layouts().len() - 1);
        let desc = D3D11_TEXTURE2D_DESC {
            Width: SCREEN_WIDTH,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_TYPELESS,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let zeros = vec![0u8; (SCREEN_WIDTH * height * 4) as usize];
        let initial = D3D11_SUBRESOURCE_DATA {
            pSysMem: zeros.as_ptr() as *const c_void,
            SysMemPitch: SCREEN_WIDTH * 4,
            SysMemSlicePitch: 0,
        };
        let mut texture = None;
        match device.CreateTexture2D(&desc, Some(&initial), Some(&mut texture)) {
            Ok(()) => texture,
            Err(e) => {
                log!("cannot create transparent screen texture: {e}");
                None
            }
        }
    }
}

/// If this is a WestPlay screen texture as the game loads it from the mod: the index of its layout.
fn screen_layout(desc: &D3D11_TEXTURE2D_DESC) -> Option<usize> {
    if desc.MipLevels != 1 || desc.ArraySize != 1 || desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM_SRGB {
        return None;
    }
    screen_layout_for_texture(desc.Width, desc.Height)
}

/// Screen UI targets are small single-sample RGBA8 render targets that can also be sampled.
fn is_screen_target(desc: &D3D11_TEXTURE2D_DESC) -> bool {
    let rt_and_srv = (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32;
    desc.BindFlags & rt_and_srv == rt_and_srv
        && desc.ArraySize == 1
        && desc.SampleDesc.Count == 1
        && desc.Width <= 2048
        && desc.Height <= 2048
        && (desc.Format == DXGI_FORMAT_R8G8B8A8_UNORM_SRGB || desc.Format == DXGI_FORMAT_R8G8B8A8_UNORM)
}

/// # Safety
/// As [`on_set_render_targets`].
unsafe fn first_target(count: u32, views: *const *mut c_void) -> Option<D3D11_TEXTURE2D_DESC> {
    // SAFETY: the game's arguments (the caller's contract): at least one view when `count` is set (checked).
    unsafe {
        if views.is_null() || count == 0 || count > 8 {
            return None;
        }
        let view = ID3D11RenderTargetView::from_raw_borrowed(&*views)?;
        let texture = view.GetResource().ok()?.cast::<ID3D11Texture2D>().ok()?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc);
        Some(desc)
    }
}

/// Called once per presented game frame.
pub fn on_frame(frame: u64) {
    FRAME.store(frame, Ordering::Relaxed);
    if !frame.is_multiple_of(300) {
        return;
    }
    let mut state = state();
    let before = state.0.screens.len();
    state.0.screens.retain(|s| frame.saturating_sub(s.last_used) < STALE_AFTER_FRAMES);
    if state.0.screens.len() != before {
        log!("released {} unused screen textures, {} in use", before - state.0.screens.len(), state.0.screens.len());
    }
}

pub fn log_stats() {
    log!("screen textures in use: {}", state().0.screens.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lets_the_old_devices_textures_go_when_the_game_has_a_new_device() {
        let mut state = State::default();
        assert!(state.adopt_device(0x1000));
        // The same device: kept.
        assert!(!state.adopt_device(0x1000));
        // A new one: everything made on the old one goes.
        state.frames = FrameSource::default();
        assert!(state.adopt_device(0x2000));
        assert_eq!(state.device, 0x2000);
        assert!(state.transparent.is_none());
    }
}
