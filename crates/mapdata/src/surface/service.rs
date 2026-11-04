//! A service area's ground. The game marks no such area: it only says where fuel and rest are offered. So it's the paving
//! reached from there, as the game draws it, over ground wide enough to drive on, so that it doesn't run off along a town's
//! sidewalks, and up to the roads round it: paving goes on under a road, into the lots beyond. The map shows it in a colour of
//! its own, to tell the place from the land.

use std::collections::{HashMap, HashSet};

use crate::sea;

use super::shapes::{self, Ring, Spot};

/// How far from where fuel or rest is offered paving can be its service area's, m.
pub const REACH: f64 = 350.0;
/// Points this close together are one place, m.
const SAME_PLACE: f64 = 64.0;
/// Half the narrowest paving a service area goes on over, m.
const NARROWEST: f64 = 2.5;
/// How far from a point its paving may start, m: a pump stands on an island, a parking spot's mark beside its lot.
const STARTS_WITHIN: f64 = 30.0;
/// The grid the paving is followed on, m a cell, and the outlines cut on, units a metre.
const CELL: f64 = 1.0;
const CUT_GRID: f64 = 32.0;
/// Further than anything on the grid.
const FAR: f32 = 1e9;
/// How far from its paving a road's surface can end and still meet it, m.
const MEETS: f64 = 1.0;
/// How far from what's reached its paving's true edges can be, m: over the narrowest ground it goes on along.
const EDGES_WITHIN: f64 = NARROWEST + 3.0;

/// The outlines of one piece of ground, x, y, z, its holes turning the other way.
pub type Piece = Vec<Vec<[f64; 3]>>;

/// A block's paving round a place: true within the box (min x, min z, max x, max z) only, as what lies over it is known
/// only there; and the roads and junctions there, each its map item and the pieces of its surface.
pub struct Paving {
    pub within: [f64; 4],
    pub pieces: Vec<Piece>,
    pub roads: Vec<(u32, Vec<Piece>)>,
}

/// The ground of the service area at `points`: outlines (x, y, z; y isn't used) with their holes, which turn the other way;
/// and the roads and junctions whose surfaces meet it, their map items.
#[derive(Default)]
pub struct Area {
    pub points: Vec<[f64; 2]>,
    pub rings: Vec<Vec<[f64; 3]>>,
    pub roads: Vec<u32>,
}

/// The box round `points` their paving can be in: min x, min z, max x, max z.
pub fn reach_of(points: &[[f64; 2]]) -> [f64; 4] {
    let mut reach = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
    for p in points {
        reach = [reach[0].min(p[0] - REACH), reach[1].min(p[1] - REACH), reach[2].max(p[0] + REACH), reach[3].max(p[1] + REACH)];
    }
    reach
}

/// `points` in groups that are one place.
pub fn places(points: &[[f64; 2]]) -> Vec<Vec<[f64; 2]>> {
    let cell = |p: &[f64; 2]| ((p[0] / SAME_PLACE).floor() as i32, (p[1] / SAME_PLACE).floor() as i32);
    let mut cells: HashMap<(i32, i32), Vec<[f64; 2]>> = HashMap::new();
    for p in points {
        cells.entry(cell(p)).or_default().push(*p);
    }
    let mut keys: Vec<(i32, i32)> = cells.keys().copied().collect();
    keys.sort_unstable();
    let mut seen: HashSet<(i32, i32)> = HashSet::new();
    let mut out = Vec::new();
    for key in keys {
        if !seen.insert(key) {
            continue;
        }
        let (mut stack, mut place) = (vec![key], Vec::new());
        while let Some((x, z)) = stack.pop() {
            place.extend(cells.get(&(x, z)).into_iter().flatten().copied());
            for next in (x - 1..=x + 1).flat_map(|x| (z - 1..=z + 1).map(move |z| (x, z))) {
                if cells.contains_key(&next) && seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        out.push(place);
    }
    out
}

/// The cells `rings` (in cells, even-odd) cover within `clip` (cells: min x, min y, max x, max y), those whose middles are, a run
/// of a row at a time.
fn runs(w: usize, h: usize, rings: &[Vec<[f64; 2]>], clip: [f64; 4], mut run: impl FnMut(std::ops::RangeInclusive<usize>)) {
    let (mut top, mut bottom) = (f64::MAX, f64::MIN);
    for p in rings.iter().flatten() {
        (top, bottom) = (top.min(p[1]), bottom.max(p[1]));
    }
    if top > bottom || bottom < 0.0 || top >= h as f64 {
        return;
    }
    for row in top.floor().max(0.0) as usize..=(bottom.ceil() as usize).min(h - 1) {
        let y = row as f64 + 0.5;
        if y < clip[1] || y >= clip[3] {
            continue;
        }
        let mut crossings: Vec<f64> = Vec::new();
        for ring in rings {
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                if (a[1] <= y) != (b[1] <= y) {
                    crossings.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                }
            }
        }
        crossings.sort_by(f64::total_cmp);
        for pair in crossings.chunks_exact(2) {
            let from = (pair[0].max(clip[0]) - 0.5).ceil().max(0.0) as usize;
            let to = (pair[1].min(clip[2] - 1e-9) - 0.5).floor().min(w as f64 - 1.0);
            if to >= from as f64 {
                run(row * w + from..=row * w + to as usize);
            }
        }
    }
}

/// How far each cell of `mask` is from the nearest that isn't, in cells (chamfered). 0 for those that aren't, and `outside`
/// beyond the grid.
fn inside_distance(mask: &[bool], w: usize, h: usize, outside: f32) -> Vec<f32> {
    let mut d: Vec<f32> = mask.iter().map(|&on| if on { FAR } else { 0.0 }).collect();
    let step = |d: &mut [f32], x: usize, y: usize, dx: i64, dy: i64, cost: f32| {
        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
        let there = if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 { outside } else { d[ny as usize * w + nx as usize] };
        let at = y * w + x;
        d[at] = d[at].min(there + cost);
    };
    let diagonal = std::f32::consts::SQRT_2;
    for y in 0..h {
        for x in 0..w {
            if mask[y * w + x] {
                step(&mut d, x, y, -1, 0, 1.0);
                step(&mut d, x, y, 0, -1, 1.0);
                step(&mut d, x, y, -1, -1, diagonal);
                step(&mut d, x, y, 1, -1, diagonal);
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            if mask[y * w + x] {
                step(&mut d, x, y, 1, 0, 1.0);
                step(&mut d, x, y, 0, 1, 1.0);
                step(&mut d, x, y, 1, 1, diagonal);
                step(&mut d, x, y, -1, 1, diagonal);
            }
        }
    }
    d
}

/// The ground of the service area at `points`, of the paving in `found`, with the roads that meet it.
pub fn ground(points: &[[f64; 2]], found: &[Paving]) -> (Vec<Vec<[f64; 3]>>, Vec<u32>) {
    let [x0, z0, x1, z1] = reach_of(points);
    let (w, h) = (((x1 - x0) / CELL).ceil() as usize, ((z1 - z0) / CELL).ceil() as usize);
    let in_cells = |ring: &[[f64; 3]]| -> Vec<[f64; 2]> { ring.iter().map(|p| [(p[0] - x0) / CELL, (p[2] - z0) / CELL]).collect() };
    let mut paved = vec![false; w * h];
    for paving in found {
        let [bx0, bz0, bx1, bz1] = paving.within;
        let clip = [(bx0 - x0) / CELL, (bz0 - z0) / CELL, (bx1 - x0) / CELL, (bz1 - z0) / CELL];
        for piece in &paving.pieces {
            let rings: Vec<Vec<[f64; 2]>> = piece.iter().map(|ring| in_cells(ring)).collect();
            runs(w, h, &rings, clip, |run| paved[run].fill(true));
        }
    }
    let everywhere = [0.0, 0.0, w as f64, h as f64];
    let mut on_road = vec![false; w * h];
    for (_, pieces) in found.iter().flat_map(|paving| &paving.roads) {
        for piece in pieces {
            let rings: Vec<Vec<[f64; 2]>> = piece.iter().map(|ring| in_cells(ring)).collect();
            runs(w, h, &rings, everywhere, |run| on_road[run].fill(true));
        }
    }
    paved.iter_mut().zip(&on_road).for_each(|(paved, road)| *paved &= !road);
    // Followed along what's wide enough, from the nearest such ground to each point.
    let narrowest = (NARROWEST / CELL) as f32;
    let depth = inside_distance(&paved, w, h, 0.0);
    let core: Vec<bool> = depth.iter().map(|&d| d >= narrowest).collect();
    let mut reached = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    let within = (STARTS_WITHIN / CELL) as i64;
    for p in points {
        let (cx, cy) = (((p[0] - x0) / CELL) as i64, ((p[1] - z0) / CELL) as i64);
        let nearest = (cx - within..=cx + within)
            .flat_map(|x| (cy - within..=cy + within).map(move |y| (x, y)))
            .filter(|&(x, y)| x >= 0 && y >= 0 && x < w as i64 && y < h as i64 && core[y as usize * w + x as usize])
            .filter(|&(x, y)| (x - cx).pow(2) + (y - cy).pow(2) <= within * within)
            .min_by_key(|&(x, y)| (x - cx).pow(2) + (y - cy).pow(2));
        if let Some((x, y)) = nearest {
            stack.push(y as usize * w + x as usize);
        }
    }
    while let Some(at) = stack.pop() {
        if reached[at] {
            continue;
        }
        reached[at] = true;
        let (x, y) = (at % w, at / w);
        for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
            if nx < w && ny < h && core[ny * w + nx] && !reached[ny * w + nx] {
                stack.push(ny * w + nx);
            }
        }
    }
    if !reached.iter().any(|&on| on) {
        return (Vec::new(), Vec::new());
    }
    let not_reached: Vec<bool> = reached.iter().map(|&on| !on).collect();
    let from = inside_distance(&not_reached, w, h, FAR);
    let meets = narrowest + (MEETS / CELL) as f32;
    let mut roads: Vec<u32> = Vec::new();
    for paving in found {
        for (road, pieces) in &paving.roads {
            let mut near = false;
            for piece in pieces {
                let rings: Vec<Vec<[f64; 2]>> = piece.iter().map(|ring| in_cells(ring)).collect();
                runs(w, h, &rings, everywhere, |run| near = near || from[run].iter().any(|&d| d <= meets));
            }
            if near {
                roads.push(*road);
            }
        }
    }
    roads.sort_unstable();
    roads.dedup();
    // Back out to the paving's own edges, which are the true ones: this only says which of it. Under the roads round it, not
    // past them.
    let mut field: Vec<f32> = reached.iter().map(|&on| if on { 1.0 } else { -1.0 }).collect();
    let mut layer: Vec<usize> = (0..w * h).filter(|&at| reached[at]).collect();
    for _ in 0..(EDGES_WITHIN / CELL).ceil() as usize {
        let mut next = Vec::new();
        for &at in &layer {
            let (x, y) = (at % w, at / w);
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < w && ny < h && field[ny * w + nx] < 0.0 && (!on_road[at] || on_road[ny * w + nx]) {
                    field[ny * w + nx] = 1.0;
                    next.push(ny * w + nx);
                }
            }
        }
        layer = next;
    }
    let spot = |x: f64, z: f64| Spot::new(((x - x0) * CUT_GRID).round() as i32, ((z - z0) * CUT_GRID).round() as i32);
    let mask: Vec<Ring> = sea::trace(&field, w, h, 0.0)
        .iter()
        .map(|ring| ring.iter().map(|p| spot(x0 + (f64::from(p[0]) + 0.5) * CELL, z0 + (f64::from(p[1]) + 0.5) * CELL)).collect())
        .collect();
    let mut exact: Vec<Ring> = Vec::new();
    for paving in found {
        let mut own: Vec<Ring> = Vec::new();
        for piece in &paving.pieces {
            let mut rings: Vec<Ring> = piece.iter().map(|ring| ring.iter().map(|p| spot(p[0], p[2])).collect()).collect();
            // Its outline turning the way outlines do, so that pieces overlapping add up.
            let largest = rings.iter().map(|ring| shapes::twice_area(ring)).max_by_key(|area| area.abs()).unwrap_or(0);
            if largest < 0 {
                rings.iter_mut().for_each(|ring| ring.reverse());
            }
            own.extend(rings);
        }
        let [bx0, bz0, bx1, bz1] = paving.within;
        let (from, to) = (spot(bx0.max(x0), bz0.max(z0)), spot(bx1.min(x1), bz1.min(z1)));
        exact.extend(shapes::intersect(&own, &[shapes::rectangle(from.x, from.y, to.x, to.y)]));
    }
    let rings = shapes::intersect(&exact, &mask)
        .iter()
        .map(|ring| ring.iter().map(|p| [x0 + f64::from(p.x) / CUT_GRID, 0.0, z0 + f64::from(p.y) / CUT_GRID]).collect())
        .collect();
    (rings, roads)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, z0: f64, x1: f64, z1: f64) -> Piece {
        vec![vec![[x0, 0.0, z0], [x1, 0.0, z0], [x1, 0.0, z1], [x0, 0.0, z1]]]
    }

    /// `pieces` as one block's, true everywhere.
    fn paving(pieces: Vec<Piece>) -> [Paving; 1] {
        [Paving { within: [-1e6, -1e6, 1e6, 1e6], pieces, roads: Vec::new() }]
    }

    fn area(rings: &[Vec<[f64; 3]>]) -> f64 {
        let twice: f64 = rings
            .iter()
            .map(|ring| (0..ring.len()).map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                a[0] * b[2] - b[0] * a[2]
            }).sum::<f64>())
            .sum();
        twice.abs() / 2.0
    }

    #[test]
    fn it_is_the_paving_reached_with_its_own_edges() {
        let (found, _) = ground(&[[50.0, 50.0]], &paving(vec![square(0.0, 0.0, 100.0, 100.0)]));
        assert!((area(&found) - 10000.0).abs() < 1.0, "{}", area(&found));
    }

    #[test]
    fn it_goes_on_over_wide_paving_and_not_along_a_sidewalk() {
        let lot = square(0.0, 0.0, 100.0, 100.0);
        let drive = square(100.0, 40.0, 120.0, 52.0);
        let more = square(120.0, 0.0, 200.0, 100.0);
        let sidewalk = square(200.0, 48.0, 240.0, 51.0);
        let town = square(240.0, 0.0, 300.0, 100.0);
        let (found, _) = ground(&[[50.0, 50.0]], &paving(vec![lot, drive, more, sidewalk, town]));
        let joined = 10000.0 + 240.0 + 8000.0;
        // The sidewalk's first metres are within the cut's reach of the ground; the town isn't reached.
        assert!(area(&found) > joined && area(&found) < joined + 60.0, "{}", area(&found));
    }

    #[test]
    fn a_hole_in_the_paving_stays_one() {
        let mut lot = square(0.0, 0.0, 100.0, 100.0);
        lot.extend(square(40.0, 40.0, 60.0, 60.0).into_iter().map(|ring| ring.into_iter().rev().collect()));
        let (found, _) = ground(&[[20.0, 20.0]], &paving(vec![lot]));
        assert!((area(&found) - 9600.0).abs() < 1.0, "{}", area(&found));
    }

    #[test]
    fn a_pump_on_an_island_starts_from_the_paving_beside_it() {
        assert!(area(&ground(&[[-10.0, 50.0]], &paving(vec![square(0.0, 0.0, 100.0, 100.0)])).0) > 9000.0);
        assert!(ground(&[[-100.0, 50.0]], &paving(vec![square(0.0, 0.0, 100.0, 100.0)])).0.is_empty());
    }

    #[test]
    fn blocks_each_say_what_is_true_in_their_own_box() {
        // One piece of paving seen by two blocks: the first takes a bite out of it, in its own box; the second, not knowing
        // what lies there, has it whole, and only its own box counts.
        let whole = square(0.0, 0.0, 200.0, 100.0);
        let bitten = Paving {
            within: [-1e6, -1e6, 100.0, 1e6],
            pieces: vec![square(0.0, 0.0, 200.0, 40.0), square(0.0, 60.0, 200.0, 100.0), square(0.0, 40.0, 50.0, 60.0)],
            roads: Vec::new(),
        };
        let other = Paving { within: [100.0, -1e6, 1e6, 1e6], pieces: vec![whole], roads: Vec::new() };
        let (found, _) = ground(&[[20.0, 20.0]], &[bitten, other]);
        // 50 x 20 of it gone.
        assert!((area(&found) - (20000.0 - 1000.0)).abs() < 1.0, "{}", area(&found));
    }

    #[test]
    fn a_road_meets_the_ground_if_its_surface_reaches_it_and_not_beside_a_verge() {
        let lot = square(0.0, 0.0, 100.0, 100.0);
        let touching = (1, vec![square(100.0, 0.0, 110.0, 100.0)]);
        let behind_a_verge = (2, vec![square(-20.0, 0.0, -8.0, 100.0)]);
        let found = [Paving { within: [-1e6, -1e6, 1e6, 1e6], pieces: vec![lot], roads: vec![touching, behind_a_verge] }];
        assert_eq!(ground(&[[50.0, 50.0]], &found).1, [1]);
    }

    #[test]
    fn it_stops_at_a_road_though_the_paving_goes_on_under_it() {
        // Paving all the way across, with a street over it: only the near lot is the place's.
        let town = square(0.0, 0.0, 300.0, 100.0);
        let street = vec![square(100.0, -50.0, 112.0, 150.0)];
        let found = [Paving { within: [-1e6, -1e6, 1e6, 1e6], pieces: vec![town], roads: vec![(1, street)] }];
        let (rings, roads) = ground(&[[50.0, 50.0]], &found);
        // Up to the street, and a few metres under it: never the far lot, not even along the street's far side.
        assert!(area(&rings) > 10000.0 && area(&rings) < 10000.0 + 700.0, "{}", area(&rings));
        assert_eq!(roads, [1]);
    }

    #[test]
    fn the_island_a_loop_of_road_goes_round_is_no_road() {
        // A junction's surface, a loop: its outline with a hole, where the lay-by's paved island is.
        let mut circle = square(0.0, 0.0, 100.0, 60.0);
        circle.extend(square(10.0, 10.0, 90.0, 50.0).into_iter().map(|ring| ring.into_iter().rev().collect()));
        let found = [Paving { within: [-1e6, -1e6, 1e6, 1e6], pieces: vec![square(10.0, 10.0, 90.0, 50.0)], roads: vec![(1, vec![circle])] }];
        let (rings, roads) = ground(&[[50.0, 30.0]], &found);
        assert!((area(&rings) - 3200.0).abs() < 1.0, "{}", area(&rings));
        assert_eq!(roads, [1]);
    }

    #[test]
    fn points_close_together_are_one_place() {
        let list = places(&[[0.0, 0.0], [50.0, 0.0], [100.0, 30.0], [1000.0, 0.0]]);
        assert_eq!(list.len(), 2);
    }
}
