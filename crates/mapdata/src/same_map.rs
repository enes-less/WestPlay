//! Whether two built maps are the same, byte for byte: every file of their tiles (the folder each one's `map.json`
//! names), their road signs, and `map.json` itself but for the tiles folder's name (a build's own time) and what it
//! was built from (`source`: the builder's version, the mods' files and their order as listed, not the map). What a
//! change to how maps are built must leave as it was (`examples/same_map`, `tools/dev/map_check.ps1`), and what two
//! builds of one map must give however their work is shared out (the review of 2026-11-11, C3 and M8). A tile of open
//! sea alone, listed and not written (tiles.rs `SeaTiles`), is the same as one written that's open sea alone
//! ([`crate::tiles::is_sea_only`]): the map the cab shows is.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::tiles::{SEA_INDEX, TileOptions, is_sea_only};

/// How the maps built in `first` and `second` (each a folder with its `map.json`) differ: what's in one and not the
/// other, or in both but not the same (`map.json` as above); and how many files are the same. An error if either
/// isn't a built map.
pub fn differences(first: &Path, second: &Path) -> Result<(Vec<String>, usize), String> {
    let (a, b) = (built(first)?, built(second)?);
    let mut differ: Vec<String> = Vec::new();
    if a.manifest != b.manifest {
        differ.push("map.json".into());
    }
    let mut same = 0;
    let names: BTreeSet<&String> = a.files.keys().chain(&a.sea).chain(b.files.keys()).chain(&b.sea).collect();
    for name in names {
        let alike = match (a.tile(name), b.tile(name)) {
            (Some(Tile::File(one)), Some(Tile::File(other))) => one == other,
            (Some(Tile::Sea), Some(Tile::Sea)) => true,
            (Some(Tile::Sea), Some(Tile::File(file))) | (Some(Tile::File(file)), Some(Tile::Sea)) => {
                is_sea_only(file, TileOptions::default().buffer)
            }
            (Some(_), None) => {
                differ.push(format!("{name}: only in the first"));
                continue;
            }
            (None, _) => {
                differ.push(format!("{name}: only in the second"));
                continue;
            }
        };
        if alike {
            same += 1;
        } else {
            differ.push(format!("{name}: different"));
        }
    }
    Ok((differ, same))
}

/// A tile of a built map: a file, or open sea alone, listed and not written.
enum Tile<'a> {
    File(&'a [u8]),
    Sea,
}

/// A built map: its manifest without the tiles folder's name (in every path it gives: the tiles', the places', the
/// surfaces', the meshes', the countries', the world's), and its files (tiles and signs) by their path within it.
struct Built {
    manifest: serde_json::Value,
    files: BTreeMap<String, Vec<u8>>,
    /// The tiles of open sea alone, listed and not written, by their path as `files` has them.
    sea: BTreeSet<String>,
}

impl Built {
    fn tile(&self, name: &str) -> Option<Tile<'_>> {
        match self.files.get(name) {
            Some(file) => Some(Tile::File(file)),
            None => self.sea.contains(name).then_some(Tile::Sea),
        }
    }
}

fn built(dir: &Path) -> Result<Built, String> {
    let at = |error: &dyn std::fmt::Display| format!("{}: {error}", dir.display());
    let text = std::fs::read_to_string(dir.join("map.json")).map_err(|error| at(&error))?;
    let mut manifest: serde_json::Value = serde_json::from_str(&text).map_err(|error| at(&error))?;
    let tiles = manifest["tiles"].as_str().ok_or_else(|| at(&"map.json names no tiles"))?.to_owned();
    // The folder's name is the build's time, in each path into it; what's in it is compared below.
    let fields = manifest.as_object_mut().ok_or_else(|| at(&"map.json isn't an object"))?;
    fields.remove("source");
    // The tiles of open sea alone: compared as tiles, with what the other map has there.
    let listed = fields.remove("seaTiles").and_then(|index| index.as_str().map(str::to_owned));
    without_folder(&mut manifest, &tiles);
    let mut files = BTreeMap::new();
    for (folder, under) in [(dir.join(&tiles), "tiles"), (dir.join("signs"), "signs")] {
        read_all(&folder, &folder, under, &mut files).map_err(|error| at(&error))?;
    }
    let mut sea = BTreeSet::new();
    if let Some(index) = listed {
        let text = std::fs::read_to_string(dir.join(&index)).map_err(|error| at(&error))?;
        let index: serde_json::Value = serde_json::from_str(&text).map_err(|error| at(&error))?;
        for (z, zoom) in index["zooms"].as_object().ok_or_else(|| at(&"the sea's list has no zooms"))? {
            // The list and each zoom's sea tile are how the tiles are kept, not tiles.
            if let Some(tile) = zoom["tile"].as_str() {
                files.remove(&format!("tiles/{tile}"));
            }
            for run in zoom["rows"].as_array().ok_or_else(|| at(&"the sea's list has no rows"))? {
                let run: Vec<u64> =
                    run.as_array().into_iter().flatten().filter_map(serde_json::Value::as_u64).collect();
                let [y, first, last] = run[..] else { return Err(at(&"a run of sea tiles isn't a row, first, last")) };
                sea.extend((first..=last).map(|x| format!("tiles/{z}/{x}/{y}.pbf")));
            }
        }
        files.remove(&format!("tiles/{SEA_INDEX}"));
    }
    Ok(Built { manifest, files, sea })
}

/// `value` with `folder` at the start of any path in it as `tiles`.
fn without_folder(value: &mut serde_json::Value, folder: &str) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(rest) = text.strip_prefix(folder) {
                *text = format!("tiles{rest}");
            }
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(|value| without_folder(value, folder)),
        serde_json::Value::Object(fields) => fields.values_mut().for_each(|value| without_folder(value, folder)),
        _ => {}
    }
}

/// Every file under `dir` (a folder that isn't there has none), by its path from `root` after `under`.
fn read_all(root: &Path, dir: &Path, under: &str, files: &mut BTreeMap<String, Vec<u8>>) -> std::io::Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(()) };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            read_all(root, &path, under, files)?;
        } else {
            let name = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            files.insert(format!("{under}/{name}"), std::fs::read(&path)?);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_dir::TempDir;

    fn write_map(dir: &Path, folder: &str, tile: &[u8]) {
        std::fs::create_dir_all(dir.join(folder).join("10")).unwrap();
        std::fs::write(dir.join(folder).join("10/1.pbf"), tile).unwrap();
        let places = format!("{folder}/places.json");
        let manifest = serde_json::json!({ "tiles": folder, "places": places, "source": folder });
        std::fs::write(dir.join("map.json"), manifest.to_string()).unwrap();
    }

    #[test]
    fn two_builds_differ_only_by_what_they_hold() {
        let (a, b) = (TempDir::new(), TempDir::new());
        // Built at other times (their folders' names, and what they were built from, differ): the same.
        write_map(&a.path, "tiles-100", b"tile");
        write_map(&b.path, "tiles-200", b"tile");
        assert_eq!(differences(&a.path, &b.path), Ok((Vec::new(), 1)));
        // A tile not the same; a tile in one only.
        write_map(&b.path, "tiles-200", b"other");
        std::fs::write(b.path.join("tiles-200/10/2.pbf"), b"more").unwrap();
        let (differ, same) = differences(&a.path, &b.path).unwrap();
        assert_eq!(differ, ["tiles/10/1.pbf: different", "tiles/10/2.pbf: only in the second"]);
        assert_eq!(same, 0);
        assert!(differences(&a.path, &a.path.join("none")).is_err());
    }

    #[test]
    fn a_tile_listed_as_open_sea_is_the_same_as_one_written_of_it() {
        // Open sea alone at zoom 12 (the base map's, 2026-11-10), and a tile with a shore in it.
        let hex = |text: &str| -> Vec<u8> {
            (0..text.len()).step_by(2).map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap()).collect()
        };
        let sea = hex("1a1f78020a0373656112131803220f0980417f1a008042ff410000ff410f288020");
        let shore = hex("1a2278020a0373656112151803221109ef03ff031a008088020f0000ff87020f28808001");
        let (listed, written) = (TempDir::new(), TempDir::new());
        // One map lists tile 12/5/7 as sea (its file not there); the other has it written.
        write_map(&listed.path, "tiles-1", b"tile");
        std::fs::create_dir_all(listed.path.join("tiles-1/sea")).unwrap();
        std::fs::write(listed.path.join("tiles-1/sea/12.pbf"), &sea).unwrap();
        let index = serde_json::json!({ "zooms": { "12": { "tile": "sea/12.pbf", "rows": [[7, 5, 5]] } } });
        std::fs::write(listed.path.join("tiles-1").join(SEA_INDEX), index.to_string()).unwrap();
        let mut manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(listed.path.join("map.json")).unwrap()).unwrap();
        manifest["seaTiles"] = serde_json::json!(format!("tiles-1/{SEA_INDEX}"));
        std::fs::write(listed.path.join("map.json"), manifest.to_string()).unwrap();
        write_map(&written.path, "tiles-2", b"tile");
        std::fs::create_dir_all(written.path.join("tiles-2/12/5")).unwrap();
        std::fs::write(written.path.join("tiles-2/12/5/7.pbf"), &sea).unwrap();
        assert_eq!(differences(&listed.path, &written.path), Ok((Vec::new(), 2)));
        assert_eq!(differences(&written.path, &listed.path), Ok((Vec::new(), 2)));
        // Written with a shore in it: not the sea listed.
        std::fs::write(written.path.join("tiles-2/12/5/7.pbf"), &shore).unwrap();
        assert_eq!(differences(&listed.path, &written.path).unwrap().0, ["tiles/12/5/7.pbf: different"]);
    }
}
