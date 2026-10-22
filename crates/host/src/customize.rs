//! The player's own wallpapers and icon sets (the companion's Customize), kept in WestPlay's `customize`
//! folder beside its maps:
//!
//! - `wallpapers/`: any image, a wallpaper of its own for both Light and Dark mode, or with its dark look beside
//!   it (`silk.jpg` and `silk.dark.jpg`, as icons have theirs) one that changes with them.
//!   Picking either file brings both; a folder brings all of its wallpapers.
//! - `icons/<set>/`: an icon set: a file for each app, named by its role (`maps.png`, `youtube-music.webp`,
//!   [`ICON_ROLES`]), or two that follow Light and Dark (`maps.light.png`, `maps.dark.png`). A role it leaves out
//!   keeps WestPlay's own icon.
//!
//! Served to the cabin page at `https://westplay.customize/` (the host) and to the companion at
//! `westplay-customize://` (the app). What's added is checked first: images only (each one decoded, an SVG
//! looked at), each within a size, icons only by their exact names. Nothing is guessed: what's left out is
//! reported, by name, with why.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// The folder's name, in WestPlay's data folder.
pub const FOLDER: &str = "customize";
const WALLPAPERS: &str = "wallpapers";
const ICONS: &str = "icons";

/// An icon's file name (without its extension), and the role the cabin page knows it by.
pub const ICON_ROLES: [(&str, &str); 10] = [
    ("maps", "maps"),
    ("music", "music"),
    ("spotify", "spotify"),
    ("podcasts", "podcasts"),
    ("messages", "messages"),
    ("phone", "phone"),
    ("settings", "settings"),
    ("weather", "weather"),
    ("youtube", "youtube"),
    ("youtube-music", "youtubeMusic"),
];

/// The kinds of image a wallpaper can be.
const WALLPAPER_TYPES: [&str; 7] = ["jpg", "jpeg", "png", "webp", "gif", "bmp", "svg"];
/// The kinds of image an icon can be.
const ICON_TYPES: [&str; 5] = ["png", "webp", "jpg", "jpeg", "svg"];
/// The largest wallpaper taken: a photo straight from a camera fits.
const WALLPAPER_MAX: u64 = 50 * 1024 * 1024;
/// The largest icon taken.
const ICON_MAX: u64 = 5 * 1024 * 1024;
/// The most files a .zip is looked through for an icon set.
const ZIP_ENTRIES_MAX: usize = 500;

/// Everything the player added, as the companion and the cabin page show it. Paths are relative to the
/// `customize` folder, with `/`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Customizations {
    pub wallpapers: Vec<Wallpaper>,
    pub icon_sets: Vec<IconSet>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Wallpaper {
    /// `custom:` and its file's name (a pair's: the name before `.dark`/`.light`).
    pub id: String,
    pub name: String,
    pub dark: Option<String>,
    pub light: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IconSet {
    pub name: String,
    /// Each icon it has, by the cabin page's role.
    pub icons: BTreeMap<&'static str, Icon>,
}

/// An icon, for a light screen and a dark one (the same file for both when it's one icon).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Icon {
    pub light: Option<String>,
    pub dark: Option<String>,
}

/// Which look an icon's file is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Look {
    Both,
    Light,
    Dark,
}

/// A file that wasn't added, and why: `notAnImage`, `tooLarge`, `unknownName` (an icon not named by a role),
/// `twice` (a second icon for the same role), `unreadable`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Problem {
    pub file: String,
    pub reason: &'static str,
}

/// What adding did.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Added {
    /// The wallpapers' or icon set's names, as they were kept.
    pub added: Vec<String>,
    pub problems: Vec<Problem>,
}

/// Lists what's in the `customize` folder `dir` (none of it if it isn't there).
pub fn list(dir: &Path) -> Customizations {
    Customizations { wallpapers: wallpapers(&dir.join(WALLPAPERS)), icon_sets: icon_sets(&dir.join(ICONS)) }
}

/// A wallpaper file's name: its wallpaper's name (`silk` of `silk.dark.jpg`) and which look the file is for:
/// `silk.jpg` both (or, beside a dark one, the light one), `silk.dark.jpg` the dark one (`silk.light.jpg` the
/// light one, as icons take it too).
fn wallpaper_of(stem: &str) -> (&str, Look) {
    match variant(stem) {
        Some((base, true)) => (base, Look::Dark),
        Some((base, false)) => (base, Look::Light),
        None => (stem, Look::Both),
    }
}

/// A wallpaper's files, each look's (both, light, dark), as (file, its name).
type Looks = [Option<(PathBuf, String)>; 3];

fn wallpapers(dir: &Path) -> Vec<Wallpaper> {
    let mut found: BTreeMap<String, (String, Looks)> = BTreeMap::new();
    for (file, name) in files(dir) {
        let Some((stem, extension)) = split_name(&name) else { continue };
        if !WALLPAPER_TYPES.contains(&extension.as_str()) {
            continue;
        }
        let (base, look) = wallpaper_of(stem);
        let slot = match look {
            Look::Both => 0,
            Look::Light => 1,
            Look::Dark => 2,
        };
        // Two pictures of one name for the same look (`silk.jpg`, `silk.png`): the second its own wallpaper.
        let mut key = base.to_lowercase();
        if found.get(&key).is_some_and(|(_, looks)| looks[slot].is_some()) {
            key = name.to_lowercase();
        }
        let entry = found.entry(key).or_insert_with(|| (base.to_owned(), Default::default()));
        entry.1[slot] = Some((file, name));
    }
    let mut all: Vec<Wallpaper> = found
        .into_values()
        .filter_map(|(base, [both, light, dark])| {
            let path = |file: &(PathBuf, String)| format!("{WALLPAPERS}/{}", file.1);
            if light.is_none() && dark.is_none() {
                // One picture: the same in Light and Dark mode (the screen's look follows the player's choice).
                let (_, name) = both?;
                let path = format!("{WALLPAPERS}/{name}");
                return Some(Wallpaper {
                    id: format!("custom:{name}"),
                    name: base,
                    dark: Some(path.clone()),
                    light: Some(path),
                });
            }
            Some(Wallpaper {
                id: format!("custom:{base}"),
                light: light.as_ref().or(both.as_ref()).map(path),
                dark: dark.as_ref().or(both.as_ref()).map(path),
                name: base,
            })
        })
        .collect();
    all.sort_by_key(|wallpaper| wallpaper.name.to_lowercase());
    all
}

fn icon_sets(dir: &Path) -> Vec<IconSet> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut sets: Vec<IconSet> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            let mut found: Vec<(&'static str, Look, String)> = files(&entry.path())
                .into_iter()
                .filter_map(|(_, file)| {
                    let (stem, extension) = split_name(&file)?;
                    let (role, look) = icon_of(stem)?;
                    ICON_TYPES.contains(&extension.as_str()).then(|| (role, look, format!("{ICONS}/{name}/{file}")))
                })
                .collect();
            // One icon for both first, so a light or dark one of the same app takes its side over it.
            found.sort_by_key(|(_, look, path)| (*look != Look::Both, path.to_lowercase()));
            let mut icons: BTreeMap<&'static str, Icon> = BTreeMap::new();
            for (role, look, path) in found {
                let icon = icons.entry(role).or_default();
                if look != Look::Dark {
                    icon.light = Some(path.clone());
                }
                if look != Look::Light {
                    icon.dark = Some(path);
                }
            }
            (!icons.is_empty()).then_some(IconSet { name, icons })
        })
        .collect();
    sets.sort_by_key(|set| set.name.to_lowercase());
    sets
}

/// Adds wallpapers: each image picked, with its other look beside it (`silk.jpg` and `silk.dark.jpg`: picking
/// either brings both), copied into the folder under its own name (a number added if it's taken).
pub fn add_wallpapers(dir: &Path, picked: &[PathBuf]) -> std::io::Result<Added> {
    let folder = dir.join(WALLPAPERS);
    std::fs::create_dir_all(&folder)?;
    let mut report = Added::default();
    // Each wallpaper once, by its source folder and name, with all its files.
    let mut wallpapers: BTreeMap<(PathBuf, String), Vec<PathBuf>> = BTreeMap::new();
    for file in picked {
        let name = shown_name(file);
        let Some((stem, extension)) = split_name(&name) else {
            report.problems.push(Problem { file: name, reason: "notAnImage" });
            continue;
        };
        if !WALLPAPER_TYPES.contains(&extension.as_str()) {
            report.problems.push(Problem { file: name, reason: "notAnImage" });
            continue;
        }
        let base = wallpaper_of(stem).0.to_lowercase();
        let source = file.parent().map(Path::to_path_buf).unwrap_or_default();
        if wallpapers.contains_key(&(source.clone(), base.clone())) {
            continue;
        }
        let mut members = vec![file.clone()];
        for (sibling, sibling_name) in files(&source) {
            let Some((sibling_stem, sibling_extension)) = split_name(&sibling_name) else { continue };
            let same = sibling != *file && wallpaper_of(sibling_stem).0.eq_ignore_ascii_case(&base);
            if same && WALLPAPER_TYPES.contains(&sibling_extension.as_str()) {
                members.push(sibling);
            }
        }
        wallpapers.insert((source, base), members);
    }
    for members in wallpapers.into_values() {
        // Its name as the player wrote it (the first file's).
        let first = members.first().map(|file| shown_name(file)).unwrap_or_default();
        let base = split_name(&first).map(|(stem, _)| wallpaper_of(stem).0.to_owned()).unwrap_or_default();
        let mut checked: Vec<(Look, String, Vec<u8>)> = Vec::new();
        for member in members {
            let name = shown_name(&member);
            let Some((stem, extension)) = split_name(&name) else { continue };
            let look = wallpaper_of(stem).1;
            if checked.iter().any(|(taken, ..)| *taken == look) {
                report.problems.push(Problem { file: name, reason: "twice" });
                continue;
            }
            let read = std::fs::read(&member).map_err(|_| "unreadable");
            match read.and_then(|bytes| check_image(&name, bytes, &WALLPAPER_TYPES, WALLPAPER_MAX)) {
                Ok(bytes) => checked.push((look, extension, bytes)),
                Err(reason) => report.problems.push(Problem { file: name, reason }),
            }
        }
        if checked.is_empty() {
            continue;
        }
        // One name for the whole wallpaper, so its looks stay together.
        let name = free_wallpaper_name(&folder, &plain_name(&base));
        for (look, extension, bytes) in &checked {
            let suffix = match look {
                Look::Both => "",
                Look::Light => ".light",
                Look::Dark => ".dark",
            };
            write_new(&folder.join(format!("{name}{suffix}.{extension}")), bytes)?;
        }
        report.added.push(name);
    }
    Ok(report)
}

/// Adds every wallpaper in a folder (its pictures, each with its other look).
pub fn add_wallpapers_from_folder(dir: &Path, source: &Path) -> std::io::Result<Added> {
    let pictures: Vec<PathBuf> = files(source)
        .into_iter()
        .filter(|(_, name)| {
            split_name(name).is_some_and(|(_, extension)| WALLPAPER_TYPES.contains(&extension.as_str()))
        })
        .map(|(path, _)| path)
        .collect();
    add_wallpapers(dir, &pictures)
}

/// Adds an icon set from a folder of icons named by role, called by the folder's name.
pub fn add_icon_set_from_folder(dir: &Path, source: &Path) -> std::io::Result<Added> {
    let candidates =
        files(source).into_iter().map(|(path, name)| (name, std::fs::read(path).map_err(|_| "unreadable")));
    add_icon_set(dir, &shown_name(source), candidates)
}

/// Adds an icon set from a .zip of icons named by role (at its top, or in a folder in it), called by the
/// zip's name. Only the files' own names count: where they sit in the zip never decides where they go.
pub fn add_icon_set_from_zip(dir: &Path, source: &Path) -> std::io::Result<Added> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(source)?).map_err(std::io::Error::other)?;
    let mut candidates = Vec::new();
    for index in 0..zip.len().min(ZIP_ENTRIES_MAX) {
        let Ok(mut entry) = zip.by_index(index) else { continue };
        let path = entry.name().replace('\\', "/");
        let name = path.rsplit('/').next().unwrap_or_default().to_owned();
        if entry.is_dir() || name.is_empty() || name.starts_with('.') || path.starts_with("__MACOSX/") {
            continue;
        }
        let mut bytes = Vec::new();
        // Read up to just past the limit: a file claiming to be small but unpacking larger is still caught.
        let read = (&mut entry).take(ICON_MAX + 1).read_to_end(&mut bytes).map(|_| bytes).map_err(|_| "unreadable");
        candidates.push((name, read));
    }
    let stem = source.file_stem().and_then(|stem| stem.to_str()).unwrap_or("Icons");
    add_icon_set(dir, stem, candidates.into_iter())
}

fn add_icon_set(
    dir: &Path,
    name: &str,
    candidates: impl Iterator<Item = (String, Result<Vec<u8>, &'static str>)>,
) -> std::io::Result<Added> {
    let mut report = Added::default();
    let mut icons: Vec<(&str, Look, String, Vec<u8>)> = Vec::new();
    let mut candidates: Vec<_> = candidates.collect();
    candidates.sort_by_key(|candidate| candidate.0.to_lowercase());
    for (file, read) in candidates {
        let Some((role, look)) = split_name(&file).and_then(|(stem, _)| icon_of(stem)) else {
            report.problems.push(Problem { file, reason: "unknownName" });
            continue;
        };
        if icons.iter().any(|(taken, taken_look, ..)| *taken == role && *taken_look == look) {
            report.problems.push(Problem { file, reason: "twice" });
            continue;
        }
        match read.and_then(|bytes| check_image(&file, bytes, &ICON_TYPES, ICON_MAX)) {
            Ok(bytes) => {
                let extension = split_name(&file).map(|(_, extension)| extension).unwrap_or_default();
                let suffix = match look {
                    Look::Both => "",
                    Look::Light => ".light",
                    Look::Dark => ".dark",
                };
                icons.push((role, look, format!("{}{suffix}.{extension}", file_stem_of(role)), bytes));
            }
            Err(reason) => report.problems.push(Problem { file, reason }),
        }
    }
    if icons.is_empty() {
        return Ok(report);
    }
    let sets = dir.join(ICONS);
    std::fs::create_dir_all(&sets)?;
    let kept = free_name(&sets, &plain_name(name));
    let folder = sets.join(&kept);
    std::fs::create_dir(&folder)?;
    for (_, _, file, bytes) in icons {
        write_new(&folder.join(file), &bytes)?;
    }
    report.added.push(kept);
    Ok(report)
}

/// Removes a wallpaper (both of a pair's images). `false`: there's no such wallpaper.
pub fn remove_wallpaper(dir: &Path, id: &str) -> std::io::Result<bool> {
    let Some(wallpaper) = wallpapers(&dir.join(WALLPAPERS)).into_iter().find(|wallpaper| wallpaper.id == id) else {
        return Ok(false);
    };
    let mut paths: Vec<String> = [wallpaper.dark, wallpaper.light].into_iter().flatten().collect();
    // One picture for both looks: one file.
    paths.dedup();
    for path in paths {
        std::fs::remove_file(dir.join(path))?;
    }
    Ok(true)
}

/// Removes an icon set. `false`: there's no such set.
pub fn remove_icon_set(dir: &Path, name: &str) -> std::io::Result<bool> {
    if !icon_sets(&dir.join(ICONS)).iter().any(|set| set.name == name) {
        return Ok(false);
    }
    std::fs::remove_dir_all(dir.join(ICONS).join(name))?;
    Ok(true)
}

/// The files directly in `dir` (not its folders), with their names.
fn files(dir: &Path) -> Vec<(PathBuf, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| Some((entry.path(), entry.file_name().to_str()?.to_owned())))
        .collect()
}

/// A file's name before its last dot, and its extension in lower case.
fn split_name(name: &str) -> Option<(&str, String)> {
    let (stem, extension) = name.rsplit_once('.')?;
    (!stem.is_empty()).then(|| (stem, extension.to_lowercase()))
}

/// `name.dark` or `name.light`: the pair's name, and whether it's the dark one.
fn variant(stem: &str) -> Option<(&str, bool)> {
    let (base, which) = stem.rsplit_once('.')?;
    if base.is_empty() {
        return None;
    }
    match which.to_lowercase().as_str() {
        "dark" => Some((base, true)),
        "light" => Some((base, false)),
        _ => None,
    }
}

/// The role and look of an icon file named `stem`: `maps` (both looks), `maps.light`, `maps.dark` (any case).
fn icon_of(stem: &str) -> Option<(&'static str, Look)> {
    if let Some(role) = icon_role(stem) {
        return Some((role, Look::Both));
    }
    let (base, dark) = variant(stem)?;
    Some((icon_role(base)?, if dark { Look::Dark } else { Look::Light }))
}

/// The cabin page's role for an icon named `stem`, exactly (any case).
fn icon_role(stem: &str) -> Option<&'static str> {
    ICON_ROLES.iter().find(|(file, _)| file.eq_ignore_ascii_case(stem)).map(|(_, role)| *role)
}

fn file_stem_of(role: &str) -> &'static str {
    ICON_ROLES.iter().find(|(_, known)| *known == role).map(|(file, _)| *file).unwrap_or("icon")
}

/// Checks that `bytes` are an image of a kind in `types`, within `max`: decoded, or for an SVG, an `<svg>`.
fn check_image(name: &str, bytes: Vec<u8>, types: &[&str], max: u64) -> Result<Vec<u8>, &'static str> {
    let extension = split_name(name).map(|(_, extension)| extension).ok_or("notAnImage")?;
    if !types.contains(&extension.as_str()) {
        return Err("notAnImage");
    }
    if bytes.len() as u64 > max {
        return Err("tooLarge");
    }
    let image = if extension == "svg" {
        std::str::from_utf8(&bytes).is_ok_and(|text| text.contains("<svg"))
    } else {
        image::load_from_memory(&bytes).is_ok()
    };
    if image { Ok(bytes) } else { Err("notAnImage") }
}

/// A file's name as the player knows it.
fn shown_name(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A name that's safe as one file or folder name: no separators or characters Windows refuses.
fn plain_name(name: &str) -> String {
    let plain: String =
        name.chars().map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
    let plain = plain.trim().trim_end_matches('.').trim();
    if plain.is_empty() { "Icons".into() } else { plain.into() }
}

/// `name` in `folder`, or with a number added before its extension (and before `.dark`/`.light`) if it's
/// taken: `Dusk (2).dark.jpg`.
fn free_name(folder: &Path, name: &str) -> String {
    let name = plain_name(name);
    if !folder.join(&name).exists() {
        return name;
    }
    let (base, rest) = match name.split_once('.') {
        Some((base, rest)) if !base.is_empty() => (base.to_owned(), format!(".{rest}")),
        _ => (name.clone(), String::new()),
    };
    (2..).map(|n| format!("{base} ({n}){rest}")).find(|candidate| !folder.join(candidate).exists()).unwrap_or(name)
}

/// A wallpaper's name in `folder`, or with a number added if a wallpaper there has it (`Silk (2)`): any of its
/// files (`Silk.jpg`, `silk.dark.png`) takes the name.
fn free_wallpaper_name(folder: &Path, name: &str) -> String {
    let taken: Vec<String> = files(folder)
        .iter()
        .filter_map(|(_, file)| split_name(file).map(|(stem, _)| wallpaper_of(stem).0.to_lowercase()))
        .collect();
    std::iter::once(name.to_owned())
        .chain((2..).map(|n| format!("{name} ({n})")))
        .find(|candidate| !taken.contains(&candidate.to_lowercase()))
        .unwrap_or_else(|| name.to_owned())
}

/// Writes a new file: beside its place first, then moved there, so nothing reads half of it.
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let partial = path.with_extension("partial");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("westplay-customize-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A PNG of one grey level.
    fn png(level: u8) -> Vec<u8> {
        let image = image::GrayImage::from_pixel(8, 8, image::Luma([level]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn wallpapers_pair_by_name_and_single_ones_serve_both_looks() {
        let source = folder("wallpapers-source");
        for (name, level) in [("Dusk.png", 230), ("Dusk.dark.png", 20), ("Night.png", 15), ("Snow.png", 240)] {
            std::fs::write(source.join(name), png(level)).unwrap();
        }
        std::fs::write(source.join("notes.txt"), "hello").unwrap();
        std::fs::write(source.join("fake.jpg"), "not a jpeg").unwrap();
        let dir = folder("wallpapers");

        // Picking only Dusk.png brings its dark look, Dusk.dark.png, along.
        let picked: Vec<PathBuf> = ["Dusk.png", "Night.png", "Snow.png", "notes.txt", "fake.jpg"]
            .iter()
            .map(|name| source.join(name))
            .collect();
        let report = add_wallpapers(&dir, &picked).unwrap();
        assert_eq!(report.added, vec!["Dusk".to_owned(), "Night".to_owned(), "Snow".to_owned()]);
        assert_eq!(
            report.problems,
            vec![
                Problem { file: "notes.txt".into(), reason: "notAnImage" },
                Problem { file: "fake.jpg".into(), reason: "notAnImage" },
            ]
        );

        let listed = list(&dir).wallpapers;
        let by_name = |name: &str| listed.iter().find(|w| w.name == name).unwrap().clone();
        let dusk = by_name("Dusk");
        assert_eq!(dusk.id, "custom:Dusk");
        assert_eq!(dusk.light.as_deref(), Some("wallpapers/Dusk.png"));
        assert_eq!(dusk.dark.as_deref(), Some("wallpapers/Dusk.dark.png"));
        // A dark picture and a light one alike: the same file in both looks.
        for (name, file) in [("Night", "wallpapers/Night.png"), ("Snow", "wallpapers/Snow.png")] {
            assert_eq!(by_name(name).dark.as_deref(), Some(file));
            assert_eq!(by_name(name).light.as_deref(), Some(file));
        }
        assert!(remove_wallpaper(&dir, "custom:Night.png").unwrap());
        assert!(!dir.join("wallpapers/Night.png").exists());

        // Picking its dark look again brings both, under a name of their own; removing takes both files.
        let again = add_wallpapers(&dir, &[source.join("Dusk.dark.png")]).unwrap();
        assert_eq!(again.added, vec!["Dusk (2)".to_owned()]);
        assert!(dir.join("wallpapers/Dusk (2).png").exists() && dir.join("wallpapers/Dusk (2).dark.png").exists());
        assert!(remove_wallpaper(&dir, "custom:Dusk").unwrap());
        assert!(!dir.join("wallpapers/Dusk.png").exists() && !dir.join("wallpapers/Dusk.dark.png").exists());
        assert!(!remove_wallpaper(&dir, "custom:Dusk").unwrap());
        let _ = std::fs::remove_dir_all(&source);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_gives_all_its_wallpapers_each_with_its_looks() {
        let source = folder("wallpaper-folder").join("Silk");
        std::fs::create_dir_all(&source).unwrap();
        for (name, level) in [("silk.jpg", 220), ("silk.dark.jpg", 25), ("Waves.png", 200), ("old.light.png", 240)] {
            let bytes = if name.ends_with(".jpg") {
                let mut out = std::io::Cursor::new(Vec::new());
                image::GrayImage::from_pixel(8, 8, image::Luma([level]))
                    .write_to(&mut out, image::ImageFormat::Jpeg)
                    .unwrap();
                out.into_inner()
            } else {
                png(level)
            };
            std::fs::write(source.join(name), bytes).unwrap();
        }
        std::fs::write(source.join("Thumbs.db"), "x").unwrap();
        let dir = folder("wallpaper-folder-out");

        let report = add_wallpapers_from_folder(&dir, &source).unwrap();
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        let listed = list(&dir).wallpapers;
        let names: Vec<_> = listed.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, vec!["old", "silk", "Waves"]);
        let silk = &listed[1];
        assert_eq!(
            (silk.light.as_deref(), silk.dark.as_deref()),
            (Some("wallpapers/silk.jpg"), Some("wallpapers/silk.dark.jpg"))
        );
        // An older light-only name still works.
        assert_eq!((listed[0].light.as_deref(), listed[0].dark.as_deref()), (Some("wallpapers/old.light.png"), None));
        let _ = std::fs::remove_dir_all(source.parent().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn icon_sets_take_only_icons_named_by_role() {
        let source = folder("icons-source").join("Retro Icons");
        std::fs::create_dir_all(&source).unwrap();
        for name in ["Maps.png", "youtube-music.png", "Spotify Logo.png", "youtubemusic.png", "music.gif"] {
            std::fs::write(source.join(name), png(100)).unwrap();
        }
        let dir = folder("icons");

        let report = add_icon_set_from_folder(&dir, &source).unwrap();
        assert_eq!(report.added, vec!["Retro Icons".to_owned()]);
        let reasons: Vec<_> = report.problems.iter().map(|p| (p.file.as_str(), p.reason)).collect();
        assert_eq!(
            reasons,
            vec![("music.gif", "notAnImage"), ("Spotify Logo.png", "unknownName"), ("youtubemusic.png", "unknownName")]
        );
        let sets = list(&dir).icon_sets;
        assert_eq!(sets.len(), 1);
        let both = |path: &str| Icon { light: Some(path.into()), dark: Some(path.into()) };
        let icons: Vec<_> = sets[0].icons.iter().map(|(role, icon)| (*role, icon.clone())).collect();
        assert_eq!(
            icons,
            vec![
                ("maps", both("icons/Retro Icons/maps.png")),
                ("youtubeMusic", both("icons/Retro Icons/youtube-music.png"))
            ]
        );

        assert!(remove_icon_set(&dir, "Retro Icons").unwrap());
        assert!(list(&dir).icon_sets.is_empty());
        assert!(!remove_icon_set(&dir, "../outside").unwrap());
        let _ = std::fs::remove_dir_all(source.parent().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn icons_follow_light_and_dark_by_their_names() {
        let source = folder("looks-source").join("Night");
        std::fs::create_dir_all(&source).unwrap();
        for name in
            ["phone.png", "phone.DARK.png", "music.light.png", "music.dark.png", "maps.dark.png", "maps.dark.webp"]
        {
            std::fs::write(source.join(name), png(80)).unwrap();
        }
        let dir = folder("looks");

        let report = add_icon_set_from_folder(&dir, &source).unwrap();
        let reasons: Vec<_> = report.problems.iter().map(|p| (p.file.as_str(), p.reason)).collect();
        assert_eq!(reasons, vec![("maps.dark.webp", "twice")]);
        let icons = &list(&dir).icon_sets[0].icons;
        let icon = |role: &str| icons.get(role).cloned().unwrap_or_default();
        let path = |file: &str| Some(format!("icons/Night/{file}"));
        // One for both, and a dark one taking its side; a pair; a dark one alone.
        assert_eq!(icon("phone"), Icon { light: path("phone.png"), dark: path("phone.dark.png") });
        assert_eq!(icon("music"), Icon { light: path("music.light.png"), dark: path("music.dark.png") });
        assert_eq!(icon("maps"), Icon { light: None, dark: path("maps.dark.png") });
        let _ = std::fs::remove_dir_all(source.parent().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_zip_gives_its_icons_by_their_names_never_by_where_they_sit() {
        let source = folder("zip-source");
        let zip_path = source.join("Neon.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in [
            ("Neon/phone.png", png(50)),
            ("../../escape/weather.png", png(60)),
            ("__MACOSX/Neon/._phone.png", b"junk".to_vec()),
            ("Neon/readme.txt", b"hi".to_vec()),
            ("Neon/other/phone.png", png(70)),
        ] {
            zip.start_file(name, options).unwrap();
            std::io::Write::write_all(&mut zip, &bytes).unwrap();
        }
        zip.finish().unwrap();
        let dir = folder("zip");

        let report = add_icon_set_from_zip(&dir, &zip_path).unwrap();
        assert_eq!(report.added, vec!["Neon".to_owned()]);
        let reasons: Vec<_> = report.problems.iter().map(|p| (p.file.as_str(), p.reason)).collect();
        assert_eq!(reasons, vec![("phone.png", "twice"), ("readme.txt", "unknownName")]);
        let icons: Vec<_> = list(&dir).icon_sets[0].icons.keys().copied().collect();
        assert_eq!(icons, vec!["phone", "weather"]);
        // Nothing outside the set's own folder.
        assert!(!dir.join("escape").exists() && !source.join("escape").exists());
        let _ = std::fs::remove_dir_all(&source);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_icon_set_with_no_icons_adds_nothing() {
        let source = folder("empty-source").join("Nothing");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("logo.png"), png(10)).unwrap();
        let dir = folder("empty");
        let report = add_icon_set_from_folder(&dir, &source).unwrap();
        assert!(report.added.is_empty());
        assert!(list(&dir).icon_sets.is_empty());
        let _ = std::fs::remove_dir_all(source.parent().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_are_kept_plain() {
        assert_eq!(plain_name("My: Set?"), "My_ Set_");
        assert_eq!(plain_name("  .. "), "Icons");
        assert_eq!(plain_name("..\\up"), ".._up");
    }
}
