//! The game's own picture of its world, behind its world map (`/material/ui/map/map0..3`, four 2048-pixel quarters
//! down each column first: north-west, south-west, north-east, south-east): ATS's is North America, its sea grey,
//! land black and the game's states dark grey, each within its border lines. It says what the roads don't: what is
//! sea and what is land beyond the game's world (Canada, Mexico, the Gulf, the Pacific) and across the states the
//! player hasn't. The map's sea (sea.rs, ATS's nearest rule) is held to it, and beyond the map's area the cab draws
//! its land (`world.json`).
//!
//! The game places it from its code (`/material/ui/map/map%u.mat`); nothing in its files says where, so where is
//! measured (`examples/ui_map_fit`, 2026-10-28, ATS 1.61.3.18): the placement at which each state's roads fall in
//! the picture's region for it and the Pacific on its sea. 95.6% of the roads' ground lies on the game's states
//! there, 0.2% on sea, 98.8% of the open sea on sea, every state's nodes 83-99% in their own region (the rest on its
//! border lines: the picture is drawn, not surveyed, its borders and coasts good to about a kilometre). A rounder
//! placement (64 m a pixel round the world's centre) fits worse (1.9% of the roads on sea). Each export checks it
//! still fits the player's map ([`WorldPicture::fits`]) before trusting it.

use crate::files::{FileError, GameFiles};
use crate::geometry::Point;
use crate::sea::{self, Cell, SeaGrid, Shape};

/// Where a picture lies in the world: pixel (0, 0)'s corner, and metres a pixel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub origin: Point,
    pub scale: f32,
}

/// ATS's (measured: the module's notes).
pub const USA_PLACEMENT: Placement = Placement { origin: [-132_660.14, -132_527.67], scale: 64.121_06 };

/// The share of a map's road nodes that must lie off the picture's sea for it to be taken as placed right.
pub const FITS: f32 = 0.98;

/// The picture's placement for the map `map`, if it's been measured (ATS's base map's; ETS2's not yet).
pub fn placement_of(map: &str) -> Option<Placement> {
    (map == "usa").then_some(USA_PLACEMENT)
}

/// The picture as sea and land, with how far each pixel is from the other.
pub struct WorldPicture {
    pub placement: Placement,
    /// Pixels a side.
    pub size: usize,
    /// Row by row from the north-west: sea or not.
    sea: Vec<bool>,
    /// Each pixel's distance to the nearest sea pixel and to the nearest land pixel, in pixels (by steps across
    /// and diagonally, as near as it needs to be: a pixel's 64 m), up to `u16::MAX`.
    to_sea: Vec<u16>,
    to_land: Vec<u16>,
}

/// The picture's sea's grey (155, its commonest after black; the coasts' and lines' anti-aliasing round it).
const SEA_GREY: std::ops::RangeInclusive<u8> = 140..=170;
/// How thick a line drawn in the sea's grey (a state's border) may be and not be taken for sea, pixels each side.
const LINE: usize = 2;

impl std::fmt::Debug for WorldPicture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WorldPicture({0}×{0}, {1:?})", self.size, self.placement)
    }
}

impl WorldPicture {
    /// The game's picture for the map `map`, placed (`None`: no placement known, or the game has no picture).
    pub fn read(files: &GameFiles, map: &str) -> Result<Option<Self>, FileError> {
        let Some(placement) = placement_of(map) else { return Ok(None) };
        let mut quarters = Vec::new();
        for i in 0..4 {
            match files.read_texture(&format!("/material/ui/map/map{i}.tobj"))? {
                Some(texture) => quarters.push(texture),
                None => return Ok(None),
            }
        }
        let half = quarters[0].width as usize;
        if quarters.iter().any(|q| q.width as usize != half || q.height as usize != half) {
            return Ok(None);
        }
        let size = half * 2;
        let mut grey = vec![0u8; size * size];
        for (i, quarter) in quarters.iter().enumerate() {
            let (ox, oy) = ((i / 2) * half, (i % 2) * half);
            for y in 0..half {
                for x in 0..half {
                    let p = (y * half + x) * 4;
                    let sum = quarter.rgba[p] as u16 + quarter.rgba[p + 1] as u16 + quarter.rgba[p + 2] as u16;
                    grey[(oy + y) * size + ox + x] = (sum / 3) as u8;
                }
            }
        }
        Ok(Some(Self::from_grey(&grey, size, placement)))
    }

    /// From the picture's greys (`size` a side): its sea the sea's grey, opened by [`LINE`] (worn away and grown
    /// back) so the border lines drawn in that grey are land.
    pub fn from_grey(grey: &[u8], size: usize, placement: Placement) -> Self {
        let raw: Vec<bool> = grey.iter().map(|g| SEA_GREY.contains(g)).collect();
        let worn = square_filter(&raw, size, LINE, true);
        let sea = square_filter(&worn, size, LINE, false);
        let to_sea = distances(&sea, size, true);
        let to_land = distances(&sea, size, false);
        Self { placement, size, sea, to_sea, to_land }
    }

    /// The pixel at the world point `[x, z]`, if it's on the picture.
    fn pixel(&self, x: f32, z: f32) -> Option<usize> {
        let px = (x - self.placement.origin[0]) / self.placement.scale;
        let py = (z - self.placement.origin[1]) / self.placement.scale;
        (px >= 0.0 && py >= 0.0 && (px as usize) < self.size && (py as usize) < self.size)
            .then(|| py as usize * self.size + px as usize)
    }

    /// Sea at `[x, z]` (beyond the picture, sea: the cab's map draws sea there).
    pub fn sea_at(&self, x: f32, z: f32) -> bool {
        self.pixel(x, z).is_none_or(|i| self.sea[i])
    }

    /// Sea within `reach` metres of `[x, z]`.
    pub fn sea_within(&self, x: f32, z: f32, reach: f32) -> bool {
        self.pixel(x, z).is_none_or(|i| self.to_sea[i] as f32 * self.placement.scale <= reach)
    }

    /// Sea at `[x, z]` and all round it, `reach` metres out.
    pub fn deep_sea(&self, x: f32, z: f32, reach: f32) -> bool {
        self.pixel(x, z).is_none_or(|i| self.sea[i] && self.to_land[i] as f32 * self.placement.scale > reach)
    }

    /// The share of `points` (a map's road nodes) off the picture's sea: whether it's placed right for the map.
    pub fn fits(&self, points: impl IntoIterator<Item = Point>) -> f32 {
        let (mut on_land, mut all) = (0usize, 0usize);
        for [x, z] in points {
            all += 1;
            on_land += !self.sea_at(x, z) as usize;
        }
        if all == 0 { 0.0 } else { on_land as f32 / all as f32 }
    }

    /// Where the picture lies on the ground plane (min x, min z, max x, max z).
    pub fn bounds(&self) -> [f32; 4] {
        let Placement { origin, scale } = self.placement;
        [origin[0], origin[1], origin[0] + self.size as f32 * scale, origin[1] + self.size as f32 * scale]
    }

    /// The picture's land as smooth shapes on the ground plane, as the map's own land is traced (sea.rs).
    pub fn land_shapes(&self, tolerance: f32) -> Vec<Shape> {
        sea::land_shapes(&self.grid(false), tolerance)
    }

    /// The picture's sea as smooth shapes, traced as its land is, carried on a pixel past the picture's edges (to
    /// meet the sea drawn beyond it): what the cab draws over its own ground, so that what hasn't loaded yet shows
    /// as land, as before, not as sea (seen 2026-10-28: the land drawn over sea everywhere came in after it,
    /// zoomed in, a flash of sea).
    pub fn sea_shapes(&self, tolerance: f32) -> Vec<Shape> {
        sea::land_shapes_carried_on(&self.grid(true), tolerance, &mut |_| {})
    }

    /// The picture as a sea grid of its pixels: its land as land, or (`sea_as_land`) its sea, to trace either.
    fn grid(&self, sea_as_land: bool) -> SeaGrid {
        let mut grid = SeaGrid::new(self.bounds(), self.placement.scale);
        // The grid's cells are the picture's pixels (its bounds a whole number of them).
        debug_assert_eq!((grid.width, grid.height), (self.size, self.size));
        for (cell, sea) in grid.cells.iter_mut().zip(&self.sea) {
            *cell = if *sea != sea_as_land { Cell::Sea } else { Cell::Land };
        }
        grid
    }

    /// Where the picture's land is cut off (min x, min z, max x, max z on the ground plane, each `None` where it
    /// isn't): the picture is drawn inside a frame of sea's grey (ATS's 3 pixels, measured 2026-11-04), and on a side
    /// whose land runs to the frame (Canada's north, Mexico's south, the Atlantic coast's east) the land stops there,
    /// cut straight, though the world goes on. Nothing in the game says what lies beyond, so nothing is drawn there:
    /// the cab's camera stops at the cut (owner 2026-10-30: the world isn't infinite, the camera is restricted).
    /// Carrying the land on straight past it drew every coast and island the edge crosses as a band for ever (owner
    /// 2026-11-04: "weird infinite land"). Where the sea runs to the edge (the Pacific), `None`: the sea beyond is the
    /// picture's sea going on. Specks thinner than a border line ([`LINE`]: the anti-aliasing a cut leaves in the
    /// frame) aren't land here.
    pub fn cut(&self) -> [Option<f32>; 4] {
        let Placement { origin, scale } = self.placement;
        let n = self.size;
        let cleaned = self.land_without_specks();
        let land = |x: usize, y: usize| cleaned[y * n + x];
        let row = |y: usize| (0..n).any(|x| land(x, y));
        let column = |x: usize| (0..n).any(|y| land(x, y));
        // Each side's outer lines without land: the frame where the land runs to it, more where the sea does.
        let empty = |line: &dyn Fn(usize) -> bool| (0..n).take_while(|&k| !line(k)).count();
        let sides = [empty(&|k| column(k)), empty(&|k| row(k)), empty(&|k| column(n - 1 - k)), empty(&|k| row(n - 1 - k))];
        let Some(frame) = sides.iter().copied().min().filter(|frame| *frame < n / 2) else { return [None; 4] };
        let at = |side: usize| {
            let along = if side.is_multiple_of(2) { origin[0] } else { origin[1] };
            let pixels = if side < 2 { frame } else { n - frame };
            (sides[side] == frame).then_some(along + pixels as f32 * scale)
        };
        [at(0), at(1), at(2), at(3)]
    }

    /// The picture's land with what's thinner than a border line ([`LINE`]) worn away: anti-aliasing specks.
    fn land_without_specks(&self) -> Vec<bool> {
        let land: Vec<bool> = self.sea.iter().map(|sea| !sea).collect();
        let worn = square_filter(&land, self.size, LINE, true);
        square_filter(&worn, self.size, LINE, false)
    }
}

/// Each pixel `keep` only if every pixel within `reach` (a square) is (wearing away the others), else it's the
/// opposite: `keep = true` wears `mask` away, `false` grows it.
fn square_filter(mask: &[bool], size: usize, reach: usize, keep: bool) -> Vec<bool> {
    // Rows first, then columns: a square's minimum (or maximum) is its rows' over its columns'.
    let pass = |source: &[bool], along_rows: bool| {
        let mut out = vec![false; source.len()];
        for line in 0..size {
            for at in 0..size {
                let lo = at.saturating_sub(reach);
                let hi = (at + reach).min(size - 1);
                let index = |k: usize| if along_rows { line * size + k } else { k * size + line };
                let target = if along_rows { line * size + at } else { at * size + line };
                out[target] =
                    if keep { (lo..=hi).all(|k| source[index(k)]) } else { (lo..=hi).any(|k| source[index(k)]) };
            }
        }
        out
    };
    pass(&pass(mask, true), false)
}

/// Each pixel's distance to the nearest pixel where `mask` is `to`, in pixels (a two-pass chamfer: 1 across,
/// 1.4 diagonally, rounded to tenths), `u16::MAX` where there's none.
fn distances(mask: &[bool], size: usize, to: bool) -> Vec<u16> {
    const FAR: u32 = u32::MAX / 2;
    let mut d: Vec<u32> = mask.iter().map(|m| if *m == to { 0 } else { FAR }).collect();
    let (straight, diagonal) = (10u32, 14u32);
    for y in 0..size {
        for x in 0..size {
            let i = y * size + x;
            let mut best = d[i];
            if x > 0 {
                best = best.min(d[i - 1] + straight);
            }
            if y > 0 {
                best = best.min(d[i - size] + straight);
                if x > 0 {
                    best = best.min(d[i - size - 1] + diagonal);
                }
                if x + 1 < size {
                    best = best.min(d[i - size + 1] + diagonal);
                }
            }
            d[i] = best;
        }
    }
    for y in (0..size).rev() {
        for x in (0..size).rev() {
            let i = y * size + x;
            let mut best = d[i];
            if x + 1 < size {
                best = best.min(d[i + 1] + straight);
            }
            if y + 1 < size {
                best = best.min(d[i + size] + straight);
                if x + 1 < size {
                    best = best.min(d[i + size + 1] + diagonal);
                }
                if x > 0 {
                    best = best.min(d[i + size - 1] + diagonal);
                }
            }
            d[i] = best;
        }
    }
    d.into_iter().map(|v| if v >= FAR { u16::MAX } else { (v / 10).min(u16::MAX as u32 - 1) as u16 }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 40-pixel picture: sea on the left half, land on the right, a 2-pixel border line in the sea's grey
    /// across the land, and a lake of 3 pixels.
    fn picture() -> WorldPicture {
        let size = 40;
        let mut grey = vec![0u8; size * size];
        for y in 0..size {
            for x in 0..size {
                let i = y * size + x;
                grey[i] = if x < 20 {
                    155
                } else if y == 20 || y == 21 {
                    156 // a border line
                } else if (30..33).contains(&x) && (5..8).contains(&y) {
                    155 // a lake, 3 pixels across
                } else {
                    53
                };
            }
        }
        WorldPicture::from_grey(&grey, size, Placement { origin: [0.0, 0.0], scale: 10.0 })
    }

    #[test]
    fn the_sea_without_its_border_lines() {
        let p = picture();
        assert!(p.sea_at(50.0, 205.0), "the sea");
        assert!(!p.sea_at(300.0, 205.0), "a border line across the land is land");
        assert!(!p.sea_at(315.0, 65.0), "so is a lake thinner than a line's width");
        assert!(!p.sea_at(300.0, 300.0), "the land");
        assert!(p.sea_at(-5.0, 10.0), "beyond the picture, sea");
    }

    #[test]
    fn how_far_the_sea_is() {
        let p = picture();
        assert!(p.sea_within(250.0, 100.0, 60.0), "5 pixels from the coast");
        assert!(!p.sea_within(350.0, 100.0, 60.0), "15 pixels from it");
        assert!(p.deep_sea(50.0, 100.0, 100.0), "15 pixels out");
        assert!(!p.deep_sea(180.0, 100.0, 100.0), "2 pixels out");
    }

    #[test]
    fn fits_a_map_whose_roads_are_on_its_land() {
        let p = picture();
        assert_eq!(p.fits([[300.0, 300.0], [250.0, 100.0]]), 1.0);
        assert_eq!(p.fits([[300.0, 300.0], [50.0, 100.0]]), 0.5);
    }

    #[test]
    fn cut_where_its_land_runs_off() {
        // The land half (x 200..400) runs off the top, the bottom and the right edge; the sea off the left.
        assert_eq!(picture().cut(), [None, Some(0.0), Some(400.0), Some(400.0)]);
    }

    /// The game's picture is drawn in a frame of sea's grey (ATS's 3 pixels), its land cut at the frame's inner edge,
    /// with a speck of the cut's anti-aliasing in the frame: cut at the frame, the speck not land.
    #[test]
    fn cut_inside_its_frame() {
        let size = 40;
        let frame = 3;
        let mut grey = vec![155u8; size * size];
        for y in frame..size - frame {
            for x in 20..size - frame {
                grey[y * size + x] = 53;
            }
        }
        // Anti-aliasing in the frame, by the cut: two pixels out of the sea's grey, beside the land's.
        grey[10 * size + size - frame] = 130;
        grey[11 * size + size - frame] = 130;
        let p = WorldPicture::from_grey(&grey, size, Placement { origin: [0.0, 0.0], scale: 10.0 });
        assert_eq!(p.cut(), [None, Some(30.0), Some(370.0), Some(370.0)]);
    }

    #[test]
    fn its_sea_traced_past_its_edges() {
        let shapes = picture().sea_shapes(5.0);
        assert_eq!(shapes.len(), 1, "one sea");
        let (min, max) = shapes[0].outline.iter().fold((f32::MAX, f32::MIN), |m, p| (m.0.min(p[0]), m.1.max(p[0])));
        assert!(min < -5.0, "past the picture's west edge: {min}");
        assert!((190.0..=210.0).contains(&max), "the coast at x 200: {max}");
    }

    #[test]
    fn its_land_traced() {
        let shapes = picture().land_shapes(5.0);
        assert_eq!(shapes.len(), 1, "one land, the line and the lake in it");
        let xs: Vec<f32> = shapes[0].outline.iter().map(|p| p[0]).collect();
        let (min, max) = xs.iter().fold((f32::MAX, f32::MIN), |m, x| (m.0.min(*x), m.1.max(*x)));
        assert!((190.0..=210.0).contains(&min), "the coast at x 200: {min}");
        // To the picture's edge, less the half pixel the tracer closes outlines in (sea beyond, as here).
        assert!(max >= 395.0, "to the picture's edge: {max}");
    }
}
