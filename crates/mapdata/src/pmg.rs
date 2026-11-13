//! A model's geometry (`.pmg`) as the games load it: the current format (0x15) and the older ones map mods still ship
//! (0x14, 0x13, 0x12), which hold the same things at other places. Read 2026-11-11 from the Oyuncuyusbis map, 702 of
//! whose 4,523 models are older: read as the current format, one piece's indices came out as a billion and the map's
//! build asked for 13 GB at once. Checked by `examples/pmg_survey`: every piece's indices in its file and on its
//! vertices, every vertex inside its piece's own box, every material one its descriptor has.
//!
//! The first word is the format's version, then "gmP". After it, every format has its piece count (+4) and part count
//! (+8); then counts that differ (more in the newer), the model's sphere and box, and where its tables are. A piece's
//! record holds its index and vertex counts, its material, its box, and where its vertices and indices are:
//!
//! | format | box | parts, locators, pieces | piece record | material, box | vertices | indices |
//! |---|---|---|---|---|---|---|
//! | 0x15 | 48 | 76, 80, 84 | 100 | 16, 36 | interleaved, stride at 60, positions at 64, colours at 76 | 96 |
//! | 0x14 | 40 | 68, 72, 76 | 100 | 16, 36 | as 0x15 | 96 |
//! | 0x13 | 36 | 64, 68, 72 | 104 | 20, 40 | interleaved in blocks, no stride stored (below) | 88 |
//! | 0x12 | 36 | 64, 68, 72 | 104 | 20, 40 | each its own array (positions 12 bytes each, colours 4) | 88 |
//!
//! In every format, a piece's record says where its positions (+64), normals (+68), texture coordinates (+72) and
//! colours (+76) start (all ones: it has none). 0x13 interleaves them in blocks with no stride stored: positions and
//! normals in one, colours and coordinates in another (a flag's), or all four in one (a road's, a billboard's). A
//! block's vertices run on to where the next array starts (another piece's, or the indices'), its stride a whole
//! share of that.
//!
//! A part's record (24 bytes, every format): its name (a token), its pieces' count and first, its locators' count and
//! first. A locator's (44 bytes): its name, its position at +8, its rotation at +24.

use std::ops::Range;

use crate::reader::word;

/// The current format's first word.
pub const CURRENT: u32 = 0x506d_6715;
const OLDEST: u32 = 0x506d_6712;

const PIECE_COUNT_AT: usize = 4;
const PART_COUNT_AT: usize = 8;
const PART_SIZE: usize = 24;
pub const LOCATOR_SIZE: usize = 44;
/// "No such array" in a piece's record.
const NONE: u32 = u32::MAX;
/// How far a vertex may lie outside its piece's stored box (m, and a share of the box's size) for the box to hold it.
pub const BOX_SLACK: f32 = 0.01;

/// How a format's pieces keep their vertices.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Vertices {
    /// Interleaved, their stride in the record at this place.
    Stored(usize),
    /// Interleaved in blocks, their strides not stored ([`Pmg::block_stride`]).
    Block,
    /// Each attribute its own array.
    Separate,
}

#[derive(Debug)]
struct Layout {
    box_at: usize,
    parts_at: usize,
    locators_at: usize,
    pieces_at: usize,
    piece_size: usize,
    material: usize,
    piece_box: usize,
    vertices: Vertices,
    positions: usize,
    normals: usize,
    coordinates: usize,
    colours: usize,
    /// More arrays a piece's record may set (in the older formats, +84: 16 bytes a vertex, in the vertex, in 1,154
    /// pieces of the Oyuncuyusbis map, 2026-11-12, examples/pmg_layouts).
    more: &'static [usize],
    indices: usize,
}

const V15: Layout = Layout {
    box_at: 48,
    parts_at: 76,
    locators_at: 80,
    pieces_at: 84,
    piece_size: 100,
    material: 16,
    piece_box: 36,
    vertices: Vertices::Stored(60),
    positions: 64,
    normals: 68,
    coordinates: 72,
    colours: 76,
    more: &[],
    indices: 96,
};
const V14: Layout = Layout { box_at: 40, parts_at: 68, locators_at: 72, pieces_at: 76, ..V15 };
const V13: Layout = Layout {
    box_at: 36,
    parts_at: 64,
    locators_at: 68,
    pieces_at: 72,
    piece_size: 104,
    material: 20,
    piece_box: 40,
    vertices: Vertices::Block,
    positions: 64,
    normals: 68,
    coordinates: 72,
    colours: 76,
    more: &[80, 84],
    indices: 88,
};
const V12: Layout = Layout { vertices: Vertices::Separate, ..V13 };

/// A model's geometry, read in its own format.
pub struct Pmg<'a> {
    data: &'a [u8],
    layout: &'static Layout,
    pub version: u32,
}

/// One of a model's pieces (a mesh of one material).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub indices: usize,
    pub vertices: usize,
    pub material: usize,
    /// Its box: its least corner, its greatest (metres, the model's axes).
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// Where its vertices' positions start, and from one to the next (bytes).
    pub positions: usize,
    pub stride: usize,
    /// Where its vertices' colours start (each four bytes, alpha last), and from one to the next; `None`: it has none.
    pub colours: Option<usize>,
    pub colour_stride: usize,
    /// Where its vertices' first texture coordinates start (two floats), and from one to the next; `None`: it has none,
    /// or its format's arrays don't say how far apart they are (0x12's, each its own: none read takes them).
    pub coordinates: Option<usize>,
    pub coordinate_stride: usize,
    /// Where its indices start (u16 each, a triangle list).
    pub index_at: usize,
}

/// One of a model's locators, as its record holds it ([`LOCATOR_SIZE`] bytes: its name, its position at +8, its scale
/// at +20, its rotation at +24).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Locator {
    /// A token.
    pub name: u64,
    /// Metres, the model's own axes (x right, y up, z back).
    pub position: [f32; 3],
    /// A quaternion, w first.
    pub rotation: [f32; 4],
}

/// One of a model's parts: a named group of its pieces and locators.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub name: Option<u64>,
    pub pieces: Range<usize>,
    pub locators: Range<usize>,
}

impl<'a> Pmg<'a> {
    /// `data` as a model's geometry, if it's one of the formats read (its box reads whatever its tables are: a model whose
    /// pieces are damaged is still its box).
    pub fn read(data: &'a [u8]) -> Option<Self> {
        let version = word(data, 0)?;
        let layout = match version {
            0x506d_6715 => &V15,
            0x506d_6714 => &V14,
            0x506d_6713 => &V13,
            0x506d_6712 => &V12,
            _ => return None,
        };
        debug_assert!((OLDEST..=CURRENT).contains(&version));
        Some(Self { data, layout, version })
    }

    /// `count` records of `size` bytes at the place the word at `table_at` gives: whether they lie within the file.
    fn table_within(&self, table_at: usize, count: usize, size: usize) -> bool {
        count == 0
            || self
                .at(table_at)
                .and_then(|start| start.checked_add(count.checked_mul(size)?))
                .is_some_and(|end| end <= self.data.len())
    }

    fn word(&self, at: usize) -> Option<u32> {
        word(self.data, at)
    }

    fn at(&self, at: usize) -> Option<usize> {
        self.word(at).map(|value| value as usize)
    }

    fn float(&self, at: usize) -> Option<f32> {
        self.word(at).map(f32::from_bits)
    }

    fn corner(&self, at: usize) -> Option<[f32; 3]> {
        Some([self.float(at)?, self.float(at + 4)?, self.float(at + 8)?])
    }

    /// How many pieces it has; `None` if their records don't lie within the file (the count is the file's word: a
    /// damaged or misread one would size what's read of it by anything, a billion pieces and the build gone: the
    /// review of 2026-11-11, C4a).
    pub fn piece_count(&self) -> Option<usize> {
        let count = self.at(PIECE_COUNT_AT)?;
        self.table_within(self.layout.pieces_at, count, self.layout.piece_size).then_some(count)
    }

    /// How many parts it has; `None` if their records don't lie within the file (as [`Self::piece_count`]).
    pub fn part_count(&self) -> Option<usize> {
        let count = self.at(PART_COUNT_AT)?;
        self.table_within(self.layout.parts_at, count, PART_SIZE).then_some(count)
    }

    /// The model's box: its least corner, its greatest.
    pub fn model_box(&self) -> Option<([f32; 3], [f32; 3])> {
        Some((self.corner(self.layout.box_at)?, self.corner(self.layout.box_at + 12)?))
    }

    /// Where its locators' records are.
    pub fn locators_at(&self) -> Option<usize> {
        self.at(self.layout.locators_at)
    }

    pub fn part(&self, part: usize) -> Option<Part> {
        let record = self.at(self.layout.parts_at)?.checked_add(part.checked_mul(PART_SIZE)?)?;
        let name =
            Some(u64::from(self.word(record)?) | u64::from(self.word(record + 4)?) << 32).filter(|&name| name != 0);
        let range = |count: usize, first: usize| first..first.saturating_add(count);
        Some(Part {
            name,
            pieces: range(self.at(record + 8)?, self.at(record + 12)?),
            locators: range(self.at(record + 16)?, self.at(record + 20)?),
        })
    }

    /// The material of piece `piece` (its place in the descriptor's materials).
    pub fn material(&self, piece: usize) -> Option<usize> {
        self.at(self.record(piece)? + self.layout.material)
    }

    fn record(&self, piece: usize) -> Option<usize> {
        self.at(self.layout.pieces_at)?.checked_add(piece.checked_mul(self.layout.piece_size)?)
    }

    pub fn piece(&self, piece: usize) -> Option<Piece> {
        let layout = self.layout;
        let record = self.record(piece)?;
        let field = |at: usize| self.word(record + at);
        let (indices, vertices) = (field(0)? as usize, field(4)? as usize);
        let positions = field(layout.positions)? as usize;
        let colours = field(layout.colours)?;
        let coordinates = field(layout.coordinates)?;
        let (stride, colour_stride, coordinates) = match layout.vertices {
            Vertices::Stored(at) => {
                let stride = field(at)? as usize;
                (stride, stride, (coordinates != NONE).then_some((coordinates as usize, stride)))
            }
            Vertices::Block => {
                let blocks = self.blocks(piece, vertices)?;
                let stride_of = |at: usize| {
                    blocks.iter().find(|&&(start, stride)| (start..start + stride).contains(&at)).map(|block| block.1)
                };
                let colour_stride = if colours == NONE { 0 } else { stride_of(colours as usize)? };
                // Never the piece's failing: they're no part of its shape.
                let coordinates = (coordinates != NONE)
                    .then(|| stride_of(coordinates as usize).map(|stride| (coordinates as usize, stride)))
                    .flatten();
                (stride_of(positions)?, colour_stride, coordinates)
            }
            Vertices::Separate => (12, 4, None),
        };
        Some(Piece {
            indices,
            vertices,
            material: field(layout.material)? as usize,
            min: self.corner(record + layout.piece_box)?,
            max: self.corner(record + layout.piece_box + 12)?,
            positions,
            stride,
            colours: (colours != NONE).then_some(colours as usize),
            colour_stride,
            coordinates: coordinates.map(|(at, _)| at),
            coordinate_stride: coordinates.map_or(0, |(_, stride)| stride),
            index_at: field(layout.indices)? as usize,
        })
    }

    /// The starts of the arrays a piece's record at `record` sets, lowest first.
    fn arrays_of(&self, record: usize) -> Option<Vec<usize>> {
        let layout = self.layout;
        let fields = [layout.positions, layout.normals, layout.coordinates, layout.colours];
        let mut arrays = Vec::new();
        for &field in fields.iter().chain(layout.more) {
            match self.word(record + field)? {
                NONE => {}
                at => arrays.push(at as usize),
            }
        }
        arrays.sort_unstable();
        arrays.dedup();
        Some(arrays)
    }

    /// The blocks `piece`'s `vertices` are interleaved in, in a format that doesn't store their strides: each its start
    /// and its stride. A block starts at the lowest of its arrays not in one before it, and runs on to where an array
    /// that isn't its own starts: another piece's, the indices, the file's end, or one of its own past it. Of those
    /// ends, the furthest whose span is a whole number of vertices, each of its arrays within the first, is its end
    /// (with one vertex, any is: then they read alike). Whatever array comes first in a block (the review of
    /// 2026-11-11, M13: colours were taken to start theirs).
    fn blocks(&self, piece: usize, vertices: usize) -> Option<Vec<(usize, usize)>> {
        if vertices == 0 {
            return None;
        }
        let layout = self.layout;
        let own = self.arrays_of(self.record(piece)?)?;
        // Where others' arrays and every piece's indices start: no block of this piece runs past one.
        let mut others = vec![self.data.len()];
        for other in 0..self.piece_count()? {
            let record = self.record(other)?;
            others.push(self.at(record + layout.indices)?);
            if other != piece {
                others.extend(self.arrays_of(record)?);
            }
        }
        let mut blocks = Vec::new();
        let mut next = 0;
        while let Some(&start) = own.get(next) {
            let foreign = others.iter().copied().filter(|&at| at > start).min()?;
            let mut ends: Vec<usize> = own.iter().copied().filter(|&at| at > start && at < foreign).collect();
            ends.push(foreign);
            let fits = |end: usize| {
                let span = end - start;
                let stride = span / vertices;
                let within = own.iter().filter(|&&at| at >= start && at < end).all(|&at| at - start + 4 <= stride);
                (span.is_multiple_of(vertices) && stride >= 4 && within).then_some(stride)
            };
            let (end, stride) = ends.iter().rev().find_map(|&end| Some((end, fits(end)?)))?;
            blocks.push((start, stride));
            next = own.iter().position(|&at| at >= end).unwrap_or(own.len());
        }
        Some(blocks)
    }

    /// Vertex `index` of `piece`'s position; `None` past its vertices or the file.
    pub fn position(&self, piece: &Piece, index: usize) -> Option<[f32; 3]> {
        if index >= piece.vertices {
            return None;
        }
        self.corner(piece.positions.checked_add(index.checked_mul(piece.stride)?)?)
    }

    /// Vertex `index` of `piece`'s first texture coordinates (u, v); `None` past its vertices or the file, or where
    /// they aren't read ([`Piece::coordinates`]).
    pub fn coordinate(&self, piece: &Piece, index: usize) -> Option<[f32; 2]> {
        if index >= piece.vertices {
            return None;
        }
        let at = piece.coordinates?.checked_add(index.checked_mul(piece.coordinate_stride)?)?;
        Some([self.float(at)?, self.float(at + 4)?])
    }

    /// Locator `index` (as a part's [`Part::locators`] counts them); `None` past the file.
    pub fn locator(&self, index: usize) -> Option<Locator> {
        let at = self.locators_at()?.checked_add(index.checked_mul(LOCATOR_SIZE)?)?;
        Some(Locator {
            name: u64::from(self.word(at)?) | u64::from(self.word(at + 4)?) << 32,
            position: self.corner(at + 8)?,
            rotation: [self.float(at + 24)?, self.float(at + 28)?, self.float(at + 32)?, self.float(at + 36)?],
        })
    }

    /// Vertex `index` of `piece`'s alpha (0 to 1); 1 where it has no colours.
    pub fn alpha(&self, piece: &Piece, index: usize) -> f32 {
        piece
            .colours
            .and_then(|colours| colours.checked_add(index.checked_mul(piece.colour_stride)?)?.checked_add(3))
            .and_then(|at| self.data.get(at))
            .map_or(1.0, |&alpha| f32::from(alpha) / 255.0)
    }

    /// Index `which` of `piece`'s list; `None` past it or the file.
    pub fn index(&self, piece: &Piece, which: usize) -> Option<usize> {
        if which >= piece.indices {
            return None;
        }
        let at = piece.index_at.checked_add(which.checked_mul(2)?)?;
        Some(u16::from_le_bytes(self.data.get(at..at.checked_add(2)?)?.try_into().ok()?) as usize)
    }

    /// `piece`'s box (least corner, greatest): the one stored if it holds every vertex ([`BOX_SLACK`]), otherwise
    /// the vertices' own (some mods' exporters left a mirrored piece's box as it was: the Oyuncuyusbis map's snow
    /// roads, 0x13, their x the other way round).
    pub fn bounds(&self, piece: &Piece) -> ([f32; 3], [f32; 3]) {
        let size = (0..3).map(|axis| piece.max[axis] - piece.min[axis]).fold(0.0f32, f32::max);
        let slack = BOX_SLACK + BOX_SLACK * size;
        let corners = || (0..self.held_vertices(piece)).filter_map(|index| self.position(piece, index));
        let held = corners()
            .all(|c| (0..3).all(|axis| c[axis] >= piece.min[axis] - slack && c[axis] <= piece.max[axis] + slack));
        if held || piece.vertices == 0 {
            return (piece.min, piece.max);
        }
        corners().fold(([f32::MAX; 3], [f32::MIN; 3]), |(mut min, mut max), c| {
            for axis in 0..3 {
                min[axis] = min[axis].min(c[axis]);
                max[axis] = max[axis].max(c[axis]);
            }
            (min, max)
        })
    }

    /// How many of `piece`'s vertices have their positions in the file (its count is the file's word: walked as said, a
    /// damaged one ran a loop of billions over nothing, the fuzzing of 2026-11-12, examples/reader_fuzz).
    pub fn held_vertices(&self, piece: &Piece) -> usize {
        let Some(room) = self.data.len().checked_sub(piece.positions.saturating_add(12)) else { return 0 };
        match piece.stride {
            0 => piece.vertices.min(1),
            stride => piece.vertices.min(room / stride + 1),
        }
    }

    /// `piece`'s triangles, each three corners (x, y, z, metres: the model's own axes), as far as the file holds its
    /// indices (their count is the file's word); a triangle a corner of which doesn't read, or isn't finite, left out.
    pub fn triangles(&self, piece: &Piece) -> Vec<[[f32; 3]; 3]> {
        let vertex = |which: usize| -> Option<[f32; 3]> {
            self.position(piece, self.index(piece, which)?).filter(|corner| corner.iter().all(|v| v.is_finite()))
        };
        (0..self.held_indices(piece) / 3 * 3)
            .step_by(3)
            .filter_map(|first| Some([vertex(first)?, vertex(first + 1)?, vertex(first + 2)?]))
            .collect()
    }

    /// How many of `piece`'s indices the file holds (a damaged count would say anything).
    pub fn held_indices(&self, piece: &Piece) -> usize {
        piece.indices.min(self.data.len().saturating_sub(piece.index_at) / 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(data: &mut Vec<u8>, at: usize, value: u32) {
        if data.len() < at + 4 {
            data.resize(at + 4, 0);
        }
        data[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_f(data: &mut Vec<u8>, at: usize, value: f32) {
        put(data, at, value.to_bits());
    }

    /// Piece `p`'s vertex `v`: x = 10 p + v, y = v, z = −v; its alpha 10 p + v.
    fn corner(p: usize, v: usize) -> [f32; 3] {
        [(10 * p + v) as f32, v as f32, -(v as f32)]
    }

    /// An old-format model (0x13 or 0x12) of `pieces` pieces of three vertices and one triangle each, its vertices laid
    /// out by `layout`: each piece's arrays' starts (positions, normals, coordinates, colours) and strides
    /// (positions', colours').
    fn old_model(version: u32, pieces: usize, layout: impl Fn(usize) -> ([u32; 4], usize, usize)) -> Vec<u8> {
        let mut data = Vec::new();
        put(&mut data, 0, version);
        put(&mut data, 4, pieces as u32);
        put(&mut data, 72, 200);
        let mut indices_at = 0usize;
        for p in 0..pieces {
            let ([positions, _, coordinates, colours], stride, colour_stride) = layout(p);
            for v in 0..3 {
                let at = positions as usize + v * stride;
                for (axis, value) in corner(p, v).into_iter().enumerate() {
                    put_f(&mut data, at + 4 * axis, value);
                }
                if colours != NONE {
                    put(&mut data, colours as usize + v * colour_stride, ((10 * p + v) as u32) << 24 | 0x80_8080);
                }
            }
            // Whole vertices: the indices after the last one's last byte, each block's (colours in a block of their own,
            // starting with them or with the coordinates before them, or in the positions').
            indices_at = indices_at.max(positions as usize + 3 * stride);
            if colours != NONE && !(positions..positions + stride as u32).contains(&colours) {
                let apart = |at: u32| at != NONE && !(positions..positions + stride as u32).contains(&at);
                let start = if apart(coordinates) { colours.min(coordinates) } else { colours };
                indices_at = indices_at.max(start as usize + 3 * colour_stride);
            }
        }
        data.resize(data.len().max(indices_at), 0);
        for p in 0..pieces {
            let record = 200 + 104 * p;
            let (arrays, _, _) = layout(p);
            put(&mut data, record, 3);
            put(&mut data, record + 4, 3);
            put(&mut data, record + 20, p as u32);
            for (i, at) in arrays.into_iter().enumerate() {
                put(&mut data, record + 64 + 4 * i, at);
            }
            // The record's other array fields unset, as a real file's are (all ones, never 0).
            put(&mut data, record + 80, NONE);
            put(&mut data, record + 84, NONE);
            let at = indices_at + 6 * p;
            put(&mut data, record + 88, at as u32);
            for (k, index) in [0u16, 1, 2].into_iter().enumerate() {
                if data.len() < at + 2 * k + 2 {
                    data.resize(at + 2 * k + 2, 0);
                }
                data[at + 2 * k..at + 2 * k + 2].copy_from_slice(&index.to_le_bytes());
            }
        }
        data
    }

    fn check(data: &[u8], pieces: usize, strides: (usize, usize)) {
        let model = Pmg::read(data).expect("a format read");
        assert_eq!(model.piece_count(), Some(pieces));
        for p in 0..pieces {
            let piece = model.piece(p).expect("its record");
            assert_eq!((piece.stride, piece.colour_stride, piece.material), (strides.0, strides.1, p));
            for v in 0..3 {
                assert_eq!(model.index(&piece, v), Some(v));
                assert_eq!(model.position(&piece, v), Some(corner(p, v)));
                assert_eq!(model.alpha(&piece, v), (10 * p + v) as f32 / 255.0);
            }
            assert_eq!(model.position(&piece, 3), None);
            assert_eq!(model.triangles(&piece), [[corner(p, 0), corner(p, 1), corner(p, 2)]]);
        }
    }

    /// Each piece's texture coordinates: where they start and how far apart, as `layout` (by piece) lays them out.
    fn check_coordinates(data: &[u8], pieces: usize, layout: impl Fn(usize) -> Option<(usize, usize)>) {
        let model = Pmg::read(data).expect("a format read");
        for p in 0..pieces {
            let piece = model.piece(p).expect("its record");
            assert_eq!(piece.coordinates.map(|at| (at, piece.coordinate_stride)), layout(p), "piece {p}");
        }
    }

    #[test]
    fn an_old_models_vertices_in_one_block_each() {
        // 0x13, a road's or a billboard's: positions, normals, colours, coordinates in one vertex of 36 bytes.
        let data = old_model(0x506d_6713, 3, |p| {
            let at = 1000 + 3 * 36 * p as u32;
            ([at, at + 12, at + 28, at + 24], 36, 36)
        });
        check(&data, 3, (36, 36));
        check_coordinates(&data, 3, |p| Some((1000 + 3 * 36 * p + 28, 36)));
    }

    #[test]
    fn an_old_models_vertices_in_two_blocks_each() {
        // 0x13, a flag's: every piece's positions and normals (24 bytes a vertex), then every piece's colours and
        // coordinates (12), then the indices.
        let data = old_model(0x506d_6713, 3, |p| {
            let (first, second) = (1000 + 3 * 24 * p as u32, 1000 + 3 * 24 * 3 + 3 * 12 * p as u32);
            ([first, first + 12, second + 4, second], 24, 12)
        });
        check(&data, 3, (24, 12));
        check_coordinates(&data, 3, |p| Some((1000 + 3 * 24 * 3 + 3 * 12 * p + 4, 12)));
    }

    #[test]
    fn a_block_read_whatever_array_starts_it() {
        // As a flag's, but coordinates first in the second block, then the colours (the review of 2026-11-11, M13: the
        // colours were taken to start their block, and its stride came out short).
        let data = old_model(0x506d_6713, 3, |p| {
            let (first, second) = (1000 + 3 * 24 * p as u32, 1000 + 3 * 24 * 3 + 3 * 12 * p as u32);
            ([first, first + 12, second, second + 8], 24, 12)
        });
        check(&data, 3, (24, 12));
        check_coordinates(&data, 3, |p| Some((1000 + 3 * 24 * 3 + 3 * 12 * p, 12)));
    }

    #[test]
    fn an_array_of_the_records_more_is_one_of_its_vertex() {
        // A road's of the Oyuncuyusbis map (doroga_beton): positions, normals, 16 bytes at +84's place (+24), colours,
        // coordinates: 52 bytes a vertex.
        let mut data = old_model(0x506d_6713, 2, |p| {
            let at = 1000 + 3 * 52 * p as u32;
            ([at, at + 12, at + 44, at + 40], 52, 52)
        });
        for p in 0..2 {
            put(&mut data, 200 + 104 * p + 84, 1000 + 3 * 52 * p as u32 + 24);
        }
        check(&data, 2, (52, 52));
    }

    #[test]
    fn the_oldest_models_arrays_apart() {
        // 0x12: each array its own (positions 12 bytes a vertex, colours 4), a piece's after another's.
        let data = old_model(0x506d_6712, 2, |p| {
            let at = 1000 + 3 * 44 * p as u32;
            ([at, at + 36, at + 72, at + 120], 12, 4)
        });
        check(&data, 2, (12, 4));
        // Its coordinates' own array says nothing of how far apart they are: not read.
        check_coordinates(&data, 2, |_| None);
        // A piece without colours: alpha 1.
        let data = old_model(0x506d_6712, 1, |_| ([1000, 1036, 1072, NONE], 12, 4));
        let model = Pmg::read(&data).unwrap();
        let piece = model.piece(0).unwrap();
        assert_eq!((piece.colours, model.alpha(&piece, 0)), (None, 1.0));
    }

    #[test]
    fn each_formats_header_read_at_its_own_places() {
        for (version, box_at, parts_at, locators_at, pieces_at) in
            [(0x506d_6715, 48, 76, 80, 84), (0x506d_6714, 40, 68, 72, 76), (0x506d_6713, 36, 64, 68, 72)]
        {
            let mut data = Vec::new();
            put(&mut data, 0, version);
            put(&mut data, 8, 1);
            for axis in 0..3 {
                put_f(&mut data, box_at + 4 * axis, -1.0 - axis as f32);
                put_f(&mut data, box_at + 12 + 4 * axis, 1.0 + axis as f32);
            }
            put(&mut data, parts_at, 300);
            put(&mut data, locators_at, 400);
            put(&mut data, pieces_at, 500);
            for (at, value) in [(0, 0x1234), (4, 0), (8, 2), (12, 5), (16, 1), (20, 3)] {
                put(&mut data, 300 + at, value);
            }
            let locator = 400 + 3 * LOCATOR_SIZE;
            put(&mut data, locator, 0x5678);
            // Its position, its scale, its rotation.
            for (index, value) in [1.5, -2.0, 0.25, 1.0, 0.5, 0.0, -0.5, 0.0].into_iter().enumerate() {
                put_f(&mut data, locator + 8 + 4 * index, value);
            }
            let model = Pmg::read(&data).unwrap();
            assert_eq!(model.model_box(), Some(([-1.0, -2.0, -3.0], [1.0, 2.0, 3.0])));
            assert_eq!(model.locators_at(), Some(400));
            let read = Locator { name: 0x5678, position: [1.5, -2.0, 0.25], rotation: [0.5, 0.0, -0.5, 0.0] };
            assert_eq!(model.locator(3), Some(read));
            assert_eq!(model.locator(4), None, "past the file");
            assert_eq!(model.part(0), Some(Part { name: Some(0x1234), pieces: 5..7, locators: 3..4 }));
        }
        assert!(Pmg::read(&0x506d_6716u32.to_le_bytes()).is_none());
        // Counts whose records would run past the file: none (a billion pieces, a billion parts), its box still read.
        for (version, box_at) in [(0x506d_6715u32, 48), (0x506d_6713, 36)] {
            let mut data = Vec::new();
            put(&mut data, 0, version);
            put(&mut data, 4, 1_000_000_000);
            put(&mut data, 100, 0);
            put_f(&mut data, box_at + 12, 1.0);
            let model = Pmg::read(&data).unwrap();
            assert_eq!((model.piece_count(), model.part_count()), (None, Some(0)), "{version:x}");
            assert_eq!(model.model_box(), Some(([0.0; 3], [1.0, 0.0, 0.0])), "{version:x}");
            put(&mut data, 4, 0);
            put(&mut data, 8, 1_000_000_000);
            let model = Pmg::read(&data).unwrap();
            assert_eq!((model.piece_count(), model.part_count()), (Some(0), None), "{version:x}");
        }
        assert!(Pmg::read(&0x506d_6711u32.to_le_bytes()).is_none());
    }

    #[test]
    fn a_pieces_vertices_are_only_those_in_the_file() {
        let mut data = old_model(0x506d_6713, 1, |_| ([1000, 1012, 1028, 1024], 36, 36));
        let model = Pmg::read(&data).unwrap();
        assert_eq!(model.held_vertices(&model.piece(0).unwrap()), 3);
        // Its vertex count damaged to 4 billion (the fuzzing of 2026-11-12: walking them all never ended).
        put(&mut data, 200 + 4, u32::MAX);
        let model = Pmg::read(&data).unwrap();
        let piece = Piece { stride: 36, ..model.piece(0).unwrap_or(Piece { vertices: u32::MAX as usize, ..PIECE }) };
        assert!(model.held_vertices(&piece) <= data.len() / 36 + 1);
        let started = std::time::Instant::now();
        let _ = model.bounds(&piece);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        // A stride of 0 (a damaged record): one vertex at most, never a division by it.
        assert!(model.held_vertices(&Piece { stride: 0, ..piece }) <= 1);
    }

    #[test]
    fn a_piece_of_no_vertices_draws_nothing_and_a_damaged_index_count_ends() {
        // 0x13, three pieces in a block each, the middle one's vertex count 0 (the review of 2026-11-11, tests): its
        // blocks have no vertices to be measured by, so it reads as no piece; the others read as they are.
        let mut data = old_model(0x506d_6713, 3, |p| {
            let at = 1000 + 3 * 36 * p as u32;
            ([at, at + 12, at + 28, at + 24], 36, 36)
        });
        put(&mut data, 200 + 104 + 4, 0);
        let model = Pmg::read(&data).unwrap();
        assert_eq!(model.piece(1), None);
        for p in [0, 2] {
            let piece = model.piece(p).expect("its record");
            assert_eq!(model.triangles(&piece), [[corner(p, 0), corner(p, 1), corner(p, 2)]]);
        }
        // The current format's reads (its stride stored): no vertex, so no triangle, its indices whatever they are.
        let piece = Piece { indices: 3, index_at: 1000, ..PIECE };
        assert_eq!((model.held_vertices(&piece), model.position(&piece, 0)), (0, None));
        assert!(model.triangles(&piece).is_empty());
        assert_eq!(model.bounds(&piece), (PIECE.min, PIECE.max));
        // An index count damaged to 4 billion: only those the file holds, walked at once.
        let piece = model.piece(0).unwrap();
        let damaged = Piece { indices: u32::MAX as usize, ..piece };
        assert!(model.held_indices(&damaged) <= data.len() / 2);
        let started = std::time::Instant::now();
        let _ = model.triangles(&damaged);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    /// A piece of no vertices at the place the tests' models put the first's.
    const PIECE: Piece = Piece {
        indices: 0,
        vertices: 0,
        material: 0,
        min: [0.0; 3],
        max: [0.0; 3],
        positions: 1000,
        stride: 36,
        colours: None,
        colour_stride: 36,
        coordinates: None,
        coordinate_stride: 0,
        index_at: 0,
    };

    #[test]
    fn a_box_that_doesnt_hold_its_vertices_is_theirs() {
        let mut data = old_model(0x506d_6713, 1, |_| ([1000, 1012, 1028, 1024], 36, 36));
        // Its box mirrored in x (x −2 to 0, the vertices' 0 to 2), as some mods' are.
        for (at, value) in [(40, -2.0), (44, 0.0), (48, -2.0), (52, 0.0), (56, 2.0), (60, 0.0)] {
            put_f(&mut data, 200 + at, value);
        }
        let model = Pmg::read(&data).unwrap();
        let piece = model.piece(0).unwrap();
        assert_eq!(model.bounds(&piece), ([0.0, 0.0, -2.0], [2.0, 2.0, 0.0]));
        // A box that holds them stays as stored, larger or not.
        for (at, value) in [(40, -1.0), (44, -1.0), (48, -3.0), (52, 3.0), (56, 3.0), (60, 1.0)] {
            put_f(&mut data, 200 + at, value);
        }
        let model = Pmg::read(&data).unwrap();
        let piece = model.piece(0).unwrap();
        assert_eq!(model.bounds(&piece), ([-1.0, -1.0, -3.0], [3.0, 3.0, 1.0]));
    }
}
