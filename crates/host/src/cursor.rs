//! The screen's cursor, drawn by the host into each frame it shares with the game, where the mouse is at once.
//!
//! The page drew its own (`ui/src/cabin/shell/Cursor.tsx`), and each move went the whole way round before it
//! showed: the host passing it to the page, the page drawing its next frame, Windows composing and capturing
//! it, the game drawing it on its next frame (owner 2026-10-27: a cursor three or four frames behind the hand,
//! more at a game's 30 fps). Drawn here, the move shows in the next frame the game takes. The page still has
//! every move and click (hovering, pressing), and says whether what's under the cursor is light ([`set_light`]):
//! the cursor goes dark there, as it did.
//!
//! It looks as the page's did (`shell.css`, `.cursor`): a soft white dot with a ring, its shadow outside it,
//! shrinking while pressed; dark over something light; faded away after a while untouched. Each change moves on
//! the page's own curve and times. The page is squeezed into the frame where it's laid out taller
//! (`webview::Stage::set_page_height`), and so is the dot: the game stretches the frame back, and it's round.
//!
//! [`set_light`]: Cursor::set_light

use std::time::{Duration, Instant};

use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET,
    D2D1_BITMAP_PROPERTIES1, D2D1_BUFFER_PRECISION_8BPC_UNORM, D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT,
    D2D1_COLOR_SPACE_SRGB, D2D1_COMBINE_MODE_EXCLUDE, D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_ELLIPSE,
    D2D1_EXTEND_MODE_CLAMP, D2D1_FACTORY_TYPE_MULTI_THREADED, D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1,
    D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, D2D1CreateFactory, ID2D1Bitmap1, ID2D1DeviceContext, ID2D1Factory1,
    ID2D1Geometry, ID2D1GeometrySink, ID2D1GradientStopCollection, ID2D1PathGeometry,
};
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::{IDXGIDevice, IDXGISurface};
use windows::core::{Interface, Result};
use windows_numerics::{Matrix3x2, Vector2};

use crate::gpu::Gpu;

/// The dot's radius, its ring's width inside its edge, its shadow's offset down and blur (CSS's blur radius:
/// a Gaussian of half of it), page pixels (`shell.css`, `.cursor-dot`).
const RADIUS: f32 = 22.0;
const RING: f32 = 2.0;
const SHADOW_DOWN: f32 = 4.0;
const SHADOW_BLUR: f32 = 16.0;
/// How small it is while pressed.
const PRESSED_SCALE: f32 = 0.82;
/// How long it stays untouched before it fades away (`Cursor.tsx`, `IDLE_MS`).
const IDLE_AFTER: Duration = Duration::from_secs(5);
/// How long each change takes, as the page's transitions: fading, shrinking, its colours.
const FADE: Duration = Duration::from_millis(600);
const SHRINK: Duration = Duration::from_millis(120);
const COLOUR: Duration = Duration::from_millis(260);

/// Apple's own curve, as every WestPlay transition moves on (`shell.css`, `--motion-ease`: a critically damped
/// spring, `1 - (1 + wt)e^-wt`, settled at its end), the same samples, followed straight between them as CSS's
/// `linear()` does.
const EASE: [f32; 25] = [
    0.0, 0.0575, 0.1803, 0.3208, 0.4551, 0.5729, 0.6711, 0.7501, 0.8122, 0.8601, 0.8966, 0.924, 0.9445, 0.9596, 0.9708,
    0.9789, 0.9848, 0.9891, 0.9922, 0.9944, 0.996, 0.9972, 0.998, 0.9986, 1.0,
];

/// The curve at `t` (0 to 1).
fn ease(t: f32) -> f32 {
    let at = t.clamp(0.0, 1.0) * (EASE.len() - 1) as f32;
    let below = (at as usize).min(EASE.len() - 2);
    let within = at - below as f32;
    EASE[below] + (EASE[below + 1] - EASE[below]) * within
}

/// A value moving from one to another over a while, on the curve.
#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

impl Tween {
    fn at(value: f32, now: Instant) -> Self {
        Self { from: value, to: value, start: now, duration: Duration::ZERO }
    }

    fn value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let t = now.saturating_duration_since(self.start).as_secs_f32() / self.duration.as_secs_f32();
        self.from + (self.to - self.from) * ease(t)
    }

    /// Moves on to `to` over `duration`, from wherever it is now (a change of mind halfway goes on from there).
    fn aim(&mut self, to: f32, duration: Duration, now: Instant) {
        if to != self.to {
            *self = Self { from: self.value(now), to, start: now, duration };
        }
    }

    fn snap(&mut self, to: f32, now: Instant) {
        *self = Self::at(to, now);
    }
}

/// The cursor as it looks now, to draw: where (page pixels), how seen (0 to 1), how big (1: as it rests), how
/// dark (0: white, 1: dark, over something light) and how pressed in colour (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub x: f32,
    pub y: f32,
    pub opacity: f32,
    pub scale: f32,
    pub tone: f32,
    pub press: f32,
}

/// The mouse in the page this turn, as the pointer has it (`pointer::Pointer::update`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mouse {
    /// The player holds the interaction key: the mouse is in the page.
    pub active: bool,
    pub x: f32,
    pub y: f32,
    pub pressed: bool,
    /// It moved, clicked or scrolled this turn.
    pub used: bool,
}

pub struct Cursor {
    active: bool,
    x: f32,
    y: f32,
    pressed: bool,
    light: bool,
    last_used: Instant,
    opacity: Tween,
    scale: Tween,
    tone: Tween,
    press: Tween,
}

impl Cursor {
    pub fn new(now: Instant) -> Self {
        Self {
            active: false,
            x: 0.0,
            y: 0.0,
            pressed: false,
            light: false,
            last_used: now,
            opacity: Tween::at(0.0, now),
            scale: Tween::at(1.0, now),
            tone: Tween::at(0.0, now),
            press: Tween::at(0.0, now),
        }
    }

    /// Follows the mouse, each turn: shown at once as it comes into the page, faded away after a while untouched
    /// and back as it's used. Let go of (the interaction key released) is left untouched too (owner 2026-10-27:
    /// it went at once): it stays where it was, a press let go, and fades as untouched; taken up again before it's
    /// gone, it goes on from where it is.
    pub fn follow(&mut self, mouse: Mouse, now: Instant) {
        if !mouse.active {
            if self.active {
                self.active = false;
                self.pressed = false;
                self.scale.aim(1.0, SHRINK, now);
                self.press.aim(0.0, COLOUR, now);
            }
            // Only ever going: never shown while let go.
            if now.saturating_duration_since(self.last_used) >= IDLE_AFTER {
                self.opacity.aim(0.0, FADE, now);
            }
            return;
        }
        if !self.active {
            self.active = true;
            self.last_used = now;
            // Gone: shown at once where the mouse is; still seen: on from there.
            if self.opacity.value(now) <= 0.0 {
                self.opacity.snap(1.0, now);
                self.scale.snap(if mouse.pressed { PRESSED_SCALE } else { 1.0 }, now);
                self.press.snap(f32::from(u8::from(mouse.pressed)), now);
            }
        }
        (self.x, self.y) = (mouse.x, mouse.y);
        if mouse.used || mouse.pressed != self.pressed {
            self.last_used = now;
        }
        self.pressed = mouse.pressed;
        self.scale.aim(if mouse.pressed { PRESSED_SCALE } else { 1.0 }, SHRINK, now);
        self.press.aim(f32::from(u8::from(mouse.pressed)), COLOUR, now);
        // Not while a button is held: a drag can pause without the cursor going.
        let idle = !mouse.pressed && now.saturating_duration_since(self.last_used) >= IDLE_AFTER;
        self.opacity.aim(if idle { 0.0 } else { 1.0 }, FADE, now);
    }

    /// Whether what's under the cursor is light, as the page says (it goes dark there).
    pub fn set_light(&mut self, light: bool, now: Instant) {
        self.light = light;
        self.tone.aim(f32::from(u8::from(light)), COLOUR, now);
    }

    /// How it looks now (`None`: not seen at all).
    pub fn look(&self, now: Instant) -> Option<Look> {
        let opacity = self.opacity.value(now);
        (opacity > 0.0).then(|| Look {
            x: self.x,
            y: self.y,
            opacity,
            scale: self.scale.value(now),
            tone: self.tone.value(now),
            press: self.press.value(now),
        })
    }
}

/// A colour in sRGB (0 to 1) with its alpha, straight.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Colour {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

const fn white(a: f32) -> Colour {
    Colour { r: 1.0, g: 1.0, b: 1.0, a }
}

const fn black(a: f32) -> Colour {
    Colour { r: 0.0, g: 0.0, b: 0.0, a }
}

/// Between two colours, as CSS goes between them (premultiplied).
fn mix(from: Colour, to: Colour, t: f32) -> Colour {
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    let a = lerp(from.a, to.a);
    if a <= 0.0 {
        return black(0.0);
    }
    let channel = |x: f32, y: f32| lerp(x * from.a, y * to.a) / a;
    Colour { r: channel(from.r, to.r), g: channel(from.g, to.g), b: channel(from.b, to.b), a }
}

/// The cursor's colours at `look`'s tone and press (`shell.css`: `.cursor-dot`, `.pressed`, `.on-light`): its
/// fill, its ring, its shadow.
fn colours(look: &Look) -> (Colour, Colour, Colour) {
    let on_dark = mix(white(0.35), white(0.6), look.press);
    let on_light = mix(black(0.2), black(0.36), look.press);
    let fill = mix(on_dark, on_light, look.tone);
    let ring = mix(white(0.85), black(0.72), look.tone);
    let shadow = mix(black(0.35), black(0.18), look.tone);
    (fill, ring, shadow)
}

/// How much of a shadow's colour is seen `outside` page pixels past the dot's edge (negative: under it), its
/// blur's Gaussian `sigma`: a straight edge blurred, as CSS blurs a shadow.
fn shadow_at(outside: f32, sigma: f32) -> f32 {
    // The normal distribution's tail (Abramowitz and Stegun's 7.1.26 for erf, ample for a soft shadow).
    let erf = |x: f32| {
        let sign = x.signum();
        let x = x.abs();
        let t = 1.0 / (1.0 + 0.327_591_1 * x);
        let y = 1.0
            - (((((1.061_405_4 * t - 1.453_152_1) * t) + 1.421_413_8) * t - 0.284_496_74) * t + 0.254_829_6)
                * t
                * (-x * x).exp();
        sign * y
    };
    0.5 * (1.0 - erf(outside / (sigma * std::f32::consts::SQRT_2)))
}

/// Draws the cursor with Direct2D onto the frame shared with the game, on the host's device.
pub struct Painter {
    factory: ID2D1Factory1,
    context: ID2D1DeviceContext,
    /// The shared frame as Direct2D draws on it (the context's target, held with it).
    _target: ID2D1Bitmap1,
}

impl Painter {
    /// A painter for `texture` (the shared frame: a render target, B8G8R8A8), on `gpu`'s device.
    pub fn new(gpu: &Gpu, texture: &ID3D11Texture2D) -> Result<Self> {
        // SAFETY: plain Direct2D calls on the host's live device and texture; every description is a local. The
        // factory is multithreaded: the device is shared with Windows' capture threads (`gpu`).
        unsafe {
            let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, None)?;
            let device = factory.CreateDevice(&gpu.device.cast::<IDXGIDevice>()?)?;
            let context = device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let properties = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: std::mem::ManuallyDrop::new(None),
            };
            let target = context.CreateBitmapFromDxgiSurface(&texture.cast::<IDXGISurface>()?, Some(&properties))?;
            context.SetTarget(&target);
            Ok(Self { factory, context, _target: target })
        }
    }

    /// Draws `look` over what the frame holds, the page squeezed into it `squeeze` times its height.
    pub fn draw(&self, look: &Look, squeeze: f32) -> Result<()> {
        let (fill, ring, shadow) = colours(look);
        let radius = RADIUS * look.scale;
        let down = SHADOW_DOWN * look.scale;
        let sigma = SHADOW_BLUR / 2.0 * look.scale;
        let centre = Vector2 { X: look.x, Y: look.y };
        let dot = D2D1_ELLIPSE { point: centre, radiusX: radius, radiusY: radius };
        // SAFETY: plain Direct2D calls on the live context and its target; brushes, geometries and layer
        // parameters are locals that live through the drawing.
        unsafe {
            let context = &self.context;
            context.BeginDraw();
            // Page pixels: the page squeezed into the frame.
            context.SetTransform(&Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: squeeze, M31: 0.0, M32: 0.0 });
            // The whole cursor seen as one, fading.
            let whole = layer(look.opacity, None);
            context.PushLayer(&whole, None);

            // Its shadow, outside it only (CSS clips an outer shadow at its element): the dot's own shape
            // taken out of an area around it.
            let reach = radius + down + 3.0 * sigma;
            let around =
                D2D_RECT_F { left: look.x - reach, top: look.y - reach, right: look.x + reach, bottom: look.y + reach };
            let outside = self.outside(&around, &dot)?;
            let mut clipped = layer(1.0, Some(outside));
            context.PushLayer(&clipped, None);
            let edge = radius + 3.0 * sigma;
            let stops: Vec<D2D1_GRADIENT_STOP> = (0..=12)
                .map(|step| {
                    let at = edge * step as f32 / 12.0;
                    let alpha = shadow.a * shadow_at(at - radius, sigma);
                    D2D1_GRADIENT_STOP { position: at / edge, color: straight(black(alpha)) }
                })
                .collect();
            let stops = context.CreateGradientStopCollection(
                &stops,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_COLOR_SPACE_SRGB,
                D2D1_BUFFER_PRECISION_8BPC_UNORM,
                D2D1_EXTEND_MODE_CLAMP,
                D2D1_COLOR_INTERPOLATION_MODE_STRAIGHT,
            )?;
            let gradient = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                center: Vector2 { X: look.x, Y: look.y + down },
                gradientOriginOffset: Vector2::default(),
                radiusX: edge,
                radiusY: edge,
            };
            let brush =
                context.CreateRadialGradientBrush(&gradient, None, &stops.cast::<ID2D1GradientStopCollection>()?)?;
            let spread = D2D1_ELLIPSE { point: gradient.center, radiusX: edge, radiusY: edge };
            context.FillEllipse(&spread, &brush);
            context.PopLayer();
            // The layer's mask is ours to let go of (the parameters hold it without dropping it).
            drop(std::mem::ManuallyDrop::take(&mut clipped.geometricMask));

            // The dot, and its ring just inside its edge.
            let brush = context.CreateSolidColorBrush(&straight(fill), None)?;
            context.FillEllipse(&dot, &brush);
            let width = RING * look.scale;
            let rim = D2D1_ELLIPSE { point: centre, radiusX: radius - width / 2.0, radiusY: radius - width / 2.0 };
            let brush = context.CreateSolidColorBrush(&straight(ring), None)?;
            context.DrawEllipse(&rim, &brush, width, None);

            context.PopLayer();
            context.SetTransform(&Matrix3x2::identity());
            context.EndDraw(None, None)
        }
    }

    /// `area` with the `dot` taken out of it.
    fn outside(&self, area: &D2D_RECT_F, dot: &D2D1_ELLIPSE) -> Result<ID2D1Geometry> {
        // SAFETY: plain Direct2D geometry calls; the sink is closed before the geometry is used.
        unsafe {
            let area = self.factory.CreateRectangleGeometry(area)?;
            let dot = self.factory.CreateEllipseGeometry(dot)?;
            let path: ID2D1PathGeometry = self.factory.CreatePathGeometry()?.cast()?;
            let sink: ID2D1GeometrySink = path.Open()?;
            area.CombineWithGeometry(&dot, D2D1_COMBINE_MODE_EXCLUDE, None, 0.25, &sink)?;
            sink.Close()?;
            path.cast()
        }
    }
}

/// A layer seen `opacity` times, clipped to `mask` if given.
fn layer(opacity: f32, mask: Option<ID2D1Geometry>) -> D2D1_LAYER_PARAMETERS1 {
    D2D1_LAYER_PARAMETERS1 {
        contentBounds: D2D_RECT_F { left: f32::MIN, top: f32::MIN, right: f32::MAX, bottom: f32::MAX },
        geometricMask: std::mem::ManuallyDrop::new(mask),
        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
        maskTransform: Matrix3x2::identity(),
        opacity,
        opacityBrush: std::mem::ManuallyDrop::new(None),
        layerOptions: D2D1_LAYER_OPTIONS1_NONE,
    }
}

/// Direct2D's colour for `colour`, its alpha straight (Direct2D's brushes and gradients premultiply themselves).
fn straight(colour: Colour) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: colour.r, g: colour.g, b: colour.b, a: colour.a }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse(x: f32, y: f32, pressed: bool, used: bool) -> Mouse {
        Mouse { active: true, x, y, pressed, used }
    }

    #[test]
    fn follows_apples_curve() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert!((ease(0.5) - 0.9445).abs() < 1e-4);
        // Straight between two samples, and always on its way.
        assert!((ease(1.0 / 48.0) - 0.0575 / 2.0).abs() < 1e-4);
        assert!((0..100).all(|i| ease(i as f32 / 100.0) <= ease((i + 1) as f32 / 100.0)));
    }

    #[test]
    fn shows_at_once_and_stays_a_while_let_go() {
        let start = Instant::now();
        let later = |ms| start + Duration::from_millis(ms);
        let mut cursor = Cursor::new(start);
        // Never let go of before (WestPlay starting): nothing, whatever the time.
        cursor.follow(Mouse::default(), start);
        assert_eq!(cursor.look(start), None);
        cursor.follow(mouse(100.0, 50.0, true, true), later(10));
        let look = cursor.look(later(10)).unwrap();
        assert_eq!((look.x, look.y, look.opacity), (100.0, 50.0, 1.0));
        // Let go of (pressed then): where it was, its press let go, until five seconds from its last use...
        for ms in [20, 2_000, 5_009] {
            cursor.follow(Mouse::default(), later(ms));
        }
        let still = cursor.look(later(5_009)).unwrap();
        assert_eq!((still.x, still.y, still.opacity, still.scale), (100.0, 50.0, 1.0, 1.0));
        // ...then fading as untouched, and gone.
        cursor.follow(Mouse::default(), later(5_010));
        cursor.follow(Mouse::default(), later(5_310));
        let fading = cursor.look(later(5_310)).unwrap().opacity;
        assert!(fading > 0.0 && fading < 0.1, "{fading}");
        cursor.follow(Mouse::default(), later(5_700));
        assert_eq!(cursor.look(later(5_700)), None);
        // Taken up again: shown at once where the mouse is.
        cursor.follow(mouse(30.0, 40.0, false, false), later(6_000));
        assert_eq!(cursor.look(later(6_000)).unwrap().opacity, 1.0);
        // Let go of and taken up again before it's gone: on from where it is, not shown anew.
        cursor.follow(Mouse::default(), later(6_100));
        cursor.follow(mouse(31.0, 40.0, false, true), later(6_200));
        let back = cursor.look(later(6_200)).unwrap();
        assert_eq!((back.x, back.opacity), (31.0, 1.0));
    }

    #[test]
    fn fades_away_untouched_and_back_as_its_used() {
        let start = Instant::now();
        let mut cursor = Cursor::new(start);
        cursor.follow(mouse(10.0, 10.0, false, true), start);
        let later = |ms| start + Duration::from_millis(ms);
        // Untouched: seen until five seconds, then fading over 600 ms.
        cursor.follow(mouse(10.0, 10.0, false, false), later(4_999));
        assert_eq!(cursor.look(later(4_999)).unwrap().opacity, 1.0);
        cursor.follow(mouse(10.0, 10.0, false, false), later(5_000));
        let fading = cursor.look(later(5_300)).unwrap().opacity;
        assert!(fading > 0.0 && fading < 0.1, "{fading}");
        assert_eq!(cursor.look(later(5_600)), None);
        // Held pressed, it never fades.
        let mut held = Cursor::new(start);
        held.follow(mouse(10.0, 10.0, true, true), start);
        held.follow(mouse(10.0, 10.0, true, false), later(9_000));
        assert_eq!(held.look(later(9_000)).unwrap().opacity, 1.0);
        // Moved again: back, fading in from where it was.
        cursor.follow(mouse(12.0, 10.0, false, true), later(6_000));
        let back = cursor.look(later(6_300)).unwrap().opacity;
        assert!(back > 0.9 && back < 1.0, "{back}");
    }

    #[test]
    fn shrinks_while_pressed_and_goes_dark_over_light() {
        let start = Instant::now();
        let mut cursor = Cursor::new(start);
        cursor.follow(mouse(10.0, 10.0, false, true), start);
        cursor.follow(mouse(10.0, 10.0, true, false), start);
        cursor.set_light(true, start);
        let later = start + Duration::from_millis(300);
        let look = cursor.look(later).unwrap();
        assert!((look.scale - PRESSED_SCALE).abs() < 1e-3);
        assert!((look.tone - 1.0).abs() < 1e-2 && (look.press - 1.0).abs() < 1e-2);
        let (fill, ring, _) = colours(&look);
        assert!(fill.r < 0.01 && (fill.a - 0.36).abs() < 0.01, "{fill:?}");
        assert!(ring.r < 0.01 && (ring.a - 0.72).abs() < 0.01, "{ring:?}");
    }

    #[test]
    fn colours_mix_as_css_mixes_them() {
        // Halfway between white and clear black: white, half as seen (premultiplied, not grey).
        let half = mix(white(1.0), black(0.0), 0.5);
        assert_eq!((half.r, half.a), (1.0, 0.5));
        // The page's own colours at rest.
        let rest = Look { x: 0.0, y: 0.0, opacity: 1.0, scale: 1.0, tone: 0.0, press: 0.0 };
        let (fill, ring, shadow) = colours(&rest);
        assert_eq!((fill, ring, shadow), (white(0.35), white(0.85), black(0.35)));
    }

    /// The real drawing, on this PC's GPU: the cursor over a dark and a light half, at rest, pressed and dark,
    /// the page squeezed as a tall page is; the pixels read back and saved as a picture (WESTPLAY_CURSOR_PNG,
    /// else the temporary folder's westplay-cursor.png) to look at.
    #[test]
    #[ignore = "draws on the GPU"]
    fn draws_as_the_pages_cursor_looked() {
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ,
            D3D11_MAPPED_SUBRESOURCE, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
            D3D11_USAGE_STAGING,
        };
        use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
        let (width, height) = (400u32, 160u32);
        let gpu = Gpu::new(None).unwrap();
        // The left half dark (20), the right light (235).
        let pixels: Vec<u8> = (0..height)
            .flat_map(|_| (0..width).flat_map(|x| if x < width / 2 { [20, 20, 20, 255] } else { [235, 235, 235, 255] }))
            .collect();
        let mut desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let initial =
            D3D11_SUBRESOURCE_DATA { pSysMem: pixels.as_ptr().cast(), SysMemPitch: width * 4, SysMemSlicePitch: 0 };
        let (mut texture, mut staging) = (None, None);
        // SAFETY: plain Direct3D calls on the test's own device; the descriptions and pixels are locals.
        unsafe { gpu.device.CreateTexture2D(&desc, Some(&initial), Some(&mut texture)).unwrap() };
        let texture = texture.unwrap();
        let painter = Painter::new(&gpu, &texture).unwrap();
        // A page 1.25 times the frame's height: squeezed to 0.8, the dot drawn 0.8 as tall.
        let squeeze = 0.8;
        let at =
            |x: f32, scale: f32, tone: f32, press: f32| Look { x, y: 80.0 / squeeze, opacity: 1.0, scale, tone, press };
        painter.draw(&at(100.0, 1.0, 0.0, 0.0), squeeze).unwrap();
        painter.draw(&at(240.0, PRESSED_SCALE, 1.0, 1.0), squeeze).unwrap();
        painter.draw(&at(330.0, 1.0, 1.0, 0.0), squeeze).unwrap();

        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        let mut read = vec![0u8; (width * height * 4) as usize];
        // SAFETY: the staging copy is the test's own, mapped for reading and unmapped after; rows are copied
        // within the mapped pitch.
        unsafe {
            gpu.device.CreateTexture2D(&desc, None, Some(&mut staging)).unwrap();
            let staging = staging.unwrap();
            gpu.context.CopyResource(&staging, &texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            gpu.context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).unwrap();
            for row in 0..height as usize {
                let from = (mapped.pData as *const u8).add(row * mapped.RowPitch as usize);
                std::ptr::copy_nonoverlapping(from, read[row * width as usize * 4..].as_mut_ptr(), width as usize * 4);
            }
            gpu.context.Unmap(&staging, 0);
        }
        let grey = |x: u32, y: u32| read[((y * width + x) * 4) as usize];
        let path = std::env::var_os("WESTPLAY_CURSOR_PNG")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("westplay-cursor.png"));
        let rgba: Vec<u8> = read.chunks(4).flat_map(|p| [p[2], p[1], p[0], 255]).collect();
        image::save_buffer(&path, &rgba, width, height, image::ColorType::Rgba8).unwrap();
        println!("saved {}", path.display());

        // Far from it: untouched.
        assert_eq!((grey(10, 10), grey(390, 10)), (20, 235));
        // Its middle at rest, white at 0.35 over 20: about 101.
        let middle = grey(100, 80);
        assert!((95..=108).contains(&middle), "rest middle {middle}");
        // Its ring, white at 0.85, one pixel inside the edge (22 page pixels: across, unsqueezed).
        let ring = grey(100 + 21, 80);
        assert!(ring > 180, "ring {ring}");
        // Dark over light: black at 0.2 over 235, about 188.
        let dark = grey(330, 80);
        assert!((180..=196).contains(&dark), "dark middle {dark}");
        // Its shadow under it, darker than above it, and none under the dot's own middle.
        let (below, above) = (grey(330, 80 + 22 + 3), grey(330, 80 - 22 - 3));
        assert!(below < above && above <= 235, "shadow below {below}, above {above}");
        // Squeezed: as tall as 0.8 of its width (17.6 of 22 frame pixels): inside at 16 down, outside at 19.
        assert!(grey(100, 80 + 16) > 60, "inside the squeezed dot");
        assert!(grey(100, 80 + 19) < 40, "past the squeezed dot");
    }

    #[test]
    fn its_shadow_softens_past_its_edge() {
        assert!((shadow_at(0.0, 8.0) - 0.5).abs() < 1e-3);
        assert!(shadow_at(-24.0, 8.0) > 0.99);
        assert!(shadow_at(24.0, 8.0) < 0.01);
        assert!(shadow_at(4.0, 8.0) < shadow_at(2.0, 8.0));
    }
}
