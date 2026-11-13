//! A game's landmarks where it has no `landmark` folder of its own. ATS files its by place: in
//! `model/panorama/<place>/` (the Golden Gate's towers, Hoover Dam, St Louis's arch, along with the mountains and
//! whole skylines made to be seen from far) and in folders of a city's own (`model/lv`: the Bellagio, Caesars,
//! the wheel; `model/reno`, `model/ely`, `model/carson_city`, `model/winn`). A real landmark is one building, placed
//! once or twice on the whole map; what's placed more is filler (`lv/panorama1`, a 403 × 489 m block of city placed
//! 8 times; `ca_sf/yachts`, 9). Of those, the ones big enough to be a building, and not by their own name terrain, a
//! boat, a billboard, a wall or a tunnel, or a block of city with no building named after it (owner 2026-11-02,
//! after "your filtering was wrong": PLAN.md, "True shapes").

use std::collections::HashMap;

use crate::buildings::model_box;
use crate::files::GameFiles;
use crate::world::{BuildingKind, BuildingModel, Definitions, Map};

/// A city's own folder of landmarks.
const CITIES: [&str; 5] = ["/model/lv/", "/model/reno/", "/model/ely/", "/model/carson_city/", "/model/winn/"];
/// Placed more often than this anywhere, a model is filler.
const MOST_PLACED: usize = 2;
/// The least a landmark covers (m²) and stands (m).
const LEAST_AREA: f32 = 300.0;
const LEAST_TALL: f32 = 8.0;
/// Words in a model's own name (its folder's name taken off it: `wy_rock_springs_church` is a church) that say it's
/// no building: terrain, a boat, a billboard, a wall, a road's way in under the ground.
const LEFT_OUT: [&str; 32] = [
    "mountain", "mountains", "forest", "rock", "rocks", "hill", "hills", "ridge", "cliff", "cliffs", "canyon", "vegetation", "tree",
    "trees", "peak", "mound", "yacht", "yachts", "boat", "boats", "ship", "ships", "submarine", "empress", "advert", "billboard",
    "bilboard", "wall", "wallpaint", "trolley", "tunnel", "underground",
];
/// What may follow "panorama" or "skyline" in the name of a block of city that names no building.
const NO_NAME: [&str; 9] = ["north", "south", "far", "close", "single", "ggb", "to", "sf", "part3"];
/// Left out by the owner: Hoover Dam (2026-11-02, "remove hoover dam 3d building"): the map has no canyon for it.
const OWNER_LEFT_OUT: [&str; 1] = ["/model/panorama/us_93/az_us_93_hoover_dam.pmd"];

/// Whether a model's file is where a game without a `landmark` folder keeps its landmarks.
pub fn candidate(path: &str) -> bool {
    path.contains("/panorama/") || CITIES.iter().any(|city| path.starts_with(city))
}

/// The model's own words: the folder's name off the file's name, cut at `_` and `-`.
fn own_words(path: &str) -> Vec<String> {
    let mut parts = path.trim_end_matches(".pmd").rsplit('/');
    let stem = parts.next().unwrap_or("");
    let folder = parts.next().unwrap_or("");
    let stem = stem.strip_prefix(folder).unwrap_or(stem);
    stem.split(['_', '-']).filter(|word| !word.is_empty()).map(str::to_owned).collect()
}

/// A block of city standing behind the game's roads: "panorama" or "skyline" and no building named after it, or
/// a downtown by a letter (`ut_salt_lake_city_downtown_a`).
fn city_block(words: &[String]) -> bool {
    let unnamed = |word: &String| word.chars().all(|c| c.is_ascii_digit()) || word.len() <= 2 || NO_NAME.contains(&word.as_str());
    words.iter().enumerate().any(|(at, word)| {
        let tail = word.strip_prefix("panorama").or(word.strip_prefix("skyline"));
        let block = tail.is_some_and(|tail| tail.len() <= 2) && words[at + 1..].iter().all(unnamed);
        block || (word == "downtown" && at + 1 < words.len() && words[at + 1..].iter().all(|next| next.len() <= 2))
    })
}

/// Whether a candidate's file names a landmark, by its own words.
fn named_one(path: &str) -> bool {
    let words = own_words(path);
    !OWNER_LEFT_OUT.contains(&path) && !words.iter().any(|word| LEFT_OUT.contains(&word.as_str())) && !city_block(&words)
}

/// The map whose landmarks are kept by place: ATS's (its `landmark` folder holds 19 models; ETS2's 2,624, placed
/// 7,144 times, and what ETS2 keeps in its own `panorama` isn't looked at yet: PLAN.md, "True shapes").
const BY_PLACE: &str = "usa";

/// Picks the landmarks of `definitions`' candidates on the map `name` from how often `map` places each, and their
/// size: they're buildings from here on, as landmarks. The others' placements go.
pub fn pick(map: &mut Map, name: &str, definitions: &mut Definitions, files: &GameFiles) {
    if name != BY_PLACE {
        definitions.landmark_candidates.clear();
    }
    if definitions.landmark_candidates.is_empty() {
        keep_picked(map, definitions);
        return;
    }
    let mut placed: HashMap<u64, usize> = HashMap::new();
    for model in map.models.iter().filter(|model| definitions.landmark_candidates.contains_key(&model.model)) {
        *placed.entry(model.model).or_default() += 1;
    }
    for (model, count) in placed {
        let path = &definitions.landmark_candidates[&model];
        if count > MOST_PLACED || !named_one(path) {
            continue;
        }
        let pmg = path.strip_suffix(".pmd").map(|stem| format!("{stem}.pmg"));
        let Some(model_box) = pmg.and_then(|pmg| files.read(&pmg).ok().flatten()).and_then(|pmg| model_box(&pmg)) else {
            continue;
        };
        let size = [0, 1, 2].map(|k| model_box.max[k] - model_box.min[k]);
        if size[0] * size[2] >= LEAST_AREA && size[1] >= LEAST_TALL {
            definitions.buildings.insert(model, BuildingModel { path: path.clone(), kind: BuildingKind::Landmark });
        }
    }
    keep_picked(map, definitions);
}

/// Of `map`'s placed models, only buildings (the landmarks [`pick`] made some): a map read again after `pick` was run
/// keeps what it kept.
pub fn keep_picked(map: &mut Map, definitions: &Definitions) {
    map.models.retain(|model| definitions.buildings.contains_key(&model.model));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_of_city_is_no_landmark_but_a_tower_named_in_one_is() {
        assert!(!named_one("/model/panorama/or_portland/or_portland_panorama3.pmd"));
        assert!(!named_one("/model/panorama/ca_los_angeles/ca_la_skyline.pmd"));
        assert!(!named_one("/model/panorama/ut_salt_lake_city/ut_salt_lake_city_downtown_a.pmd"));
        assert!(!named_one("/model/reno/panorama1.pmd"));
        assert!(named_one("/model/panorama/ca_san_francisco/ca_sf_panorama_salesforce.pmd"));
        assert!(named_one("/model/reno/panorama3_harrahs.pmd"));
    }

    #[test]
    fn its_own_words_say_what_it_is_its_folder_doesnt() {
        assert!(!named_one("/model/panorama/ca_sf/yachts.pmd"));
        assert!(!named_one("/model/panorama/or_roxy_ann_peak.pmd"));
        assert!(named_one("/model/panorama/wy_rock_springs/wy_rock_springs_church.pmd"));
        assert!(named_one("/model/panorama/ca_san_francisco/ca_sf_ggb_pillar_1.pmd"));
        assert!(!named_one("/model/panorama/us_93/az_us_93_hoover_dam.pmd"));
        assert!(named_one("/model/panorama/us_93/az_us_93_hoover_dam_bridge.pmd"));
    }
}
