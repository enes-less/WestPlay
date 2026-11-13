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
    /// Parts of countries in another time zone than the rest of theirs.
    pub areas: Vec<ZoneArea>,
    /// The zone the SDK's game time is in (minutes), the game's (`None`: not said, ETS2's central Europe).
    pub base: Option<i32>,
}

/// A part of a country in another time zone than the rest of it (a `country_data`'s
/// `secondary_time_zone_area[]` and `secondary_time_zone[]`: ATS's Navajo Nation in Arizona, El Paso in Texas,
/// on Mountain time; read 2026-10-28, El Paso at (-49616, 38395) inside Texas's): the country, its rectangle
/// (west and east x, north and south z, metres) and its zone (minutes from UTC).
#[derive(Clone, Debug, PartialEq)]
pub struct ZoneArea {
    pub country: u8,
    pub x: [f32; 2],
    pub z: [f32; 2],
    pub zone: i32,
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
    for ((column, row), counts) in &votes {
        // The most, the lower id of a tie: the same every build.
        let most = counts
            .iter()
            .map(|(&country, &count)| (country, count))
            .max_by_key(|&(country, count)| (count, std::cmp::Reverse(country)));
        let at = (row - rows.0) as usize * width + (column - columns.0) as usize;
        voted[at] = most.map_or(0, |(country, _)| country);
    }
    // A square with no country of its own takes the nearest within the rings: only those within the rings of a voted
    // square can, so only they're looked round (every square of the grid was: a stray node far out made billions of
    // looks, the review of 2026-11-11, minor 17).
    let mut cells = voted.clone();
    let mut looked = vec![false; width * height];
    for &(column, row) in votes.keys() {
        let (column, row) = ((column - columns.0) as i64, (row - rows.0) as i64);
        for r in (row - SEARCH_RINGS).max(0)..=(row + SEARCH_RINGS).min(height as i64 - 1) {
            for c in (column - SEARCH_RINGS).max(0)..=(column + SEARCH_RINGS).min(width as i64 - 1) {
                let at = r as usize * width + c as usize;
                if voted[at] == 0 && !looked[at] {
                    looked[at] = true;
                    cells[at] = nearest(&voted, width, height, c, r);
                }
            }
        }
    }
    let (x, z) = (columns.0 as f32 * CELL, rows.0 as f32 * CELL);
    Countries { x, z, width, height, cells, zones: BTreeMap::new(), areas: Vec::new(), base: None }
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

/// Each country's time zone (minutes), by id, and the parts of countries in another: from the game's country
/// definitions (`def/country/*.sui`, and a mod's `def/country.<mod>.sii`), map mods' countries with theirs.
pub fn time_zones(files: &GameFiles) -> Result<(BTreeMap<u8, i32>, Vec<ZoneArea>), FileError> {
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
    let (mut zones, mut areas) = (BTreeMap::new(), Vec::new());
    for path in paths {
        let Some(bytes) = files.read(&path)? else { continue };
        let text = String::from_utf8_lossy(&bytes);
        zones.extend(zones_in(&text));
        areas.extend(areas_in(&text));
    }
    Ok((zones, areas))
}

/// The parts of countries in another time zone ([`ZoneArea`]) in a file's text: each country definition's
/// `secondary_time_zone_area[]: (x0, x1, z0, z1)` and `secondary_time_zone[]` in their order.
fn areas_in(text: &str) -> Vec<ZoneArea> {
    let mut found = Vec::new();
    let (mut id, mut rectangles, mut zones) = (None::<u8>, Vec::new(), Vec::new());
    let mut done = |id: Option<u8>, rectangles: &mut Vec<[f32; 4]>, zones: &mut Vec<i32>| {
        if let Some(country) = id {
            for (r, &zone) in rectangles.iter().zip(zones.iter()) {
                found.push(ZoneArea { country, x: [r[0], r[1]], z: [r[2], r[3]], zone });
            }
        }
        rectangles.clear();
        zones.clear();
    };
    for line in text.lines() {
        let line = line.trim();
        if line.contains("country_data") && line.contains(':') && !line.starts_with('#') {
            done(id, &mut rectangles, &mut zones);
            id = None;
        }
        let after = |name: &str| Some(line.strip_prefix(name)?.trim_start().strip_prefix(':')?.trim());
        if let Some(value) = after("country_id") {
            id = value.split(|c: char| c.is_whitespace() || c == '#').next().and_then(|word| word.parse().ok());
        } else if let Some(value) = after("secondary_time_zone_area[]") {
            let inside = value.trim_start_matches('(').split(')').next().unwrap_or_default();
            let numbers: Vec<f32> = inside.split(',').filter_map(|number| number.trim().parse().ok()).collect();
            if let [x0, x1, z0, z1] = numbers[..] {
                rectangles.push([x0, x1, z0, z1]);
            }
        } else if let Some(value) = after("secondary_time_zone[]")
            && let Some(zone) = value.split(|c: char| c.is_whitespace() || c == '#').next().and_then(|w| w.parse().ok())
        {
            zones.push(zone);
        }
    }
    done(id, &mut rectangles, &mut zones);
    found
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

    /// The time zone at `[x, z]` (minutes): of the part of its country there in another zone ([`ZoneArea`]), else
    /// of its country; `None` where neither is known. The page works it out the same (maps/countries.ts).
    pub fn zone_at(&self, x: f32, z: f32) -> Option<i32> {
        let country = self.at(x, z);
        let inside = |area: &&ZoneArea| {
            area.country == country && (area.x[0]..=area.x[1]).contains(&x) && (area.z[0]..=area.z[1]).contains(&z)
        };
        self.areas.iter().find(inside).map(|area| area.zone).or_else(|| self.zones.get(&country).copied())
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
            "base": self.base,
            "areas": self.areas.iter().map(|area| serde_json::json!({
                "country": area.country, "x": area.x, "z": area.z, "zone": area.zone,
            })).collect::<Vec<_>>(),
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

    /// Each square as when every square of the grid looked round itself: random roads in clusters far apart.
    #[test]
    fn only_squares_near_roads_look_round_and_each_comes_out_as_before() {
        let mut seed = 0x51_7C_C1_B7_27_22_0A_95u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..20 {
            let mut nodes = Vec::new();
            for cluster in 0..3 {
                let (cx, cz) = (cluster as f32 * 60_000.0, (next() % 40_000) as f32);
                for _ in 0..40 {
                    let (x, z) = (cx + (next() % 20_000) as f32, cz + (next() % 20_000) as f32);
                    let country = (next() % 4) as u8;
                    nodes.push(node(x, z, country, if next() % 3 == 0 { 0 } else { country }));
                }
            }
            let countries = grid(&nodes);
            let (width, height) = (countries.width, countries.height);
            // Each square's own votes again, and every square looking round as before.
            let mut voted = vec![0u8; width * height];
            for (at, own_vote) in voted.iter_mut().enumerate() {
                let (x, z) = (countries.x + (at % width) as f32 * CELL, countries.z + (at / width) as f32 * CELL);
                let inside =
                    |n: &&Node| (x..x + CELL).contains(&n.position[0]) && (z..z + CELL).contains(&n.position[2]);
                let own: Vec<&Node> = nodes.iter().filter(inside).collect();
                let mut counts: HashMap<u8, u32> = HashMap::new();
                for n in own {
                    for country in countries_of(n).into_iter().filter(|&c| c != 0) {
                        *counts.entry(country).or_default() += 1;
                    }
                }
                *own_vote = counts.into_iter().max_by_key(|&(c, n)| (n, std::cmp::Reverse(c))).map_or(0, |(c, _)| c);
            }
            for at in 0..width * height {
                let before = match voted[at] {
                    0 => nearest(&voted, width, height, (at % width) as i64, (at / width) as i64),
                    country => country,
                };
                assert_eq!(countries.cells[at], before, "square {at}");
            }
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
    fn reads_the_parts_of_countries_in_another_time_zone() {
        // ATS's Texas (1.61), and a country with none.
        let text = "SiiNunit
{
country_data : country.data.texas
{
	country_id: 43
	time_zone: -300	# -5 hours
	time_zone_name: \"@@tz_cdt@@\"
	secondary_time_zone_area[]: (-51286.5, -43729.5, 37159.7, 42864.0)
	secondary_time_zone[]: -360
	secondary_time_zone_name[]: \"@@tz_mdt@@\"
}
country_data : country.data.illinois
{
	country_id: 14
	time_zone: -300
}
}
";
        assert_eq!(zones_in(text), [(43, -300), (14, -300)]);
        assert_eq!(
            areas_in(text),
            [ZoneArea { country: 43, x: [-51286.5, -43729.5], z: [37159.7, 42864.0], zone: -360 }]
        );
        // El Paso in it (Mountain time), Dallas outside it (Central): the grid all Texas.
        let mut texas = grid(&[node(-49616.0, 38395.0, 43, 43), node(-30000.0, 38000.0, 43, 43)]);
        (texas.zones, texas.areas) = (BTreeMap::from([(43, -300)]), areas_in(text));
        assert_eq!(texas.zone_at(-49616.0, 38395.0), Some(-360));
        assert_eq!(texas.zone_at(-30000.0, 38000.0), Some(-300));
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
