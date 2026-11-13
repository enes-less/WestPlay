//! A model's drawn pieces: their triangles, what their materials say, how two-layer materials blend.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::buildings::{unseen_material, visible_pieces};
use crate::files::GameFiles;
use crate::pmd::Pmd;
use crate::pmg::Pmg;
use crate::token::token_to_string;

use super::sort::Rules;

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
pub fn model_pieces(
    files: &GameFiles,
    materials: &Materials,
    pmd_path: &str,
    variant: Option<u64>,
    look: u64,
) -> Vec<Piece> {
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

impl Piece {
    /// Everything it is, into `hasher`, bit for bit (for `SortedFrom::fingerprint`): its part, its material by what
    /// it is, its box, its triangles and their paint.
    pub fn fingerprint_into(&self, hasher: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        self.part.hash(hasher);
        let material = &*self.material;
        (&material.effect, &material.substance, &material.textures, material.unseen).hash(hasher);
        [self.box_x.0, self.box_x.1, self.box_y.0, self.box_y.1].map(f32::to_bits).hash(hasher);
        self.triangles.len().hash(hasher);
        for triangle in &self.triangles {
            triangle.map(|v| v.map(f32::to_bits)).hash(hasher);
        }
        self.paint.len().hash(hasher);
        for paint in &self.paint {
            paint.map(f32::to_bits).hash(hasher);
        }
    }
}

/// A `.pmd`'s material paths in the look named `look` (a token), its first if it hasn't that one.
fn look_materials(pmd: &[u8], look: u64) -> Vec<String> {
    Pmd::read(pmd).map_or_else(Vec::new, |pmd| pmd.materials(Some(look)))
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
    // Its `.pmg` beside it (its name's end changed, not every `.pmd` in its path: the review of 2026-11-11, nits).
    let Some(pmg_path) = pmd_path.strip_suffix(".pmd").map(|stem| format!("{stem}.pmg")) else { return Vec::new() };
    let (Some(pmd), Some(pmg)) = (files.read(pmd_path).ok().flatten(), files.read(&pmg_path).ok().flatten()) else {
        return Vec::new();
    };
    // In its own format ([`Pmg`]): read as another's, a piece's counts are anything (a map mod's older models, 0x14:
    // one piece's indices read as a billion, its triangles asked 13 GB at once and the build ended, a player's,
    // 2026-11-11).
    let Some(model) = Pmg::read(&pmg) else { return Vec::new() };
    let shown = variant.and_then(|variant| visible_pieces(&pmd, &pmg, variant));
    let paths = look_materials(&pmd, look);
    let Some(count) = model.piece_count() else { return Vec::new() };
    let mut part_of = vec![String::new(); count];
    let mut turned_on = vec![false; count];
    // Parts whose records aren't in the file name none of its pieces (its variants then show them all).
    for part in (0..model.part_count().unwrap_or(0)).map_while(|part| model.part(part)) {
        let name = part.name.and_then(token_to_string).unwrap_or_default();
        for piece in part.pieces.start..part.pieces.end.min(count) {
            part_of[piece].clone_from(&name);
            turned_on[piece] = part.name.is_some_and(|token| extra.contains(&token));
        }
    }
    let mut out = Vec::new();
    for index in 0..count {
        if shown.as_ref().is_some_and(|shown| !shown.get(index).copied().unwrap_or(true)) && !turned_on[index] {
            continue;
        }
        let Some(piece) = model.piece(index) else { continue };
        let Some(path) = paths.get(piece.material) else { continue };
        let material = materials.get(files, path);
        if material.unseen || piece.stride < 12 {
            continue;
        }
        let vertex = |index: usize| model.position(&piece, index).unwrap_or([f32::NAN; 3]);
        let alpha = |index: usize| model.alpha(&piece, index);
        // No more indices than the file holds after where they start (a damaged count would ask for anything).
        let indices = model.held_indices(&piece);
        let mut triangles = Vec::with_capacity(indices / 3);
        let mut paint = Vec::with_capacity(indices / 3);
        for first in (0..indices).step_by(3) {
            let corner = |k: usize| model.index(&piece, first + k).unwrap_or(usize::MAX);
            let (a, b, c) = (corner(0), corner(1), corner(2));
            if a < piece.vertices && b < piece.vertices && c < piece.vertices {
                triangles.push([vertex(a), vertex(b), vertex(c)]);
                paint.push([alpha(a), alpha(b), alpha(c)]);
            }
        }
        let (min, max) = model.bounds(&piece);
        out.push(Piece {
            part: part_of[index].clone(),
            material,
            box_x: (min[0], max[0]),
            box_y: (min[1], max[1]),
            triangles,
            paint,
        });
    }
    out
}
