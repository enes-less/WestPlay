//! What the Maps app shows, from the game's map: roads (with junction interiors from their prefabs), map
//! areas, land (where the open sea isn't, [`crate::sea`]), painted water and place names, as vector tile
//! features ([`crate::tiles`]).
//!
//! The game's ground plane is metres (x east, z south). MapLibre works in degrees, so positions are
//! scaled as if the world lay on the equator, where a degree is the same distance both ways and Web
//! Mercator barely bends: [`to_lng_lat`]. The UI places the truck with the same scale.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::access::{Access, Through};
use crate::geometry::{self, Point, PrefabPlacement};
use crate::locale::Strings;
use crate::prefab::{Prefab, SpawnKind};
use crate::routing::Graph;
use crate::sea::{self, Cell, SeaGrid, SeaOptions};
use crate::sector::{ItemData, Node, Relief, TerrainSide};
use crate::signs;
use crate::surface::{self, Surfaces};
use crate::terrain::{self, QuadGrid};
use crate::tiles::{Feature, Geometry, Mercator, Value};
use crate::token::{self, token_to_string};
use crate::world::{City, Definitions, Map, Prefabs, RoadClass};

/// Metres in a degree at the equator (WGS 84). Must match the UI (ui/src/cabin/maps/projection.ts).
pub const METRES_PER_DEGREE: f64 = 111_319.490_793_273_57;

pub fn to_lng_lat(p: Point) -> [f64; 2] {
    [p[0] as f64 / METRES_PER_DEGREE, -p[1] as f64 / METRES_PER_DEGREE]
}

/// A ground-plane point in Web Mercator world units (0..1), where vector tiles are cut.
pub fn to_mercator(p: Point) -> Mercator {
    let [lng, lat] = to_lng_lat(p);
    let lat = lat.to_radians();
    [(lng + 180.0) / 360.0, 0.5 - (std::f64::consts::FRAC_PI_4 + lat / 2.0).tan().ln() / (2.0 * std::f64::consts::PI)]
}

/// A rectangle of the ground plane: min x, min z, max x, max z.
#[derive(Clone, Copy, Debug)]
pub struct Bounds(pub [f32; 4]);

impl Bounds {
    pub fn around(centre: Point, radius: f32) -> Self {
        Self([centre[0] - radius, centre[1] - radius, centre[0] + radius, centre[1] + radius])
    }

    pub(crate) fn overlaps(&self, item: &[f32; 4]) -> bool {
        let [x0, z0, x1, z1] = self.0;
        item[0] <= x1 && item[2] >= x0 && item[1] <= z1 && item[3] >= z0
    }
}

/// What an export holds, for a summary.
#[derive(Debug, Default)]
pub struct Counts {
    pub areas: usize,
    pub roads: usize,
    /// Places marked with an icon (fuel, services, garages).
    pub places: usize,
    /// Places the player can be routed to (Maps' search), and those left out for want of a road to them.
    pub destinations: usize,
    pub unrouted: usize,
    /// Road number signs.
    pub signs: usize,
    pub labels: usize,
    pub missing_prefabs: usize,
    pub water: usize,
    /// Painted water partly hidden under land the game draws higher over it (pieces of it: a terrain side,
    /// a bezier patch).
    pub water_cut: usize,
    /// Land came for a piece of water already drawn, so the map was drawn again holding every piece's land
    /// to the end (it never should: what can put land over a piece is found first).
    pub water_late: bool,
    pub land: usize,
}

/// A place name, drawn by the page.
#[derive(Clone, Debug)]
pub struct Label {
    /// The place's own name, in its own language and script (the game's `city_name`).
    pub name: String,
    /// Its name in the player's language, when that's different.
    pub localized: Option<String>,
    pub kind: &'static str,
    pub at: Point,
    /// How many people live there: smaller places' names give way when they'd overlap.
    pub population: u32,
}

/// A place the player can be routed to (Maps' search): what and where, in which city, and the map node the
/// game's GPS takes it as: one a road joins, as the game's map puts a waypoint on the road beside a building
/// (a junction's end with nothing joined, such as a service area's unused one, leaves the game's route
/// ending there).
#[derive(Clone, Debug)]
pub struct Destination {
    /// As the map's place layer names its kinds (`fuel`, `company`…), or `city`.
    pub kind: &'static str,
    /// Its own name (a brand's, a company's, a port's; a city's own), if it has one: a filling station with
    /// none is simply a filling station.
    pub name: Option<String>,
    /// A city's name in the player's language, when that's different.
    pub localized: Option<String>,
    /// The city it's in, or the nearest within reach (a token).
    pub city: Option<u64>,
    pub at: Point,
    pub node: u64,
    /// The other end of the lane past it, where which end a route comes to last depends on its way there (access.rs).
    pub also: Option<u64>,
}

pub struct Export {
    pub features: Vec<Feature>,
    pub labels: Vec<Label>,
    pub destinations: Vec<Destination>,
    pub counts: Counts,
    /// The painted water's area as the game paints it, and as it shows (where no land lies over it), m².
    pub water_area: [f32; 2],
    /// The grid the open sea was found on, for checking it.
    pub sea: SeaGrid,
}

/// How far the coast may stray when its points are thinned, metres.
const COAST_TOLERANCE: f32 = 3.0;

/// The zooms things appear from: the land and motorways from afar, side streets only up close.
const LAND_ZOOM: u8 = 6;
const PAINTED_WATER_ZOOM: u8 = 10;
const AREA_ZOOM: u8 = 12;
/// How far [`export`] is after each of its steps, of the whole: its lanes measured, the painted water read,
/// the roads' classes known, every item drawn, the water shown, the sea found, the land traced, the roads
/// joined (then the places routed). Measured on ProMods 2.84's whole map (2026-10-18: 53 s in all; the items
/// 24 s, the water 11 s, the sea 6 s, the land 3 s, the roads 5 s).
const DRAWING_STEPS: [f32; 8] = [0.01, 0.06, 0.07, 0.52, 0.73, 0.85, 0.91, 0.99];

/// The map inside `bounds`, as tile features in six layers:
///
/// - `sea`: the sea across the whole area, under everything (one rectangle: the page draws the sea
///   beyond it, as far as the map shows, as the game's world lies in the sea);
/// - `land`: everything the sea doesn't cover, over it;
/// - `water`: ground the game paints with water (sea along the coasts, rivers, lakes);
/// - `area`: polygons with `color` (0 paved, 1 light, 2 dark, 3 green) and `over` (drawn above roads);
/// - `road`: centre lines with `class` (`motorway`, `trunk`, `local`, `minor`, `rail`) and `width` in
///   metres;
/// - `place`: points with `kind` (`fuel`, `service`, `garage`, `dealer`, `company`, `parking`, `hotel`,
///   `recruitment`, `weigh`, `ferry`, `train`, `toll`, `border`) and `name` (its own: a dealer's brand, a
///   company, a port; or else what it is, in the player's language), where the page draws icons; garages
///   also with their `city` and `sale_name` (what one for sale is called);
/// - `sign`: points where the game shows a road's number, with `sign`, its image's name in the sprite
///   sheet ([`crate::signs`]); only the signs in `signs` (those on the sheet).
///
/// And place names, in their own language and in the player's (`strings`), for the page to draw. Hidden
/// items, and secret ones (roads the player discovers by driving them), are left out (but not as ground
/// for finding the sea: the land under a hidden road is still land).
///
/// `progress` is told how far it is, 0 to 1, after each step and through its pass over the map's items
/// (each step's share measured on a whole map, [`DRAWING_STEPS`]), so a long one shows it's going.
///
/// `stream_water`: each piece of painted water drawn, and its land let go, as soon as the last item that can
/// put land over it is drawn (found first: [`water_reach`]); otherwise all at the end. Land coming for a piece
/// already drawn (its reach misjudged) is told in [`Counts::water_late`]: the map should then be drawn again,
/// from the map read anew, without (`export::export` does).
///
/// Each item's terrain (and a road's) is let go once it's drawn (nothing after reads it): the map comes back
/// without it.
///
/// `stop`, once set, ends it after the batch of items or the step it's in: `None`.
#[allow(clippy::too_many_arguments)]
pub fn export(
    map: &mut Map,
    definitions: &Definitions,
    surfaces: &Surfaces,
    graph: &Graph,
    strings: &Strings,
    prefabs: &mut Prefabs,
    signs: &BTreeSet<String>,
    bounds: Bounds,
    options: SeaOptions,
    stream_water: bool,
    stop: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    progress: &mut dyn FnMut(f32),
) -> Option<Export> {
    let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
    let [lanes_done, painted_done, classes_done, items_done, water_done, sea_done, land_done, roads_done] =
        DRAWING_STEPS;
    let mut features = Vec::new();
    let mut counts = Counts::default();
    progress(lanes_done);
    // City token → its largest area and that area's centre.
    let mut cities: HashMap<u64, (f32, Point)> = HashMap::new();
    // Ferry port or train terminal token → whether it's a train terminal, and its docks' positions.
    let mut ferries: BTreeMap<u64, (bool, Vec<Point>)> = BTreeMap::new();
    // The places overlays mark, and those prefabs do (an overlay may mark one a prefab already has).
    let mut overlay_places: Vec<(Place, Point)> = Vec::new();
    let mut prefab_places_at: Vec<(Place, Point)> = Vec::new();
    // Rest areas made of roads and models, marked only by their rest triggers: each trigger's middle.
    let mut rest_triggers: Vec<Point> = Vec::new();
    // Where the player can be routed: each place as found (its kind, name, where, and the junction it's
    // part of), routed once every junction's joined ends and every road are known.
    let mut to_route: Vec<(Place, Option<String>, Point, Option<u64>)> = Vec::new();
    let mut joined_ends: HashMap<u64, Vec<(u64, Point)>> = HashMap::new();
    // The ways through the prefabs of what a service area offers.
    let mut ways_through: HashMap<u64, Vec<Through>> = HashMap::new();
    let mut road_ends = NodeGrid::default();
    // Land goes straight onto the sea's grid (a whole map's terrain is millions of quads). Painted water is
    // read first, and the land over it kept as the land is read: the water is painted on the grid and drawn
    // once all land is there, where no land hides it.
    let mut grid = SeaGrid::new(bounds.0, options.cell);
    let mut water = Water::stopping(painted_water(map, definitions, surfaces, &bounds), stop.clone());
    progress(painted_done);

    let fuel_brands = fuel_brands(map, definitions, strings, prefabs);
    // The cities' areas, for the city each garage belongs to.
    let city_areas: Vec<(u64, [f32; 4])> = map
        .items
        .iter()
        .filter_map(|item| match &item.data {
            ItemData::City { city, width, height, .. } => {
                let [x, z] = [item.bounds[0], item.bounds[1]];
                Some((*city, [x, z, x + width, z + height]))
            }
            _ => None,
        })
        .collect();

    progress(classes_done);
    // Told every hundredth of the items.
    let every = (map.items.len() / 100).max(1);
    let all_items = map.items.len().max(1);
    let share = |index: usize| index as f32 / all_items as f32;
    // A batch of items' shapes worked out on the build's threads at once (terrain, a road's line and outline: what
    // takes the time), then each item drawn with them in the map's order, as one at a time would: the same
    // export, byte for byte.
    let drawn: Vec<usize> = (0..map.items.len()).filter(|&index| bounds.overlaps(&map.items[index].bounds)).collect();
    // Roads by uid, to let a road's terrain go once the last road with its uid is drawn (two may share one).
    let mut roads_left: HashMap<u64, usize> = HashMap::new();
    for &index in &drawn {
        if let ItemData::Road { .. } = map.items[index].data {
            *roads_left.entry(map.items[index].uid).or_default() += 1;
        }
    }
    // Each prefab's ground: land on the grid, over the water under it.
    for hull in &surfaces.footprints {
        let ring: Vec<Point> = hull.iter().map(|v| [v[0], v[2]]).collect();
        if ring.len() >= 3 {
            grid.paint(&ring, Cell::Land);
        }
        water.ground(hull);
    }
    features.extend(surfaces.road_lines.iter().cloned());
    let workers = crate::threads::for_a_build();
    // Each piece of painted water, by the last item that can put land over it: drawn once that's drawn.
    let mut finishing: Vec<(usize, usize)> = if stream_water {
        let items: Vec<(usize, &crate::sector::Item)> = drawn.iter().map(|&index| (index, &map.items[index])).collect();
        let last = water_reach(&items, map, definitions, surfaces, &water, prefabs, workers);
        let mut order: Vec<(usize, usize)> =
            last.iter().enumerate().filter_map(|(piece, last)| Some(((*last)?, piece))).collect();
        order.sort_unstable();
        order
    } else {
        Vec::new()
    };
    finishing.reverse();
    for indices in drawn.chunks(WORK_BATCH) {
        // The batch drawn (the map only read), then its terrain let go.
        {
            let map = &*map;
            let batch: Vec<(usize, &crate::sector::Item)> =
                indices.iter().map(|&index| (index, &map.items[index])).collect();
            let batch = batch.as_slice();
            let drawing = Drawing { map, definitions, surfaces, grid: &grid, water: &water };
            let mut worked = work_out(batch, workers, |item| item_work(item, &drawing)).into_iter();
            for &(index, item) in batch {
                let work = worked.next().unwrap_or(Work::None);
                if index % every == 0 {
                    progress(classes_done + (items_done - classes_done) * share(index));
                }
                match &item.data {
                    ItemData::Terrain { .. } => {
                        let Work::Terrain(sides) = work else { continue };
                        for side in sides {
                            side.put(&mut grid, &mut water);
                        }
                    }
                    ItemData::BezierPatch { .. } => {
                        let Work::Patch(patch) = work else { continue };
                        patch.put(&mut grid, &mut water);
                    }
                    ItemData::Road { flags, look, start_node, end_node, .. } => {
                        let (Work::Road(road), Some(start), Some(end)) =
                            (work, map.nodes.get(start_node), map.nodes.get(end_node))
                        else {
                            continue;
                        };
                        // The ends of roads a truck drives (not paths, dirt tracks or rails), for places with no
                        // junction of their own to be routed to.
                        let driven = definitions.road_looks.get(look).is_some_and(|look| {
                            matches!(look.class, RoadClass::Local | RoadClass::Trunk | RoadClass::Motorway)
                        });
                        if driven && !flags.hidden && !flags.secret {
                            road_ends.add(*start_node, [start.position[0], start.position[2]]);
                            road_ends.add(*end_node, [end.position[0], end.position[2]]);
                        }
                        for runs in &road.outline {
                            grid.fill(runs, Cell::Land);
                        }
                        water.cover(&road.surface);
                        for side in road.grounds.iter().flatten() {
                            side.put(&mut grid, &mut water);
                        }
                        counts.roads += 1;
                    }
                    ItemData::Prefab { flags, model, nodes, origin, .. } => {
                        let Some(prefab) = prefabs.get(*model) else {
                            counts.missing_prefabs += 1;
                            continue;
                        };
                        let start = nodes.first().and_then(|uid| map.nodes.get(uid));
                        let Some(own) = prefab.nodes.get(*origin as usize) else { continue };
                        let Some(placement) = start.map(|at| PrefabPlacement::new(own, at)) else { continue };
                        let shown = !flags.hidden && !flags.secret;
                        // Its ends something joins (a road, another junction): where its places are routed to.
                        let ends: Vec<(u64, Point)> = nodes
                            .iter()
                            .filter_map(|uid| {
                                let node = map.nodes.get(uid)?;
                                let joined = node.forward_item != 0 && node.backward_item != 0;
                                joined.then_some((*uid, [node.position[0], node.position[2]]))
                            })
                            .collect();
                        if shown {
                            let path = definitions.prefab_paths.get(model).map_or("", String::as_str);
                            let places = prefab_places(prefab, is_hotel(path));
                            if ends.len() > 1 && places.iter().any(|(place, _)| place.in_service_area()) {
                                ways_through.insert(item.uid, ways_through_prefab(prefab, nodes, *origin as usize, &placement, &ends));
                            }
                            for (place, at) in places {
                                // A dealer is named after its brand, which its prefab's folder says; a filling station
                                // a fuel company owns, after the company; any other place (and a dealer of no brand)
                                // by what it is.
                                let brand = match place {
                                    Place::Dealer => {
                                        definitions.prefab_paths.get(model).and_then(|path| dealer_brand(path, strings))
                                    }
                                    Place::Fuel => fuel_brands.stations.get(&item.uid).cloned(),
                                    _ => None,
                                };
                                let at = placement.place(at);
                                // Routed to under its own name only (a brand's); the page says what it is otherwise.
                                to_route.push((place, brand.clone(), at, Some(item.uid)));
                                let name = brand.or_else(|| place.generic_name(strings));
                                prefab_places_at.push((place, at));
                                let mut feature = place_feature(at, place, name);
                                // A garage: its city (the save game says which cities' garages the player owns, so
                                // the page draws theirs as theirs), and its name for when it's for sale.
                                if place == Place::Garage {
                                    let city = city_of(at, &city_areas).and_then(token_to_string);
                                    feature.props.extend(city.map(|city| ("city", Value::Str(city))));
                                    let for_sale = strings.resolve("@@mapl_garage@@").map(str::to_owned);
                                    feature.props.extend(for_sale.map(|name| ("sale_name", Value::Str(name))));
                                }
                                features.push(feature);
                                counts.places += 1;
                            }
                        }
                        if !ends.is_empty() {
                            joined_ends.insert(item.uid, ends);
                        }
                    }
                    ItemData::MapArea { flags, draw_over, nodes, color } => {
                        let ring: Vec<Point> =
                            nodes.iter().filter_map(|uid| map.nodes.get(uid)).map(geometry::node_point).collect();
                        if ring.len() < 3 {
                            continue;
                        }
                        grid.paint(&ring, Cell::Land);
                        if !flags.hidden && !flags.secret {
                            features.push(area_feature(&ring, *color as u8, *draw_over));
                            counts.areas += 1;
                        }
                    }
                    // A company's depot, named; dealers' own depots (DAF's, Scania's, Volvo's: `*_dlr`) are shown as
                    // their dealers already, and a fuel company's filling stations as filling stations (named after it).
                    ItemData::Company { company, prefab, node, .. } => {
                        let is_dealer = token_to_string(*company).is_some_and(|token| token.ends_with("_dlr"));
                        let (Some(name), Some(node)) = (definitions.companies.get(company), map.nodes.get(node)) else {
                            continue;
                        };
                        if is_dealer || fuel_brands.companies.contains(company) {
                            continue;
                        }
                        let at = [node.position[0], node.position[2]];
                        let name = strings.resolve(name).unwrap_or(name).to_owned();
                        to_route.push((Place::Company, Some(name.clone()), at, Some(*prefab)));
                        features.push(place_feature(at, Place::Company, Some(name)));
                        counts.places += 1;
                    }
                    ItemData::MapOverlay { node, .. } => {
                        let Some(node) = map.nodes.get(node) else { continue };
                        let at = [node.position[0], node.position[2]];
                        if let Some(place) = Place::of_overlay(&item.data) {
                            overlay_places.push((place, at));
                        } else if let Some(sign) = signs::road_sign(&item.data).filter(|sign| signs.contains(sign)) {
                            features.push(sign_feature(at, sign));
                            counts.signs += 1;
                        }
                    }
                    ItemData::RestTrigger { nodes } => {
                        let corners: Vec<Point> = nodes
                            .iter()
                            .filter_map(|node| map.nodes.get(node).map(|node| [node.position[0], node.position[2]]))
                            .collect();
                        if !corners.is_empty() {
                            let count = corners.len() as f32;
                            rest_triggers.push(
                                corners
                                    .iter()
                                    .fold([0.0, 0.0], |sum, at| [sum[0] + at[0] / count, sum[1] + at[1] / count]),
                            );
                        }
                    }
                    ItemData::Ferry { train, port, node } => {
                        if let Some(node) = map.nodes.get(node) {
                            let entry = ferries.entry(*port).or_insert((*train, Vec::new()));
                            entry.1.push([node.position[0], node.position[2]]);
                        }
                    }
                    ItemData::City { hidden: false, city, width, height, .. } => {
                        // A city is often several areas: its name goes on the largest.
                        let area = width * height;
                        // The city item's box starts at its position.
                        let centre = [item.bounds[0] + width / 2.0, item.bounds[1] + height / 2.0];
                        let best = cities.entry(*city).or_insert((0.0, centre));
                        if area > best.0 {
                            *best = (area, centre);
                        }
                    }
                    _ => {}
                }
            }
            let through = batch.last().map_or(0, |&(index, _)| index);
            let mut ready = Vec::new();
            while finishing.last().is_some_and(|&(last, _)| last <= through) {
                ready.extend(finishing.pop().map(|(_, piece)| piece));
            }
            water.finish(&ready);
        }
        if stopped() {
            return None;
        }
        for &index in indices {
            let item = &mut map.items[index];
            match item.data {
                ItemData::Terrain { .. } | ItemData::BezierPatch { .. } => item.data = ItemData::Other,
                ItemData::Road { .. } => {
                    let uid = item.uid;
                    if let Some(left) = roads_left.get_mut(&uid) {
                        *left -= 1;
                        if *left == 0 {
                            map.road_terrain.remove(&uid);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // The sea under the whole area, and the land over it; then the painted water. (The sea comes with each
    // tile rather than as the page's background, so a tile still loading shows as plain ground, not as
    // open sea.)
    progress(items_done);
    let shown = water.shown(&mut |done| progress(items_done + (water_done - items_done) * done));
    for shape in &shown {
        grid.paint_rings(shape, Cell::Water);
    }
    let water_area = [water.area, shown.iter().map(|shape| shape_area(shape)).sum()];
    counts.water_cut = water.cut();
    counts.water_late = water.late;
    // Its pieces and the land kept over them (on a whole map, gigabytes) aren't needed any more: not held
    // through the sea's grid, the build's biggest step for memory.
    drop(water);
    progress(water_done);
    if stopped() {
        return None;
    }
    // The world's land drawn beyond the area (the game's picture of it): the area's own land carried on to meet it.
    let carried_on = options.world.is_some();
    grid.find_sea_telling(options, &mut |done| progress(water_done + (sea_done - water_done) * done));
    if stopped() {
        return None;
    }
    let [x0, z0, x1, z1] = grid.traced_bounds();
    let whole_area = vec![vec![[x0, z0], [x1, z0], [x1, z1], [x0, z1]]];
    features.push(polygon_feature("sea", LAND_ZOOM, &whole_area, Vec::new()));
    progress(sea_done);
    let traced = if carried_on { sea::land_shapes_carried_on } else { sea::land_shapes_telling };
    let land = traced(&grid, COAST_TOLERANCE, &mut |done| {
        progress(sea_done + (land_done - sea_done) * done);
    });
    for shape in land {
        let mut rings = vec![shape.outline];
        rings.extend(shape.holes);
        features.push(polygon_feature("land", LAND_ZOOM, &rings, Vec::new()));
        counts.land += 1;
    }
    counts.water = shown.len();
    features.extend(shown.iter().map(|shape| polygon_feature("water", PAINTED_WATER_ZOOM, shape, Vec::new())));

    // The overlays' places, but for one a prefab marks already (a weigh station both mark).
    for (place, at) in overlay_places {
        let known = prefab_places_at.iter().any(|&(other, there)| other == place && distance(at, there) < SAME_PLACE);
        if !known {
            features.push(place_feature(at, place, place.generic_name(strings)));
            counts.places += 1;
        }
    }

    for at in lone_rests(&rest_triggers, &prefab_places_at) {
        to_route.push((Place::Parking, None, at, None));
        features.push(place_feature(at, Place::Parking, Place::Parking.generic_name(strings)));
        counts.places += 1;
    }

    // One icon a port or terminal, at the middle of its docks, named as the game names it.
    for (port, (train, docks)) in ferries {
        let count = docks.len() as f32;
        let at = docks.iter().fold([0.0, 0.0], |sum, dock| [sum[0] + dock[0] / count, sum[1] + dock[1] / count]);
        let place = if train { Place::Train } else { Place::Ferry };
        let name = definitions.ferries.get(&port).map(|name| strings.resolve(name).unwrap_or(name).to_owned());
        to_route.push((place, name.clone(), at, None));
        features.push(place_feature(at, place, name.or_else(|| place.generic_name(strings))));
        counts.places += 1;
    }

    let mut cities: Vec<_> = cities.into_iter().collect();
    cities.sort_by_key(|(city, _)| *city);
    // Each city's label, by the city (routed to as a place too, below).
    let routed_cities: Vec<(u64, Label)> = cities
        .into_iter()
        .filter_map(|(city, (_, at))| {
            let City { name, population, localized } = definitions.cities.get(&city)?;
            let localized = localized.as_deref().and_then(|text| strings.resolve(text)).filter(|text| text != name);
            let label = Label {
                name: name.clone(),
                localized: localized.map(str::to_owned),
                kind: "city",
                at,
                population: *population,
            };
            Some((city, label))
        })
        .collect();
    let labels: Vec<Label> = routed_cities.iter().map(|(_, label)| label.clone()).collect();
    counts.labels = labels.len();
    progress(land_done);
    if stopped() {
        return None;
    }
    // What a service area offers routed through it (access.rs). Any other place to a joined end of its junction a route gets
    // to, the nearest; else (a port, a junction with none) to the nearest end of a road nearby. A city to the road end nearest
    // its centre. Where a route gets to none of those (a bit of road cut off from the rest), what there is.
    progress(roads_done);
    let access = Access::new(map, definitions, graph, &surfaces.service);
    let mut destinations = Vec::new();
    let reached = |node: u64| access.reaches(node);
    for (place, name, at, junction) in to_route {
        let ends = junction.and_then(|uid| joined_ends.get(&uid)).map_or(&[][..], Vec::as_slice);
        let reached_ends: Vec<(u64, Point)> = ends.iter().copied().filter(|&(node, _)| reached(node)).collect();
        let through = junction.and_then(|uid| ways_through.get(&uid)).map_or(&[][..], Vec::as_slice);
        let way_in = if place.in_service_area() { access.way_in(at, &reached_ends, through) } else { None };
        let also = way_in.and_then(|way| way.also);
        let node = way_in
            .map(|way| way.node)
            .or_else(|| nearest_end(&reached_ends, at))
            .or_else(|| road_ends.nearest_such(at, PLACE_ROAD_REACH, &reached))
            .or_else(|| nearest_end(ends, at))
            .or_else(|| road_ends.nearest_such(at, PLACE_ROAD_REACH, &|_| true));
        match node {
            Some(node) => destinations.push(Destination {
                kind: place.name(),
                name,
                localized: None,
                city: city_of(at, &city_areas),
                at,
                node,
                also,
            }),
            None => counts.unrouted += 1,
        }
    }
    for (city, label) in &routed_cities {
        let node = road_ends
            .nearest_such(label.at, CITY_ROAD_REACH, &reached)
            .or_else(|| road_ends.nearest_such(label.at, CITY_ROAD_REACH, &|_| true));
        match node {
            Some(node) => destinations.push(Destination {
                kind: "city",
                name: Some(label.name.clone()),
                localized: label.localized.clone(),
                city: Some(*city),
                at: label.at,
                node,
                also: None,
            }),
            None => counts.unrouted += 1,
        }
    }
    let destinations = one_each(destinations);
    progress(1.0);
    counts.destinations = destinations.len();
    Some(Export { features, labels, destinations, counts, water_area, sea: grid })
}

/// A point with its height: x, height (the game's y), z.
type Vertex = [f32; 3];

/// One piece of painted water (a terrain side's, a bezier patch's), as the game paints it: its quads with
/// their heights, and its shapes, each an outline and its holes (a river bank down a whole strip is one
/// shape, not one per column of quads).
struct WaterPiece {
    quads: Vec<[Vertex; 4]>,
    shapes: Vec<Vec<Vec<Point>>>,
    /// Shown as it is, whatever land lies round it: a river model's strip is the water that shows (its banks are laid to
    /// it), where painted water runs on under the land.
    open: bool,
}

/// The painted water, and the land read over it.
///
/// The game lays water flat at the water's height and lets it run on under the land around (the sea along
/// a coast in a strip a kilometre wide, under the hills behind it; a river under its banks), and draws the
/// land, higher, over it: the water shows only where nothing higher lies over it. A fifth of the painted
/// water lies under land (a town by Vlorë, a junction by Taranto, on the sea). So each piece shows only
/// where no ground (terrain strips, roads' terrain, bezier patches, roads, junctions round their roads) lies
/// more than [`LAND_OVER_WATER`] above it.
struct Water {
    pieces: Vec<WaterPiece>,
    /// Each piece's lowest water.
    lowest: Vec<f32>,
    /// Which pieces no land hides ([`WaterPiece::open`]).
    open: Vec<bool>,
    /// The pieces by the cells ([`WATER_CELL`]) their quads reach.
    cells: HashMap<(i32, i32), Vec<u32>>,
    /// The land quads over each piece, rising above its lowest water somewhere (handed over with it to be
    /// drawn).
    covers: Vec<Vec<[Vertex; 4]>>,
    /// Pieces handed over to be drawn ([`Self::finish`]).
    sent: Vec<bool>,
    /// What draws the pieces handed over, beside the items' drawing (`None` once it's ended).
    painter: Option<Painter>,
    /// Whether any land was kept over each piece.
    covered: Vec<bool>,
    /// Land came for a piece already drawn.
    late: bool,
    /// The painted water's whole area, m².
    area: f32,
}

/// The cells painted water is looked up in, metres.
const WATER_CELL: f32 = 64.0;

/// A piece of painted water to draw (its index, the piece, the land over it), and one drawn (its index and its
/// shapes as they show).
type PaintJob = (usize, WaterPiece, Vec<[Vertex; 4]>);
type Painted = (usize, Vec<Vec<Vec<Point>>>);

/// Draws pieces of painted water on threads of its own ([`shown_water`]), each piece handed over whole, while
/// the map's items are drawn; their shapes collected at the end, by piece.
struct Painter {
    jobs: Option<std::sync::mpsc::Sender<PaintJob>>,
    /// (In a mutex: the water is read from many threads while the items are worked out.)
    painted: std::sync::Mutex<std::sync::mpsc::Receiver<Painted>>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl Painter {
    /// A painter whose pieces give up drawing once `stop` is set (their shapes then empty: the map is given
    /// up too).
    fn new(stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        let (jobs, waiting) = std::sync::mpsc::channel::<PaintJob>();
        let (done, painted) = std::sync::mpsc::channel::<Painted>();
        let waiting = std::sync::Arc::new(std::sync::Mutex::new(waiting));
        let workers = (0..crate::threads::for_a_build())
            .map(|_| {
                let (waiting, done, stop) = (waiting.clone(), done.clone(), stop.clone());
                std::thread::spawn(move || {
                    loop {
                        let job = waiting.lock().unwrap_or_else(|e| e.into_inner()).recv();
                        let Ok((index, piece, covers)) = job else { break };
                        if done.send((index, shown_water(&piece, &covers, &stop))).is_err() {
                            break;
                        }
                    }
                })
            })
            .collect();
        Self { jobs: Some(jobs), painted: std::sync::Mutex::new(painted), workers }
    }

    fn paint(&self, index: usize, piece: WaterPiece, covers: Vec<[Vertex; 4]>) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send((index, piece, covers));
        }
    }

    /// Every piece's shapes (of `pieces`, by index), once all handed over are drawn; `progress` told the share
    /// drawn as they come.
    fn end(mut self, pieces: usize, progress: &mut dyn FnMut(f32)) -> Vec<Vec<Vec<Vec<Point>>>> {
        drop(self.jobs.take());
        let mut done = vec![Vec::new(); pieces];
        let mut count = 0;
        let painted = self.painted.into_inner().unwrap_or_else(|e| e.into_inner());
        for (index, shapes) in painted.iter() {
            done[index] = shapes;
            count += 1;
            if count % 64 == 0 {
                progress(count as f32 / pieces.max(1) as f32);
            }
            if count == pieces {
                break;
            }
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
        done
    }
}

impl Water {
    fn new(pieces: Vec<WaterPiece>) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (index, piece) in pieces.iter().enumerate() {
            let mut reached: Vec<(i32, i32)> = piece.quads.iter().flat_map(|quad| cells_of(quad, WATER_CELL)).collect();
            reached.sort_unstable();
            reached.dedup();
            for cell in reached {
                cells.entry(cell).or_default().push(index as u32);
            }
        }
        let lowest =
            pieces.iter().map(|piece| piece.quads.iter().flatten().map(|v| v[1]).fold(f32::MAX, f32::min)).collect();
        let covers = vec![Vec::new(); pieces.len()];
        let area = pieces.iter().flat_map(|piece| &piece.shapes).map(|shape| shape_area(shape)).sum();
        let (sent, covered) = (vec![false; pieces.len()], vec![false; pieces.len()]);
        let open = pieces.iter().map(|piece| piece.open).collect();
        let painter = Some(Painter::new(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))));
        Self { pieces, lowest, cells, covers, sent, painter, covered, late: false, area, open }
    }

    /// [`Self::new`], its pieces given up once `stop` is set.
    fn stopping(pieces: Vec<WaterPiece>, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        let mut water = Self::new(pieces);
        water.painter = Some(Painter::new(stop));
        water
    }

    /// A land quad read: kept for each piece of water it may lie over, higher than its water.
    fn land(&mut self, quad: &[Vertex; 4]) {
        for piece in self.under(quad) {
            self.keep(piece, quad);
        }
    }

    /// A land quad kept over a piece; one coming for a piece already drawn noticed.
    fn keep(&mut self, piece: u32, quad: &[Vertex; 4]) {
        let piece = piece as usize;
        if self.sent[piece] {
            self.late = true;
            return;
        }
        self.covers[piece].push(*quad);
        self.covered[piece] = true;
    }

    /// The pieces of water a land quad may lie over, higher than their water ([`Self::land`] without keeping
    /// it: found without changing anything, so many at once, and kept after with [`Self::cover`]).
    fn under(&self, quad: &[Vertex; 4]) -> Vec<u32> {
        let top = quad.iter().map(|v| v[1]).fold(f32::MIN, f32::max);
        let mut under: Vec<u32> = cells_of(quad, WATER_CELL)
            .filter_map(|cell| self.cells.get(&cell))
            .flatten()
            .copied()
            .filter(|&piece| !self.open[piece as usize] && top > self.lowest[piece as usize] + LAND_OVER_WATER)
            .collect();
        under.sort_unstable();
        under.dedup();
        under
    }

    /// Every piece of water within a quad's cells, however high: what land there could lie over.
    fn near(&self, quad: &[Vertex; 4]) -> Vec<u32> {
        let mut near: Vec<u32> =
            cells_of(quad, WATER_CELL).filter_map(|cell| self.cells.get(&cell)).flatten().copied().collect();
        near.sort_unstable();
        near.dedup();
        near
    }

    /// Land quads kept over the pieces of water found under them ([`Self::under`]), in their order.
    fn cover(&mut self, covered: &[(u32, [Vertex; 4])]) {
        for (piece, quad) in covered {
            self.keep(*piece, quad);
        }
    }

    /// Land quads ([`Self::under`] each), as [`Self::cover`] keeps them.
    fn covering(&self, quads: &[[Vertex; 4]]) -> Vec<(u32, [Vertex; 4])> {
        quads.iter().flat_map(|quad| self.under(quad).into_iter().map(move |piece| (piece, *quad))).collect()
    }

    /// A junction's ground read, as the points round its roads (`points`, with their heights): the ground
    /// within them (their convex hull) over the water as land.
    fn ground(&mut self, points: &[Vertex]) {
        let hull = convex_hull(points);
        for i in 1..hull.len().saturating_sub(1) {
            self.land(&[hull[0], hull[i], hull[i + 1], hull[i + 1]]);
        }
    }

    /// Every piece's shapes as they show (each an outline and its holes), in the pieces' order (so the
    /// export is reproducible): those not handed over yet handed over now, `progress` told the share of all
    /// the pieces drawn, 0 to 1, as they come.
    fn shown(&mut self, progress: &mut dyn FnMut(f32)) -> Vec<Vec<Vec<Point>>> {
        let left: Vec<usize> = (0..self.pieces.len()).filter(|&piece| !self.sent[piece]).collect();
        self.finish(&left);
        let mut done = self.painter.take().map(|painter| painter.end(self.pieces.len(), progress)).unwrap_or_default();
        done.iter_mut().flat_map(std::mem::take).collect()
    }

    /// Hands these pieces over to be drawn (all their land read), their water and land with them.
    fn finish(&mut self, pieces: &[usize]) {
        for &index in pieces {
            self.sent[index] = true;
            let empty = WaterPiece { quads: Vec::new(), shapes: Vec::new(), open: false };
            let piece = std::mem::replace(&mut self.pieces[index], empty);
            if let Some(painter) = &self.painter {
                painter.paint(index, piece, std::mem::take(&mut self.covers[index]));
            }
        }
    }

    /// How many pieces land lies over.
    fn cut(&self) -> usize {
        self.covered.iter().filter(|&&covered| covered).count()
    }
}

/// Whether a terrain side has any land (quads not painted water).
fn has_land(definitions: &Definitions, side: &TerrainSide) -> bool {
    let grid = &side.grid;
    (0..grid.cols).any(|col| {
        (0..grid.rows).any(|row| !grid.material_at(row, col).is_some_and(|m| definitions.water_materials.contains(&m)))
    })
}

/// The convex hull of `points` on the ground (each keeping its height), in order round it (Andrew's
/// monotone chain).
fn convex_hull(points: &[Vertex]) -> Vec<Vertex> {
    let mut points = points.to_vec();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[2].total_cmp(&b[2])));
    points.dedup_by(|a, b| a[0] == b[0] && a[2] == b[2]);
    if points.len() < 3 {
        return points;
    }
    let turn = |o: &Vertex, a: &Vertex, b: &Vertex| (a[0] - o[0]) * (b[2] - o[2]) - (a[2] - o[2]) * (b[0] - o[0]);
    let mut hull: Vec<Vertex> = Vec::with_capacity(points.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let ordered: Box<dyn Iterator<Item = &Vertex>> =
            if pass == 0 { Box::new(points.iter()) } else { Box::new(points.iter().rev()) };
        for p in ordered {
            while hull.len() >= start + 2 && turn(&hull[hull.len() - 2], &hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(*p);
        }
        hull.pop();
    }
    hull
}

/// The cells of `size` metres a quad's box reaches.
fn cells_of(quad: &[Vertex; 4], size: f32) -> impl Iterator<Item = (i32, i32)> {
    let (x0, x1) = quad.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v[0]), hi.max(v[0])));
    let (z0, z1) = quad.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v[2]), hi.max(v[2])));
    let cell = move |v: f32| (v / size).floor() as i32;
    (cell(x0)..=cell(x1)).flat_map(move |x| (cell(z0)..=cell(z1)).map(move |z| (x, z)))
}

/// A shape's area (its outline's, less its holes'), m².
fn shape_area(shape: &[Vec<Point>]) -> f32 {
    let mut rings = shape.iter().map(|ring| sea::signed_area(ring).abs() / 2.0);
    let outline = rings.next().unwrap_or(0.0);
    outline - rings.sum::<f32>()
}

/// How much higher than painted water land must lie to hide it, metres: a beach or a river bank meeting the
/// water at its height leaves it showing up to where it rises out of it.
const LAND_OVER_WATER: f32 = 0.25;
/// How finely painted water is looked at where land lies over it, metres. (Its own edges stay where they
/// are: see [`shown_water`].)
const WATER_SAMPLE: f32 = 2.0;
/// The cells a piece's triangles and edges are looked up in, metres.
const WATER_LOOKUP: f32 = 16.0;
/// The share of a piece of painted water below which a part of it the land closes in all round is a crumb
/// of hidden water showing through, not water ([`shown_water`]).
const WATER_CRUMB: f32 = 0.05;
/// Points dropped from the traced shapes where they add less than this, metres.
const WATER_TOLERANCE: f32 = 0.2;

/// A piece of painted water's shapes where it shows, with `covers` (the land quads over it) lying over it.
/// None: its shapes as they are. Otherwise traced over a lattice of [`WATER_SAMPLE`]: at each point, how far
/// inside its shapes it lies (so its own edges come out where they are, straight on, meeting the next
/// piece's), and how far its water lies above the land there (so it ends where the land rises out of it):
/// the less of the two, traced where it's 0.
///
/// Given up (nothing) once `stop` is set: a piece's lattice and lookups grow with it, and a build stopped for
/// its memory mustn't go on growing here.
fn shown_water(
    piece: &WaterPiece,
    covers: &[[Vertex; 4]],
    stop: &std::sync::atomic::AtomicBool,
) -> Vec<Vec<Vec<Point>>> {
    let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
    if covers.is_empty() {
        return piece.shapes.clone();
    }
    let reach = 2.0 * WATER_SAMPLE;
    let cell = |x: f32, z: f32| ((x / WATER_LOOKUP).floor() as i32, (z / WATER_LOOKUP).floor() as i32);
    // Quads as triangles, by the cells their boxes reach.
    let triangles = |quads: &[[Vertex; 4]]| {
        let triangles: Vec<[Vertex; 3]> = quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (index, triangle) in triangles.iter().enumerate() {
            if index % 1024 == 0 && stopped() {
                break;
            }
            let quad = [triangle[0], triangle[1], triangle[2], triangle[2]];
            for key in cells_of(&quad, WATER_LOOKUP) {
                cells.entry(key).or_default().push(index as u32);
            }
        }
        (triangles, cells)
    };
    let (water_triangles, water_cells) = triangles(&piece.quads);
    let (land, land_cells) = triangles(covers);
    // The shapes' edges, by the cells within `reach` of them.
    let edges: Vec<(Point, Point)> = piece
        .shapes
        .iter()
        .flatten()
        .flat_map(|ring| (0..ring.len()).map(|i| (ring[i], ring[(i + 1) % ring.len()])))
        .collect();
    let mut edge_cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (index, &(a, b)) in edges.iter().enumerate() {
        let (x0, x1) = (a[0].min(b[0]) - reach, a[0].max(b[0]) + reach);
        let (z0, z1) = (a[1].min(b[1]) - reach, a[1].max(b[1]) + reach);
        let ((cx0, cz0), (cx1, cz1)) = (cell(x0, z0), cell(x1, z1));
        for x in cx0..=cx1 {
            for z in cz0..=cz1 {
                edge_cells.entry((x, z)).or_default().push(index as u32);
            }
        }
    }
    let heights_at = |triangles: &[[Vertex; 3]], cells: &HashMap<(i32, i32), Vec<u32>>, p: Point| {
        cells
            .get(&cell(p[0], p[1]))
            .into_iter()
            .flatten()
            .filter_map(|&index| height_in(&triangles[index as usize], p))
            .collect::<Vec<f32>>()
    };
    // The lattice over the piece and `reach` round it, with a ring of outside round that, so every outline
    // closes.
    let (x0, x1) = piece.quads.iter().flatten().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v[0]), hi.max(v[0])));
    let (z0, z1) = piece.quads.iter().flatten().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v[2]), hi.max(v[2])));
    let origin = [x0 - reach, z0 - reach];
    let (nx, nz) = (
        ((x1 - x0 + 2.0 * reach) / WATER_SAMPLE).ceil() as usize + 1,
        ((z1 - z0 + 2.0 * reach) / WATER_SAMPLE).ceil() as usize + 1,
    );
    let (w, h) = (nx + 2, nz + 2);
    let mut field = vec![-reach; w * h];
    // Where it shows next to its own edge (where it goes on into the water beyond it).
    let mut at_edge = vec![false; w * h];
    for j in 0..nz {
        if stopped() {
            return Vec::new();
        }
        for i in 0..nx {
            let p = [origin[0] + i as f32 * WATER_SAMPLE, origin[1] + j as f32 * WATER_SAMPLE];
            let water_height = heights_at(&water_triangles, &water_cells, p).into_iter().reduce(f32::max);
            let (near, edge) = edge_cells
                .get(&cell(p[0], p[1]))
                .into_iter()
                .flatten()
                .map(|&index| (distance_to_segment(p, edges[index as usize].0, edges[index as usize].1), index))
                .fold((reach, None), |best, (gap, index)| if gap < best.0 { (gap, Some(index)) } else { best });
            let inside = if water_height.is_some() { near } else { -near };
            let above_land = match water_height {
                None => reach,
                Some(water) => heights_at(&land, &land_cells, p)
                    .into_iter()
                    .reduce(f32::max)
                    .map_or(reach, |land| (water + LAND_OVER_WATER - land).clamp(-reach, reach)),
            };
            field[(j + 1) * w + i + 1] = inside.min(above_land);
            // Next to its edge, and the ground just past it open too (no land over the water there): its
            // edge isn't under the land itself.
            let open_past = |edge: u32, water: f32| {
                let (a, b) = edges[edge as usize];
                let Some(way) = direction(a, b) else { return false };
                let t = (((p[0] - a[0]) * (b[0] - a[0]) + (p[1] - a[1]) * (b[1] - a[1]))
                    / ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)))
                .clamp(0.0, 1.0);
                let on = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                // Past it: whichever side its own water isn't.
                [1.0, -1.0].into_iter().any(|side: f32| {
                    let past = [on[0] - way[1] * side * WATER_SAMPLE, on[1] + way[0] * side * WATER_SAMPLE];
                    heights_at(&water_triangles, &water_cells, past).is_empty()
                        && heights_at(&land, &land_cells, past).into_iter().all(|land| land <= water + LAND_OVER_WATER)
                })
            };
            at_edge[(j + 1) * w + i + 1] = match (water_height, edge) {
                (Some(water), Some(edge)) => near < reach && above_land > 0.0 && open_past(edge, water),
                _ => false,
            };
        }
    }
    // Crumbs of it the land closes in all round (never reaching its own edge where the ground past it is
    // open), a sliver of the whole, are left out: the water the game hides showing through where its models
    // stand on the ground (a town's blocks of buildings, a junction's middle), which the map's data doesn't
    // have. Where it shows on to its edge (a harbour, the coast) it's the water beyond, showing; and a lake
    // its banks close in all round is most of its own water.
    let whole: f32 = piece.shapes.iter().map(|shape| shape_area(shape)).sum();
    let crumb = (whole * WATER_CRUMB / (WATER_SAMPLE * WATER_SAMPLE)) as usize;
    let mut seen = vec![false; w * h];
    for start in 0..w * h {
        if field[start] <= 0.0 || seen[start] {
            continue;
        }
        let mut pocket = vec![start];
        seen[start] = true;
        let mut next = 0;
        while let Some(&at) = pocket.get(next) {
            next += 1;
            let (x, y) = (at % w, at / w);
            for neighbour in [
                (x > 0).then(|| at - 1),
                (x + 1 < w).then(|| at + 1),
                (y > 0).then(|| at - w),
                (y + 1 < h).then(|| at + w),
            ]
            .into_iter()
            .flatten()
            {
                if field[neighbour] > 0.0 && !seen[neighbour] {
                    seen[neighbour] = true;
                    pocket.push(neighbour);
                }
            }
        }
        if pocket.len() < crumb && !pocket.iter().any(|&at| at_edge[at]) {
            for at in pocket {
                field[at] = -reach;
            }
        }
    }
    let rings: Vec<Vec<Point>> = sea::trace(&field, w, h, 0.0)
        .into_iter()
        .map(|ring| {
            let ring: Vec<Point> = ring
                .iter()
                .map(|p| [origin[0] + (p[0] - 1.0) * WATER_SAMPLE, origin[1] + (p[1] - 1.0) * WATER_SAMPLE])
                .collect();
            sea::simplify_ring(&ring, WATER_TOLERANCE)
        })
        .filter(|ring| ring.len() >= 3)
        .collect();
    sea::group_holes(rings)
        .into_iter()
        .map(|shape| std::iter::once(shape.outline).chain(shape.holes).collect())
        .collect()
}

/// How high `triangle` is at `p`, if `p` lies on it (its edges included).
fn height_in(triangle: &[Vertex; 3], p: Point) -> Option<f32> {
    let [a, b, c] = *triangle;
    let det = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
    if det.abs() < 1e-9 {
        return None;
    }
    let u = ((b[2] - c[2]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[1] - c[2])) / det;
    let v = ((c[2] - a[2]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[1] - c[2])) / det;
    let t = 1.0 - u - v;
    const EDGE: f32 = -1e-5;
    (u >= EDGE && v >= EDGE && t >= EDGE).then(|| u * a[1] + v * b[1] + t * c[1])
}

/// The painted water within `bounds`: every terrain side's (terrain strips', roads' terrain's) and bezier
/// patch's, read before the land so the land over it can be kept as it's read.
fn painted_water(map: &Map, definitions: &Definitions, surfaces: &Surfaces, bounds: &Bounds) -> Vec<WaterPiece> {
    let has_water = |grid: &QuadGrid| {
        (0..grid.cols).any(|col| {
            (0..grid.rows)
                .any(|row| grid.material_at(row, col).is_some_and(|m| definitions.water_materials.contains(&m)))
        })
    };
    // The water painted at prefabs' edges.
    let mut pieces: Vec<WaterPiece> = surfaces
        .edge_water
        .iter()
        .map(|quads| WaterPiece {
            quads: quads.clone(),
            shapes: quads.iter().map(|quad| vec![quad.iter().map(|v| [v[0], v[2]]).collect()]).collect(),
            open: false,
        })
        .collect();
    for item in map.items.iter().filter(|item| bounds.overlaps(&item.bounds)) {
        match &item.data {
            ItemData::Terrain { start_node, end_node, right, left, relief, research } => {
                let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else { continue };
                for (side, relief, sign) in [(right, &relief[0], 1.0), (left, &relief[1], -1.0)] {
                    if has_water(&side.grid) {
                        pieces.extend(strip_ground(definitions, (start, end, research.length), 0.0, side, relief, sign).water);
                    }
                }
            }
            ItemData::BezierPatch { node, control_points, grid } => {
                let Some(node) = map.nodes.get(node) else { continue };
                if has_water(grid) {
                    pieces.extend(patch_ground(definitions, node, control_points, grid).water);
                }
            }
            // Rivers laid as curve models.
            ItemData::Curve { start_node, end_node, length, linear, subcurves, .. } => {
                let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else { continue };
                for sub in subcurves {
                    let Some(curve) = definitions.water_curves.get(&sub.model) else { continue };
                    let reach = curve.variations.get(&sub.variation).copied().unwrap_or(curve.whole);
                    let (quads, outline) = surface::river(start, end, *length, *linear, sub, reach);
                    pieces.push(WaterPiece { quads, shapes: vec![vec![outline]], open: true });
                }
            }
            ItemData::Road { look, start_node, end_node, relief, length, .. } => {
                let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else { continue };
                let Some(terrain) = map.road_terrain.get(&item.uid) else { continue };
                let width = definitions.road_looks.get(look).map_or(8.0, |look| look.width() + look.offset);
                let from = surfaces.ground_from.get(&item.uid).copied().unwrap_or([width / 2.0; 2]);
                let sides = [(&terrain.right, &relief[0], 1.0, from[0]), (&terrain.left, &relief[1], -1.0, from[1])];
                for (side, relief, sign, inner) in sides {
                    if has_water(&side.grid) {
                        pieces.extend(strip_ground(definitions, (start, end, *length), inner, side, relief, sign).water);
                    }
                }
            }
            _ => {}
        }
    }
    pieces
}

/// A piece of terrain's ground: its land, as runs of quads down a column (to paint on the sea's grid) and
/// quad by quad with their heights (to hide the water under it), and its painted water.
struct TerrainGround {
    land_runs: Vec<Vec<Point>>,
    land: Vec<[Vertex; 4]>,
    water: Option<WaterPiece>,
}

/// How many items' shapes are worked out at once: enough to keep the build's threads busy, few enough that their
/// shapes don't pile up in memory.
const WORK_BATCH: usize = 8192;

/// How far a junction's ground may lie beyond its model's points, metres (its roads' half widths): its reach
/// over the water is judged generously, so a piece is never drawn before a junction's land has come.
const PREFAB_GROUND_REACH: f32 = 50.0;

/// For each piece of painted water, the last item (by its index in the map) that can put land over it:
/// each item's land worked out as the drawing will (terrain, patches, roads: in parallel) and the pieces it
/// lies over found as the drawing finds them; a junction's, its model's points placed and grown by
/// [`PREFAB_GROUND_REACH`], over every piece within. `None`: no land can come over it.
fn water_reach(
    drawn: &[(usize, &crate::sector::Item)],
    map: &Map,
    definitions: &Definitions,
    surfaces: &Surfaces,
    water: &Water,
    prefabs: &mut Prefabs,
    workers: usize,
) -> Vec<Option<usize>> {
    let mut last: Vec<Option<usize>> = vec![None; water.pieces.len()];
    let mut reached = |index: usize, pieces: &[u32]| {
        for &piece in pieces {
            let last = &mut last[piece as usize];
            *last = Some(last.map_or(index, |last| last.max(index)));
        }
    };
    for batch in drawn.chunks(WORK_BATCH) {
        let run = batch.len().div_ceil(workers.max(1)).max(1);
        let reach: Vec<Vec<u32>> = std::thread::scope(|scope| {
            let runs: Vec<_> = batch
                .chunks(run)
                .map(|run| {
                    scope.spawn(|| {
                        run.iter().map(|&(_, item)| item_reach(item, map, definitions, surfaces, water)).collect::<Vec<_>>()
                    })
                })
                .collect();
            runs.into_iter().flat_map(|run| run.join().expect("a reach worker")).collect()
        });
        for (&(index, _), pieces) in batch.iter().zip(&reach) {
            reached(index, pieces);
        }
        for &(index, item) in batch {
            let ItemData::Prefab { model, nodes, origin, .. } = &item.data else { continue };
            let Some(prefab) = prefabs.get(*model) else { continue };
            let start = nodes.first().and_then(|uid| map.nodes.get(uid));
            let Some(own) = prefab.nodes.get(*origin as usize) else { continue };
            let Some(placement) = start.map(|at| PrefabPlacement::new(own, at)) else { continue };
            let own = prefab
                .nodes
                .iter()
                .map(|node| node.position)
                .chain(prefab.map_points.iter().map(|point| point.position))
                .chain(prefab.curves.iter().flat_map(|curve| [curve.start, curve.end]));
            let placed: Vec<Point> = own.map(|p| placement.place(p)).collect();
            if placed.is_empty() {
                continue;
            }
            let (x0, x1) = placed.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[0]), hi.max(p[0])));
            let (z0, z1) = placed.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
            let r = PREFAB_GROUND_REACH;
            let footprint =
                [[x0 - r, 0.0, z0 - r], [x1 + r, 0.0, z0 - r], [x1 + r, 0.0, z1 + r], [x0 - r, 0.0, z1 + r]];
            reached(index, &water.near(&footprint));
        }
    }
    last
}

/// The pieces of painted water an item's land (terrain, a patch's, a road's surface and terrain) lies over,
/// as the drawing finds them ([`item_work`]); none for other items.
fn item_reach(item: &crate::sector::Item, map: &Map, definitions: &Definitions, surfaces: &Surfaces, water: &Water) -> Vec<u32> {
    let mut quads: Vec<[Vertex; 4]> = Vec::new();
    match &item.data {
        ItemData::Terrain { start_node, end_node, right, left, relief, research } => {
            let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else {
                return Vec::new();
            };
            for (side, relief, sign) in [(right, &relief[0], 1.0), (left, &relief[1], -1.0)] {
                quads.extend(strip_ground(definitions, (start, end, research.length), 0.0, side, relief, sign).land);
            }
        }
        ItemData::BezierPatch { node, control_points, grid: patch } => {
            let Some(node) = map.nodes.get(node) else { return Vec::new() };
            quads.extend(patch_ground(definitions, node, control_points, patch).land);
        }
        ItemData::Road { look, start_node, end_node, relief, length, .. } => {
            let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else {
                return Vec::new();
            };
            let road_look = definitions.road_looks.get(look);
            let width = road_look.map_or(8.0, |look| look.width() + look.offset);
            let terrain = map.road_terrain.get(&item.uid);
            if terrain
                .is_some_and(|terrain| has_land(definitions, &terrain.right) || has_land(definitions, &terrain.left))
            {
                let line = geometry::road_curve(start, end);
                quads.extend(surface_quads(&line, &geometry::road_heights(start, end), width));
            }
            if let Some(terrain) = terrain {
                let from = surfaces.ground_from.get(&item.uid).copied().unwrap_or([width / 2.0; 2]);
                let sides = [(&terrain.right, &relief[0], 1.0, from[0]), (&terrain.left, &relief[1], -1.0, from[1])];
                for (side, relief, sign, inner) in sides {
                    quads.extend(strip_ground(definitions, (start, end, *length), inner, side, relief, sign).land);
                }
            }
        }
        _ => return Vec::new(),
    }
    let mut pieces: Vec<u32> = quads.iter().flat_map(|quad| water.under(quad)).collect();
    pieces.sort_unstable();
    pieces.dedup();
    pieces
}

/// What an item's drawing works out from the map alone, before it's drawn: its terrain's land, or a road's
/// line, outline, terrain and the lines it's drawn as. Nothing for other items (a junction's drawing reads
/// the prefabs, kept one at a time).
enum Work {
    None,
    Terrain([PlacedGround; 2]),
    Patch(PlacedGround),
    Road(RoadWork),
}

struct RoadWork {
    /// Its surface as land on the grid (along its lanes, as wide as it is): the cells of each piece of it.
    outline: Vec<Vec<std::ops::Range<usize>>>,
    /// Its surface over the painted water under it, where it hides it: on the ground (land beside it, in its
    /// terrain), not a bridge's.
    surface: Vec<(u32, [Vertex; 4])>,
    /// Its terrain either side, starting at its edge.
    grounds: Option<[PlacedGround; 2]>,
}

/// Each item's [`Work`], in their order, worked out by `workers` threads, each on a run of them.
fn work_out<'a>(
    items: &[(usize, &'a crate::sector::Item)],
    workers: usize,
    work: impl Fn(&'a crate::sector::Item) -> Work + Sync,
) -> Vec<Work> {
    let run = items.len().div_ceil(workers.max(1)).max(1);
    std::thread::scope(|scope| {
        let runs: Vec<_> = items
            .chunks(run)
            .map(|run| scope.spawn(|| run.iter().map(|&(_, item)| work(item)).collect::<Vec<Work>>()))
            .collect();
        runs.into_iter().flat_map(|run| run.join().expect("an items worker")).collect()
    })
}

/// The map an item's drawn on: its items, and the grid and the water its land goes onto, read (not changed)
/// while items are worked out.
struct Drawing<'a> {
    map: &'a Map,
    definitions: &'a Definitions,
    surfaces: &'a Surfaces,
    grid: &'a SeaGrid,
    water: &'a Water,
}

fn item_work(item: &crate::sector::Item, drawing: &Drawing) -> Work {
    let Drawing { map, definitions, surfaces, grid, water } = *drawing;
    let placed = |terrain: TerrainGround| PlacedGround::of(terrain, grid, water);
    match &item.data {
        ItemData::Terrain { start_node, end_node, right, left, relief, research } => {
            let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else {
                return Work::None;
            };
            let ends = (start, end, research.length);
            Work::Terrain(
                [(right, &relief[0], 1.0), (left, &relief[1], -1.0)]
                    .map(|(side, relief, sign)| placed(strip_ground(definitions, ends, 0.0, side, relief, sign))),
            )
        }
        ItemData::BezierPatch { node, control_points, grid: quads } => match map.nodes.get(node) {
            Some(node) => Work::Patch(placed(patch_ground(definitions, node, control_points, quads))),
            None => Work::None,
        },
        ItemData::Road { look, start_node, end_node, relief, length, .. } => {
            let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else {
                return Work::None;
            };
            let width = definitions.road_looks.get(look).map_or(8.0, |look| look.width() + look.offset);
            let line = geometry::road_curve(start, end);
            let outline = thick_line(&line, width).iter().map(|ring| grid.runs(&[ring])).collect();
            let heights = geometry::road_heights(start, end);
            let terrain = map.road_terrain.get(&item.uid);
            let surface = match terrain {
                Some(terrain) if has_land(definitions, &terrain.right) || has_land(definitions, &terrain.left) => {
                    water.covering(&surface_quads(&line, &heights, width))
                }
                _ => Vec::new(),
            };
            let from = surfaces.ground_from.get(&item.uid).copied().unwrap_or([width / 2.0; 2]);
            let ends = (start, end, *length);
            let grounds = terrain.map(|terrain| {
                [(&terrain.right, &relief[0], 1.0, from[0]), (&terrain.left, &relief[1], -1.0, from[1])]
                    .map(|(side, relief, sign, inner)| placed(strip_ground(definitions, ends, inner, side, relief, sign)))
            });
            Work::Road(RoadWork { outline, surface, grounds })
        }
        _ => Work::None,
    }
}

/// Terrain's land, to go onto the sea's grid and be kept over the painted water it lies over (its water was
/// read before: [`painted_water`]), worked out without changing either: the grid's cells it covers, and the
/// water pieces each of its quads lies over.
struct PlacedGround {
    runs: Vec<Vec<std::ops::Range<usize>>>,
    covered: Vec<(u32, [Vertex; 4])>,
}

impl PlacedGround {
    fn of(terrain: TerrainGround, grid: &SeaGrid, water: &Water) -> Self {
        let runs = terrain.land_runs.iter().map(|ring| grid.runs(&[ring])).collect();
        Self { runs, covered: water.covering(&terrain.land) }
    }

    /// Onto the grid and over the water.
    fn put(&self, grid: &mut SeaGrid, water: &mut Water) {
        for runs in &self.runs {
            grid.fill(runs, Cell::Land);
        }
        water.cover(&self.covered);
    }
}

/// The quads of a road's surface (`line` with its `heights`, `width` wide), as land over the water.
fn surface_quads(line: &[Point], heights: &[f32], width: f32) -> Vec<[Vertex; 4]> {
    let half = width / 2.0;
    (0..line.len().saturating_sub(1).min(heights.len().saturating_sub(1)))
        .filter_map(|i| {
            let (a, b) = (line[i], line[i + 1]);
            let way = direction(a, b)?;
            let across = [-way[1] * half, way[0] * half];
            let (ha, hb) = (heights[i], heights[i + 1]);
            Some([
                [a[0] + across[0], ha, a[1] + across[1]],
                [b[0] + across[0], hb, b[1] + across[1]],
                [b[0] - across[0], hb, b[1] - across[1]],
                [a[0] - across[0], ha, a[1] - across[1]],
            ])
        })
        .collect()
}

/// The shapes of a grid's water quads (`is_water` by row and column), their corners placed by `corner`
/// (column, row).
fn water_shapes(
    grid: &QuadGrid,
    is_water: impl Fn(u16, u16) -> bool,
    corner: impl Fn(u16, u16) -> Point,
) -> Vec<Vec<Vec<Point>>> {
    terrain::region_outlines(grid.rows, grid.cols, is_water)
        .into_iter()
        .map(|shape| {
            shape.into_iter().map(|ring| ring.into_iter().map(|(col, row)| corner(col, row)).collect()).collect()
        })
        .collect()
}

fn polygon_feature(
    layer: &'static str,
    minzoom: u8,
    rings: &[Vec<Point>],
    props: Vec<(&'static str, Value)>,
) -> Feature {
    let rings = rings.iter().map(|ring| ring.iter().map(|&p| to_mercator(p)).collect()).collect();
    Feature { layer, minzoom, maxzoom: u8::MAX, props, geometry: Geometry::Polygon(rings) }
}

/// The zoom the buildings' tiles have them at (overzoomed past it; the page raises them from 15).
const BUILDING_ZOOM: u8 = 14;

/// A building's part to raise (buildings.rs): its outline and holes, how high it reaches and where it starts (metres above the
/// ground), what kind (`building`, `landmark`, `canopy`), and a canopy's id (the page leaves out the one the
/// truck is under).
pub fn building_feature(building: &crate::buildings::Building) -> Feature {
    use crate::world::BuildingKind;
    let kind = match building.kind {
        BuildingKind::Building => "building",
        BuildingKind::Landmark => "landmark",
        BuildingKind::Canopy => "canopy",
    };
    let round = |metres: f32| f64::from((metres * 10.0).round() / 10.0);
    let mut props = vec![
        ("height", Value::Num(round(building.height))),
        ("base", Value::Num(round(building.base))),
        ("kind", Value::Str(kind.to_owned())),
    ];
    if building.kind == BuildingKind::Canopy {
        // Within what a number keeps exactly in the page (2^53).
        props.push(("id", Value::Num((building.uid & ((1 << 53) - 1)) as f64)));
    }
    polygon_feature("building", BUILDING_ZOOM, &building.rings, props)
}

fn area_feature(ring: &[Point], color: u8, over: bool) -> Feature {
    let props = vec![("color", Value::Num(color as f64)), ("over", Value::Bool(over))];
    polygon_feature("area", AREA_ZOOM, std::slice::from_ref(&ring.to_vec()), props)
}

/// A place the map marks with an icon: what a prefab's spawn points mark, where it offers rest, a company's
/// depot, or a ferry port or train terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Place {
    Fuel,
    Service,
    /// A garage the player can own (owned or for sale: which, the save game knows, not the map).
    Garage,
    /// A truck dealer.
    Dealer,
    /// A company's depot.
    Company,
    /// Where the player can rest: a rest area, or a filling station's or a service area's resting place.
    Parking,
    /// A hotel (a resting place, in the game's terms, but a hotel's).
    Hotel,
    /// A recruitment agency (drivers to hire).
    Recruitment,
    WeighStation,
    Ferry,
    /// A train terminal (the Channel Tunnel's shuttles).
    Train,
    /// A toll gate.
    Toll,
    /// A border crossing.
    Border,
}

impl Place {
    fn of(kind: SpawnKind) -> Option<Self> {
        match kind {
            SpawnKind::Fuel => Some(Self::Fuel),
            SpawnKind::Service => Some(Self::Service),
            SpawnKind::Garage | SpawnKind::GarageForSale => Some(Self::Garage),
            SpawnKind::TruckDealer => Some(Self::Dealer),
            SpawnKind::Recruitment => Some(Self::Recruitment),
            SpawnKind::WeighStation => Some(Self::WeighStation),
            _ => None,
        }
    }

    /// What a service area offers: a route goes through the area to it (access.rs).
    fn in_service_area(self) -> bool {
        matches!(self, Self::Fuel | Self::Service | Self::Parking)
    }

    fn name(self) -> &'static str {
        match self {
            Self::Fuel => "fuel",
            Self::Service => "service",
            Self::Garage => "garage",
            Self::Dealer => "dealer",
            Self::Company => "company",
            Self::Parking => "parking",
            Self::Hotel => "hotel",
            Self::Recruitment => "recruitment",
            Self::WeighStation => "weigh",
            Self::Ferry => "ferry",
            Self::Train => "train",
            Self::Toll => "toll",
            Self::Border => "border",
        }
    }

    /// The place a map overlay marks, by the icon it names: toll gates, border crossings and weigh stations
    /// (some weigh stations are marked so, others by their prefab's spawn points).
    fn of_overlay(item: &ItemData) -> Option<Self> {
        let ItemData::MapOverlay { flags, kind: 0, icon, .. } = item else { return None };
        if flags.hidden || flags.secret {
            return None;
        }
        match token_to_string(*icon)?.as_str() {
            "toll_ico" => Some(Self::Toll),
            "border_ico" => Some(Self::Border),
            "weigh_ico" => Some(Self::WeighStation),
            _ => None,
        }
    }

    /// What the place is, as the game's map legend says it in the player's language ("Recruitment agency",
    /// "Resting place"; the game's own case, right for every language), for a place with no name of its
    /// own: an icon alone doesn't read as a real map's. A garage is just a garage: whether it's the player's
    /// or for sale, the save game knows, not the map.
    fn generic_name(self, strings: &Strings) -> Option<String> {
        let key = match self {
            // The game has no word for it (its legend calls hotels resting places): ours, the same or
            // nearly so in most of its languages.
            Self::Hotel => return Some("Hotel".into()),
            Self::Fuel => "mapl_gas",
            Self::Service => "mapl_service",
            Self::Garage => "truckman_garage",
            Self::Dealer => "mapl_dealer",
            Self::Company => return None,
            Self::Parking => "mapl_parking",
            Self::Recruitment => "mapl_recruit",
            Self::WeighStation => "mapl_weighst",
            Self::Ferry => "mapl_port",
            Self::Train => "mapl_train",
            Self::Toll => "mapl_toll",
            Self::Border => "mapl_border",
        };
        strings.resolve(&format!("@@{key}@@")).map(str::to_owned)
    }

    /// The zoom it appears from: ferries, trains and border crossings (few, and long routes run through
    /// them) further out.
    fn zoom(self) -> u8 {
        match self {
            Self::Ferry | Self::Train | Self::Border => TRANSIT_ZOOM,
            _ => PLACE_ZOOM,
        }
    }
}

/// The fuel companies (Ini in Italy, ÉCO in France, Primel and TQG in Scandinavia, Medas in Greece): the
/// game puts them on filling stations, where fuel is delivered to them, so every site of theirs is a filling
/// station of that brand rather than a depot.
struct FuelBrands {
    /// The companies, by token.
    companies: HashSet<u64>,
    /// Their filling stations, by prefab item: the company's name.
    stations: HashMap<u64, String>,
}

fn fuel_brands(map: &Map, definitions: &Definitions, strings: &Strings, prefabs: &mut Prefabs) -> FuelBrands {
    let mut models = HashMap::new();
    let mut stations = Vec::new();
    for item in &map.items {
        let ItemData::Prefab { model, .. } = &item.data else { continue };
        models.insert(item.uid, *model);
        let Some(prefab) = prefabs.get(*model) else { continue };
        let points = &prefab.spawn_points;
        if !points.is_empty() && points.iter().all(|point| point.kind == SpawnKind::Fuel) {
            let [x0, z0, x1, z1] = item.bounds;
            stations.push((item.uid, [(x0 + x1) / 2.0, (z0 + z1) / 2.0]));
        }
    }
    let mut sites = Vec::new();
    for item in &map.items {
        let ItemData::Company { company, prefab, node, .. } = &item.data else { continue };
        let site = match models.get(prefab).and_then(|model| prefabs.get(*model)) {
            Some(own) if own.spawn_points.iter().any(|point| point.kind == SpawnKind::Fuel) => Site::Station(*prefab),
            Some(own) if own.spawn_points.is_empty() => match map.nodes.get(node) {
                Some(node) => Site::Open([node.position[0], node.position[2]]),
                None => Site::Depot,
            },
            _ => Site::Depot,
        };
        sites.push((*company, site));
    }
    let mut brands = FuelBrands { companies: HashSet::new(), stations: HashMap::new() };
    for (company, owned) in brand_stations(&sites, &stations) {
        let Some(name) = definitions.companies.get(&company) else { continue };
        let name = strings.resolve(name).unwrap_or(name);
        brands.stations.extend(owned.into_iter().map(|station| (station, name.to_owned())));
        brands.companies.insert(company);
    }
    brands
}

/// What stands at a company's site.
#[derive(Clone, Copy)]
enum Site {
    /// A filling station, the site's own prefab (Ini's and ÉCO's `…_company` gas plazas).
    Station(u64),
    /// A prefab with no spawn points of its own (a custom depot): what's built there are prefabs of their
    /// own around it, at this point.
    Open(Point),
    /// Anything else: a depot.
    Depot,
}

/// The fuel companies, by `sites` (company, what stands there): those whose every site is a filling station,
/// either its own prefab or, at an open site, the filling stations (`stations`: prefab item, middle) within
/// [`STATION_REACH`] of it; with those stations. A station two companies' sites reach is the nearer site's
/// (its own prefab nearest of all), or the lower company's at the same distance: never left to chance (a
/// hash table's order had a station near two sites change its name from build to build). A company left
/// with none is still a fuel company.
fn brand_stations(sites: &[(u64, Site)], stations: &[(u64, Point)]) -> HashMap<u64, Vec<u64>> {
    // Each company's stations, each with how far from its site; `None` once a site of it isn't a station.
    let mut brands: HashMap<u64, Option<Vec<(u64, f32)>>> = HashMap::new();
    for &(company, site) in sites {
        let found: Vec<(u64, f32)> = match site {
            Site::Station(prefab) => vec![(prefab, 0.0)],
            Site::Open(at) => stations
                .iter()
                .map(|&(prefab, middle)| (prefab, (middle[0] - at[0]).hypot(middle[1] - at[1])))
                .filter(|&(_, away)| away <= STATION_REACH)
                .collect(),
            Site::Depot => Vec::new(),
        };
        let owned = brands.entry(company).or_insert_with(|| Some(Vec::new()));
        match (found.is_empty(), owned.as_mut()) {
            (false, Some(owned)) => owned.extend(found),
            _ => *owned = None,
        }
    }
    let brands: Vec<(u64, Vec<(u64, f32)>)> =
        brands.into_iter().filter_map(|(company, owned)| Some((company, owned?))).collect();
    // Each station's company: the nearest, then the lowest.
    let mut owner: HashMap<u64, (f32, u64)> = HashMap::new();
    for (company, owned) in &brands {
        for &(station, away) in owned {
            let best = owner.entry(station).or_insert((away, *company));
            if (away, *company) < *best {
                *best = (away, *company);
            }
        }
    }
    brands
        .into_iter()
        .map(|(company, owned)| {
            let mine = owned
                .into_iter()
                .filter(|(station, _)| owner.get(station).is_some_and(|&(_, best)| best == company))
                .map(|(station, _)| station)
                .collect();
            (company, mine)
        })
        .collect()
}

/// How far from an open site its filling station's pumps may stand, metres. The fuel companies' pumps stand
/// within 80 m of theirs; the next filling station along, 114 m or more.
const STATION_REACH: f32 = 100.0;

/// The names the brands give their truck dealerships (not translated), by the dealer prefab's folder.
const DEALER_NAMES: [(&str, &str); 7] = [
    ("daf", "DAF"),
    ("iveco", "Iveco"),
    ("man", "MAN Truck & Bus"),
    ("mb", "Mercedes-Benz Truck & Van"),
    ("renault", "Renault Trucks"),
    ("scania", "SCANIA"),
    ("volvo", "Volvo Trucks"),
];

/// A truck dealer's name, from its prefab's path (`/prefab2/truck_dealer/<brand>/…`): the brand's own name
/// for its dealerships, or for a brand not listed (a mod's), the game's name for the brand (`brand_<id>` in
/// its text, in the player's language). `None` for a dealer of no particular brand.
fn dealer_brand(path: &str, strings: &Strings) -> Option<String> {
    let brand = path.strip_prefix("/prefab2/truck_dealer/")?.split_once('/')?.0;
    match DEALER_NAMES.iter().find(|(folder, _)| *folder == brand) {
        Some((_, name)) => Some((*name).to_owned()),
        None => strings.resolve(&format!("@@brand_{brand}@@")).map(str::to_owned),
    }
}

/// The zoom places appear from (the page shows them closer in, and keeps them from piling up), and ferry
/// ports and train terminals.
const PLACE_ZOOM: u8 = 11;
const TRANSIT_ZOOM: u8 = 9;

/// The places a prefab offers, each once, in the prefab's own space: at the middle of its spawn points of
/// that kind (a filling station's pumps make one icon), and a resting place (a hotel, if `hotel`) at the
/// middle of its rest triggers' corners, the area where the game offers sleep, unless it's a garage's or
/// a dealer's (their own icon says enough). In the order they first appear. (Parking spawn points don't
/// make one: farms and depots have them, for parked trucks, but nowhere to rest.)
fn prefab_places(prefab: &Prefab, hotel: bool) -> Vec<(Place, Point)> {
    let rest = token::string_to_token(REST_TRIGGER);
    let resting = if hotel { Place::Hotel } else { Place::Parking };
    let spawned = prefab.spawn_points.iter().filter_map(|point| Some((Place::of(point.kind)?, point.position)));
    let rests = prefab.triggers.iter().filter(|trigger| Some(trigger.action) == rest).map(|t| (resting, t.position));
    let mut places: Vec<(Place, Point, f32)> = Vec::new();
    for (place, at) in spawned.chain(rests) {
        match places.iter_mut().find(|(seen, _, _)| *seen == place) {
            Some((_, sum, count)) => (sum[0], sum[1], *count) = (sum[0] + at[0], sum[1] + at[1], *count + 1.0),
            None => places.push((place, at, 1.0)),
        }
    }
    let own_icon = places.iter().any(|(place, _, _)| matches!(place, Place::Garage | Place::Dealer));
    places
        .into_iter()
        .filter(|(place, _, _)| !(own_icon && *place == Place::Parking))
        .map(|(place, sum, count)| (place, [sum[0] / count, sum[1] / count]))
        .collect()
}

/// Whether the map marks places in a prefab ([`prefab_places`]): its buildings are raised (buildings.rs).
pub fn has_places(prefab: &Prefab) -> bool {
    !prefab_places(prefab, false).is_empty()
}

/// The city a place at `at` belongs to: the city whose area it's in, or the nearest within reach (some
/// garages and depots stand just outside their city's area).
fn city_of(at: Point, areas: &[(u64, [f32; 4])]) -> Option<u64> {
    let outside =
        |[x0, z0, x1, z1]: [f32; 4]| (x0 - at[0]).max(at[0] - x1).max(0.0).hypot((z0 - at[1]).max(at[1] - z1).max(0.0));
    areas
        .iter()
        .map(|&(city, area)| (outside(area), city))
        .filter(|&(distance, _)| distance <= CITY_REACH)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, city)| city)
}

/// How far outside its city's area a place may stand, metres.
const CITY_REACH: f32 = 3000.0;

/// How far from a road a place with no joined junction end of its own (a ferry port's docks) may be, and a
/// city's centre, to be routed to that road, metres.
const PLACE_ROAD_REACH: f32 = 500.0;
const CITY_ROAD_REACH: f32 = 3000.0;

/// Places of one kind and name this close together are one place (a filling station's pump islands, each a
/// junction of its own), metres.
const SAME_DESTINATION: f32 = 150.0;

/// The destinations with each place once: of those of one kind and name within [`SAME_DESTINATION`], the
/// first.
fn one_each(destinations: Vec<Destination>) -> Vec<Destination> {
    let cell = |at: Point| ((at[0] / SAME_DESTINATION).floor() as i32, (at[1] / SAME_DESTINATION).floor() as i32);
    let mut kept: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    let mut each = Vec::new();
    for destination in destinations {
        let (x, z) = cell(destination.at);
        let seen = (x - 1..=x + 1).flat_map(|x| (z - 1..=z + 1).map(move |z| (x, z))).any(|near| {
            kept.get(&near).is_some_and(|indices| {
                indices.iter().any(|&i: &usize| {
                    let other: &Destination = &each[i];
                    other.kind == destination.kind
                        && other.name == destination.name
                        && distance(other.at, destination.at) <= SAME_DESTINATION
                })
            })
        });
        if !seen {
            kept.entry((x, z)).or_default().push(each.len());
            each.push(destination);
        }
    }
    each
}

/// Of `ends` (node, where), the node nearest `at`.
/// `prefab`'s ways through from each of its joined `ends` to each other, placed.
fn ways_through_prefab(prefab: &Prefab, nodes: &[u64], origin: usize, placement: &PrefabPlacement, ends: &[(u64, Point)]) -> Vec<Through> {
    let world = |own: usize| nodes.get(geometry::prefab_world_node(own, origin, nodes.len())).copied();
    let joined = |uid: &u64| ends.iter().any(|(end, _)| end == uid);
    let mut ways = Vec::new();
    for own_from in 0..prefab.nodes.len() {
        for own_to in 0..prefab.nodes.len() {
            let (Some(from), Some(to)) = (world(own_from), world(own_to)) else { continue };
            if own_from == own_to || !joined(&from) || !joined(&to) {
                continue;
            }
            if let Some(path) = prefab.path(own_from, own_to) {
                ways.push(Through { from, to, line: path.iter().map(|p| placement.place(*p)).collect() });
            }
        }
    }
    ways
}

fn nearest_end(ends: &[(u64, Point)], at: Point) -> Option<u64> {
    ends.iter().min_by(|a, b| distance(a.1, at).total_cmp(&distance(b.1, at))).map(|&(node, _)| node)
}

/// Nodes by where they are, in square cells, to find the one nearest a point.
#[derive(Default)]
struct NodeGrid {
    cells: HashMap<(i32, i32), Vec<(u64, Point)>>,
}

/// A [`NodeGrid`] cell's size, metres.
const NODE_CELL: f32 = 500.0;

impl NodeGrid {
    fn cell(at: Point) -> (i32, i32) {
        ((at[0] / NODE_CELL).floor() as i32, (at[1] / NODE_CELL).floor() as i32)
    }

    fn add(&mut self, node: u64, at: Point) {
        self.cells.entry(Self::cell(at)).or_default().push((node, at));
    }

    /// The node nearest `at` within `reach` metres of those `such` says.
    fn nearest_such(&self, at: Point, reach: f32, such: &dyn Fn(u64) -> bool) -> Option<u64> {
        let (x, z) = Self::cell(at);
        let cells = (reach / NODE_CELL).ceil() as i32;
        (x - cells..=x + cells)
            .flat_map(|x| (z - cells..=z + cells).map(move |z| (x, z)))
            .filter_map(|cell| self.cells.get(&cell))
            .flatten()
            .map(|&(node, there)| (node, distance(at, there)))
            .filter(|&(node, d)| d <= reach && such(node))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(node, _)| node)
    }
}

/// How near two marks of one kind of place may be and be the same place, metres.
const SAME_PLACE: f32 = 200.0;

/// The trigger action where the game offers the player rest.
const REST_TRIGGER: &str = "hud_parking";

/// The rest areas made of roads and models, marked only by their rest triggers (`triggers`, each one's
/// middle; owner 2026-10-21: the game's map shows the two on either side of the motorway north-west of
/// Nürnberg, ours showed none): one each, but for a trigger a rest area's prefab (`prefab_places`) or
/// another trigger beside it marks already.
fn lone_rests(triggers: &[Point], prefab_places: &[(Place, Point)]) -> Vec<Point> {
    let resting = |place: Place| matches!(place, Place::Parking | Place::Hotel);
    let mut rests: Vec<Point> = Vec::new();
    for &at in triggers {
        let known = prefab_places.iter().any(|&(place, there)| resting(place) && distance(at, there) < REST_BESIDE)
            || rests.iter().any(|&there| distance(at, there) < SAME_REST);
        if !known {
            rests.push(at);
        }
    }
    rests
}

/// How near a rest trigger on the map by itself may be to a rest area's prefab and be part of it, metres
/// (the rest area across a motorway, about 100 m off, is another one).
const REST_BESIDE: f32 = 80.0;
/// How near two rest triggers on the map by themselves may be and be one rest area, metres (the rest
/// areas on either side of a motorway stay two, as the game's map shows them).
const SAME_REST: f32 = 50.0;

/// Whether a prefab is a hotel, by its descriptor's folder or name (`/prefab/hotel/…`, `…/hotel_c_sc.ppd`).
fn is_hotel(path: &str) -> bool {
    path.split('/').any(|part| part == "hotel" || part.starts_with("hotel_"))
}

fn place_feature(at: Point, place: Place, name: Option<String>) -> Feature {
    let mut props = vec![("kind", Value::Str(place.name().into()))];
    props.extend(name.map(|name| ("name", Value::Str(name))));
    Feature {
        layer: "place",
        minzoom: place.zoom(),
        maxzoom: u8::MAX,
        props,
        geometry: Geometry::Point(to_mercator(at)),
    }
}

/// The zoom road number signs appear from (the page shows them a step closer in).
const SIGN_ZOOM: u8 = 9;

fn sign_feature(at: Point, sign: String) -> Feature {
    let props = vec![("sign", Value::Str(sign))];
    Feature { layer: "sign", minzoom: SIGN_ZOOM, maxzoom: u8::MAX, props, geometry: Geometry::Point(to_mercator(at)) }
}

/// The zoom no-entry signs appear from (the page shows them a step closer in).
const NO_ENTRY_ZOOM: u8 = 14;

/// A no-entry sign at a hidden road's entrance ([`crate::no_entry`]).
pub fn no_entry_feature(at: Point) -> Feature {
    Feature { layer: "no_entry", minzoom: NO_ENTRY_ZOOM, maxzoom: u8::MAX, props: Vec::new(), geometry: Geometry::Point(to_mercator(at)) }
}

/// What a terrain quad is: painted with water, or land. (Its height is no test for water: the sea is flat
/// at height 0, but polders and some inland ground lie below that too.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ground {
    Water,
    Land,
}

/// The quads of one side of a terrain strip (a terrain item, or a road's terrain), by what they are,
/// merged across each column. `inner` is where the side starts, metres from the curve; `sign` is 1 for
/// the right of travel, -1 for the left.
fn strip_ground(
    definitions: &Definitions,
    (start, end, length): (&Node, &Node, f32),
    inner: f32,
    side: &TerrainSide,
    relief: &Relief,
    sign: f32,
) -> TerrainGround {
    let grid = &side.grid;
    if grid.rows == 0 || grid.cols == 0 {
        return TerrainGround { land_runs: Vec::new(), land: Vec::new(), water: None };
    }
    let frames: Vec<geometry::Frame> = (0..=usize::from(grid.cols))
        .map(|col| {
            let (p, right) = surface::frame(start, end, f64::from(length), col as f64 / f64::from(grid.cols), true);
            geometry::Frame {
                point: [p[0] as f32, p[2] as f32],
                right: [right[0] as f32, right[1] as f32],
                height: p[1] as f32,
            }
        })
        .collect();
    let profile = definitions.terrain_profiles.get(&relief.profile);
    let ground = terrain::side_ground(&frames, inner, sign, side, relief, profile, start.position);
    let kind = |row: u16, col: u16| {
        if grid.material_at(row, col).is_some_and(|m| definitions.water_materials.contains(&m)) {
            Ground::Water
        } else {
            Ground::Land
        }
    };
    // The land in runs down each column: out along one column line, back along the next, through every row
    // edge between.
    let land_runs = grid
        .runs(|row, col| kind(row, col) == Ground::Land)
        .into_iter()
        .map(|(col, first, last)| {
            let near = (first..=last + 1).map(|row| ground.vertex(col, row));
            let far = (first..=last + 1).rev().map(|row| ground.vertex(col + 1, row));
            near.chain(far).map(|v| [v[0], v[2]]).collect()
        })
        .collect();
    let quad = |row: u16, col: u16| {
        [
            ground.vertex(col, row),
            ground.vertex(col + 1, row),
            ground.vertex(col + 1, row + 1),
            ground.vertex(col, row + 1),
        ]
    };
    let of_kind = |wanted: Ground| -> Vec<[Vertex; 4]> {
        (0..grid.cols)
            .flat_map(|col| (0..grid.rows).map(move |row| (row, col)))
            .filter(|&(row, col)| kind(row, col) == wanted)
            .map(|(row, col)| quad(row, col))
            .collect()
    };
    let water_quads = of_kind(Ground::Water);
    let water = (!water_quads.is_empty()).then(|| WaterPiece {
        quads: water_quads,
        shapes: water_shapes(
            grid,
            |row, col| kind(row, col) == Ground::Water,
            |col, row| {
                let v = ground.vertex(col, row);
                [v[0], v[2]]
            },
        ),
        open: false,
    });
    TerrainGround { land_runs, land: of_kind(Ground::Land), water }
}

/// The quads of a bezier patch, by what they are. Rows run along the patch's x, columns along its z.
fn patch_ground(
    definitions: &Definitions,
    node: &Node,
    control_points: &[[[f32; 3]; 4]; 4],
    grid: &QuadGrid,
) -> TerrainGround {
    // Its points in the world, as high as its own (the patch turned only about the up axis).
    let at = |row: u16, col: u16| -> Vertex {
        let (u, v) = (row as f32 / grid.rows as f32, col as f32 / grid.cols as f32);
        let p = geometry::bezier_patch_point(control_points, u, v);
        let [x, z] = geometry::from_node_frame(node, [p[0], p[2]]);
        [x, node.position[1] + p[1], z]
    };
    let is_water = |row, col| grid.material_at(row, col).is_some_and(|m| definitions.water_materials.contains(&m));
    let (mut land_runs, mut land, mut water_quads) = (Vec::new(), Vec::new(), Vec::new());
    for col in 0..grid.cols {
        for row in 0..grid.rows {
            let quad = [at(row, col), at(row + 1, col), at(row + 1, col + 1), at(row, col + 1)];
            if is_water(row, col) {
                water_quads.push(quad);
            } else {
                land_runs.push(quad.iter().map(|v| [v[0], v[2]]).collect());
                land.push(quad);
            }
        }
    }
    let water = (!water_quads.is_empty()).then(|| WaterPiece {
        quads: water_quads,
        shapes: water_shapes(grid, is_water, |col, row| {
            let v = at(row, col);
            [v[0], v[2]]
        }),
        open: false,
    });
    TerrainGround { land_runs, land, water }
}

fn distance(a: Point, b: Point) -> f32 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length2 = dx * dx + dy * dy;
    let t = if length2 == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length2).clamp(0.0, 1.0) };
    (p[0] - (a[0] + dx * t)).hypot(p[1] - (a[1] + dy * t))
}

/// The unit direction from `a` to `b`, if they're apart.
fn direction(a: Point, b: Point) -> Option<Point> {
    let (x, z) = (b[0] - a[0], b[1] - a[1]);
    let long = x.hypot(z);
    (long > 0.0).then(|| [x / long, z / long])
}

/// A line `width` metres wide, as one quad per segment.
fn thick_line(line: &[Point], width: f32) -> Vec<Vec<Point>> {
    let half = width / 2.0;
    line.windows(2)
        .filter_map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            let way = direction(a, b)?;
            let across = [-way[1] * half, way[0] * half];
            Some(vec![
                [a[0] + across[0], a[1] + across[1]],
                [b[0] + across[0], b[1] + across[1]],
                [b[0] - across[0], b[1] - across[1]],
                [a[0] - across[0], a[1] - across[1]],
            ])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A piece's shapes as they show, never stopped.
    fn shown_water_now(piece: &WaterPiece, covers: &[[Vertex; 4]]) -> Vec<Vec<Vec<Point>>> {
        shown_water(piece, covers, &std::sync::atomic::AtomicBool::new(false))
    }
    use crate::prefab::{SpawnPoint, Trigger};

    /// A quad from `x0` to `x1` and `z0` to `z1`, `h0` high along x0 and `h1` along x1.
    fn ground_quad(x0: f32, z0: f32, x1: f32, z1: f32, h0: f32, h1: f32) -> [Vertex; 4] {
        [[x0, h0, z0], [x1, h1, z0], [x1, h1, z1], [x0, h0, z1]]
    }

    /// Water 0 m high from x 0 to 100 and z 0 to 100, as four quads, one shape.
    fn sea_piece() -> WaterPiece {
        let quads = [(0.0, 0.0), (50.0, 0.0), (0.0, 50.0), (50.0, 50.0)]
            .map(|(x, z)| ground_quad(x, z, x + 50.0, z + 50.0, 0.0, 0.0))
            .to_vec();
        WaterPiece { quads, shapes: vec![vec![vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]]], open: false }
    }

    /// A shape's extent along x.
    fn x_extent(shapes: &[Vec<Vec<Point>>]) -> (f32, f32) {
        shapes.iter().flatten().flatten().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[0]), hi.max(p[0])))
    }

    #[test]
    fn painted_water_shows_where_no_land_lies_over_it() {
        // Nothing over it: as it is.
        assert_eq!(shown_water_now(&sea_piece(), &[]), sea_piece().shapes);
        // Land 5 m up over it from x 60 on (and on beyond it): it ends there, to half a sample (where land
        // stops sheer, the water's edge falls between two samples); its own edges stay where they are (to
        // meet the next piece's).
        let shown = shown_water_now(&sea_piece(), &[ground_quad(60.0, -20.0, 140.0, 120.0, 5.0, 5.0)]);
        assert_eq!(shown.len(), 1);
        let (left, right) = x_extent(&shown);
        assert!(left.abs() < 0.05 && (right - 60.0).abs() <= WATER_SAMPLE / 2.0 + 0.01, "{left} {right}");
        let area = shape_area(&shown[0]);
        // Less, at most, the corners the tracing cuts across: half a sample square each.
        assert!(
            (area - 6000.0).abs() <= 100.0 * WATER_SAMPLE / 2.0 + 4.0 * WATER_SAMPLE * WATER_SAMPLE / 2.0,
            "{area}"
        );
        for p in shown.iter().flatten().flatten() {
            assert!(p[1] > -0.05 && p[1] < 100.05, "{p:?}");
        }
        // A beach rising out of it, from 5 m under at x 0 to 5 m up at x 100: it ends where the beach is
        // a quarter of a metre over it.
        let beach = shown_water_now(&sea_piece(), &[ground_quad(0.0, 0.0, 100.0, 100.0, -5.0, 5.0)]);
        let (_, right) = x_extent(&beach);
        assert!((right - 52.5).abs() < 0.5, "{right}");
        // The sea bed under it hides nothing.
        let bed = shown_water_now(&sea_piece(), &[ground_quad(-20.0, -20.0, 120.0, 120.0, -10.0, -10.0)]);
        assert!((shape_area(&bed[0]) - 10_000.0).abs() < 10.0);
        // A lake in a field all round it, the field's ground running on under the lake: it shows whole.
        let field = [ground_quad(-50.0, -50.0, 150.0, 150.0, -0.5, -0.5)];
        assert!((shape_area(&shown_water_now(&sea_piece(), &field)[0]) - 10_000.0).abs() < 10.0);
    }

    #[test]
    fn crumbs_of_hidden_water_are_left_out_but_lakes_and_harbours_kept() {
        // A lake under its banks all round (land 5 m up 10 m in from its edges): most of it shows.
        let banks = [
            ground_quad(-10.0, -10.0, 10.0, 110.0, 5.0, 5.0),
            ground_quad(90.0, -10.0, 110.0, 110.0, 5.0, 5.0),
            ground_quad(-10.0, -10.0, 110.0, 10.0, 5.0, 5.0),
            ground_quad(-10.0, 90.0, 110.0, 110.0, 5.0, 5.0),
        ];
        let lake = shown_water_now(&sea_piece(), &banks);
        assert_eq!(lake.len(), 1);
        assert!((shape_area(&lake[0]) - 6400.0).abs() < 400.0, "{}", shape_area(&lake[0]));
        // A strip of sea a kilometre square under a town, but for a block 20 m square in its middle where
        // nothing lies over it: a crumb, left out.
        let sea = WaterPiece {
            quads: vec![ground_quad(0.0, 0.0, 1000.0, 1000.0, 0.0, 0.0)],
            shapes: vec![vec![vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0], [0.0, 1000.0]]]],
            open: false,
        };
        let town = [
            ground_quad(-10.0, -10.0, 490.0, 1010.0, 20.0, 20.0),
            ground_quad(510.0, -10.0, 1010.0, 1010.0, 20.0, 20.0),
            ground_quad(490.0, -10.0, 510.0, 490.0, 20.0, 20.0),
            ground_quad(490.0, 510.0, 510.0, 1010.0, 20.0, 20.0),
        ];
        assert!(shown_water_now(&sea, &town).is_empty());
        // The same block reaching the sea's own edge, open past it (a harbour): kept.
        let harbour = [
            ground_quad(-10.0, -10.0, 490.0, 1010.0, 20.0, 20.0),
            ground_quad(510.0, -10.0, 1010.0, 1010.0, 20.0, 20.0),
            ground_quad(490.0, -10.0, 510.0, 490.0, 20.0, 20.0),
        ];
        let shown = shown_water_now(&sea, &harbour);
        assert_eq!(shown.len(), 1);
        // 20 m wide, less up to half a sample at each sheer side.
        let area = shape_area(&shown[0]);
        assert!(area > (20.0 - WATER_SAMPLE - 0.1) * 505.0 && area < 20.0 * 515.0, "{area}");
    }

    #[test]
    fn only_land_rising_over_the_water_is_kept_for_it() {
        let mut water = Water::new(vec![sea_piece()]);
        water.land(&ground_quad(-20.0, -20.0, 120.0, 120.0, -10.0, -10.0));
        water.land(&ground_quad(500.0, 500.0, 600.0, 600.0, 50.0, 50.0));
        assert_eq!(water.cut(), 0);
        water.land(&ground_quad(60.0, -20.0, 140.0, 120.0, 5.0, 5.0));
        assert_eq!(water.cut(), 1);
        let shown = water.shown(&mut |_| {});
        assert!(
            (shape_area(&shown[0]) - 6000.0).abs()
                <= 100.0 * WATER_SAMPLE / 2.0 + 4.0 * WATER_SAMPLE * WATER_SAMPLE / 2.0
        );
        assert!((water.area - 10_000.0).abs() < 1.0);
    }

    #[test]
    fn names_dealers_as_their_brands_do() {
        let strings: Strings = [("brand_volvo", "Volvo"), ("brand_kenworth", "Kenworth")]
            .map(|(k, v)| (k.into(), v.into()))
            .into_iter()
            .collect();
        let brand = |path| dealer_brand(path, &strings);
        assert_eq!(brand("/prefab2/truck_dealer/volvo/volvo_truck_dealer_big_02.ppd").as_deref(), Some("Volvo Trucks"));
        assert_eq!(
            brand("/prefab2/truck_dealer/mb/mb_truck_dealer_big_02.ppd").as_deref(),
            Some("Mercedes-Benz Truck & Van")
        );
        // A brand not listed: the game's name for it.
        assert_eq!(brand("/prefab2/truck_dealer/kenworth/kw_dealer.ppd").as_deref(), Some("Kenworth"));
        // No brand folder, or a brand nobody names: no name.
        assert_eq!(brand("/prefab2/truck_dealer/truck_dealer_fr.ppd"), None);
        assert_eq!(brand("/prefab2/truck_dealer/unknown/dealer.ppd"), None);
    }

    #[test]
    fn a_prefab_offers_each_place_once_at_the_middle_of_its_points() {
        let spawn = |x: f32, kind| SpawnPoint { position: [x, 0.0], kind };
        let filling_station = Prefab {
            nodes: Vec::new(),
            map_points: Vec::new(),
            spawn_points: vec![
                spawn(0.0, SpawnKind::Other(1)),
                spawn(10.0, SpawnKind::Fuel),
                spawn(20.0, SpawnKind::Fuel),
                spawn(50.0, SpawnKind::GarageForSale),
                spawn(54.0, SpawnKind::Garage),
                spawn(30.0, SpawnKind::Fuel),
            ],
            triggers: Vec::new(),
            curves: Vec::new(),
        };
        assert_eq!(prefab_places(&filling_station, false), [(Place::Fuel, [20.0, 0.0]), (Place::Garage, [52.0, 0.0])]);
    }

    #[test]
    fn a_fuel_company_is_one_whose_every_site_is_a_filling_station() {
        // Filling stations: two at TQG's open site (one 80 m off), one 120 m off (the next along), one alone.
        let stations = [(10, [30.0, 0.0]), (11, [0.0, 80.0]), (12, [-120.0, 0.0]), (13, [5000.0, 0.0])];
        let (tqg, ini, farm, mixed) = (1, 2, 3, 4);
        let sites = [
            (tqg, Site::Open([0.0, 0.0])),
            (ini, Site::Station(20)),
            (ini, Site::Station(21)),
            // A farm's custom depot with no filling station by it.
            (farm, Site::Open([2000.0, 0.0])),
            // A company with a filling station at one site and a depot at another: not a fuel company.
            (mixed, Site::Open([5000.0, 50.0])),
            (mixed, Site::Depot),
        ];
        let mut brands: Vec<(u64, Vec<u64>)> = brand_stations(&sites, &stations).into_iter().collect();
        brands.sort();
        assert_eq!(brands, [(tqg, vec![10, 11]), (ini, vec![20, 21])]);
    }

    #[test]
    fn a_station_two_companies_reach_is_the_nearer_sites_whatever_the_order() {
        // Station 10 between two companies' open sites, 30 m from Felleskjøpet's and 60 m from TQG's; station
        // 11 as far from both. Every order of the sites gives the same owners.
        let stations = [(10, [30.0, 0.0]), (11, [0.0, 50.0])];
        let (tqg, felleskjopet) = (7, 9);
        let mut sites = vec![(tqg, Site::Open([90.0, 0.0])), (felleskjopet, Site::Open([0.0, 0.0]))];
        let owners = |sites: &[(u64, Site)]| {
            let mut brands: Vec<(u64, Vec<u64>)> = brand_stations(sites, &stations).into_iter().collect();
            brands.sort();
            brands
        };
        // TQG keeps being a fuel company, with none of them.
        let expected = vec![(tqg, vec![]), (felleskjopet, vec![10, 11])];
        assert_eq!(owners(&sites), expected);
        sites.reverse();
        assert_eq!(owners(&sites), expected);
        // As near to both: the lower company's.
        let sites = [(tqg, Site::Open([0.0, 0.0])), (felleskjopet, Site::Open([0.0, 0.0]))];
        assert_eq!(owners(&sites), [(tqg, vec![10, 11]), (felleskjopet, vec![])]);
    }

    #[test]
    fn names_a_place_without_a_name_by_what_it_is_in_the_players_language() {
        let strings: Strings = [("mapl_recruit", "İş kurumu ofisi"), ("truckman_garage", "Garaj")]
            .map(|(k, v)| (k.into(), v.into()))
            .into_iter()
            .collect();
        assert_eq!(Place::Recruitment.generic_name(&strings).as_deref(), Some("İş kurumu ofisi"));
        assert_eq!(Place::Garage.generic_name(&strings).as_deref(), Some("Garaj"));
        // Not in this language: no name, the icon alone. Companies always have their own; hotels ours.
        assert_eq!(Place::Parking.generic_name(&strings), None);
        assert_eq!(Place::Hotel.generic_name(&strings).as_deref(), Some("Hotel"));
        assert_eq!(Place::Company.generic_name(&strings), None);
    }

    #[test]
    fn a_place_made_of_several_junctions_is_one_destination() {
        let place = |kind, name: Option<&str>, x: f32, node| Destination {
            kind,
            name: name.map(str::to_owned),
            localized: None,
            city: None,
            at: [x, 0.0],
            node,
            also: None,
        };
        let each = one_each(vec![
            // A filling station's two pump islands, 40 m apart: one.
            place("fuel", None, 0.0, 1),
            place("fuel", None, 40.0, 2),
            // Another station further on, a service beside the first, and two companies side by side: each.
            place("fuel", None, 900.0, 3),
            place("service", None, 20.0, 4),
            place("company", Some("Tradeaux"), 10.0, 5),
            place("company", Some("Polaris Lines"), 10.0, 6),
        ]);
        assert_eq!(each.iter().map(|d| d.node).collect::<Vec<_>>(), [1, 3, 4, 5, 6]);
    }

    #[test]
    fn routes_a_place_to_the_nearest_node_within_reach() {
        let mut grid = NodeGrid::default();
        grid.add(1, [10.0, 0.0]);
        grid.add(2, [-700.0, 0.0]);
        grid.add(3, [1200.0, 5.0]);
        assert_eq!(grid.nearest_such([0.0, 0.0], 500.0, &|_| true), Some(1));
        // Across cells, and nothing beyond reach.
        assert_eq!(grid.nearest_such([-600.0, 0.0], 500.0, &|_| true), Some(2));
        assert_eq!(grid.nearest_such([3000.0, 0.0], 500.0, &|_| true), None);
        // A junction's own ends: the nearest.
        assert_eq!(nearest_end(&[(7, [0.0, 40.0]), (8, [0.0, 5.0])], [0.0, 0.0]), Some(8));
        assert_eq!(nearest_end(&[], [0.0, 0.0]), None);
    }

    #[test]
    fn finds_the_city_a_garage_belongs_to() {
        let areas = [(1, [0.0, 0.0, 100.0, 100.0]), (2, [1000.0, 0.0, 1100.0, 100.0])];
        assert_eq!(city_of([50.0, 50.0], &areas), Some(1));
        // Just outside: the nearer city; far from any: none.
        assert_eq!(city_of([700.0, 50.0], &areas), Some(2));
        assert_eq!(city_of([50.0, 9000.0], &areas), None);
    }

    #[test]
    fn reads_toll_gates_border_crossings_and_weigh_stations_from_overlays() {
        use crate::sector::MapFlags;
        let overlay = |icon: &str, hidden: bool| ItemData::MapOverlay {
            flags: MapFlags { hidden, secret: false, dlc_guard: 0 },
            kind: 0,
            icon: token::string_to_token(icon).unwrap(),
            node: 0,
        };
        assert_eq!(Place::of_overlay(&overlay("toll_ico", false)), Some(Place::Toll));
        assert_eq!(Place::of_overlay(&overlay("border_ico", false)), Some(Place::Border));
        assert_eq!(Place::of_overlay(&overlay("weigh_ico", false)), Some(Place::WeighStation));
        // A road sign, or a hidden overlay: no place.
        assert_eq!(Place::of_overlay(&overlay("d_a1", false)), None);
        assert_eq!(Place::of_overlay(&overlay("toll_ico", true)), None);
    }

    #[test]
    fn rest_areas_marked_only_by_their_triggers_are_places_too() {
        // Either side of a motorway, 100 m apart: two, as the game's map shows them.
        assert_eq!(lone_rests(&[[0.0, 0.0], [100.0, 0.0]], &[]), [[0.0, 0.0], [100.0, 0.0]]);
        // A rest area's several triggers, 20 m apart: one.
        assert_eq!(lone_rests(&[[0.0, 0.0], [20.0, 0.0]], &[]), [[0.0, 0.0]]);
        // Beside a rest area's prefab: part of it. Across the motorway from one: another.
        let prefab = [(Place::Parking, [0.0, 0.0])];
        assert_eq!(lone_rests(&[[30.0, 0.0]], &prefab), Vec::<Point>::new());
        assert_eq!(lone_rests(&[[120.0, 0.0]], &prefab), [[120.0, 0.0]]);
        // A filling station beside it doesn't count as somewhere to rest.
        assert_eq!(lone_rests(&[[30.0, 0.0]], &[(Place::Fuel, [0.0, 0.0])]), [[30.0, 0.0]]);
    }

    #[test]
    fn rests_where_the_game_offers_rest_not_at_every_parking_space() {
        let rest = |x: f32| Trigger { position: [x, 0.0], action: token::string_to_token(REST_TRIGGER).unwrap(), links: [-1, -1] };
        let spawn = |x: f32, kind| SpawnPoint { position: [x, 0.0], kind };
        let prefab = |spawn_points, triggers| Prefab {
            nodes: Vec::new(),
            map_points: Vec::new(),
            spawn_points,
            triggers,
            curves: Vec::new(),
        };
        // A filling station with a resting area: both, the resting place in the middle of its area.
        let station = prefab(vec![spawn(30.0, SpawnKind::Fuel)], vec![rest(0.0), rest(10.0)]);
        assert_eq!(prefab_places(&station, false), [(Place::Fuel, [30.0, 0.0]), (Place::Parking, [5.0, 0.0])]);
        // A hotel's is a hotel.
        assert_eq!(prefab_places(&prefab(Vec::new(), vec![rest(4.0)]), true), [(Place::Hotel, [4.0, 0.0])]);
        // A garage's rest is the garage's.
        let garage = prefab(vec![spawn(0.0, SpawnKind::GarageForSale)], vec![rest(8.0)]);
        assert_eq!(prefab_places(&garage, false), [(Place::Garage, [0.0, 0.0])]);
        // A depot's parking spaces, with nowhere to rest: nothing.
        assert_eq!(prefab_places(&prefab(vec![spawn(0.0, SpawnKind::Parking)], Vec::new()), false), []);
        assert!(is_hotel("/prefab/hotel/hotel_a.ppd") && is_hotel("/prefab2/hotel_c_sc.ppd"));
        assert!(!is_hotel("/prefab2/gas_station/gas_hotel_like.ppd"));
    }
}
