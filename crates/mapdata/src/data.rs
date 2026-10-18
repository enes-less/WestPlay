//! Sector `.data` files: the bulky part of roads and prefabs (their terrain, vegetation, overrides), stored
//! apart from the items in `.base`, each keyed by its item's uid. The map reads the roads' terrain sides,
//! where the water beside a road is.

use std::collections::HashMap;

use crate::reader::Reader;
use crate::sector::{
    OLDEST_VERSION, ROAD_VEGETATION_SIZE, SUPPORTED_VERSION, SectorError, TerrainSide, VEGETATION_SPHERE_SIZE,
    skip_counted,
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

/// Reads a `.data` file: the terrain of each road in it, by road uid. `owners` says what each uid is
/// (from the sector's `.base`); an entry for an unknown uid stops the read, since its size can't be known.
pub fn parse(data: &[u8], owners: &HashMap<u64, DataOwner>) -> Result<HashMap<u64, RoadTerrain>, SectorError> {
    let mut r = Reader::new(data);
    let version = r.u32()?;
    // An older map mod's (as its sectors, `sector::OLDEST_VERSION` on): how its records differed isn't known,
    // so its roads' terrain is left out (only the water painted along them), rather than misread.
    if (OLDEST_VERSION..SUPPORTED_VERSION).contains(&version) {
        return Ok(HashMap::new());
    }
    if version != SUPPORTED_VERSION {
        return Err(SectorError::UnsupportedVersion(version));
    }
    r.skip(8 + 4)?; // game and its map version
    let mut roads = HashMap::new();
    loop {
        let offset = r.pos();
        let uid = r.u64()?;
        if uid == END {
            break;
        }
        match owners.get(&uid) {
            Some(DataOwner::Road) => {
                roads.insert(uid, read_road(&mut r)?);
            }
            Some(DataOwner::Prefab) => skip_prefab(&mut r)?,
            None => return Err(SectorError::UnknownDataOwner { uid, offset }),
        }
    }
    if !r.is_at_end() {
        return Err(SectorError::TrailingBytes { read: r.pos(), len: data.len() });
    }
    Ok(roads)
}

fn read_road(r: &mut Reader) -> Result<RoadTerrain, SectorError> {
    r.skip(8)?; // overlay
    let mut sides = [TerrainSide::default(), TerrainSide::default()];
    for side in &mut sides {
        r.skip(2 * (8 + 2 + 2))?; // models: name, offset, distance
        side.size = r.u16()? as f32 / 10.0;
        r.skip(3 * ROAD_VEGETATION_SIZE)?;
        r.skip(8)?; // sidewalk material
        side.grid = read_quad_grid(r)?;
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

fn skip_prefab(r: &mut Reader) -> Result<(), SectorError> {
    r.skip(8)?; // look
    for _ in 0..PREFAB_CORNERS {
        r.skip(2 + 2 + 2)?; // terrain size, detail vegetation from, to
        r.skip(2 * ROAD_VEGETATION_SIZE)?;
    }
    skip_counted(r, 0, 8 + 2 * (8 + 2 + 2), 0)?; // vegetation parts
    r.skip(4)?; // random seed
    skip_counted(r, 0, VEGETATION_SPHERE_SIZE, 0)?;
    r.skip(PREFAB_CORNERS * 3 * 8)?; // per corner: model, variant, look
    for _ in 0..PREFAB_CORNERS {
        read_quad_grid(r)?;
    }
    Ok(())
}
