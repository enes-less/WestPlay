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
//! The mod is a zip, as `.scs` mods may be. (Ported from the Python script that built it before, and checked
//! against it: the same patch byte for byte, and the same mod from the same game.)

use std::io::Write;
use std::path::{Path, PathBuf};

use westplay_mapdata::files::GameFiles;
use westplay_shared::{SCREEN_WIDTH, ScreenLayout, screen_layouts};

/// The mod's file, in the game's `mod` folder.
pub const FILE_NAME: &str = "westplay.scs";
/// Where the mod's screen materials and textures live in the game's file system.
const SCREEN_DIR: &str = "material/ui/westplay";
/// The screen's draw order in its group: the stock elements use layers 0 to 3.
const SCREEN_LAYER: u32 = 100;
/// The "screen on" group's id in the GPS scripts.
const SCREEN_ON_GROUP: &str = "100";

/// What a build made: the layouts patched, and those whose scripts the game doesn't have.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Built {
    pub patched: Vec<&'static str>,
    pub missing: Vec<&'static str>,
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
        // Each of its scripts the game has (a layout whose scripts it has none of is missing).
        let mut any = false;
        for &script in layout.gps_scripts {
            let original = game.read(script).map_err(|error| BuildError::Read(error.to_string()))?;
            let Some(original) = original else { continue };
            let text = String::from_utf8(original)
                .map_err(|_| BuildError::Script { script, problem: "not UTF-8 text".into() })?;
            let patched =
                patch_gps_script(&text, &texture).map_err(|problem| BuildError::Script { script, problem })?;
            files.push((script.to_owned(), patched.into_bytes()));
            any = true;
        }
        if any {
            built.patched.push(layout.id);
        } else {
            built.missing.push(layout.id);
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((zip(&files), built))
}

/// Writes the mod into the game's `mod` folder (`mod_dir`), replacing the one there whole: written beside it
/// first, then moved over it, so the game never finds half a file.
pub fn install(bytes: &[u8], mod_dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(mod_dir)?;
    let path = mod_dir.join(FILE_NAME);
    let partial = mod_dir.join(format!("{FILE_NAME}.partial"));
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, &path)?;
    Ok(path)
}

/// A layout's screen texture's name: `screen_daf_2021`.
pub fn texture_name(layout: &ScreenLayout) -> String {
    format!("screen_{}", layout.id.replace('-', "_"))
}

fn manifest() -> String {
    format!(
        "SiiNunit\n{{\nmod_package : .package_name\n{{\n\tpackage_version: \"{}\"\n\tdisplay_name: \"WestPlay\"\n\tauthor: \"Western\"\n\tcategory[]: \"ui\"\n\tdescription_file: \"description.txt\"\n}}\n}}\n",
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
/// a child of the group, over the whole of it. The rest of the script stays as it is.
pub fn patch_gps_script(text: &str, texture: &str) -> Result<String, String> {
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

    /// A GPS script laid out as the game's: a window, the "screen on" group with two children, and them.
    const SCRIPT: &str = "SiiNunit\n{\nui::window : _nameless.1 {\n my_children: 1\n my_children[0]: _nameless.2\n}\n\nui::group : _nameless.2 {\n coords_l: 0\n coords_r: 1279\n coords_t: 719\n coords_b: 0\n id: 100\n my_children: 2\n my_children[0]: _nameless.3\n my_children[1]: _nameless.4\n my_parent: _nameless.1\n}\n\nui::text : _nameless.3 {\n my_parent: _nameless.2\n}\n\nui::text : _nameless.4 {\n my_parent: _nameless.2\n}\n\n}\n";

    #[test]
    fn adds_the_screen_on_top_of_the_screen_on_group() {
        let patched = patch_gps_script(SCRIPT, "screen_generic").unwrap();
        assert!(patched.contains(" my_children: 3\n my_children[0]: _nameless.3\n my_children[1]: _nameless.4\n my_children[2]: _nameless.westplay.screen\n"));
        assert!(patched.contains("ui::text : _nameless.westplay.screen {\n text: \"<img src=/material/ui/westplay/screen_generic.mat xscale=stretch yscale=stretch>\"\n coords_l: 0\n coords_r: 1279\n coords_t: 719\n coords_b: 0\n"));
        assert!(patched.contains(" layer: 100\n tab: -1\n pointer: -1\n my_parent: _nameless.2\n}\n\n}\n"));
        // The window still has its one child; the rest is as it was.
        assert!(patched.starts_with("SiiNunit\n{\nui::window : _nameless.1 {\n my_children: 1\n"));
    }

    #[test]
    fn keeps_a_scripts_windows_line_endings() {
        let patched = patch_gps_script(&SCRIPT.replace('\n', "\r\n"), "screen_generic").unwrap();
        assert!(patched.contains(" my_children[2]: _nameless.westplay.screen\r\n") && !patched.contains("\r\r"));
    }

    #[test]
    fn refuses_a_script_without_a_screen_on_group() {
        assert_eq!(patch_gps_script(&SCRIPT.replace("id: 100", "id: 7"), "x"), Err("no group with id 100".into()));
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
