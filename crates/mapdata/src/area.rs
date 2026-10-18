//! What the Maps app shows, from the game's map: roads (with junction interiors from their prefabs), map
//! areas, land (where the open sea isn't, [`crate::sea`]), painted water and place names, as vector tile
//! features ([`crate::tiles`]).
//!
//! The game's ground plane is metres (x east, z south). MapLibre works in degrees, so positions are
//! scaled as if the world lay on the equator, where a degree is the same distance both ways and Web
//! Mercator barely bends: [`to_lng_lat`]. The UI places the truck with the same scale.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque};

use crate::geometry::{self, Point, PrefabPlacement};
use crate::lanes;
use crate::locale::Strings;
use crate::prefab::{MapPoint, MapPointKind, Prefab, SpawnKind};
use crate::sea::{self, Cell, SeaGrid, SeaOptions};
use crate::sector::{ItemData, Node, Relief, TerrainSide};
use crate::signs;
use crate::terrain::{self, QuadGrid};
use crate::tiles::{Feature, Geometry, Mercator, Value};
use crate::token::{self, token_to_string};
use crate::world::{City, Definitions, LANE_WIDTH, Map, Prefabs, RoadClass};

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
    pub prefab_roads: usize,
    /// Roads tapering where lanes are added or dropped.
    pub lane_changes: usize,
    /// Roads handed over to the pieces that split them (a dual carriageway's way in and way out).
    pub splits: usize,
    /// Stretches of road drawn raised over the roads they pass over (bridges), by level (1 and up).
    pub raised: [usize; ROAD_LEVELS as usize - 1],
    /// The lines the roads and junction pieces were joined into, up close and further out together.
    pub road_lines: usize,
    /// Places marked with an icon (fuel, services, garages).
    pub places: usize,
    /// Places the player can be routed to (Maps' search), and those left out for want of a road to them.
    pub destinations: usize,
    pub unrouted: usize,
    /// Road number signs.
    pub signs: usize,
    pub prefab_areas: usize,
    pub labels: usize,
    pub missing_prefabs: usize,
    pub water: usize,
    /// Painted water partly hidden under land the game draws higher over it (pieces of it: a terrain side,
    /// a bezier patch).
    pub water_cut: usize,
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
}

pub struct Export {
    pub features: Vec<Feature>,
    pub labels: Vec<Label>,
    pub destinations: Vec<Destination>,
    pub counts: Counts,
    /// Every shown junction whose drawn road splits at a point inside it ([`CentreFork`]): its model's
    /// path and where (an audit of how junctions are drawn).
    pub centre_forks: Vec<(String, Point)>,
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
/// Junctions' own road segments: too small to see further out, and there are many of them.
const JUNCTION_ZOOM: u8 = 11;

fn road_zoom(class: RoadClass) -> u8 {
    match class {
        RoadClass::Motorway => 6,
        RoadClass::Trunk => 8,
        RoadClass::Local => 11,
        RoadClass::Rail => 12,
        RoadClass::Minor => 13,
    }
}

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
#[allow(clippy::too_many_arguments)]
pub fn export(
    map: &Map,
    definitions: &Definitions,
    strings: &Strings,
    prefabs: &mut Prefabs,
    signs: &BTreeSet<String>,
    bounds: Bounds,
    options: SeaOptions,
    progress: &mut dyn FnMut(f32),
) -> Export {
    let [lanes_done, painted_done, classes_done, items_done, water_done, sea_done, land_done, roads_done] =
        DRAWING_STEPS;
    let mut features = Vec::new();
    let mut roads = Vec::new();
    let mut counts = Counts::default();
    let mut centre_forks = Vec::new();
    // Where each road look's lanes lie, from the prefabs' lanes (lanes.rs): off its line, or as two
    // carriageways.
    let measured = lanes::road_offsets(map, prefabs);
    progress(lanes_done);
    // City token → its largest area and that area's centre.
    let mut cities: HashMap<u64, (f32, Point)> = HashMap::new();
    // Ferry port or train terminal token → whether it's a train terminal, and its docks' positions.
    let mut ferries: BTreeMap<u64, (bool, Vec<Point>)> = BTreeMap::new();
    // The places overlays mark, and those prefabs do (an overlay may mark one a prefab already has).
    let mut overlay_places: Vec<(Place, Point)> = Vec::new();
    let mut prefab_places_at: Vec<(Place, Point)> = Vec::new();
    // Where the player can be routed: each place as found (its kind, name, where, and the junction it's
    // part of), routed once every junction's joined ends and every road are known.
    let mut to_route: Vec<(Place, Option<String>, Point, Option<u64>)> = Vec::new();
    let mut joined_ends: HashMap<u64, Vec<(u64, Point)>> = HashMap::new();
    let mut road_ends = NodeGrid::default();
    // Land goes straight onto the sea's grid (a whole map's terrain is millions of quads). Painted water is
    // read first, and the land over it kept as the land is read: the water is painted on the grid and drawn
    // once all land is there, where no land hides it.
    let mut grid = SeaGrid::new(bounds.0, options.cell);
    let mut water = Water::new(painted_water(map, definitions, &bounds));
    progress(painted_done);

    // The class of every road, for the junctions they lead into.
    let road_classes: HashMap<u64, RoadClass> = map
        .items
        .iter()
        .filter_map(|item| match &item.data {
            ItemData::Road { look, .. } => Some((item.uid, definitions.road_looks.get(look)?.class)),
            _ => None,
        })
        .collect();
    let point_classes = prefab_point_classes(map, prefabs, &road_classes);
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
    let share = |index: usize| index as f32 / map.items.len().max(1) as f32;
    for (index, item) in map.items.iter().enumerate().filter(|(_, item)| bounds.overlaps(&item.bounds)) {
        if index % every == 0 {
            progress(classes_done + (items_done - classes_done) * share(index));
        }
        match &item.data {
            ItemData::Terrain { start_node, end_node, right, left, relief } => {
                let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else {
                    continue;
                };
                for (side, relief, sign) in [(right, &relief[0], 1.0), (left, &relief[1], -1.0)] {
                    ground(&mut grid, &mut water, strip_ground(definitions, start, end, 0.0, side, relief, sign));
                }
            }
            ItemData::BezierPatch { node, control_points, grid: quads } => {
                let Some(node) = map.nodes.get(node) else { continue };
                ground(&mut grid, &mut water, patch_ground(definitions, node, control_points, quads));
            }
            ItemData::Road { flags, look, start_node, end_node, relief } => {
                let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else {
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
                let look_token = *look;
                // On its lanes, which for some looks lie off its line or apart, as two carriageways (lanes.rs).
                let lines = drawn_lines(measured.get(look), definitions.road_looks.get(look));
                let look = definitions.road_looks.get(look);
                let width = look.map_or(8.0, |look| look.width() + look.offset);
                let line = geometry::road_curve(start, end);
                let middle = measured.get(&look_token).map_or(0.0, |offset| offset.trusted());
                let curve = geometry::offset_line(&line, middle);
                for ring in thick_line(&curve, width) {
                    grid.paint(&ring, Cell::Land);
                }
                // On the ground (land beside it, in its terrain), its surface hides the painted water under it,
                // between its terrain either side (which starts half its width from its line). Not a bridge's:
                // with no land beside it, the water under it shows.
                let terrain = map.road_terrain.get(&item.uid);
                if terrain.is_some_and(|terrain| {
                    has_land(definitions, &terrain.right) || has_land(definitions, &terrain.left)
                }) {
                    water.surface(&line, &geometry::road_heights(start, end), width);
                }
                if let Some(terrain) = map.road_terrain.get(&item.uid) {
                    // The road's terrain starts at its edge.
                    for (side, relief, sign) in [(&terrain.right, &relief[0], 1.0), (&terrain.left, &relief[1], -1.0)] {
                        let quads = strip_ground(definitions, start, end, width / 2.0, side, relief, sign);
                        ground(&mut grid, &mut water, quads);
                    }
                }
                if let Some(look) = look
                    && !flags.hidden
                    && !flags.secret
                {
                    let class = look.class;
                    let heights = geometry::road_heights(start, end);
                    for &(shift, width) in &lines {
                        let points = geometry::offset_line(&line, shift);
                        let heights = heights.clone();
                        roads.push(RoadLine { points, heights, class, width, minzoom: road_zoom(class), level: 0 });
                    }
                    counts.roads += 1;
                }
            }
            ItemData::Prefab { flags, model, nodes, origin, .. } => {
                let Some(prefab) = prefabs.get(*model) else {
                    counts.missing_prefabs += 1;
                    continue;
                };
                let Some(placement) = place_prefab(map, prefab, nodes, *origin as usize) else { continue };
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
                if !ends.is_empty() {
                    joined_ends.insert(item.uid, ends);
                }
                let classes = point_classes.get(&item.uid).map_or(&[][..], Vec::as_slice);
                // Which of its ends something joins (a road, another prefab), by prefab node.
                let joined: Vec<bool> = (0..prefab.nodes.len())
                    .map(|end| {
                        let world = geometry::prefab_world_node(end, *origin as usize, prefab.nodes.len());
                        let node = nodes.get(world).and_then(|uid| map.nodes.get(uid));
                        node.is_some_and(|node| node.forward_item != 0 && node.backward_item != 0)
                    })
                    .collect();
                // Under the roads, whatever the prefab says: its "over roads" areas (a filling station's
                // forecourt) would hide the roads through them, which run on to the pumps on our map.
                let mut areas: Vec<(Vec<Point>, Vec<Point>)> = Vec::new();
                for (own, color) in prefab_areas(prefab) {
                    let ring: Vec<Point> = own.iter().map(|&p| placement.place(p)).collect();
                    areas.push((own, ring.clone()));
                    grid.paint(&ring, Cell::Land);
                    if shown {
                        features.push(area_feature(&ring, color, false));
                        counts.prefab_areas += 1;
                    }
                }
                if shown {
                    let path = definitions.prefab_paths.get(model).map_or("", String::as_str);
                    for (place, at) in prefab_places(prefab, is_hotel(path)) {
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
                if shown {
                    let path = definitions.prefab_paths.get(model).cloned().unwrap_or_default();
                    for at in prefab_centre_forks(prefab) {
                        centre_forks.push((path.clone(), placement.place(at)));
                    }
                }
                // How high each of its ends is where it's placed, by prefab node: a junction's roads are as
                // high as the ends they run between (a template junction bends to the ground). Unless its
                // lanes lie at different heights in the prefab itself (a road over another inside it, slip
                // roads climbing between them): then as high as the lane they run along, lifted into the world
                // as its ends are. (Its map points' own heights are no guide: in many they're all 0.)
                let end_heights: Vec<Option<f32>> = (0..prefab.nodes.len())
                    .map(|own| {
                        let world = geometry::prefab_world_node(own, *origin as usize, prefab.nodes.len());
                        nodes.get(world).and_then(|uid| map.nodes.get(uid)).map(|node| node.position[1])
                    })
                    .collect();
                let known: Vec<(Point, f32)> = prefab
                    .nodes
                    .iter()
                    .zip(&end_heights)
                    .filter_map(|(end, &height)| Some((end.position, height?)))
                    .collect();
                let lanes = lanes_by_height(prefab);
                let lifts: Vec<(Point, f32)> = prefab
                    .nodes
                    .iter()
                    .zip(&end_heights)
                    .filter_map(|(end, &height)| Some((end.position, height? - end.height)))
                    .collect();
                // Its ground: round its roads (a crossing's corners between its arms) and its areas (a depot's
                // yards), at their heights.
                let mut ground_points: Vec<Vertex> = areas
                    .iter()
                    .flat_map(|(own, ring)| {
                        own.iter().zip(ring).map(|(&own, &at)| [at[0], blended(&known, own), at[1]])
                    })
                    .collect();
                for (line, ends, class, width) in prefab_road_lines(prefab, classes, &joined) {
                    // Where it ends inside, as high as the ends around it.
                    let height_at = |end: Option<usize>, at: Point| {
                        end.and_then(|end| end_heights.get(end).copied().flatten())
                            .unwrap_or_else(|| blended(&known, at))
                    };
                    let (first, last) = (line[0], line[line.len() - 1]);
                    let mut heights = climbing(&line, height_at(ends[0], first), height_at(ends[1], last));
                    if let Some(lanes) = &lanes {
                        for ((height, &at), lane) in heights.iter_mut().zip(&line).zip(lane_heights(lanes, &line)) {
                            if let Some(lane) = lane {
                                *height = lane + blended(&lifts, at);
                            }
                        }
                    }
                    let line: Vec<Point> = line.iter().map(|&p| placement.place(p)).collect();
                    for ring in thick_line(&line, width) {
                        grid.paint(&ring, Cell::Land);
                    }
                    ground_points.extend(road_edges(&line, &heights, width));
                    if shown {
                        roads.push(RoadLine {
                            points: line,
                            heights,
                            class,
                            width,
                            minzoom: road_zoom(class).max(JUNCTION_ZOOM),
                            level: 0,
                        });
                        counts.prefab_roads += 1;
                    }
                }
                water.ground(&ground_points);
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

    // The sea under the whole area, and the land over it; then the painted water. (The sea comes with each
    // tile rather than as the page's background, so a tile still loading shows as plain ground, not as
    // open sea.)
    progress(items_done);
    let shown = water.shown(&mut |done| progress(items_done + (water_done - items_done) * done));
    for shape in &shown {
        grid.paint_rings(shape, Cell::Water);
    }
    let water_area = [water.area(), shown.iter().map(|shape| shape_area(shape)).sum()];
    counts.water_cut = water.cut();
    progress(water_done);
    grid.find_sea_telling(options, &mut |done| progress(water_done + (sea_done - water_done) * done));
    let [x0, z0, x1, z1] = grid.traced_bounds();
    let whole_area = vec![vec![[x0, z0], [x1, z0], [x1, z1], [x0, z1]]];
    features.push(polygon_feature("sea", LAND_ZOOM, &whole_area, Vec::new()));
    progress(sea_done);
    let land = sea::land_shapes_telling(&grid, COAST_TOLERANCE, &mut |done| {
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
    let roads = taper_lane_changes(hand_over_splits(roads, &mut counts.splits), &mut counts.lane_changes);
    let roads = road_levels(roads, &mut counts.raised);
    let road_features = road_features(roads);
    counts.road_lines = road_features.len();
    features.extend(road_features);
    // Every place routed to a joined end of its junction, the nearest; else (a port, a junction with none) to
    // the nearest end of a road nearby. A city to the road end nearest its centre.
    progress(roads_done);
    let mut destinations = Vec::new();
    for (place, name, at, junction) in to_route {
        let node = junction
            .and_then(|uid| joined_ends.get(&uid))
            .and_then(|ends| nearest_end(ends, at))
            .or_else(|| road_ends.nearest(at, PLACE_ROAD_REACH));
        match node {
            Some(node) => destinations.push(Destination {
                kind: place.name(),
                name,
                localized: None,
                city: city_of(at, &city_areas),
                at,
                node,
            }),
            None => counts.unrouted += 1,
        }
    }
    for (city, label) in &routed_cities {
        match road_ends.nearest(label.at, CITY_ROAD_REACH) {
            Some(node) => destinations.push(Destination {
                kind: "city",
                name: Some(label.name.clone()),
                localized: label.localized.clone(),
                city: Some(*city),
                at: label.at,
                node,
            }),
            None => counts.unrouted += 1,
        }
    }
    let destinations = one_each(destinations);
    progress(1.0);
    counts.destinations = destinations.len();
    Export { features, labels, destinations, counts, centre_forks, water_area, sea: grid }
}

/// A point with its height: x, height (the game's y), z.
type Vertex = [f32; 3];

/// One piece of painted water (a terrain side's, a bezier patch's), as the game paints it: its quads with
/// their heights, and its shapes, each an outline and its holes (a river bank down a whole strip is one
/// shape, not one per column of quads).
struct WaterPiece {
    quads: Vec<[Vertex; 4]>,
    shapes: Vec<Vec<Vec<Point>>>,
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
    /// The pieces by the cells ([`WATER_CELL`]) their quads reach.
    cells: HashMap<(i32, i32), Vec<u32>>,
    /// The land quads over each piece, rising above its lowest water somewhere.
    covers: Vec<Vec<[Vertex; 4]>>,
}

/// The cells painted water is looked up in, metres.
const WATER_CELL: f32 = 64.0;

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
        Self { pieces, lowest, cells, covers }
    }

    /// A land quad read: kept for each piece of water it may lie over, higher than its water.
    fn land(&mut self, quad: &[Vertex; 4]) {
        let top = quad.iter().map(|v| v[1]).fold(f32::MIN, f32::max);
        let mut under: Vec<u32> = cells_of(quad, WATER_CELL)
            .filter_map(|cell| self.cells.get(&cell))
            .flatten()
            .copied()
            .filter(|&piece| top > self.lowest[piece as usize] + LAND_OVER_WATER)
            .collect();
        under.sort_unstable();
        under.dedup();
        for piece in under {
            self.covers[piece as usize].push(*quad);
        }
    }

    /// A road's surface read (`line` with its `heights`, `width` wide): over the water as land.
    fn surface(&mut self, line: &[Point], heights: &[f32], width: f32) {
        let half = width / 2.0;
        for i in 0..line.len().saturating_sub(1).min(heights.len().saturating_sub(1)) {
            let (a, b) = (line[i], line[i + 1]);
            let Some(way) = direction(a, b) else { continue };
            let across = [-way[1] * half, way[0] * half];
            let (ha, hb) = (heights[i], heights[i + 1]);
            self.land(&[
                [a[0] + across[0], ha, a[1] + across[1]],
                [b[0] + across[0], hb, b[1] + across[1]],
                [b[0] - across[0], hb, b[1] - across[1]],
                [a[0] - across[0], ha, a[1] - across[1]],
            ]);
        }
    }

    /// A junction's ground read, as the points round its roads (`points`, with their heights): the ground
    /// within them (their convex hull) over the water as land.
    fn ground(&mut self, points: &[Vertex]) {
        let hull = convex_hull(points);
        for i in 1..hull.len().saturating_sub(1) {
            self.land(&[hull[0], hull[i], hull[i + 1], hull[i + 1]]);
        }
    }

    /// Every piece's shapes as they show (each an outline and its holes), the pieces looked at in parallel;
    /// `progress` told the share of pieces done, 0 to 1, until every worker has ended.
    fn shown(&self, progress: &mut dyn FnMut(f32)) -> Vec<Vec<Vec<Point>>> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let next = AtomicUsize::new(0);
        let finished = AtomicUsize::new(0);
        let total = self.pieces.len().max(1);
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        let mut done: Vec<(usize, Vec<Vec<Vec<Point>>>)> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        let mut done = Vec::new();
                        loop {
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            let Some(piece) = self.pieces.get(index) else { break };
                            done.push((index, shown_water(piece, &self.covers[index])));
                            finished.fetch_add(1, Ordering::Relaxed);
                        }
                        done
                    })
                })
                .collect();
            while !workers.iter().all(|worker| worker.is_finished()) {
                progress(finished.load(Ordering::Relaxed) as f32 / total as f32);
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            workers.into_iter().flat_map(|worker| worker.join().expect("a water worker")).collect()
        });
        // In the pieces' order, so the export is reproducible.
        done.sort_by_key(|(index, _)| *index);
        done.into_iter().flat_map(|(_, shapes)| shapes).collect()
    }

    /// How many pieces land lies over.
    fn cut(&self) -> usize {
        self.covers.iter().filter(|covers| !covers.is_empty()).count()
    }

    /// The painted water's whole area, m².
    fn area(&self) -> f32 {
        self.pieces.iter().flat_map(|piece| &piece.shapes).map(|shape| shape_area(shape)).sum()
    }
}

/// Whether a terrain side has any land (quads not painted water).
fn has_land(definitions: &Definitions, side: &TerrainSide) -> bool {
    let grid = &side.grid;
    (0..grid.cols).any(|col| {
        (0..grid.rows).any(|row| !grid.material_at(row, col).is_some_and(|m| definitions.water_materials.contains(&m)))
    })
}

/// The points on either edge of a road (`line`, `width` wide) beside each of its points, each as high as
/// the road there.
fn road_edges<'a>(line: &'a [Point], heights: &'a [f32], width: f32) -> impl Iterator<Item = Vertex> + 'a {
    let half = width / 2.0;
    (0..line.len().min(heights.len())).flat_map(move |i| {
        let way = direction(line[i.saturating_sub(1)], line[(i + 1).min(line.len() - 1)]).unwrap_or([1.0, 0.0]);
        let (p, height) = (line[i], heights[i]);
        [[p[0] - way[1] * half, height, p[1] + way[0] * half], [p[0] + way[1] * half, height, p[1] - way[0] * half]]
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
fn shown_water(piece: &WaterPiece, covers: &[[Vertex; 4]]) -> Vec<Vec<Vec<Point>>> {
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
fn painted_water(map: &Map, definitions: &Definitions, bounds: &Bounds) -> Vec<WaterPiece> {
    let has_water = |grid: &QuadGrid| {
        (0..grid.cols).any(|col| {
            (0..grid.rows)
                .any(|row| grid.material_at(row, col).is_some_and(|m| definitions.water_materials.contains(&m)))
        })
    };
    let mut pieces = Vec::new();
    for item in map.items.iter().filter(|item| bounds.overlaps(&item.bounds)) {
        match &item.data {
            ItemData::Terrain { start_node, end_node, right, left, relief } => {
                let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else { continue };
                for (side, relief, sign) in [(right, &relief[0], 1.0), (left, &relief[1], -1.0)] {
                    if has_water(&side.grid) {
                        pieces.extend(strip_ground(definitions, start, end, 0.0, side, relief, sign).water);
                    }
                }
            }
            ItemData::BezierPatch { node, control_points, grid } => {
                let Some(node) = map.nodes.get(node) else { continue };
                if has_water(grid) {
                    pieces.extend(patch_ground(definitions, node, control_points, grid).water);
                }
            }
            ItemData::Road { look, start_node, end_node, relief, .. } => {
                let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else { continue };
                let Some(terrain) = map.road_terrain.get(&item.uid) else { continue };
                let width = definitions.road_looks.get(look).map_or(8.0, |look| look.width() + look.offset);
                for (side, relief, sign) in [(&terrain.right, &relief[0], 1.0), (&terrain.left, &relief[1], -1.0)] {
                    if has_water(&side.grid) {
                        pieces.extend(strip_ground(definitions, start, end, width / 2.0, side, relief, sign).water);
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

/// Terrain's land onto the sea's grid, and kept over the painted water it lies over. (Its water was read
/// before: [`painted_water`].)
fn ground(grid: &mut SeaGrid, water: &mut Water, terrain: TerrainGround) {
    for ring in &terrain.land_runs {
        grid.paint(ring, Cell::Land);
    }
    for quad in &terrain.land {
        water.land(quad);
    }
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
/// [`STATION_REACH`] of it; with those stations.
fn brand_stations(sites: &[(u64, Site)], stations: &[(u64, Point)]) -> HashMap<u64, Vec<u64>> {
    let mut brands: HashMap<u64, Option<Vec<u64>>> = HashMap::new();
    for &(company, site) in sites {
        let found = match site {
            Site::Station(prefab) => vec![prefab],
            Site::Open(at) => stations
                .iter()
                .filter(|(_, middle)| (middle[0] - at[0]).hypot(middle[1] - at[1]) <= STATION_REACH)
                .map(|&(prefab, _)| prefab)
                .collect(),
            Site::Depot => Vec::new(),
        };
        let owned = brands.entry(company).or_insert_with(|| Some(Vec::new()));
        match (found.is_empty(), owned.as_mut()) {
            (false, Some(owned)) => owned.extend(found),
            _ => *owned = None,
        }
    }
    brands.into_iter().filter_map(|(company, owned)| Some((company, owned?))).collect()
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

    /// The node nearest `at`, within `reach` metres.
    fn nearest(&self, at: Point, reach: f32) -> Option<u64> {
        let (x, z) = Self::cell(at);
        let cells = (reach / NODE_CELL).ceil() as i32;
        (x - cells..=x + cells)
            .flat_map(|x| (z - cells..=z + cells).map(move |z| (x, z)))
            .filter_map(|cell| self.cells.get(&cell))
            .flatten()
            .map(|&(node, there)| (node, distance(at, there)))
            .filter(|&(_, d)| d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(node, _)| node)
    }
}

/// How near two marks of one kind of place may be and be the same place, metres.
const SAME_PLACE: f32 = 200.0;

/// The trigger action where the game offers the player rest.
const REST_TRIGGER: &str = "hud_parking";

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

/// A road's centre line, or a junction's piece of one, before they're joined into longer lines.
struct RoadLine {
    points: Vec<Point>,
    /// How high each point is, metres (the game's y).
    heights: Vec<f32>,
    class: RoadClass,
    width: f32,
    minzoom: u8,
    /// The level it's drawn on: 0 on the ground, one above any road it passes over ([`road_levels`]).
    level: u8,
}

/// How much wider than a road the line it runs on into may be for the road to taper into it, metres: a
/// lane or two added or dropped (more is a junction, not a lane change).
const TAPER_MAX: f32 = 2.0 * LANE_WIDTH;
/// A tapering road's width steps, metres, and the most ground one step covers (its line is cut finer
/// where its points are further apart).
const TAPER_STEP: f32 = 0.25;
const TAPER_SPACING: f32 = 2.0;
/// Ends this close along the road are at the same node, metres (a junction's piece ends on its node, give
/// or take rounding; side by side across it, they're on their own lanes there).
const SAME_END: f32 = 0.05;
/// How far a narrower line may stick out past the wider one it runs on into, at their shared node, and
/// still be taken to be on its lanes, metres: the game's lanes are half a metre wider than the map's, and
/// lanes' middles are measured to a quarter metre (lanes.rs).
const WITHIN_SLACK: f32 = 0.75;
/// How far apart across the road a line and the one beside it it runs on into may be, metres (edge to
/// edge): lanes side by side, the game's a little wider than the map's. A motorway's other carriageway lies
/// further, past its central reservation (lanes.rs's `RESERVATION`).
const BESIDE: f32 = 1.5;
/// How far apart along the road lines' ends may be and still be level across their node, metres.
const LEVEL: f32 = 0.2;
/// Lines carrying a road on from its end this close across it start from the same place, metres (less than
/// half a lane): a junction's pieces fanning out, not lanes going their own ways side by side.
const SAME_START: f32 = 1.0;
/// A line carries a road straight on where they leave their shared end within about 37° of opposite ways
/// (the cosine; measured a few metres in, so a curving piece reads a little sharper than it is): a lane
/// branching off there (a filling station's forecourt) doesn't.
const STRAIGHT_ON: f32 = 0.8;

/// Lanes added and dropped gradually, as in the game: where a road runs on from a wider one (a motorway's
/// extra lane after a filling station's slip road joins it, ending in the game's lane-closing prefab; a
/// dual carriageway's two lanes each way merging into one before a roundabout), the lanes it doesn't have
/// carry on into it and narrow away by its other end, rather than stopping dead at the node. And the same
/// where a lane opens: the road widens towards the wider one ahead.
///
/// The two meet at their node, but each is drawn on its own lanes' middle (lanes.rs), so where the lane
/// comes or goes on one side, their ends lie apart across the node: the road then also moves over to the
/// wider one's middle as it widens, keeping the edge that carries on straight. Roads and junctions' pieces
/// alike, but only where the wider one is the one line carrying the road straight on, within its width,
/// and nothing carries the wider one on beside the road (a junction's crossing roads, a lane branching
/// off, or a road forking into two side by side are no lane change).
///
/// And lanes joining a road from beside it, or leaving it: a line running straight on into one line it
/// lies beside or sticks out of (a slip road's lane closing into the motorway; three lanes, drawn as two
/// lines, closing into two lanes between them) moves over gradually to lie within it by the node, rather
/// than ending beside it; but only where lanes do close there (more lanes on its side of the node than
/// the line carrying them on has: not a motorway's carriageways meeting a junction's piece drawn as one,
/// where the motorway runs on straight).
fn taper_lane_changes(roads: Vec<RoadLine>, count: &mut usize) -> Vec<RoadLine> {
    // Each line's ends: whether it's its start, and the end and the way the line leaves it.
    let ends_of = |road: &RoadLine| {
        let reversed: Vec<Point> = road.points.iter().rev().copied().collect();
        [(true, line_end(&road.points)), (false, line_end(&reversed))]
    };
    let cell = |p: Point| ((p[0] / TAPER_CELL).floor() as i32, (p[1] / TAPER_CELL).floor() as i32);
    type Ends = HashMap<(i32, i32), Vec<(usize, bool, Point, Point)>>;
    let mut ends: Ends = HashMap::new();
    for (index, road) in roads.iter().enumerate() {
        for (start, end) in ends_of(road) {
            if let Some((at, away)) = end {
                ends.entry(cell(at)).or_default().push((index, start, at, away));
            }
        }
    }
    // The lines carrying a line's end `at` (leaving along `away`) straight on: level with it along the
    // road, and over it or beside it across it. Each with whether it's its start, how far its end lies to
    // the right of `at` facing `away`, and whether one lies within the other.
    let carrying = |index: usize, at: Point, away: Point| -> Vec<(usize, bool, f32, bool)> {
        let (x, z) = cell(at);
        let own = roads[index].width;
        let mut on: Vec<(usize, bool, f32, bool)> = (x - 1..=x + 1)
            .flat_map(|x| (z - 1..=z + 1).map(move |z| (x, z)))
            .filter_map(|key| ends.get(&key))
            .flatten()
            .filter_map(|&(other, other_start, other_at, other_away)| {
                let offset = [other_at[0] - at[0], other_at[1] - at[1]];
                let along = offset[0] * away[0] + offset[1] * away[1];
                let right = offset[1] * away[0] - offset[0] * away[1];
                let width = roads[other].width;
                let (narrow, wide) = (own.min(width), own.max(width));
                (other != index
                    && along.abs() < LEVEL
                    && right.abs() - (own + width) / 2.0 <= BESIDE
                    && other_away[0] * away[0] + other_away[1] * away[1] < -STRAIGHT_ON)
                    .then_some((other, other_start, right, right.abs() + narrow / 2.0 <= wide / 2.0 + WITHIN_SLACK))
            })
            .collect();
        on.sort_by_key(|&(other, other_start, _, _)| (other, other_start));
        on.dedup_by_key(|&mut (other, other_start, _, _)| (other, other_start));
        on
    };
    // What a line meets at an end, as a width and how far right of the line's own way its middle lies: the
    // line carrying it on there (the one it's within, or else the one beside it).
    // - Wider by a lane or two, and carried on by nothing else beside it: its width and middle (a lane
    //   added or dropped).
    // - Otherwise its own width, but moved over to lie within it (lanes merging into it from beside it, or
    //   branching off it; a motorway's carriageways meeting a road drawn as one).
    // Its own width and middle where nothing, or more than one line, carries it on.
    let met = |index: usize, start: bool, end: Option<(Point, Point)>| -> (f32, f32) {
        let own = roads[index].width;
        let Some((at, away)) = end else { return (own, 0.0) };
        let on = carrying(index, at, away);
        let within: Vec<(usize, bool, f32, bool)> = on.iter().copied().filter(|line| line.3).collect();
        let (other, other_start, right, is_within) = match (&within[..], &on[..]) {
            ([only], _) | ([], [only]) => *only,
            _ => return (own, 0.0),
        };
        let wider = roads[other].width;
        if wider < own {
            return (own, 0.0);
        }
        // `away` runs into the line from its end: along it from its start, against it from its end.
        let towards = |shift: f32| {
            let shift = if start { shift } else { -shift };
            if shift.abs() < SAME_END { 0.0 } else { shift }
        };
        // The lines carrying the other one on, this one's side of their node.
        let Some((other_at, other_away)) = ends_of(&roads[other])[if other_start { 0 } else { 1 }].1 else {
            return (own, 0.0);
        };
        let back = carrying(other, other_at, other_away);
        let Some(&(_, _, here, _)) = back.iter().find(|line| line.0 == index) else { return (own, 0.0) };
        // Lines carrying the wider one on side by side (a road forking into two, each on its own lanes),
        // not just this one where it is (a junction's pieces fanning out from one point, all of them
        // tapering), make it no lane change.
        let side_by_side = back.iter().any(|&(_, _, right, _)| (right - here).abs() > SAME_START);
        if wider > own && wider - own <= TAPER_MAX && !side_by_side {
            return (wider, towards(right));
        }
        // Already on its lanes within it; or beside it with no lane closing (a motorway's carriageways on
        // either side of a junction's piece drawn as one: the motorway runs on straight, the piece is
        // where it's off); or where it lies from its middle, and the nearest place it would lie within it.
        let lanes: f32 = back.iter().map(|line| roads[line.0].width).sum();
        if is_within || lanes <= wider + WITHIN_SLACK {
            return (own, 0.0);
        }
        let room = (wider - own) / 2.0;
        let inside = (-right).clamp(-room, room);
        (own, towards(right + inside))
    };
    // Each line's width and shift at its start and end, where it changes.
    type Change = Option<((f32, f32), (f32, f32))>;
    let changes: Vec<Change> = roads
        .iter()
        .enumerate()
        .map(|(index, road)| {
            let [(_, first), (_, last)] = ends_of(road);
            let (start, end) = (met(index, true, first), met(index, false, last));
            (start != (road.width, 0.0) || end != (road.width, 0.0)).then_some((start, end))
        })
        .collect();
    *count += changes.iter().flatten().count();
    roads
        .into_iter()
        .zip(changes)
        .flat_map(|(road, change)| match change {
            Some((start, end)) => tapered(road, start, end),
            None => vec![road],
        })
        .collect()
}

/// The cells line ends are looked up in for [`taper_lane_changes`], metres: about as far as a line's middle
/// may lie from one beside it.
const TAPER_CELL: f32 = 16.0;

/// Where a road splits (a dual carriageway into a roundabout's way in and way out: a junction's pieces
/// starting side by side across the road's end, [`prefab_roads`]), the road hands over to them cleanly:
/// it stops half its width short of its end and they start that far back, side by side inside it. Drawn
/// with round ends, the road would otherwise bulge out between them, as a blob where the road divides.
fn hand_over_splits(mut roads: Vec<RoadLine>, count: &mut usize) -> Vec<RoadLine> {
    let cell = |p: Point| ((p[0] / SPLIT_CELL).floor() as i32, (p[1] / SPLIT_CELL).floor() as i32);
    // Each line's ends, by the cell they're in: the line, whether it's its start, the end and the way the
    // line leaves it.
    type Ends = HashMap<(i32, i32), Vec<(usize, bool, Point, Point)>>;
    let mut ends: Ends = HashMap::new();
    for (index, road) in roads.iter().enumerate() {
        let reversed: Vec<Point> = road.points.iter().rev().copied().collect();
        for (start, end) in [(true, line_end(&road.points)), (false, line_end(&reversed))] {
            if let Some((at, away)) = end {
                ends.entry(cell(at)).or_default().push((index, start, at, away));
            }
        }
    }
    // For each road end that splits (the road, and whether it's its start): how far the road stops short,
    // and the pieces carrying it on (each, and whether it's its start that's there).
    type Handover = (usize, bool, f32, Vec<(usize, bool)>);
    let mut handovers: Vec<Handover> = Vec::new();
    for (index, road) in roads.iter().enumerate() {
        let reversed: Vec<Point> = road.points.iter().rev().copied().collect();
        for (start, end) in [(true, line_end(&road.points)), (false, line_end(&reversed))] {
            let Some((at, into)) = end else { continue };
            let half = road.width / 2.0;
            let (x, z) = cell(at);
            // Lines starting across this end (along it, within the road's width), carrying it straight on.
            let mut on: Vec<(usize, bool, f32)> = (x - 1..=x + 1)
                .flat_map(|x| (z - 1..=z + 1).map(move |z| (x, z)))
                .filter_map(|key| ends.get(&key))
                .flatten()
                .filter(|&&(other, _, other_at, away)| {
                    let offset = [other_at[0] - at[0], other_at[1] - at[1]];
                    let along = offset[0] * into[0] + offset[1] * into[1];
                    let across = offset[0] * into[1] - offset[1] * into[0];
                    other != index
                        && along.abs() < SPLIT_ALONG
                        && across.abs() <= half
                        && away[0] * into[0] + away[1] * into[1] < -SPLIT_ONWARD
                })
                .map(|&(other, other_start, _, _)| (other, other_start, roads[other].width))
                .collect();
            on.sort_by_key(|&(other, other_start, _)| (other, other_start));
            on.dedup_by_key(|&mut (other, other_start, _)| (other, other_start));
            let shared: f32 = on.iter().map(|&(_, _, width)| width).sum();
            if on.len() < 2 || shared > road.width + SHARED_ROAD_SLACK {
                continue;
            }
            // Never more than a third of a short road.
            let back = half.min(line_length(&road.points) / 3.0);
            handovers.push((
                index,
                start,
                back,
                on.iter().map(|&(other, other_start, _)| (other, other_start)).collect(),
            ));
        }
    }
    for (index, start, back, pieces) in handovers {
        let (end, into) = {
            let points = &roads[index].points;
            let reversed: Vec<Point> = points.iter().rev().copied().collect();
            let end = if start { line_end(points) } else { line_end(&reversed) };
            let Some(end) = end else { continue };
            end
        };
        let half = roads[index].width / 2.0;
        // Each piece starts `back` further into the road, along it. And inside it: where its lanes lie
        // wider than the road is drawn (a two-way road dividing into a motorway's carriageways, its
        // halves on their lanes past its edges), it starts within the road's edge and eases out onto its
        // lanes, over as far again past the node (no further than half its length), without a corner.
        for (piece, piece_start) in pieces {
            let room = (half - roads[piece].width / 2.0).max(0.0);
            let line = &mut roads[piece];
            if !piece_start {
                line.points.reverse();
                line.heights.reverse();
            }
            let at = line.points[0];
            let lies = (at[0] - end[0]) * into[1] - (at[1] - end[1]) * into[0];
            let inside = lies.clamp(-room, room);
            line.points.insert(0, [at[0] + into[0] * back, at[1] + into[1] * back]);
            line.heights.insert(0, line.heights[0]);
            if inside != lies {
                let length = (2.0 * back).min(back + line_length(&line.points[1..]) / 2.0);
                (line.points, line.heights) = eased_out(&line.points, &line.heights, inside - lies, length);
            }
            if !piece_start {
                line.points.reverse();
                line.heights.reverse();
            }
        }
        let road = &mut roads[index];
        if !start {
            road.points.reverse();
            road.heights.reverse();
        }
        (road.points, road.heights) = cut_start(&road.points, &road.heights, back);
        if !start {
            road.points.reverse();
            road.heights.reverse();
        }
        *count += 1;
    }
    roads
}

/// `points` (with their `heights`) with their first `length` metres moved `shift` metres to the right of
/// the way they run: all of it at their start, easing smoothly to none by `length` (no corner at either
/// end), cut finer along there.
fn eased_out(points: &[Point], heights: &[f32], shift: f32, length: f32) -> (Vec<Point>, Vec<f32>) {
    // A point at least every metre over the eased stretch.
    let (line, heights, along) =
        cut_finer(points, heights, |from, step| if from < length { step.ceil() as usize } else { 1 });
    let shifts: Vec<f32> = along
        .iter()
        .map(|&at| {
            let t = (at / length.max(f32::EPSILON)).min(1.0);
            shift * (1.0 - t * t * (3.0 - 2.0 * t))
        })
        .collect();
    (geometry::offset_line_by(&line, &shifts), heights)
}

/// `points` and their `heights` cut finer: each segment into `cuts(from, length)` equal steps (at least one;
/// `from` is how far along the line it starts). With how far along the line each point lies.
fn cut_finer(points: &[Point], heights: &[f32], cuts: impl Fn(f32, f32) -> usize) -> (Vec<Point>, Vec<f32>, Vec<f32>) {
    let (mut line, mut high, mut along) = (vec![points[0]], vec![heights[0]], vec![0.0]);
    for (index, pair) in points.windows(2).enumerate() {
        let step = distance(pair[0], pair[1]);
        let from = *along.last().unwrap();
        let count = cuts(from, step).max(1);
        for cut in 1..=count {
            let t = cut as f32 / count as f32;
            line.push([pair[0][0] + (pair[1][0] - pair[0][0]) * t, pair[0][1] + (pair[1][1] - pair[0][1]) * t]);
            high.push(heights[index] + (heights[index + 1] - heights[index]) * t);
            along.push(from + step * t);
        }
    }
    (line, high, along)
}

/// The cells line ends are looked up in, metres: about the widest road's half width.
const SPLIT_CELL: f32 = 16.0;
/// How far along the road the pieces splitting it may start from its end, metres.
const SPLIT_ALONG: f32 = 0.5;
/// The pieces splitting a road head on from it within about 60° (the cosine): they part ways there.
const SPLIT_ONWARD: f32 = 0.5;

fn line_length(points: &[Point]) -> f32 {
    points.windows(2).map(|pair| distance(pair[0], pair[1])).sum()
}

/// `points` (with their `heights`) without their first `length` metres.
fn cut_start(points: &[Point], heights: &[f32], length: f32) -> (Vec<Point>, Vec<f32>) {
    let mut left = length;
    for (index, pair) in points.windows(2).enumerate() {
        let step = distance(pair[0], pair[1]);
        if step > left {
            let t = left / step;
            let cut = [pair[0][0] + (pair[1][0] - pair[0][0]) * t, pair[0][1] + (pair[1][1] - pair[0][1]) * t];
            let height = heights[index] + (heights[index + 1] - heights[index]) * t;
            return (
                std::iter::once(cut).chain(points[index + 1..].iter().copied()).collect(),
                std::iter::once(height).chain(heights[index + 1..].iter().copied()).collect(),
            );
        }
        left -= step;
    }
    match (points.last(), heights.last()) {
        (Some(&last), Some(&height)) => (vec![last, last], vec![height, height]),
        _ => (Vec::new(), Vec::new()),
    }
}

/// A line's first point and the way it leaves it (towards a point a few metres in, past any kink at the
/// very end).
fn line_end(points: &[Point]) -> Option<(Point, Point)> {
    let first = *points.first()?;
    let further = points.iter().find(|&&p| distance(first, p) >= 3.0).or(points.last())?;
    Some((first, direction(first, *further)?))
}

/// `road` cut into pieces narrowing (or widening) evenly along it from `start` to `end`: each a width,
/// metres (in steps of [`TAPER_STEP`]), and how far right of the road's line it's drawn there, metres (the
/// road moving over evenly too).
fn tapered(road: RoadLine, start: (f32, f32), end: (f32, f32)) -> Vec<RoadLine> {
    let (mut points, heights, along) =
        cut_finer(&road.points, &road.heights, |_, step| (step / TAPER_SPACING).ceil() as usize);
    let total = *along.last().unwrap();
    if total <= 0.0 {
        return vec![road];
    }
    let ((start, start_shift), (end, end_shift)) = (start, end);
    if start_shift != 0.0 || end_shift != 0.0 {
        let shifts: Vec<f32> = along.iter().map(|&at| start_shift + (end_shift - start_shift) * at / total).collect();
        points = geometry::offset_line_by(&points, &shifts);
    }
    let mut pieces: Vec<RoadLine> = Vec::new();
    for (segment, pair) in points.windows(2).enumerate() {
        let t = (along[segment] + along[segment + 1]) / 2.0 / total;
        let width = ((start + (end - start) * t) / TAPER_STEP).round() * TAPER_STEP;
        match pieces.last_mut() {
            Some(piece) if piece.width == width => {
                piece.points.push(pair[1]);
                piece.heights.push(heights[segment + 1]);
            }
            _ => pieces.push(RoadLine {
                points: pair.to_vec(),
                heights: heights[segment..=segment + 1].to_vec(),
                class: road.class,
                width,
                minzoom: road.minzoom,
                level: road.level,
            }),
        }
    }
    pieces
}

/// How many levels roads are drawn on: the ground and three above it (the page draws as many: style.ts
/// ROAD_LEVELS). A stretch over more is drawn on the highest.
pub const ROAD_LEVELS: u8 = 4;
/// How much higher than a road another must be, where they lie over one another, to pass over it, metres:
/// the game's bridges clear the roads beneath by 5.5 m or more, while roads meeting at a junction lie level,
/// give or take a metre where the ground slopes, and a slip road still climbing where its drawn line first
/// meets the road it joins (drawn lines run wider than their lanes) lies 3 to 4 m off it.
const CLEARANCE: f32 = 4.5;
/// Lines are compared in steps this long at most, metres: a raised stretch starts and ends at one.
const LEVEL_STEP: f32 = 4.0;
/// The cells steps are looked up in, metres.
const LEVEL_CELL: f32 = 32.0;
/// Metres a pixel covers at zoom 15 (512-pixel tiles round the world's 360 degrees).
const METRES_PER_PIXEL_15: f32 = 40_075_016.7 / (512.0 * 32768.0);
/// How far past its edge a road's casing shows, metres: a pixel at zoom 15, and a little more.
const CASING_REACH: f32 = 2.5;

/// How far from its line a road shows on the map, metres: half its drawn width (its true width, or wider
/// as the page draws it by its class: style.ts roadWidth, which is widest at zoom 15 of the zooms the cab
/// drives at) and its casing.
fn drawn_reach(class: RoadClass, width: f32) -> f32 {
    // style.ts CLASS_WIDTHS at zoom 15, pixels.
    let pixels = match class {
        RoadClass::Motorway => 7.0,
        RoadClass::Trunk => 6.0,
        RoadClass::Local => 4.5,
        RoadClass::Minor => 3.0,
        RoadClass::Rail => 0.0,
    };
    let lanes = (width / class.typical_width()).sqrt().clamp(0.55, 1.2);
    width.max(pixels * lanes * METRES_PER_PIXEL_15) / 2.0 + CASING_REACH
}

/// Roads over roads where the game has them over (a bridge over the motorway beneath it), not by their class
/// (a motorway over everything). Wherever a road passes over another (lies over it, as wide as they are,
/// more than [`CLEARANCE`] higher), the stretch of it that shows on top of the other is drawn raised, a level
/// above the highest it passes over; the rest of it stays on the ground. So a raised stretch is no longer than its crossing, and
/// never reaches a junction beyond (whose other roads would show under it). Rail lines stay under every road,
/// as before. `raised` counts the raised stretches by level.
fn road_levels(roads: Vec<RoadLine>, raised: &mut [usize; ROAD_LEVELS as usize - 1]) -> Vec<RoadLine> {
    struct Step {
        a: Point,
        b: Point,
        heights: [f32; 2],
        line: usize,
    }
    let finer =
        |road: &RoadLine| cut_finer(&road.points, &road.heights, |_, length| (length / LEVEL_STEP).ceil() as usize);
    // Every line in steps, line by line (a line's are `first_step[line]..first_step[line + 1]`).
    let mut steps: Vec<Step> = Vec::new();
    let mut first_step = Vec::with_capacity(roads.len() + 1);
    for (line, road) in roads.iter().enumerate() {
        first_step.push(steps.len());
        if road.class == RoadClass::Rail || road.points.len() < 2 {
            continue;
        }
        let (points, heights, _) = finer(road);
        steps.extend((0..points.len() - 1).map(|at| Step {
            a: points[at],
            b: points[at + 1],
            heights: [heights[at], heights[at + 1]],
            line,
        }));
    }
    first_step.push(steps.len());
    // Each step in every cell it shows in.
    let reach: Vec<f32> = roads.iter().map(|road| drawn_reach(road.class, road.width)).collect();
    let cell = |v: f32| (v / LEVEL_CELL).floor() as i32;
    let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (index, step) in steps.iter().enumerate() {
        let r = reach[step.line];
        for x in cell(step.a[0].min(step.b[0]) - r)..=cell(step.a[0].max(step.b[0]) + r) {
            for z in cell(step.a[1].min(step.b[1]) - r)..=cell(step.a[1].max(step.b[1]) + r) {
                cells.entry((x, z)).or_default().push(index as u32);
            }
        }
    }
    // Which steps pass over which: where they'd show on top of one another, more than CLEARANCE higher. And
    // which lines truly pass over which: where they lie over one another as wide as they are.
    let mut over: HashSet<(u32, u32)> = HashSet::new();
    let mut crossing: HashSet<(usize, usize)> = HashSet::new();
    for members in cells.values() {
        for (i, &s) in members.iter().enumerate() {
            let one = &steps[s as usize];
            let (one_low, one_high) = (one.heights[0].min(one.heights[1]), one.heights[0].max(one.heights[1]));
            for &t in &members[i + 1..] {
                let other = &steps[t as usize];
                let (other_low, other_high) =
                    (other.heights[0].min(other.heights[1]), other.heights[0].max(other.heights[1]));
                if one_high - other_low <= CLEARANCE && other_high - one_low <= CLEARANCE {
                    continue;
                }
                let (gap, along_one, along_other) = closest_between(one.a, one.b, other.a, other.b);
                if gap >= reach[one.line] + reach[other.line] {
                    continue;
                }
                let height_one = one.heights[0] + (one.heights[1] - one.heights[0]) * along_one;
                let height_other = other.heights[0] + (other.heights[1] - other.heights[0]) * along_other;
                let ((upper, upper_line), (lower, lower_line)) = if height_one > height_other + CLEARANCE {
                    ((s, one.line), (t, other.line))
                } else if height_other > height_one + CLEARANCE {
                    ((t, other.line), (s, one.line))
                } else {
                    continue;
                };
                over.insert((upper, lower));
                if gap < (roads[one.line].width + roads[other.line].width) / 2.0 {
                    crossing.insert((upper_line, lower_line));
                }
            }
        }
    }
    // Only between lines one truly over the other somewhere (a motorway's carriageways on a hillside, one a
    // few metres above the other, never are), but there as far as they show on top of one another.
    over.retain(|&(upper, lower)| crossing.contains(&(steps[upper as usize].line, steps[lower as usize].line)));
    // The raised stretches: each line's runs of steps over others.
    let mut is_over = vec![false; steps.len()];
    for &(upper, _) in &over {
        is_over[upper as usize] = true;
    }
    const NONE: u32 = u32::MAX;
    let mut run_of = vec![NONE; steps.len()];
    let mut runs = 0;
    for line in 0..roads.len() {
        for step in first_step[line]..first_step[line + 1] {
            if is_over[step] {
                if step == first_step[line] || !is_over[step - 1] {
                    runs += 1;
                }
                run_of[step] = runs - 1;
            }
        }
    }
    // Each a level above the highest it passes over (the ground's 0), up to the top level.
    let mut levels = vec![1u8; runs as usize];
    let mut beneath: Vec<(u32, u32)> = over
        .iter()
        .filter(|&&(_, lower)| run_of[lower as usize] != NONE)
        .map(|&(upper, lower)| (run_of[upper as usize], run_of[lower as usize]))
        .collect();
    beneath.sort_unstable();
    beneath.dedup();
    for _ in 0..ROAD_LEVELS {
        let mut changed = false;
        for &(upper, lower) in &beneath {
            let level = (levels[lower as usize] + 1).min(ROAD_LEVELS - 1);
            if levels[upper as usize] < level {
                levels[upper as usize] = level;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for &level in &levels {
        raised[level as usize - 1] += 1;
    }
    // Lines with raised stretches cut where their level changes.
    let mut out = Vec::with_capacity(roads.len() + 2 * runs as usize);
    for (line, road) in roads.into_iter().enumerate() {
        let (from, to) = (first_step[line], first_step[line + 1]);
        if run_of[from..to].iter().all(|&run| run == NONE) {
            out.push(road);
            continue;
        }
        let (points, heights, _) = finer(&road);
        let level_of = |step: usize| match run_of[from + step] {
            NONE => 0,
            run => levels[run as usize],
        };
        let mut start = 0;
        for step in 1..=to - from {
            if step == to - from || level_of(step) != level_of(start) {
                out.push(RoadLine {
                    points: points[start..=step].to_vec(),
                    heights: heights[start..=step].to_vec(),
                    class: road.class,
                    width: road.width,
                    minzoom: road.minzoom,
                    level: level_of(start),
                });
                start = step;
            }
        }
    }
    out
}

/// The closest points of the segments `a`–`b` and `c`–`d`: how far apart they are, and where along each
/// (0 at its start, 1 at its end).
fn closest_between(a: Point, b: Point, c: Point, d: Point) -> (f32, f32, f32) {
    let dot = |p: Point, q: Point| p[0] * q[0] + p[1] * q[1];
    let (u, v, w) = ([b[0] - a[0], b[1] - a[1]], [d[0] - c[0], d[1] - c[1]], [a[0] - c[0], a[1] - c[1]]);
    let (uu, uv, vv, uw, vw) = (dot(u, u), dot(u, v), dot(v, v), dot(u, w), dot(v, w));
    // Where the lines through them come closest (or the first's start, if they're parallel), then each in
    // turn the nearest point on its segment to the other.
    let across = uu * vv - uv * uv;
    let s = if across > 1e-9 { ((uv * vw - vv * uw) / across).clamp(0.0, 1.0) } else { 0.0 };
    let t = if vv > 1e-9 { ((uv * s + vw) / vv).clamp(0.0, 1.0) } else { 0.0 };
    let s = if uu > 1e-9 { ((uv * t - uw) / uu).clamp(0.0, 1.0) } else { 0.0 };
    let p = [a[0] + u[0] * s, a[1] + u[1] * s];
    let q = [c[0] + v[0] * t, c[1] + v[1] * t];
    (distance(p, q), s, t)
}

/// The last zoom roads are drawn generalised: by class alone, at their class's typical width (lanes don't
/// show yet), junction pieces joined in with the roads so motorways run on through interchanges, small
/// gaps bridged. From the next zoom on, pieces join only where class and width match (a road on into the
/// junction pieces that carry it on), so the map up close is unchanged.
const GENERAL_ROADS_ZOOM: u8 = 11;
/// The gap a generalised road is joined across, metres: a seventh of a pixel at the last general zoom.
const GENERAL_ROADS_SNAP: f32 = 5.0;
/// Up close, only ends that meet: a centimetre.
const DETAILED_ROADS_SNAP: f32 = 0.01;

/// The roads as tile features: their pieces joined into lines ([`geometry::join_lines`]), once for up
/// close and once, generalised, for further out (a joined line appears from its earliest piece's zoom).
/// Groups keep the order they first appear in, so the export is reproducible.
fn road_features(roads: Vec<RoadLine>) -> Vec<Feature> {
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
    let detailed = |road: &RoadLine| Some((road.class, road.width.to_bits(), road.level));
    for ((class, width, level), members) in grouped(&roads, detailed) {
        for (line, minzoom) in joined(&roads, &members, DETAILED_ROADS_SNAP) {
            let minzoom = minzoom.max(GENERAL_ROADS_ZOOM + 1);
            let mut feature = road_feature(&line, class, f32::from_bits(width), minzoom, u8::MAX);
            if level > 0 {
                feature.props.push(("level", Value::Num(level.into())));
            }
            features.push(feature);
        }
    }
    for (class, members) in grouped(&roads, |road| (road.minzoom <= GENERAL_ROADS_ZOOM).then_some(road.class)) {
        for (line, minzoom) in joined(&roads, &members, GENERAL_ROADS_SNAP) {
            features.push(road_feature(&line, class, class.typical_width(), minzoom, GENERAL_ROADS_ZOOM));
        }
    }
    features
}

fn road_feature(line: &[Point], class: RoadClass, width: f32, minzoom: u8, maxzoom: u8) -> Feature {
    Feature {
        layer: "road",
        minzoom,
        maxzoom,
        props: vec![("class", Value::Str(class.name().into())), ("width", Value::Num(width as f64))],
        geometry: Geometry::Lines(vec![line.iter().map(|&p| to_mercator(p)).collect()]),
    }
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
    start: &Node,
    end: &Node,
    inner: f32,
    side: &TerrainSide,
    relief: &Relief,
    sign: f32,
) -> TerrainGround {
    let grid = &side.grid;
    if grid.rows == 0 || grid.cols == 0 {
        return TerrainGround { land_runs: Vec::new(), land: Vec::new(), water: None };
    }
    let frames = geometry::curve_frames(start, end, grid.cols.into());
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
    });
    TerrainGround { land_runs, land, water }
}

/// A line `width` metres wide, as one quad per segment.
fn thick_line(line: &[Point], width: f32) -> Vec<Vec<Point>> {
    let half = width / 2.0;
    line.windows(2)
        .filter_map(|pair| {
            let [a, b] = [pair[0], pair[1]];
            let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
            let length = (dx * dx + dz * dz).sqrt();
            if length < 1e-3 {
                return None;
            }
            let (nx, nz) = (-dz / length * half, dx / length * half);
            Some(vec![[a[0] + nx, a[1] + nz], [b[0] + nx, b[1] + nz], [b[0] - nx, b[1] - nz], [a[0] - nx, a[1] - nz]])
        })
        .collect()
}

/// A place where a visible road meets a junction (a prefab) but the junction draws no road out to it: a
/// gap in the map.
#[derive(Clone, Debug)]
pub struct JunctionGap {
    pub prefab_path: String,
    /// The junction is one the game keeps off its own map.
    pub hidden: bool,
    pub at: Point,
}

/// How far a junction's road may pass from the end of the road it meets and still meet it, metres: a lane
/// (a prefab's road points sit near its ends, not always exactly on them).
const JUNCTION_REACH: f32 = 4.0;

/// Every junction gap on the map, to check the rules for a prefab's roads map-wide.
pub fn junction_gaps(map: &Map, definitions: &Definitions, prefabs: &mut Prefabs) -> Vec<JunctionGap> {
    // Shown roads, as the export draws them (on their lanes: one line or two carriageways).
    let measured = lanes::road_offsets(map, prefabs);
    let roads: HashMap<u64, Vec<(f32, f32)>> = map
        .items
        .iter()
        .filter_map(|item| match &item.data {
            ItemData::Road { flags, look, .. } if !flags.hidden && !flags.secret => {
                Some((item.uid, drawn_lines(measured.get(look), definitions.road_looks.get(look))))
            }
            _ => None,
        })
        .collect();
    let mut gaps = Vec::new();
    for item in &map.items {
        let ItemData::Prefab { flags, model, nodes, origin, .. } = &item.data else { continue };
        if flags.secret {
            continue;
        }
        let Some(prefab) = prefabs.get(*model) else { continue };
        let Some(placement) = place_prefab(map, prefab, nodes, *origin as usize) else { continue };
        // Each step of each piece, with how far from its middle the road reaches (where pieces share a
        // road, they run beside its middle, not on it).
        let steps: Vec<(Point, Point, f32)> = prefab_roads(prefab, &[], &[])
            .into_iter()
            .flat_map(|(line, _, width)| {
                let line: Vec<Point> = line.iter().map(|&p| placement.place(p)).collect();
                let reach = JUNCTION_REACH.max(width / 2.0 + SHARED_ROAD_SLACK);
                line.windows(2).map(|step| (step[0], step[1], reach)).collect::<Vec<_>>()
            })
            .collect();
        for node in nodes.iter().filter_map(|uid| map.nodes.get(uid)) {
            let Some(lines) = [node.forward_item, node.backward_item].iter().find_map(|item| roads.get(item)) else {
                continue;
            };
            let (line, way) = (geometry::node_point(node), geometry::node_forward(node));
            for &(shift, width) in lines {
                // Where the road's line (or carriageway) ends as drawn: its line, moved across it.
                let at = [line[0] - way[1] * shift, line[1] + way[0] * shift];
                // Some prefabs' roads run on a little past their ends, through where the road meets them.
                // The road joins a piece where they overlap across it: within both their half widths (where
                // a road splits, its pieces run beside its middle, each on its own lanes).
                if steps.iter().any(|&(a, b, reach)| distance_to_segment(at, a, b) <= reach + width / 2.0) {
                    continue;
                }
                let prefab_path = definitions.prefab_paths.get(model).cloned().unwrap_or_default();
                gaps.push(JunctionGap { prefab_path, hidden: flags.hidden, at });
            }
        }
    }
    gaps
}

fn place_prefab(map: &Map, prefab: &Prefab, nodes: &[u64], origin: usize) -> Option<PrefabPlacement> {
    let at = map.nodes.get(nodes.first()?)?;
    Some(PrefabPlacement::new(prefab.nodes.get(origin)?, at))
}

/// For every road point of every prefab (by item, then point): how far it is from the nearest road of each
/// class (by [`CLASS_RANKS`]), along the prefabs' own roads and on through prefabs joined end to end, for
/// [`segment_class`]. Looking past a junction's own ends keeps a road's class through a chain of junctions:
/// a junction wedged between others has no road at its ends, and the only class it could see was its slip
/// road's (the motorway through it came out a local road).
fn prefab_point_classes(
    map: &Map,
    prefabs: &mut Prefabs,
    road_classes: &HashMap<u64, RoadClass>,
) -> HashMap<u64, Vec<[f32; CLASSES]>> {
    // Every prefab's road points as one network: each point's links, metres long; points at the same
    // world node (prefabs joined end to end, or a road's end) are one place.
    let mut links: Vec<Vec<(usize, f32)>> = Vec::new();
    let mut at_node: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut placed = Vec::new();
    for item in &map.items {
        let ItemData::Prefab { model, nodes, origin, .. } = &item.data else { continue };
        let Some(prefab) = prefabs.get(*model) else { continue };
        let Some(placement) = place_prefab(map, prefab, nodes, *origin as usize) else { continue };
        let (points, used, base) = (&prefab.map_points, prefab_road_points(prefab), links.len());
        links.resize(base + points.len(), Vec::new());
        for i in (0..points.len()).filter(|&i| used[i]) {
            for &n in points[i].neighbours.iter().filter(|&&n| used[n]) {
                let length = distance(placement.place(points[i].position), placement.place(points[n].position));
                links[base + i].push((base + n, length));
                links[base + n].push((base + i, length));
            }
            let world = points[i]
                .control_node
                .map(|end| geometry::prefab_world_node(end, *origin as usize, prefab.nodes.len()));
            if let Some(uid) = world.and_then(|index| nodes.get(index)) {
                at_node.entry(*uid).or_default().push(base + i);
            }
        }
        placed.push((item.uid, base, points.len()));
    }
    let mut ends: Vec<(u64, Vec<usize>)> = at_node.into_iter().collect();
    ends.sort_unstable_by_key(|(uid, _)| *uid);
    let mut seeds = Vec::new();
    for (uid, points) in &ends {
        for &a in points {
            links[a].extend(points.iter().filter(|&&b| b != a).map(|&b| (b, 0.0)));
        }
        let Some(node) = map.nodes.get(uid) else { continue };
        if let Some(class) = [node.forward_item, node.backward_item].iter().find_map(|item| road_classes.get(item)) {
            seeds.extend(points.iter().map(|&point| (point, rank(*class))));
        }
    }
    let distances = nearest_distances(&links, &seeds);
    placed.into_iter().map(|(uid, base, len)| (uid, distances[base..base + len].to_vec())).collect()
}

/// Road classes, best first: the order a junction's road takes one in when more than one fits it.
const CLASS_RANKS: [RoadClass; CLASSES] =
    [RoadClass::Motorway, RoadClass::Trunk, RoadClass::Local, RoadClass::Minor, RoadClass::Rail];
const CLASSES: usize = 5;

fn rank(class: RoadClass) -> usize {
    CLASS_RANKS.iter().position(|&ranked| ranked == class).unwrap_or(CLASSES - 1)
}

/// How much further than its nearest road a road of another class may be from a segment's end and still
/// count as reaching it.
const CLASS_REACH: f32 = 2.0;

/// A junction segment's class, from its ends' distances to each class of road. Of the classes that reach
/// both ends (no further than [`CLASS_REACH`] times each end's nearest road), a segment of two lanes or more
/// takes the best (the motorway through a chain of junctions) and a one-lane segment the slowest (a service
/// area's forecourt lanes, though the motorway's ramps are nearer than its local roads). Where none reaches
/// both, the slower of the two ends' nearest classes: a link between a fast road and a slower one, a slip
/// road. `None` with no road near either end.
fn segment_class(a: &[f32; CLASSES], b: &[f32; CLASSES], lanes: u32) -> Option<RoadClass> {
    let nearest = |d: &[f32; CLASSES]| d.iter().copied().fold(f32::INFINITY, f32::min);
    let (near_a, near_b) = (nearest(a), nearest(b));
    if near_a.is_infinite() && near_b.is_infinite() {
        return None;
    }
    let reaches = |d: &[f32; CLASSES], near: f32, rank: usize| d[rank].is_finite() && d[rank] <= CLASS_REACH * near;
    let mut both = (0..CLASSES).filter(|&rank| reaches(a, near_a, rank) && reaches(b, near_b, rank));
    if let Some(rank) = if lanes >= 2 { both.next() } else { both.next_back() } {
        return Some(CLASS_RANKS[rank]);
    }
    let nearest_rank = |d: &[f32; CLASSES], near: f32| (0..CLASSES).find(|&rank| d[rank] == near);
    let slower = nearest_rank(a, near_a).into_iter().chain(nearest_rank(b, near_b)).max()?;
    Some(CLASS_RANKS[slower])
}

/// Each point's distance to the nearest seed of each class (seeds by rank) along the links.
fn nearest_distances(links: &[Vec<(usize, f32)>], seeds: &[(usize, usize)]) -> Vec<[f32; CLASSES]> {
    let mut distances = vec![[f32::INFINITY; CLASSES]; links.len()];
    for rank in 0..CLASSES {
        let starts = seeds.iter().filter(|(_, seed)| *seed == rank).map(|&(point, _)| point);
        for (all, one) in distances.iter_mut().zip(seed_distances(links, starts)) {
            all[rank] = one;
        }
    }
    distances
}

/// Each point's distance to the nearest of `seeds` along the links (Dijkstra).
fn seed_distances(links: &[Vec<(usize, f32)>], seeds: impl Iterator<Item = usize>) -> Vec<f32> {
    let mut best = vec![f32::INFINITY; links.len()];
    // Distances are never negative, so their bits order as they do.
    let mut queue = BinaryHeap::new();
    for point in seeds {
        best[point] = 0.0;
        queue.push(Reverse((0f32.to_bits(), point)));
    }
    while let Some(Reverse((bits, point))) = queue.pop() {
        let length = f32::from_bits(bits);
        if length > best[point] {
            continue;
        }
        for &(next, step) in &links[point] {
            if length + step < best[next] {
                best[next] = length + step;
                queue.push(Reverse(((length + step).to_bits(), next)));
            }
        }
    }
    best
}

fn distance(a: Point, b: Point) -> f32 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

/// A prefab's area polygons, in its own space, with their colours: each a ring of area points linked as
/// neighbours.
fn prefab_areas(prefab: &Prefab) -> Vec<(Vec<Point>, u8)> {
    let points = &prefab.map_points;
    let mut visited = vec![false; points.len()];
    let mut areas = Vec::new();
    for start in 0..points.len() {
        if visited[start] || points[start].kind != MapPointKind::Area {
            continue;
        }
        let mut ring = vec![start];
        visited[start] = true;
        let mut current = start;
        while let Some(&next) = points[current].neighbours.iter().find(|&&n| !visited[n]) {
            visited[next] = true;
            ring.push(next);
            current = next;
        }
        if ring.len() >= 3 {
            let first = &points[start];
            areas.push((ring.iter().map(|&i| points[i].position).collect(), first.area_color));
        }
    }
    areas
}

/// Which of a prefab's map points its roads run through. A prefab's map points hold the roads the map
/// draws and, flagged, its navigation graph: mostly the same roads again, through a centre point. But the
/// drawn roads don't always have them all, so, as the game's map shows:
///
/// - with no drawn roads at all (many simple crossings), the whole navigation graph is drawn;
/// - a navigation link straight from one end of the prefab to another that no drawn road runs along is
///   drawn (the road past a filling station whose drawn roads are only its forecourt lanes);
/// - an end the roads still don't reach is joined on through the navigation graph, by the shortest way to
///   an end they do reach.
fn prefab_road_points(prefab: &Prefab) -> Vec<bool> {
    let points = &prefab.map_points;
    let road = |i: usize| matches!(points[i].kind, MapPointKind::Road(_));
    let navigation = |i: usize| road(i) && points[i].navigation_only;
    let mut used: Vec<bool> = (0..points.len()).map(|i| road(i) && !points[i].navigation_only).collect();
    drop_repeated_graphs(points, &mut used);
    if !used.contains(&true) {
        return (0..points.len()).map(navigation).collect();
    }

    // Direct links between ends, unless drawn roads run along them.
    let drawn: Vec<(Point, Point)> = (0..points.len())
        .filter(|&i| used[i])
        .flat_map(|i| {
            points[i].neighbours.iter().filter(|&&n| used[n]).map(move |&n| (points[i].position, points[n].position))
        })
        .collect();
    for i in (0..points.len()).filter(|&i| navigation(i) && points[i].control_node.is_some()) {
        for &n in &points[i].neighbours {
            let direct = navigation(n) && points[n].control_node.is_some();
            if direct && !runs_along(points[i].position, points[n].position, &drawn, PREFAB_ROAD_ALONG) {
                used[i] = true;
                used[n] = true;
            }
        }
    }

    // Ends not reached yet, joined on through the navigation graph.
    let reached: HashSet<usize> =
        (0..points.len()).filter(|&i| used[i]).filter_map(|i| points[i].control_node).collect();
    let ends: BTreeSet<usize> = points.iter().filter_map(|point| point.control_node).collect();
    for node in ends.into_iter().filter(|end| !reached.contains(end)) {
        let Some(start) = (0..points.len()).find(|&i| navigation(i) && points[i].control_node == Some(node)) else {
            continue;
        };
        let mut came_from = vec![None; points.len()];
        let mut queue = VecDeque::from([start]);
        let mut seen = vec![false; points.len()];
        seen[start] = true;
        let mut found = None;
        while let Some(i) = queue.pop_front() {
            if i != start && points[i].control_node.is_some_and(|end| reached.contains(&end)) {
                found = Some(i);
                break;
            }
            for &n in &points[i].neighbours {
                if navigation(n) && !seen[n] {
                    seen[n] = true;
                    came_from[n] = Some(i);
                    queue.push_back(n);
                }
            }
        }
        let mut at = found;
        while let Some(i) = at {
            used[i] = true;
            at = came_from[i];
        }
    }
    used
}

/// Leaves out (from `used`) a drawn graph that repeats another: some prefabs draw their roads twice, as
/// two separate graphs reaching the same ends, either an identical copy or one running straight from end
/// to end beside one through a centre point (134 models, mostly motorway forks, exits and merges); drawn
/// both, they overlap at odd angles. The one without a centre point is kept, its pieces running from end
/// to end along the roads they join; otherwise the first.
fn drop_repeated_graphs(points: &[MapPoint], used: &mut [bool]) {
    // Each drawn graph: its points, the ends it reaches, and whether it has a centre point.
    let mut graph_of = vec![usize::MAX; points.len()];
    let mut graphs: Vec<(Vec<usize>, BTreeSet<usize>, bool)> = Vec::new();
    for start in 0..points.len() {
        if !used[start] || graph_of[start] != usize::MAX {
            continue;
        }
        let (mut members, mut stack) = (Vec::new(), vec![start]);
        graph_of[start] = graphs.len();
        while let Some(i) = stack.pop() {
            members.push(i);
            for &n in &points[i].neighbours {
                if used[n] && graph_of[n] == usize::MAX {
                    graph_of[n] = graphs.len();
                    stack.push(n);
                }
            }
        }
        let ends: BTreeSet<usize> = members.iter().filter_map(|&i| points[i].control_node).collect();
        let centred = members.iter().any(|&i| points[i].control_node.is_none());
        graphs.push((members, ends, centred));
    }
    for (index, (members, ends, centred)) in graphs.iter().enumerate() {
        let preferred = |other: usize| {
            let (_, other_ends, other_centred) = &graphs[other];
            other_ends == ends && (other_centred < centred || (other_centred == centred && other < index))
        };
        if ends.len() >= 2 && (0..graphs.len()).any(|other| other != index && preferred(other)) {
            for &i in members {
                used[i] = false;
            }
        }
    }
}

/// How near a drawn road must stay to a navigation link all along to be the same road, metres: a lane.
const PREFAB_ROAD_ALONG: f32 = 4.0;

/// Whether the line from `a` to `b` stays within `tolerance` of the `segments` all along (checked every few
/// metres).
fn runs_along(a: Point, b: Point, segments: &[(Point, Point)], tolerance: f32) -> bool {
    let length = (b[0] - a[0]).hypot(b[1] - a[1]);
    let steps = (length / 2.0).ceil().max(1.0) as usize;
    (0..=steps).all(|step| {
        let t = step as f32 / steps as f32;
        let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        segments.iter().any(|&(s, e)| distance_to_segment(p, s, e) <= tolerance)
    })
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length2 = dx * dx + dy * dy;
    let t = if length2 == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length2).clamp(0.0, 1.0) };
    (p[0] - (a[0] + dx * t)).hypot(p[1] - (a[1] + dy * t))
}

/// Whether a prefab is a site off the road rather than a junction on it: a filling station, a service shop,
/// a garage, a car park, a hotel (as its spawn points say).
fn is_site(prefab: &Prefab) -> bool {
    use SpawnKind::{Fuel, Garage, GarageForSale, Hotel, Parking, Service};
    prefab
        .spawn_points
        .iter()
        .any(|point| matches!(point.kind, Fuel | Service | Garage | GarageForSale | Parking | Hotel))
}

/// A prefab's roads, in its own space, each with its class ([`segment_class`], from `distances` by point,
/// from [`prefab_point_classes`]; a local road where there's no road near, and at most a local road in a
/// site: a filling station's or a service shop's lanes, though motorway ramps lead to them) and its width.
///
/// Its road segments are chained into pieces, from end to end or to where they meet others or change
/// class or width, and each piece is drawn as a smooth curve ([`smooth_piece`]): at the prefab's ends
/// that something joins (`joined`, by prefab node) along the direction the road there comes in, so a
/// junction flows on from its roads as the game's own curves do, rather than as the straight lines between
/// its few points. (An end nothing joins has a direction that means nothing: a bus station's dead end
/// would hook off sideways.) Where pieces split one road
/// between them (a motorway forking into its two carriageways, a slip road leaving a carriageway), they
/// start side by side across it, each in its own lanes, rather than all from its middle.
fn prefab_roads(prefab: &Prefab, distances: &[[f32; CLASSES]], joined: &[bool]) -> Vec<(Vec<Point>, RoadClass, f32)> {
    prefab_road_lines(prefab, distances, joined)
        .into_iter()
        .map(|(line, _, class, width)| (line, class, width))
        .collect()
}

/// A line of [`prefab_roads`], with the prefab's ends (nodes) it runs from and to: none where it ends inside.
type PrefabRoadLine = (Vec<Point>, [Option<usize>; 2], RoadClass, f32);

/// [`prefab_roads`], each line with the prefab's ends it runs between.
fn prefab_road_lines(prefab: &Prefab, distances: &[[f32; CLASSES]], joined: &[bool]) -> Vec<PrefabRoadLine> {
    let points = &prefab.map_points;
    // Where the drawn road splits at a point inside ([`CentreFork`]), each branch is drawn from the trunk's
    // end, on the lanes leading to it, through the point (running beside the others up to it): no stub of
    // the whole road, no branches all starting from its middle. Each such piece, with where the point is
    // along it.
    let (pieces, through_centre) = branches_from_their_end(prefab, drawn_pieces(prefab, distances));

    // Each piece's control points: its ends at a prefab's ends on the lanes there (the prefab's lanes are where
    // the road really is: its node, and its drawn graph, may lie off them), so the roads there join on
    // exactly. A piece from one end to another ends on the lanes the way between them takes (where a
    // motorway splits into its carriageways, each carriageway's: they run on straight, though the drawn
    // graph is a V); any other, on all the lanes there, and moved across the road where pieces share it.
    let mut on_their_lanes: HashSet<(usize, bool)> = HashSet::new();
    let mut controls: Vec<Vec<Point>> = Vec::with_capacity(pieces.len());
    for (index, (piece, _)) in pieces.iter().enumerate() {
        let last = piece.len() - 1;
        let mut control: Vec<Point> = piece.iter().map(|&i| points[i].position).collect();
        for (at, other, start) in [(0, last, true), (last, 0, false)] {
            let Some(node) = points[piece[at]].control_node.filter(|&node| node < prefab.nodes.len()) else {
                continue;
            };
            let way = points[piece[other]]
                .control_node
                .filter(|&to| to != node && to < prefab.nodes.len())
                .and_then(|to| lanes::across(prefab, node, &lanes::lanes_between(prefab, node, to)));
            if way.is_some() {
                on_their_lanes.insert((index, start));
            }
            control[at] = way
                .or_else(|| lanes::across(prefab, node, &lanes::lanes_at(prefab, node)))
                .unwrap_or(prefab.nodes[node].position);
        }
        if let Some(&centre) = through_centre.get(&index) {
            let line: Vec<Point> = piece.iter().map(|&i| points[i].position).collect();
            control = beside_to_centre(&control, &line, centre);
        }
        controls.push(control);
    }
    let mut at_node: BTreeMap<usize, Vec<(usize, bool)>> = BTreeMap::new();
    for (index, (piece, _)) in pieces.iter().enumerate() {
        let (first, last) = (piece[0], piece[piece.len() - 1]);
        if first == last {
            continue;
        }
        for (point, start) in [(first, true), (last, false)] {
            if let Some(node) = points[point].control_node.filter(|&node| node < prefab.nodes.len()) {
                at_node.entry(node).or_default().push((index, start));
            }
        }
    }
    for (node_index, ends) in &at_node {
        // Pieces on their own lanes are already side by side, where their lanes are.
        if ends.iter().any(|end| on_their_lanes.contains(end)) {
            continue;
        }
        let node = &prefab.nodes[*node_index];
        let width_of = |index: usize| f32::from_bits(pieces[index].1.1);
        let road_width = node.lanes as f32 * LANE_WIDTH;
        let shared: f32 = ends.iter().map(|&(index, _)| width_of(index)).sum();
        if ends.len() < 2 || shared > road_width + SHARED_ROAD_SLACK {
            continue;
        }
        // Across the road, by where each piece leads (its far end).
        let node_index = *node_index;
        let Some(out) = unit(node.direction) else { continue };
        let across = [-out[1], out[0]];
        let lateral = |p: Point| (p[0] - node.position[0]) * across[0] + (p[1] - node.position[1]) * across[1];
        let mut ends = ends.clone();
        ends.sort_by(|&(a, a_start), &(b, b_start)| {
            let far_end = |index: usize, start: bool| {
                let piece = &controls[index];
                if start { piece[piece.len() - 1] } else { piece[0] }
            };
            lateral(far_end(a, a_start)).total_cmp(&lateral(far_end(b, b_start)))
        });
        // A road of two carriageways (a motorway's), met by two pieces (a roundabout's way in and out): one
        // on each carriageway, where its lanes are.
        if let (Some((left, right)), [first, second]) = (lanes::carriageways_at(prefab, node_index), &ends[..]) {
            for (&(index, start), at) in [(first, left), (second, right)] {
                let piece = &mut controls[index];
                let end = if start { 0 } else { piece.len() - 1 };
                piece[end] = at;
            }
            continue;
        }
        let mut from = -shared / 2.0;
        for (index, start) in ends {
            let offset = from + width_of(index) / 2.0;
            from += width_of(index);
            let piece = &mut controls[index];
            let at = if start { 0 } else { piece.len() - 1 };
            piece[at] = [piece[at][0] + across[0] * offset, piece[at][1] + across[1] * offset];
        }
    }

    // Where a piece ends at a prefab's end, it runs along the road there; where it meets others inside
    // the prefab, on smoothly into whichever goes on straightest, if any does.
    let mut meeting: HashMap<usize, Vec<Point>> = HashMap::new();
    for (index, (piece, _)) in pieces.iter().enumerate() {
        let control = &controls[index];
        let n = control.len();
        meeting.entry(piece[0]).or_default().extend(direction(control[0], control[1]));
        meeting.entry(piece[piece.len() - 1]).or_default().extend(direction(control[n - 1], control[n - 2]));
    }
    // The direction into a piece at its end at `point`, `away` being its first step from there.
    let into = |point: usize, away: Point| -> Point {
        let end = points[point].control_node.filter(|&node| joined.get(node) == Some(&true));
        if let Some(node) = end.and_then(|node| prefab.nodes.get(node))
            && let Some(out) = unit(node.direction)
        {
            return if out[0] * away[0] + out[1] * away[1] > 0.0 { out } else { [-out[0], -out[1]] };
        }
        let straight_on = meeting
            .get(&point)
            .into_iter()
            .flatten()
            .map(|&other| (geometry::turn(away, [-other[0], -other[1]]), other))
            .filter(|&(angle, _)| angle < geometry::STROKE_TURN)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        match straight_on {
            Some((_, other)) => unit([away[0] - other[0], away[1] - other[1]]).unwrap_or(away),
            None => away,
        }
    };

    let mut roads = Vec::with_capacity(pieces.len());
    for ((piece, (class, width)), control) in pieces.iter().zip(controls) {
        let n = control.len();
        let (first, last) = (piece[0], piece[n - 1]);
        let closed = first == last && n > 2;
        let (start, end) = if closed {
            let through = direction(control[n - 2], control[1]).unwrap_or([1.0, 0.0]);
            (through, through)
        } else {
            let away_start = direction(control[0], control[1]).unwrap_or([1.0, 0.0]);
            let away_end = direction(control[n - 1], control[n - 2]).unwrap_or([1.0, 0.0]);
            let end = into(last, away_end);
            (into(first, away_start), [-end[0], -end[1]])
        };
        let ends = (points[first].control_node, points[last].control_node);
        let carriageways = match ends {
            (Some(from), Some(to)) if from != to => carriageway_pieces(prefab, from, to, &control),
            _ => None,
        };
        let node_at = |point: usize| points[point].control_node.filter(|&node| node < prefab.nodes.len());
        let between = [node_at(first), node_at(last)];
        match carriageways {
            Some(carriageways) => {
                for (control, width) in carriageways {
                    roads.push((smooth_piece(&control, start, end), between, *class, width));
                }
            }
            None => roads.push((smooth_piece(&control, start, end), between, *class, f32::from_bits(*width))),
        }
    }
    roads
}

/// How finely a prefab's lane curves are followed for their heights: points along each.
const LANE_STEPS: usize = 8;
/// How far from a lane a junction's road may be drawn and still run along it, metres: a road's middle lies
/// between its lanes.
const LANE_NEAR: f32 = 5.0;
/// A road runs along a lane heading within about 37° of its way, or of the other way (the cosine).
const LANE_ALIGNED: f32 = 0.8;

/// A prefab's lane curves, as points each with its height, where they lie at different heights in the
/// prefab itself, more than [`CLEARANCE`] apart (a road over another inside it): `None` where they're all
/// about level, so its roads are as high as its ends.
fn lanes_by_height(prefab: &Prefab) -> Option<Vec<Vec<(Point, f32)>>> {
    let heights = prefab.curves.iter().flat_map(|curve| curve.heights);
    let (low, high) = heights.fold((f32::MAX, f32::MIN), |(low, high), height| (low.min(height), high.max(height)));
    (high - low > CLEARANCE).then(|| prefab.curves.iter().map(|curve| curve.points(LANE_STEPS)).collect())
}

/// How high each of `line`'s points is (in the prefab's own space), from the `lanes` ([`lanes_by_height`])
/// it runs along: the nearest lane within [`LANE_NEAR`] running its way (or the other way), at that point
/// along it. A road crossing over another meets the other's lanes across its way, so it takes its own.
/// `None` where no lane runs along it.
fn lane_heights(lanes: &[Vec<(Point, f32)>], line: &[Point]) -> Vec<Option<f32>> {
    (0..line.len())
        .map(|at| {
            let way = direction(line[at.saturating_sub(1)], line[(at + 1).min(line.len() - 1)])?;
            let mut nearest: Option<(f32, f32)> = None;
            for pair in lanes.iter().flat_map(|lane| lane.windows(2)) {
                let ((a, a_height), (b, b_height)) = (pair[0], pair[1]);
                let Some(along) = direction(a, b) else { continue };
                if (along[0] * way[0] + along[1] * way[1]).abs() < LANE_ALIGNED {
                    continue;
                }
                let (gap, _, t) = closest_between(line[at], line[at], a, b);
                if gap < LANE_NEAR && nearest.is_none_or(|(best, _)| gap < best) {
                    nearest = Some((gap, a_height + (b_height - a_height) * t));
                }
            }
            nearest.map(|(_, height)| height)
        })
        .collect()
}

/// The heights along `line`, climbing evenly from `from` at its start to `to` at its end.
fn climbing(line: &[Point], from: f32, to: f32) -> Vec<f32> {
    let total = line_length(line);
    let mut along = 0.0;
    let mut heights = Vec::with_capacity(line.len());
    for (index, &point) in line.iter().enumerate() {
        if index > 0 {
            along += distance(line[index - 1], point);
        }
        heights.push(if total > 0.0 { from + (to - from) * along / total } else { from });
    }
    heights
}

/// A value at `p` from `values` known at points (each a point and its value): the value at a point there,
/// and between, weighted by how near each is (inverse square distance). 0 with none known.
fn blended(values: &[(Point, f32)], p: Point) -> f32 {
    let (mut sum, mut weights) = (0.0, 0.0);
    for &(at, value) in values {
        let squared = (at[0] - p[0]).powi(2) + (at[1] - p[1]).powi(2);
        if squared < 1e-6 {
            return value;
        }
        sum += value / squared;
        weights += 1.0 / squared;
    }
    if weights > 0.0 { sum / weights } else { 0.0 }
}

/// A prefab's drawn road segments chained into pieces (by the map points along each), each with its class
/// and width ([`prefab_roads`]).
fn drawn_pieces(prefab: &Prefab, distances: &[[f32; CLASSES]]) -> Vec<(Vec<usize>, (RoadClass, u32))> {
    let points = &prefab.map_points;
    let used = prefab_road_points(prefab);
    let far = [f32::INFINITY; CLASSES];
    let distance_of = |i: usize| distances.get(i).unwrap_or(&far);
    let site = is_site(prefab);
    let node_of = |i: usize| points[i].control_node.and_then(|node| prefab.nodes.get(node));

    // A point's lanes, or its end's: always its end's for the navigation graph, whose lane counts don't
    // match the roads (the road past a filling station would come out half as wide as the roads either side).
    let lanes = |i: usize| {
        let end = node_of(i).map(|node| node.lanes);
        match points[i].kind {
            MapPointKind::Road(_) if points[i].navigation_only && end.is_some() => end,
            MapPointKind::Road(Some(lanes)) => Some(lanes),
            _ => end,
        }
    };
    // A segment's lanes: from end to end, the fewer (a slip road's one lane leaving a four-lane end, three
    // of which go on as the motorway); from an end into the prefab, the end's (the road coming in), unless
    // the road splits there (a roundabout's way in and way out, each one direction of a dual
    // carriageway): then each its own, as the point inside says (no more than the end's); otherwise
    // either's.
    // How many drawn links meet at each point (listed from either side).
    let mut links: HashSet<(usize, usize)> = HashSet::new();
    for i in (0..points.len()).filter(|&i| used[i]) {
        links.extend(points[i].neighbours.iter().filter(|&&n| used[n] && n != i).map(|&n| (i.min(n), i.max(n))));
    }
    let mut degree = vec![0; points.len()];
    for &(a, b) in &links {
        degree[a] += 1;
        degree[b] += 1;
    }
    let splits = |end: usize| degree[end] > 1;
    let into = |end: usize, inside: usize| match (lanes(end), lanes(inside)) {
        (Some(end_lanes), Some(own)) if splits(end) => Some(own.min(end_lanes)),
        (end_lanes, own) => end_lanes.or(own),
    };
    let segment_lanes = |i: usize, n: usize| match (node_of(i).is_some(), node_of(n).is_some()) {
        (true, true) => lanes(i).into_iter().chain(lanes(n)).min(),
        (true, false) => into(i, n),
        (false, true) => into(n, i),
        (false, false) => lanes(i).or(lanes(n)),
    };

    // Links may be listed from one end or both: each is drawn once.
    let mut segments = Vec::new();
    let mut seen = HashSet::new();
    for i in (0..points.len()).filter(|&i| used[i]) {
        for &n in points[i].neighbours.iter().filter(|&&n| used[n] && seen.insert((i.min(n), i.max(n)))) {
            let lanes = segment_lanes(i, n).unwrap_or(2).max(1);
            let width = lanes as f32 * LANE_WIDTH;
            let class = match segment_class(distance_of(i), distance_of(n), lanes).unwrap_or(RoadClass::Local) {
                RoadClass::Motorway | RoadClass::Trunk if site => RoadClass::Local,
                class => class,
            };
            segments.push((i, n, (class, width.to_bits())));
        }
    }
    chain_segments(points.len(), &segments, |i| points[i].control_node.is_some())
}

/// Where a junction's drawn road splits at a point inside it (not one of its ends): one end there, the
/// trunk, whose lanes lead to (or come from) every other end there, and the branches to those other
/// ends, between which no lanes run (a road dividing into a loop's way in and way out; a motorway into
/// the motorway and an exit, or the two joining). Drawn as its graph has it, a stub of the whole road
/// runs from the trunk to that point and the branches all start from its middle, overlapping into a
/// lump. A crossing drawn through a centre point (every arm leading to every other) isn't one.
#[derive(Clone, Debug, PartialEq)]
struct CentreFork {
    /// The point inside where it splits.
    at: usize,
    /// The piece from the trunk end to it.
    trunk: usize,
    /// The pieces from it to the other ends.
    branches: Vec<usize>,
}

/// The centre forks among a prefab's drawn `pieces` ([`CentreFork`]): only where each arm is one piece
/// from the point to an end.
fn centre_forks<K>(prefab: &Prefab, pieces: &[(Vec<usize>, K)]) -> Vec<CentreFork> {
    let points = &prefab.map_points;
    let end_node = |point: usize| points[point].control_node.filter(|&node| node < prefab.nodes.len());
    let mut arms: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    for (index, (piece, _)) in pieces.iter().enumerate() {
        let (first, last) = (piece[0], piece[piece.len() - 1]);
        if first == last {
            continue;
        }
        for (inside, end) in [(first, last), (last, first)] {
            if end_node(inside).is_none() {
                arms.entry(inside).or_default().push((index, end));
            }
        }
    }
    let mut forks = Vec::new();
    for (at, arms) in arms {
        if arms.len() < 3 {
            continue;
        }
        let Some(nodes) = arms.iter().map(|&(_, end)| end_node(end)).collect::<Option<Vec<usize>>>() else {
            continue;
        };
        let connected = |a: usize, b: usize| !lanes::lanes_between(prefab, a, b).is_empty();
        let trunks: Vec<usize> = (0..nodes.len())
            .filter(|&t| {
                let others: Vec<usize> = (0..nodes.len()).filter(|&o| o != t).collect();
                others.iter().all(|&o| connected(nodes[t], nodes[o]))
                    && others.iter().all(|&a| others.iter().all(|&b| a == b || !connected(nodes[a], nodes[b])))
            })
            .collect();
        let [trunk] = trunks[..] else { continue };
        forks.push(CentreFork {
            at,
            trunk: arms[trunk].0,
            branches: arms.iter().enumerate().filter(|&(arm, _)| arm != trunk).map(|(_, &(piece, _))| piece).collect(),
        });
    }
    forks
}

/// A prefab's drawn pieces: each its map points and its key (class and width).
type Pieces<K> = Vec<(Vec<usize>, K)>;

/// `pieces` with each [`CentreFork`]'s trunk and branches replaced by one piece per branch: from the trunk's
/// end through the point where the road splits on to the branch's end, with the branch's class and width.
/// And for each such piece (by its index), where the point is along it (its index in the piece).
fn branches_from_their_end<K: Copy>(
    prefab: &Prefab,
    pieces: Vec<(Vec<usize>, K)>,
) -> (Pieces<K>, HashMap<usize, usize>) {
    let forks = centre_forks(prefab, &pieces);
    let mut gone: HashSet<usize> = HashSet::new();
    let mut added: Vec<(Vec<usize>, K, usize)> = Vec::new();
    // A piece from its end `first` on (as it is, or turned round).
    let from = |piece: &[usize], first: usize| -> Vec<usize> {
        if piece[0] == first { piece.to_vec() } else { piece.iter().rev().copied().collect() }
    };
    for fork in &forks {
        // Two forks sharing a piece (one right after another): left as drawn.
        if gone.contains(&fork.trunk) || fork.branches.iter().any(|branch| gone.contains(branch)) {
            continue;
        }
        let trunk = &pieces[fork.trunk].0;
        let trunk_end = if trunk[0] == fork.at { trunk[trunk.len() - 1] } else { trunk[0] };
        let to_centre = from(trunk, trunk_end);
        for &branch in &fork.branches {
            let mut through = to_centre.clone();
            through.extend(&from(&pieces[branch].0, fork.at)[1..]);
            added.push((through, pieces[branch].1, to_centre.len() - 1));
        }
        gone.insert(fork.trunk);
        gone.extend(&fork.branches);
    }
    let mut out: Vec<(Vec<usize>, K)> =
        pieces.into_iter().enumerate().filter(|(index, _)| !gone.contains(index)).map(|(_, piece)| piece).collect();
    let mut centres = HashMap::new();
    for (piece, key, centre) in added {
        centres.insert(out.len(), centre);
        out.push((piece, key));
    }
    (out, centres)
}

/// A branch drawn from the trunk's end through the point where the road splits ([`branches_from_their_end`]),
/// `line` its points as drawn and `control` with its ends on its lanes: its points between moved across
/// as its start was (onto its lanes, beside the other branches) all the way to that point (`centre`, its
/// index), then easing back onto its own line by its end.
fn beside_to_centre(control: &[Point], line: &[Point], centre: usize) -> Vec<Point> {
    let n = line.len();
    let Some(way) = (n >= 3).then(|| direction(line[0], line[1])).flatten() else { return control.to_vec() };
    let shift = lanes::right_of(way, [control[0][0] - line[0][0], control[0][1] - line[0][1]]);
    let mut along = vec![0.0];
    for pair in line.windows(2) {
        along.push(along.last().unwrap() + distance(pair[0], pair[1]));
    }
    let (at_centre, total) = (along[centre], along[n - 1]);
    let shifts: Vec<f32> = along
        .iter()
        .map(|&at| {
            if at <= at_centre {
                shift
            } else {
                shift * (1.0 - (at - at_centre) / (total - at_centre).max(f32::EPSILON))
            }
        })
        .collect();
    let mut moved = geometry::offset_line_by(line, &shifts);
    moved[0] = control[0];
    moved[n - 1] = control[n - 1];
    moved
}

/// Where a prefab's drawn road splits at a centre point ([`CentreFork`]), in the prefab's space: to audit.
pub(crate) fn prefab_centre_forks(prefab: &Prefab) -> Vec<Point> {
    let pieces = drawn_pieces(prefab, &[]);
    centre_forks(prefab, &pieces).iter().map(|fork| prefab.map_points[fork.at].position).collect()
}

/// A piece from the prefab's end `from` to its end `to` (its control points `control`), where the prefab's
/// lanes at both ends are two carriageways (a motorway passing through it: a rest area's lay-bys, a sign
/// gantry): drawn as those two carriageways, each from its lanes at one end to its lanes at the other, at
/// its own width, so the motorway's carriageways either side run straight on through it. (Drawn as one
/// line down the middle, as its drawn graph has it, they'd run into a narrower road and out again.) Each
/// keeps the piece's shape, moved across onto its lanes. `None` for any other piece.
fn carriageway_pieces(prefab: &Prefab, from: usize, to: usize, control: &[Point]) -> Option<[(Vec<Point>, f32); 2]> {
    lanes::carriageways_at(prefab, from)?;
    lanes::carriageways_at(prefab, to)?;
    // One way through: where its lanes are as they go in at `a` and come out at `b`, and how many.
    let way = |a: usize, b: usize| -> Option<(Point, Point, usize)> {
        let going_in = lanes::lanes_driven(prefab, a, b, true);
        let coming_out = lanes::lanes_driven(prefab, b, a, false);
        if going_in.is_empty() || coming_out.is_empty() {
            return None;
        }
        Some((lanes::across(prefab, a, &going_in)?, lanes::across(prefab, b, &coming_out)?, going_in.len()))
    };
    let (there_from, there_to, there_lanes) = way(from, to)?;
    let (back_to, back_from, back_lanes) = way(to, from)?;
    // The piece moved across from its line onto `start` at its first point and `end` at its last, easing in
    // between (by how far along it each point is).
    let moved = |start: Point, end: Point| -> Vec<Point> {
        let n = control.len();
        let (Some(first_way), Some(last_way)) =
            (direction(control[0], control[1]), direction(control[n - 2], control[n - 1]))
        else {
            return vec![start, end];
        };
        let right_of = |way: Point, v: Point| lanes::right_of(way, v);
        let shift_start = right_of(first_way, [start[0] - control[0][0], start[1] - control[0][1]]);
        let shift_end = right_of(last_way, [end[0] - control[n - 1][0], end[1] - control[n - 1][1]]);
        let mut along = vec![0.0];
        for pair in control.windows(2) {
            along.push(along.last().unwrap() + distance(pair[0], pair[1]));
        }
        let total = along.last().copied().unwrap_or(0.0).max(f32::EPSILON);
        let shifts: Vec<f32> = along.iter().map(|at| shift_start + (shift_end - shift_start) * at / total).collect();
        let mut moved = geometry::offset_line_by(control, &shifts);
        // Exactly on the lanes at the ends, where the roads either side meet them.
        moved[0] = start;
        moved[n - 1] = end;
        moved
    };
    Some([
        (moved(there_from, there_to), there_lanes as f32 * LANE_WIDTH),
        (moved(back_from, back_to), back_lanes as f32 * LANE_WIDTH),
    ])
}

/// The lines a road of a look is drawn as: how far each lies to the right of the road's line, and how wide
/// it is (metres). Two, one per carriageway, where the look's directions lie apart (a motorway's central
/// reservation: lanes.rs); one, on its lanes' middle, otherwise.
fn drawn_lines(measured: Option<&lanes::LookOffset>, look: Option<&crate::world::RoadLook>) -> Vec<(f32, f32)> {
    if let Some(split) = measured.and_then(lanes::LookOffset::split) {
        return vec![
            (split.left, split.left_lanes as f32 * LANE_WIDTH),
            (split.right, split.right_lanes as f32 * LANE_WIDTH),
        ];
    }
    let width = look.map_or(2.0 * LANE_WIDTH, |look| look.width());
    vec![(measured.map_or(0.0, lanes::LookOffset::trusted), width)]
}

/// How much wider than the road at a prefab's end the pieces leaving it may be together and still be
/// taken to share it, metres.
const SHARED_ROAD_SLACK: f32 = 0.5;

/// A junction piece's curve: a point about every this many metres along a bend.
const JUNCTION_CURVE_STEP: f32 = 4.0;

/// A smooth curve through a piece's control points: cubic Hermite spans between them (as the game's road
/// curves, with tangents as long as each span), leaving along `start` and arriving along `end` (unit
/// directions of travel), through the points between along the line from the one before to the one after.
/// A straight span stays two points.
fn smooth_piece(control: &[Point], start: Point, end: Point) -> Vec<Point> {
    let n = control.len();
    let tangent = |i: usize| match i {
        0 => start,
        _ if i == n - 1 => end,
        _ => direction(control[i - 1], control[i + 1]).unwrap_or(start),
    };
    let mut curve = vec![control[0]];
    for i in 0..n - 1 {
        let (p0, p1) = (control[i], control[i + 1]);
        let length = distance(p0, p1);
        let (t0, t1) = (tangent(i), tangent(i + 1));
        let chord = [p1[0] - p0[0], p1[1] - p0[1]];
        let bend = geometry::turn(t0, chord).max(geometry::turn(t1, chord));
        let steps = if bend < 0.01 { 1 } else { ((length / JUNCTION_CURVE_STEP).ceil() as usize).clamp(2, 32) };
        let (m0, m1) = ([t0[0] * length, t0[1] * length], [t1[0] * length, t1[1] * length]);
        curve.extend((1..=steps).map(|step| {
            let s = step as f32 / steps as f32;
            let (s2, s3) = (s * s, s * s * s);
            let (h00, h10, h01, h11) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2, s3 - s2);
            [
                h00 * p0[0] + h10 * m0[0] + h01 * p1[0] + h11 * m1[0],
                h00 * p0[1] + h10 * m0[1] + h01 * p1[1] + h11 * m1[1],
            ]
        }));
    }
    curve
}

/// Segments (a, b, key) chained into pieces of points: on through every point where exactly two segments
/// with the same key meet and `breaks` doesn't say stop; anywhere else (an end, a crossing, a change of
/// key) a piece ends. A loop with nowhere to break comes out closed, its first point again at its end.
fn chain_segments<K: Copy + PartialEq>(
    count: usize,
    segments: &[(usize, usize, K)],
    breaks: impl Fn(usize) -> bool,
) -> Vec<(Vec<usize>, K)> {
    let mut incident = vec![Vec::new(); count];
    for (index, &(a, b, _)) in segments.iter().enumerate() {
        incident[a].push(index);
        incident[b].push(index);
    }
    let through =
        |p: usize| !breaks(p) && incident[p].len() == 2 && segments[incident[p][0]].2 == segments[incident[p][1]].2;
    let mut done = vec![false; segments.len()];
    let walk = |from: usize, first: usize, done: &mut Vec<bool>| {
        let (mut piece, mut at, mut segment) = (vec![from], from, first);
        loop {
            done[segment] = true;
            let (a, b, _) = segments[segment];
            at = if a == at { b } else { a };
            piece.push(at);
            if !through(at) {
                break;
            }
            match incident[at].iter().copied().find(|&next| !done[next]) {
                Some(next) => segment = next,
                None => break,
            }
        }
        (piece, segments[first].2)
    };
    let mut pieces = Vec::new();
    for p in (0..count).filter(|&p| !through(p)) {
        for &segment in &incident[p] {
            if !done[segment] {
                pieces.push(walk(p, segment, &mut done));
            }
        }
    }
    for segment in 0..segments.len() {
        if !done[segment] {
            pieces.push(walk(segments[segment].0, segment, &mut done));
        }
    }
    pieces
}

fn unit(v: Point) -> Option<Point> {
    let length = v[0].hypot(v[1]);
    (length > 1e-6).then(|| [v[0] / length, v[1] / length])
}

/// The unit direction from `a` to `b`, if they're apart.
fn direction(a: Point, b: Point) -> Option<Point> {
    unit([b[0] - a[0], b[1] - a[1]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefab::{NavCurve, PrefabNode, SpawnPoint, Trigger};

    fn line(points: &[Point], width: f32) -> RoadLine {
        let heights = vec![0.0; points.len()];
        RoadLine { points: points.to_vec(), heights, class: RoadClass::Local, width, minzoom: 0, level: 0 }
    }

    #[test]
    fn a_lane_dropped_at_a_node_narrows_away_along_the_road_after_it() {
        // Out of a three-lane road onto a two-lane road, 40 m long.
        let roads = vec![line(&[[-50.0, 0.0], [0.0, 0.0]], 12.0), line(&[[0.0, 0.0], [40.0, 0.0]], 8.0)];
        let roads = taper_lane_changes(roads, &mut 0);
        let after: Vec<&RoadLine> = roads.iter().filter(|road| road.points[0][0] >= 0.0).collect();
        // Wide where it leaves the wider line, its own width by its end, narrowing all the way.
        assert!(after.len() > 4);
        assert!(after[0].width > 11.0 && after.last().unwrap().width < 8.5);
        assert!(after.windows(2).all(|pair| pair[1].width < pair[0].width));
        // Still one line end to end.
        assert_eq!(after[0].points[0], [0.0, 0.0]);
        assert_eq!(after.last().unwrap().points.last(), Some(&[40.0, 0.0]));
        assert!(after.windows(2).all(|pair| pair[0].points.last() == pair[1].points.first()));
    }

    #[test]
    fn a_lane_opening_on_one_side_moves_the_road_over_as_it_widens() {
        // A two-lane one-way road, then a three-lane one, each on its lanes' middle: the lane opens on the
        // right (+z, heading +x), so the wider road's middle lies 2.25 m right of the narrower one's.
        let roads = vec![line(&[[-40.0, 0.0], [0.0, 0.0]], 8.0), line(&[[0.0, 2.25], [50.0, 2.25]], 12.0)];
        let mut count = 0;
        let roads = taper_lane_changes(roads, &mut count);
        assert_eq!(count, 1);
        let before: Vec<&RoadLine> = roads.iter().filter(|road| road.points[0][0] < 0.0).collect();
        // Its own width and middle where it starts; the wider road's where it meets it.
        assert_eq!(before[0].points[0], [-40.0, 0.0]);
        assert_eq!(before[0].width, 8.0);
        let last = before.last().unwrap();
        assert!(last.width > 11.5);
        assert!(distance(*last.points.last().unwrap(), [0.0, 2.25]) < 1e-4);
        // Its left edge carries on nearly straight (the map's lanes are 4 m, the game's 4.5).
        for road in &before {
            for point in &road.points {
                assert!((point[1] - road.width / 2.0 + 4.0).abs() < 0.3, "{point:?} {}", road.width);
            }
        }
        // Either way round.
        let reversed = vec![line(&[[0.0, 0.0], [-40.0, 0.0]], 8.0), line(&[[50.0, 2.25], [0.0, 2.25]], 12.0)];
        let roads = taper_lane_changes(reversed, &mut count);
        assert!(roads.iter().any(|road| road.width > 11.5 && distance(road.points[0], [0.0, 2.25]) < 1e-4));
    }

    #[test]
    fn a_road_forking_side_by_side_is_no_lane_change() {
        // A three-lane road carried on by a two-lane road on its left lanes and a slip road on its right.
        let roads = vec![
            line(&[[-50.0, 0.0], [0.0, 0.0]], 12.0),
            line(&[[0.0, -2.25], [40.0, -2.25]], 8.0),
            line(&[[0.0, 4.5], [40.0, 10.0]], 4.0),
        ];
        let mut count = 0;
        assert_eq!(taper_lane_changes(roads, &mut count).len(), 3);
        assert_eq!(count, 0);
    }

    #[test]
    fn lanes_beside_a_road_move_over_into_it() {
        // Three lanes, drawn as a two-lane line and a slip road's lane beside it, closing into a two-lane
        // road between them (heading +x; right is +z).
        let roads = vec![
            line(&[[-40.0, -2.25], [0.0, -2.25]], 8.0),
            line(&[[-40.0, 4.5], [0.0, 4.5]], 4.0),
            line(&[[0.0, 0.0], [50.0, 0.0]], 8.0),
        ];
        let mut count = 0;
        let roads = taper_lane_changes(roads, &mut count);
        assert_eq!(count, 2);
        // Each where it was at its far end, and within the road at the node.
        let end_of = |width: f32, from: Point| {
            let pieces: Vec<&RoadLine> =
                roads.iter().filter(|road| road.width == width && road.points[0][0] < 0.0).collect();
            let first = pieces.iter().find(|road| road.points[0] == from).expect("starts where it was");
            assert_eq!(first.points[0], from);
            *pieces.iter().map(|road| road.points.last().unwrap()).max_by(|a, b| a[0].total_cmp(&b[0])).unwrap()
        };
        assert!(distance(end_of(8.0, [-40.0, -2.25]), [0.0, 0.0]) < 1e-3);
        assert!(distance(end_of(4.0, [-40.0, 4.5]), [0.0, 2.0]) < 1e-3);
        // Carriageways on either side of a line with as many lanes stay where they are.
        let carriageways = vec![
            line(&[[-50.0, -6.5], [0.0, -6.5]], 8.0),
            line(&[[-50.0, 6.5], [0.0, 6.5]], 8.0),
            line(&[[0.0, 0.0], [40.0, 0.0]], 16.0),
        ];
        let mut none = 0;
        taper_lane_changes(carriageways, &mut none);
        assert_eq!(none, 0);
        // A road past a central reservation is no lane beside it.
        let apart = vec![line(&[[-50.0, 0.0], [0.0, 0.0]], 8.0), line(&[[0.0, 13.0], [40.0, 13.0]], 8.0)];
        assert_eq!(taper_lane_changes(apart, &mut count).len(), 2);
    }

    #[test]
    fn a_lane_branching_off_where_the_road_narrows_is_no_matter() {
        // Out of a filling station: its through lanes run straight on into the road, its forecourt's
        // lane joins at an angle.
        let roads = vec![
            line(&[[-50.0, 0.0], [0.0, 0.0]], 12.0),
            line(&[[-30.0, 30.0], [0.0, 0.0]], 4.0),
            line(&[[0.0, 0.0], [40.0, 0.0]], 8.0),
        ];
        let mut count = 0;
        let roads = taper_lane_changes(roads, &mut count);
        assert_eq!(count, 1);
        assert!(roads.iter().any(|road| road.points[0][0] >= 0.0 && road.width > 11.0));
    }

    #[test]
    fn a_road_hands_over_to_the_pieces_splitting_it() {
        // A 16 m road ending at [0, 0], carried on by two 8 m pieces side by side across its end, which
        // part ways (a roundabout's way in and way out).
        let roads = vec![
            line(&[[-50.0, 0.0], [0.0, 0.0]], 16.0),
            line(&[[0.0, -4.0], [16.0, -16.0]], 8.0),
            line(&[[16.0, 16.0], [0.0, 4.0]], 8.0),
        ];
        let mut count = 0;
        let roads = hand_over_splits(roads, &mut count);
        assert_eq!(count, 1);
        // The road stops half its width short; the pieces start that far back, still side by side.
        assert_eq!(roads[0].points.last(), Some(&[-8.0, 0.0]));
        assert_eq!(roads[1].points[0], [-8.0, -4.0]);
        assert_eq!(roads[2].points.last(), Some(&[-8.0, 4.0]));
        // One piece carrying a road on is no split.
        let one = vec![line(&[[-50.0, 0.0], [0.0, 0.0]], 16.0), line(&[[0.0, 0.0], [20.0, 0.0]], 8.0)];
        assert_eq!(hand_over_splits(one, &mut count)[0].points.last(), Some(&[0.0, 0.0]));
    }

    #[test]
    fn pieces_splitting_a_road_on_lanes_wider_than_it_is_drawn_start_inside_it() {
        // A 16 m road ending at [0, 0] whose lanes lie wider than it's drawn (Italy's two-lane roads into a
        // motorway: the lanes' middles 6.5 m either side), carried on by its two halves on their lanes.
        let roads = vec![
            line(&[[-50.0, 0.0], [0.0, 0.0]], 16.0),
            line(&[[0.0, -6.5], [15.0, -6.5]], 8.0),
            line(&[[15.0, 6.5], [0.0, 6.5]], 8.0),
        ];
        let mut count = 0;
        let roads = hand_over_splits(roads, &mut count);
        assert_eq!(count, 1);
        assert_eq!(roads[0].points.last(), Some(&[-8.0, 0.0]));
        // Where the road stops, they start inside it, filling it (not past its edges with its round end
        // between them), then ease out onto their lanes, there 7.5 m past the node: half the 15 m piece.
        let right = &roads[1].points;
        let left: Vec<Point> = roads[2].points.iter().rev().copied().collect();
        for (piece, side) in [(right, -1.0), (&left, 1.0)] {
            assert!(distance(piece[0], [-8.0, 4.0 * side]) < 1e-4, "{:?}", piece[0]);
            assert_eq!(piece.last(), Some(&[15.0, 6.5 * side]));
            for point in piece.iter().filter(|point| point[0] >= 7.5) {
                assert!((point[1] - 6.5 * side).abs() < 1e-4, "{point:?}");
            }
            // Outwards all the way, and smoothly: parallel to the road where it starts, no corner along.
            assert!(piece.windows(2).all(|pair| (pair[1][1] - pair[0][1]) * side >= -1e-4));
            let ways: Vec<Point> = piece.windows(2).filter_map(|pair| direction(pair[0], pair[1])).collect();
            assert!(ways[0][1].abs() < 0.05, "{:?}", ways[0]);
            for pair in ways.windows(2) {
                assert!(geometry::turn(pair[0], pair[1]) < 0.1, "{pair:?}");
            }
        }
        assert!(roads.iter().all(|road| road.heights.len() == road.points.len()));
    }

    fn road(points: &[Point], heights: &[f32], class: RoadClass, width: f32) -> RoadLine {
        RoadLine { points: points.to_vec(), heights: heights.to_vec(), class, width, minzoom: 0, level: 0 }
    }

    /// A motorway on the ground heading +z, and a local road over it on a bridge, 10 m up.
    fn bridge() -> Vec<RoadLine> {
        vec![
            road(&[[0.0, -60.0], [0.0, 60.0]], &[0.0, 0.0], RoadClass::Motorway, 16.0),
            road(&[[-60.0, 0.0], [60.0, 0.0]], &[10.0, 10.0], RoadClass::Local, 8.0),
        ]
    }

    fn of_class(roads: &[RoadLine], class: RoadClass) -> Vec<&RoadLine> {
        roads.iter().filter(|road| road.class == class).collect()
    }

    #[test]
    fn a_road_over_another_is_raised_just_where_it_passes_over_it() {
        let mut raised = [0; ROAD_LEVELS as usize - 1];
        let roads = road_levels(bridge(), &mut raised);
        assert_eq!(raised, [1, 0, 0]);
        // The motorway as it was.
        let motorway = of_class(&roads, RoadClass::Motorway);
        assert_eq!(motorway.len(), 1);
        assert_eq!(motorway[0].points, [[0.0, -60.0], [0.0, 60.0]]);
        assert_eq!(motorway[0].level, 0);
        // The bridge's road: raised where the two show on top of one another (as wide as they're drawn at
        // zoom 15, with their casings: 18.7 m apart), to the 4 m step; on the ground either side, still one
        // line end to end, as high as it was.
        let local = of_class(&roads, RoadClass::Local);
        assert_eq!(local.iter().map(|road| road.level).collect::<Vec<_>>(), [0, 1, 0]);
        let (start, end) = (local[1].points[0], *local[1].points.last().unwrap());
        assert!(distance(start, [-20.0, 0.0]) < 1e-3 && distance(end, [20.0, 0.0]) < 1e-3, "{start:?} {end:?}");
        assert_eq!(local[0].points[0], [-60.0, 0.0]);
        assert_eq!(local[2].points.last(), Some(&[60.0, 0.0]));
        assert!(local.windows(2).all(|pair| pair[0].points.last() == pair[1].points.first()));
        assert!(local.iter().all(|road| road.heights.len() == road.points.len()));
        assert!(local.iter().flat_map(|road| &road.heights).all(|&height| height == 10.0));
    }

    #[test]
    fn roads_level_where_they_meet_stay_on_the_ground() {
        // A slip road climbing beside the motorway from the ground to the bridge's road, joining it level.
        let mut roads = bridge();
        roads.push(road(&[[40.0, -80.0], [40.0, 0.0]], &[0.0, 10.0], RoadClass::Local, 4.0));
        let mut raised = [0; ROAD_LEVELS as usize - 1];
        let roads = road_levels(roads, &mut raised);
        assert_eq!(raised, [1, 0, 0]);
        assert!(roads.iter().filter(|road| road.points[0][0] == 40.0).all(|road| road.level == 0));
        // A motorway's carriageways on a hillside, one 5 m above the other, 12 m apart: side by side, never
        // over one another (though as wide as they're drawn further out, they touch).
        let hillside = vec![
            road(&[[-60.0, -6.0], [60.0, -6.0]], &[0.0, 0.0], RoadClass::Motorway, 8.0),
            road(&[[60.0, 6.0], [-60.0, 6.0]], &[5.0, 5.0], RoadClass::Motorway, 8.0),
        ];
        let mut none = [0; ROAD_LEVELS as usize - 1];
        assert_eq!(road_levels(hillside, &mut none).len(), 2);
        assert_eq!(none, [0, 0, 0]);
        // Roads crossing at a junction, the ground sloping a little.
        let crossing = vec![
            road(&[[0.0, -60.0], [0.0, 60.0]], &[0.0, 2.0], RoadClass::Motorway, 16.0),
            road(&[[-60.0, 0.0], [60.0, 0.0]], &[1.5, 0.5], RoadClass::Local, 8.0),
        ];
        let mut none = [0; ROAD_LEVELS as usize - 1];
        let roads = road_levels(crossing, &mut none);
        assert_eq!(none, [0, 0, 0]);
        assert_eq!(roads.len(), 2);
        assert_eq!(roads[1].points, [[-60.0, 0.0], [60.0, 0.0]]);
    }

    #[test]
    fn a_road_over_a_bridge_is_raised_above_it() {
        // A third road, 20 m up, over both where they cross.
        let mut roads = bridge();
        roads.push(road(&[[-50.0, -50.0], [50.0, 50.0]], &[20.0, 20.0], RoadClass::Trunk, 12.0));
        let mut raised = [0; ROAD_LEVELS as usize - 1];
        let roads = road_levels(roads, &mut raised);
        assert_eq!(raised, [1, 1, 0]);
        let top = of_class(&roads, RoadClass::Trunk);
        assert_eq!(top.iter().map(|road| road.level).collect::<Vec<_>>(), [0, 2, 0]);
        assert_eq!(of_class(&roads, RoadClass::Local)[1].level, 1);
    }

    #[test]
    fn heights_follow_the_lines_through_tapers() {
        let climbing = road(&[[0.0, 0.0], [40.0, 0.0]], &[0.0, 10.0], RoadClass::Local, 12.0);
        let pieces = tapered(climbing, (12.0, 0.0), (8.0, 0.0));
        assert!(pieces.len() > 4);
        for piece in &pieces {
            assert_eq!(piece.heights.len(), piece.points.len());
            for (point, height) in piece.points.iter().zip(&piece.heights) {
                assert!((height - point[0] / 4.0).abs() < 1e-4, "{point:?} {height}");
            }
        }
    }

    #[test]
    fn a_junctions_road_over_another_inside_it_is_raised() {
        // As the game's hw3-2_x_hw3-2: a road straight through between its ends 0 and 2, 5 m up, over another
        // between its ends 1 and 3, 4 m down, with no point where they meet (its map points all at 0).
        let node = |position: Point, direction: Point| PrefabNode {
            height: 0.0,
            position,
            direction,
            lanes: 2,
            curves_in: Vec::new(),
            curves_out: Vec::new(),
        };
        let point = |position, control_node, neighbour| MapPoint {
            position,
            kind: MapPointKind::Road(None),
            lane_offset: 0.0,
            area_color: 0,
            area_over_roads: false,
            navigation_only: false,
            control_node,
            neighbours: vec![neighbour],
        };
        let prefab = Prefab {
            nodes: vec![
                node([-40.0, 0.0], [1.0, 0.0]),
                node([0.0, -40.0], [0.0, 1.0]),
                node([40.0, 0.0], [-1.0, 0.0]),
                node([0.0, 40.0], [0.0, -1.0]),
            ],
            map_points: vec![
                point([-40.0, 0.0], Some(0), 2),
                point([0.0, -40.0], Some(1), 3),
                point([40.0, 0.0], Some(2), 0),
                point([0.0, 40.0], Some(3), 1),
            ],
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: Vec::new(),
        };
        let lines = prefab_road_lines(&prefab, &[], &[true; 4]);
        assert_eq!(lines.len(), 2);
        let world = [5.0, -4.0, 5.0, -4.0];
        let roads: Vec<RoadLine> = lines
            .into_iter()
            .map(|(points, [from, to], class, width)| {
                let heights = climbing(&points, world[from.unwrap()], world[to.unwrap()]);
                RoadLine { points, heights, class, width, minzoom: 0, level: 0 }
            })
            .collect();
        let mut raised = [0; ROAD_LEVELS as usize - 1];
        let roads = road_levels(roads, &mut raised);
        assert_eq!(raised, [1, 0, 0]);
        assert!(roads.iter().any(|road| road.level == 1 && road.heights.iter().all(|&height| height == 5.0)));
        assert!(roads.iter().filter(|road| road.heights[0] == -4.0).all(|road| road.level == 0));
    }

    #[test]
    fn a_junctions_roads_are_as_high_as_the_lanes_they_run_along() {
        let lane = |start: Point, end: Point, heights: [f32; 2]| {
            let way = direction(start, end).unwrap();
            NavCurve {
                start,
                end,
                heights,
                start_direction: way,
                end_direction: way,
                length: distance(start, end),
                next: Vec::new(),
            }
        };
        let prefab = |curves| Prefab {
            nodes: Vec::new(),
            map_points: Vec::new(),
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves,
        };
        // All about level: its roads are as high as its ends.
        assert!(lanes_by_height(&prefab(vec![lane([-40.0, 2.0], [40.0, 2.0], [0.0, 1.0])])).is_none());
        // A road 5 m up over another 4 m down (crossing at [0, 0]), and a slip road climbing from the lower
        // to the upper beside them.
        let lanes = lanes_by_height(&prefab(vec![
            lane([-40.0, 2.0], [40.0, 2.0], [5.0, 5.0]),
            lane([2.0, -40.0], [2.0, 40.0], [-4.0, -4.0]),
            lane([20.0, -40.0], [20.0, 40.0], [-4.0, 5.0]),
        ]))
        .unwrap();
        // Each road its own lane's height where they cross, not the other's.
        let upper = lane_heights(&lanes, &[[-40.0, 0.0], [0.0, 0.0], [40.0, 0.0]]);
        assert!(upper.iter().all(|&height| height.is_some_and(|height| (height - 5.0).abs() < 1e-4)), "{upper:?}");
        let lower = lane_heights(&lanes, &[[0.0, -40.0], [0.0, 0.0], [0.0, 40.0]]);
        assert!(lower.iter().all(|&height| height.is_some_and(|height| (height + 4.0).abs() < 1e-4)), "{lower:?}");
        // The slip road climbing as its lane does.
        let slip = lane_heights(&lanes, &[[22.0, -40.0], [22.0, 0.0], [22.0, 40.0]]);
        assert_eq!(slip.len(), 3);
        for (height, want) in slip.iter().zip([-4.0, 0.5, 5.0]) {
            assert!(height.is_some_and(|height| (height - want).abs() < 0.05), "{slip:?}");
        }
        // Far from any lane: none.
        assert_eq!(lane_heights(&lanes, &[[-30.0, 30.0], [-20.0, 30.0]]), [None, None]);
    }

    /// A quad from `x0` to `x1` and `z0` to `z1`, `h0` high along x0 and `h1` along x1.
    fn ground_quad(x0: f32, z0: f32, x1: f32, z1: f32, h0: f32, h1: f32) -> [Vertex; 4] {
        [[x0, h0, z0], [x1, h1, z0], [x1, h1, z1], [x0, h0, z1]]
    }

    /// Water 0 m high from x 0 to 100 and z 0 to 100, as four quads, one shape.
    fn sea_piece() -> WaterPiece {
        let quads = [(0.0, 0.0), (50.0, 0.0), (0.0, 50.0), (50.0, 50.0)]
            .map(|(x, z)| ground_quad(x, z, x + 50.0, z + 50.0, 0.0, 0.0))
            .to_vec();
        WaterPiece { quads, shapes: vec![vec![vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]]] }
    }

    /// A shape's extent along x.
    fn x_extent(shapes: &[Vec<Vec<Point>>]) -> (f32, f32) {
        shapes.iter().flatten().flatten().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[0]), hi.max(p[0])))
    }

    #[test]
    fn painted_water_shows_where_no_land_lies_over_it() {
        // Nothing over it: as it is.
        assert_eq!(shown_water(&sea_piece(), &[]), sea_piece().shapes);
        // Land 5 m up over it from x 60 on (and on beyond it): it ends there, to half a sample (where land
        // stops sheer, the water's edge falls between two samples); its own edges stay where they are (to
        // meet the next piece's).
        let shown = shown_water(&sea_piece(), &[ground_quad(60.0, -20.0, 140.0, 120.0, 5.0, 5.0)]);
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
        let beach = shown_water(&sea_piece(), &[ground_quad(0.0, 0.0, 100.0, 100.0, -5.0, 5.0)]);
        let (_, right) = x_extent(&beach);
        assert!((right - 52.5).abs() < 0.5, "{right}");
        // The sea bed under it hides nothing.
        let bed = shown_water(&sea_piece(), &[ground_quad(-20.0, -20.0, 120.0, 120.0, -10.0, -10.0)]);
        assert!((shape_area(&bed[0]) - 10_000.0).abs() < 10.0);
        // A lake in a field all round it, the field's ground running on under the lake: it shows whole.
        let field = [ground_quad(-50.0, -50.0, 150.0, 150.0, -0.5, -0.5)];
        assert!((shape_area(&shown_water(&sea_piece(), &field)[0]) - 10_000.0).abs() < 10.0);
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
        let lake = shown_water(&sea_piece(), &banks);
        assert_eq!(lake.len(), 1);
        assert!((shape_area(&lake[0]) - 6400.0).abs() < 400.0, "{}", shape_area(&lake[0]));
        // A strip of sea a kilometre square under a town, but for a block 20 m square in its middle where
        // nothing lies over it: a crumb, left out.
        let sea = WaterPiece {
            quads: vec![ground_quad(0.0, 0.0, 1000.0, 1000.0, 0.0, 0.0)],
            shapes: vec![vec![vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0], [0.0, 1000.0]]]],
        };
        let town = [
            ground_quad(-10.0, -10.0, 490.0, 1010.0, 20.0, 20.0),
            ground_quad(510.0, -10.0, 1010.0, 1010.0, 20.0, 20.0),
            ground_quad(490.0, -10.0, 510.0, 490.0, 20.0, 20.0),
            ground_quad(490.0, 510.0, 510.0, 1010.0, 20.0, 20.0),
        ];
        assert!(shown_water(&sea, &town).is_empty());
        // The same block reaching the sea's own edge, open past it (a harbour): kept.
        let harbour = [
            ground_quad(-10.0, -10.0, 490.0, 1010.0, 20.0, 20.0),
            ground_quad(510.0, -10.0, 1010.0, 1010.0, 20.0, 20.0),
            ground_quad(490.0, -10.0, 510.0, 490.0, 20.0, 20.0),
        ];
        let shown = shown_water(&sea, &harbour);
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
        assert!((water.area() - 10_000.0).abs() < 1.0);
    }

    #[test]
    fn a_junctions_roads_climb_between_the_ends_they_run_between() {
        let heights = climbing(&[[0.0, 0.0], [10.0, 0.0], [10.0, 30.0]], 2.0, 10.0);
        assert_eq!(heights, [2.0, 4.0, 10.0]);
        // Where one ends inside, as high as the ends around it, the nearer the more.
        let ends = [([0.0, 0.0], 1.0), ([10.0, 0.0], 3.0)];
        assert_eq!((blended(&ends, [0.0, 0.0]), blended(&ends, [10.0, 0.0])), (1.0, 3.0));
        assert!((blended(&ends, [5.0, 0.0]) - 2.0).abs() < 1e-5);
        assert!(blended(&ends, [2.0, 0.0]) < 1.5);
        assert_eq!(blended(&[], [2.0, 0.0]), 0.0);
    }

    #[test]
    fn junctions_and_big_changes_are_no_lane_change() {
        // Two lines carry straight on from the road's end (a junction splitting a road), or the one that
        // does is far wider.
        let fork = vec![
            line(&[[-50.0, 0.0], [0.0, 0.0]], 12.0),
            line(&[[-50.0, 5.0], [0.0, 0.0]], 12.0),
            line(&[[0.0, 0.0], [40.0, 0.0]], 8.0),
        ];
        assert_eq!(taper_lane_changes(fork, &mut 0).len(), 3);
        let wide = vec![line(&[[-50.0, 0.0], [0.0, 0.0]], 24.0), line(&[[0.0, 0.0], [40.0, 0.0]], 8.0)];
        assert_eq!(taper_lane_changes(wide, &mut 0).len(), 2);
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

    /// A T crossing: three end points around a middle one, all of the navigation graph or all drawn.
    fn crossing(navigation_only: bool, offset: usize) -> Vec<MapPoint> {
        let point = |position, control_node, neighbours| MapPoint {
            position,
            kind: MapPointKind::Road(None),
            lane_offset: 0.0,
            area_color: 0,
            area_over_roads: false,
            navigation_only,
            control_node,
            neighbours,
        };
        vec![
            point([-10.0, 0.0], Some(0), vec![offset + 3]),
            point([0.0, -10.0], Some(1), vec![offset + 3]),
            point([0.0, 10.0], Some(2), vec![offset + 3]),
            point([0.0, 0.0], None, vec![offset, offset + 1, offset + 2]),
        ]
    }

    #[test]
    fn draws_a_crossings_navigation_graph_only_when_it_has_no_drawn_one() {
        let prefab = |map_points| Prefab {
            nodes: Vec::new(),
            map_points,
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: Vec::new(),
        };
        // Only a navigation graph: the game's map draws it, and so does ours.
        assert_eq!(prefab_roads(&prefab(crossing(true, 0)), &[], &[]).len(), 3);
        // A drawn graph and the navigation graph repeating it: the drawn one only.
        let mut both = crossing(false, 0);
        both.extend(crossing(true, 4));
        let roads = prefab_roads(&prefab(both), &[], &[]);
        assert_eq!(roads.len(), 3);
    }

    #[test]
    fn joins_an_end_the_drawn_roads_miss_through_the_navigation_graph() {
        // A filling station: its drawn roads run from end 0 round to end 2 (the forecourt lanes); the road on
        // past it to end 1 is in the navigation graph only.
        let point = |position, control_node, navigation_only, neighbours| MapPoint {
            position,
            kind: MapPointKind::Road(None),
            lane_offset: 0.0,
            area_color: 0,
            area_over_roads: false,
            navigation_only,
            control_node,
            neighbours,
        };
        let map_points = vec![
            point([-50.0, 0.0], Some(0), false, vec![1]),
            point([0.0, 30.0], None, false, vec![0, 2]),
            point([50.0, 30.0], Some(2), false, vec![1]),
            point([-50.0, 0.0], Some(0), true, vec![6]),
            point([50.0, 0.0], Some(1), true, vec![6]),
            point([50.0, 30.0], Some(2), true, vec![6]),
            point([40.0, 5.0], None, true, vec![3, 4, 5]),
        ];
        let prefab = Prefab {
            nodes: Vec::new(),
            map_points,
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: Vec::new(),
        };
        // The forecourt lanes, and the way from end 1 to the nearest end they reach (end 0).
        assert_eq!(prefab_road_points(&prefab), [true, true, true, true, true, false, true]);
        // Each a piece, from end to end.
        assert_eq!(prefab_roads(&prefab, &[], &[]).len(), 2);
    }

    fn road_point(
        position: Point,
        control_node: Option<usize>,
        navigation_only: bool,
        neighbours: Vec<usize>,
    ) -> MapPoint {
        MapPoint {
            position,
            kind: MapPointKind::Road(None),
            lane_offset: 0.0,
            area_color: 0,
            area_over_roads: false,
            navigation_only,
            control_node,
            neighbours,
        }
    }

    #[test]
    fn draws_the_road_past_a_filling_station_whose_drawn_roads_are_its_forecourt() {
        // As gas_small_road_01_it: forecourt lanes curving out between the ends, a marker for its icon, and
        // the road straight past only in the navigation graph.
        let mut marker = road_point([-69.0, -18.7], None, false, Vec::new());
        marker.kind = MapPointKind::Marker;
        let map_points = vec![
            road_point([-69.0, -16.1], Some(1), false, vec![1]),
            road_point([-18.2, -43.4], None, false, vec![0, 2]),
            road_point([69.1, -16.0], Some(0), false, vec![1]),
            marker,
            road_point([69.1, -13.8], Some(0), true, vec![5]),
            road_point([-69.0, -13.8], Some(1), true, vec![4]),
        ];
        let prefab = Prefab {
            nodes: Vec::new(),
            map_points,
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: Vec::new(),
        };
        assert_eq!(prefab_road_points(&prefab), [true, true, true, false, true, true]);
        assert_eq!(prefab_roads(&prefab, &[], &[]).len(), 2);
    }

    #[test]
    fn leaves_out_a_navigation_link_a_drawn_road_already_follows() {
        // As it_gas1_fork_tmpl: the drawn road and the navigation link are the same 8 m.
        let map_points = vec![
            road_point([0.0, 0.0], Some(0), false, vec![1]),
            road_point([0.0, -8.0], Some(1), false, vec![0]),
            road_point([0.0, 0.0], Some(0), true, vec![3]),
            road_point([0.0, -8.0], Some(1), true, vec![2]),
        ];
        let prefab = Prefab {
            nodes: Vec::new(),
            map_points,
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: Vec::new(),
        };
        assert_eq!(prefab_road_points(&prefab), [true, true, false, false]);
    }

    /// A motorway fork as it_hw3_fork_tmpl: six lanes in at node 0, three out at each of nodes 1 and 2;
    /// drawn twice, straight from end to end and through a centre point.
    fn fork() -> Prefab {
        let lanes = |lanes, position, control_node, neighbours| {
            let mut point = road_point(position, Some(control_node), false, neighbours);
            point.kind = MapPointKind::Road(Some(lanes));
            point
        };
        let mut centre = road_point([0.0, -10.0], None, false, vec![3, 4, 5]);
        centre.kind = MapPointKind::Road(Some(3));
        let node = |position, direction, lanes| PrefabNode {
            height: 0.0,
            position,
            direction,
            lanes,
            curves_in: Vec::new(),
            curves_out: Vec::new(),
        };
        Prefab {
            nodes: vec![
                node([0.0, 0.0], [0.0, 1.0], 6),
                node([11.0, -15.0], [0.0, -1.0], 3),
                node([-11.0, -15.0], [0.0, -1.0], 3),
            ],
            map_points: vec![
                lanes(3, [-11.0, -15.0], 2, vec![2]),
                lanes(3, [11.0, -15.0], 1, vec![2]),
                lanes(6, [0.0, 0.0], 0, vec![0, 1]),
                lanes(3, [11.0, -15.0], 1, vec![6]),
                lanes(3, [-11.0, -15.0], 2, vec![6]),
                lanes(6, [0.0, 0.0], 0, vec![6]),
                centre,
            ],
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: Vec::new(),
        }
    }

    #[test]
    fn a_motorway_splits_into_its_carriageways_straight_on_its_lanes_not_its_drawn_v() {
        // The game's motorway split (it_hw2_fork): the drawn graph is a V from the motorway's line (node 0) to
        // each carriageway's line (6.5 m to either side); the lanes run straight, 4.25 and 8.75 m out.
        let node = |position, direction, curves_in, curves_out| PrefabNode {
            height: 0.0,
            position,
            direction,
            lanes: 2,
            curves_in,
            curves_out,
        };
        let lane = |x: f32, from: f32, to: f32| {
            let way = [0.0, (to - from).signum()];
            NavCurve {
                heights: [0.0, 0.0],
                start: [x, from],
                end: [x, to],
                start_direction: way,
                end_direction: way,
                length: 15.0,
                next: vec![],
            }
        };
        let point = |position, control, neighbours, lanes| {
            let mut point = road_point(position, Some(control), false, neighbours);
            point.kind = MapPointKind::Road(Some(lanes));
            point
        };
        let split = Prefab {
            nodes: vec![
                node([0.0, 0.0], [0.0, 1.0], vec![1, 0], vec![2, 3]),
                node([6.5, 15.0], [0.0, -1.0], vec![2, 3], vec![]),
                node([-6.5, 15.0], [0.0, -1.0], vec![], vec![1, 0]),
            ],
            map_points: vec![
                point([6.5, 15.0], 1, vec![2], 2),
                point([-6.5, 15.0], 2, vec![2], 2),
                point([0.0, 0.0], 0, vec![0, 1], 4),
            ],
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: vec![lane(-8.75, 0.0, 15.0), lane(-4.25, 0.0, 15.0), lane(4.25, 15.0, 0.0), lane(8.75, 15.0, 0.0)],
        };
        let pieces = prefab_roads(&split, &[], &[true, true, true]);
        assert_eq!(pieces.len(), 2);
        // Each carriageway straight on from the motorway, 6.5 m to its side, all the way.
        for (piece, _, _) in &pieces {
            let x = piece[0][0];
            assert!((x.abs() - 6.5).abs() < 1e-4, "{piece:?}");
            assert!(piece.iter().all(|p| (p[0] - x).abs() < 1e-4), "{piece:?}");
        }
    }

    #[test]
    fn a_road_splitting_at_a_centre_point_goes_on_as_its_branches_side_by_side() {
        // The game's it_road1_hw_split: a two-way road (one lane each way, 2.25 m out; heading +z, right is
        // -x) dividing into a loop's way in and way out, its drawn graph through a centre point 12 m in.
        let node = |position, direction, lanes, curves_in, curves_out| PrefabNode {
            height: 0.0,
            position,
            direction,
            lanes,
            curves_in,
            curves_out,
        };
        let lane = |start: Point, end: Point| NavCurve {
            heights: [0.0, 0.0],
            start,
            end,
            start_direction: [0.0, 1.0],
            end_direction: [0.0, 1.0],
            length: 30.0,
            next: vec![],
        };
        let split = Prefab {
            nodes: vec![
                node([0.0, 0.0], [0.0, 1.0], 2, vec![0], vec![1]),
                node([-10.0, 30.0], [0.3, -0.95], 1, vec![], vec![0]),
                node([10.0, 30.0], [-0.3, -0.95], 1, vec![1], vec![]),
            ],
            map_points: vec![
                road_point([0.0, 0.0], Some(0), false, vec![3]),
                road_point([-10.0, 30.0], Some(1), false, vec![3]),
                road_point([10.0, 30.0], Some(2), false, vec![3]),
                road_point([0.0, 12.0], None, false, vec![0, 1, 2]),
            ],
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: vec![lane([-2.25, 0.0], [-10.0, 30.0]), lane([10.0, 30.0], [2.25, 0.0])],
        };
        let pieces = prefab_roads(&split, &[], &[true, true, true]);
        // The way in and the way out, no stub of the whole road between.
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        for (piece, _, width) in &pieces {
            assert_eq!(*width, 4.0);
            // From the road's end, on its own lane there.
            let (start, end) = (piece[0], piece[piece.len() - 1]);
            let at_road = if start[1].abs() < 1e-4 { start } else { end };
            assert!(at_road[1].abs() < 1e-4 && (at_road[0].abs() - 2.25).abs() < 1e-3, "{piece:?}");
            // Beside the other, on its own side, up to the centre point: never back on the road's line.
            let side = at_road[0].signum();
            for p in piece.iter().filter(|p| p[1] <= 12.0) {
                assert!(p[0] * side > 1.0, "{p:?} in {piece:?}");
            }
        }
    }

    #[test]
    fn a_motorway_runs_straight_through_a_rest_area_on_its_carriageways() {
        // The game's it_2lane_restarea_wide: 58 m of motorway with lay-bys, two lanes each way 4.25 and
        // 8.75 m out (heading +z, right is -x), and a drawn graph of one 4-lane piece down the middle.
        let node = |position, direction, curves_in, curves_out| PrefabNode {
            height: 0.0,
            position,
            direction,
            lanes: 4,
            curves_in,
            curves_out,
        };
        let lane = |x: f32, from: f32, to: f32| {
            let way = [0.0, (to - from).signum()];
            NavCurve {
                heights: [0.0, 0.0],
                start: [x, from],
                end: [x, to],
                start_direction: way,
                end_direction: way,
                length: 58.0,
                next: vec![],
            }
        };
        let rest_area = Prefab {
            nodes: vec![
                node([0.0, 0.0], [0.0, 1.0], vec![0, 1], vec![2, 3]),
                node([0.0, 58.0], [0.0, -1.0], vec![2, 3], vec![0, 1]),
            ],
            map_points: vec![
                road_point([0.0, 58.0], Some(1), false, vec![1]),
                road_point([0.0, 0.0], Some(0), false, vec![0]),
            ],
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: vec![lane(-4.25, 0.0, 58.0), lane(-8.75, 0.0, 58.0), lane(4.25, 58.0, 0.0), lane(8.75, 58.0, 0.0)],
        };
        let pieces = prefab_roads(&rest_area, &[], &[true, true]);
        // Two carriageways, 8 m each, 6.5 m either side, straight the whole way (as the roads either side).
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        let mut sides: Vec<f32> = pieces.iter().map(|(piece, _, _)| piece[0][0]).collect();
        sides.sort_by(f32::total_cmp);
        assert!((sides[0] + 6.5).abs() < 1e-4 && (sides[1] - 6.5).abs() < 1e-4, "{sides:?}");
        for (piece, _, width) in &pieces {
            assert_eq!(*width, 8.0);
            let x = piece[0][0];
            assert!(piece.iter().all(|p| (p[0] - x).abs() < 1e-4), "{piece:?}");
            let (start, end) = (piece[0][1].min(piece[piece.len() - 1][1]), piece[0][1].max(piece[piece.len() - 1][1]));
            assert_eq!((start, end), (0.0, 58.0));
        }
    }

    #[test]
    fn draws_a_prefabs_roads_once_where_it_has_them_twice() {
        // The graph straight from end to end, not the one through a centre point beside it.
        assert_eq!(prefab_road_points(&fork()), [true, true, true, false, false, false, false]);
        // Identical copies: the first.
        let mut twice = crossing(false, 0);
        twice.extend(crossing(false, 4));
        let prefab = Prefab {
            nodes: Vec::new(),
            map_points: twice,
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: Vec::new(),
        };
        assert_eq!(prefab_road_points(&prefab), [true, true, true, true, false, false, false, false]);
    }

    #[test]
    fn a_fork_splits_the_road_into_its_carriageways_curving_on_from_it() {
        let roads = prefab_roads(&fork(), &[], &[true; 3]);
        assert_eq!(roads.len(), 2);
        for (line, _, width) in &roads {
            assert_eq!(*width, 3.0 * LANE_WIDTH);
            // From the six-lane road's end, each carriageway in its own half of it...
            let start = if line[0][1] == 0.0 { line[0] } else { line[line.len() - 1] };
            let end = if line[0][1] == 0.0 { line[line.len() - 1] } else { line[0] };
            assert!((start[0].abs() - 6.0).abs() < 1e-4 && start[0].signum() == end[0].signum(), "{line:?}");
            // ... leaving straight along the road (not at an angle to it), and smoothly: a curve, not a line.
            assert!(line.len() > 4);
            let first_step = if line[0][1] == 0.0 { line[1] } else { line[line.len() - 2] };
            assert!((first_step[0] - start[0]).abs() < 0.3 * (first_step[1] - start[1]).abs(), "{line:?}");
        }
    }

    #[test]
    fn chains_segments_into_pieces_between_ends_crossings_and_changes() {
        // 0 - 1 - 2 - 3 (3 a crossing with 4 and 5), and 5 - 6 of another key; then a loop 7 - 8 - 9.
        let segments = [
            (0, 1, 'a'),
            (2, 1, 'a'),
            (2, 3, 'a'),
            (3, 4, 'a'),
            (3, 5, 'a'),
            (5, 6, 'b'),
            (7, 8, 'c'),
            (8, 9, 'c'),
            (9, 7, 'c'),
        ];
        let pieces = chain_segments(10, &segments, |p| p == 0);
        let pieces: Vec<Vec<usize>> = pieces.into_iter().map(|(piece, _)| piece).collect();
        assert_eq!(pieces, [vec![0, 1, 2, 3], vec![3, 4], vec![3, 5], vec![5, 6], vec![7, 8, 9, 7]]);
    }

    /// The classes of a network's segments, from seeds (point, class) and links (a, b, metres).
    /// Links (a, b, metres, lanes).
    fn segment_classes(links: &[(usize, usize, f32, u32)], seeds: &[(usize, RoadClass)]) -> Vec<Option<RoadClass>> {
        let count = links.iter().map(|&(a, b, _, _)| a.max(b) + 1).max().unwrap_or(0);
        let mut network = vec![Vec::new(); count];
        for &(a, b, length, _) in links {
            network[a].push((b, length));
            network[b].push((a, length));
        }
        let seeds: Vec<(usize, usize)> = seeds.iter().map(|&(point, class)| (point, rank(class))).collect();
        let distances = nearest_distances(&network, &seeds);
        links.iter().map(|&(a, b, _, lanes)| segment_class(&distances[a], &distances[b], lanes)).collect()
    }

    #[test]
    fn a_motorway_keeps_its_class_through_a_chain_of_junctions() {
        // A motorway (two lanes) through junctions joined end to end, its own roads only further on:
        // 0 - 1 - 2 - 3, 60 m a step; a slip road (one lane) leaves from 2 to a local road's end (4) 40 m
        // off, nearer than either.
        let classes = segment_classes(
            &[(0, 1, 60.0, 2), (1, 2, 60.0, 2), (2, 3, 60.0, 2), (2, 4, 40.0, 1)],
            &[(0, RoadClass::Motorway), (3, RoadClass::Motorway), (4, RoadClass::Local)],
        );
        let (motorway, local) = (Some(RoadClass::Motorway), Some(RoadClass::Local));
        assert_eq!(classes, [motorway, motorway, motorway, local]);
    }

    #[test]
    fn a_slip_road_off_a_motorway_stays_a_slip_road() {
        // A motorway exit with roads at all its ends: the motorway on (1 - 2) and the slip road (0 - 2).
        let seeds = [(0, RoadClass::Local), (1, RoadClass::Motorway), (2, RoadClass::Motorway)];
        let classes = segment_classes(&[(1, 2, 40.0, 2), (0, 2, 42.0, 1)], &seeds);
        assert_eq!(classes, [Some(RoadClass::Motorway), Some(RoadClass::Local)]);
        // No road near at all: no class (the caller draws a local road).
        assert_eq!(segment_classes(&[(0, 1, 10.0, 2)], &[]), [None]);
    }

    #[test]
    fn a_sites_lanes_are_local_roads_whatever_leads_to_them() {
        // A filling station's lane between ends nearest a motorway ramp.
        let lane =
            vec![road_point([0.0, 0.0], Some(0), false, vec![1]), road_point([30.0, 0.0], Some(1), false, vec![0])];
        let mut station = Prefab {
            nodes: Vec::new(),
            map_points: lane,
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: Vec::new(),
        };
        let near_ramp = [[10.0, f32::INFINITY, 90.0, f32::INFINITY, f32::INFINITY]; 2];
        assert_eq!(prefab_roads(&station, &near_ramp, &[])[0].1, RoadClass::Motorway);
        station.spawn_points.push(SpawnPoint { position: [15.0, 5.0], kind: SpawnKind::Fuel });
        assert_eq!(prefab_roads(&station, &near_ramp, &[])[0].1, RoadClass::Local);
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
        assert_eq!(grid.nearest([0.0, 0.0], 500.0), Some(1));
        // Across cells, and nothing beyond reach.
        assert_eq!(grid.nearest([-600.0, 0.0], 500.0), Some(2));
        assert_eq!(grid.nearest([3000.0, 0.0], 500.0), None);
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
    fn rests_where_the_game_offers_rest_not_at_every_parking_space() {
        let rest = |x: f32| Trigger { position: [x, 0.0], action: token::string_to_token(REST_TRIGGER).unwrap() };
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
