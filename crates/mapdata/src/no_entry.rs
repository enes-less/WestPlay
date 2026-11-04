//! No-entry signs at the entrances of the roads the game leaves off its own map (owner 2026-11-01). It's the road items the
//! game hides that count: a junction on its map is a junction, whatever its arms lead to. Hidden roads joined to each other
//! are one stretch, and a stretch with a place on it is how the place is reached: no sign on that.

use std::collections::{HashMap, HashSet};

use crate::geometry::{self, Point};
use crate::sector::ItemData;
use crate::surface::root;
use crate::world::{Definitions, Map, Prefabs, RoadClass};

/// How far along a hidden road its sign stands from its entrance, m: on the road, clear of the junction it joins.
const SIGN_IN: f32 = 12.0;

/// A road or a junction: its nodes, whether the game hides it, whether it's a place.
struct Piece {
    nodes: Vec<u64>,
    hidden: bool,
    place: bool,
}

/// The nodes where a hidden stretch with no place on it joins the game's map. `places`: the nodes the map's places are
/// reached at.
fn entrances(pieces: &[Piece], places: &HashSet<u64>) -> HashSet<u64> {
    let mut joined: HashMap<u64, u64> = HashMap::new();
    let mut on_map: HashSet<u64> = HashSet::new();
    let mut at_place: HashSet<u64> = places.clone();
    for piece in pieces {
        if piece.hidden {
            for pair in piece.nodes.windows(2) {
                let (a, b) = (root(&mut joined, pair[0]), root(&mut joined, pair[1]));
                joined.insert(a, b);
            }
        } else {
            on_map.extend(piece.nodes.iter().copied());
        }
        if piece.place {
            at_place.extend(piece.nodes.iter().copied());
        }
    }
    let hidden = || pieces.iter().filter(|piece| piece.hidden && !piece.nodes.is_empty());
    let mut with_place: HashSet<u64> = HashSet::new();
    for piece in hidden() {
        if piece.nodes.iter().any(|node| at_place.contains(node)) {
            with_place.insert(root(&mut joined, piece.nodes[0]));
        }
    }
    let mut found = HashSet::new();
    for piece in hidden() {
        if !with_place.contains(&root(&mut joined, piece.nodes[0])) {
            found.extend(piece.nodes.iter().filter(|node| on_map.contains(node)));
        }
    }
    found
}

/// Where `map`'s no-entry signs stand: on each hidden road at its entrance; where a hidden junction joins the game's map,
/// on the junction's end. In the map's order, so an export is reproducible.
pub fn signs(map: &Map, definitions: &Definitions, prefabs: &mut Prefabs, places: &HashSet<u64>) -> Vec<Point> {
    let track = |look: &u64| definitions.road_looks.get(look).is_some_and(|look| look.class == RoadClass::Rail);
    let mut pieces: Vec<Piece> = Vec::new();
    for item in &map.items {
        match &item.data {
            ItemData::Road { flags, look, start_node, end_node, .. } if !flags.secret && !track(look) => {
                pieces.push(Piece { nodes: vec![*start_node, *end_node], hidden: flags.hidden, place: false });
            }
            ItemData::Prefab { flags, model, nodes, .. } if !flags.secret => {
                let place = prefabs.get(*model).is_some_and(|prefab| !prefab.spawn_points.is_empty());
                pieces.push(Piece { nodes: nodes.clone(), hidden: flags.hidden, place });
            }
            _ => {}
        }
    }
    let entrances = entrances(&pieces, places);
    let mut signs = Vec::new();
    let mut signed: HashSet<u64> = HashSet::new();
    for item in &map.items {
        let ItemData::Road { flags, look, start_node, end_node, .. } = &item.data else { continue };
        if !flags.hidden || flags.secret || track(look) {
            continue;
        }
        let invisible = definitions.road_looks.get(look).is_some_and(|look| look.invisible);
        let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else { continue };
        for (node, backwards) in [(start_node, false), (end_node, true)] {
            if entrances.contains(node) {
                signed.insert(*node);
                if invisible {
                    continue;
                }
                let mut line = geometry::road_curve(start, end);
                if backwards {
                    line.reverse();
                }
                signs.push(along(&line, SIGN_IN));
            }
        }
    }
    for node in entrances.difference(&signed) {
        if let Some(node) = map.nodes.get(node) {
            signs.push([node.position[0], node.position[2]]);
        }
    }
    signs.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    signs
}

/// The point `distance` along `line`, or its end if it's shorter.
fn along(line: &[Point], distance: f32) -> Point {
    let mut left = distance;
    for pair in line.windows(2) {
        let long = (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
        if long >= left && long > 0.0 {
            let t = left / long;
            return [pair[0][0] + (pair[1][0] - pair[0][0]) * t, pair[0][1] + (pair[1][1] - pair[0][1]) * t];
        }
        left -= long;
    }
    line.last().copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn road(nodes: &[u64]) -> Piece {
        Piece { nodes: nodes.to_vec(), hidden: false, place: false }
    }

    fn hidden(nodes: &[u64]) -> Piece {
        Piece { nodes: nodes.to_vec(), hidden: true, place: false }
    }

    #[test]
    fn a_hidden_street_has_its_entrance_where_it_joins_the_map() {
        // 1 — 2 — 3 on the map; 2 — 4 — 5 hidden.
        let pieces = [road(&[1, 2]), road(&[2, 3]), hidden(&[2, 4]), hidden(&[4, 5])];
        assert_eq!(entrances(&pieces, &HashSet::new()), HashSet::from([2]));
    }

    #[test]
    fn a_junction_on_the_map_with_no_hidden_road_has_no_entrance() {
        let pieces = [road(&[1, 2]), road(&[2, 3, 4]), road(&[4, 5])];
        assert!(entrances(&pieces, &HashSet::new()).is_empty());
    }

    #[test]
    fn a_stretch_with_a_place_on_it_or_at_its_end_has_none() {
        // A place reached at node 5.
        let pieces = [road(&[1, 2]), hidden(&[2, 4]), hidden(&[4, 5])];
        assert!(entrances(&pieces, &HashSet::from([5])).is_empty());
        // A depot on the game's map, its back gate on the stretch.
        let depot = Piece { nodes: vec![1, 5], hidden: false, place: true };
        assert!(entrances(&[depot, hidden(&[5, 6])], &HashSet::new()).is_empty());
    }

    #[test]
    fn a_sign_stands_on_the_road_not_past_it() {
        assert_eq!(along(&[[0.0, 0.0], [5.0, 0.0], [5.0, 20.0]], 12.0), [5.0, 7.0]);
        assert_eq!(along(&[[0.0, 0.0], [5.0, 0.0]], 12.0), [5.0, 0.0]);
    }
}
