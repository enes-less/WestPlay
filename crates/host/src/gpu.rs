//! D3D11 device on the game's GPU and the shared frame texture the plugin reads.

use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HMODULE, LUID};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX, D3D11_RESOURCE_MISC_SHARED_NTHANDLE, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_SHARED_RESOURCE_READ, DXGI_SHARED_RESOURCE_WRITE, IDXGIAdapter, IDXGIDevice,
    IDXGIFactory4, IDXGIKeyedMutex, IDXGIResource1,
};
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
use windows::core::{HSTRING, Interface, Result};

pub struct Gpu {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    /// The same device as a WinRT `IDirect3DDevice`, for Windows Graphics Capture.
    pub winrt_device: IDirect3DDevice,
    pub luid: LUID,
    pub adapter_name: String,
}

impl Gpu {
    /// Creates a device on the adapter with `luid`, or on the default adapter.
    pub fn new(luid: Option<LUID>) -> Result<Self> {
        // SAFETY: plain Direct3D calls with locals; the device and context are returned, owned.
        unsafe {
            let factory: IDXGIFactory4 = CreateDXGIFactory1()?;
            let adapter: IDXGIAdapter = match luid {
                Some(luid) => factory.EnumAdapterByLuid(luid)?,
                None => factory.EnumAdapters(0)?,
            };
            let desc = adapter.GetDesc()?;

            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let device: ID3D11Device = device.ok_or_else(windows::core::Error::empty)?;
            let context = context.ok_or_else(windows::core::Error::empty)?;
            // Capture callbacks run on worker threads.
            let _ = device.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);
            let winrt_device = CreateDirect3D11DeviceFromDXGIDevice(&device.cast::<IDXGIDevice>()?)?.cast()?;

            let adapter_name = String::from_utf16_lossy(&desc.Description).trim_end_matches('\0').to_string();
            Ok(Self { device, context, winrt_device, luid: desc.AdapterLuid, adapter_name })
        }
    }
}

/// BGRA texture shared with the game by name, guarded by a keyed mutex.
pub struct SharedFrame {
    pub texture: ID3D11Texture2D,
    pub mutex: IDXGIKeyedMutex,
    pub width: u32,
    pub height: u32,
    handle: HANDLE,
}

// SAFETY: Direct3D 11 resources and DXGI keyed mutexes are free-threaded (unlike a device context, which this
// doesn't hold), and the handle is only closed, once, as it goes.
unsafe impl Send for SharedFrame {}
// SAFETY: as for `Send`: its texture and mutex may be used from several threads at once.
unsafe impl Sync for SharedFrame {}

impl SharedFrame {
    pub fn new(gpu: &Gpu, width: u32, height: u32, generation: u32) -> Result<Self> {
        // SAFETY: the device is alive (`gpu`); the description and the handle out are locals.
        unsafe {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: (D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0 | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0) as u32,
            };
            let mut texture = None;
            gpu.device.CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture: ID3D11Texture2D = texture.ok_or_else(windows::core::Error::empty)?;
            let mutex = texture.cast()?;
            let name = HSTRING::from(westplay_shared::frame_texture_name(generation));
            // The name stays resolvable for as long as this handle is open.
            let handle = texture.cast::<IDXGIResource1>()?.CreateSharedHandle(
                None,
                (DXGI_SHARED_RESOURCE_READ | DXGI_SHARED_RESOURCE_WRITE).0,
                &name,
            )?;
            Ok(Self { texture, mutex, width, height, handle })
        }
    }

    /// Tries to lock the texture without waiting. `AcquireSync` reports a timeout as a success code, so
    /// the raw HRESULT is compared.
    pub fn try_lock(&self) -> Lock {
        let hr =
            // SAFETY: the keyed mutex is this frame's own, alive as long as it; zero timeout, only a status back.
            unsafe { (Interface::vtable(&self.mutex).AcquireSync)(self.mutex.as_raw(), westplay_shared::FRAME_KEY, 0) };
        lock_of(hr.0)
    }

    pub fn unlock(&self) {
        // SAFETY: released only by the holder: `unlock` follows a `try_lock` that held it.
        unsafe {
            let _ = self.mutex.ReleaseSync(westplay_shared::FRAME_KEY);
        }
    }
}

/// How trying to lock the shared texture went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lock {
    /// Locked: copy, then [`SharedFrame::unlock`].
    Held,
    /// The game is copying from it (or the wait timed out): try again later.
    Busy,
    /// Its other user died holding it (the game crashing mid-copy): the texture and its lock are no longer
    /// consistent, and must be made anew (DXGI's guidance), under a new generation.
    Abandoned,
}

/// What `AcquireSync`'s result means.
fn lock_of(hr: i32) -> Lock {
    const WAIT_ABANDONED: i32 = 0x80;
    match hr {
        0 => Lock::Held,
        WAIT_ABANDONED => Lock::Abandoned,
        _ => Lock::Busy,
    }
}

impl Drop for SharedFrame {
    fn drop(&mut self) {
        // SAFETY: the shared handle is this frame's own, closed once, as it goes.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_a_lock_abandoned_by_a_dead_game_from_a_busy_one() {
        assert_eq!(lock_of(0), Lock::Held);
        // WAIT_TIMEOUT, a success code: busy.
        assert_eq!(lock_of(0x102), Lock::Busy);
        assert_eq!(lock_of(0x80), Lock::Abandoned);
        // DXGI_ERROR_DEVICE_REMOVED and friends: not ours to recover here (the pipeline is rebuilt for the
        // game's GPU anyway).
        assert_eq!(lock_of(0x887A0005u32 as i32), Lock::Busy);
    }
}
