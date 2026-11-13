//! Sector `.data` files: the bulky part of roads and prefabs (their terrain, vegetation, overrides), stored
//! apart from the items in `.base`, each keyed by its item's uid. The map reads the roads' terrain sides,
//! where the water beside a road is.

use std::collections::HashMap;

use crate::reader::Reader;
use crate::sector::{
    ROAD_VEGETATION_SIZE, SUPPORTED_VERSION, SectorError, TerrainSide, VEGETATION_SPHERE_SIZE, skip_counted,
};
use crate::terrain::read_quad_grid;

/// The uid after the last entry.
const END: u64 = u64::MAX;

/// What a `.data` entry belongs to, which decides its layout.
#[derive(Clone, Copy, Debug)]
pub enum DataOwner {
    Road,
    Prefab,
}

/// Prefabs keep data for this many corners, however many nodes they have.
const PREFAB_CORNERS: usize = 6;

/// A road's terrain, right side then left side of its direction of travel.
#[derive(Debug, Default)]
pub struct RoadTerrain {
    pub right: TerrainSide,
    pub left: TerrainSide,
}

/// A prefab's own data: its look, and at each of its nodes the terrain extruded from that edge and the corner
/// model set there (`/def/world/prefab_corner.sii`).
#[derive(Debug, Default)]
pub struct PrefabData {
    pub look: u64,
    pub corners: Vec<PrefabCorner>,
}

#[derive(Debug, Default)]
pub struct PrefabCorner {
    pub terrain: TerrainSide,
    /// Its corner model, the model's variant and look (tokens, 0 for none).
    pub model: [u64; 3],
}

/// A sector's `.data`: each road's terrain and each prefab's own data, by uid.
#[derive(Debug, Default)]
pub struct SectorData {
    pub roads: HashMap<u64, RoadTerrain>,
    pub prefabs: HashMap<u64, PrefabData>,
}

/// Reads a `.data` file. `owners` says what each uid is (from the sector's `.base`); an entry for an unknown uid
/// stops the read, since its size can't be known.
pub fn parse(data: &[u8], owners: &HashMap<u64, DataOwner>) -> Result<SectorData, SectorError> {
    crate::reader::guarded(|| parse_unguarded(data, owners), SectorError::Panicked)
}

fn parse_unguarded(data: &[u8], owners: &HashMap<u64, DataOwner>) -> Result<SectorData, SectorError> {
    let mut r = Reader::new(data);
    let version = r.u32()?;
    // An older map mod's: how its records differed isn't known, so its roads' terrain is left out (only the
    // water painted along them), rather than misread.
    if version < SUPPORTED_VERSION {
        return Ok(SectorData::default());
    }
    if version != SUPPORTED_VERSION {
        return Err(SectorError::UnsupportedVersion(version));
    }
    r.skip(8 + 4)?; // game and its map version
    let mut read = SectorData::default();
    loop {
        let offset = r.pos();
        let uid = r.u64()?;
        if uid == END {
            break;
        }
        match owners.get(&uid) {
            Some(DataOwner::Road) => {
                read.roads.insert(uid, read_road(&mut r)?);
            }
            Some(DataOwner::Prefab) => {
                read.prefabs.insert(uid, read_prefab(&mut r)?);
            }
            None => return Err(SectorError::UnknownDataOwner { uid, offset }),
        }
    }
    if !r.is_at_end() {
        return Err(SectorError::TrailingBytes { read: r.pos(), len: data.len() });
    }
    Ok(read)
}

fn read_road(r: &mut Reader) -> Result<RoadTerrain, SectorError> {
    r.skip(8)?; // overlay
    let mut sides = [TerrainSide::default(), TerrainSide::default()];
    for side in &mut sides {
        r.skip(2 * (8 + 2 + 2))?; // models: name, offset, distance
        side.size = r.u16()? as f32 / 10.0;
        r.skip(3 * ROAD_VEGETATION_SIZE)?;
        r.skip(8)?; // sidewalk material
        let at = r.pos();
        side.grid = read_quad_grid(r).map_err(|error| SectorError::of_grid(error, at))?;
    }
    r.skip(8 + 4 + 2)?; // centre material, its colour and rotation
    r.skip(4 + 4)?; // random seed, the preceding road's length
    r.skip(8 + 2 + 1 + 1)?; // centre vegetation
    r.skip(4 * 2)?; // no detail vegetation from, to (both sides)
    skip_counted(r, 0, VEGETATION_SPHERE_SIZE, 0)?;
    skip_counted(r, 0, 8, 0)?; // additional parts, left
    skip_counted(r, 0, 8, 0)?; // and right
    skip_counted(r, 0, 2 + 2 + 8 + 8, 0)?; // edge overrides, left
    skip_counted(r, 0, 2 + 2 + 8 + 8, 0)?; // and right
    skip_counted(r, 0, 2 + 2 + 8, 0)?; // variant overrides, left
    skip_counted(r, 0, 2 + 2 + 8, 0)?; // and right
    r.skip(4 * 8)?; // edge looks
    let [right, left] = sides;
    Ok(RoadTerrain { right, left })
}

fn read_prefab(r: &mut Reader) -> Result<PrefabData, SectorError> {
    let look = r.u64()?;
    let mut corners: Vec<PrefabCorner> = (0..PREFAB_CORNERS).map(|_| PrefabCorner::default()).collect();
    for corner in &mut corners {
        corner.terrain.size = r.u16()? as f32 / 10.0;
        r.skip(2 + 2)?; // detail vegetation from, to
        r.skip(2 * ROAD_VEGETATION_SIZE)?;
    }
    skip_counted(r, 0, 8 + 2 * (8 + 2 + 2), 0)?; // vegetation parts
    r.skip(4)?; // random seed
    skip_counted(r, 0, VEGETATION_SPHERE_SIZE, 0)?;
    for corner in &mut corners {
        corner.model = [r.u64()?, r.u64()?, r.u64()?];
    }
    for corner in &mut corners {
        let at = r.pos();
        corner.terrain.grid = read_quad_grid(r).map_err(|error| SectorError::of_grid(error, at))?;
    }
    Ok(PrefabData { look, corners })
}
