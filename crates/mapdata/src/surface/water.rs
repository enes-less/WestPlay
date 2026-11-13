//! Water that isn't painted terrain: rivers laid as curve models, pools and fountains as models. The game's own lists
//! say neither: a model has water if a piece of it is drawn with a water effect, whatever a mod calls it.

use std::collections::HashMap;

use crate::files::GameFiles;
use crate::sector::{Node, SubCurve};
use crate::sii;
use crate::token::string_to_token;
use crate::world::Definitions;

use super::faces::{Materials, Piece, model_pieces};
use super::frame;
use super::prefabs::OWN_GRID;
use super::shapes::{self, Ring, Spot};

/// A river's quads are about this long, m.
const RIVER_STEP: f64 = 5.0;
/// Water lies this far above its curve's nodes in the models (all three of the base game's), m.
const WATER_LIFT: f64 = 0.05;

fn is_water(piece: &Piece) -> bool {
    piece.material.effect.starts_with("eut2.water")
}

/// Whether a model has water, read from its materials alone (most haven't).
fn has_water(files: &GameFiles, materials: &Materials, path: &str) -> bool {
    let Some(pmd) = files.read(path).ok().flatten() else { return false };
    crate::buildings::pmd_materials(&pmd).iter().any(|material| materials.get(files, material).effect.starts_with("eut2.water"))
}

/// Finds the models and curve models with water, into `definitions`. They're kept when the map's read.
pub fn find_water(files: &GameFiles, definitions: &mut Definitions) {
    let materials = Materials::default();
    for unit in sii::read_family_or_note(files, "/def/world", "model") {
        let (Some(token), Some(path)) = (unit.name.strip_prefix("model.").and_then(string_to_token), unit.get("model_desc")) else {
            continue;
        };
        let path = path.trim_matches('"');
        if has_water(files, &materials, path) {
            definitions.water_models.insert(token, path.to_owned());
        }
    }
    for unit in sii::read_family_or_note(files, "/def/world", "curve_model") {
        let (Some(token), Some(path)) = (unit.name.strip_prefix("curve.").and_then(string_to_token), unit.get("model_desc")) else {
            continue;
        };
        let path = path.trim_matches('"');
        if !has_water(files, &materials, path) {
            continue;
        }
        // How far each part's water reaches across.
        let mut parts: HashMap<u64, (f32, f32)> = HashMap::new();
        let mut whole = (f32::MAX, f32::MIN);
        for piece in model_pieces(files, &materials, path, None, 0).iter().filter(|piece| is_water(piece)) {
            let Some(part) = string_to_token(&piece.part) else { continue };
            let entry = parts.entry(part).or_insert((f32::MAX, f32::MIN));
            *entry = (entry.0.min(piece.box_x.0), entry.1.max(piece.box_x.1));
            whole = (whole.0.min(piece.box_x.0), whole.1.max(piece.box_x.1));
        }
        let mut variations: HashMap<u64, (f32, f32)> = HashMap::new();
        for line in unit.all("variation") {
            let mut fields = line.trim_matches('"').split('|').map(str::trim);
            let Some(name) = fields.next().and_then(string_to_token) else { continue };
            let mut range = (f32::MAX, f32::MIN);
            for part in fields.filter_map(|part| string_to_token(part.split(':').next()?.trim())) {
                if let Some(reach) = parts.get(&part) {
                    range = (range.0.min(reach.0), range.1.max(reach.1));
                }
            }
            if range.0 <= range.1 {
                variations.insert(name, range);
            }
        }
        definitions.water_curves.insert(token, crate::world::WaterCurve { variations, whole });
    }
}

/// A river's strip along a curve item: its quads (x, height, z) and its outline.
pub fn river(a: &Node, b: &Node, length: f32, linear: bool, sub: &SubCurve, reach: (f32, f32)) -> (Vec<[[f32; 3]; 4]>, Vec<[f32; 2]>) {
    let length = f64::from(length);
    let steps = ((length / RIVER_STEP).ceil() as usize).clamp(4, 400);
    let scale = f64::from(sub.scale);
    let (from, to) = (f64::from(reach.0) * scale, f64::from(reach.1) * scale);
    let (p0, p1) = (a.position.map(f64::from), b.position.map(f64::from));
    let across_straight = {
        let (x, z) = (p1[0] - p0[0], p1[2] - p0[2]);
        let long = x.hypot(z).max(1e-9);
        [-z / long, x / long]
    };
    let stations: Vec<([f64; 3], [f64; 2], f64)> = (0..=steps)
        .map(|i| {
            let s = i as f64 / steps as f64;
            let shift = f64::from(sub.offsets[0]) + (f64::from(sub.offsets[2]) - f64::from(sub.offsets[0])) * s;
            if linear {
                let p = [0, 1, 2].map(|k| p0[k] + (p1[k] - p0[k]) * s);
                (p, across_straight, shift)
            } else {
                let (p, n) = frame(a, b, length, s, false);
                (p, n, shift)
            }
        })
        .collect();
    let edge = |(p, n, shift): &([f64; 3], [f64; 2], f64), d: f64| -> [f32; 3] {
        [(p[0] + n[0] * (d + shift)) as f32, (p[1] + WATER_LIFT) as f32, (p[2] + n[1] * (d + shift)) as f32]
    };
    let quads = stations.windows(2).map(|pair| [edge(&pair[0], from), edge(&pair[1], from), edge(&pair[1], to), edge(&pair[0], to)]).collect();
    let mut outline: Vec<[f32; 2]> = stations.iter().map(|s| edge(s, from)).map(|v| [v[0], v[2]]).collect();
    outline.extend(stations.iter().rev().map(|s| edge(s, to)).map(|v| [v[0], v[2]]));
    (quads, outline)
}

/// A model's water from above, in its own space: outlines (and holes), with their heights.
pub fn model_water(files: &GameFiles, materials: &Materials, path: &str) -> Vec<Vec<[f32; 3]>> {
    let pieces = model_pieces(files, materials, path, None, 0);
    let mut rings: Vec<Ring> = Vec::new();
    let mut high = f32::MIN;
    for piece in pieces.iter().filter(|piece| is_water(piece) && !piece.part.starts_with("coll")) {
        for t in &piece.triangles {
            let (u, v) = ([t[1][0] - t[0][0], t[1][2] - t[0][2]], [t[2][0] - t[0][0], t[2][2] - t[0][2]]);
            if (u[0] * v[1] - u[1] * v[0]).abs() < 1e-6 {
                continue;
            }
            let ring: Ring = t
                .iter()
                .map(|p| Spot::new((f64::from(p[0]) * OWN_GRID).round() as i32, (f64::from(p[2]) * OWN_GRID).round() as i32))
                .collect();
            rings.push(shapes::outward(ring));
            high = high.max(t[0][1]).max(t[1][1]).max(t[2][1]);
        }
    }
    shapes::union(&rings)
        .iter()
        .map(|ring| ring.iter().map(|p| [(f64::from(p.x) / OWN_GRID) as f32, high, (f64::from(p.y) / OWN_GRID) as f32]).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(x: f32, z: f32) -> Node {
        // Facing +x: forward, -z, turned a quarter to the right.
        let half = std::f32::consts::FRAC_PI_4;
        Node { uid: 0, position: [x, 2.0, z], rotation: [half.cos(), 0.0, -half.sin(), 0.0], forward_item: 0, backward_item: 0, flags: 0 }
    }

    #[test]
    fn a_river_is_its_waters_width_times_its_scale_along_its_curve() {
        let sub = SubCurve { model: 0, flags: 0, stretch: 1.0, scale: 2.0, variation: 0, look: 0, offsets: [0.0; 4] };
        let (quads, outline) = river(&node(0.0, 0.0), &node(100.0, 0.0), 100.0, false, &sub, (-5.0, 5.0));
        assert_eq!(quads.len(), 20);
        assert_eq!(outline.len(), 42);
        let (low, high) = outline.iter().fold((f32::MAX, f32::MIN), |(low, high), p| (low.min(p[1]), high.max(p[1])));
        assert!((low + 10.0).abs() < 0.01 && (high - 10.0).abs() < 0.01, "{low} {high}");
        assert!((quads[0][0][1] - 2.05).abs() < 1e-4);
    }
}
