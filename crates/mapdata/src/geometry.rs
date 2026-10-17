//! Turning the map's nodes and items into shapes on the ground plane (x east, z south, metres).

use std::collections::HashMap;

use crate::prefab::PrefabNode;
use crate::sector::Node;

pub type Point = [f32; 2];

/// Where a node points, on the ground: the direction of travel along a road from its start node to its
/// end node. The node's rotation is a quaternion (w, x, y, z) turning the game's forward (0, 0, -1).
pub fn node_forward(node: &Node) -> Point {
    let [w, _, y, _] = node.rotation;
    let yaw = 2.0 * y.atan2(w);
    [-yaw.sin(), -yaw.cos()]
}

/// How steeply a node points up (+) or down (-): the rise of its direction per metre travelled.
fn node_rise(node: &Node) -> f32 {
    let [w, x, y, z] = node.rotation;
    // The rotated forward (0, 0, -1): its up component, against its length on the ground.
    let up = -2.0 * (y * z - w * x);
    let ground = (1.0 - up * up).max(0.0).sqrt().max(1e-3);
    up / ground
}

pub fn node_point(node: &Node) -> Point {
    [node.position[0], node.position[2]]
}

/// The yaw (about the up axis) that turns the game's forward (0, -1) into `direction`.
fn yaw_of(direction: Point) -> f32 {
    (-direction[0]).atan2(-direction[1])
}

fn rotate(p: Point, yaw: f32) -> Point {
    let (sin, cos) = yaw.sin_cos();
    [p[0] * cos + p[1] * sin, -p[0] * sin + p[1] * cos]
}

/// A road's centre line: a cubic Hermite curve between its nodes, leaving and arriving along their
/// directions, with tangents as long as the straight distance between them (the game's own road curve).
pub fn road_curve(start: &Node, end: &Node) -> Vec<Point> {
    let (p0, p1) = (node_point(start), node_point(end));
    let length = ((p1[0] - p0[0]).powi(2) + (p1[1] - p0[1]).powi(2)).sqrt();
    let (f0, f1) = (node_forward(start), node_forward(end));
    let (m0, m1) = ([f0[0] * length, f0[1] * length], [f1[0] * length, f1[1] * length]);
    let segments = road_segments(start, end);
    (0..=segments)
        .map(|i| {
            let s = i as f32 / segments as f32;
            let (s2, s3) = (s * s, s * s * s);
            let (h00, h10, h01, h11) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2, s3 - s2);
            [
                h00 * p0[0] + h10 * m0[0] + h01 * p1[0] + h11 * m1[0],
                h00 * p0[1] + h10 * m0[1] + h01 * p1[1] + h11 * m1[1],
            ]
        })
        .collect()
}

/// How many segments a road's centre line is drawn in ([`road_curve`]): about one every 8 m, enough for the
/// tightest curves at the closest zoom.
fn road_segments(start: &Node, end: &Node) -> usize {
    let (p0, p1) = (node_point(start), node_point(end));
    let length = ((p1[0] - p0[0]).powi(2) + (p1[1] - p0[1]).powi(2)).sqrt();
    ((length / 8.0).ceil() as usize).clamp(1, 64)
}

/// How high a road's centre line is at each of [`road_curve`]'s points, metres: climbing between its nodes
/// along their slopes ([`curve_frames`]).
pub fn road_heights(start: &Node, end: &Node) -> Vec<f32> {
    curve_frames(start, end, road_segments(start, end)).iter().map(|frame| frame.height).collect()
}

/// `points` moved `shift` metres to the right of the way they run (x east, z south), each across the line
/// there (along the bisector of its two segments).
pub fn offset_line(points: &[Point], shift: f32) -> Vec<Point> {
    if shift == 0.0 {
        return points.to_vec();
    }
    offset_line_by(points, &vec![shift; points.len()])
}

/// `points` each moved its own `shifts` metres to the right of the way they run, as [`offset_line`].
pub fn offset_line_by(points: &[Point], shifts: &[f32]) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let along = |a: Point, b: Point| {
        let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
        let length = dx.hypot(dz);
        (length > 0.0).then(|| [dx / length, dz / length])
    };
    (0..points.len())
        .map(|i| {
            let before = i.checked_sub(1).and_then(|b| along(points[b], points[i]));
            let after = points.get(i + 1).and_then(|&next| along(points[i], next));
            let way = match (before, after) {
                (Some(b), Some(a)) => {
                    let sum = [b[0] + a[0], b[1] + a[1]];
                    let length = sum[0].hypot(sum[1]);
                    if length > 1e-6 { [sum[0] / length, sum[1] / length] } else { a }
                }
                (Some(d), None) | (None, Some(d)) => d,
                (None, None) => return points[i],
            };
            let shift = shifts.get(i).copied().unwrap_or(0.0);
            [points[i][0] - way[1] * shift, points[i][1] + way[0] * shift]
        })
        .collect()
}

/// A point along a road or terrain curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub point: Point,
    /// The direction to the right of travel, on the ground.
    pub right: Point,
    /// The curve's height there, metres.
    pub height: f32,
}

/// Points along a road or terrain curve, `segments` equal steps of its parameter.
pub fn curve_frames(start: &Node, end: &Node, segments: usize) -> Vec<Frame> {
    let (p0, p1) = (node_point(start), node_point(end));
    let length = ((p1[0] - p0[0]).powi(2) + (p1[1] - p0[1]).powi(2)).sqrt();
    let (f0, f1) = (node_forward(start), node_forward(end));
    let (m0, m1) = ([f0[0] * length, f0[1] * length], [f1[0] * length, f1[1] * length]);
    let (y0, y1) = (start.position[1], end.position[1]);
    let (n0, n1) = (node_rise(start) * length, node_rise(end) * length);
    (0..=segments.max(1))
        .map(|i| {
            let s = i as f32 / segments.max(1) as f32;
            let (s2, s3) = (s * s, s * s * s);
            let (h00, h10, h01, h11) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2, s3 - s2);
            let point = [
                h00 * p0[0] + h10 * m0[0] + h01 * p1[0] + h11 * m1[0],
                h00 * p0[1] + h10 * m0[1] + h01 * p1[1] + h11 * m1[1],
            ];
            // The curve's direction (its derivative), turned a quarter to the right.
            let (d00, d10, d01, d11) =
                (6.0 * s2 - 6.0 * s, 3.0 * s2 - 4.0 * s + 1.0, -6.0 * s2 + 6.0 * s, 3.0 * s2 - 2.0 * s);
            let tangent = [
                d00 * p0[0] + d10 * m0[0] + d01 * p1[0] + d11 * m1[0],
                d00 * p0[1] + d10 * m0[1] + d01 * p1[1] + d11 * m1[1],
            ];
            let norm = (tangent[0].powi(2) + tangent[1].powi(2)).sqrt().max(f32::EPSILON);
            Frame {
                point,
                right: [-tangent[1] / norm, tangent[0] / norm],
                height: h00 * y0 + h10 * n0 + h01 * y1 + h11 * n1,
            }
        })
        .collect()
}

/// A point on a 4×4 bezier patch at (`u`, `v`), both 0..1. `points` are `[x][z]`, each (x, y, z).
pub fn bezier_patch_point(points: &[[[f32; 3]; 4]; 4], u: f32, v: f32) -> [f32; 3] {
    let basis = |t: f32| {
        let s = 1.0 - t;
        [s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t]
    };
    let (bu, bv) = (basis(u), basis(v));
    let mut point = [0.0; 3];
    for (i, row) in points.iter().enumerate() {
        for (j, control) in row.iter().enumerate() {
            let weight = bu[i] * bv[j];
            for axis in 0..3 {
                point[axis] += weight * control[axis];
            }
        }
    }
    point
}

/// Turns a point given relative to `node` (in its own frame: forward is -z) into the world.
pub fn from_node_frame(node: &Node, p: Point) -> Point {
    let r = rotate(p, yaw_of(node_forward(node)));
    [node.position[0] + r[0], node.position[2] + r[1]]
}

/// Places a prefab: its `origin` node sits on the world node `at`, turned to point the same way. Maps
/// points in the prefab's own space to the world.
pub struct PrefabPlacement {
    origin: Point,
    at: Point,
    yaw: f32,
}

impl PrefabPlacement {
    pub fn new(origin: &PrefabNode, at: &Node) -> Self {
        Self { origin: origin.position, at: node_point(at), yaw: yaw_of(node_forward(at)) - yaw_of(origin.direction) }
    }

    pub fn place(&self, p: Point) -> Point {
        let r = rotate([p[0] - self.origin[0], p[1] - self.origin[1]], self.yaw);
        [self.at[0] + r[0], self.at[1] + r[1]]
    }
}

/// Which of a prefab item's world nodes the prefab's own node `index` is: the item lists its nodes
/// starting from the prefab's origin node.
pub fn prefab_world_node(index: usize, origin: usize, node_count: usize) -> usize {
    (index + node_count - origin % node_count) % node_count
}

/// Which of a prefab's own nodes the item's world node `world` is ([`prefab_world_node`] the other way).
pub fn prefab_own_node(world: usize, origin: usize, node_count: usize) -> usize {
    (world + origin) % node_count.max(1)
}

/// A line joined from pieces: its points, and which pieces it was made of.
#[derive(Clone, Debug, PartialEq)]
pub struct JoinedLine {
    pub points: Vec<Point>,
    pub pieces: Vec<usize>,
}

/// The sharpest turn a joined line takes where three or more pieces meet (where two do, it follows
/// whatever way the road goes), radians: about 50°.
pub(crate) const STROKE_TURN: f32 = 0.87;

/// Joins lines into longer ones where their ends meet (within `snap` metres; a gap is bridged, no point
/// moves): where two ends meet they join; where three or more do (a junction), the line goes on along
/// the straightest piece, and the others start lines of their own ("strokes", as cartographers build
/// them). Lines of fewer than two points are left out. The same lines always join the same way.
pub fn join_lines(lines: &[Vec<Point>], snap: f32) -> Vec<JoinedLine> {
    const SAME: f32 = 0.01;
    let cell = snap.max(SAME);
    let cell_of = |p: Point| ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64);
    // Each line end, by the cell it's in: which line, and whether it's the line's start.
    let mut cells: HashMap<(i64, i64), Vec<(usize, bool)>> = HashMap::new();
    for (index, line) in lines.iter().enumerate().filter(|(_, line)| line.len() >= 2) {
        cells.entry(cell_of(line[0])).or_default().push((index, true));
        cells.entry(cell_of(line[line.len() - 1])).or_default().push((index, false));
    }
    let end_at =
        |(index, start): (usize, bool)| if start { lines[index][0] } else { lines[index][lines[index].len() - 1] };
    let ends_near = |p: Point| -> Vec<(usize, bool)> {
        let (cx, cy) = cell_of(p);
        let mut near: Vec<(usize, bool)> = (-1..=1)
            .flat_map(|dx| (-1..=1).map(move |dy| (cx + dx, cy + dy)))
            .filter_map(|key| cells.get(&key))
            .flatten()
            .copied()
            .filter(|&end| distance(end_at(end), p) <= cell)
            .collect();
        near.sort_unstable();
        near
    };
    // A piece starting from `end`, oriented away from it.
    let from_end = |(index, start): (usize, bool)| -> Vec<Point> {
        if start { lines[index].clone() } else { lines[index].iter().rev().copied().collect() }
    };
    // Where a line heading `heading` into `p` goes on: the one other end there, or at a junction the
    // straightest way on.
    let way_on = |p: Point, heading: Point, used: &[bool]| -> Option<(usize, bool)> {
        let ends = ends_near(p);
        let free = ends.iter().copied().filter(|&(index, _)| !used[index]);
        if ends.len() == 2 {
            return free.into_iter().next();
        }
        free.map(|end| {
            let piece = from_end(end);
            (turn(heading, [piece[1][0] - piece[0][0], piece[1][1] - piece[0][1]]), end)
        })
        .filter(|(angle, _)| *angle < STROKE_TURN)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, end)| end)
    };

    let mut used = vec![false; lines.len()];
    let mut joined = Vec::new();
    for first in 0..lines.len() {
        if used[first] || lines[first].len() < 2 {
            continue;
        }
        used[first] = true;
        let mut points = lines[first].clone();
        let mut pieces = vec![first];
        // On from its end, then back from its start.
        loop {
            let (tail, before) = (points[points.len() - 1], points[points.len() - 2]);
            let Some(end) = way_on(tail, [tail[0] - before[0], tail[1] - before[1]], &used) else { break };
            used[end.0] = true;
            pieces.push(end.0);
            let piece = from_end(end);
            let skip = usize::from(distance(piece[0], tail) <= SAME);
            points.extend(&piece[skip..]);
        }
        loop {
            let (head, after) = (points[0], points[1]);
            let Some(end) = way_on(head, [head[0] - after[0], head[1] - after[1]], &used) else { break };
            used[end.0] = true;
            pieces.insert(0, end.0);
            let mut piece = from_end(end);
            piece.reverse();
            if distance(piece[piece.len() - 1], head) <= SAME {
                piece.pop();
            }
            points.splice(0..0, piece);
        }
        joined.push(JoinedLine { points, pieces });
    }
    joined
}

fn distance(a: Point, b: Point) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The angle between two directions, radians (a straight on 0, a U-turn π; π for a direction of no length).
pub(crate) fn turn(a: Point, b: Point) -> f32 {
    let lengths = a[0].hypot(a[1]) * b[0].hypot(b[1]);
    if lengths == 0.0 {
        return std::f32::consts::PI;
    }
    ((a[0] * b[0] + a[1] * b[1]) / lengths).clamp(-1.0, 1.0).acos()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(x: f32, z: f32, yaw: f32) -> Node {
        let half = yaw / 2.0;
        Node {
            uid: 0,
            position: [x, 0.0, z],
            rotation: [half.cos(), 0.0, half.sin(), 0.0],
            backward_item: 0,
            forward_item: 0,
            flags: 0,
        }
    }

    fn close(a: Point, b: Point) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn offsets_a_line_to_the_right_of_its_way() {
        // Heading north (-z): right is east. A corner moves along its bisector.
        let line = offset_line(&[[0.0, 0.0], [0.0, -10.0], [10.0, -10.0]], 2.0);
        assert_eq!(line[0], [2.0, 0.0]);
        assert!((line[2][0] - 10.0).abs() < 1e-6 && (line[2][1] + 8.0).abs() < 1e-6);
        assert!((line[1][0] - 2f32.sqrt()).abs() < 1e-5 && (line[1][1] + 10.0 - 2f32.sqrt()).abs() < 1e-5);
    }

    #[test]
    fn unturned_nodes_face_north() {
        assert!(close(node_forward(&node(0.0, 0.0, 0.0)), [0.0, -1.0]));
        // A quarter turn left (counter-clockwise seen from above) faces west.
        assert!(close(node_forward(&node(0.0, 0.0, std::f32::consts::FRAC_PI_2)), [-1.0, 0.0]));
    }

    #[test]
    fn straight_roads_stay_straight_and_end_on_their_nodes() {
        let points = road_curve(&node(0.0, 0.0, 0.0), &node(0.0, -100.0, 0.0));
        assert!(close(points[0], [0.0, 0.0]) && close(*points.last().unwrap(), [0.0, -100.0]));
        assert!(points.iter().all(|p| p[0].abs() < 1e-3));
    }

    #[test]
    fn curve_frames_point_right_of_travel() {
        let frames = curve_frames(&node(0.0, 0.0, 0.0), &node(0.0, -100.0, 0.0), 4);
        assert_eq!(frames.len(), 5);
        // Driving north: right is east.
        assert!(frames.iter().all(|frame| close(frame.right, [1.0, 0.0])));
        assert!(close(frames[2].point, [0.0, -50.0]));
    }

    #[test]
    fn a_roads_heights_are_at_its_curves_points() {
        let mut high = node(30.0, -100.0, 0.0);
        high.position[1] = 10.0;
        let low = node(0.0, 0.0, 0.0);
        let curve = road_curve(&low, &high);
        let heights = road_heights(&low, &high);
        assert_eq!(heights.len(), curve.len());
        assert_eq!((heights[0], heights[heights.len() - 1]), (0.0, 10.0));
        let frames = curve_frames(&low, &high, curve.len() - 1);
        assert!(frames.iter().zip(&curve).all(|(frame, point)| close(frame.point, *point)));
    }

    #[test]
    fn curve_frames_climb_between_their_nodes() {
        let mut low = node(0.0, 0.0, 0.0);
        let mut high = node(0.0, -100.0, 0.0);
        high.position[1] = 10.0;
        let frames = curve_frames(&low, &high, 2);
        assert!((frames[0].height, frames[1].height, frames[2].height) == (0.0, 5.0, 10.0));
        // Level nodes, but both pointing up a little: the curve bulges up between them.
        let tilt = |n: &mut Node| n.rotation = [0.9998, 0.02, 0.0, 0.0];
        tilt(&mut low);
        high.position[1] = 0.0;
        tilt(&mut high);
        assert!(curve_frames(&low, &high, 2)[1].height.abs() < 1e-3, "rise at the start, fall at the end cancel");
    }

    #[test]
    fn bezier_patches_pass_through_their_corners() {
        let mut points = [[[0.0; 3]; 4]; 4];
        for (i, row) in points.iter_mut().enumerate() {
            for (j, p) in row.iter_mut().enumerate() {
                *p = [i as f32 * 10.0, 2.0, j as f32 * 10.0];
            }
        }
        let ground = |p: [f32; 3]| [p[0], p[2]];
        assert!(close(ground(bezier_patch_point(&points, 0.0, 0.0)), [0.0, 0.0]));
        assert!(close(ground(bezier_patch_point(&points, 1.0, 1.0)), [30.0, 30.0]));
        assert!(close(ground(bezier_patch_point(&points, 0.5, 0.5)), [15.0, 15.0]));
        assert!((bezier_patch_point(&points, 0.3, 0.7)[1] - 2.0).abs() < 1e-4);
    }

    #[test]
    fn prefabs_turn_with_their_node() {
        let origin = PrefabNode {
            height: 0.0,
            position: [10.0, 0.0],
            direction: [0.0, -1.0],
            lanes: 2,
            curves_in: Vec::new(),
            curves_out: Vec::new(),
        };
        // The world node faces west: the prefab's north becomes west.
        let placement = PrefabPlacement::new(&origin, &node(100.0, 100.0, std::f32::consts::FRAC_PI_2));
        assert!(close(placement.place([10.0, 0.0]), [100.0, 100.0]));
        assert!(close(placement.place([10.0, -5.0]), [95.0, 100.0]));
    }

    #[test]
    fn prefab_items_list_nodes_from_the_origin() {
        assert_eq!(prefab_world_node(2, 2, 4), 0);
        assert_eq!(prefab_world_node(1, 2, 4), 3);
        assert_eq!(prefab_world_node(0, 0, 3), 0);
    }

    fn points(joined: &[JoinedLine]) -> Vec<Vec<Point>> {
        joined.iter().map(|line| line.points.clone()).collect()
    }

    #[test]
    fn joins_lines_end_to_end_turning_them_round() {
        // A road in three pieces, the middle one drawn the other way.
        let road = [vec![[10.0, 0.0], [20.0, 0.0]], vec![[0.0, 0.0], [10.0, 0.0]], vec![[30.0, 0.0], [20.0, 0.0]]];
        let joined = join_lines(&road, 0.01);
        assert_eq!(points(&joined), [vec![[0.0, 0.0], [10.0, 0.0], [20.0, 0.0], [30.0, 0.0]]]);
        assert_eq!(joined[0].pieces, [1, 0, 2]);
    }

    #[test]
    fn goes_straight_on_through_a_junction() {
        // A road through a junction, and a branch off it at a right angle: the road joins, the branch
        // stays its own line.
        let junction = [vec![[0.0, 0.0], [10.0, 0.0]], vec![[10.0, 0.0], [10.0, 10.0]], vec![[10.0, 0.0], [20.0, 0.0]]];
        assert_eq!(
            points(&join_lines(&junction, 0.01)),
            [vec![[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]], vec![[10.0, 0.0], [10.0, 10.0]]]
        );
        // A Y, three ways 120° apart: none goes straight on into another.
        let y = [vec![[0.0, 0.0], [10.0, 0.0]], vec![[10.0, 0.0], [5.0, 8.66]], vec![[10.0, 0.0], [5.0, -8.66]]];
        assert_eq!(join_lines(&y, 0.01).len(), 3);
    }

    #[test]
    fn bridges_small_gaps_without_moving_points_and_stops_round_a_loop() {
        let gap = [vec![[0.0, 0.0], [5.0, 0.0]], vec![[8.0, 0.0], [15.0, 0.0]]];
        assert_eq!(join_lines(&gap, 0.01).len(), 2);
        assert_eq!(points(&join_lines(&gap, 5.0)), [vec![[0.0, 0.0], [5.0, 0.0], [8.0, 0.0], [15.0, 0.0]]]);
        let ring = [vec![[0.0, 0.0], [1.0, 0.0]], vec![[1.0, 0.0], [1.0, 1.0]], vec![[1.0, 1.0], [0.0, 0.0]]];
        let joined = join_lines(&ring, 0.01);
        assert_eq!((joined.len(), joined[0].points.len()), (1, 4));
    }
}
