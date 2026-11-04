//! What passes over what. The map is flat, but the game's roads aren't: where one surface lies over another (a bridge, a
//! junction's flyover), the upper one has to be drawn after the lower. Levels here aren't given to whole roads: each face
//! is one up from the highest face it clears, so a road is raised only where something runs under it, and however many
//! ramps wind round each other, the top surface at every point is the one on top.

use std::collections::HashMap;

use super::Kind;

/// How far apart two surfaces have to be to be one over the other, m: less, and they're the same surface meeting.
pub const CLEAR: f64 = 2.5;
/// The most levels above the ground.
pub const MOST: u8 = 3;
const CELL: f64 = 16.0;
/// Overlaps smaller than this are surfaces brushing, m².
const OVERLAP: f64 = 0.25;

pub type Triangle = [[f64; 3]; 3];

/// A face that may be over or under another.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub t: Triangle,
    pub kind: Kind,
    pub near: bool,
}

fn box_of(t: &Triangle) -> [f64; 4] {
    t.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, v| [b[0].min(v[0]), b[1].min(v[2]), b[2].max(v[0]), b[3].max(v[2])])
}

/// Where two triangles overlap on the ground: its area and its middle (Sutherland-Hodgman).
fn overlap(a: &Triangle, b: &Triangle) -> Option<(f64, [f64; 2])> {
    let flat = |t: &Triangle| -> [[f64; 2]; 3] {
        let list = [[t[0][0], t[0][2]], [t[1][0], t[1][2]], [t[2][0], t[2][2]]];
        let area = (list[1][0] - list[0][0]) * (list[2][1] - list[0][1]) - (list[2][0] - list[0][0]) * (list[1][1] - list[0][1]);
        if area < 0.0 { [list[0], list[2], list[1]] } else { list }
    };
    let (a, b) = (flat(a), flat(b));
    let mut polygon: Vec<[f64; 2]> = a.to_vec();
    for i in 0..3 {
        let (p, q) = (b[i], b[(i + 1) % 3]);
        let side = |v: [f64; 2]| (q[0] - p[0]) * (v[1] - p[1]) - (q[1] - p[1]) * (v[0] - p[0]);
        let mut next = Vec::with_capacity(polygon.len() + 1);
        for k in 0..polygon.len() {
            let (u, w) = (polygon[k], polygon[(k + 1) % polygon.len()]);
            let (su, sw) = (side(u), side(w));
            if su >= 0.0 {
                next.push(u);
            }
            if (su >= 0.0) != (sw >= 0.0) {
                let s = su / (su - sw);
                next.push([u[0] + s * (w[0] - u[0]), u[1] + s * (w[1] - u[1])]);
            }
        }
        polygon = next;
        if polygon.len() < 3 {
            return None;
        }
    }
    let (mut area, mut x, mut z) = (0.0, 0.0, 0.0);
    for k in 1..polygon.len() - 1 {
        let (p, q, r) = (polygon[0], polygon[k], polygon[k + 1]);
        let part = ((q[0] - p[0]) * (r[1] - p[1]) - (r[0] - p[0]) * (q[1] - p[1])) / 2.0;
        area += part;
        x += part * (p[0] + q[0] + r[0]) / 3.0;
        z += part * (p[1] + q[1] + r[1]) / 3.0;
    }
    (area > OVERLAP).then(|| (area, [x / area, z / area]))
}

/// How high `t`'s plane is at (x, z).
fn height_at(t: &Triangle, x: f64, z: f64) -> f64 {
    let [a, b, c] = *t;
    let det = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
    if det.abs() < 1e-12 {
        return (a[1] + b[1] + c[1]) / 3.0;
    }
    let w1 = ((x - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (z - a[2])) / det;
    let w2 = ((b[0] - a[0]) * (z - a[2]) - (x - a[0]) * (b[2] - a[2])) / det;
    a[1] + w1 * (b[1] - a[1]) + w2 * (c[1] - a[1])
}

/// Each face's level: 0, or one above the highest face it clears.
pub fn levels(faces: &[Face]) -> Vec<u8> {
    let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (i, face) in faces.iter().enumerate() {
        let [x0, z0, x1, z1] = box_of(&face.t);
        for x in (x0 / CELL).floor() as i32..=(x1 / CELL).floor() as i32 {
            for z in (z0 / CELL).floor() as i32..=(z1 / CELL).floor() as i32 {
                grid.entry((x, z)).or_default().push(i as u32);
            }
        }
    }
    let middle = |face: &Face| (face.t[0][1] + face.t[1][1] + face.t[2][1]) / 3.0;
    let mut order: Vec<u32> = (0..faces.len() as u32).collect();
    order.sort_by(|&a, &b| middle(&faces[a as usize]).total_cmp(&middle(&faces[b as usize])));
    let mut level = vec![0u8; faces.len()];
    let mut done = vec![false; faces.len()];
    for &index in &order {
        let face = &faces[index as usize];
        let [x0, z0, x1, z1] = box_of(&face.t);
        let mut best = 0u8;
        for x in (x0 / CELL).floor() as i32..=(x1 / CELL).floor() as i32 {
            for z in (z0 / CELL).floor() as i32..=(z1 / CELL).floor() as i32 {
                for &other in grid.get(&(x, z)).map_or(&[][..], Vec::as_slice) {
                    let under = &faces[other as usize];
                    if !done[other as usize] || level[other as usize] < best {
                        continue;
                    }
                    let [ox0, oz0, ox1, oz1] = box_of(&under.t);
                    if ox0 > x1 || ox1 < x0 || oz0 > z1 || oz1 < z0 {
                        continue;
                    }
                    if let Some((_, at)) = overlap(&face.t, &under.t)
                        && height_at(&face.t, at[0], at[1]) - height_at(&under.t, at[0], at[1]) >= CLEAR
                    {
                        best = level[other as usize] + 1;
                    }
                }
            }
        }
        level[index as usize] = best.min(MOST);
        done[index as usize] = true;
    }
    level
}

/// Faces indexed, to ask what's at a point of the ground.
pub struct FaceIndex<'a> {
    /// The faces, as one list: `first`'s, then `then`'s (two lists indexed together without copying them into one: a
    /// block's ground and roads, hundreds of megabytes in a city's).
    first: &'a [Face],
    then: &'a [Face],
    grid: super::Cells,
}

impl<'a> FaceIndex<'a> {
    pub fn new(faces: &'a [Face]) -> Self {
        Self::both(faces, &[])
    }

    /// `first`'s faces and `then`'s, indexed as one list of them in that order.
    pub fn both(first: &'a [Face], then: &'a [Face]) -> Self {
        let mut added = Vec::new();
        for (i, face) in first.iter().chain(then).enumerate() {
            let [x0, z0, x1, z1] = box_of(&face.t);
            for x in (x0 / CELL).floor() as i32..=(x1 / CELL).floor() as i32 {
                for z in (z0 / CELL).floor() as i32..=(z1 / CELL).floor() as i32 {
                    added.push(((x, z), i as u32));
                }
            }
        }
        Self { first, then, grid: super::Cells::new(added) }
    }

    /// The `i`th face of the list.
    fn face(&self, i: usize) -> &Face {
        self.first.get(i).unwrap_or_else(|| &self.then[i - self.first.len()])
    }

    /// The heights of the faces at (x, z).
    pub fn heights(&self, x: f64, z: f64) -> impl Iterator<Item = f64> + '_ {
        self.faces_at(x, z).map(|(_, height)| height)
    }

    /// The faces at (x, z): which, and how high.
    pub fn faces_at(&self, x: f64, z: f64) -> impl Iterator<Item = (usize, f64)> + '_ {
        let cell = ((x / CELL).floor() as i32, (z / CELL).floor() as i32);
        self.grid.at(cell).iter().filter_map(move |&i| {
            let t = &self.face(i as usize).t;
            let [a, b, c] = *t;
            let side = |p: [f64; 3], q: [f64; 3]| (q[0] - p[0]) * (z - p[2]) - (q[2] - p[2]) * (x - p[0]);
            let (s0, s1, s2) = (side(a, b), side(b, c), side(c, a));
            let inside = (s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0) || (s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0);
            inside.then(|| (i as usize, height_at(t, x, z)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::RoadClass;

    fn strip(from: [f64; 2], to: [f64; 2], y: f64) -> Vec<Face> {
        let (a, b, c, d) = ([from[0], y, from[1]], [to[0], y, from[1]], [to[0], y, to[1]], [from[0], y, to[1]]);
        let kind = Kind::Road(RoadClass::Local);
        vec![Face { t: [a, b, c], kind, near: false }, Face { t: [a, c, d], kind, near: false }]
    }

    #[test]
    fn a_road_over_another_is_one_up_only_where_it_passes_over() {
        // Along the ground, and across it 6 m up in three pieces.
        let mut faces = strip([0.0, -5.0], [100.0, 5.0], 0.0);
        faces.extend(strip([40.0, -60.0], [60.0, -20.0], 6.0));
        faces.extend(strip([40.0, -20.0], [60.0, 20.0], 6.0));
        faces.extend(strip([40.0, 20.0], [60.0, 60.0], 6.0));
        let levels = levels(&faces);
        assert_eq!(levels, [0, 0, 0, 0, 1, 1, 0, 0]);
    }

    #[test]
    fn surfaces_meeting_at_their_height_stay_on_the_ground() {
        let mut faces = strip([0.0, -5.0], [100.0, 5.0], 0.0);
        faces.extend(strip([50.0, -30.0], [60.0, 30.0], 1.0));
        assert!(levels(&faces).iter().all(|&level| level == 0));
    }

    #[test]
    fn a_stack_of_flyovers_climbs_one_each() {
        let mut faces = strip([0.0, 0.0], [10.0, 10.0], 0.0);
        faces.extend(strip([0.0, 0.0], [10.0, 10.0], 6.0));
        faces.extend(strip([0.0, 0.0], [10.0, 10.0], 12.0));
        assert_eq!(levels(&faces), [0, 0, 1, 1, 2, 2]);
    }
}
