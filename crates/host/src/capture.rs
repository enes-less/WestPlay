//! Captures the offscreen WebView2 visual with Windows Graphics Capture and publishes frames into the
//! shared texture, at most [`MAX_FPS`] per second. WGC only delivers frames when the content changes, so
//! an idle UI costs nothing; and so a frame that arrives before it's due is kept, not dropped, and goes out
//! when it is: dropped, it might be the last of an animation or a cursor move, and nothing would come to
//! replace it.
//!
//! The screen's cursor is drawn here, over each frame as it goes out (`cursor.rs`): the page's frame is kept
//! as captured, and as the cursor moves it goes out again from there with the cursor where it is now, the page
//! and Windows not waited for.
//!
//! Threads: WGC delivers frames on its own threads, which only leave the newest in a [`Mailbox`] (a swap,
//! no GPU work); the host's loop publishes it when due ([`Capture::flush`]), the only place frames are
//! copied. The mailbox's lock is never held across GPU work: WGC may call in while holding the device's
//! lock, and a lock held across a copy (which takes the device's lock) would deadlock against it.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use westplay_shared::BridgeMapping;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::UI::Composition::Visual;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_MAP_FLAG_DO_NOT_WAIT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING, ID3D11Device,
    ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::core::{IInspectable, Interface, Result};

use crate::brightness::{self, Dimmer};
use crate::cursor::{Look, Painter};
use crate::gpu::{Gpu, Lock, SharedFrame};

/// Frames per second published to the game. Chromium renders at the monitor's refresh rate.
pub const MAX_FPS: u32 = 60;
const FRAME_INTERVAL: Duration = Duration::from_micros(1_000_000 / MAX_FPS as u64);
/// Tolerance for capture timing jitter.
const FRAME_JITTER: Duration = Duration::from_micros(1_500);
/// Frames in the capture pool: one kept in the mailbox, one being published, one WGC draws into.
const POOL_FRAMES: i32 = 3;
/// How soon the cursor's frame may go out again after the last, as it moves or changes between the page's frames
/// (as often as a fast game takes frames, and its fades smoothly).
const CURSOR_INTERVAL: Duration = Duration::from_millis(6);

pub struct Capture {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    mailbox: Arc<Mailbox<Direct3D11CaptureFrame>>,
    context: ID3D11DeviceContext,
    shared: Arc<SharedFrame>,
    bridge: Arc<BridgeMapping>,
    pub frames: Arc<AtomicU64>,
    /// Frames Windows Graphics Capture delivered (each the page drawn anew; some replaced before they're due).
    pub arrived: Arc<AtomicU64>,
    /// Times a frame couldn't go out because the game was copying the one before (it's tried again).
    pub dropped: Arc<AtomicU64>,
    /// Whether the shared texture's lock was abandoned (the game died holding it): the pipeline must be
    /// built anew.
    abandoned: AtomicBool,
    /// The page's last frame as captured, without the cursor: what the cursor is drawn over, again as it moves.
    clean: ID3D11Texture2D,
    has_clean: Cell<bool>,
    /// Draws the cursor (`None`: Direct2D couldn't be had here, and the page draws its own).
    painter: Option<Painter>,
    /// The cursor as last drawn into the shared frame, and when; whether drawing it failed (said once).
    drawn: Cell<Option<Look>>,
    cursor_at: Cell<Instant>,
    draw_failed: Cell<bool>,
    /// What's under the cursor in the page's last frame, light or not (the cursor goes dark over light).
    under: Option<UnderCursor>,
    /// Copies the page's frame out with its light scaled (`None`: Direct2D couldn't be had here, and the frame goes
    /// out at full light).
    dimmer: Option<Dimmer>,
    /// The screen's light (0 to 1 of full: `brightness.rs`), and as the frame out was last made with it.
    light: Cell<f32>,
    drawn_light: Cell<f32>,
    dim_failed: Cell<bool>,
}

impl Capture {
    pub fn start(gpu: &Gpu, visual: &Visual, shared: Arc<SharedFrame>, bridge: Arc<BridgeMapping>) -> Result<Self> {
        let item = GraphicsCaptureItem::CreateFromVisual(visual)?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &gpu.winrt_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            POOL_FRAMES,
            item.Size()?,
        )?;
        let session = pool.CreateCaptureSession(&item)?;
        let _ = session.SetIsCursorCaptureEnabled(false);

        let mailbox = Arc::new(Mailbox::new(Instant::now()));
        let arrived = Arc::new(AtomicU64::new(0));
        pool.FrameArrived(&TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new({
            let mailbox = mailbox.clone();
            let arrived = arrived.clone();
            move |pool, _| {
                let Some(pool) = pool.as_ref() else { return Ok(()) };
                let frame = pool.TryGetNextFrame()?;
                arrived.fetch_add(1, Ordering::Relaxed);
                // The frame it replaces goes back to the pool here, after the mailbox is let go.
                drop(mailbox.put(frame));
                Ok(())
            }
        }))?;
        session.StartCapture()?;
        let clean = clean_texture(gpu, &shared)?;
        let dimmer = match Dimmer::new(gpu, &clean, &shared.texture) {
            Ok(dimmer) => Some(dimmer),
            Err(e) => {
                crate::log!("brightness: the screen stays at full light, Direct2D couldn't be had here: {e}");
                None
            }
        };
        let painter = match Painter::new(gpu, &shared.texture) {
            Ok(painter) => Some(painter),
            Err(e) => {
                crate::log!("cursor: drawn by the page, Direct2D couldn't be had here: {e}");
                None
            }
        };
        Ok(Self {
            pool,
            session,
            mailbox,
            context: gpu.context.clone(),
            shared,
            bridge,
            frames: Arc::new(AtomicU64::new(0)),
            arrived,
            dropped: Arc::new(AtomicU64::new(0)),
            abandoned: AtomicBool::new(false),
            clean,
            has_clean: Cell::new(false),
            painter,
            drawn: Cell::new(None),
            cursor_at: Cell::new(Instant::now()),
            draw_failed: Cell::new(false),
            under: UnderCursor::new(&gpu.device).ok(),
            dimmer,
            light: Cell::new(1.0),
            drawn_light: Cell::new(1.0),
            dim_failed: Cell::new(false),
        })
    }

    /// The screen's light from now on (0 to 1 of full: `brightness::light`); the last frame goes out again with it.
    pub fn set_light(&self, light: f32) {
        self.light.set(if light.is_finite() { light.clamp(brightness::LEAST_LIGHT, 1.0) } else { 1.0 });
    }

    /// Whether what's under the cursor is light, as the frame shows it (whatever draws it: the cab, a web page, a
    /// video, the game's own screen where the cab leaves it); `None` till it's been read.
    pub fn light_under(&self) -> Option<bool> {
        self.under.as_ref().and_then(UnderCursor::light)
    }

    /// Whether the cursor is drawn here (else the page draws its own).
    pub fn draws_cursor(&self) -> bool {
        self.painter.is_some()
    }

    /// Whether the shared texture can't be used any more (its lock abandoned): build the pipeline anew.
    pub fn broken(&self) -> bool {
        self.abandoned.load(Ordering::Relaxed)
    }

    /// Publishes the newest frame if it's waiting and due, with the `cursor` drawn over it where it is now (the
    /// page squeezed into the frame `squeeze` times its height); else, the cursor moved or changed, the page's last
    /// frame again with it (called every turn of the host's loop).
    pub fn flush(&self, cursor: Option<Look>, squeeze: f32) {
        if self.broken() {
            return;
        }
        let cursor = cursor.filter(|_| self.painter.is_some());
        let now = Instant::now();
        let sent = flush(&self.mailbox, now, |frame| {
            let texture: Result<ID3D11Texture2D> = frame
                .Surface()
                // SAFETY: the frame's surface is a Direct3D surface (the capture's own frames), alive through the call.
                .and_then(|surface| unsafe { surface.cast::<IDirect3DDxgiInterfaceAccess>()?.GetInterface() });
            // A frame whose texture can't be had is done with all the same.
            let Ok(texture) = texture else { return true };
            match self.publish(&texture, cursor, squeeze) {
                Lock::Held => true,
                Lock::Busy => false,
                // Done with: the pipeline is built anew, with frames to come.
                Lock::Abandoned => {
                    self.abandoned.store(true, Ordering::Relaxed);
                    true
                }
            }
        });
        match sent {
            Some(true) => self.sent(cursor, now),
            Some(false) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            None => {}
        }
        // What's under the cursor, read again now and then from the page's last frame.
        if let (Some(look), Some(under)) = (cursor, &self.under)
            && self.has_clean.get()
        {
            under.step(&self.context, &self.clean, look.x, look.y * squeeze, now);
        }
        // The cursor moved or changed, or the screen's light, and no new frame of the page's to take it: the last one
        // again, with it.
        if sent != Some(true)
            && self.has_clean.get()
            && (cursor != self.drawn.get() || self.light.get() != self.drawn_light.get())
            && now.saturating_duration_since(self.cursor_at.get()) >= CURSOR_INTERVAL
        {
            match self.compose(cursor, squeeze) {
                Lock::Held => self.sent(cursor, now),
                // Tried again next turn.
                Lock::Busy => {}
                Lock::Abandoned => self.abandoned.store(true, Ordering::Relaxed),
            }
        }
    }

    /// A frame went out, with `cursor` on it.
    fn sent(&self, cursor: Option<Look>, now: Instant) {
        self.bridge.bridge().host.frame_seq.fetch_add(1, Ordering::Release);
        self.frames.fetch_add(1, Ordering::Relaxed);
        self.drawn.set(cursor);
        self.drawn_light.set(self.light.get());
        self.cursor_at.set(now);
    }

    /// Keeps a frame of the page's as captured, and sends it out with the cursor on it.
    fn publish(&self, frame: &ID3D11Texture2D, cursor: Option<Look>, squeeze: f32) -> Lock {
        // SAFETY: the frame and the clean copy are alive, the clean copy the shared frame's size; the region fits
        // both.
        unsafe {
            let mut desc = Default::default();
            frame.GetDesc(&mut desc);
            let region = D3D11_BOX {
                left: 0,
                top: 0,
                front: 0,
                right: desc.Width.min(self.shared.width),
                bottom: desc.Height.min(self.shared.height),
                back: 1,
            };
            self.context.CopySubresourceRegion(&self.clean, 0, 0, 0, 0, frame, 0, Some(&region));
        }
        self.has_clean.set(true);
        self.compose(cursor, squeeze)
    }

    /// The page's last frame into the shared texture, the cursor drawn over it, if its lock could be had
    /// ([`Lock::Held`]: done).
    fn compose(&self, cursor: Option<Look>, squeeze: f32) -> Lock {
        let lock = self.shared.try_lock();
        if lock != Lock::Held {
            return lock;
        }
        // At less than full light, copied with its light scaled; at full light (or should that fail), as it is.
        let light = self.light.get();
        let dimmed = light < 1.0
            && self.dimmer.as_ref().is_some_and(|dimmer| match dimmer.draw(light) {
                Ok(()) => true,
                Err(e) => {
                    if !self.dim_failed.replace(true) {
                        crate::log!("brightness: couldn't be applied: {e}");
                    }
                    false
                }
            });
        if !dimmed {
            // SAFETY: both textures are alive and the same size and format; the shared one is locked (above).
            unsafe { self.context.CopyResource(&self.shared.texture, &self.clean) };
        }
        let light = if dimmed { light } else { 1.0 };
        if let (Some(look), Some(painter)) = (cursor, &self.painter)
            && let Err(e) = painter.draw(&look, squeeze, light)
            && !self.draw_failed.replace(true)
        {
            crate::log!("cursor: couldn't be drawn: {e}");
        }
        self.shared.unlock();
        Lock::Held
    }
}

/// The side of the patch under the cursor read from the frame (its pixels).
const UNDER_SIDE: u32 = 24;
/// How often the patch under the cursor is read again.
const UNDER_EVERY: Duration = Duration::from_millis(50);
/// How light what's under the cursor is (0 to 1) for it to go dark, and to be light again: apart, so it doesn't
/// flicker over something in between.
const LIGHT_ON: f32 = 0.6;
const LIGHT_OFF: f32 = 0.45;

/// What's under the cursor, light or dark, from the page's frames as captured (owner 2026-11-09: "it shouldn't care
/// what lies under it, app or not. if light cursor is not visible on whatever is in background, it will go dark.
/// vice versa"): a patch round it copied to a readable texture, read a turn or more later without waiting for the
/// GPU (so the frames never wait on it), then copied anew where the cursor is then.
struct UnderCursor {
    staging: ID3D11Texture2D,
    /// The copy waiting to be read, and its size.
    waiting: Cell<Option<(u32, u32)>>,
    at: Cell<Option<Instant>>,
    light: Cell<Option<bool>>,
}

impl UnderCursor {
    fn new(device: &ID3D11Device) -> Result<Self> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: UNDER_SIDE,
            Height: UNDER_SIDE,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut staging = None;
        // SAFETY: the host's live device; the description is a local.
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut staging))? };
        Ok(Self {
            staging: staging.ok_or_else(windows::core::Error::empty)?,
            waiting: Cell::new(None),
            at: Cell::new(None),
            light: Cell::new(None),
        })
    }

    fn light(&self) -> Option<bool> {
        self.light.get()
    }

    /// Reads the patch copied before, if the GPU is done with it; then, nothing left to read, copies the one round
    /// `(x, y)` (the frame's pixels) of `frame`. At most every [`UNDER_EVERY`].
    fn step(&self, context: &ID3D11DeviceContext, frame: &ID3D11Texture2D, x: f32, y: f32, now: Instant) {
        if self.at.get().is_some_and(|at| now.saturating_duration_since(at) < UNDER_EVERY) {
            return;
        }
        self.at.set(Some(now));
        if let Some((width, height)) = self.waiting.get() {
            // SAFETY: the readable texture is ours, mapped for reading and unmapped after; rows read within the
            // mapped pitch and the size copied into it.
            unsafe {
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                let read = context.Map(
                    &self.staging,
                    0,
                    D3D11_MAP_READ,
                    D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                    Some(&mut mapped),
                );
                // The GPU not done with it yet: read next turn.
                if read.is_err() {
                    return;
                }
                let mut total = 0.0f32;
                for row in 0..height as usize {
                    let line = std::slice::from_raw_parts(
                        (mapped.pData as *const u8).add(row * mapped.RowPitch as usize),
                        width as usize * 4,
                    );
                    total += line.chunks(4).map(luminance).sum::<f32>();
                }
                context.Unmap(&self.staging, 0);
                let light = total / (width * height).max(1) as f32;
                let was = self.light.get() == Some(true);
                self.light.set(Some(if was { light > LIGHT_OFF } else { light > LIGHT_ON }));
            }
        }
        // The patch round the cursor now, within the frame, handed to the GPU at once.
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: the frame is alive; the description is a local.
        unsafe { frame.GetDesc(&mut desc) };
        let half = UNDER_SIDE as f32 / 2.0;
        let left = (x - half).max(0.0).min(desc.Width.saturating_sub(UNDER_SIDE) as f32) as u32;
        let top = (y - half).max(0.0).min(desc.Height.saturating_sub(UNDER_SIDE) as f32) as u32;
        let (width, height) = (UNDER_SIDE.min(desc.Width), UNDER_SIDE.min(desc.Height));
        let region = D3D11_BOX { left, top, front: 0, right: left + width, bottom: top + height, back: 1 };
        // SAFETY: both textures are alive; the region lies within the frame and fits the readable texture, which isn't
        // mapped.
        unsafe {
            context.CopySubresourceRegion(&self.staging, 0, 0, 0, 0, frame, 0, Some(&region));
            context.Flush();
        }
        self.waiting.set(Some((width, height)));
    }
}

/// A frame's pixel's lightness (0 to 1; BGRA, its colour already times its cover: a see-through pixel as dark as
/// what shows through it there usually is, the truck's own screen).
fn luminance(pixel: &[u8]) -> f32 {
    (0.0722 * f32::from(pixel[0]) + 0.7152 * f32::from(pixel[1]) + 0.2126 * f32::from(pixel[2])) / 255.0
}

/// The texture the page's frames are kept in as captured: the shared frame's size and format, ours alone.
fn clean_texture(gpu: &Gpu, shared: &SharedFrame) -> Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: shared.width,
        Height: shared.height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut texture = None;
    // SAFETY: the host's live device; the description is a local.
    unsafe { gpu.device.CreateTexture2D(&desc, None, Some(&mut texture))? };
    texture.ok_or_else(windows::core::Error::empty)
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// The newest captured frame not yet published, and when the next may go out: on a fixed [`MAX_FPS`]
/// schedule (comparing against "time since the last frame" instead would halve a 75 Hz stream to 37.5
/// fps). Its lock is only ever held to swap a frame in or out, never across GPU work (see the module).
struct Mailbox<F> {
    slot: Mutex<Slot<F>>,
}

struct Slot<F> {
    frame: Option<F>,
    due: Instant,
}

impl<F> Mailbox<F> {
    fn new(now: Instant) -> Self {
        Self { slot: Mutex::new(Slot { frame: None, due: now }) }
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Slot<F>> {
        self.slot.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Leaves a new frame, returning the one it replaces (to let go of after the mailbox).
    fn put(&self, frame: F) -> Option<F> {
        self.slot().frame.replace(frame)
    }

    /// The waiting frame, if its turn has come.
    fn take_due(&self, now: Instant) -> Option<F> {
        let mut slot = self.slot();
        if now + FRAME_JITTER < slot.due {
            return None;
        }
        slot.frame.take()
    }

    /// The frame taken went out: the next is due an interval on, staying on schedule; or, after an idle
    /// period (a whole interval or more past its turn), an interval after this one, the schedule starting
    /// over from it rather than letting frames catch up.
    fn published(&self, now: Instant) {
        let mut slot = self.slot();
        slot.due = if now >= slot.due + FRAME_INTERVAL { now + FRAME_INTERVAL } else { slot.due + FRAME_INTERVAL };
    }

    /// The frame taken couldn't go out: it waits again, unless a newer one came meanwhile (then it's
    /// returned, to let go of after the mailbox).
    fn put_back(&self, frame: F) -> Option<F> {
        let mut slot = self.slot();
        if slot.frame.is_some() {
            return Some(frame);
        }
        slot.frame = Some(frame);
        None
    }
}

/// Publishes the mailbox's frame with `publish` if it's due (which says whether it went out): `None` if
/// none was due, else whether it went out. The mailbox is let go of before `publish` runs.
fn flush<F>(mailbox: &Mailbox<F>, now: Instant, publish: impl FnOnce(&F) -> bool) -> Option<bool> {
    let frame = mailbox.take_due(now)?;
    let sent = publish(&frame);
    if sent {
        mailbox.published(now);
        drop(frame);
    } else {
        drop(mailbox.put_back(frame));
    }
    Some(sent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_frame_arriving_early_waits_for_its_turn_and_still_goes_out() {
        let start = Instant::now();
        let mailbox = Mailbox::new(start);
        // The first frame goes out at once.
        mailbox.put(1);
        assert_eq!(flush(&mailbox, start, |_| true), Some(true));
        // A 144 Hz monitor's next frame, 7 ms on: too early, so it waits (the last of an animation, say)...
        mailbox.put(2);
        assert_eq!(flush(&mailbox, start + Duration::from_millis(7), |_| true), None);
        // ...and goes out when due, though no newer frame ever comes.
        let mut sent = None;
        assert_eq!(
            flush(&mailbox, start + FRAME_INTERVAL, |&frame| {
                sent = Some(frame);
                true
            }),
            Some(true)
        );
        assert_eq!(sent, Some(2));
        assert_eq!(flush(&mailbox, start + FRAME_INTERVAL * 3, |_| true), None);
    }

    #[test]
    fn a_frame_the_game_was_busy_for_is_tried_again_unless_a_newer_came() {
        let start = Instant::now();
        let mailbox = Mailbox::new(start);
        mailbox.put(1);
        assert_eq!(flush(&mailbox, start, |_| false), Some(false));
        // Still waiting, and goes out on the next try.
        assert_eq!(flush(&mailbox, start, |&frame| frame == 1), Some(true));
        // Busy again, but a newer frame arrived meanwhile: the newer one waits, the busy one is let go.
        mailbox.put(2);
        let later = start + FRAME_INTERVAL;
        assert_eq!(
            flush(&mailbox, later, |_| {
                mailbox.put(3);
                false
            }),
            Some(false)
        );
        assert_eq!(flush(&mailbox, later, |&frame| frame == 3), Some(true));
    }

    #[test]
    fn stays_on_schedule_without_a_backlog_after_idling() {
        let start = Instant::now();
        let mailbox = Mailbox::new(start);
        mailbox.put(0);
        flush(&mailbox, start, |_| true);
        // A 75 Hz stream: frames go out on the 60 Hz grid.
        let mut published = 0;
        for i in 1..=75u32 {
            let now = start + Duration::from_micros(1_000_000 / 75 * u64::from(i));
            mailbox.put(i);
            if flush(&mailbox, now, |_| true) == Some(true) {
                published += 1;
            }
        }
        assert!((58..=60).contains(&published), "{published} in a second");
        // Idle for a while: the next frame goes out at once, the one after a full interval later.
        let later = start + Duration::from_secs(3);
        mailbox.put(100);
        assert_eq!(flush(&mailbox, later, |_| true), Some(true));
        mailbox.put(101);
        assert_eq!(flush(&mailbox, later + FRAME_INTERVAL / 2, |_| true), None);
        assert_eq!(flush(&mailbox, later + FRAME_INTERVAL, |_| true), Some(true));
    }

    /// What's under the cursor read from a frame on this PC's GPU: dark over its dark half, light over its light half,
    /// back to dark again; without the frames waiting (read a turn after it's copied).
    #[test]
    fn reads_whether_whats_under_the_cursor_is_light() {
        use windows::Win32::Graphics::Direct3D11::D3D11_SUBRESOURCE_DATA;
        let gpu = Gpu::new(None).unwrap();
        let (width, height) = (400u32, 160u32);
        // The left half dark (20), the right light (235).
        let pixels: Vec<u8> = (0..height)
            .flat_map(|_| (0..width).flat_map(|x| if x < width / 2 { [20, 20, 20, 255] } else { [235, 235, 235, 255] }))
            .collect();
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let initial =
            D3D11_SUBRESOURCE_DATA { pSysMem: pixels.as_ptr().cast(), SysMemPitch: width * 4, SysMemSlicePitch: 0 };
        let mut frame = None;
        // SAFETY: plain Direct3D calls on the test's own device; the description and pixels are locals.
        unsafe { gpu.device.CreateTexture2D(&desc, Some(&initial), Some(&mut frame)).unwrap() };
        let frame = frame.unwrap();
        let under = UnderCursor::new(&gpu.device).unwrap();
        // A turn every UNDER_EVERY, as the host's, for half a second at each place.
        let over = |x: f32| {
            for _ in 0..10 {
                under.step(&gpu.context, &frame, x, 80.0, Instant::now());
                std::thread::sleep(UNDER_EVERY);
            }
            under.light()
        };
        assert_eq!(over(100.0), Some(false), "over the dark half");
        assert_eq!(over(300.0), Some(true), "over the light half");
        assert_eq!(over(60.0), Some(false), "dark again");
        // At the frame's very corner: the patch kept within it.
        assert_eq!(over(399.0), Some(true));
    }

    /// The deadlock this layout prevents (it hung the host once): WGC hands a frame over while holding the
    /// device's lock, while the host publishes, taking the device's lock for the copy. Here a mutex stands
    /// in for the device's lock: the "WGC" thread holds it as it hands frames over, the "host" thread takes
    /// it to publish. With the mailbox locked across the copy, the two wait for each other forever.
    #[test]
    fn handing_frames_over_under_the_device_lock_never_deadlocks_with_publishing() {
        let device = Arc::new(Mutex::new(()));
        let mailbox = Arc::new(Mailbox::new(Instant::now()));
        let (done, finished) = mpsc::channel();
        let capture = {
            let (device, mailbox, done) = (device.clone(), mailbox.clone(), done.clone());
            std::thread::spawn(move || {
                for frame in 0..20_000u32 {
                    let _device = device.lock().unwrap();
                    mailbox.put(frame);
                }
                done.send("capture").unwrap();
            })
        };
        let host = std::thread::spawn(move || {
            // Every frame is due: the clock runs a whole interval on each turn.
            let mut now = Instant::now();
            for _ in 0..20_000 {
                now += FRAME_INTERVAL;
                flush(&mailbox, now, |_| {
                    let _device = device.lock().unwrap();
                    true
                });
            }
            done.send("host").unwrap();
        });
        for _ in 0..2 {
            finished.recv_timeout(Duration::from_secs(20)).expect("capture and publishing deadlocked");
        }
        capture.join().unwrap();
        host.join().unwrap();
    }
}
