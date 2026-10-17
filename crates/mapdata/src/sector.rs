//! Map sectors: `map/<map>/sec±XXXX±ZZZZ.base` and `.aux`, one pair per square of the world, holding the
//! map's items (roads, prefabs, map areas, cities in `.base`; terrain, models and other scenery in `.aux`)
//! and the nodes they hang on. Both files have the same layout.
//!
//! Only the current format is read ([`SUPPORTED_VERSION`]): every item's size depends on the version, so a
//! sector in any other one is refused rather than misread. Items the Maps app draws are decoded
//! ([`ItemData`]); the rest are only measured, to step over them.

use std::fmt;

use crate::reader::{OutOfBounds, Reader};
use crate::terrain::{QuadGrid, read_quad_grid};

/// The sector format version of ETS2/ATS 1.5x as of 2026 (the first four bytes of every sector file).
pub const SUPPORTED_VERSION: u32 = 907;

/// Where an item's own fields start: after its type, uid and bounding box.
const ITEM_HEADER_SIZE: usize = 0x34;
const NODE_SIZE: usize = 0x38;
/// A vegetation sphere of terrain, bezier patch, road and prefab items.
pub(crate) const VEGETATION_SPHERE_SIZE: usize = 0x14;
/// A vegetation setting of a terrain or road side: name, density, distance, scale, from, to.
pub(crate) const ROAD_VEGETATION_SIZE: usize = 8 + 2 + 1 + 1 + 2 + 2;

#[derive(Debug, PartialEq)]
pub enum SectorError {
    UnsupportedVersion(u32),
    /// An item type this reader doesn't know the layout of, so nothing after it can be found.
    UnknownItemType {
        item_type: u32,
        offset: usize,
    },
    OutOfBounds(OutOfBounds),
    /// A `.data` entry for an item the sector's `.base` doesn't have, so its size can't be known.
    UnknownDataOwner {
        uid: u64,
        offset: usize,
    },
    /// Everything was read but the file goes on: some item's size was misjudged.
    TrailingBytes {
        read: usize,
        len: usize,
    },
}

impl fmt::Display for SectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(f, "sector version {version} (only {SUPPORTED_VERSION} is supported)")
            }
            Self::UnknownItemType { item_type, offset } => write!(f, "unknown item type {item_type} at 0x{offset:x}"),
            Self::OutOfBounds(error) => error.fmt(f),
            Self::UnknownDataOwner { uid, offset } => write!(f, "data for unknown item {uid:x} at 0x{offset:x}"),
            Self::TrailingBytes { read, len } => write!(f, "read 0x{read:x} of 0x{len:x} bytes"),
        }
    }
}

impl std::error::Error for SectorError {}

impl From<OutOfBounds> for SectorError {
    fn from(error: OutOfBounds) -> Self {
        Self::OutOfBounds(error)
    }
}

#[derive(Debug)]
pub struct Sector {
    pub items: Vec<Item>,
    pub nodes: Vec<Node>,
}

#[derive(Debug)]
pub struct Item {
    pub uid: u64,
    /// The game's item type number (see [`item_type`]).
    pub item_type: u32,
    /// Bounding box on the ground plane, in metres: (min x, min z, max x, max z).
    pub bounds: [f32; 4],
    pub data: ItemData,
}

/// The game's item type numbers.
pub mod item_type {
    pub const TERRAIN: u32 = 1;
    pub const BUILDING: u32 = 2;
    pub const ROAD: u32 = 3;
    pub const PREFAB: u32 = 4;
    pub const MODEL: u32 = 5;
    pub const COMPANY: u32 = 6;
    pub const SERVICE: u32 = 7;
    pub const CUT_PLANE: u32 = 8;
    pub const CITY: u32 = 12;
    pub const MAP_OVERLAY: u32 = 18;
    pub const FERRY: u32 = 19;
    pub const GARAGE: u32 = 22;
    pub const TRIGGER: u32 = 34;
    pub const FUEL_PUMP: u32 = 35;
    pub const ROAD_SIDE: u32 = 36;
    pub const BUS_STOP: u32 = 37;
    pub const TRAFFIC_RULE: u32 = 38;
    pub const BEZIER_PATCH: u32 = 39;
    pub const TRAJECTORY: u32 = 41;
    pub const MAP_AREA: u32 = 42;
    pub const CURVE: u32 = 44;
    pub const CUTSCENE: u32 = 46;
    pub const VISIBILITY_AREA: u32 = 48;
    pub const MOVER: u32 = 9;
    pub const ENVIRONMENT_AREA: u32 = 11;
    pub const HINGE: u32 = 13;
    pub const ANIMATED_MODEL: u32 = 15;
    pub const SOUND: u32 = 21;
    pub const CAMERA_POINT: u32 = 23;
    pub const WALKER: u32 = 28;
    pub const COMPOUND: u32 = 40;
    pub const FAR_MODEL: u32 = 43;
    pub const CAMERA_PATH: u32 = 45;
    pub const HOOKUP: u32 = 47;
    pub const GATE: u32 = 49;
}

/// Visibility shared by the items the map shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapFlags {
    /// Not on the map at all (or only in the game's navigation, for map areas).
    pub hidden: bool,
    /// Only on the map once the player has driven there (secret roads and their places).
    pub secret: bool,
    /// Which map DLC the item belongs to (0 for the base map), for showing only what the player owns.
    pub dlc_guard: u8,
}

#[derive(Debug)]
pub enum ItemData {
    Road {
        flags: MapFlags,
        /// The road's look (`road_look.*` in `def/world/road_look*.sii`), a token.
        look: u64,
        start_node: u64,
        end_node: u64,
        /// The shape of the ground beside it, right then left (its quads are in the sector's `.data`).
        relief: [Relief; 2],
    },
    Prefab {
        flags: MapFlags,
        /// The prefab model (`prefab.*` in `def/world/prefab*.sii`), a token.
        model: u64,
        variant: u64,
        nodes: Vec<u64>,
        /// Which of the prefab's own nodes `nodes[0]` is (it places and turns the model).
        origin: u8,
    },
    MapArea {
        flags: MapFlags,
        /// Drawn above roads rather than below.
        draw_over: bool,
        /// Its polygon, as node uids in order.
        nodes: Vec<u64>,
        /// The game's map colours: 0 road, 1 light, 2 dark, 3 green… (the UI's palette decides).
        color: u32,
    },
    City {
        hidden: bool,
        /// The city (`city.*` in `def/city*.sii`), a token.
        city: u64,
        width: f32,
        height: f32,
        node: u64,
    },
    Company {
        dlc_guard: u8,
        /// The company (`company.permanent.*`), a token.
        company: u64,
        prefab: u64,
        node: u64,
    },
    Ferry {
        /// A train terminal rather than a port.
        train: bool,
        /// The port (`ferry.*` in `def/ferry*.sii`), a token.
        port: u64,
        node: u64,
    },
    MapOverlay {
        flags: MapFlags,
        /// 1 parking, 4 landmark, otherwise a named icon.
        kind: u8,
        /// The icon, a token, for the named kind.
        icon: u64,
        node: u64,
    },
    /// Ground along a curve between two nodes, to its right and left.
    Terrain {
        start_node: u64,
        end_node: u64,
        right: TerrainSide,
        left: TerrainSide,
        /// Right then left.
        relief: [Relief; 2],
    },
    /// Ground shaped by a 4×4 grid of bezier control points around a node.
    BezierPatch {
        node: u64,
        /// Control points relative to the node, metres, `[x][z]`.
        control_points: [[[f32; 3]; 4]; 4],
        grid: QuadGrid,
    },
    /// Everything else: stepped over.
    Other,
}

/// One side of a terrain strip (or of a road): how far it reaches and its quads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainSide {
    /// Metres from the centre line (from the road's edge, for roads).
    pub size: f32,
    pub grid: QuadGrid,
}

/// The shape of the ground across a terrain side: a terrain profile (its rows' widths and heights) scaled
/// by a coefficient, raised by an offset.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Relief {
    /// `terrain.*` in `def/world/terrain_profile*.sii`, a token.
    pub profile: u64,
    pub coefficient: f32,
    /// Metres (roads only).
    pub height_offset: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub uid: u64,
    /// Position in metres: x east, y up, z south.
    pub position: [f32; 3],
    /// Orientation quaternion as stored: (w, x, y, z).
    pub rotation: [f32; 4],
    /// The items before and after the node along a road (0 if none).
    pub backward_item: u64,
    pub forward_item: u64,
    pub flags: u32,
}

/// Reads a whole `.base` or `.aux` sector file.
pub fn parse(data: &[u8]) -> Result<Sector, SectorError> {
    let mut r = Reader::new(data);
    let version = r.u32()?;
    if version != SUPPORTED_VERSION {
        return Err(SectorError::UnsupportedVersion(version));
    }
    r.skip(0x10 - 4)?;
    let mut items = Vec::new();
    let mut nodes = Vec::new();
    parse_items(&mut r, &mut items, &mut nodes)?;
    parse_nodes(&mut r, &mut nodes)?;

    // Children of visibility areas, listed again for the whole sector.
    let children = r.count()?;
    r.skip_records(children, 8)?;

    if !r.is_at_end() {
        return Err(SectorError::TrailingBytes { read: r.pos(), len: data.len() });
    }
    Ok(Sector { items, nodes })
}

/// A count of items, then the items.
fn parse_items(r: &mut Reader, items: &mut Vec<Item>, nodes: &mut Vec<Node>) -> Result<(), SectorError> {
    let count = r.count()?;
    items.reserve((count as usize).min(r.remaining() / ITEM_HEADER_SIZE));
    for _ in 0..count {
        let start = r.pos();
        parse_item(r, items, nodes).map_err(|error| match error {
            ItemError::Unknown(item_type) => SectorError::UnknownItemType { item_type, offset: start },
            ItemError::OutOfBounds(error) => SectorError::OutOfBounds(error),
        })?;
    }
    Ok(())
}

/// A count of nodes, then the nodes.
fn parse_nodes(r: &mut Reader, nodes: &mut Vec<Node>) -> Result<(), OutOfBounds> {
    let count = r.count()?;
    nodes.reserve((count as usize).min(r.remaining() / NODE_SIZE));
    for _ in 0..count {
        nodes.push(parse_node(r)?);
    }
    Ok(())
}

enum ItemError {
    Unknown(u32),
    OutOfBounds(OutOfBounds),
}

impl From<OutOfBounds> for ItemError {
    fn from(error: OutOfBounds) -> Self {
        Self::OutOfBounds(error)
    }
}

/// Reads one item into `items` (a compound adds the items and nodes it holds, too).
fn parse_item(r: &mut Reader, items: &mut Vec<Item>, nodes: &mut Vec<Node>) -> Result<(), ItemError> {
    let item_type = r.u32()?;
    let uid = r.u64()?;
    // Bounding box: two corners of five values each, x, y and z first (the other two are unknown).
    let (min_x, _min_y, min_z) = (r.f32()?, r.f32()?, r.f32()?);
    r.skip(8)?;
    let (max_x, _max_y, max_z) = (r.f32()?, r.f32()?, r.f32()?);
    r.skip(8)?;
    let flags = r.bytes(4)?;
    let flags = [flags[0], flags[1], flags[2], flags[3]];
    r.skip(1)?; // view distance

    use item_type::*;
    let bit = |byte: usize, bit: u32| flags[byte] & (1 << bit) != 0;
    let data = match item_type {
        ROAD => {
            let flags = MapFlags { hidden: bit(3, 1), secret: bit(2, 0), dlc_guard: r.clone().bytes(2)?[1] };
            r.skip(4)?;
            let look = r.u64()?;
            r.skip(2 * 8 + 2 * 8 + 4 * 8)?; // traffic rules, variants, edges
            let mut relief = [Relief::default(); 2];
            for side in &mut relief {
                side.profile = r.u64()?;
                side.coefficient = r.f32()?;
            }
            r.skip(2 * 8 + 8)?; // looks, material
            r.skip(6 * (8 + 2))?; // railings
            for side in &mut relief {
                side.height_offset = r.i32()? as f32 / 100.0;
            }
            let (start_node, end_node) = (r.u64()?, r.u64()?);
            r.skip(4)?; // length
            ItemData::Road { flags, look, start_node, end_node, relief }
        }
        PREFAB => {
            let flags = MapFlags { hidden: bit(2, 1), secret: bit(0, 5), dlc_guard: flags[1] };
            let model = r.u64()?;
            let variant = r.u64()?;
            let parts = r.count()?;
            r.skip_records(parts, 8)?;
            let node_count = r.count()?;
            let nodes = r.u64s(node_count)?;
            let connected = r.count()?;
            r.skip_records(connected, 8)?;
            r.skip(8)?;
            let origin = r.u8()?;
            r.skip(1)?;
            r.skip_records(node_count, 0x0c)?; // node looks
            r.skip(8)?;
            ItemData::Prefab { flags, model, variant, nodes, origin }
        }
        MAP_AREA => {
            let map_flags = MapFlags { hidden: bit(0, 3), secret: bit(0, 4), dlc_guard: flags[1] };
            let node_count = r.count()?;
            let nodes = r.u64s(node_count)?;
            let color = r.u32()?;
            ItemData::MapArea { flags: map_flags, draw_over: bit(0, 0), nodes, color }
        }
        CITY => {
            let city = r.u64()?;
            let (width, height) = (r.f32()?, r.f32()?);
            let node = r.u64()?;
            ItemData::City { hidden: bit(0, 0), city, width, height, node }
        }
        COMPANY => {
            r.skip(8)?; // the city it belongs to
            let prefab = r.u64()?;
            let company = r.u64()?;
            let node = r.u64()?;
            let spots = r.count()?;
            r.skip_records(spots, 0x0c)?;
            ItemData::Company { dlc_guard: flags[1], company, prefab, node }
        }
        FERRY => {
            let port = r.u64()?;
            r.skip(8)?; // the prefab it's linked to
            let node = r.u64()?;
            r.skip(0x0c)?; // unload offset
            ItemData::Ferry { train: flags[0] != 0, port, node }
        }
        MAP_OVERLAY => {
            let map_flags = MapFlags { hidden: flags[0] == 0xff, secret: bit(2, 1), dlc_guard: flags[1] };
            let icon = r.u64()?;
            let node = r.u64()?;
            ItemData::MapOverlay { flags: map_flags, kind: flags[0] & 0x0f, icon, node }
        }
        TERRAIN => {
            let (start_node, end_node) = (r.u64()?, r.u64()?);
            r.skip(12 + 12)?; // node offsets
            r.skip(4 + 4 + 4)?; // length, the preceding terrain's length, random seed
            r.skip(3 * 10)?; // railings: model and offset
            let mut sizes = [0.0; 2];
            let mut relief = [Relief::default(); 2];
            for (size, side) in sizes.iter_mut().zip(&mut relief) {
                *size = r.u16()? as f32 / 10.0;
                side.profile = r.u64()?;
                side.coefficient = r.f32()?;
                r.skip(8 + 4)?; // the preceding terrain's profile and coefficient
                r.skip(3 * ROAD_VEGETATION_SIZE)?;
                r.skip(4)?; // no detail vegetation from, to
            }
            let spheres = r.count()?;
            r.skip_records(spheres, VEGETATION_SPHERE_SIZE)?;
            let right = TerrainSide { size: sizes[0], grid: read_quad_grid(r)? };
            let left = TerrainSide { size: sizes[1], grid: read_quad_grid(r)? };
            r.skip(4 * 8)?; // edges and their looks
            ItemData::Terrain { start_node, end_node, right, left, relief }
        }
        BEZIER_PATCH => {
            let mut control_points = [[[0.0; 3]; 4]; 4];
            for row in &mut control_points {
                for point in row.iter_mut() {
                    *point = [r.f32()?, r.f32()?, r.f32()?];
                }
            }
            // Stored mirrored along x.
            control_points.reverse();
            r.skip(4)?; // tessellation (the grid has it too)
            let node = r.u64()?;
            r.skip(4)?; // random seed
            r.skip(3 * (8 + 2 + 1))?; // vegetation
            let spheres = r.count()?;
            r.skip_records(spheres, VEGETATION_SPHERE_SIZE)?;
            ItemData::BezierPatch { node, control_points, grid: read_quad_grid(r)? }
        }
        COMPOUND => {
            r.skip(8)?; // node
            let mut inner_items = Vec::new();
            let count = r.count()?;
            for _ in 0..count {
                parse_item(r, &mut inner_items, nodes)?;
            }
            parse_nodes(r, nodes)?;
            items.extend(inner_items);
            ItemData::Other
        }
        _ => {
            skip_item(r, item_type)?;
            ItemData::Other
        }
    };
    items.push(Item { uid, item_type, bounds: [min_x, min_z, max_x, max_z], data });
    Ok(())
}

/// Steps over the fields of an item the map doesn't draw. `r` is just past its flags.
fn skip_item(r: &mut Reader, item_type: u32) -> Result<(), ItemError> {
    use item_type::*;
    match item_type {
        BUILDING => skip_counted(r, 0x2c, 4, 0)?,
        MODEL => skip_counted(r, 0x18, 8, 0x24)?,
        SERVICE | FUEL_PUMP | VISIBILITY_AREA => skip_counted(r, 0x10, 8, 0)?,
        GARAGE => skip_counted(r, 0x1c, 8, 0)?,
        CUT_PLANE => skip_counted(r, 0, 8, 0)?,
        BUS_STOP => r.skip(0x18)?, // city, prefab, node
        TRAFFIC_RULE => {
            let tags = r.count()?;
            r.skip_records(tags, 8)?;
            let nodes = r.count()?;
            r.skip_records(nodes, 8)?;
            r.skip(0x0c)?; // rule and range
        }
        TRAJECTORY => {
            let nodes = r.count()?;
            r.skip_records(nodes, 8)?;
            r.skip(8)?;
            let route_rules = r.count()?;
            r.skip_records(route_rules, 0x1c)?;
            let checkpoints = r.count()?;
            r.skip_records(checkpoints, 0x10)?;
            let tags = r.count()?;
            r.skip_records(tags, 8)?;
        }
        TRIGGER => {
            let tags = r.count()?;
            r.skip_records(tags, 8)?;
            let nodes = r.count()?;
            r.skip_records(nodes, 8)?;
            let actions = r.count()?;
            for _ in 0..actions {
                r.skip(8)?; // the action
                let overrides = r.i32()?;
                if overrides < 0 {
                    continue;
                }
                r.skip_records(overrides as u32, 4)?;
                let parameters = r.count()?;
                for _ in 0..parameters {
                    skip_string(r)?;
                }
                let target_tags = r.count()?;
                r.skip_records(target_tags, 8)?;
                r.skip(8)?; // range and type
            }
            if nodes == 1 {
                r.skip(4)?; // radius
            }
        }
        CUTSCENE => {
            let tags = r.count()?;
            r.skip_records(tags, 8)?;
            r.skip(8)?; // node
            let actions = r.count()?;
            for _ in 0..actions {
                let numbers = r.count()?;
                r.skip_records(numbers, 4)?;
                let strings = r.count()?;
                for _ in 0..strings {
                    skip_string(r)?;
                }
                let target_tags = r.count()?;
                r.skip_records(target_tags, 8)?;
                r.skip(8)?; // range and flags
            }
        }
        CURVE => {
            r.skip(4 * 8 + 4)?; // four nodes and the length
            let used = r.u32()?;
            for _ in 0..used.count_ones() {
                r.skip(0x4c)?;
                let heights = r.count()?;
                r.skip_records(heights, 4)?;
                r.skip(0x14)?;
            }
        }
        ROAD_SIDE => skip_road_side(r)?,
        MOVER => {
            // Tags; model, look, variant; speed, delay, width; count; lengths; nodes.
            skip_counted(r, 0, 8, 3 * 8 + 3 * 4 + 4)?;
            skip_counted(r, 0, 4, 0)?;
            skip_counted(r, 0, 8, 0)?;
        }
        ENVIRONMENT_AREA => r.skip(4 + 4 + 4 + 8 + 8 + 8)?, // size, fog, climate, reflection, node
        HINGE => r.skip(8 + 8 + 8 + 4 + 4)?,                // model, look, node, rotation limits
        ANIMATED_MODEL => skip_counted(r, 0, 8, 8 + 8)?,    // tags; model, node
        SOUND => r.skip(8 + 8 + 4 + 4 + 8)?,                // name, reverb, size, node
        CAMERA_POINT => skip_counted(r, 0, 8, 8)?,          // tags; node
        WALKER => {
            // Name prefix; speed, delay, count, width, angle; lengths; nodes.
            r.skip(8 + 5 * 4)?;
            skip_counted(r, 0, 4, 0)?;
            skip_counted(r, 0, 8, 0)?;
        }
        FAR_MODEL => {
            // Size; models and their scales; children; nodes.
            r.skip(4 + 4)?;
            skip_counted(r, 0, 8 + 12, 0)?;
            skip_counted(r, 0, 8, 0)?;
            skip_counted(r, 0, 8, 0)?;
        }
        CAMERA_PATH => {
            // Tags, nodes, track points, control nodes; keyframes; speed.
            for _ in 0..4 {
                skip_counted(r, 0, 8, 0)?;
            }
            skip_counted(r, 0, 4 + 4 + 4 + 4 + 12 + 12, 4)?;
        }
        HOOKUP => {
            skip_string(r)?;
            r.skip(8)?; // node
        }
        GATE => {
            r.skip(8)?; // model
            skip_counted(r, 0, 8, 0)?; // nodes
            for _ in 0..2 {
                skip_string(r)?; // activation trigger
                r.skip(4)?; // its node
            }
        }
        _ => return Err(ItemError::Unknown(item_type)),
    }
    Ok(())
}

/// Skips `before` bytes, then a count and that many records of `size`, then `after` bytes.
pub(crate) fn skip_counted(r: &mut Reader, before: usize, size: usize, after: usize) -> Result<(), OutOfBounds> {
    r.skip(before)?;
    let count = r.count()?;
    r.skip_records(count, size)?;
    r.skip(after)
}

/// A string: 64-bit length, then its bytes.
pub(crate) fn skip_string(r: &mut Reader) -> Result<(), OutOfBounds> {
    let len = r.u64()?;
    r.skip(usize::try_from(len).unwrap_or(usize::MAX))
}

/// Road signs: boards, then optional per-sign overrides.
fn skip_road_side(r: &mut Reader) -> Result<(), OutOfBounds> {
    r.skip(0x20)?; // sign model, node, look, variant
    let boards = r.u8()?;
    r.skip_records(boards.into(), 0x18)?;
    let template_len = r.u64()?;
    if template_len == 0 {
        return Ok(());
    }
    r.skip(usize::try_from(template_len).unwrap_or(usize::MAX))?;
    let board_overrides = r.count()?;
    for _ in 0..board_overrides {
        r.skip(8)?; // area name
        let flags = r.u8()?;
        if flags & 0x01 != 0 {
            r.skip(2)?;
        }
        if flags & 0x02 != 0 {
            r.skip(8)?;
        }
    }
    let sign_overrides = r.count()?;
    for _ in 0..sign_overrides {
        r.skip(0x0c)?;
        let attributes = r.count()?;
        for _ in 0..attributes {
            let kind = r.u16()?;
            r.skip(4)?; // index
            match kind {
                0x05 => skip_string(r)?,
                0x06 => r.skip(8)?,
                0x01 => r.skip(1)?,
                _ => r.skip(4)?,
            }
        }
    }
    Ok(())
}

fn parse_node(r: &mut Reader) -> Result<Node, OutOfBounds> {
    let uid = r.u64()?;
    let fixed = |v: i32| v as f32 / 256.0;
    let position = [fixed(r.i32()?), fixed(r.i32()?), fixed(r.i32()?)];
    let rotation = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
    let backward_item = r.u64()?;
    let forward_item = r.u64()?;
    let flags = r.u32()?;
    Ok(Node { uid, position, rotation, backward_item, forward_item, flags })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sector(version: u32, items: &[Vec<u8>], extra: &[u8]) -> Vec<u8> {
        let mut data = version.to_le_bytes().to_vec();
        data.extend([0; 12]);
        data.extend((items.len() as u32).to_le_bytes());
        for item in items {
            data.extend(item);
        }
        data.extend(0u32.to_le_bytes()); // nodes
        data.extend(0u32.to_le_bytes()); // visibility children
        data.extend(extra);
        data
    }

    fn item(item_type: u32, uid: u64, flags: [u8; 4], fields: &[u8]) -> Vec<u8> {
        let mut data = item_type.to_le_bytes().to_vec();
        data.extend(uid.to_le_bytes());
        for v in [10.0f32, 0.0, 20.0, 0.0, 0.0, 30.0, 5.0, 40.0, 0.0, 0.0] {
            data.extend(v.to_le_bytes());
        }
        data.extend(flags);
        data.push(0);
        data.extend(fields);
        data
    }

    fn map_area(nodes: &[u64], color: u32, flags: u8) -> Vec<u8> {
        let mut fields = (nodes.len() as u32).to_le_bytes().to_vec();
        for node in nodes {
            fields.extend(node.to_le_bytes());
        }
        fields.extend(color.to_le_bytes());
        item(item_type::MAP_AREA, 7, [flags, 0, 0, 0], &fields)
    }

    #[test]
    fn reads_items_and_checks_the_whole_file_was_understood() {
        let data = sector(SUPPORTED_VERSION, &[map_area(&[1, 2, 3], 3, 0b1_0001)], &[]);
        let parsed = parse(&data).unwrap();
        let item = &parsed.items[0];
        assert_eq!((item.uid, item.bounds), (7, [10.0, 20.0, 30.0, 40.0]));
        let ItemData::MapArea { flags, draw_over, nodes, color } = &item.data else { panic!() };
        assert_eq!((flags.secret, flags.hidden, *draw_over), (true, false, true));
        assert_eq!((nodes.as_slice(), *color), (&[1, 2, 3][..], 3));

        let longer = sector(SUPPORTED_VERSION, &[map_area(&[1], 0, 0)], &[0]);
        assert!(matches!(parse(&longer), Err(SectorError::TrailingBytes { .. })));
    }

    #[test]
    fn refuses_other_versions_and_unknown_items() {
        assert_eq!(parse(&sector(906, &[], &[])).unwrap_err(), SectorError::UnsupportedVersion(906));
        let unknown = sector(SUPPORTED_VERSION, &[item(99, 1, [0; 4], &[])], &[]);
        assert_eq!(parse(&unknown).unwrap_err(), SectorError::UnknownItemType { item_type: 99, offset: 0x14 });
    }

    #[test]
    fn reports_truncated_files() {
        let mut data = sector(SUPPORTED_VERSION, &[map_area(&[1, 2], 0, 0)], &[]);
        data.truncate(data.len() - 12);
        assert!(matches!(parse(&data), Err(SectorError::OutOfBounds(_))));
    }
}
