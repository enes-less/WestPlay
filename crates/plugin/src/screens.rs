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
//! to draw a screen, we copy the newest app frame into it first.
//!
//! WestPlay's tablet (`westplay_shared::TABLET`) is a model whose screen shows its layout's texture itself, drawn
//! in the game's 3D pass rather than into a screen's render target. Every screen texture is also known by the view
//! the game makes of it ([`on_view_created`]), and found among the views the game binds by that view's address
//! alone, whatever the target: the tablet's only so, a truck screen's too. When WestPlay is switched off, the app is
//! not running, or the truck isn't one WestPlay supports on this screen (most trucks share the stock one),
//! it is made fully transparent, so the game's original screen shows through.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

use westplay_shared::{
    CAB_MARK_HEIGHT, CAB_MARK_WIDTH, SCREEN_HEIGHT, SCREEN_WIDTH, ScreenLayout, TABLET, TRUCK_SUPPORTED,
    TRUCK_UNSUPPORTED, screen_layout_for_texture, screen_layouts, screen_supported,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_IMMUTABLE, ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView, ID3D11Resource,
    ID3D11ShaderResourceView, ID3D11Texture2D,
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
/// Whether a screen the game draws is WestPlay's in the truck the player is in (`screen_supported`): the stock
/// GPS screen is shared by trucks WestPlay doesn't support. False until a screen is drawn.
static ON_SUPPORTED_TRUCK: AtomicBool = AtomicBool::new(false);
/// The frame a screen of WestPlay's in this truck was last drawn in (0: none since the truck changed). The
/// tablet is drawn beside the truck's own screen, which isn't WestPlay's there: the truck is WestPlay's while
/// either was drawn lately, not by whichever was drawn last.
static SUPPORTED_DRAWN: AtomicU64 = AtomicU64::new(0);
/// Whether the log has said this truck's screen isn't WestPlay's (once a truck: `truck_changed`).
static UNSUPPORTED_SAID: AtomicBool = AtomicBool::new(false);
/// Whether the log has said the cab's mark is drawn in this truck (once a truck: `truck_changed`): so whether the
/// camera system goes by it here, not by the tablet (which, a hookup on a dashboard set's toy spot, is drawn from
/// outside the cab too).
static CAB_MARK_SAID: AtomicBool = AtomicBool::new(false);
/// Whether the tablet stays blank in this truck (`westplay_shared::TABLET_APART_FILE`: owner 2026-10-26, a hookup any
/// vehicle's set takes, kept from showing in those it mustn't), as the truck changes.
static TABLET_BLANK: AtomicBool = AtomicBool::new(false);
/// Whether the truck's own screen (not the tablet) has shown WestPlay since the truck changed: the tablet then stays
/// blank in it (owner 2026-10-27). Both showing, the page was laid out for one and then the other every frame (a
/// player's Scania S, its own screen and the tablet: 85 times a second, the page slowing and freezing).
static OWN_SCREEN_SEEN: AtomicBool = AtomicBool::new(false);
/// The frame any screen of WestPlay's layouts (WestPlay's or not) was last drawn in, in this truck (0: none since the
/// truck changed).
static ANY_SCREEN_DRAWN: AtomicU64 = AtomicU64::new(0);
/// The frame the cab began being drawn with the electrics on and no such screen (0: it isn't).
static CAB_WITHOUT_SCREEN: AtomicU64 = AtomicU64::new(0);
/// How long the cab is drawn with the electrics on and no screen of WestPlay's layouts before its truck counts as
/// not WestPlay's (frames: a screen starts with the electrics in a frame or two).
const NO_SCREEN_FRAMES: u64 = 120;
/// How lately (frames) a screen of WestPlay's was drawn for the truck to still be WestPlay's when another is.
const SUPPORTED_LATELY: u64 = 30;

/// The screen textures' views the game made (the tablet's, and the trucks' screens'), at most this many at once (one
/// a texture is usual: the game makes a texture's view once).
const KEPT_VIEWS: usize = 8;
/// Their addresses (0: a free slot), compared with each view the game binds: a frame's thousands of binds call
/// nothing of theirs.
static VIEW_ADDRESSES: [AtomicUsize; KEPT_VIEWS] = [const { AtomicUsize::new(0) }; KEPT_VIEWS];

/// A view of a screen texture, held (a reference of ours: its address can't be another's while kept), its texture
/// and its layout.
struct KeptView {
    view: ID3D11ShaderResourceView,
    texture: ID3D11Texture2D,
    height: u32,
    layout: usize,
}

struct KeptViews(Vec<KeptView>);
// SAFETY: as `Shared`: used on the game's render thread, through the mutex.
unsafe impl Send for KeptViews {}

static KEPT: LazyLock<Mutex<KeptViews>> = LazyLock::new(|| Mutex::new(KeptViews(Vec::new())));

fn kept_views() -> std::sync::MutexGuard<'static, KeptViews> {
    KEPT.lock().unwrap_or_else(|e| e.into_inner())
}

/// The render target last set ([`target_said`]: width, height and format; 0: none), for the log.
static CURRENT_TARGET: AtomicU64 = AtomicU64::new(0);
/// Whether the log has said a truck's screen was drawn into a target WestPlay doesn't look in (once a truck:
/// `truck_changed`).
static OTHER_TARGET_SAID: AtomicBool = AtomicBool::new(false);

/// A render target's width, height and format in one number, for [`CURRENT_TARGET`].
fn target_said(desc: &D3D11_TEXTURE2D_DESC) -> u64 {
    (desc.Width as u64 & 0xffff) << 32 | (desc.Height as u64 & 0xffff) << 16 | (desc.Format.0 as u64 & 0xffff)
}

/// Whether the truck's screen is one WestPlay shows on (see [`ON_SUPPORTED_TRUCK`]).
pub fn on_supported_truck() -> bool {
    ON_SUPPORTED_TRUCK.load(Ordering::Acquire)
}

/// The player is in another truck (or the game told its truck again): whether WestPlay is on its screen is
/// known again only once the game draws a screen of WestPlay's in it. Until then, holding the interaction
/// key takes nothing from the game (a player's log, 2026-10-21: in a Mercedes Actros after a DAF 2021, the
/// key still took the mouse).
pub fn truck_changed(id: &str) {
    // The vehicles the tablet stays blank in, as setup last wrote them beside the mod (none: none). A truck with
    // no id (the game says one now and then while the player stays in theirs, as under TruckersMP, 2026-10-26)
    // leaves it as it was.
    if !id.is_empty() {
        let path = crate::log::log_dir().parent().map(|dir| dir.join(westplay_shared::TABLET_APART_FILE));
        let marks =
            path.and_then(|path| std::fs::read_to_string(path).ok()).map(|text| westplay_shared::read_marks(&text));
        let blank = marks.is_some_and(|marks| westplay_shared::vehicle_marked(id, &marks));
        if blank && !TABLET_BLANK.load(Ordering::Acquire) {
            log!("tablet: kept blank in this truck");
        }
        TABLET_BLANK.store(blank, Ordering::Release);
        // Another truck: its own screen not seen showing WestPlay yet.
        OWN_SCREEN_SEEN.store(false, Ordering::Release);
    }
    SUPPORTED_DRAWN.store(0, Ordering::Release);
    UNSUPPORTED_SAID.store(false, Ordering::Release);
    CAB_MARK_SAID.store(false, Ordering::Release);
    OTHER_TARGET_SAID.store(false, Ordering::Release);
    ANY_SCREEN_DRAWN.store(0, Ordering::Release);
    CAB_WITHOUT_SCREEN.store(0, Ordering::Release);
    if let Some(bridge) = bridge::get() {
        bridge.plugin.truck_supported.store(0, Ordering::Release);
    }
    if ON_SUPPORTED_TRUCK.swap(false, Ordering::AcqRel) {
        log!("screen: another truck, WestPlay's screen not seen in it yet");
    }
}

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
        let target = first_target(count, views);
        let candidate = target.as_ref().is_some_and(is_screen_target);
        CURRENT_IS_CANDIDATE.store(candidate, Ordering::Relaxed);
        CURRENT_TARGET.store(target.as_ref().map_or(0, target_said), Ordering::Relaxed);
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
        if views.is_null() || count == 0 || count > 128 {
            return;
        }
        let views = std::slice::from_raw_parts(views, count as usize);
        if VIEW_ADDRESSES.iter().any(|address| address.load(Ordering::Relaxed) != 0) {
            refresh_kept(context, views);
        }
        if !CURRENT_IS_CANDIDATE.load(Ordering::Relaxed) {
            return;
        }
        for view in views {
            // A kept view's texture was refreshed by its address, whatever the target.
            if is_kept(*view) {
                continue;
            }
            let Some(view) = ID3D11ShaderResourceView::from_raw_borrowed(view) else { continue };
            let Ok(resource) = view.GetResource() else { continue };
            let Ok(texture) = resource.cast::<ID3D11Texture2D>() else { continue };
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            if is_cab_mark(&desc) {
                cab_drawn();
                continue;
            }
            if let Some(layout) = screen_layout(&desc)
                && let Some(context) = ID3D11DeviceContext::from_raw_borrowed(&context)
            {
                refresh(context, texture, desc.Height, layout);
            }
        }
    }
}

/// Whether a screen shows WestPlay itself: a layout WestPlay's in the truck (`supported`); the tablet not where it's
/// kept blank (`blank`: the vehicles set apart), nor once the truck's own screen has shown WestPlay (`own_seen`).
fn shows_here(tablet: bool, supported: bool, blank: bool, own_seen: bool) -> bool {
    supported && !(tablet && (blank || own_seen))
}

/// Whether a screen of WestPlay's, last drawn in frame `drawn` (0: none since the truck changed), was drawn
/// lately at `frame`.
fn drawn_lately(drawn: u64, frame: u64) -> bool {
    drawn != 0 && frame.saturating_sub(drawn) <= SUPPORTED_LATELY
}

/// The game made a view (`view`) of `resource`: kept if it's a screen texture (the tablet's or a truck screen's),
/// to be known by its address wherever the game binds it. The tablet is drawn in the 3D pass, never into a screen
/// target. A truck's screen is drawn into one, but not always one [`is_screen_target`] takes, it seems: a player's
/// log (2026-10-27, Scania S under TruckersMP, a 1920 x 1080 game with 2880 x 2160 targets of its own) had it go
/// unrefreshed for 14 minutes after the game ran again from its menu, frozen on its last picture while the
/// dashboard's mark beside it was seen all along; [`refresh_kept`] says the target if so.
///
/// # Safety
/// From the `CreateShaderResourceView` hook, with the resource the game gave and the live view it just made.
pub unsafe fn on_view_created(resource: *mut c_void, view: *mut c_void) {
    // SAFETY: the game's own resource and the view the device just made of it (the caller's contract), both live
    // in this call; the view kept is a reference of ours.
    unsafe {
        let Some(resource) = ID3D11Resource::from_raw_borrowed(&resource) else { return };
        let Ok(texture) = resource.cast::<ID3D11Texture2D>() else { return };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc);
        let Some(layout) = screen_layout(&desc) else { return };
        let Some(view) = ID3D11ShaderResourceView::from_raw_borrowed(&view) else { return };
        let id = screen_layouts()[layout].id;
        let mut kept = kept_views();
        let Some(slot) = VIEW_ADDRESSES.iter().find(|address| address.load(Ordering::Acquire) == 0) else {
            log!("screen: more than {KEPT_VIEWS} views of screen textures at once, {id}'s not kept");
            return;
        };
        kept.0.push(KeptView { view: view.clone(), texture, height: desc.Height, layout });
        slot.store(view.as_raw() as usize, Ordering::Release);
        if id == TABLET {
            log!("tablet: its screen texture's view made ({} kept)", kept.0.len());
        } else {
            log!("screen: {id}'s texture view made ({} kept)", kept.0.len());
        }
    }
}

/// Whether `view` (an address the game binds) is one of the kept views.
fn is_kept(view: *mut c_void) -> bool {
    !view.is_null() && VIEW_ADDRESSES.iter().any(|address| address.load(Ordering::Relaxed) == view as usize)
}

/// Each screen texture refreshed whose kept view is one of `views` (the game's, about to be bound).
///
/// # Safety
/// As [`on_set_shader_resources`]: the game's live context, and its views' addresses.
unsafe fn refresh_kept(context: *mut c_void, views: &[*mut c_void]) {
    // SAFETY: the views are only compared by address; the textures refreshed are ours (held), on the game's live
    // context (the caller's contract).
    unsafe {
        for &bound in views.iter().filter(|view| is_kept(**view)) {
            let found = kept_views()
                .0
                .iter()
                .find(|kept| kept.view.as_raw() == bound)
                .map(|kept| (kept.texture.clone(), kept.height, kept.layout));
            let Some((texture, height, layout)) = found else { continue };
            // A truck's screen drawn into a target WestPlay didn't look in before: said once a truck, with the target.
            if layout != westplay_shared::tablet_layout()
                && !CURRENT_IS_CANDIDATE.load(Ordering::Relaxed)
                && !OTHER_TARGET_SAID.swap(true, Ordering::AcqRel)
            {
                let target = CURRENT_TARGET.load(Ordering::Relaxed);
                log!(
                    "screen: {} drawn into a {}x{} target (format {}), not a screen target's: refreshed by its view",
                    screen_layouts()[layout].id,
                    target >> 32,
                    (target >> 16) & 0xffff,
                    target & 0xffff
                );
            }
            let Some(context) = ID3D11DeviceContext::from_raw_borrowed(&context) else { return };
            refresh(context, texture, height, layout);
        }
    }
}

/// Lets go of the kept views the game holds no more (only ours left), their slots free again.
fn release_views() {
    let mut kept = kept_views();
    kept.0.retain(|kept| {
        // SAFETY: our own reference to the view keeps it live: its count read by taking one more and giving it
        // back (IUnknown's first slots, as every COM object's).
        let others = unsafe {
            let raw = kept.view.as_raw();
            let vtable = *(raw as *const *const windows::core::IUnknown_Vtbl);
            ((*vtable).AddRef)(raw);
            ((*vtable).Release)(raw) > 1
        };
        if !others {
            let address = kept.view.as_raw() as usize;
            for slot in &VIEW_ADDRESSES {
                let _ = slot.compare_exchange(address, 0, Ordering::AcqRel, Ordering::Acquire);
            }
            let id = screen_layouts()[kept.layout].id;
            if id == TABLET {
                log!("tablet: a view of its screen let go");
            } else {
                log!("screen: a view of {id}'s texture let go");
            }
        }
        others
    });
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
        // A truck's id can't be read while the telemetry thread writes it: the last answer stands meanwhile.
        let truck = bridge::get().and_then(|b| b.plugin.truck.read());
        let tablet = layout == westplay_shared::tablet_layout();
        let supported =
            truck.as_ref().map_or_else(on_supported_truck, |truck| screen_supported(layout, truck.id.as_str()));
        if supported && !tablet && !OWN_SCREEN_SEEN.swap(true, Ordering::AcqRel) {
            log!("screen: {} shows WestPlay in this truck: the tablet kept blank in it", screen_layouts()[layout].id);
        }
        let here = shows_here(
            tablet,
            supported,
            TABLET_BLANK.load(Ordering::Acquire),
            OWN_SCREEN_SEEN.load(Ordering::Acquire),
        );
        let lately = drawn_lately(SUPPORTED_DRAWN.load(Ordering::Acquire), frame);
        if here {
            SUPPORTED_DRAWN.store(frame.max(1), Ordering::Release);
        }
        // WestPlay's truck while one of its screens was drawn lately (the tablet, beside the truck's own).
        let supported = here || lately;
        let was = ON_SUPPORTED_TRUCK.swap(supported, Ordering::AcqRel);
        ANY_SCREEN_DRAWN.store(frame.max(1), Ordering::Release);
        // Said for the app (the companion says a truck isn't WestPlay's): WestPlay's once a screen of WestPlay's has
        // been drawn in it at all (the tablet, left out as the player looks away from it, beside a truck's own screen
        // that isn't WestPlay's), until the truck changes.
        if let Some(bridge) = bridge::get() {
            let ever = SUPPORTED_DRAWN.load(Ordering::Acquire) != 0;
            let said = if here || ever { TRUCK_SUPPORTED } else { TRUCK_UNSUPPORTED };
            bridge.plugin.truck_supported.store(said, Ordering::Release);
        }
        // Said once a truck (or as it stops being WestPlay's): a truck first seen unsupported was said nothing
        // (a player's Actros MP4, 2026-10-24: the log never said why WestPlay didn't show).
        if !supported && (was || !UNSUPPORTED_SAID.swap(true, Ordering::AcqRel)) {
            let truck = truck.map(|truck| truck.id.as_str().to_owned()).unwrap_or_default();
            log!("screen: {} isn't WestPlay's in {truck}, the game's own shows", screen_layouts()[layout].id);
        }
        // This screen WestPlay's itself: the truck's own, beside the tablet, keeps the game's.
        let enabled = here && bridge::get().is_some_and(|b| b.host.screen_enabled.load(Ordering::Acquire) == 1);
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

        // The screen WestPlay is on, said for the app (not the truck's own beside the tablet).
        if let Some(bridge) = bridge::get()
            && (here || !lately)
        {
            let plugin = &bridge.plugin;
            let published = layout as u32 + 1;
            if plugin.screen_layout.swap(published, Ordering::AcqRel) != published {
                log!("screen layout: {}", screen_layouts()[layout].id);
            }
            // The cab drawn, for the camera system: not by the tablet in a truck whose cab has its mark (the tablet
            // is drawn from outside the cab too, as a hookup on a set's toy spot).
            if !(tablet && CAB_MARK_SAID.load(Ordering::Acquire)) {
                plugin.screen_frame.store(plugin.heartbeat.load(Ordering::Relaxed), Ordering::Release);
            }
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
        // As tall as the tallest screen texture (the last layout's), so it covers any of them.
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

/// Whether a texture is the cab's mark (`westplay_shared::CAB_MARK_WIDTH`): in the mod's file format, at its size.
fn is_cab_mark(desc: &D3D11_TEXTURE2D_DESC) -> bool {
    desc.MipLevels == 1
        && desc.ArraySize == 1
        && desc.Format == DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
        && desc.Width == CAB_MARK_WIDTH
        && desc.Height == CAB_MARK_HEIGHT
}

/// The cab's mark drawn into a dashboard's screen: the camera in the cab, this frame, said for the app.
fn cab_drawn() {
    let Some(bridge) = bridge::get() else { return };
    let plugin = &bridge.plugin;
    plugin.cab_frame.store(plugin.heartbeat.load(Ordering::Relaxed), Ordering::Release);
    if !CAB_MARK_SAID.swap(true, Ordering::AcqRel) {
        log!("cab: the dashboard's mark drawn: the camera in the cab is told by it");
    }
    // A truck with no screen of WestPlay's layouts at all (a mod truck's own GPS screen): its cab drawn a while with
    // the electrics on (with them off, no screen is drawn in any truck) and no such screen, not WestPlay's.
    let frame = FRAME.load(Ordering::Relaxed);
    let electrics = plugin.telemetry.read().is_some_and(|telemetry| telemetry.electric_enabled);
    // Never once a screen of WestPlay's has been drawn in this truck (its tablet, looked away from).
    if !electrics
        || SUPPORTED_DRAWN.load(Ordering::Acquire) != 0
        || drawn_lately(ANY_SCREEN_DRAWN.load(Ordering::Acquire), frame)
    {
        CAB_WITHOUT_SCREEN.store(0, Ordering::Release);
        return;
    }
    let since = CAB_WITHOUT_SCREEN.load(Ordering::Acquire);
    if since == 0 {
        CAB_WITHOUT_SCREEN.store(frame.max(1), Ordering::Release);
    } else if frame.saturating_sub(since) > NO_SCREEN_FRAMES
        && plugin.truck_supported.swap(TRUCK_UNSUPPORTED, Ordering::AcqRel) != TRUCK_UNSUPPORTED
    {
        log!("screen: no screen of WestPlay's layouts in this truck: not WestPlay's");
    }
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
    release_views();
    let mut state = state();
    let before = state.0.screens.len();
    state.0.screens.retain(|s| frame.saturating_sub(s.last_used) < STALE_AFTER_FRAMES);
    if state.0.screens.len() != before {
        log!("released {} unused screen textures, {} in use", before - state.0.screens.len(), state.0.screens.len());
        // None drawn for a while: not on WestPlay's screen any more, whatever truck this is.
        if state.0.screens.is_empty() {
            ON_SUPPORTED_TRUCK.store(false, Ordering::Release);
        }
    }
}

pub fn log_stats() {
    log!("screen textures in use: {}", state().0.screens.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_truck_is_not_on_westplays_screen_until_one_is_drawn_in_it() {
        ON_SUPPORTED_TRUCK.store(true, Ordering::Release);
        truck_changed("scania.s_2016");
        assert!(!on_supported_truck());
        // Told again with nothing to clear: still not.
        truck_changed("scania.s_2016");
        assert!(!on_supported_truck());
    }

    #[test]
    fn the_tablet_keeps_the_truck_westplays_beside_its_own_screen() {
        // The tablet drawn in frame 100; the truck's own screen (not WestPlay's) in the same frame, the next, or
        // half a second on at 60 fps: the truck is still WestPlay's.
        for frame in [100, 101, 130] {
            assert!(drawn_lately(100, frame), "{frame}");
        }
        // No tablet for longer (the camera outside, the tablet taken off), or none since the truck changed.
        assert!(!drawn_lately(100, 131));
        assert!(!drawn_lately(0, 10));
        // Drawn after it in the same frame order: as lately as can be.
        assert!(drawn_lately(101, 100));
    }

    #[test]
    fn the_tablet_is_blank_once_the_trucks_own_screen_shows_westplay() {
        // A truck's own screen of WestPlay's shows it; one that isn't WestPlay's in the truck doesn't.
        assert!(shows_here(false, true, false, true));
        assert!(!shows_here(false, false, false, false));
        // The tablet shows it in a truck whose own screen hasn't (none of WestPlay's, or not seen yet)...
        assert!(shows_here(true, true, false, false));
        // ...but not once it has, nor in the vehicles it's kept blank in.
        assert!(!shows_here(true, true, false, true));
        assert!(!shows_here(true, true, true, false));
    }

    #[test]
    fn a_real_truck_change_forgets_the_own_screen_an_unnamed_one_doesnt() {
        OWN_SCREEN_SEEN.store(true, Ordering::Release);
        // TruckersMP's now-and-then truck with no id, the player still in theirs.
        truck_changed("");
        assert!(OWN_SCREEN_SEEN.load(Ordering::Acquire));
        truck_changed("scania.s_2016");
        assert!(!OWN_SCREEN_SEEN.load(Ordering::Acquire));
    }

    #[test]
    fn says_a_targets_size_and_format_in_one_number() {
        // The player's log's 2880 x 2160 sRGB target, read back as the log line does.
        let desc = D3D11_TEXTURE2D_DESC {
            Width: 2880,
            Height: 2160,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            ..Default::default()
        };
        let said = target_said(&desc);
        assert_eq!((said >> 32, (said >> 16) & 0xffff, said & 0xffff), (2880, 2160, 29));
        assert!(!is_screen_target(&desc));
    }

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
