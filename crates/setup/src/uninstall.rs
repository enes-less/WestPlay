//! Taking WestPlay out of the game, for the companion's uninstall: its screens mod from the game's `mod`
//! folder, and its entry from each profile's active mods (so no profile asks for a mod that's gone). The
//! plugin is `plugin::uninstall`'s; WestPlay's own data is the app's to remove.
//!
//! A profile is changed in its list of active mods only (`save::without_mods`: a binary profile byte for
//! byte but that list), checked before it's written (the profile without WestPlay reads back as the same
//! profile but for its entry), and written beside the old one and moved over it, so a profile is never left
//! half-written. It's written as the game reads it, decrypted; the game encrypts it again when it next saves.

use std::io;
use std::path::{Path, PathBuf};

use westplay_mapdata::save;
use westplay_mapdata::sii::Unit;

use crate::game_mod;
use crate::profiles;

/// Removes WestPlay's screens mod (and a copy left half-written) from the game's `mod` folder in Documents.
pub fn remove_mod(game_documents: &Path) -> io::Result<()> {
    let dir = game_documents.join("mod");
    for file in [dir.join(game_mod::FILE_NAME), dir.join(format!("{}.partial", game_mod::FILE_NAME))] {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// How a profile lists WestPlay's mod: its package is the mod's file without `.scs` (`westplay|WestPlay`).
pub fn is_westplay(entry: &str) -> bool {
    entry.split('|').next() == Some(game_mod::FILE_NAME.trim_end_matches(".scs"))
}

/// What taking WestPlay out of the profiles did.
#[derive(Debug, Default, PartialEq)]
pub struct ProfilesReport {
    /// Profiles that listed it, and no longer do.
    pub changed: usize,
    /// Those it couldn't be taken out of, and why.
    pub failed: Vec<(PathBuf, String)>,
}

/// Takes WestPlay's mod out of every profile's active mods, in the game's Documents folder and Steam Cloud's
/// (`steam_userdata`).
pub fn remove_from_profiles(game_documents: &Path, steam_userdata: Option<&Path>) -> ProfilesReport {
    let mut report = ProfilesReport::default();
    for profile in profiles::find(game_documents, steam_userdata, None) {
        let file = profile.dir.join("profile.sii");
        match remove_from_profile(&file) {
            Ok(true) => report.changed += 1,
            Ok(false) => {}
            Err(error) => report.failed.push((file, error)),
        }
    }
    report
}

/// Takes WestPlay out of one profile file: `true` if it was listed.
pub fn remove_from_profile(file: &Path) -> Result<bool, String> {
    let data = std::fs::read(file).map_err(|e| e.to_string())?;
    let Some(changed) = save::without_mods(&data, is_westplay).map_err(|e| e.to_string())? else {
        return Ok(false);
    };
    check(&data, &changed)?;
    let partial = file.with_extension("sii.westplay-partial");
    std::fs::write(&partial, &changed).map_err(|e| e.to_string())?;
    if let Err(error) = std::fs::rename(&partial, file) {
        let _ = std::fs::remove_file(&partial);
        return Err(error.to_string());
    }
    Ok(true)
}

/// That the profile without WestPlay is the same profile in everything but its active mods, and those are
/// the same but for WestPlay's.
fn check(before: &[u8], after: &[u8]) -> Result<(), String> {
    let before = save::read(before).map_err(|e| e.to_string())?;
    let after = save::read(after).map_err(|e| e.to_string())?;
    let expected: Vec<String> = save::active_mods(&before).into_iter().filter(|m| !is_westplay(m)).collect();
    if save::active_mods(&after) != expected || without_mods(&before) != without_mods(&after) {
        return Err("the profile wouldn't read back the same: left as it was".into());
    }
    Ok(())
}

/// A unit's class, name and attributes, as compared.
type Compared<'a> = (&'a str, &'a str, Vec<&'a (String, String)>);

/// Every unit's class, name and attributes but its active mods (their entries, and a text profile's count).
fn without_mods(units: &[Unit]) -> Vec<Compared<'_>> {
    units
        .iter()
        .map(|unit| {
            let attributes = unit
                .attributes
                .iter()
                .filter(|(key, value)| !(key == "active_mods" && (value.contains('|') || value.parse::<u32>().is_ok())))
                .collect();
            (unit.class.as_str(), unit.name.as_str(), attributes)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("westplay-uninstall-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn knows_westplay_by_its_package() {
        assert!(is_westplay("westplay|WestPlay"));
        assert!(!is_westplay("westplay_extras|Something"));
        assert!(!is_westplay("promods|ProMods"));
    }

    /// A text profile listing WestPlay loses that entry (and nothing else); one that doesn't is left alone,
    /// not even written.
    #[test]
    fn takes_westplay_out_of_a_profile() {
        let dir = folder("profile");
        let file = dir.join("profile.sii");
        std::fs::write(
            &file,
            "SiiNunit\n{\nuser_profile : _nameless.1 {\n profile_name: Western\n active_mods: 2\n \
             active_mods[0]: \"westplay|WestPlay\"\n active_mods[1]: \"promods|ProMods\"\n}\n}\n",
        )
        .unwrap();
        assert_eq!(remove_from_profile(&file), Ok(true));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(" active_mods: 1\n active_mods[0]: \"promods|ProMods\"\n"));
        assert!(text.contains("profile_name: Western"));
        let written = std::fs::metadata(&file).unwrap().modified().unwrap();
        assert_eq!(remove_from_profile(&file), Ok(false));
        assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), written);
        assert!(!dir.join("profile.sii.westplay-partial").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removes_the_mod_and_only_it() {
        let docs = folder("mod");
        let mods = docs.join("mod");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join(game_mod::FILE_NAME), b"mod").unwrap();
        std::fs::write(mods.join("promods.scs"), b"theirs").unwrap();
        remove_mod(&docs).unwrap();
        remove_mod(&docs).unwrap();
        assert!(!mods.join(game_mod::FILE_NAME).exists());
        assert!(mods.join("promods.scs").exists());
        let _ = std::fs::remove_dir_all(&docs);
    }
}
