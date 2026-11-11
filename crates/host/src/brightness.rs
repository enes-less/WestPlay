//! The screen's brightness (Settings › Appearance › Brightness; owner 2026-11-11: "real brightness. Not just a black
//! overlay"): the light the screen gives off, scaled as a screen's backlight scales it. Applied to the frame the game
//! shows, as it's put together (`capture.rs`): the page, a video playing in it, and the cursor drawn over them, so
//! everything on the screen dims together, which the page alone couldn't do. In linear light: each colour decoded
//! from sRGB, its light scaled, and encoded again, through Direct2D's table transfer.
//!
//! The slider goes evenly for the eye (CIE lightness), from [`LEAST_LIGHT`] of the screen's full light to all of it:
//! never black, as no screen's lowest brightness is.

use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COMPOSITE_MODE_SOURCE_COPY, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    CLSID_D2D1TableTransfer, D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_OPTIONS_TARGET,
    D2D1_BITMAP_PROPERTIES1, D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_MULTI_THREADED,
    D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR, D2D1_PROPERTY_TYPE_BOOL, D2D1_PROPERTY_TYPE_UNKNOWN,
    D2D1_TABLETRANSFER_PROP_ALPHA_DISABLE, D2D1_TABLETRANSFER_PROP_BLUE_TABLE, D2D1_TABLETRANSFER_PROP_CLAMP_OUTPUT,
    D2D1_TABLETRANSFER_PROP_GREEN_TABLE, D2D1_TABLETRANSFER_PROP_RED_TABLE, D2D1CreateFactory, ID2D1Bitmap1,
    ID2D1DeviceContext, ID2D1Effect, ID2D1Factory1, ID2D1Image,
};
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::{IDXGIDevice, IDXGISurface};
use windows::core::{Interface, Result};

use crate::gpu::Gpu;

/// The least light the slider goes down to, of the screen's full light (CIE lightness 9 of 100).
pub const LEAST_LIGHT: f32 = 0.01;

/// Entries in each channel's table (Direct2D goes between them in a straight line).
const TABLE: usize = 256;

/// The light (0 to 1 of the screen's full light, linear) at the slider's `level` (0 to 1): evenly for the eye, as
/// CIE lightness, from [`LEAST_LIGHT`] at the bottom to all of it at the top.
pub fn light(level: f32) -> f32 {
    let level = if level.is_finite() { level.clamp(0.0, 1.0) } else { 1.0 };
    let least = lightness(LEAST_LIGHT);
    luminance(least + (100.0 - least) * level)
}

/// CIE lightness (0 to 100) of the light `y` (0 to 1).
fn lightness(y: f32) -> f32 {
    if y > 216.0 / 24389.0 { 116.0 * y.cbrt() - 16.0 } else { y * 24389.0 / 27.0 }
}

/// The light (0 to 1) of CIE lightness `l` (0 to 100).
fn luminance(l: f32) -> f32 {
    if l > 8.0 { ((l + 16.0) / 116.0).powi(3) } else { l * 27.0 / 24389.0 }
}

/// An sRGB channel (0 to 1) as light.
fn decode(value: f32) -> f32 {
    if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
}

/// Light (0 to 1) as an sRGB channel.
fn encode(light: f32) -> f32 {
    if light <= 0.003_130_8 { light * 12.92 } else { 1.055 * light.powf(1.0 / 2.4) - 0.055 }
}

/// The sRGB channel `value` (0 to 1) with its light scaled by `light`.
pub fn dimmed(value: f32, light: f32) -> f32 {
    encode(decode(value.clamp(0.0, 1.0)) * light.clamp(0.0, 1.0))
}

/// Each channel's table for `light`: an sRGB value in, the same with its light scaled out.
fn table(light: f32) -> Vec<f32> {
    (0..TABLE).map(|step| dimmed(step as f32 / (TABLE - 1) as f32, light)).collect()
}

/// Copies one texture into another (the page's frame into the one the game shows) with its light scaled.
pub struct Dimmer {
    context: ID2D1DeviceContext,
    effect: ID2D1Effect,
    /// The table set, the light it's for.
    table_for: std::cell::Cell<Option<f32>>,
    /// The source and target as Direct2D sees them (held with the context and the effect).
    _source: ID2D1Bitmap1,
    _target: ID2D1Bitmap1,
}

impl Dimmer {
    /// From `source` (B8G8R8A8, a shader resource) into `target` (the same size and format, a render target), on
    /// `gpu`'s device.
    pub fn new(gpu: &Gpu, source: &ID3D11Texture2D, target: &ID3D11Texture2D) -> Result<Self> {
        let properties = |options| D2D1_BITMAP_PROPERTIES1 {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            bitmapOptions: options,
            colorContext: std::mem::ManuallyDrop::new(None),
        };
        // SAFETY: plain Direct2D calls on the host's live device and textures; every description is a local. The
        // factory is multithreaded: the device is shared with Windows' capture threads (`gpu`).
        unsafe {
            let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, None)?;
            let device = factory.CreateDevice(&gpu.device.cast::<IDXGIDevice>()?)?;
            let context = device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let source = context.CreateBitmapFromDxgiSurface(
                &source.cast::<IDXGISurface>()?,
                Some(&properties(D2D1_BITMAP_OPTIONS_NONE)),
            )?;
            let target = context.CreateBitmapFromDxgiSurface(
                &target.cast::<IDXGISurface>()?,
                Some(&properties(D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW)),
            )?;
            context.SetTarget(&target);
            let effect = context.CreateEffect(&CLSID_D2D1TableTransfer)?;
            effect.SetInput(0, &source, true);
            // Alpha as it is; nothing past the table's ends.
            let yes = 1i32.to_le_bytes();
            effect.SetValue(D2D1_TABLETRANSFER_PROP_ALPHA_DISABLE.0 as u32, D2D1_PROPERTY_TYPE_BOOL, &yes)?;
            effect.SetValue(D2D1_TABLETRANSFER_PROP_CLAMP_OUTPUT.0 as u32, D2D1_PROPERTY_TYPE_BOOL, &yes)?;
            Ok(Self { context, effect, table_for: std::cell::Cell::new(None), _source: source, _target: target })
        }
    }

    /// The source into the target, its light scaled by `light` (0 to 1).
    pub fn draw(&self, light: f32) -> Result<()> {
        if self.table_for.get() != Some(light) {
            let bytes: Vec<u8> = table(light).iter().flat_map(|value| value.to_le_bytes()).collect();
            for channel in [
                D2D1_TABLETRANSFER_PROP_RED_TABLE,
                D2D1_TABLETRANSFER_PROP_GREEN_TABLE,
                D2D1_TABLETRANSFER_PROP_BLUE_TABLE,
            ] {
                // SAFETY: a table of floats for one of the effect's own properties, the bytes alive through the call.
                unsafe { self.effect.SetValue(channel.0 as u32, D2D1_PROPERTY_TYPE_UNKNOWN, &bytes)? };
            }
            self.table_for.set(Some(light));
        }
        // SAFETY: plain Direct2D calls on the live context, its target and the effect over the source. The whole
        // target is written (copied, not blended over what it held: the frame's clear corners stay clear).
        unsafe {
            self.context.BeginDraw();
            self.context.DrawImage(
                &self.effect.cast::<ID2D1Image>()?,
                None,
                None,
                D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
                D2D1_COMPOSITE_MODE_SOURCE_COPY,
            );
            self.context.EndDraw(None, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slider_goes_from_the_least_light_to_all_of_it_evenly_for_the_eye() {
        assert!((light(0.0) - LEAST_LIGHT).abs() < 1e-4);
        assert!((light(1.0) - 1.0).abs() < 1e-5);
        // Halfway for the eye: CIE lightness halfway between 9 and 100 (54.5), about a fifth of the light.
        assert!((light(0.5) - 0.225).abs() < 0.01, "{}", light(0.5));
        // Only ever brighter going up.
        assert!((1..=100).all(|step| light(step as f32 / 100.0) > light((step - 1) as f32 / 100.0)));
        // Out of range or not a number: held in, or full.
        assert_eq!((light(-1.0), light(2.0), light(f32::NAN)), (light(0.0), 1.0, 1.0));
    }

    #[test]
    fn dims_in_light_not_in_srgb_values() {
        // sRGB 188 is about half the light: at half the light, a quarter (sRGB 137), not 94 as halving the value.
        let value = dimmed(188.0 / 255.0, 0.5);
        assert!((value * 255.0 - 137.0).abs() < 1.0, "{}", value * 255.0);
        // Black stays black, full light leaves every value as it is.
        assert_eq!(dimmed(0.0, 0.3), 0.0);
        assert!((0..=255).all(|step| (dimmed(step as f32 / 255.0, 1.0) * 255.0 - step as f32).abs() < 0.01));
        // The tables from 0 to 1 with the light scaled.
        let half = table(0.5);
        assert_eq!((half.len(), half[0]), (TABLE, 0.0));
        assert!((half[TABLE - 1] - encode(0.5)).abs() < 1e-6);
    }

    /// The real drawing, on this PC's GPU: a frame of four greys and a clear corner, copied at half the light; the
    /// pixels read back.
    #[test]
    #[ignore = "draws on the GPU"]
    fn copies_a_frame_with_its_light_scaled() {
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ,
            D3D11_MAPPED_SUBRESOURCE, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
            D3D11_USAGE_STAGING,
        };
        use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
        let (width, height) = (8u32, 2u32);
        let gpu = Gpu::new(None).unwrap();
        // Greys 255, 188, 100, 20 (two pixels each) on the top row; the bottom row clear (premultiplied 0).
        let greys = [255u8, 255, 188, 188, 100, 100, 20, 20];
        let pixels: Vec<u8> =
            greys.iter().flat_map(|&g| [g, g, g, 255]).chain(std::iter::repeat_n(0u8, width as usize * 4)).collect();
        let mut desc = D3D11_TEXTURE2D_DESC {
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
        // The target starts white all over: what it held before must not show through the clear row.
        let white = vec![255u8; (width * height * 4) as usize];
        let before =
            D3D11_SUBRESOURCE_DATA { pSysMem: white.as_ptr().cast(), SysMemPitch: width * 4, SysMemSlicePitch: 0 };
        let (mut source, mut target, mut staging) = (None, None, None);
        // SAFETY: plain Direct3D calls on the test's own device; the descriptions and pixels are locals.
        unsafe {
            gpu.device.CreateTexture2D(&desc, Some(&initial), Some(&mut source)).unwrap();
            desc.BindFlags = (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32;
            gpu.device.CreateTexture2D(&desc, Some(&before), Some(&mut target)).unwrap();
        }
        let (source, target) = (source.unwrap(), target.unwrap());
        let dimmer = Dimmer::new(&gpu, &source, &target).unwrap();
        dimmer.draw(0.5).unwrap();

        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        let mut read = vec![0u8; (width * height * 4) as usize];
        // SAFETY: the staging copy is the test's own, mapped for reading and unmapped after; rows are copied within the
        // mapped pitch.
        unsafe {
            gpu.device.CreateTexture2D(&desc, None, Some(&mut staging)).unwrap();
            let staging = staging.unwrap();
            gpu.context.CopyResource(&staging, &target);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            gpu.context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).unwrap();
            for row in 0..height as usize {
                let from = (mapped.pData as *const u8).add(row * mapped.RowPitch as usize);
                std::ptr::copy_nonoverlapping(from, read[row * width as usize * 4..].as_mut_ptr(), width as usize * 4);
            }
            gpu.context.Unmap(&staging, 0);
        }
        let pixel = |x: u32, y: u32| {
            let at = ((y * width + x) * 4) as usize;
            [read[at], read[at + 1], read[at + 2], read[at + 3]]
        };
        for (x, &grey) in greys.iter().enumerate() {
            let want = (dimmed(grey as f32 / 255.0, 0.5) * 255.0).round() as i32;
            let [b, g, r, a] = pixel(x as u32, 0);
            assert!(
                [b, g, r].iter().all(|&c| (c as i32 - want).abs() <= 1) && a == 255,
                "grey {grey}: {:?}, want {want}",
                pixel(x as u32, 0)
            );
        }
        assert!((0..width).all(|x| pixel(x, 1) == [0, 0, 0, 0]), "the clear row: {:?}", pixel(0, 1));
    }
}
