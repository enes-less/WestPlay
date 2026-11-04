//! What of a map the player's game has: ATS's base map holds every state's roads, junctions, terrain and buildings,
//! each guarded by the map DLCs it needs (a road into Washington is there for a player without Washington, hidden
//! in the game). The game's guards are numbers into its own list of names (`dlc_<state>`, or `dlc_<state>_and_
//! <state>` for what two states share), each state by its `country_code`; a state the game has is one whose cities
//! are defined (each state's DLC adds its own `def/city.<dlc>.sii`; the base game, California's).
//!
//! The list is only in the game's executable (`amtrucks.exe`; no def file has it), so it's read from there on every
//! build ([`read_guards`]): the run of its names, guard 1 the first, 0 the base game's (57 names in 1.61.3.18 and
//! 1.62.1.1, the same; checked 2026-10-28 against the map, each guard's roads in its states). The guard is an
//! item's flags' second byte for junctions, map areas, overlays and companies, a road's own (its data's); terrain,
//! buildings, models and the like have none the map shows, so they're kept or not by the state they lie in. Every
//! visible road in a state the game hasn't is guarded (the unguarded there are hidden template roads). A guard past
//! the list (the map has some, for states not out yet) is a DLC no one has. Two states' guards are taken to need
//! both, so that no road shows that the player can't drive.

use std::collections::{HashMap, HashSet};

use crate::countries::Countries;
use crate::files::{FileError, GameFiles};
use crate::sector::ItemData;
use crate::sii;
use crate::world::Map;

/// The game's list of its map DLCs' guards, from its executable's bytes, in its order (guard `n` the `n - 1`th):
/// the one run of NUL-ended names `dlc_<xx>` and `dlc_<xx>_and_<yy>` (two-letter state codes) in it, at least two
/// long. `None` if there's none, or more than one: then it isn't known which is the list.
pub fn read_guards(executable: &[u8]) -> Option<Vec<String>> {
    let is_guard = |name: &[u8]| {
        let Some(codes) = name.strip_prefix(b"dlc_") else { return false };
        let codes: Vec<&[u8]> = split_and(codes);
        !codes.is_empty() && codes.iter().all(|code| code.len() == 2 && code.iter().all(u8::is_ascii_lowercase))
    };
    let mut runs: Vec<Vec<String>> = Vec::new();
    let mut at = 0;
    while let Some(found) = find(&executable[at..], b"dlc_").map(|i| at + i) {
        // A name starts after a NUL (strings end in NULs, padded with more).
        if found > 0 && executable[found - 1] != 0 {
            at = found + 1;
            continue;
        }
        let mut run = Vec::new();
        let mut next = found;
        while let Some(end) = executable[next..].iter().position(|b| *b == 0).map(|i| next + i) {
            let name = &executable[next..end];
            if !is_guard(name) {
                break;
            }
            run.push(String::from_utf8_lossy(name).into_owned());
            next = end;
            while executable.get(next) == Some(&0) {
                next += 1;
            }
        }
        at = next.max(found + 1);
        if run.len() >= 2 {
            runs.push(run);
        }
    }
    (runs.len() == 1).then(|| runs.remove(0))
}

/// `a_and_b_and_c` as `[a, b, c]`.
fn split_and(codes: &[u8]) -> Vec<&[u8]> {
    let mut parts = Vec::new();
    let mut rest = codes;
    while let Some(i) = find(rest, b"_and_") {
        parts.push(&rest[..i]);
        rest = &rest[i + 5..];
    }
    parts.push(rest);
    parts
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// Which guards and which countries (states) the player's game has.
#[derive(Debug, Clone)]
pub struct Owned {
    /// By guard: whether the game has everything it needs.
    guards: [bool; 256],
    /// By country id (as the map's nodes give them): whether the game has the country.
    countries: [bool; 256],
}

/// A country as the game defines it: its id (the map's nodes'), code (`NV`) and name (`nevada`).
#[derive(Debug, Clone, PartialEq)]
pub struct Country {
    pub id: u8,
    pub code: String,
    pub name: String,
}

impl Owned {
    /// What the player's game has, by its guards' names (its executable's: [`read_guards`]).
    pub fn read(files: &GameFiles, guards: &[String]) -> Result<Self, FileError> {
        let countries = read_countries(files)?;
        let with_cities: HashSet<String> = sii::read_family(files, "/def", "city")?
            .iter()
            .filter_map(|city| city.get("country").map(|country| country.trim_matches('"').to_owned()))
            .collect();
        Ok(Self::from_parts(guards, &countries, &with_cities))
    }

    /// From the guards' names, the game's countries, and the names of those with cities.
    pub fn from_parts(guards: &[impl AsRef<str>], countries: &[Country], with_cities: &HashSet<String>) -> Self {
        let mut owned = [false; 256];
        let by_code: HashMap<String, &Country> = countries.iter().map(|c| (c.code.to_lowercase(), c)).collect();
        for country in countries {
            owned[country.id as usize] = with_cities.contains(&country.name);
        }
        let mut by_guard = [false; 256];
        by_guard[0] = true;
        for (index, name) in guards.iter().enumerate().take(255) {
            let Some(codes) = name.as_ref().strip_prefix("dlc_") else { continue };
            // Every state it names, each one the game has.
            by_guard[index + 1] =
                codes.split("_and_").all(|code| by_code.get(code).is_some_and(|country| owned[country.id as usize]));
        }
        Self { guards: by_guard, countries: owned }
    }

    pub fn guard(&self, guard: u8) -> bool {
        self.guards[guard as usize]
    }

    /// Whether the game has the country `id` (0, none: kept).
    pub fn country(&self, id: u8) -> bool {
        id == 0 || self.countries[id as usize]
    }
}

/// The game's countries (`def/country.sii`, its includes, and every `country.*.sii` a DLC or mod adds).
pub fn read_countries(files: &GameFiles) -> Result<Vec<Country>, FileError> {
    let units = sii::read_family(files, "/def", "country")?;
    Ok(units
        .iter()
        .filter(|unit| unit.class == "country_data")
        .filter_map(|unit| {
            let name = unit.name.strip_prefix("country.data.")?.to_owned();
            let id = unit.get("country_id")?.split_whitespace().next()?.parse().ok()?;
            let code = unit.get("country_code")?.trim_matches('"').to_owned();
            Some(Country { id, code, name })
        })
        .collect())
}

/// What [`keep_owned`] took out of the map: by its guard, and by the state it lies in.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Dropped {
    pub guarded: usize,
    pub elsewhere: usize,
}

/// The map as the player's game has it: items whose guard needs a DLC it hasn't left out, and the unguarded
/// (terrain, buildings, models…) lying in a state it hasn't (by `countries`, the map's countries grid).
pub fn keep_owned(map: &mut Map, owned: &Owned, countries: &Countries) -> Dropped {
    let mut dropped = Dropped::default();
    let in_owned = |x: f32, z: f32| owned.country(countries.at(x, z));
    map.items.retain(|item| {
        let (guard, hidden) = match &item.data {
            ItemData::Road { flags, .. }
            | ItemData::Prefab { flags, .. }
            | ItemData::MapArea { flags, .. }
            | ItemData::MapOverlay { flags, .. } => (Some(flags.dlc_guard), flags.hidden),
            ItemData::Company { dlc_guard, .. } => (Some(*dlc_guard), false),
            _ => (None, false),
        };
        let [x0, z0, x1, z1] = item.bounds;
        let inside = in_owned((x0 + x1) / 2.0, (z0 + z1) / 2.0);
        let keep = match guard {
            // A hidden road, junction or map area (the game's navigation's alone, never drawn: templates in every
            // state, unguarded) only in a state the game has, too.
            Some(guard) if hidden => owned.guard(guard) && inside,
            Some(guard) => owned.guard(guard),
            None => inside,
        };
        if !keep {
            if guard.is_some() {
                dropped.guarded += 1;
            } else {
                dropped.elsewhere += 1;
            }
        }
        keep
    });
    let nodes = &map.nodes;
    let at_node = |uid: u64| nodes.get(&uid).is_none_or(|node| in_owned(node.position[0], node.position[2]));
    let before = map.models.len() + map.walls.len();
    map.models.retain(|model| at_node(model.node));
    map.walls.retain(|wall| at_node(wall.start));
    dropped.elsewhere += before - map.models.len() - map.walls.len();
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn country(id: u8, code: &str, name: &str) -> Country {
        Country { id, code: code.to_owned(), name: name.to_owned() }
    }

    #[test]
    fn a_guard_needs_every_state_it_names_and_a_state_needs_its_cities() {
        let countries = [country(1, "CA", "california"), country(37, "OR", "oregon"), country(47, "WA", "washington")];
        let with_cities: HashSet<String> = ["california", "oregon"].iter().map(|c| c.to_string()).collect();
        let owned = Owned::from_parts(&["dlc_or", "dlc_wa", "dlc_wa_and_or", "dlc_xx"], &countries, &with_cities);
        assert!(owned.guard(0), "the base game's");
        assert!(owned.guard(1), "Oregon's");
        assert!(!owned.guard(2), "Washington's");
        assert!(!owned.guard(3), "Washington and Oregon's: both needed");
        assert!(!owned.guard(4), "a state the game doesn't define");
        assert!(!owned.guard(5), "past the list");
        assert!(owned.country(0) && owned.country(1) && owned.country(37));
        assert!(!owned.country(47));
    }

    #[test]
    fn reads_the_one_list_of_guards_in_the_executable() {
        let mut exe = b"\x01\x02.bat\0\0\0dlc_nv\0dlc_az\0\0\0\0dlc_wa_and_or\0dlc_tx\0dlc_\0".to_vec();
        exe.extend_from_slice(b"\0\0xdlc_ca\0dlc_nope\0dlc_xx_and\0");
        assert_eq!(read_guards(&exe), Some(vec!["dlc_nv".into(), "dlc_az".into(), "dlc_wa_and_or".into(), "dlc_tx".into()]));
        // Two lists: which is the game's isn't known.
        exe.extend_from_slice(b"\0dlc_ut\0dlc_id\0");
        assert_eq!(read_guards(&exe), None);
        assert_eq!(read_guards(b"\0dlc_nv\0other\0"), None, "one name is no list");
    }

    /// The installed game's own list (`WESTPLAY_ATS_EXE`: its `amtrucks.exe`), as measured 2026-10-28 on 1.61.3.18
    /// and 2026-11-04 on 1.62.1.1.
    #[test]
    #[ignore = "needs the game: WESTPLAY_ATS_EXE"]
    fn reads_the_games_own_list() {
        let exe = std::fs::read(std::env::var("WESTPLAY_ATS_EXE").expect("WESTPLAY_ATS_EXE")).unwrap();
        let guards = read_guards(&exe).expect("a list");
        assert_eq!((guards.len(), guards[0].as_str(), guards[21].as_str()), (57, "dlc_nv", "dlc_mt"));
        assert_eq!(guards[56], "dlc_sd_and_wy");
    }
}
