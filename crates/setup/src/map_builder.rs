//! A map built in a process of its own, `westplay-maps.exe` (owner 2026-11-06): WestPlay hands it what to build
//! ([`Request`], one JSON line on its standard input) and hears how it goes ([`Said`], a JSON line each on its
//! standard output). A build that crashes (a mod read wrong, 2026-11-06: one ended WestPlay with it) ends only that
//! process; and the process can use a memory allocator of its own, mimalloc, which a faster build draws twice as
//! fast with (Windows' own heap takes a lock every thread waits on: the buildings' models 50 s with it, 6 s
//! without, the base game's map 217 s to 133 s), where WestPlay's own process keeps Windows'.
//!
//! [`build`] is the build itself, as the process runs it, and as WestPlay runs it in its own process where the
//! process isn't there (a development build that hasn't built it).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use westplay_mapdata::export::{MapSize, Progress, Stage};

use crate::game::Game;
use crate::map::{self, BuildError, LeftOut};
use crate::profiles::Profile;

/// The map builder's file, beside WestPlay's.
pub const FILE_NAME: &str = "westplay-maps.exe";

/// The environment variable that tells the map builder which memory allocator to use ([`MIMALLOC`]: mimalloc;
/// otherwise Windows'), read before its first allocation: a process has one for its whole life.
pub const ALLOCATOR_VARIABLE: &str = "WESTPLAY_MAP_ALLOCATOR";
pub const MIMALLOC: &str = "mimalloc";

/// What to build.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub game: Game,
    pub profile: Profile,
    /// The language place names come in (`tr_tr`).
    pub language: Option<String>,
    /// WestPlay's maps folder: the map goes into its own folder in it.
    pub maps: PathBuf,
    /// Prepare Maps Faster: every thread, more drawn at once while the memory has room.
    pub fast: bool,
}

/// How a build goes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Said {
    /// How far it's got (`stage`: `reading`, `drawing`, `tiles`, `done`), and the memory the build has taken
    /// (bytes; `None`: not known).
    Progress {
        stage: String,
        done: f32,
        size: Option<Size>,
        memory: Option<u64>,
    },
    Built(Outcome),
    Failed(Failure),
}

/// How big the map is to draw (`export::MapSize`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Size {
    pub items: usize,
    pub extent: [f32; 4],
    pub sea_cells: u64,
    pub sea_cell: f32,
}

/// What a finished build says beyond the map (each logged by WestPlay).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// Map mods left out, their files unreadable.
    pub left_out: Vec<LeftOut>,
    /// Why the countries' time zones weren't read, if they weren't.
    pub time_zones_unread: Option<String>,
    /// What of the map DLCs the game hasn't was left out: by its guard, and by the state it lies in (`None`: the
    /// map drawn whole).
    pub not_owned: Option<(usize, usize)>,
    /// How well the game's world picture fits the roads, and whether it was used.
    pub world: Option<(f32, bool)>,
}

/// Why a build didn't make the map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "camelCase")]
pub enum Failure {
    /// The game is running: built once it's closed.
    GameRunning,
    Error {
        error: String,
    },
}

/// Builds `request`'s map in this process, telling `said` how it goes, and says how it ended.
pub fn build(request: &Request, said: &mut dyn FnMut(Said), memory: &dyn Fn() -> Option<u64>) -> Said {
    let mut tell = |progress: Progress| {
        said(Said::Progress {
            stage: stage_name(progress.stage).to_owned(),
            done: progress.done,
            size: progress.size.map(|size| Size {
                items: size.items,
                extent: size.extent,
                sea_cells: size.sea_cells,
                sea_cell: size.sea_cell,
            }),
            memory: memory(),
        })
    };
    let built =
        map::build(&request.game, &request.profile, request.language.as_deref(), &request.maps, None, &mut tell);
    match built {
        Ok(built) => Said::Built(Outcome {
            left_out: built.left_out,
            time_zones_unread: built.summary.time_zones_unread,
            not_owned: built.summary.not_owned.map(|dropped| (dropped.guarded, dropped.elsewhere)),
            world: built.summary.world,
        }),
        Err(BuildError::GameRunning) => Said::Failed(Failure::GameRunning),
        Err(error) => Said::Failed(Failure::Error { error: error.to_string() }),
    }
}

/// A stage by its name, as [`Said::Progress`] has it.
pub fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Reading => "reading",
        Stage::Drawing => "drawing",
        Stage::Tiles => "tiles",
        Stage::Done => "done",
    }
}

/// [`Said::Progress`] as the export's own progress (`None`: a stage it doesn't know).
pub fn progress_of(stage: &str, done: f32, size: Option<Size>) -> Option<Progress> {
    let stage = match stage {
        "reading" => Stage::Reading,
        "drawing" => Stage::Drawing,
        "tiles" => Stage::Tiles,
        "done" => Stage::Done,
        _ => return None,
    };
    let size = size.map(|size| MapSize {
        items: size.items,
        extent: size.extent,
        sea_cells: size.sea_cells,
        sea_cell: size.sea_cell,
    });
    Some(Progress { stage, done, size })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Kind;

    #[test]
    fn a_request_and_what_is_said_go_through_json_as_they_are() {
        let request = Request {
            game: Game {
                kind: Kind::Ets2,
                dir: PathBuf::from("C:/Games/ETS2"),
                found_by: crate::game::FoundBy::Steam,
                version: Some(crate::game::Version([1, 61, 1, 9])),
                dlcs: vec!["dlc_balkan_e".into()],
            },
            profile: Profile::new_player(Kind::Ets2),
            language: Some("tr_tr".into()),
            maps: PathBuf::from("C:/maps"),
            fast: true,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(!json.contains('\n'), "one line");
        assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), request);
        for said in [
            Said::Progress {
                stage: "drawing".into(),
                done: 0.4,
                size: Some(Size { items: 3, extent: [0.0, 1.0, 2.0, 3.0], sea_cells: 9, sea_cell: 20.0 }),
                memory: Some(1 << 30),
            },
            Said::Built(Outcome { not_owned: Some((1, 2)), world: Some((0.9, true)), ..Outcome::default() }),
            Said::Failed(Failure::GameRunning),
            Said::Failed(Failure::Error { error: "a mod".into() }),
        ] {
            let json = serde_json::to_string(&said).unwrap();
            assert_eq!(serde_json::from_str::<Said>(&json).unwrap(), said, "{json}");
        }
    }

    #[test]
    fn each_stage_by_its_name_and_back() {
        for stage in [Stage::Reading, Stage::Drawing, Stage::Tiles, Stage::Done] {
            assert_eq!(progress_of(stage_name(stage), 0.5, None).map(|progress| progress.stage), Some(stage));
        }
        assert_eq!(progress_of("cooking", 0.5, None), None);
    }
}
