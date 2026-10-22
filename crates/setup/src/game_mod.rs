//! WestPlay's game mod (`westplay.scs`), built on the player's PC from their own game: for each screen layout
//! (`westplay_shared::screen_layouts`), the game's GPS screen scripts with one image added as the top layer of
//! their "screen on" group, the screen texture WestPlay draws into. Nothing of the game's is shipped: the
//! scripts are the player's own copy's, patched, and the rest is WestPlay's.
//!
//! Each layout's texture is SCREEN_WIDTH x (SCREEN_HEIGHT + the layout's index), so the plugin knows from its
//! size which kind of screen the game draws. It starts fully transparent: until the plugin fills it, or with
//! WestPlay off, the game's own screen shows through. A layout whose script the game doesn't have (a DLC the
//! player doesn't own) is left out.
//!
//! It isn't put in the game's `mod` folder or shown in its Mod Manager: it's written into WestPlay's own folder
//! (`places::westplay_documents`), and WestPlay's plugin mounts it into the game itself, first of all mods, in
//! every mode (single player, Convoy, TruckersMP). A copy in the game's `mod` folder is taken out
//! (`uninstall::remove_mod`): with both, the game has the screens' scripts twice and stops when a screen starts.
//!
//! The mod is a zip, as `.scs` mods may be. (Ported from the Python script that built it before, and checked
//! against it: the same patch byte for byte, and the same mod from the same game.)

use std::io::Write;
use std::path::{Path, PathBuf};

use westplay_mapdata::files::GameFiles;
use westplay_shared::{SCREEN_WIDTH, ScreenLayout, screen_layouts};

/// The mod's file, in WestPlay's own folder (`places::westplay_documents`).
pub const FILE_NAME: &str = "westplay.scs";
/// Where the mod's screen materials and textures live in the game's file system.
const SCREEN_DIR: &str = "material/ui/westplay";
/// The screen's draw order in its group: the stock elements use layers 0 to 3.
const SCREEN_LAYER: u32 = 100;
/// The "screen on" group's id in the GPS scripts.
const SCREEN_ON_GROUP: &str = "100";

/// A screen of WestPlay's own making, for interiors the game gives the same screen script as others: the Smart
/// Dash Advanced (12.9") includes the same screens file as the 10.1" (`computers_2025.sui`, naming one GPS
/// script), so the mod couldn't tell them apart, and the larger screen got the smaller one's sizing. The
/// layout's script (its one, in `screen_layouts.txt`) is a copy of `source`, patched as the others are; its
/// interiors' definitions (the files in `interiors` whose name has `marker`), copies of the player's own with
/// their one `@include` of `screens` changed, include `own_screens` beside them: a copy of that screens file
/// whose `gps_path` names the copy. Built whole or not at all: if anything isn't as read (a game update), the
/// layout is left out, and those interiors keep the screen they share.
struct OwnScreen {
    layout: &'static str,
    source: &'static str,
    screens: &'static str,
    own_screens: &'static str,
    interiors: &'static [&'static str],
    marker: &'static str,
}

const OWN_SCREENS: &[OwnScreen] = &[OwnScreen {
    layout: "scania-2025-advanced",
    source: "ui/dashboard/scania_2025_gps.sii",
    screens: "computers_2025.sui",
    own_screens: "westplay_computers_2025_advanced.sui",
    interiors: &["def/vehicle/truck/scania.s_2016/interior", "def/vehicle/truck/scania.r_2016/interior"],
    marker: "smartdash_advanced",
}];

/// The part of a GPS script's screen its display shows, where the display shows only part of it: fractions of
/// the "screen on" group's width from the left and of its height from the top. WestPlay's image goes on that
/// part alone, so none of it is cut off and it isn't drawn out of shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShownPart {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

/// The parts shown, by layout. The stock GPS screen (`ui/gps.sii`, a 256 x 256 square) is WestPlay's only in
/// the Scania S and R's 7" display, which shows the square's full width (0.5%-99.5%) and rows 12.2%-73.5%
/// from its top: 254 x 157 of it, 1.615:1, as the interior's model maps its screen piece
/// (`scania_2016/interior/model.pmg`, piece 80, `gps.tobj`: PLAN.md section C). Stretched over the whole
/// square, the frame lost its bottom and top and was drawn 1.3 times too tall (owner 2026-10-21).
const SHOWN_PARTS: &[(&str, ShownPart)] =
    &[("generic", ShownPart { left: 0.005, right: 0.995, top: 0.122, bottom: 0.735 })];

/// The 7" Scania's screen drawn sharp. The interiors naming no GPS screen of their own (all but the Smart
/// Dash's) get the stock one at the game's default size: a 256 x 256 target (the 7" model's `gps.tobj` piece
/// was drawn with one: PLAN.md section C), into which WestPlay's 1280 x 720 frame was shrunk five times over,
/// and of which only 254 x 157 pixels show (owner 2026-10-21: "wtf is this resolution"). At 1024 x 1024
/// (square still, so the display shows the same part of it), about 1016 x 628 show, near the frame's own size.
/// Set as the Smart Dash's screens file sets its own (`gps_drawable_size`).
const SHARP_INTERIORS: &[&str] =
    &["def/vehicle/truck/scania.s_2016/interior", "def/vehicle/truck/scania.r_2016/interior"];
/// The interiors with a screen of their own, left as they are.
const SHARP_SKIP: &str = "smartdash";
const SHARP_SIZE: &str = "gps_drawable_size: (1024, 1024)";
/// The stock GPS screen's script.
const STOCK_GPS: &str = "ui/gps.sii";

/// The 7" interiors' definitions, each the player's own with [`SHARP_SIZE`] added after its `dashboard_path`.
/// Whole or not at all: an interior not as read (naming a GPS screen or its size already: a game update)
/// leaves them all at the game's size.
fn sharp_screen_files(game: &GameFiles) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut files = Vec::new();
    for dir in SHARP_INTERIORS {
        let names = game.list(dir).map_err(|error| error.to_string())?;
        for name in names.iter().filter(|name| name.ends_with(".sii") && !name.contains(SHARP_SKIP)) {
            let path = format!("{dir}/{name}");
            let bytes = game.read(&path).map_err(|error| error.to_string())?.ok_or(format!("no {path}"))?;
            let text = String::from_utf8(bytes).map_err(|_| format!("{path}: not UTF-8 text"))?;
            // The stock screen, named or not (the exclusive interiors, brown, wood and V8, name it).
            let other_screen = text.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("gps_path") && !line.contains(&format!("\"/{STOCK_GPS}\""))
            });
            if other_screen || text.contains("gps_drawable_size") {
                return Err(format!("{path}: names a GPS screen of its own, or its size"));
            }
            let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
            let mut lines: Vec<String> = text.split(newline).map(str::to_owned).collect();
            let found: Vec<usize> =
                (0..lines.len()).filter(|&at| lines[at].trim_start().starts_with("dashboard_path")).collect();
            let [at] = found[..] else {
                return Err(format!("{path}: {} dashboard_path lines, not one", found.len()));
            };
            let indent = lines[at][..lines[at].len() - lines[at].trim_start().len()].to_owned();
            lines.insert(at + 1, format!("{indent}{SHARP_SIZE}"));
            files.push((path, lines.join(newline).into_bytes()));
        }
    }
    if files.is_empty() {
        return Err("no 7\" interiors".into());
    }
    Ok(files)
}

/// An own screen's files (`own`, its layout's `script`, its texture `texture`), from `game`: the script's
/// copy, and per interiors' folder the screens file's copy and the interiors' definitions.
fn own_screen_files(
    game: &GameFiles,
    own: &OwnScreen,
    script: &str,
    texture: &str,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let read = |path: &str| -> Result<String, String> {
        let bytes = game.read(path).map_err(|error| error.to_string())?.ok_or(format!("no {path}"))?;
        String::from_utf8(bytes).map_err(|_| format!("{path}: not UTF-8 text"))
    };
    let mut files = vec![(script.to_owned(), patch_gps_script(&read(own.source)?, texture, None)?.into_bytes())];
    let replace_once = |text: &str, from: &str, to: &str, path: &str| -> Result<String, String> {
        match text.matches(from).count() {
            1 => Ok(text.replacen(from, to, 1)),
            count => Err(format!("{path}: {count} times {from}, not once")),
        }
    };
    let mut interiors = 0;
    for dir in own.interiors {
        let screens_path = format!("{dir}/{}", own.screens);
        let screens = replace_once(
            &read(&screens_path)?,
            &format!("\"/{}\"", own.source),
            &format!("\"/{script}\""),
            &screens_path,
        )?;
        files.push((format!("{dir}/{}", own.own_screens), screens.into_bytes()));
        let names = game.list(dir).map_err(|error| error.to_string())?;
        for name in names.iter().filter(|name| name.contains(own.marker) && name.ends_with(".sii")) {
            let path = format!("{dir}/{name}");
            let include = |file: &str| format!("@include \"{file}\"");
            let text = replace_once(&read(&path)?, &include(own.screens), &include(own.own_screens), &path)?;
            files.push((path, text.into_bytes()));
            interiors += 1;
        }
    }
    if interiors == 0 {
        return Err(format!("no interiors named {}", own.marker));
    }
    Ok(files)
}

/// What a build made: the layouts patched, and those whose scripts the game doesn't have; and why the 7"
/// Scania screen stays at the game's size, if it does (its interiors not as read: `sharp_screen_files`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Built {
    pub patched: Vec<&'static str>,
    pub missing: Vec<&'static str>,
    pub not_sharp: Option<String>,
}

/// Why the mod couldn't be built.
#[derive(Debug)]
pub enum BuildError {
    /// A script couldn't be read from the game.
    Read(String),
    /// A script isn't laid out as WestPlay expects (a new game version, or a mod's own).
    Script { script: &'static str, problem: String },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(error) => write!(f, "{error}"),
            Self::Script { script, problem } => write!(f, "{script}: {problem}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// The mod built from `game` (the installed game, mounted), as the bytes of its `.scs` file.
pub fn build(game: &GameFiles) -> Result<(Vec<u8>, Built), BuildError> {
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("manifest.sii".into(), manifest().into_bytes()),
        ("description.txt".into(), b"WestPlay screens. Requires the WestPlay app and plugin.\n".to_vec()),
    ];
    let mut built = Built::default();
    for (index, layout) in screen_layouts().iter().enumerate() {
        let texture = texture_name(layout);
        files.push((format!("{SCREEN_DIR}/{texture}.mat"), material(&texture).into_bytes()));
        files.push((format!("{SCREEN_DIR}/{texture}.tobj"), tobj(&format!("/{SCREEN_DIR}/{texture}.dds"))));
        files.push((
            format!("{SCREEN_DIR}/{texture}.dds"),
            transparent_dds(SCREEN_WIDTH, ScreenLayout::texture_height(index)),
        ));
        // A screen of WestPlay's own making: whole, or missing.
        if let Some(own) = OWN_SCREENS.iter().find(|own| own.layout == layout.id) {
            match layout.gps_scripts {
                [script] => match own_screen_files(game, own, script, &texture) {
                    Ok(own_files) => {
                        files.extend(own_files);
                        built.patched.push(layout.id);
                    }
                    Err(_) => built.missing.push(layout.id),
                },
                _ => built.missing.push(layout.id),
            }
            continue;
        }
        // Each of its scripts the game has (a layout whose scripts it has none of is missing).
        let mut any = false;
        for &script in layout.gps_scripts {
            let original = game.read(script).map_err(|error| BuildError::Read(error.to_string()))?;
            let Some(original) = original else { continue };
            let text = String::from_utf8(original)
                .map_err(|_| BuildError::Script { script, problem: "not UTF-8 text".into() })?;
            let shown = SHOWN_PARTS.iter().find(|(id, _)| *id == layout.id).map(|(_, part)| part);
            let patched =
                patch_gps_script(&text, &texture, shown).map_err(|problem| BuildError::Script { script, problem })?;
            files.push((script.to_owned(), patched.into_bytes()));
            any = true;
        }
        if any {
            built.patched.push(layout.id);
        } else {
            built.missing.push(layout.id);
        }
    }
    // The 7" screen at a size it shows sharp, if its interiors are as read (else the game's size stays, and
    // the build says why).
    match sharp_screen_files(game) {
        Ok(sharp) => files.extend(sharp),
        Err(why) => built.not_sharp = Some(why),
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((zip(&files), built))
}

/// Writes the mod into `mod_dir` (WestPlay's own folder), replacing the one there whole: written beside it
/// first, then moved over it, so the plugin never finds half a file.
/// A step that failed names itself and its path (`writing C:\…\WestPlay\westplay.scs.partial: …`), so a
/// player's report says which.
pub fn install(bytes: &[u8], mod_dir: &Path) -> std::io::Result<PathBuf> {
    let step = |what: &str, path: &Path| {
        let what = format!("{what} {}", path.display());
        move |error: std::io::Error| std::io::Error::new(error.kind(), format!("{what}: {error}"))
    };
    std::fs::create_dir_all(mod_dir).map_err(step("making the folder", mod_dir))?;
    let path = mod_dir.join(FILE_NAME);
    let partial = mod_dir.join(format!("{FILE_NAME}.partial"));
    std::fs::write(&partial, bytes).map_err(step("writing", &partial))?;
    std::fs::rename(&partial, &path).map_err(step("moving it over", &path))?;
    Ok(path)
}

/// A layout's screen texture's name: `screen_daf_2021`.
pub fn texture_name(layout: &ScreenLayout) -> String {
    format!("screen_{}", layout.id.replace('-', "_"))
}

/// The mod's manifest. Optional in convoys (`mp_mod_optional`, as Workshop mods mark it): it changes only the
/// player's own truck screen, so players with and without it join each other's convoys.
fn manifest() -> String {
    format!(
        "SiiNunit\n{{\nmod_package : .package_name\n{{\n\tpackage_version: \"{}\"\n\tdisplay_name: \"WestPlay\"\n\tauthor: \"Western\"\n\tcategory[]: \"ui\"\n\tdescription_file: \"description.txt\"\n\tmp_mod_optional: true\n}}\n}}\n",
        env!("CARGO_PKG_VERSION")
    )
}

fn material(texture: &str) -> String {
    format!(
        "effect : \"ui.rfx\" {{\n\ttexture : \"texture\" {{\n\t\tsource : \"{texture}.tobj\"\n\t\tu_address : clamp\n\t\tv_address : clamp\n\t\tmip_filter : none\n\t}}\n}}\n"
    )
}

/// A texture object pointing at `dds_path`: the header of a stock UI texture object (`/material/ui/white.tobj`),
/// then the path.
fn tobj(dds_path: &str) -> Vec<u8> {
    const HEADER: [u8; 40] = [
        0x01, 0x0a, 0xb1, 0x70, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01, 0, 0, 0, 0x02, 0, 0x01, 0x01,
        0x02, 0, 0x02, 0x02, 0, 0, 0, 0, 0, 0x01, 0, 0,
    ];
    let mut bytes = HEADER.to_vec();
    bytes.extend((dds_path.len() as u32).to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    bytes.extend(dds_path.as_bytes());
    bytes
}

/// A fully transparent `width` x `height` texture: a DDS with a DX10 header, B8G8R8A8_UNORM_SRGB (91), the
/// format the plugin copies WestPlay's frames into.
fn transparent_dds(width: u32, height: u32) -> Vec<u8> {
    let mut header = [0u8; 148];
    header[0..4].copy_from_slice(b"DDS ");
    for (at, value) in [(4, 124u32), (8, 0x1007), (12, height), (16, width), (76, 32), (80, 0x4), (108, 0x1000)] {
        header[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    header[84..88].copy_from_slice(b"DX10");
    for (at, value) in [(128, 91u32), (132, 3), (136, 0), (140, 1), (144, 0)] {
        header[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    let mut bytes = header.to_vec();
    bytes.resize(bytes.len() + (width * height * 4) as usize, 0);
    bytes
}

/// A top-level `ui::kind : name {` block of a UI script: its kind and name, and its lines (header to brace).
struct Block<'a> {
    kind: &'a str,
    name: &'a str,
    start: usize,
    end: usize,
}

fn blocks<'a>(lines: &[&'a str]) -> Vec<Block<'a>> {
    let mut blocks = Vec::new();
    let mut current: Option<(&str, &str, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        if current.is_none() {
            current = block_header(line).map(|(kind, name)| (kind, name, index));
        } else if line.trim() == "}" {
            let (kind, name, start) = current.take().unwrap();
            blocks.push(Block { kind, name, start, end: index });
        }
    }
    blocks
}

/// `ui::group : _nameless.0abc {` → (`ui::group`, `_nameless.0abc`).
fn block_header(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim_end().strip_suffix('{')?.trim_end();
    let (kind, name) = rest.split_once(" : ")?;
    let kind_ok = kind
        .strip_prefix("ui::")
        .is_some_and(|word| !word.is_empty() && word.chars().all(|c| c.is_alphanumeric() || c == '_'));
    (kind_ok && !name.is_empty() && !name.contains(char::is_whitespace)).then_some((kind, name))
}

/// A field's value in a block (`id: 100` → `100`).
fn field<'a>(lines: &[&'a str], block: &Block, name: &str) -> Option<&'a str> {
    lines[block.start + 1..block.end].iter().find_map(|line| {
        let (key, value) = line.trim().split_once(':').unwrap_or((line.trim(), ""));
        (key == name).then(|| value.trim())
    })
}

/// Adds the WestPlay screen image `texture` as the top layer of the GPS script's "screen on" group (id 100):
/// a child of the group, over the whole of it, or over the part of it the display shows (`shown`). The rest
/// of the script stays as it is.
pub fn patch_gps_script(text: &str, texture: &str, shown: Option<&ShownPart>) -> Result<String, String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = text.split(newline).collect();
    let blocks = blocks(&lines);
    let group = blocks
        .iter()
        .find(|block| block.kind == "ui::group" && field(&lines, block, "id") == Some(SCREEN_ON_GROUP))
        .ok_or("no group with id 100")?;
    let count: usize =
        field(&lines, group, "my_children").unwrap_or("0").parse().map_err(|_| "my_children isn't a count")?;
    let coord = |name| field(&lines, group, name).ok_or_else(|| format!("the group has no {name}"));
    let (left, right, top, bottom) = (coord("coords_l")?, coord("coords_r")?, coord("coords_t")?, coord("coords_b")?);
    let (left, right, top, bottom) = match shown {
        None => (left.to_owned(), right.to_owned(), top.to_owned(), bottom.to_owned()),
        Some(part) => {
            let number = |text: &str| text.parse::<f64>().map_err(|_| format!("the group's coordinate {text}"));
            let (l, r, t, b) = (number(left)?, number(right)?, number(top)?, number(bottom)?);
            // Its coordinates go up the screen: the top is the larger.
            let at = |value: f64| format!("{}", value.round() as i64);
            (
                at(l + (r - l) * part.left),
                at(l + (r - l) * part.right),
                at(t - (t - b) * part.top),
                at(t - (t - b) * part.bottom),
            )
        }
    };
    let element = "_nameless.westplay.screen";

    let mut body = Vec::new();
    for line in &lines[group.start + 1..group.end] {
        let trimmed = line.trim();
        let line = if trimmed.starts_with("my_children:") {
            format!(" my_children: {}", count + 1)
        } else {
            (*line).to_owned()
        };
        let after = if count == 0 {
            trimmed.starts_with("my_children:")
        } else {
            trimmed.starts_with(&format!("my_children[{}]:", count - 1))
        };
        body.push(line);
        if after {
            body.push(format!(" my_children[{count}]: {element}"));
        }
    }
    let screen = [
        String::new(),
        format!("ui::text : {element} {{"),
        format!(" text: \"<img src=/{SCREEN_DIR}/{texture}.mat xscale=stretch yscale=stretch>\""),
        format!(" coords_l: {left}"),
        format!(" coords_r: {right}"),
        format!(" coords_t: {top}"),
        format!(" coords_b: {bottom}"),
        " area_l: 1".into(),
        " area_r: 0".into(),
        " area_t: 0".into(),
        " area_b: 1".into(),
        " id: 0".into(),
        format!(" layer: {SCREEN_LAYER}"),
        " tab: -1".into(),
        " pointer: -1".into(),
        format!(" my_parent: {}", group.name),
        "}".into(),
    ];
    let last_end = blocks.last().map(|block| block.end).ok_or("no blocks")?;
    let mut patched: Vec<String> = lines[..=group.start].iter().map(|line| (*line).to_owned()).collect();
    patched.extend(body);
    patched.extend(lines[group.end..=last_end].iter().map(|line| (*line).to_owned()));
    patched.extend(screen);
    patched.extend(lines[last_end + 1..].iter().map(|line| (*line).to_owned()));
    Ok(patched.join(newline))
}

/// A zip of `files` (path, contents), deflated, as the game reads `.scs` mods.
fn zip(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let (mut data, mut directory) = (Vec::new(), Vec::new());
    for (path, contents) in files {
        let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(contents).expect("deflating to memory");
        let packed = encoder.finish().expect("deflating to memory");
        let mut crc = flate2::Crc::new();
        crc.update(contents);
        let local = data.len() as u32;
        // Version 2.0, no flags, deflated, no time.
        let common = |out: &mut Vec<u8>| {
            out.extend(20u16.to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(8u16.to_le_bytes());
            out.extend([0u8; 4]);
            out.extend(crc.sum().to_le_bytes());
            out.extend((packed.len() as u32).to_le_bytes());
            out.extend((contents.len() as u32).to_le_bytes());
            out.extend((path.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
        };
        data.extend(b"PK\x03\x04");
        common(&mut data);
        data.extend(path.as_bytes());
        data.extend(&packed);
        directory.extend(b"PK\x01\x02");
        directory.extend(20u16.to_le_bytes());
        common(&mut directory);
        // No comment, disk 0, no attributes.
        directory.extend([0u8; 10]);
        directory.extend(local.to_le_bytes());
        directory.extend(path.as_bytes());
    }
    let offset = data.len() as u32;
    let size = directory.len() as u32;
    data.extend(directory);
    data.extend(b"PK\x05\x06");
    data.extend([0u8; 4]);
    data.extend((files.len() as u16).to_le_bytes());
    data.extend((files.len() as u16).to_le_bytes());
    data.extend(size.to_le_bytes());
    data.extend(offset.to_le_bytes());
    data.extend(0u16.to_le_bytes());
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_the_mod_whole_and_says_which_step_failed_where() {
        let root = std::env::temp_dir().join(format!("westplay-install-{}", std::process::id()));
        let mod_dir = root.join("Euro Truck Simulator 2").join("mod");
        let path = install(b"mod", &mod_dir).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"mod");
        assert!(!mod_dir.join("westplay.scs.partial").exists());
        // A file where the folder should be: the step and its path in the error.
        let blocked = root.join("blocked");
        std::fs::write(&blocked, b"").unwrap();
        let error = install(b"mod", &blocked.join("mod")).unwrap_err().to_string();
        assert!(error.starts_with("making the folder ") && error.contains("blocked"), "{error}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Optional in convoys: a friend without WestPlay can join, and be joined.
    #[test]
    fn the_mod_is_optional_in_convoys() {
        let units = westplay_mapdata::sii::parse(&manifest());
        let package = units.iter().find(|unit| unit.class == "mod_package").expect("a mod_package");
        assert_eq!(package.get("mp_mod_optional"), Some("true"));
    }

    /// A GPS script laid out as the game's: a window, the "screen on" group with two children, and them.
    const SCRIPT: &str = "SiiNunit\n{\nui::window : _nameless.1 {\n my_children: 1\n my_children[0]: _nameless.2\n}\n\nui::group : _nameless.2 {\n coords_l: 0\n coords_r: 1279\n coords_t: 719\n coords_b: 0\n id: 100\n my_children: 2\n my_children[0]: _nameless.3\n my_children[1]: _nameless.4\n my_parent: _nameless.1\n}\n\nui::text : _nameless.3 {\n my_parent: _nameless.2\n}\n\nui::text : _nameless.4 {\n my_parent: _nameless.2\n}\n\n}\n";

    #[test]
    fn adds_the_screen_on_top_of_the_screen_on_group() {
        let patched = patch_gps_script(SCRIPT, "screen_generic", None).unwrap();
        assert!(patched.contains(" my_children: 3\n my_children[0]: _nameless.3\n my_children[1]: _nameless.4\n my_children[2]: _nameless.westplay.screen\n"));
        assert!(patched.contains("ui::text : _nameless.westplay.screen {\n text: \"<img src=/material/ui/westplay/screen_generic.mat xscale=stretch yscale=stretch>\"\n coords_l: 0\n coords_r: 1279\n coords_t: 719\n coords_b: 0\n"));
        assert!(patched.contains(" layer: 100\n tab: -1\n pointer: -1\n my_parent: _nameless.2\n}\n\n}\n"));
        // The window still has its one child; the rest is as it was.
        assert!(patched.starts_with("SiiNunit\n{\nui::window : _nameless.1 {\n my_children: 1\n"));
    }

    #[test]
    fn puts_the_screen_on_the_part_the_display_shows() {
        // The 7" Scania's: the stock square's full width, rows 12.2%-73.5% from its top.
        let square = SCRIPT.replace("coords_r: 1279", "coords_r: 256").replace("coords_t: 719", "coords_t: 256");
        let shown = SHOWN_PARTS.iter().find(|(id, _)| *id == "generic").map(|(_, part)| part);
        let patched = patch_gps_script(&square, "screen_generic", shown).unwrap();
        let placed = "screen_generic.mat xscale=stretch yscale=stretch>\"\n coords_l: 1\n coords_r: 255\n coords_t: 225\n coords_b: 68\n";
        assert!(patched.contains(placed), "{patched}");
        // The group itself keeps its coordinates.
        assert!(patched.contains(" coords_r: 256\n coords_t: 256\n coords_b: 0\n id: 100\n"));
    }

    #[test]
    fn the_7_inch_interiors_get_a_sharp_screen() {
        let root = smart_dash_game("sharp", "/ui/dashboard/scania_2025_gps.sii");
        for truck in ["s", "r"] {
            let unit = format!("{truck}_high_black_dark");
            let path = root.join(format!("def/vehicle/truck/scania.{truck}_2016/interior/cab_{unit}.sii"));
            let text = format!(
                "SiiNunit\r\n{{\r\naccessory_interior_data: {unit}\r\n{{\r\n\tdashboard_path: \"/ui/dashboard/scania_2016_std.sii\"\r\n\r\n\tglass_panes[]: x\r\n}}\r\n}}\r\n"
            );
            std::fs::write(path, text).unwrap();
            // An exclusive interior names the stock screen itself.
            let exclusive =
                root.join(format!("def/vehicle/truck/scania.{truck}_2016/interior/cab_{truck}_high_v8.sii"));
            let text = "SiiNunit\r\n{\r\naccessory_interior_data: v8\r\n{\r\n\tdashboard_path: \"/ui/dashboard/scania_2016_exc.sii\"\r\n\tgps_path: \"/ui/gps.sii\"\r\n}\r\n}\r\n";
            std::fs::write(exclusive, text).unwrap();
        }
        let files = GameFiles::new(vec![westplay_mapdata::files::Mount::Folder(root.clone())]);
        let (bytes, built) = build(&files).unwrap();
        assert_eq!(built.not_sharp, None);
        let out = root.join("westplay.scs");
        std::fs::write(&out, bytes).unwrap();
        let archive = westplay_mapdata::archive::Archive::open(&out).unwrap();
        let text = |path: &str| archive.read(path).unwrap().map(|bytes| String::from_utf8(bytes).unwrap());
        for truck in ["s", "r"] {
            let dir = format!("def/vehicle/truck/scania.{truck}_2016/interior");
            let interior = text(&format!("{dir}/cab_{truck}_high_black_dark.sii")).unwrap();
            let sized = "\tdashboard_path: \"/ui/dashboard/scania_2016_std.sii\"\r\n\tgps_drawable_size: (1024, 1024)\r\n\r\n\tglass_panes[]: x\r\n";
            assert!(interior.contains(sized), "{interior}");
            let v8 = text(&format!("{dir}/cab_{truck}_high_v8.sii")).unwrap();
            let sized = "exc.sii\"\r\n\tgps_drawable_size: (1024, 1024)\r\n\tgps_path: \"/ui/gps.sii\"\r\n";
            assert!(v8.contains(sized), "{v8}");
            // The Smart Dash's own screens are left as they are.
            assert_eq!(text(&format!("{dir}/cab_{truck}_high_smartdash_dark.sii")), None);
        }
        drop(archive);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn keeps_a_scripts_windows_line_endings() {
        let patched = patch_gps_script(&SCRIPT.replace('\n', "\r\n"), "screen_generic", None).unwrap();
        assert!(patched.contains(" my_children[2]: _nameless.westplay.screen\r\n") && !patched.contains("\r\r"));
    }

    #[test]
    fn refuses_a_script_without_a_screen_on_group() {
        assert_eq!(
            patch_gps_script(&SCRIPT.replace("id: 100", "id: 7"), "x", None),
            Err("no group with id 100".into())
        );
    }

    #[test]
    fn makes_a_transparent_texture_of_each_layouts_size() {
        let dds = transparent_dds(1280, 721);
        assert_eq!(&dds[..4], b"DDS ");
        assert_eq!(u32::from_le_bytes(dds[12..16].try_into().unwrap()), 721);
        assert_eq!(u32::from_le_bytes(dds[128..132].try_into().unwrap()), 91);
        assert_eq!(dds.len(), 148 + 1280 * 721 * 4);
        assert!(dds[148..].iter().all(|&byte| byte == 0));
    }

    /// The real thing: the mod built from the game installed on this PC (written to the temp folder, not the
    /// game's). `cargo test -p westplay-setup -- --ignored --nocapture builds_the_mod`
    #[test]
    #[ignore = "reads the PC's game"]
    fn builds_the_mod_from_the_installed_game() {
        let game = crate::game::find().into_iter().next().expect("the game installed");
        let files = GameFiles::install(&game.dir).unwrap();
        let (bytes, built) = build(&files).unwrap();
        let out = std::env::temp_dir().join(FILE_NAME);
        std::fs::write(&out, &bytes).unwrap();
        println!("{} bytes, patched {:?}, missing {:?}: {}", bytes.len(), built.patched, built.missing, out.display());
        let archive = westplay_mapdata::archive::Archive::open(&out).unwrap();
        for layout in screen_layouts().iter().filter(|layout| built.patched.contains(&layout.id)) {
            for script in layout.gps_scripts.iter().filter_map(|script| archive.read(script).unwrap()) {
                let script = String::from_utf8(script).unwrap();
                assert!(script.contains("_nameless.westplay.screen"), "{}", layout.id);
            }
        }
    }

    /// A game folder with the Smart Dash's script and screens file, and an Advanced and a 10.1" interior in
    /// each truck's folder (`gps_path`: what its screens file names).
    fn smart_dash_game(name: &str, gps_path: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("westplay-own-screen-{name}-{}", std::process::id()));
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("ui/dashboard/scania_2025_gps.sii", SCRIPT);
        for truck in ["s", "r"] {
            let dir = format!("def/vehicle/truck/scania.{truck}_2016/interior");
            write(
                &format!("{dir}/computers_2025.sui"),
                &format!("\tgps_path:\"{gps_path}\"\r\n\tgps_drawable_size: (1024, 512)\r\n"),
            );
            let interior = |unit: &str| {
                format!(
                    "SiiNunit\r\n{{\r\naccessory_interior_data: {unit}\r\n{{\r\n@include \"computers_2025.sui\"\r\n}}\r\n}}\r\n"
                )
            };
            write(&format!("{dir}/cab_{truck}_high_smartdash_advanced_dark.sii"), &interior("a_sda_drk"));
            write(&format!("{dir}/cab_{truck}_high_smartdash_dark.sii"), &interior("a_sd_drk"));
        }
        root
    }

    #[test]
    fn the_smart_dash_advanced_gets_a_screen_of_its_own() {
        let root = smart_dash_game("whole", "/ui/dashboard/scania_2025_gps.sii");
        let files = GameFiles::new(vec![westplay_mapdata::files::Mount::Folder(root.clone())]);
        let (bytes, built) = build(&files).unwrap();
        assert!(built.patched.contains(&"scania-2025") && built.patched.contains(&"scania-2025-advanced"));
        // No 7" interiors here (the Smart Dash's alone): the build says the 7" screen stays as the game has it.
        assert_eq!(built.not_sharp.as_deref(), Some("no 7\" interiors"));
        let out = root.join("westplay.scs");
        std::fs::write(&out, bytes).unwrap();
        let archive = westplay_mapdata::archive::Archive::open(&out).unwrap();
        let text = |path: &str| archive.read(path).unwrap().map(|bytes| String::from_utf8(bytes).unwrap());
        // Its own script, patched with its own texture.
        let script = text("ui/dashboard/westplay_scania_2025_advanced_gps.sii").unwrap();
        assert!(script.contains("/material/ui/westplay/screen_scania_2025_advanced.mat"), "{script}");
        for truck in ["s", "r"] {
            let dir = format!("def/vehicle/truck/scania.{truck}_2016/interior");
            let screens = text(&format!("{dir}/westplay_computers_2025_advanced.sui")).unwrap();
            assert_eq!(
                screens,
                "\tgps_path:\"/ui/dashboard/westplay_scania_2025_advanced_gps.sii\"\r\n\tgps_drawable_size: (1024, 512)\r\n"
            );
            let advanced = text(&format!("{dir}/cab_{truck}_high_smartdash_advanced_dark.sii")).unwrap();
            assert!(advanced.contains("@include \"westplay_computers_2025_advanced.sui\"\r\n"), "{advanced}");
            assert!(advanced.contains("accessory_interior_data: a_sda_drk\r\n"));
            // The 10.1" isn't touched.
            assert_eq!(text(&format!("{dir}/cab_{truck}_high_smartdash_dark.sii")), None);
        }
        drop(archive);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_own_screen_not_as_read_is_left_out_whole() {
        // A screens file naming another script (a game update): no Advanced screen, and nothing of it.
        let root = smart_dash_game("unlike", "/ui/dashboard/scania_2026_gps.sii");
        let files = GameFiles::new(vec![westplay_mapdata::files::Mount::Folder(root.clone())]);
        let (bytes, built) = build(&files).unwrap();
        assert!(built.patched.contains(&"scania-2025") && built.missing.contains(&"scania-2025-advanced"));
        let out = root.join("westplay.scs");
        std::fs::write(&out, bytes).unwrap();
        let archive = westplay_mapdata::archive::Archive::open(&out).unwrap();
        assert_eq!(archive.read("ui/dashboard/westplay_scania_2025_advanced_gps.sii").unwrap(), None);
        let interior = "def/vehicle/truck/scania.s_2016/interior/cab_s_high_smartdash_advanced_dark.sii";
        assert_eq!(archive.read(interior).unwrap(), None);
        drop(archive);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn its_zip_reads_back() {
        let files = vec![("a/b.txt".to_owned(), b"hello".to_vec()), ("c.txt".to_owned(), vec![7u8; 5000])];
        let path = std::env::temp_dir().join(format!("westplay-mod-zip-{}.scs", std::process::id()));
        std::fs::write(&path, zip(&files)).unwrap();
        let archive = westplay_mapdata::archive::Archive::open(&path).unwrap();
        assert_eq!(archive.read("a/b.txt").unwrap().as_deref(), Some(&b"hello"[..]));
        assert_eq!(archive.read("c.txt").unwrap(), Some(vec![7u8; 5000]));
        drop(archive);
        let _ = std::fs::remove_file(path);
    }
}
