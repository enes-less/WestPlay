//! A model's drawn pieces: their triangles, what their materials say, how two-layer materials blend.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::buildings::{unseen_material, visible_pieces};
use crate::files::GameFiles;
use crate::token::token_to_string;

use super::sort::Rules;

/// Where a `.pmd`'s header says how many materials and looks it has, where its looks' names are, and where its
/// material table is (a path's place in the file, look by look).
const PMD_MATERIALS: usize = 4;
const PMD_LOOKS: usize = 8;
const PMD_LOOK_NAMES: usize = 36;
const PMD_MATERIAL_TABLE: usize = 56;
/// A `.pmg`: how many pieces and parts, and where their tables are. A piece's record: its indices, vertices,
/// material, box, the vertices' stride, where its positions, colours and indices are. A part's: its name, its
/// pieces' count and first.
const PIECE_COUNT: usize = 4;
const PART_COUNT: usize = 8;
const PARTS: usize = 76;
const PIECES: usize = 84;
const PIECE_SIZE: usize = 100;
const PART_SIZE: usize = 24;

fn word(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

fn long(data: &[u8], at: usize) -> Option<u64> {
    Some(u64::from(word(data, at)?) | u64::from(word(data, at + 4)?) << 32)
}

/// What a material says about itself.
#[derive(Debug, Default)]
pub struct Material {
    /// The engine's effect (`eut2.dif.spec.weight.weight...rfx`).
    pub effect: String,
    /// What it's made of for tyres (`road`, `grass`): empty when it says nothing.
    pub substance: String,
    /// Every texture it names, in order, resolved against its folder.
    pub textures: Vec<String>,
    /// Not drawn: a collision mesh's, a shadow's.
    pub unseen: bool,
}

/// Materials read once each: models share them.
#[derive(Default)]
pub struct Materials {
    known: Mutex<HashMap<String, Arc<Material>>>,
}

impl Materials {
    pub fn get(&self, files: &GameFiles, path: &str) -> Arc<Material> {
        if let Some(known) = self.known.lock().unwrap_or_else(|e| e.into_inner()).get(path) {
            return known.clone();
        }
        let read = Arc::new(read_material(files, path));
        self.known.lock().unwrap_or_else(|e| e.into_inner()).insert(path.to_owned(), read.clone());
        read
    }
}

/// A `.mat`'s first quoted value after a line starting with `key`.
fn quoted<'a>(text: &'a str, keys: &[&str]) -> Option<&'a str> {
    text.lines().map(str::trim_start).find(|line| keys.iter().any(|key| line.starts_with(key)))?.split('"').nth(1)
}

fn read_material(files: &GameFiles, path: &str) -> Material {
    let Some(bytes) = files.read(path).ok().flatten() else { return Material::default() };
    let text = String::from_utf8_lossy(&bytes);
    let folder = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let textures = text
        .lines()
        .map(str::trim_start)
        .filter(|line| line.starts_with("source") || line.starts_with("texture"))
        .filter_map(|line| line.split('"').nth(1))
        .filter(|texture| texture.ends_with(".tobj"))
        .map(|texture| if texture.starts_with('/') { texture.to_owned() } else { format!("{folder}/{texture}") })
        .collect();
    Material {
        effect: quoted(&text, &["effect", "material"]).unwrap_or("").to_owned(),
        substance: quoted(&text, &["substance"]).unwrap_or("").to_owned(),
        textures,
        unseen: unseen_material(&text),
    }
}

/// [`model_pieces_with`] no additional parts.
pub fn model_pieces(files: &GameFiles, materials: &Materials, pmd_path: &str, variant: Option<u64>, look: u64) -> Vec<Piece> {
    model_pieces_with(files, materials, pmd_path, variant, look, &[])
}

/// A piece of a model.
pub struct Piece {
    pub part: String,
    pub material: Arc<Material>,
    /// How far it reaches across (x) and up (y).
    pub box_x: (f32, f32),
    pub box_y: (f32, f32),
    pub triangles: Vec<[[f32; 3]; 3]>,
    /// Each triangle's corners' vertex paint (the alpha of their colour), 0..1: how two-layer materials blend.
    pub paint: Vec<[f32; 3]>,
}

/// A `.pmd`'s material paths in the look named `look` (a token), its first if it hasn't that one.
fn look_materials(pmd: &[u8], look: u64) -> Vec<String> {
    let at = |offset: usize| word(pmd, offset).map(|value| value as usize);
    let (Some(count), Some(looks), Some(names), Some(table)) =
        (at(PMD_MATERIALS), at(PMD_LOOKS), at(PMD_LOOK_NAMES), at(PMD_MATERIAL_TABLE))
    else {
        return Vec::new();
    };
    let chosen = (0..looks).find(|&index| long(pmd, names + 8 * index) == Some(look)).unwrap_or(0);
    (0..count)
        .filter_map(|material| {
            let rest = pmd.get(at(table + 4 * (chosen * count + material))?..)?;
            let end = rest.iter().position(|&byte| byte == 0)?;
            Some(String::from_utf8_lossy(&rest[..end]).into_owned())
        })
        .collect()
}

/// What a look says about a model's surfaces, as a number: what its materials are to the sorting of faces, not how they
/// look. Two looks with the same one sort the same.
pub fn look_signature(files: &GameFiles, materials: &Materials, rules: &Rules, pmd: &[u8], look: u64) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    for path in look_materials(pmd, look) {
        let material = materials.get(files, &path);
        let texture = |index: usize| material.textures.get(index).map(String::as_str).unwrap_or("");
        material.effect.hash(&mut hash);
        material.substance.hash(&mut hash);
        material.unseen.hash(&mut hash);
        (material.textures.len() >= 2).hash(&mut hash);
        rules.is_ground(texture(0)).hash(&mut hash);
        rules.is_ground(texture(1)).hash(&mut hash);
        rules.sidewalk_textures.contains(texture(0)).hash(&mut hash);
    }
    hash.finish()
}

/// The drawn pieces of the model at `pmd_path`, shown in `variant` (all with `None`) or in `extra`, the additional parts
/// its item turns on (tokens), in `look` (a token).
pub fn model_pieces_with(
    files: &GameFiles,
    materials: &Materials,
    pmd_path: &str,
    variant: Option<u64>,
    look: u64,
    extra: &[u64],
) -> Vec<Piece> {
    let pmg_path = pmd_path.replace(".pmd", ".pmg");
    let (Some(pmd), Some(pmg)) = (files.read(pmd_path).ok().flatten(), files.read(&pmg_path).ok().flatten()) else {
        return Vec::new();
    };
    let shown = variant.and_then(|variant| visible_pieces(&pmd, &pmg, variant));
    let paths = look_materials(&pmd, look);
    let w = |at: usize| word(&pmg, at).map_or(0, |value| value as usize);
    let float = |at: usize| word(&pmg, at).map_or(f32::NAN, f32::from_bits);
    let (count, parts, parts_at, table) = (w(PIECE_COUNT), w(PART_COUNT), w(PARTS), w(PIECES));
    let mut part_of = vec![String::new(); count];
    let mut turned_on = vec![false; count];
    for part in 0..parts {
        let record = parts_at + PART_SIZE * part;
        let token = long(&pmg, record);
        let name = token.and_then(token_to_string).unwrap_or_default();
        let first = w(record + 12);
        for piece in first..first.saturating_add(w(record + 8)).min(count) {
            part_of[piece].clone_from(&name);
            turned_on[piece] = token.is_some_and(|token| extra.contains(&token));
        }
    }
    let mut out = Vec::new();
    for piece in 0..count {
        if shown.as_ref().is_some_and(|shown| !shown.get(piece).copied().unwrap_or(true)) && !turned_on[piece] {
            continue;
        }
        let at = table + PIECE_SIZE * piece;
        let (indices, vertices, slot, stride) = (w(at), w(at + 4), w(at + 16), w(at + 60));
        let (positions, colours, index_at) = (w(at + 64), w(at + 76), w(at + 96));
        let Some(path) = paths.get(slot) else { continue };
        let material = materials.get(files, path);
        if material.unseen || stride < 12 {
            continue;
        }
        let vertex = |index: usize| {
            let p = positions + index * stride;
            [float(p), float(p + 4), float(p + 8)]
        };
        let alpha = |index: usize| {
            let a = colours.wrapping_add(index * stride).wrapping_add(3);
            pmg.get(a).map_or(1.0, |&alpha| f32::from(alpha) / 255.0)
        };
        let mut triangles = Vec::with_capacity(indices / 3);
        let mut paint = Vec::with_capacity(indices / 3);
        for first in (0..indices).step_by(3) {
            let corner = |k: usize| {
                let a = index_at + 2 * (first + k);
                pmg.get(a..a + 2).map_or(usize::MAX, |bytes| u16::from_le_bytes([bytes[0], bytes[1]]) as usize)
            };
            let (a, b, c) = (corner(0), corner(1), corner(2));
            if a < vertices && b < vertices && c < vertices {
                triangles.push([vertex(a), vertex(b), vertex(c)]);
                paint.push([alpha(a), alpha(b), alpha(c)]);
            }
        }
        out.push(Piece {
            part: part_of[piece].clone(),
            material,
            box_x: (float(at + 36), float(at + 48)),
            box_y: (float(at + 40), float(at + 52)),
            triangles,
            paint,
        });
    }
    out
}
