//! Reads the map from the player's own copy of the game, for WestPlay's Maps app. Nothing read here is
//! shipped with WestPlay: every player's map comes from their own game files (and only the map DLCs and
//! mods they have).
//!
//! The game's files are read straight from its `.scs` archives ([`archive`], mounted by [`files`]).
//!
//! The layouts of the game's archives and binary map files were worked out by the ts-map project
//! (<https://github.com/dariowouters/ts-map>, MIT licence, © Dario Wouters), whose readers this crate's
//! follow; see THIRD_PARTY_NOTICES.md. The terrain, `.aux` and `.data` layouts were learned from
//! TruckLib (<https://github.com/sk-zk/TruckLib>, GPL), as a reference only: none of its code is used.

pub mod archive;
pub mod area;
pub mod buildings;
/// CityHash64, shared with the plugin (which names vehicles by their ids' hashes: `westplay_shared::vehicle_marked`).
pub use westplay_shared::cityhash;
pub mod countries;
pub mod data;
pub mod export;
pub mod files;
pub mod gdeflate;
pub mod geometry;
pub mod junctions;
pub mod lanes;
pub mod locale;
pub mod prefab;
pub mod reader;
pub mod routing;
pub mod save;
pub mod sea;
pub mod sector;
pub mod signs;
pub mod sii;
pub mod terrain;
pub mod texture;
pub mod tiles;
pub mod token;
pub mod world;

#[cfg(test)]
mod test_dir;
