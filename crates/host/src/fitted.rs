//! The parts fitted to the player's truck and the speakers they carry, for the trucks whose speakers come with
//! their parts (a mod truck's door panels, A-pillars, top console and rear wall, each chosen in the truck shop;
//! the Renault T's interior, older ones without door speakers: speakers.rs's [`speakers::by_parts`]).
//!
//! The parts are the save's: the game writes each of the player's vehicles with its parts' definitions (its
//! `accessories`, each a unit with a `data_path`), so a part changed in the truck shop counts once the game
//! saves (it autosaves as the player drives). The truck is found among the vehicles by its model (its
//! `data.sii`'s folder: `scania.5s`), then by its plate (the SDK's against the save's `license_plate`), then as
//! the one the player drives (`assigned_truck`, which the owner's save had empty). Its definitions are read
//! from the game and the profile's mods: the interior's model, whose locators place the parts (one named after
//! each part's slot, the last word of its unit's name: `int_rearwall`), and each part's model, whose speakers
//! speakers.rs knows. The interior model's origin is where the SDK puts the driver's head by default (as in
//! every truck measured), so a speaker is, in cab space, the head plus its locator plus where it is in its part,
//! turned as the locator turns it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use westplay_mapdata::archive::Archive;
use westplay_mapdata::buildings::{self, Locator};
use westplay_mapdata::files::{GameFiles, Mount};
use westplay_mapdata::sii::{self, Unit};
use westplay_shared::TruckInfo;

use crate::speakers::{self, Placed, Speaker};

/// One of the player's trucks as the save has it.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedTruck {
    /// Its model, as its definitions' folder names it (`scania.5s`).
    pub model: String,
    /// Its plate as the save writes it, less the country (`CA 324KD`).
    pub plate: String,
    /// The plate's country (`italy`).
    pub country: String,
    /// The one the player drives, as the save says (`assigned_truck`).
    pub assigned: bool,
    /// Its parts' definitions (`/def/vehicle/truck/scania.5s/interior/topline_6gp.sii`…).
    pub parts: Vec<String>,
}

/// Every truck of the player's in a save's units (their vehicles with a truck's `data.sii`).
pub fn saved_trucks(units: &[Unit]) -> Vec<SavedTruck> {
    let paths: HashMap<&str, &str> =
        units.iter().filter_map(|unit| Some((unit.name.as_str(), unit.get("data_path")?))).collect();
    let player = units.iter().find(|unit| unit.class == "player");
    let assigned = player.and_then(|player| player.get("assigned_truck")).filter(|name| *name != "null");
    units
        .iter()
        .filter(|unit| unit.class == "vehicle")
        .filter_map(|vehicle| {
            let parts: Vec<String> =
                vehicle.all("accessories").filter_map(|part| paths.get(part)).map(|path| (*path).to_owned()).collect();
            let model = parts.iter().find_map(|path| {
                path.strip_prefix("/def/vehicle/truck/")?.strip_suffix("/data.sii").filter(|model| !model.contains('/'))
            })?;
            let (plate, country) =
                vehicle.get("license_plate").unwrap_or_default().rsplit_once('|').unwrap_or_default();
            Some(SavedTruck {
                model: model.to_owned(),
                plate: plate.trim().to_owned(),
                country: country.trim().to_owned(),
                assigned: assigned == Some(vehicle.name.as_str()),
                parts,
            })
        })
        .collect()
}

/// The saved truck the SDK says the player is in: the one of its model with its plate (the one the player drives,
/// if two have it). A truck the save doesn't have yet (bought or changed since the game last saved, or a quick
/// job's) is none of them, not another of its model, whose parts it hasn't got (2026-10-26: a 2013 Renault T took
/// the save's only T's 2024 interior). Without a plate from the SDK: the only one of its model, else the one driven.
pub fn find<'a>(trucks: &'a [SavedTruck], truck: &TruckInfo) -> Option<&'a SavedTruck> {
    let model = truck.id.as_str().strip_prefix("vehicle.").unwrap_or(truck.id.as_str());
    let of_model: Vec<&SavedTruck> = trucks.iter().filter(|saved| saved.model == model).collect();
    let pick = |candidates: Vec<&'a SavedTruck>| match candidates.as_slice() {
        [only] => Some(*only),
        _ => candidates.into_iter().find(|saved| saved.assigned),
    };
    if truck.plate.as_str().trim().is_empty() {
        return pick(of_model);
    }
    pick(
        of_model
            .into_iter()
            .filter(|saved| {
                same_plate(&saved.plate, truck.plate.as_str()) && saved.country == truck.plate_country.as_str()
            })
            .collect(),
    )
}

/// Whether two plates read the same: the game's markup (`<offset …>`, `<img …>`) and spacing aside.
fn same_plate(a: &str, b: &str) -> bool {
    fn plain(plate: &str) -> String {
        let mut text = String::new();
        let mut in_tag = false;
        for c in plate.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag && !c.is_whitespace() => text.extend(c.to_lowercase()),
                _ => {}
            }
        }
        text
    }
    let a = plain(a);
    !a.is_empty() && a == plain(b)
}

/// The game with the mods among `mods` (their files, in the profile's load order: each over those before) that
/// have definitions for the truck `model`, where its parts are found.
pub fn mount(game_dir: &Path, mods: &[PathBuf], model: &str) -> Result<GameFiles, String> {
    let mut files = GameFiles::install(game_dir).map_err(|e| e.to_string())?;
    let defs = format!("def/vehicle/truck/{model}");
    for path in mods {
        if path.is_dir() {
            if path.join(&defs).is_dir() {
                files.mount(Mount::Folder(path.clone()));
            }
            continue;
        }
        let Ok(archive) = Archive::open(path) else { continue };
        // Its folder listed, or (an archive whose folders are hidden) the truck's own definition in it.
        let listed = archive.list(&defs).ok().flatten().is_some();
        if listed || archive.contains(&format!("{defs}/data.sii")) {
            files.mount(Mount::Archive(archive));
        }
    }
    Ok(files)
}

/// What a saved truck's parts carry: its speakers in the cab, and for the log, each part with speakers (its
/// slot and its model's file name, how many).
#[derive(Clone, Debug, PartialEq)]
pub struct Fitted {
    pub speakers: Vec<Speaker>,
    pub parts: Vec<String>,
}

/// The speakers of `truck`'s parts, placed from `head` (the SDK's default head: the interior model's origin),
/// read from `files`; `None` if none of its parts has speakers known.
pub fn speakers(files: &GameFiles, truck: &SavedTruck, head: [f32; 3]) -> Result<Option<Fitted>, String> {
    let interior = truck
        .parts
        .iter()
        .find(|path| path.starts_with(&format!("/def/vehicle/truck/{}/interior/", truck.model)))
        .ok_or("no interior among its parts")?;
    let interior_model =
        definition(files, interior)?.get("model").map(str::to_owned).ok_or("the interior has no model")?;
    let pmg = files
        .read(&interior_model.replace(".pmd", ".pmg"))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{interior_model} not found"))?;
    let locators = buildings::model_locators(&pmg).ok_or_else(|| format!("{interior_model}'s locators don't read"))?;
    let mut placed: Vec<Placed> = Vec::new();
    let mut parts = Vec::new();
    // The interior's own speakers, at its origin.
    let origin = Locator { name: String::new(), position: [0.0; 3], rotation: [1.0, 0.0, 0.0, 0.0] };
    let mut fit = |slot: &str, model: &str, locator: &Locator| {
        let Some(units) = speakers::of_model(model) else { return };
        let part = parts.len();
        placed.extend(units.iter().map(|unit| Placed {
            role: unit.role,
            size: unit.size,
            part,
            at: add(head, place(locator, unit.at)),
        }));
        let file = model.rsplit('/').next().unwrap_or(model);
        parts.push(format!("{slot} {file} ({})", units.len()));
    };
    // The interior's speakers by its definition where its model has more than one layout (the DAF XF 105's: its
    // cabs' interiors share one model), else by its model.
    let interior_key = if speakers::of_model(interior).is_some() { interior.as_str() } else { interior_model.as_str() };
    fit("interior", interior_key, &origin);
    for path in truck.parts.iter().filter(|path| path.contains("/accessory/")) {
        let Ok(part) = definition(files, path) else { continue };
        let Some(model) = part.get("interior_model") else { continue };
        let slot = part.name.rsplit('.').next().unwrap_or_default();
        let Some(locator) = locators.iter().find(|locator| locator.name == slot) else { continue };
        fit(slot, model, locator);
    }
    if placed.is_empty() {
        return Ok(None);
    }
    Ok(Some(Fitted { speakers: speakers::assemble(&placed), parts }))
}

/// A definition file's unit.
fn definition(files: &GameFiles, path: &str) -> Result<Unit, String> {
    sii::read_units(files, path)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|unit| !unit.class.is_empty())
        .ok_or_else(|| format!("{path} has no definition"))
}

/// `at`, a point of what's fitted at `locator`, in the model the locator is in: turned by its rotation, then moved
/// to it.
fn place(locator: &Locator, at: [f32; 3]) -> [f32; 3] {
    let [w, x, y, z] = locator.rotation;
    let u = [x, y, z];
    // v + 2w(u × v) + 2u × (u × v)
    let cross =
        |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let t = cross(u, at).map(|v| 2.0 * v);
    let turned = add(add(at, t.map(|v| w * v)), cross(u, t));
    add(turned, locator.position)
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[cfg(test)]
mod tests {
    use westplay_shared::FixedStr;

    use super::*;

    fn unit(class: &str, name: &str, attributes: &[(&str, &str)]) -> Unit {
        Unit {
            class: class.into(),
            name: name.into(),
            attributes: attributes.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect(),
        }
    }

    /// A save with two PGRs (plates `CA 324KD` and `AB 111CD`, the second the one driven) and an S.
    fn save() -> Vec<Unit> {
        let vehicle = |name: &str, plate: &str, parts: &[&str]| {
            let mut attributes: Vec<(&str, &str)> = parts.iter().map(|part| ("accessories", *part)).collect();
            attributes.push(("license_plate", plate));
            unit("vehicle", name, &attributes)
        };
        let part = |name: &str, path: &str| unit("vehicle_accessory", name, &[("data_path", path)]);
        vec![
            unit("player", "player", &[("assigned_truck", "_nameless.2"), ("my_truck", "null")]),
            vehicle("_nameless.1", "CA 324KD   |italy", &["_p.1", "_p.2", "_p.3"]),
            vehicle("_nameless.2", "AB 111CD|italy", &["_p.4", "_p.5"]),
            vehicle("_nameless.3", "<offset hshift=-4>W 123<offset hshift=4>|germany", &["_p.6"]),
            part("_p.1", "/def/vehicle/truck/scania.5s/data.sii"),
            part("_p.2", "/def/vehicle/truck/scania.5s/interior/topline_6gp.sii"),
            part("_p.3", "/def/vehicle/truck/scania.5s/accessory/int_rearwall/sp8.sii"),
            part("_p.4", "/def/vehicle/truck/scania.5s/data.sii"),
            part("_p.5", "/def/vehicle/truck/scania.5s/interior/day_6.sii"),
            part("_p.6", "/def/vehicle/truck/scania.s_2016/data.sii"),
        ]
    }

    fn sdk_truck(id: &str, plate: &str, country: &str) -> TruckInfo {
        TruckInfo {
            id: FixedStr::new(id),
            plate: FixedStr::new(plate),
            plate_country: FixedStr::new(country),
            ..TruckInfo::default()
        }
    }

    #[test]
    fn reads_each_truck_of_the_save_with_its_parts() {
        let trucks = saved_trucks(&save());
        assert_eq!(trucks.len(), 3);
        assert_eq!(trucks[0].model, "scania.5s");
        assert_eq!((trucks[0].plate.as_str(), trucks[0].country.as_str()), ("CA 324KD", "italy"));
        assert_eq!(trucks[0].parts.len(), 3);
        assert!(!trucks[0].assigned && trucks[1].assigned);
        assert_eq!(trucks[2].model, "scania.s_2016");
    }

    #[test]
    fn finds_the_truck_by_its_model_and_plate() {
        let trucks = saved_trucks(&save());
        // By the plate, the game's spacing and markup aside.
        let by_plate = find(&trucks, &sdk_truck("vehicle.scania.5s", "CA 324KD", "italy"));
        assert_eq!(by_plate.map(|t| t.plate.as_str()), Some("CA 324KD"));
        let marked = find(&trucks, &sdk_truck("vehicle.scania.s_2016", "W 123", "germany"));
        assert_eq!(marked.map(|t| t.model.as_str()), Some("scania.s_2016"));
        // A plate the save hasn't got (or another country's): a truck it doesn't have yet, not another of its
        // model, even the only one.
        assert_eq!(find(&trucks, &sdk_truck("vehicle.scania.5s", "CA 324KD", "france")), None);
        assert_eq!(find(&trucks, &sdk_truck("vehicle.scania.s_2016", "DB 798XK", "italy")), None);
        assert_eq!(find(&trucks, &sdk_truck("vehicle.daf.2021", "CA 324KD", "italy")), None);
        // No plate from the SDK: the only one of its model, else the one the player drives.
        assert_eq!(
            find(&trucks, &sdk_truck("vehicle.scania.s_2016", "", "")).map(|t| t.model.as_str()),
            Some("scania.s_2016")
        );
        assert_eq!(find(&trucks, &sdk_truck("vehicle.scania.5s", "", "")).map(|t| t.plate.as_str()), Some("AB 111CD"));
    }

    #[test]
    fn reads_plates_the_same_whatever_the_markup_and_spacing() {
        assert!(same_plate("CA 324KD   ", "CA 324KD"));
        assert!(same_plate("<offset hshift=-4>W 123<offset hshift=4>", "W123"));
        assert!(!same_plate("CA 324KD", "CA 324KE"));
        assert!(!same_plate("", ""));
    }

    #[test]
    fn places_a_part_at_its_locator_turned_as_it_turns() {
        let straight =
            Locator { name: "int_rearwall".into(), position: [0.681, -0.025, 1.115], rotation: [1.0, 0.0, 0.0, 0.0] };
        let at = place(&straight, [-0.857, 0.124, -0.060]);
        assert!(at.iter().zip([-0.176, 0.099, 1.055]).all(|(a, b)| (a - b).abs() < 1e-5), "{at:?}");
        // A quarter turn about y (counterclockwise seen from above): what pointed right points forward.
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let turned = Locator { rotation: [s, 0.0, s, 0.0], position: [0.0; 3], ..straight };
        let at = place(&turned, [1.0, 0.0, 0.0]);
        assert!(at.iter().zip([0.0, 0.0, -1.0]).all(|(a, b)| (a - b).abs() < 1e-5), "{at:?}");
    }
}
