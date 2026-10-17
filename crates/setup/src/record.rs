//! What WestPlay's setup did, kept between runs (`setup.json`, in WestPlay's config folder): the game it set
//! WestPlay up in, the version of WestPlay that put its plugin and mod there, and whether the player went
//! through the companion's onboarding to its end. Nothing about the player: no profile, no mod, no account.

use std::io;
use std::path::{Path, PathBuf};

/// The file, in WestPlay's config folder.
pub const FILE_NAME: &str = "setup.json";

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Record {
    /// The game's folder WestPlay is set up in (its plugin, its mod, its map), once it is.
    pub game: Option<PathBuf>,
    /// The version of WestPlay that put its plugin and mod into that game: another one's (an update since)
    /// are put in again.
    pub version: Option<String>,
    /// The onboarding was gone through to its end: the companion opens on its own pages from then on.
    pub finished: bool,
}

impl Record {
    /// The record in `dir`: none yet (a first run), or one that can't be read, is a fresh one.
    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join(FILE_NAME))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Keeps the record in `dir`, written beside the one there first, then moved over it.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let partial = dir.join(format!("{FILE_NAME}.partial"));
        std::fs::write(&partial, serde_json::to_string_pretty(self).map_err(io::Error::other)?)?;
        std::fs::rename(&partial, dir.join(FILE_NAME))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_what_setup_did() {
        let dir = std::env::temp_dir().join(format!("westplay-record-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Record::load(&dir), Record::default());

        let record = Record {
            game: Some(PathBuf::from(r"D:\SteamLibrary\steamapps\common\ETS2")),
            version: Some("1.0.0".into()),
            finished: true,
        };
        record.save(&dir).unwrap();
        assert_eq!(Record::load(&dir), record);

        // One it can't read starts again.
        std::fs::write(dir.join(FILE_NAME), "{ not json").unwrap();
        assert_eq!(Record::load(&dir), Record::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
