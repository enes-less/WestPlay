//! Frames from the WestPlay app.
//!
//! The app publishes frames in a shared texture (see `westplay-shared`). We open it by name, and copy each
//! new frame into a local texture under the texture's keyed mutex. The local copy is what the screens
//! use, so a frame we could not pick up in time (contention) simply means showing the previous one.

use std::sync::atomic::Ordering;

use westplay_shared::{FRAME_KEY, SCREEN_HEIGHT, SCREEN_WIDTH, frame_texture_name};
use windows::Win32::Foundation::E_INVALIDARG;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, ID3D11Device, ID3D11Device1,
    ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_TYPELESS, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{DXGI_SHARED_RESOURCE_READ, DXGI_SHARED_RESOURCE_WRITE, IDXGIKeyedMutex};
use windows::core::{HSTRING, Interface};

use crate::{bridge, log};

#[derive(Default)]
pub struct FrameSource {
    generation: u32,
    shared: Option<(ID3D11Texture2D, IDXGIKeyedMutex)>,
    local: Option<ID3D11Texture2D>,
    last_sequence: u64,
    has_frame: bool,
}

impl FrameSource {
    /// Picks up a new frame if the app published one. Returns the latest frame (B8G8R8A8, typeless) and a
    /// version that changes with every new frame, or `None` when the app is not running or has not
    /// delivered a frame yet.
    ///
    /// # Safety
    /// On the game's render thread, with its live device and immediate context.
    pub unsafe fn update(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
    ) -> Option<(ID3D11Texture2D, u64)> {
        // SAFETY: the device and context are the game's, alive through its frame (the caller's contract); the
        // shared texture is copied only while its keyed mutex is held, and released at once.
        unsafe {
            let host = &bridge::get()?.host;
            if !bridge::host_alive() {
                return None;
            }

            let generation = host.frame_generation.load(Ordering::Acquire);
            if generation == 0 {
                return None;
            }
            if generation != self.generation {
                self.open(device, generation);
            }

            let (shared, mutex) = self.shared.as_ref()?;
            let local = self.local.as_ref()?;
            let sequence = host.frame_seq.load(Ordering::Acquire);
            if sequence != self.last_sequence {
                // Zero timeout: never stall the game's render thread. `AcquireSync` reports a timeout as a
                // success code, so compare the raw HRESULT.
                let acquired = (Interface::vtable(mutex).AcquireSync)(mutex.as_raw(), FRAME_KEY, 0);
                if acquired.0 == 0 {
                    context.CopyResource(local, shared);
                    let _ = mutex.ReleaseSync(FRAME_KEY);
                    self.last_sequence = sequence;
                    self.has_frame = true;
                }
            }
            self.has_frame.then(|| (local.clone(), self.last_sequence))
        }
    }

    /// # Safety
    /// As [`Self::update`]: the game's live device, on its render thread.
    unsafe fn open(&mut self, device: &ID3D11Device, generation: u32) {
        // SAFETY: the game's live device (the caller's contract).
        unsafe {
            self.generation = generation;
            self.shared = None;
            self.local = None;
            self.has_frame = false;
            self.last_sequence = 0;
            match self.try_open(device, generation) {
                Ok(()) => log!("frame: opened shared texture generation {generation}"),
                Err(e) => log!("frame: cannot open shared texture generation {generation}: {e}"),
            }
        }
    }

    /// # Safety
    /// As [`Self::update`]: the game's live device, on its render thread.
    unsafe fn try_open(&mut self, device: &ID3D11Device, generation: u32) -> windows::core::Result<()> {
        // SAFETY: the game's live device (the caller's contract); the descriptions are locals, and the shared texture
        // is refused unless it's the frame the screens are made for.
        unsafe {
            let device1: ID3D11Device1 = device.cast()?;
            let name = HSTRING::from(frame_texture_name(generation));
            let shared: ID3D11Texture2D =
                device1.OpenSharedResourceByName(&name, (DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE).0)?;
            let mutex: IDXGIKeyedMutex = shared.cast()?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            shared.GetDesc(&mut desc);
            // The screens copy it whole, into the game's own textures: only the frame they're made for.
            if let Err(problem) = frame_fits(desc.Width, desc.Height, desc.Format, desc.SampleDesc.Count) {
                return Err(windows::core::Error::new(E_INVALIDARG, format!("refused: {problem}")));
            }

            // Typeless so it can be copied into the screen texture, which the game loads as B8G8R8A8 sRGB (the
            // app's pixels are sRGB-encoded, so the game's sampling decodes them correctly).
            let local_desc = D3D11_TEXTURE2D_DESC {
                Width: desc.Width,
                Height: desc.Height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_TYPELESS,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut local = None;
            device.CreateTexture2D(&local_desc, None, Some(&mut local))?;
            let local = local.ok_or_else(windows::core::Error::empty)?;
            self.shared = Some((shared, mutex));
            self.local = Some(local);
            Ok(())
        }
    }
}

/// Whether a shared texture is a frame the screens can take ([`SCREEN_WIDTH`] x [`SCREEN_HEIGHT`], one
/// sample, B8G8R8A8 as the app makes them), or what's wrong with it: the screens copy it whole into the
/// game's screen textures, and a copy between textures that don't match is undefined in the game's driver.
fn frame_fits(width: u32, height: u32, format: DXGI_FORMAT, samples: u32) -> Result<(), String> {
    if (width, height) != (SCREEN_WIDTH, SCREEN_HEIGHT) {
        return Err(format!("{width} x {height}, not {SCREEN_WIDTH} x {SCREEN_HEIGHT}"));
    }
    if format != DXGI_FORMAT_B8G8R8A8_UNORM {
        return Err(format!("format {}, not B8G8R8A8", format.0));
    }
    if samples != 1 {
        return Err(format!("{samples} samples, not one"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM;

    #[test]
    fn takes_only_the_frames_the_screens_are_made_for() {
        let format = DXGI_FORMAT_B8G8R8A8_UNORM;
        assert_eq!(frame_fits(SCREEN_WIDTH, SCREEN_HEIGHT, format, 1), Ok(()));
        // The dev host at another size: larger would overrun the screen, smaller would leave it short.
        assert!(frame_fits(1920, 1080, format, 1).is_err());
        assert!(frame_fits(800, 480, format, 1).is_err());
        assert!(frame_fits(SCREEN_WIDTH, SCREEN_HEIGHT + 1, format, 1).is_err());
        assert!(frame_fits(SCREEN_WIDTH, SCREEN_HEIGHT, DXGI_FORMAT_R8G8B8A8_UNORM, 1).is_err());
        assert!(frame_fits(SCREEN_WIDTH, SCREEN_HEIGHT, format, 4).is_err());
    }
}
