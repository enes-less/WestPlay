//! A model's descriptor (`.pmd`, version 4): its materials look by look, its variants, its parts' attributes (which
//! of them each variant shows). Every table is checked to lie within the file as it's read: their counts are the
//! file's word, and each reader walking a table by its count went on for billions of empty steps on a damaged one (the
//! fuzzing of 2026-11-12, `examples/reader_fuzz`; the review of 2026-11-11, M6).
//!
//! The header, a word each: version, materials, looks, pieces, variants, parts, attributes, the bytes of a variant's
//! attribute values, (one more), then where the looks' names (tokens), the variants' names (tokens), the parts'
//! attribute ranges (first, last + 1), the variants' values, the attributes (name, type, offset in a variant's values:
//! 16 bytes each) and the material table (each material's path's place in the file, look by look) are.

use crate::reader::word;

/// The version read.
pub const VERSION: u32 = 4;

/// A descriptor whose tables all lie within it.
pub struct Pmd<'a> {
    data: &'a [u8],
    materials: usize,
    looks: usize,
    variants: usize,
    parts: usize,
    attributes: usize,
    values_size: usize,
    look_names_at: usize,
    variants_at: usize,
    links_at: usize,
    values_at: usize,
    attributes_at: usize,
    table_at: usize,
}

impl<'a> Pmd<'a> {
    /// `data` as a descriptor: version 4, every table within it; `None` otherwise.
    pub fn read(data: &'a [u8]) -> Option<Self> {
        let header = |index: usize| word(data, 4 * index).map(|value| value as usize);
        if word(data, 0)? != VERSION {
            return None;
        }
        let pmd = Self {
            data,
            materials: header(1)?,
            looks: header(2)?,
            variants: header(4)?,
            parts: header(5)?,
            attributes: header(6)?,
            values_size: header(7)?,
            look_names_at: header(9)?,
            variants_at: header(10)?,
            links_at: header(11)?,
            values_at: header(12)?,
            attributes_at: header(13)?,
            table_at: header(14)?,
        };
        let within = |at: usize, count: usize, size: usize| {
            count == 0
                || count.checked_mul(size).and_then(|bytes| at.checked_add(bytes)).is_some_and(|end| end <= data.len())
        };
        let tables = [
            (pmd.look_names_at, pmd.looks, 8),
            (pmd.table_at, pmd.looks.checked_mul(pmd.materials)?, 4),
            (pmd.variants_at, pmd.variants, 8),
            (pmd.links_at, pmd.parts, 8),
            (pmd.values_at, pmd.variants, pmd.values_size),
            (pmd.attributes_at, pmd.attributes, 16),
        ];
        tables.iter().all(|&(at, count, size)| within(at, count, size)).then_some(pmd)
    }

    fn long(&self, at: usize) -> Option<u64> {
        Some(u64::from(word(self.data, at)?) | u64::from(word(self.data, at + 4)?) << 32)
    }

    pub fn parts(&self) -> usize {
        self.parts
    }

    /// The materials (their `.mat` paths) of the look `look` (a token), the first look's if it has none of that name
    /// or none is given. A path that doesn't read (no end in the file) is empty, so each stays at its place.
    pub fn materials(&self, look: Option<u64>) -> Vec<String> {
        let chosen =
            look.and_then(|look| (0..self.looks).find(|&at| self.long(self.look_names_at + 8 * at) == Some(look)));
        self.look_materials(chosen.unwrap_or(0))
    }

    /// How many looks it has.
    pub fn looks(&self) -> usize {
        self.looks
    }

    /// The materials of the look at `look` (its place among them), as [`Self::materials`] gives them; none past them.
    pub fn look_materials(&self, look: usize) -> Vec<String> {
        if look >= self.looks {
            return Vec::new();
        }
        (0..self.materials)
            .map(|material| {
                let place = word(self.data, self.table_at + 4 * (look * self.materials + material));
                place
                    .and_then(|place| self.data.get(place as usize..))
                    .and_then(|rest| Some(String::from_utf8_lossy(&rest[..rest.iter().position(|&byte| byte == 0)?])))
                    .map_or_else(String::new, |path| path.into_owned())
            })
            .collect()
    }

    /// The name (a token) of the variant at `index`; `None` past them.
    pub fn variant_name(&self, index: usize) -> Option<u64> {
        if index >= self.variants {
            return None;
        }
        self.long(self.variants_at + 8 * index)
    }

    /// Which of its variants `variant` (a token) is.
    pub fn variant(&self, variant: u64) -> Option<usize> {
        (0..self.variants).find(|&at| self.long(self.variants_at + 8 * at) == Some(variant))
    }

    /// Whether part `part` is shown in variant `variant` (by [`Self::variant`]), as its `visible` attribute says; `None`
    /// if it has no such attribute (shown) or what it says doesn't read.
    pub fn part_visible(&self, part: usize, variant: usize, visible: u64) -> Option<bool> {
        if part >= self.parts || variant >= self.variants {
            return None;
        }
        let first = word(self.data, self.links_at + 8 * part)? as usize;
        let end = (word(self.data, self.links_at + 8 * part + 4)? as usize).min(self.attributes);
        let attribute = (first.min(end)..end).find(|&at| self.long(self.attributes_at + 16 * at) == Some(visible))?;
        let offset = word(self.data, self.attributes_at + 16 * attribute + 12)? as usize;
        if offset.checked_add(4)? > self.values_size {
            return None;
        }
        Some(word(self.data, self.values_at + variant * self.values_size + offset)? != 0)
    }
}
