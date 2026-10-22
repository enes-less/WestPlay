//! Which country each part of the map is in, and each country's time zone: for the cab's clock, which shows
//! the game's time where the truck is (owner 2026-10-21: an hour early in Finland).
//!
//! The game's clock adds the time zone of the country the truck is in (its `country_data`'s `time_zone`, in
//! minutes: 60 the UK and Portugal, 120 central Europe, 180 Finland, the Baltics, Romania, Bulgaria, Greece
//! and Turkey). The SDK's game time doesn't: it reads as central Europe's.
//!
//! The country: a map node's flags hold the country of the road each side of it, by its `country_id` (bits
//! 8-15 forward, 16-23 backward; found 2026-10-21 over the whole map: id 23's nodes in Finland, 10's in the
//! UK, 44's in Turkey). Not every node has one (some 330,000 of 7.5 million), but every country has thousands:
//! the map is cut into [`CELL`] squares, each the country most of its nodes are in, and a square without a
//! country's road the nearest's within `SEARCH_RINGS` (worked out here, once: the page only looks its square
//! up).

use std::collections::{BTreeMap, HashMap};

use crate::files::{FileError, GameFiles};
use crate::sector::Node;

/// The grid's square, metres.
pub const CELL: f32 = 1000.0;

/// The map's countries on a grid, and their time zones.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Countries {
    /// Where the grid starts (its west and north edge), metres.
    pub x: f32,
    pub z: f32,
    pub width: usize,
    pub height: usize,
    /// Each square's country id, row by row from the north west; 0 for none near.
    pub cells: Vec<u8>,
    /// Each country's time zone, minutes from UTC, by id.
    pub zones: BTreeMap<u8, i32>,
}

/// The countries of the roads either side of `node` (forward, backward), 0 for none.
fn countries_of(node: &Node) -> [u8; 2] {
    [(node.flags >> 8) as u8, (node.flags >> 16) as u8]
}

/// The grid from the map's nodes: each square the country most of its nodes' roads are in, else the nearest
/// such square's within `SEARCH_RINGS`; the roads' squares and that far around them.
pub fn grid<'a>(nodes: impl IntoIterator<Item = &'a Node>) -> Countries {
    let mut votes: HashMap<(i32, i32), HashMap<u8, u32>> = HashMap::new();
    for node in nodes {
        let cell = ((node.position[0] / CELL).floor() as i32, (node.position[2] / CELL).floor() as i32);
        for country in countries_of(node) {
            if country != 0 {
                *votes.entry(cell).or_default().entry(country).or_default() += 1;
            }
        }
    }
    let Some(columns) = votes.keys().map(|cell| cell.0).min().zip(votes.keys().map(|cell| cell.0).max()) else {
        return Countries::default();
    };
    let rows = (votes.keys().map(|cell| cell.1).min().unwrap(), votes.keys().map(|cell| cell.1).max().unwrap());
    // The roads' squares, and as far around them as a square looks for a country.
    let rings = SEARCH_RINGS as i32;
    let (columns, rows) = ((columns.0 - rings, columns.1 + rings), (rows.0 - rings, rows.1 + rings));
    let (width, height) = ((columns.1 - columns.0 + 1) as usize, (rows.1 - rows.0 + 1) as usize);
    let mut voted = vec![0u8; width * height];
    for ((column, row), counts) in votes {
        // The most, the lower id of a tie: the same every build.
        let most = counts.into_iter().max_by_key(|&(country, count)| (count, std::cmp::Reverse(country)));
        let at = (row - rows.0) as usize * width + (column - columns.0) as usize;
        voted[at] = most.map_or(0, |(country, _)| country);
    }
    let cells = (0..width * height)
        .map(|at| match voted[at] {
            0 => nearest(&voted, width, height, (at % width) as i64, (at / width) as i64),
            country => country,
        })
        .collect();
    Countries { x: columns.0 as f32 * CELL, z: rows.0 as f32 * CELL, width, height, cells, zones: BTreeMap::new() }
}

/// The country of the nearest square with one within `SEARCH_RINGS` of `column`, `row` (by straight distance,
/// in the first ring out that has one; of a tie, the first found); 0 if none.
fn nearest(voted: &[u8], width: usize, height: usize, column: i64, row: i64) -> u8 {
    let get = |column: i64, row: i64| -> u8 {
        if column < 0 || row < 0 || column >= width as i64 || row >= height as i64 {
            return 0;
        }
        voted[row as usize * width + column as usize]
    };
    for ring in 1..=SEARCH_RINGS {
        let mut best: Option<(i64, u8)> = None;
        for dr in -ring..=ring {
            for dc in -ring..=ring {
                if dr.abs().max(dc.abs()) != ring {
                    continue;
                }
                let country = get(column + dc, row + dr);
                let far = dr * dr + dc * dc;
                if country != 0 && best.is_none_or(|(nearest, _)| far < nearest) {
                    best = Some((far, country));
                }
            }
        }
        if let Some((_, country)) = best {
            return country;
        }
    }
    0
}

/// Each country's time zone (minutes), by id: from the game's country definitions (`def/country/*.sui`, and
/// a mod's `def/country.<mod>.sii`), map mods' countries with theirs.
pub fn time_zones(files: &GameFiles) -> Result<BTreeMap<u8, i32>, FileError> {
    let mut paths: Vec<String> = files
        .list("def/country")?
        .into_iter()
        .filter(|name| name.ends_with(".sui") || name.ends_with(".sii"))
        .map(|name| format!("def/country/{name}"))
        .collect();
    paths.extend(
        files
            .list("def")?
            .into_iter()
            .filter(|name| name.starts_with("country.") && name.ends_with(".sii"))
            .map(|name| format!("def/{name}")),
    );
    let mut zones = BTreeMap::new();
    for path in paths {
        let Some(bytes) = files.read(&path)? else { continue };
        zones.extend(zones_in(&String::from_utf8_lossy(&bytes)));
    }
    Ok(zones)
}

/// The `country_id` and `time_zone` of each country definition in a file's text.
fn zones_in(text: &str) -> Vec<(u8, i32)> {
    let value = |line: &str, name: &str| -> Option<i64> {
        let rest = line.trim().strip_prefix(name)?.trim_start().strip_prefix(':')?;
        rest.split(|c: char| c.is_whitespace() || c == '#').find(|word| !word.is_empty())?.parse().ok()
    };
    let mut found = Vec::new();
    let (mut id, mut zone) = (None, None);
    for line in text.lines() {
        if line.contains("country_data") && line.contains(':') && !line.trim_start().starts_with('#') {
            (id, zone) = (None, None);
        }
        id = value(line, "country_id").or(id);
        zone = value(line, "time_zone").or(zone);
        if let (Some(country), Some(minutes)) = (id, zone) {
            if let (Ok(country), Ok(minutes)) = (u8::try_from(country), i32::try_from(minutes)) {
                found.push((country, minutes));
            }
            (id, zone) = (None, None);
        }
    }
    found
}

impl Countries {
    /// The country at `[x, z]`: its square's (a square without a country's road has the nearest's); 0 if none.
    pub fn at(&self, x: f32, z: f32) -> u8 {
        let column = ((x - self.x) / CELL).floor() as i64;
        let row = ((z - self.z) / CELL).floor() as i64;
        if column < 0 || row < 0 || column >= self.width as i64 || row >= self.height as i64 {
            return 0;
        }
        self.cells[row as usize * self.width + column as usize]
    }

    /// As the page reads it (`countries.json`): the grid, its squares as a string of hex pairs.
    pub fn to_json(&self) -> serde_json::Value {
        let cells: String = self.cells.iter().map(|cell| format!("{cell:02x}")).collect();
        serde_json::json!({
            "cell": CELL,
            "x": self.x,
            "z": self.z,
            "width": self.width,
            "height": self.height,
            "cells": cells,
            "zones": self.zones.iter().map(|(id, minutes)| (id.to_string(), minutes)).collect::<BTreeMap<_, _>>(),
        })
    }
}

/// How many squares out a place without a country's road looks for one.
const SEARCH_RINGS: i64 = 5;

#[cfg(test)]
mod tests {
    use super::*;

    fn node(x: f32, z: f32, forward: u8, backward: u8) -> Node {
        Node {
            uid: 0,
            position: [x, 0.0, z],
            rotation: [1.0, 0.0, 0.0, 0.0],
            backward_item: 0,
            forward_item: 0,
            flags: u32::from(forward) << 8 | u32::from(backward) << 16 | 1,
        }
    }

    #[test]
    fn each_square_is_the_country_most_of_its_roads_are_in() {
        // Germany (5) at the west, Finland (23) two squares east; a border node between, mostly Finland's.
        let nodes = [
            node(100.0, 100.0, 5, 5),
            node(200.0, 300.0, 5, 0),
            node(2100.0, 100.0, 23, 23),
            node(2200.0, 200.0, 23, 5),
            node(1500.0, 500.0, 0, 0),
        ];
        let countries = grid(&nodes);
        // The roads' three squares, and as far as a square looks for a country around them.
        let rings = SEARCH_RINGS as usize;
        assert_eq!((countries.width, countries.height), (3 + 2 * rings, 1 + 2 * rings));
        assert_eq!(countries.at(500.0, 500.0), 5);
        assert_eq!(countries.at(2500.0, 500.0), 23);
        // A square without one: the nearest's (here the west one, ties the first found).
        assert_eq!(countries.at(1100.0, 500.0), 5);
        // Beyond the roads, within the search: the nearest's, on the side it's on.
        assert_eq!(countries.at(-1500.0, 500.0), 5);
        assert_eq!(countries.at(5500.0, -1500.0), 23);
        // Farther: none.
        assert_eq!(countries.at(-1500.0 - rings as f32 * CELL, 500.0), 0);
        assert_eq!(countries.at(90_000.0, 0.0), 0);
    }

    #[test]
    fn reads_each_countrys_id_and_time_zone() {
        let text = "SiiNunit\n{\ncountry_data : country.data.finland\n{\n\tname: \"Suomi\"\n\tcountry_id: 23\n\ttime_zone: 180\t\t# +3 hours in minutes\n\ttime_zone_name: \"@@tz_eest@@\"\n}\ncountry_data : country.data.uk\n{\n\tcountry_id: 10\n\ttime_zone: 60\n}\n}\n";
        assert_eq!(zones_in(text), [(23, 180), (10, 60)]);
        // One without a time zone: none.
        assert_eq!(zones_in("country_data : x\n{\ncountry_id: 3\n}\n"), []);
    }

    #[test]
    fn the_page_reads_the_squares_as_hex() {
        let mut countries = grid(&[node(100.0, 100.0, 23, 23), node(1100.0, 100.0, 5, 5)]);
        countries.zones = BTreeMap::from([(5, 120), (23, 180)]);
        let json = countries.to_json();
        let (width, height) = (countries.width, countries.height);
        assert_eq!((json["width"].as_u64(), json["height"].as_u64()), (Some(width as u64), Some(height as u64)));
        let cells = json["cells"].as_str().unwrap();
        assert_eq!(cells.len(), width * height * 2);
        // The roads' squares, in the grid's middle row past the rings around them: 23 (0x17), then 5.
        let rings = SEARCH_RINGS as usize;
        let at = (rings * width + rings) * 2;
        assert_eq!(&cells[at..at + 4], "1705");
        assert_eq!(json["zones"]["23"], 180);
    }
}
