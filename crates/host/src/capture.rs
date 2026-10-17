//! Captures the offscreen WebView2 visual with Windows Graphics Capture and publishes frames into the
//! shared texture, at most [`MAX_FPS`] per second. WGC only delivers frames when the content changes, so
//! an idle UI costs nothing; and so a frame that arrives before it's due is kept, not dropped, and goes out
//! when it is: dropped, it might be the last of an animation or a cursor move, and nothing would come to
//! replace it.
//!
//! Threads: WGC delivers frames on its own threads, which only leave the newest in a [`Mailbox`] (a swap,
//! no GPU work); the host's loop publishes it when due ([`Capture::flush`]), the only place frames are
//! copied. The mailbox's lock is never held across GPU work: WGC may call in while holding the device's
//! lock, and a lock held across a copy (which takes the device's lock) would deadlock against it.

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
use windows::Win32::Graphics::Direct3D11::{D3D11_BOX, ID3D11DeviceContext, ID3D11Texture2D};
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::core::{IInspectable, Interface, Result};

use crate::gpu::{Gpu, Lock, SharedFrame};

/// Frames per second published to the game. Chromium renders at the monitor's refresh rate.
pub const MAX_FPS: u32 = 60;
const FRAME_INTERVAL: Duration = Duration::from_micros(1_000_000 / MAX_FPS as u64);
/// Tolerance for capture timing jitter.
const FRAME_JITTER: Duration = Duration::from_micros(1_500);
/// Frames in the capture pool: one kept in the mailbox, one being published, one WGC draws into.
const POOL_FRAMES: i32 = 3;

pub struct Capture {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    mailbox: Arc<Mailbox<Direct3D11CaptureFrame>>,
    context: ID3D11DeviceContext,
    shared: Arc<SharedFrame>,
    bridge: Arc<BridgeMapping>,
    pub frames: Arc<AtomicU64>,
    /// Times a frame couldn't go out because the game was copying the one before (it's tried again).
    pub dropped: Arc<AtomicU64>,
    /// Whether the shared texture's lock was abandoned (the game died holding it): the pipeline must be
    /// built anew.
    abandoned: AtomicBool,
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
        pool.FrameArrived(&TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new({
            let mailbox = mailbox.clone();
            move |pool, _| {
                let Some(pool) = pool.as_ref() else { return Ok(()) };
                let frame = pool.TryGetNextFrame()?;
                // The frame it replaces goes back to the pool here, after the mailbox is let go.
                drop(mailbox.put(frame));
                Ok(())
            }
        }))?;
        session.StartCapture()?;
        Ok(Self {
            pool,
            session,
            mailbox,
            context: gpu.context.clone(),
            shared,
            bridge,
            frames: Arc::new(AtomicU64::new(0)),
            dropped: Arc::new(AtomicU64::new(0)),
            abandoned: AtomicBool::new(false),
        })
    }

    /// Whether the shared texture can't be used any more (its lock abandoned): build the pipeline anew.
    pub fn broken(&self) -> bool {
        self.abandoned.load(Ordering::Relaxed)
    }

    /// Publishes the newest frame if it's waiting and due (called every turn of the host's loop).
    pub fn flush(&self) {
        if self.broken() {
            return;
        }
        let sent = flush(&self.mailbox, Instant::now(), |frame| {
            let texture: Result<ID3D11Texture2D> = frame
                .Surface()
                // SAFETY: the frame's surface is a Direct3D surface (the capture's own frames), alive through the call.
                .and_then(|surface| unsafe { surface.cast::<IDirect3DDxgiInterfaceAccess>()?.GetInterface() });
            // A frame whose texture can't be had is done with all the same.
            let Ok(texture) = texture else { return true };
            match publish(&self.context, &texture, &self.shared) {
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
            Some(true) => {
                self.bridge.bridge().host.frame_seq.fetch_add(1, Ordering::Release);
                self.frames.fetch_add(1, Ordering::Relaxed);
            }
            Some(false) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            None => {}
        }
    }
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

/// Copies a frame into the shared texture, if its lock could be had ([`Lock::Held`]: copied).
fn publish(context: &ID3D11DeviceContext, frame: &ID3D11Texture2D, shared: &SharedFrame) -> Lock {
    let lock = shared.try_lock();
    if lock != Lock::Held {
        return lock;
    }
    // SAFETY: the frame and the shared texture are alive, and the shared texture is locked (above); the region
    // fits both.
    unsafe {
        let mut desc = Default::default();
        frame.GetDesc(&mut desc);
        let region = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: desc.Width.min(shared.width),
            bottom: desc.Height.min(shared.height),
            back: 1,
        };
        context.CopySubresourceRegion(&shared.texture, 0, 0, 0, 0, frame, 0, Some(&region));
    }
    shared.unlock();
    Lock::Held
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
