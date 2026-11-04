//! Polygons: outlines joined, cut and thinned exactly, on whole numbers so that touching shapes really touch.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::{Overlay, ShapeType};
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::i_float::int::point::IntPoint;

/// A point on the grid outlines are worked on.
pub type Spot = IntPoint<i32>;
/// A closed outline. Outlines turn one way, holes the other.
pub type Ring = Vec<Spot>;

/// Twice a ring's signed area.
pub fn twice_area(ring: &[Spot]) -> i64 {
    let mut sum = 0i64;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        sum += i64::from(a.x) * i64::from(b.y) - i64::from(b.x) * i64::from(a.y);
    }
    sum
}

/// `ring` turned the way outlines turn, so that overlapping outlines add up rather than cancel.
pub fn outward(mut ring: Ring) -> Ring {
    if twice_area(&ring) < 0 {
        ring.reverse();
    }
    ring
}

fn run(subject: &[Ring], clip: &[Ring], rule: OverlayRule) -> Vec<Ring> {
    let edges: usize = subject.iter().chain(clip).map(Vec::len).sum();
    if edges == 0 {
        return Vec::new();
    }
    let mut overlay = Overlay::<i32>::new(edges);
    for ring in subject {
        overlay.add_contour(ring, ShapeType::Subject);
    }
    for ring in clip {
        overlay.add_contour(ring, ShapeType::Clip);
    }
    overlay.overlay(rule, FillRule::NonZero).into_iter().flatten().collect()
}

/// The area `rings` cover together, as outlines and holes.
pub fn union(rings: &[Ring]) -> Vec<Ring> {
    run(rings, &[], OverlayRule::Subject)
}

/// Where `rings` and `cut` overlap.
pub fn intersect(rings: &[Ring], cut: &[Ring]) -> Vec<Ring> {
    run(rings, cut, OverlayRule::Intersect)
}

/// Where `rings` and `cut` overlap, each outline with its holes.
pub fn intersect_shapes(rings: &[Ring], cut: &[Ring]) -> Vec<Vec<Ring>> {
    let edges: usize = rings.iter().chain(cut).map(Vec::len).sum();
    if edges == 0 {
        return Vec::new();
    }
    let mut overlay = Overlay::<i32>::new(edges);
    for ring in rings {
        overlay.add_contour(ring, ShapeType::Subject);
    }
    for ring in cut {
        overlay.add_contour(ring, ShapeType::Clip);
    }
    overlay.overlay(OverlayRule::Intersect, FillRule::NonZero)
}

/// `rings` without `cut`.
pub fn difference(rings: &[Ring], cut: &[Ring]) -> Vec<Ring> {
    if cut.is_empty() {
        return rings.to_vec();
    }
    run(rings, cut, OverlayRule::Difference)
}

/// The outlines of `rings` with their holes, each group an outline and the holes inside it.
pub fn shapes(rings: &[Ring]) -> Vec<Vec<Ring>> {
    let edges: usize = rings.iter().map(Vec::len).sum();
    if edges == 0 {
        return Vec::new();
    }
    let mut overlay = Overlay::<i32>::new(edges);
    for ring in rings {
        overlay.add_contour(ring, ShapeType::Subject);
    }
    overlay.overlay(OverlayRule::Subject, FillRule::NonZero)
}

/// A rectangle, as an outline.
pub fn rectangle(x0: i32, y0: i32, x1: i32, y1: i32) -> Ring {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].into_iter().map(|(x, y)| Spot::new(x, y)).collect()
}

fn distance_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let long = dx * dx + dy * dy;
    let t = if long > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / long).clamp(0.0, 1.0) } else { 0.0 };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// A line's points kept where they stray more than `tolerance` from the line through their neighbours
/// (Douglas-Peucker); its ends stay.
pub fn thin_line<T: Copy>(points: &[T], at: impl Fn(&T) -> [f64; 2], tolerance: f64) -> Vec<T> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (from, to) = (at(&points[a]), at(&points[b]));
        let (mut worst, mut index) = (0.0, 0);
        for (i, point) in points.iter().enumerate().take(b).skip(a + 1) {
            let gap = distance_to_segment(at(point), from, to);
            if gap > worst {
                (worst, index) = (gap, i);
            }
        }
        if worst > tolerance {
            keep[index] = true;
            stack.push((a, index));
            stack.push((index, b));
        }
    }
    points.iter().zip(keep).filter_map(|(point, keep)| keep.then_some(*point)).collect()
}

/// A ring's points thinned as [`thin_line`]; nothing if too little of it is left.
pub fn thin_ring(ring: &[Spot], tolerance: f64) -> Option<Ring> {
    if ring.len() < 3 {
        return None;
    }
    // Cut at its two points furthest apart, so neither half is a closed line.
    let at = |p: &Spot| [f64::from(p.x), f64::from(p.y)];
    let far = (1..ring.len())
        .max_by_key(|&i| {
            let (dx, dy) = (i64::from(ring[i].x - ring[0].x), i64::from(ring[i].y - ring[0].y));
            dx * dx + dy * dy
        })
        .unwrap_or(1);
    let first: Vec<Spot> = ring[..=far].to_vec();
    let mut second: Vec<Spot> = ring[far..].to_vec();
    second.push(ring[0]);
    let mut out = thin_line(&first, at, tolerance);
    out.pop();
    let mut rest = thin_line(&second, at, tolerance);
    rest.pop();
    out.extend(rest);
    (out.len() >= 3 && twice_area(&out) != 0).then_some(out)
}

/// The sides of a regular polygon round a lane's point: its round end and bends.
const ROUND: usize = 12;

/// The area within `reach` of `lines`, on the grid `scale` units a metre.
pub fn around_lines(lines: &[Vec<[f64; 2]>], reach: f64, scale: f64) -> Vec<Ring> {
    let spot = |x: f64, z: f64| Spot::new((x * scale).round() as i32, (z * scale).round() as i32);
    let mut pieces = Vec::new();
    for line in lines {
        for pair in line.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
            let long = dx.hypot(dz);
            if long < 1e-6 {
                continue;
            }
            let (nx, nz) = (-dz / long * reach, dx / long * reach);
            pieces.push(outward(vec![
                spot(a[0] + nx, a[1] + nz),
                spot(b[0] + nx, b[1] + nz),
                spot(b[0] - nx, b[1] - nz),
                spot(a[0] - nx, a[1] - nz),
            ]));
        }
        for p in line {
            let round: Ring = (0..ROUND)
                .map(|k| {
                    let turn = k as f64 / ROUND as f64 * std::f64::consts::TAU;
                    spot(p[0] + reach * turn.cos(), p[1] + reach * turn.sin())
                })
                .collect();
            pieces.push(outward(round));
        }
    }
    union(&pieces)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: i32, y: i32, size: i32) -> Ring {
        rectangle(x, y, x + size, y + size)
    }

    fn area(rings: &[Ring]) -> i64 {
        rings.iter().map(|ring| twice_area(ring)).sum::<i64>() / 2
    }

    #[test]
    fn overlapping_outlines_add_up_whichever_way_they_turn() {
        let mut back = square(5, 0, 10);
        back.reverse();
        let joined = union(&[outward(square(0, 0, 10)), outward(back)]);
        assert_eq!(joined.len(), 1);
        assert_eq!(area(&joined), 150);
    }

    #[test]
    fn a_hole_stays_a_hole() {
        let frame = difference(&[square(0, 0, 10)], &[square(3, 3, 4)]);
        assert_eq!(area(&frame), 84);
        // Joined again with something beside it, the hole is still there.
        assert_eq!(area(&union(&[frame.clone(), vec![square(20, 0, 2)]].concat())), 88);
        assert_eq!(area(&intersect(&frame, &[square(0, 0, 5)])), 25 - 4);
    }

    #[test]
    fn thins_a_ring_and_keeps_its_corners() {
        let mut ring = Vec::new();
        for x in 0..10 {
            ring.push(Spot::new(x, 0));
        }
        ring.extend([Spot::new(10, 0), Spot::new(10, 10), Spot::new(0, 10)]);
        assert_eq!(thin_ring(&ring, 0.1).unwrap().len(), 4);
    }

    #[test]
    fn what_a_line_reaches_is_its_width_and_its_round_ends() {
        let around = around_lines(&[vec![[0.0, 0.0], [10.0, 0.0]]], 1.0, 16.0);
        let square_metres = area(&around) as f64 / (16.0 * 16.0);
        // 20 m² of strip and a circle of 1 m's radius, as twelve sides.
        assert!((square_metres - 23.0).abs() < 0.1, "{square_metres}");
    }
}
