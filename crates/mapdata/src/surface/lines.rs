//! Road lines for the map seen from far, where a road is narrower than a pixel and its surface can't show: each road's
//! carriageways, and through a prefab the way its lanes go. And tunnels: which stretches of a road run in a tube.

use std::collections::HashMap;

use crate::area::to_mercator;
use crate::geometry::{self, Point};
use crate::tiles::{Feature, Geometry, Value};
use crate::world::RoadClass;

/// The first zoom a junction's lines show from: too small to see further out, and there are many of them.
const JUNCTION_ZOOM: u8 = 11;
/// The last zoom roads are drawn generalised: by class alone, at their class's typical width, junction pieces joined in
/// with the roads so motorways run on through interchanges, small gaps bridged.
const GENERAL_ROADS_ZOOM: u8 = 11;
/// The gap a generalised road is joined across, metres: a seventh of a pixel at the last general zoom.
const GENERAL_ROADS_SNAP: f32 = 5.0;
/// Up close, only ends that meet: a centimetre.
const DETAILED_ROADS_SNAP: f32 = 0.01;

/// How far a tube's line may lie from a road's beyond the road's half width and still be its tunnel, metres: the tube
/// runs along the road's middle, its lights along its walls.
const TUBE_REACH: f32 = 4.0;
/// How much higher or lower than a road a tube may lie and still be its tunnel, metres: its lights under its roof. A road
/// over the tunnel lies far higher, or crosses it.
const TUBE_HEIGHT: f32 = 8.0;
/// How nearly along a road a tube must run to be its tunnel: the cosine of the angle between them (a road crossing over a
/// tunnel's mouth runs across it).
const TUBE_ALONG: f32 = 0.9;
/// The shortest stretch in a tunnel or out of one within a road (a gap between a tube's pieces, a road brushing a tube's
/// end), metres: shorter, it's taken as the stretches either side.
const TUBE_GAP: f32 = 20.0;
/// How far past a tube segment's end a road's step may lie and still be beside it, metres (where its pieces meet at an
/// angle).
const TUBE_END: f32 = 0.5;
/// The grid the tubes are found in, metres a cell: wider than any road's reach to its tube.
const TUBE_CELL: f32 = 64.0;

/// The first zoom a class of road shows from.
pub fn road_zoom(class: RoadClass) -> u8 {
    match class {
        RoadClass::Motorway => 6,
        RoadClass::Trunk => 8,
        RoadClass::Local => 11,
        RoadClass::Rail => 12,
        RoadClass::Minor => 13,
    }
}

/// A road's line, or a line through a junction, before they're joined into longer lines.
pub struct RoadLine {
    pub points: Vec<Point>,
    pub class: RoadClass,
    pub width: f32,
    pub minzoom: u8,
    pub tunnel: bool,
}

impl RoadLine {
    /// A junction's way through.
    pub fn junction(points: Vec<Point>, class: RoadClass) -> Self {
        let minzoom = road_zoom(class).max(JUNCTION_ZOOM);
        Self { points, class, width: class.typical_width(), minzoom, tunnel: false }
    }
}

fn direction(a: Point, b: Point) -> Option<Point> {
    let (x, z) = (b[0] - a[0], b[1] - a[1]);
    let long = x.hypot(z);
    (long > 0.0).then(|| [x / long, z / long])
}

/// A segment of a tube: its ends, and how high each is.
type TubeSegment = (Point, Point, f32, f32);

/// Tunnels' tubes as their curve items lay them, or what the game lays along their insides (lights, kerbs).
#[derive(Default)]
pub struct Tubes {
    grid: HashMap<(i32, i32), Vec<TubeSegment>>,
}

fn cell(p: Point) -> (i32, i32) {
    ((p[0] / TUBE_CELL).floor() as i32, (p[1] / TUBE_CELL).floor() as i32)
}

impl Tubes {
    pub fn add(&mut self, points: &[Point], heights: &[f32]) {
        for (i, pair) in points.windows(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            let (Some(&ha), Some(&hb)) = (heights.get(i), heights.get(i + 1)) else { continue };
            let ((x0, z0), (x1, z1)) = (cell([a[0].min(b[0]), a[1].min(b[1])]), cell([a[0].max(b[0]), a[1].max(b[1])]));
            for x in x0..=x1 {
                for z in z0..=z1 {
                    self.grid.entry((x, z)).or_default().push((a, b, ha, hb));
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.grid.is_empty()
    }

    fn holds(&self, at: Point, height: f32, way: Point, width: f32) -> bool {
        let (x, z) = cell(at);
        (x - 1..=x + 1).flat_map(|x| (z - 1..=z + 1).map(move |z| (x, z))).any(|key| {
            self.grid.get(&key).into_iter().flatten().any(|&(a, b, ha, hb)| {
                let Some(along) = direction(a, b) else { return false };
                if (along[0] * way[0] + along[1] * way[1]).abs() < TUBE_ALONG {
                    return false;
                }
                // Beside the segment, not past its ends (where the tube stops, the tunnel does).
                let long = (b[0] - a[0]).hypot(b[1] - a[1]);
                let onto = (at[0] - a[0]) * along[0] + (at[1] - a[1]) * along[1];
                if onto < -TUBE_END || onto > long + TUBE_END {
                    return false;
                }
                let t = (onto / long).clamp(0.0, 1.0);
                let on = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                (at[0] - on[0]).hypot(at[1] - on[1]) <= width / 2.0 + TUBE_REACH
                    && (height - (ha + (hb - ha) * t)).abs() <= TUBE_HEIGHT
            })
        })
    }

    /// Which steps of a road's line (x, y, z points) run in a tube.
    pub fn steps_inside(&self, line: &[[f64; 3]], width: f32) -> Vec<bool> {
        let mut inside: Vec<bool> = line
            .windows(2)
            .map(|pair| {
                let (a, b) = ([pair[0][0] as f32, pair[0][2] as f32], [pair[1][0] as f32, pair[1][2] as f32]);
                let middle = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
                let height = ((pair[0][1] + pair[1][1]) / 2.0) as f32;
                direction(a, b).is_some_and(|way| self.holds(middle, height, way, width))
            })
            .collect();
        if !inside.contains(&true) {
            return inside;
        }
        // Short stretches within the road, in or out, taken as those either side (not at its ends: the road before or
        // after it goes on in or out).
        let along: Vec<f32> = std::iter::once(0.0)
            .chain(line.windows(2).scan(0.0f32, |sum, pair| {
                *sum += (pair[1][0] - pair[0][0]).hypot(pair[1][2] - pair[0][2]) as f32;
                Some(*sum)
            }))
            .collect();
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for step in 0..inside.len() {
            match runs.last_mut() {
                Some((_, end)) if inside[*end - 1] == inside[step] => *end = step + 1,
                _ => runs.push((step, step + 1)),
            }
        }
        for &(start, end) in runs.iter().skip(1).take(runs.len().saturating_sub(2)) {
            if along[end] - along[start] < TUBE_GAP {
                let flipped = !inside[start];
                inside[start..end].iter_mut().for_each(|step| *step = flipped);
            }
        }
        inside
    }
}

/// The roads as tile features: their pieces joined into lines ([`geometry::join_lines`]), once for further out and once,
/// generalised, for further out still (a joined line appears from its earliest piece's zoom). Groups keep the order they
/// first appear in, so the export is reproducible.
pub fn road_features(roads: Vec<RoadLine>) -> Vec<Feature> {
    fn grouped<K: Copy + Eq + std::hash::Hash>(
        roads: &[RoadLine],
        key: impl Fn(&RoadLine) -> Option<K>,
    ) -> Vec<(K, Vec<usize>)> {
        let mut index: HashMap<K, usize> = HashMap::new();
        let mut groups: Vec<(K, Vec<usize>)> = Vec::new();
        for (at, road) in roads.iter().enumerate() {
            let Some(k) = key(road) else { continue };
            let group = *index.entry(k).or_insert_with(|| {
                groups.push((k, Vec::new()));
                groups.len() - 1
            });
            groups[group].1.push(at);
        }
        groups
    }
    // A group's lines, each with the zoom its earliest piece appears at.
    fn joined(roads: &[RoadLine], members: &[usize], snap: f32) -> Vec<(Vec<Point>, u8)> {
        let lines: Vec<Vec<Point>> = members.iter().map(|&at| roads[at].points.clone()).collect();
        let minzoom = |pieces: &[usize]| pieces.iter().map(|&piece| roads[members[piece]].minzoom).min();
        geometry::join_lines(&lines, snap)
            .into_iter()
            .map(|line| (line.points, minzoom(&line.pieces).unwrap_or(u8::MAX)))
            .collect()
    }
    let mut features = Vec::new();
    let detailed = |road: &RoadLine| Some((road.class, road.width.to_bits(), road.tunnel));
    for ((class, width, tunnel), members) in grouped(&roads, detailed) {
        for (line, minzoom) in joined(&roads, &members, DETAILED_ROADS_SNAP) {
            let minzoom = minzoom.max(GENERAL_ROADS_ZOOM + 1);
            features.push(road_feature(&line, class, f32::from_bits(width), minzoom, u8::MAX, tunnel));
        }
    }
    let general = |road: &RoadLine| (road.minzoom <= GENERAL_ROADS_ZOOM).then_some((road.class, road.tunnel));
    for ((class, tunnel), members) in grouped(&roads, general) {
        for (line, minzoom) in joined(&roads, &members, GENERAL_ROADS_SNAP) {
            features.push(road_feature(&line, class, class.typical_width(), minzoom, GENERAL_ROADS_ZOOM, tunnel));
        }
    }
    features
}

fn road_feature(line: &[Point], class: RoadClass, width: f32, minzoom: u8, maxzoom: u8, tunnel: bool) -> Feature {
    let mut props = vec![("class", Value::Str(class.name().into())), ("width", Value::Num(f64::from(width)))];
    if tunnel {
        props.push(("tunnel", Value::Bool(true)));
    }
    Feature {
        layer: "road",
        minzoom,
        maxzoom,
        props,
        geometry: Geometry::Lines(vec![line.iter().map(|&p| to_mercator(p)).collect()]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tube() -> Tubes {
        let mut tubes = Tubes::default();
        tubes.add(&[[0.0, 0.0], [100.0, 0.0]], &[0.0, 0.0]);
        tubes
    }

    fn road(z: f64, y: f64, across: bool) -> Vec<[f64; 3]> {
        (0..30).map(|i| if across { [50.0, y, -60.0 + 4.0 * f64::from(i)] } else { [-30.0 + 8.0 * f64::from(i), y, z] }).collect()
    }

    #[test]
    fn marks_the_stretch_of_a_road_in_a_tube() {
        let inside = tube().steps_inside(&road(1.0, 0.0, false), 8.0);
        // From x -30 by 8: out to 0, in along the tube's 100 m, out again.
        assert!(!inside[0] && inside[10] && !inside[20]);
    }

    #[test]
    fn a_road_over_a_tunnel_or_across_it_is_not_in_it() {
        assert!(!tube().steps_inside(&road(1.0, 30.0, false), 8.0).contains(&true));
        assert!(!tube().steps_inside(&road(0.0, 0.0, true), 8.0).contains(&true));
    }
}
