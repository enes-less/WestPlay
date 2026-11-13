//! WestPlay's game mod (`westplay.scs`), built on the player's PC from their own game: for each screen layout
//! (`westplay_shared::screen_layouts`), the game's GPS screen scripts with one image added as the top layer of
//! their "screen on" group, the screen texture WestPlay draws into. Nothing of the game's is shipped: the
//! scripts are the player's own copy's, patched, and the rest is WestPlay's.
//!
//! And the cab's mark in the dashboard scripts of the trucks the tablet goes in (`cab_mark`).
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

use westplay_mapdata::archive::Archive;
use westplay_mapdata::files::{GameFiles, Mount};
use westplay_shared::{
    CAB_MARK_HEIGHT, CAB_MARK_WIDTH, PARKING_MARK_HEIGHT, PARKING_MARK_WIDTH, SCREEN_HEIGHT, SCREEN_WIDTH,
    ScreenLayout, TABLET, screen_layouts,
};

use crate::cab_mark;
use crate::game::Kind;

/// The mod's file for the game `kind`, in WestPlay's own folder (`places::westplay_documents`): each game's own
/// (`westplay_shared::mod_file`).
pub fn file_name(kind: Kind) -> &'static str {
    westplay_shared::mod_file(kind.sdk_id())
}
/// Where the mod's screen materials and textures live in the game's file system.
const SCREEN_DIR: &str = "material/ui/westplay";
/// The screen's draw order: the game draws a script's elements by their layers, over all of it whatever group they're
/// in (the stock elements use layers 0 to 6), and WestPlay's is over every one of the game's but the parking camera's
/// ([`lift_parking_camera`]).
const SCREEN_LAYER: u32 = 100;
/// The "screen on" group's id in the GPS scripts.
const SCREEN_ON_GROUP: &str = "100";
/// The parking camera's group's id in the GPS scripts (the Coaches DLC's coaches: the camera behind the bus, shown
/// as it reverses or as the player turns it on).
const PARKING_CAMERA_GROUP: &str = "910";
/// The parking camera's mark's texture (`westplay_shared::PARKING_MARK_WIDTH`), in the camera's group: drawn only as
/// the game shows the camera, so the plugin can tell WestPlay's screen is under it.
const PARKING_MARK: &str = "parking_mark";

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
/// square, the frame lost its bottom and top and was drawn 1.3 times too tall (owner 2026-10-21). The trucks' own
/// screens' displays each leave a thin edge of theirs out (their models' `gps.tobj` pieces, 2026-11-03; each
/// script's "screen on" group its whole window): the DAF 2021's and the Iveco S-Way's 0.25% of its width each
/// side, the Renault T 2024's 0.5% of every side, the Volvo FH's (2021 and 2024) 0.54% and 0.55%; the frame's
/// margin and round corners were cut by up to 7 pixels there. (The Smart Dash Advanced's 0.1% is under a pixel; the
/// Smart Dash's display shows all of its.) Each page is laid out in its part's shape (westplay_shared `page_height`).
const SHOWN_PARTS: &[(&str, ShownPart)] = &[
    ("generic", ShownPart { left: 0.005, right: 0.995, top: 0.122, bottom: 0.735 }),
    ("daf-2021", ShownPart { left: 0.0025, right: 0.9975, top: 0.0, bottom: 1.0 }),
    ("iveco-sway", ShownPart { left: 0.0025, right: 0.9975, top: 0.0, bottom: 1.0 }),
    ("renault-t-2024", ShownPart { left: 0.005, right: 0.995, top: 0.005, bottom: 0.995 }),
    ("volvo-fh", ShownPart { left: 0.0054, right: 0.9946, top: 0.0055, bottom: 0.9945 }),
];

/// A mod vehicle's own screen WestPlay draws on (owner 2026-10-24, Oyuncuyusbis' coaches: "directly in their
/// screen"), from the installed mod that brings the vehicle: the layout's script (its one, in
/// `screen_layouts.txt`) is a copy of the mod's `source`, patched as the game's are, WestPlay's image on the part
/// of it the display shows (`shown`); and the vehicle's interiors naming `source` (`gps_path`), those whose
/// `model` is one of `models` (all of them, with none), are copies of the mod's with `gps_path` naming the copy and
/// the screen drawn at [`SHARP_SIZE`] (theirs 256 x 256 or 512 x 512, of which only a band shows). WestPlay's own
/// script, not the mod's: two of the coaches name the same path (`actros_gps.sii`) for different scripts, another
/// truck's mod may bring a script by that path too, and one coach's interiors show two parts of theirs. Built
/// whole or not at all: if anything isn't as read (a version of the mod not read yet), the layout is left out and
/// the vehicle keeps its own screen.
struct ModScreen {
    layout: &'static str,
    truck: &'static str,
    source: &'static str,
    models: &'static [&'static str],
    shown: ShownPart,
}

/// Oyuncuyusbis' coaches. What each display shows of its script's 256 x 256 screen (fractions from its left and
/// top), and its size, from its model's screen piece (the interior's model's or its animated model's piece on
/// `share/gps.tobj`: its corners and their texture coordinates, `examples/cab_model` CAB_VERTS, 2026-10-24).
const MOD_SCREENS: &[ModScreen] = &[
    // MB Travego 16 SHD: its standard and metallic interiors (`model`, `modelo`: the screen in their animated
    // models) show rows 13.5%-75.6%, 16.2 x 9.6 cm; its 2023 ones (`model23`) 13.4%-86.6% of the same display.
    ModScreen {
        layout: "travego-16",
        truck: "t16shd",
        source: "ui/dashboard/t16_gps.sii",
        models: &["/kazanciyan/newt16shd/interior/model.pmd", "/kazanciyan/newt16shd/interior/modelo.pmd"],
        shown: ShownPart { left: 0.0, right: 1.0, top: 0.135, bottom: 0.756 },
    },
    ModScreen {
        layout: "travego-16-23",
        truck: "t16shd",
        source: "ui/dashboard/t16_gps.sii",
        models: &["/kazanciyan/newt16shd/interior/model23.pmd"],
        shown: ShownPart { left: 0.001, right: 0.999, top: 0.134, bottom: 0.866 },
    },
    // Setra S 531 DT: the whole screen (its script maps the whole of it), 15.2 x 9.0 cm.
    ModScreen {
        layout: "setra-531",
        truck: "setradt531",
        source: "ui/dashboard/actros_gps.sii",
        models: &[],
        shown: ShownPart { left: 0.001, right: 0.999, top: 0.001, bottom: 0.999 },
    },
    // MAN Lion's Coach: rows 7.9%-66.6%, 15.3 x 9.3 cm (rounded corners).
    ModScreen {
        layout: "man-lions-coach",
        truck: "manlions",
        source: "ui/gps/man_e6_gps.sii",
        models: &[],
        shown: ShownPart { left: 0.0, right: 0.997, top: 0.079, bottom: 0.666 },
    },
];

/// Whether `layout` is a mod vehicle's own screen ([`MOD_SCREENS`]).
pub fn mod_screen_layout(layout: &str) -> bool {
    MOD_SCREENS.iter().any(|screen| screen.layout == layout)
}

/// A screen of the game's own whose script is a bare window (no "screen on" group): WestPlay's image goes on the part
/// of the window the screen's glass shows (`shown`). The script is the game's, patched as the GPS screens' are, and
/// WestPlay shows on it only in its trucks (`westplay_shared::screen_supported`); a player without it (the DLC it's
/// from) gets a window of WestPlay's at its path. Nothing of the mod whose part shows it is changed.
struct WindowScreen {
    layout: &'static str,
    shown: ShownPart,
}

/// The game's toy GPS (Cabin Accessories: `gps.dlc_toys.sii`, a 300 x 168 window), the screen of Oyuncuyusbis' Setra S
/// 516 HD/2's tablet part ("Gps", on either of its mounts): WestPlay on it rather than on the bus's own screen (owner
/// 2026-10-26: "way better"; "we should not change anything of the bus mod, just detect that screen"). The tablet's
/// screen, 20.1 x 12.2 cm (1.648:1), shows 2.14%-97.86% of the part's texture both ways (its models' screen piece,
/// `examples/cab_model` CAB_VERTS), drawn at the part's own 512 x 256.
const WINDOW_SCREENS: &[WindowScreen] = &[WindowScreen {
    layout: "setra-516",
    shown: ShownPart { left: 0.0214, right: 0.9786, top: 0.0214, bottom: 0.9786 },
}];

/// The vehicles whose own screens WestPlay draws on ([`MOD_SCREENS`], and [`WINDOW_SCREENS`]' trucks).
pub fn mod_screen_trucks() -> impl Iterator<Item = &'static str> {
    let mut trucks: Vec<&'static str> = MOD_SCREENS
        .iter()
        .map(|screen| screen.truck)
        .chain(WINDOW_SCREENS.iter().flat_map(|screen| westplay_shared::layout_trucks(screen.layout).iter().copied()))
        .collect();
    trucks.sort_unstable();
    trucks.dedup();
    trucks.into_iter()
}

/// The mods among `mods` (archives or folders: the installed ones, in no particular order) that bring a vehicle
/// whose own screen WestPlay draws on, mounted, for [`build`]. Two copies of one vehicle's mod: the last listed is
/// read.
pub fn mod_screen_mods(mods: &[PathBuf]) -> GameFiles {
    let mut mounted = GameFiles::new(Vec::new());
    for path in mods {
        let defines =
            |dir: &dyn Fn(&str) -> bool| mod_screen_trucks().any(|truck| dir(&format!("def/vehicle/truck/{truck}")));
        if path.is_dir() {
            if defines(&|dir: &str| path.join(dir).is_dir()) {
                mounted.mount(Mount::Folder(path.clone()));
            }
            continue;
        }
        let Ok(archive) = Archive::open(path) else { continue };
        // Its folder listed, or (an archive whose folders are hidden) the vehicle's own definition in it.
        let has =
            |dir: &str| archive.list(dir).ok().flatten().is_some() || archive.contains(&format!("{dir}/data.sii"));
        if defines(&has) {
            mounted.mount(Mount::Archive(archive));
        }
    }
    mounted
}

/// A mod screen's files (`screen`, its layout's `script`, its texture `texture`, `rows` tall), from `mods`: the
/// script's copy and the interiors' definitions.
fn mod_screen_files(
    mods: &GameFiles,
    screen: &ModScreen,
    script: &str,
    texture: &str,
    rows: u32,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let read = |path: &str| -> Result<String, String> {
        let bytes = mods.read(path).map_err(|error| error.to_string())?.ok_or(format!("no {path}"))?;
        // Some versions scramble their definitions (the game reads them as text).
        let text = westplay_mapdata::save::text(&bytes).map_err(|error| format!("{path}: {error}"))?;
        String::from_utf8(text.into_owned()).map_err(|_| format!("{path}: not UTF-8 text"))
    };
    let mut files =
        vec![(script.to_owned(), patch_gps_script(&read(screen.source)?, texture, Some(&screen.shown), rows)?.into_bytes())];
    let dir = format!("def/vehicle/truck/{}/interior", screen.truck);
    let names = mods.list(&dir).map_err(|error| error.to_string())?;
    for name in names.iter().filter(|name| name.ends_with(".sii")) {
        let path = format!("{dir}/{name}");
        let text = read(&path)?;
        if quoted(&text, "gps_path").as_deref() != Some(&format!("/{}", screen.source)) {
            continue;
        }
        let model = quoted(&text, "model");
        if !screen.models.is_empty() && !model.is_some_and(|model| screen.models.contains(&model.as_str())) {
            continue;
        }
        files.push((path.clone(), own_screen_interior(&text, script).map_err(|problem| format!("{path}: {problem}"))?));
    }
    if files.len() == 1 {
        return Err(format!("no interiors of {} name {}", screen.truck, screen.source));
    }
    Ok(files)
}

/// Whether the mod built at `built` (WestPlay's `.scs`) has the mod vehicles' screens as `screen_mods` (the installed
/// mods bringing them: [`mod_screen_mods`]) would make them now: each screen's files there, the same, and no screen of
/// a vehicle whose mod is gone (or changed so it can't be made).
pub fn mod_screens_current(screen_mods: &GameFiles, built: &Path) -> bool {
    let Ok(archive) = Archive::open(built) else { return false };
    let ours = GameFiles::new(vec![Mount::Archive(archive)]);
    MOD_SCREENS.iter().all(|screen| {
        let Some(index) = screen_layouts().iter().position(|layout| layout.id == screen.layout) else { return false };
        let layout = &screen_layouts()[index];
        let [script] = layout.gps_scripts else { return false };
        match mod_screen_files(screen_mods, screen, script, &texture_name(layout), ScreenLayout::texture_height(index)) {
            Ok(files) => files.iter().all(|(path, bytes)| ours.read(path).ok().flatten().as_ref() == Some(bytes)),
            Err(_) => ours.read(script).ok().flatten().is_none(),
        }
    })
}

/// The quoted value of a definition's one `name:` line (`gps_path: "/ui/gps.sii"`, however spaced); `None` without
/// one, or with more than one.
fn quoted(text: &str, name: &str) -> Option<String> {
    let mut found = text.lines().filter_map(|line| {
        let rest = line.trim_start().strip_prefix(name)?.trim_start().strip_prefix(':')?;
        Some(rest.trim().strip_prefix('"')?.split('"').next()?.to_owned())
    });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// An interior's definition naming `script` (WestPlay's) as its screen, drawn at [`SHARP_SIZE`]: its `gps_path`
/// line changed, its `gps_drawable_size` line (if it has one) replaced, or one added after `gps_path`.
fn own_screen_interior(text: &str, script: &str) -> Result<Vec<u8>, String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.split(newline).map(str::to_owned).collect();
    let at = |name: &str, lines: &[String]| -> Vec<usize> {
        (0..lines.len()).filter(|&i| lines[i].trim_start().starts_with(name)).collect()
    };
    let [path] = at("gps_path", &lines)[..] else { return Err("not one gps_path line".into()) };
    let indent = lines[path][..lines[path].len() - lines[path].trim_start().len()].to_owned();
    lines[path] = format!("{indent}gps_path: \"/{script}\"");
    match at("gps_drawable_size", &lines)[..] {
        [] => lines.insert(path + 1, format!("{indent}{SHARP_SIZE}")),
        [size] => lines[size] = format!("{indent}{SHARP_SIZE}"),
        _ => return Err("more than one gps_drawable_size line".into()),
    }
    Ok(lines.join(newline).into_bytes())
}

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
/// Screens files that set the 7" screen's size themselves (`gps_drawable_size: (256, 256)` beside the stock
/// `gps_path`), included by every interior of their truck: the Scania S BEV's (the S's 7" display, its own
/// interiors: owner 2026-10-26). Each made [`SHARP_SIZE`] on its own (`sharp_screens_file`), apart from the S
/// and R's, so neither stops the other.
const SHARP_SCREENS: &[&str] = &["def/vehicle/truck/scania.s_2024e/interior/computers.sui"];
const SHARP_SIZE: &str = "gps_drawable_size: (1024, 1024)";
/// The Coaches DLC's screens files (every interior of their bus includes its own), each naming its bus's script
/// (the layouts `scania-touring`, `man-lions-24`) at 512 x 256: a 1280-wide frame halved and more, on displays the
/// size of the Volvo FH's. Made [`BUS_SIZE`], the Volvo FH's own (the same 2:1, so the displays show the same of
/// it), on their own, apart from each other (`sized_screens_file`).
const BUS_SCREENS: &[(&str, &str)] = &[
    ("def/vehicle/bus/scania.touring_23/interior/computers.sui", "ui/dashboard/scania_touring_23_gps.sii"),
    ("def/vehicle/bus/man.lions_24/interior/computers.sui", "ui/dashboard/man_lions_24_gps.sii"),
];
const BUS_SIZE: &str = "gps_drawable_size: (1024, 512)";
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

/// A screens file of [`SHARP_SCREENS`] with its one stock-screen size made [`SHARP_SIZE`] ([`sized_screens_file`]).
fn sharp_screens_file(game: &GameFiles, path: &str) -> Result<Option<Vec<u8>>, String> {
    sized_screens_file(game, path, STOCK_GPS, SHARP_SIZE)
}

/// The screens file at `path` with its one GPS screen's size made `size` (a `gps_drawable_size` line); `None` if
/// the game hasn't it (its DLC not owned). Not as read (a screen named other than `script`, or no size or two): an
/// error, the file left as the game has it.
fn sized_screens_file(game: &GameFiles, path: &str, script: &str, size: &str) -> Result<Option<Vec<u8>>, String> {
    let Some(bytes) = game.read(path).map_err(|error| error.to_string())? else { return Ok(None) };
    let text = String::from_utf8(bytes).map_err(|_| format!("{path}: not UTF-8 text"))?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.split(newline).map(str::to_owned).collect();
    let starting = |name: &str| -> Vec<usize> {
        (0..lines.len()).filter(|&at| lines[at].trim_start().starts_with(name)).collect()
    };
    let (screens, sizes) = (starting("gps_path"), starting("gps_drawable_size"));
    let ([screen], [size_at]) = (&screens[..], &sizes[..]) else {
        return Err(format!(
            "{path}: {} gps_path and {} gps_drawable_size lines, not one each",
            screens.len(),
            sizes.len()
        ));
    };
    if !lines[*screen].contains(&format!("\"/{script}\"")) {
        return Err(format!("{path}: names another GPS screen than {script}"));
    }
    let indent = lines[*size_at][..lines[*size_at].len() - lines[*size_at].trim_start().len()].to_owned();
    lines[*size_at] = format!("{indent}{size}");
    Ok(Some(lines.join(newline).into_bytes()))
}

/// An own screen's files (`own`, its layout's `script`, its texture `texture`, `rows` tall), from `game`: the
/// script's copy, and per interiors' folder the screens file's copy and the interiors' definitions.
fn own_screen_files(
    game: &GameFiles,
    own: &OwnScreen,
    script: &str,
    texture: &str,
    rows: u32,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let read = |path: &str| -> Result<String, String> {
        let bytes = game.read(path).map_err(|error| error.to_string())?.ok_or(format!("no {path}"))?;
        String::from_utf8(bytes).map_err(|_| format!("{path}: not UTF-8 text"))
    };
    let mut files = vec![(script.to_owned(), patch_gps_script(&read(own.source)?, texture, None, rows)?.into_bytes())];
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

/// What a build made: the layouts patched, and those whose scripts the game doesn't have; why the 7" Scania
/// screen stays at the game's size, if it does (its interiors not as read: `sharp_screen_files`); and how many
/// dashboard scripts have the cab's mark.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Built {
    pub patched: Vec<&'static str>,
    pub missing: Vec<&'static str>,
    pub not_sharp: Option<String>,
    pub cab_marks: usize,
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

/// The mod built from `game` (the installed game, mounted), as the bytes of its `.scs` file; the tablet at the
/// game's trucks' slots for it and at `mod_trucks` (the installed mods': `tablet::mod_trucks`); the screens of the
/// mod vehicles in `screen_mods` (the installed mods bringing them: [`mod_screen_mods`]); the cab's mark in the
/// dashboard scripts of `cab_marks` (`cab_mark::for_tablet`).
pub fn build(
    game: &GameFiles,
    mod_trucks: &[crate::tablet::Spot],
    screen_mods: &GameFiles,
    cab_marks: &cab_mark::Marks,
) -> Result<(Vec<u8>, Built), BuildError> {
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("manifest.sii".into(), manifest().into_bytes()),
        ("description.txt".into(), b"WestPlay screens. Requires the WestPlay app and plugin.\n".to_vec()),
    ];
    let mut built = Built::default();
    for (index, layout) in screen_layouts().iter().enumerate() {
        let texture = texture_name(layout);
        let rows = ScreenLayout::texture_height(index);
        files.push((format!("{SCREEN_DIR}/{texture}.mat"), material(&texture).into_bytes()));
        files.push((format!("{SCREEN_DIR}/{texture}.tobj"), tobj(&format!("/{SCREEN_DIR}/{texture}.dds"))));
        files.push((format!("{SCREEN_DIR}/{texture}.dds"), transparent_dds(SCREEN_WIDTH, rows)));
        // WestPlay's tablet: its model shows the texture (no script draws it), whole or missing.
        if layout.id == TABLET {
            let screen = format!("/{SCREEN_DIR}/{texture}.tobj");
            let trucks = crate::tablet::trucks(game).map(|own| {
                let all: std::collections::BTreeSet<crate::tablet::Spot> =
                    own.into_iter().chain(mod_trucks.iter().cloned()).collect();
                all.into_iter().collect::<Vec<_>>()
            });
            // Each truck's hanging point's pitch, from its interiors in the game (a mod truck's aren't there: none).
            let made = trucks.and_then(|trucks| {
                let pitches = crate::tablet::hanging_pitches(game, &trucks);
                crate::tablet::files(&trucks, &pitches, &screen, ScreenLayout::texture_height(index))
            });
            match made {
                Ok(tablet) => {
                    files.extend(tablet);
                    built.patched.push(layout.id);
                }
                Err(_) => built.missing.push(layout.id),
            }
            continue;
        }
        // A mod vehicle's own screen: whole, or missing (its mod not installed).
        if let Some(screen) = MOD_SCREENS.iter().find(|screen| screen.layout == layout.id) {
            match layout.gps_scripts {
                [script] => match mod_screen_files(screen_mods, screen, script, &texture, rows) {
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
        // A screen of WestPlay's own making: whole, or missing.
        if let Some(own) = OWN_SCREENS.iter().find(|own| own.layout == layout.id) {
            match layout.gps_scripts {
                [script] => match own_screen_files(game, own, script, &texture, rows) {
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
        // Each of its scripts the game has (a layout whose scripts it has none of is missing); a window screen's,
        // WestPlay's own window where the game has none.
        let window = WINDOW_SCREENS.iter().find(|screen| screen.layout == layout.id);
        let mut any = false;
        for &script in layout.gps_scripts {
            let original = game.read(script).map_err(|error| BuildError::Read(error.to_string()))?;
            let text = match (original, window) {
                (Some(original), _) => String::from_utf8(original)
                    .map_err(|_| BuildError::Script { script, problem: "not UTF-8 text".into() })?,
                (None, Some(_)) => own_window(),
                (None, None) => continue,
            };
            let shown = SHOWN_PARTS.iter().find(|(id, _)| *id == layout.id).map(|(_, part)| part);
            let patched = match window {
                Some(window) => patch_window_script(&text, &texture, &window.shown, rows),
                None => patch_gps_script(&text, &texture, shown, rows),
            }
            .map_err(|problem| BuildError::Script { script, problem })?;
            files.push((script.to_owned(), patched.into_bytes()));
            any = true;
        }
        if any {
            built.patched.push(layout.id);
        } else {
            built.missing.push(layout.id);
        }
    }
    // The cab's mark: its texture, fully transparent, and the dashboard scripts with it, listed.
    let mark = cab_mark::TEXTURE;
    files.push((format!("{SCREEN_DIR}/{mark}.mat"), material(mark).into_bytes()));
    files.push((format!("{SCREEN_DIR}/{mark}.tobj"), tobj(&format!("/{SCREEN_DIR}/{mark}.dds"))));
    files.push((format!("{SCREEN_DIR}/{mark}.dds"), transparent_dds(CAB_MARK_WIDTH, CAB_MARK_HEIGHT)));
    // The parking camera's mark (in the GPS scripts with a camera): its texture, fully transparent.
    let parking = PARKING_MARK;
    files.push((format!("{SCREEN_DIR}/{parking}.mat"), material(parking).into_bytes()));
    files.push((format!("{SCREEN_DIR}/{parking}.tobj"), tobj(&format!("/{SCREEN_DIR}/{parking}.dds"))));
    files.push((format!("{SCREEN_DIR}/{parking}.dds"), transparent_dds(PARKING_MARK_WIDTH, PARKING_MARK_HEIGHT)));
    files.push((cab_mark::LIST.to_owned(), cab_marks.list().into_bytes()));
    files.extend(cab_marks.scripts.iter().cloned());
    built.cab_marks = cab_marks.scripts.len();
    // The 7" screen at a size it shows sharp, if its interiors are as read (else the game's size stays, and
    // the build says why).
    match sharp_screen_files(game) {
        Ok(sharp) => files.extend(sharp),
        Err(why) => built.not_sharp = Some(why),
    }
    for path in SHARP_SCREENS {
        match sharp_screens_file(game, path) {
            Ok(Some(sharp)) => files.push(((*path).to_owned(), sharp)),
            Ok(None) => {}
            Err(why) => {
                built.not_sharp.get_or_insert(why);
            }
        }
    }
    // The coaches' screens drawn sharp, where their screens are WestPlay's.
    for &(path, script) in BUS_SCREENS {
        let ours = screen_layouts()
            .iter()
            .any(|layout| layout.gps_scripts.contains(&script) && built.patched.contains(&layout.id));
        match sized_screens_file(game, path, script, BUS_SIZE) {
            Ok(Some(sized)) if ours => files.push((path.to_owned(), sized)),
            Ok(_) => {}
            Err(why) => {
                built.not_sharp.get_or_insert(why);
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((zip(&files), built))
}

/// Writes the mod into `mod_dir` (WestPlay's own folder), replacing the one there whole: written beside it
/// first, then moved over it, so the plugin never finds half a file.
/// A step that failed names itself and its path (`writing C:\…\WestPlay\westplay.scs.partial: …`), so a
/// player's report says which.
pub fn install(bytes: &[u8], mod_dir: &Path, kind: Kind) -> std::io::Result<PathBuf> {
    let step = |what: &str, path: &Path| {
        let what = format!("{what} {}", path.display());
        move |error: std::io::Error| std::io::Error::new(error.kind(), format!("{what}: {error}"))
    };
    std::fs::create_dir_all(mod_dir).map_err(step("making the folder", mod_dir))?;
    let path = mod_dir.join(file_name(kind));
    let partial = mod_dir.join(format!("{}.partial", file_name(kind)));
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

/// A UI material showing `texture`'s texture object, beside it (the screens', and the tablet's shop icon).
pub(crate) fn material(texture: &str) -> String {
    format!(
        "effect : \"ui.rfx\" {{\n\ttexture : \"texture\" {{\n\t\tsource : \"{texture}.tobj\"\n\t\tu_address : clamp\n\t\tv_address : clamp\n\t\tmip_filter : none\n\t}}\n}}\n"
    )
}

/// A texture object pointing at `dds_path`: the header of a stock UI texture object (`/material/ui/white.tobj`),
/// then the path.
pub(crate) fn tobj(dds_path: &str) -> Vec<u8> {
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
    screen_dds(width, height, &vec![0; (width * height * 4) as usize])
}

/// A `width` x `height` texture of `pixels` (blue, green, red, alpha each, row by row), laid out as the screens'
/// ([`transparent_dds`]).
pub(crate) fn screen_dds(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
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
    bytes.extend(pixels);
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
/// a child of the group, over the whole of it, or over the part of it the display shows (`shown`). The texture is
/// `rows` tall (`ScreenLayout::texture_height`): the frame's [`SCREEN_HEIGHT`] rows go on that part, the rows beyond
/// them (the layout's mark, repeating the frame's bottom row) below it, off the screen; stretched over the part
/// whole, they squashed the frame up to 1.6% and smeared its bottom row (its black round corners) along the
/// screen's bottom edge (2026-11-03). The rest of the script stays as it is.
pub fn patch_gps_script(text: &str, texture: &str, shown: Option<&ShownPart>, rows: u32) -> Result<String, String> {
    let lifted = lift_parking_camera(text)?;
    let text = lifted.as_str();
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
    let number = |text: &str| text.parse::<f64>().map_err(|_| format!("the group's coordinate {text}"));
    let (l, r, t, b) =
        (number(coord("coords_l")?)?, number(coord("coords_r")?)?, number(coord("coords_t")?)?, number(coord("coords_b")?)?);
    let whole = ShownPart { left: 0.0, right: 1.0, top: 0.0, bottom: 1.0 };
    let part = shown.unwrap_or(&whole);
    // Its coordinates go up the screen: the top is the larger. The frame's rows on the part, the texture's
    // reaching below it by its extra rows.
    let (top, bottom) = (t - (t - b) * part.top, t - (t - b) * part.bottom);
    let bottom = top - (top - bottom) * f64::from(rows) / f64::from(SCREEN_HEIGHT);
    let at = |value: f64| format!("{}", value.round() as i64);
    let (left, right, top, bottom) = (at(l + (r - l) * part.left), at(l + (r - l) * part.right), at(top), at(bottom));
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
    let patched = patched.join(newline);
    // The parking camera's mark in its group, drawn with it: the screen under the camera then (no clicks to it).
    match parking_camera(&lines, &blocks) {
        Some(camera) => add_to(&patched, camera.name, "_nameless.westplay.parking", PARKING_MARK, mark_corner),
        None => Ok(patched),
    }
}

/// A GPS script's parking camera: its group with id [`PARKING_CAMERA_GROUP`], if it has one.
fn parking_camera<'a, 'b>(lines: &[&'a str], blocks: &'b [Block<'a>]) -> Option<&'b Block<'a>> {
    blocks.iter().find(|block| block.kind == "ui::group" && field(lines, block, "id") == Some(PARKING_CAMERA_GROUP))
}

/// A mark's place in a block (left, right, top, bottom, from the block's own): its left and bottom corner, in from
/// both by [`MARK_SHARE`] of the block each way (its coordinates may go up the screen or down it).
fn mark_corner([left, right, top, bottom]: [f64; 4]) -> [f64; 4] {
    let side = |span: f64| (span.abs() / MARK_SHARE).ceil().max(1.0);
    let inward = if top >= bottom { 1.0 } else { -1.0 };
    [left, left + side(right - left), bottom + inward * side(top - bottom), bottom]
}

/// The parking camera of a GPS script (its group with id [`PARKING_CAMERA_GROUP`], and everything in it) a layer of
/// its own over WestPlay's screen ([`SCREEN_LAYER`]): each of its elements' layers past it by its own, so the
/// camera keeps its order (its background, its picture, its marks). The game draws by layers across the whole
/// script, so WestPlay's image (in the "screen on" group) was drawn over the camera, though the camera's group comes
/// after that group (owner 2026-11-13, reversing a coach: "WestPlay should be under it, not over"). A script with
/// no camera stays as it is.
fn lift_parking_camera(text: &str) -> Result<String, String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = text.split(newline).collect();
    let blocks = blocks(&lines);
    let Some(camera) = parking_camera(&lines, &blocks) else {
        return Ok(text.to_owned());
    };
    // The group, and what's in it at any depth (by each block's parent).
    let mut inside = vec![camera.name];
    loop {
        let before = inside.len();
        for block in &blocks {
            if !inside.contains(&block.name)
                && field(&lines, block, "my_parent").is_some_and(|parent| inside.contains(&parent))
            {
                inside.push(block.name);
            }
        }
        if inside.len() == before {
            break;
        }
    }
    let mut lifted: Vec<String> = lines.iter().map(|line| (*line).to_owned()).collect();
    for block in blocks.iter().filter(|block| inside.contains(&block.name)) {
        let at = (block.start + 1..block.end)
            .find(|&index| lines[index].trim().split_once(':').is_some_and(|(key, _)| key == "layer"))
            .ok_or_else(|| format!("the parking camera's {} has no layer", block.name))?;
        let value = lines[at].trim().split_once(':').map_or("", |(_, value)| value.trim());
        let layer: u32 =
            value.parse().map_err(|_| format!("the parking camera's {} has the layer {value}", block.name))?;
        let indent = &lines[at][..lines[at].len() - lines[at].trim_start().len()];
        lifted[at] = format!("{indent}layer: {}", SCREEN_LAYER + 1 + layer);
    }
    Ok(lifted.join(newline))
}

/// Adds the cab's mark (`texture`, `cab_mark`) to a dashboard script: a child of its window (its one block with no
/// parent), and of each group directly in the window that covers all of it, a square in the corner, fully
/// transparent. The rest of the script stays as it is.
///
/// The newer dashboards (the Volvo VNL 2025's, the Kenworth T680 2022's: a 1440 by 540 window) hold everything in
/// such groups, one display each, and a mark of a unit there was never drawn: in the VNL 2025, looking away from the
/// tablet was taken for leaving the cab, the music muffled (owner 2026-11-06). So the mark is some pixels on any
/// screen, 1/64 of the window each way ([`MARK_SHARE`]), and in each display as well as the window.
pub fn patch_dashboard_script(text: &str, texture: &str) -> Result<String, String> {
    if text.contains("_nameless.westplay.cab") {
        return Err("has the mark already".into());
    }
    let mut patched = add_to_window(text, "_nameless.westplay.cab", texture, mark_corner)?;
    for (index, group) in full_window_groups(text)?.into_iter().enumerate() {
        patched = add_to(&patched, &group, &format!("_nameless.westplay.cab{index}"), texture, mark_corner)?;
    }
    Ok(patched)
}

/// How much of a dashboard's window the cab's mark covers each way: 1/64 (at least a unit).
const MARK_SHARE: f64 = 64.0;

/// The groups directly in a UI script's window (its one block with no parent) that cover all of it, by name.
fn full_window_groups(text: &str) -> Result<Vec<String>, String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = text.split(newline).collect();
    let blocks = blocks(&lines);
    let roots: Vec<&Block> = blocks.iter().filter(|block| field(&lines, block, "my_parent") == Some("null")).collect();
    let [window] = roots[..] else { return Err(format!("{} blocks without a parent, not one", roots.len())) };
    let coords =
        |block: &Block| ["coords_l", "coords_r", "coords_t", "coords_b"].map(|name| field(&lines, block, name));
    Ok(blocks
        .iter()
        .filter(|block| {
            block.kind == "ui::group"
                && field(&lines, block, "my_parent") == Some(window.name)
                && coords(block) == coords(window)
        })
        .map(|block| block.name.to_owned())
        .collect())
}

/// Adds the WestPlay screen image `texture` to a window screen's script ([`WindowScreen`]), over the part of the
/// window its glass shows (`shown`), on top. The rest of the script stays as it is.
pub fn patch_window_script(text: &str, texture: &str, shown: &ShownPart, rows: u32) -> Result<String, String> {
    if text.contains("_nameless.westplay.screen") {
        return Err("has the screen already".into());
    }
    // Its coordinates may go up the screen or down it: its top is the top, either way. The script's coordinates are
    // whole units, a unit some pixels of the screen: each edge rounded outward, and a unit further, so none of what's
    // under it shows at the glass's edge (rounded to the nearest, the toy GPS's map showed a pixel's line at the
    // Setra's tablet's top and bottom: owner 2026-10-26).
    add_to_window(text, "_nameless.westplay.screen", texture, |[l, r, t, b]| {
        let outward = |inside: f64, outside: f64, value: f64| {
            if outside >= inside { value.ceil() + 1.0 } else { value.floor() - 1.0 }
        };
        let (left, right) = (l + (r - l) * shown.left, l + (r - l) * shown.right);
        let (top, bottom) = (t - (t - b) * shown.top, t - (t - b) * shown.bottom);
        // The texture's rows beyond the frame's below the glass, as `patch_gps_script` puts them.
        let bottom = top - (top - bottom) * f64::from(rows) / f64::from(SCREEN_HEIGHT);
        [
            outward(right, left, left),
            outward(left, right, right),
            outward(bottom, top, top),
            outward(top, bottom, bottom),
        ]
    })
}

/// A window of WestPlay's for a window screen whose script the game doesn't have: 1000 units each way, nothing in it.
fn own_window() -> String {
    [
        "SiiNunit",
        "{",
        "ui::window : _nameless.westplay.window {",
        " window_handler: null",
        " clip_children: true",
        " keep_aspect: none",
        " user_string_data: \"\"",
        " first_direction_focus_id: 0",
        " fitting: false",
        " my_children: 0",
        " coords_l: 0",
        " coords_r: 1000",
        " coords_t: 1000",
        " coords_b: 0",
        " area_l: 0",
        " area_r: 1000",
        " area_t: 1000",
        " area_b: 0",
        " id: 0",
        " layer: 0",
        " tab: -1",
        " pointer: -1",
        " my_parent: null",
        "}",
        "",
        "}",
        "",
    ]
    .join("\n")
}

/// Adds `texture`'s image to a UI script as the last child of its window (its one block with no parent), as `element`,
/// where `place` puts it (left, right, top, bottom) from the window's own coordinates (the same).
fn add_to_window(
    text: &str,
    element: &str,
    texture: &str,
    place: impl Fn([f64; 4]) -> [f64; 4],
) -> Result<String, String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = text.split(newline).collect();
    let blocks = blocks(&lines);
    let roots: Vec<&Block> = blocks.iter().filter(|block| field(&lines, block, "my_parent") == Some("null")).collect();
    let [window] = roots[..] else { return Err(format!("{} blocks without a parent, not one", roots.len())) };
    add_to(text, window.name, element, texture, place)
}

/// Adds `texture`'s image to a UI script as the last child of its block `parent`, as `element`, where `place` puts it
/// (left, right, top, bottom) from the block's own coordinates (the same).
fn add_to(
    text: &str,
    parent: &str,
    element: &str,
    texture: &str,
    place: impl Fn([f64; 4]) -> [f64; 4],
) -> Result<String, String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = text.split(newline).collect();
    let blocks = blocks(&lines);
    let window = blocks.iter().find(|block| block.name == parent).ok_or_else(|| format!("no block {parent}"))?;
    let count: usize =
        field(&lines, window, "my_children").unwrap_or("0").parse().map_err(|_| "my_children isn't a count")?;
    let coord = |name| -> Result<f64, String> {
        let text = field(&lines, window, name).ok_or_else(|| format!("the window has no {name}"))?;
        text.parse().map_err(|_| format!("the window's {name} {text}"))
    };
    let [left, right, top, bottom] =
        place([coord("coords_l")?, coord("coords_r")?, coord("coords_t")?, coord("coords_b")?]);
    let at = |value: f64| format!("{}", value.round() as i64);

    let mut body = Vec::new();
    for line in &lines[window.start + 1..window.end] {
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
    let image = [
        String::new(),
        format!("ui::text : {element} {{"),
        format!(" text: \"<img src=/{SCREEN_DIR}/{texture}.mat xscale=stretch yscale=stretch>\""),
        format!(" coords_l: {}", at(left)),
        format!(" coords_r: {}", at(right)),
        format!(" coords_t: {}", at(top)),
        format!(" coords_b: {}", at(bottom)),
        " area_l: 1".into(),
        " area_r: 0".into(),
        " area_t: 0".into(),
        " area_b: 1".into(),
        " id: 0".into(),
        format!(" layer: {SCREEN_LAYER}"),
        " tab: -1".into(),
        " pointer: -1".into(),
        format!(" my_parent: {}", window.name),
        "}".into(),
    ];
    let last_end = blocks.last().map(|block| block.end).ok_or("no blocks")?;
    let mut patched: Vec<String> = lines[..=window.start].iter().map(|line| (*line).to_owned()).collect();
    patched.extend(body);
    patched.extend(lines[window.end..=last_end].iter().map(|line| (*line).to_owned()));
    patched.extend(image);
    patched.extend(lines[last_end + 1..].iter().map(|line| (*line).to_owned()));
    Ok(patched.join(newline))
}

/// A zip of `files` (path, contents), deflated, as the game reads `.scs` mods.
pub(crate) fn zip(files: &[(String, Vec<u8>)]) -> Vec<u8> {
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
        let path = install(b"mod", &mod_dir, Kind::Ets2).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"mod");
        assert_eq!(path.file_name().unwrap(), "westplay.scs");
        assert!(!mod_dir.join("westplay.scs.partial").exists());
        // ATS's beside it, ETS2's left as it is.
        let ats = install(b"ats mod", &mod_dir, Kind::Ats).unwrap();
        assert_eq!(ats.file_name().unwrap(), "westplay_ats.scs");
        assert_eq!(std::fs::read(&path).unwrap(), b"mod");
        // A file where the folder should be: the step and its path in the error.
        let blocked = root.join("blocked");
        std::fs::write(&blocked, b"").unwrap();
        let error = install(b"mod", &blocked.join("mod"), Kind::Ets2).unwrap_err().to_string();
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
        let patched = patch_gps_script(SCRIPT, "screen_generic", None, SCREEN_HEIGHT).unwrap();
        assert!(patched.contains(" my_children: 3\n my_children[0]: _nameless.3\n my_children[1]: _nameless.4\n my_children[2]: _nameless.westplay.screen\n"));
        assert!(patched.contains("ui::text : _nameless.westplay.screen {\n text: \"<img src=/material/ui/westplay/screen_generic.mat xscale=stretch yscale=stretch>\"\n coords_l: 0\n coords_r: 1279\n coords_t: 719\n coords_b: 0\n"));
        assert!(patched.contains(" layer: 100\n tab: -1\n pointer: -1\n my_parent: _nameless.2\n}\n\n}\n"));
        // The window still has its one child; the rest is as it was.
        assert!(patched.starts_with("SiiNunit\n{\nui::window : _nameless.1 {\n my_children: 1\n"));
    }

    /// A coach's GPS script as the Coaches DLC's (the MAN Lion's Coach's, shortened): the "screen on" group and the
    /// parking camera's beside it in the window, the camera a background, its picture and, in a group of their own,
    /// its marks.
    const CAMERA_SCRIPT: &str = "SiiNunit\n{\nui::window : _nameless.1 {\n my_children: 2\n my_children[0]: _nameless.on\n my_children[1]: _nameless.cam\n layer: 0\n}\n\nui::group : _nameless.on {\n coords_l: 0\n coords_r: 800\n coords_t: 480\n coords_b: 0\n id: 100\n my_children: 1\n my_children[0]: _nameless.map\n layer: 1\n my_parent: _nameless.1\n}\n\nui::text : _nameless.map {\n layer: 2\n my_parent: _nameless.on\n}\n\nui::group : _nameless.cam {\n coords_l: 0\n coords_r: 800\n coords_t: 480\n coords_b: 0\n id: 910\n my_children: 3\n my_children[0]: _nameless.bg\n my_children[1]: _nameless.pic\n my_children[2]: _nameless.marks\n layer: 1\n my_parent: _nameless.1\n}\n\nui::text : _nameless.bg {\n layer: 3\n my_parent: _nameless.cam\n}\n\nui::text : _nameless.pic {\n layer: 4\n my_parent: _nameless.cam\n}\n\nui::group : _nameless.marks {\n my_children: 1\n my_children[0]: _nameless.arrows\n layer: 0\n my_parent: _nameless.cam\n}\n\nui::text : _nameless.arrows {\n layer: 5\n my_parent: _nameless.marks\n}\n\n}\n";

    #[test]
    fn the_parking_camera_is_drawn_over_the_screen_and_the_screen_over_the_rest() {
        let patched = patch_gps_script(CAMERA_SCRIPT, "screen_man_lions_24", None, SCREEN_HEIGHT).unwrap();
        let layer = |name: &str| {
            let block = patched.split(&format!(" : {name} {{")).nth(1).unwrap_or_else(|| panic!("{name}"));
            let line = block.lines().find(|line| line.trim().starts_with("layer:")).unwrap();
            line.trim()["layer:".len()..].trim().parse::<u32>().unwrap()
        };
        let screen = layer("_nameless.westplay.screen");
        assert_eq!(screen, SCREEN_LAYER);
        // The game's map, and the groups it's in, under the screen as they were.
        assert_eq!((layer("_nameless.on"), layer("_nameless.map"), layer("_nameless.1")), (1, 2, 0));
        // The camera, at every depth, over it; in its own order.
        let camera =
            ["_nameless.cam", "_nameless.bg", "_nameless.pic", "_nameless.marks", "_nameless.arrows"].map(layer);
        assert!(camera.iter().all(|&layer| layer > screen), "{camera:?}");
        assert!(layer("_nameless.bg") < layer("_nameless.pic") && layer("_nameless.pic") < layer("_nameless.arrows"));
        // The screen still in the "screen on" group.
        assert!(patched.contains(" my_children[1]: _nameless.westplay.screen\n"));
        // The camera's mark in the camera's group (its last child), drawn only with it: transparent, in its corner.
        assert!(patched.contains(" my_children: 4\n my_children[0]: _nameless.bg\n"), "{patched}");
        assert!(patched.contains(" my_children[3]: _nameless.westplay.parking\n"));
        let mark = patched.split("ui::text : _nameless.westplay.parking {").nth(1).unwrap();
        assert!(mark.contains("<img src=/material/ui/westplay/parking_mark.mat"));
        assert!(mark.contains(" my_parent: _nameless.cam\n"));
        // No mark where there's no camera.
        let plain = patch_gps_script(SCRIPT, "screen_generic", None, SCREEN_HEIGHT).unwrap();
        assert!(!plain.contains("westplay.parking"));
        // Lifting changes the camera's five layers and nothing else.
        let lifted = lift_parking_camera(CAMERA_SCRIPT).unwrap();
        let changed: Vec<(&str, &str)> =
            CAMERA_SCRIPT.lines().zip(lifted.lines()).filter(|(before, after)| before != after).collect();
        assert_eq!(CAMERA_SCRIPT.lines().count(), lifted.lines().count());
        assert_eq!(changed.len(), 5, "{changed:?}");
        assert!(changed.iter().all(|(before, after)| before.starts_with(" layer:") && after.starts_with(" layer:")));
    }

    #[test]
    fn a_script_without_a_parking_camera_keeps_its_layers() {
        assert_eq!(lift_parking_camera(SCRIPT).unwrap(), SCRIPT);
        let crlf = CAMERA_SCRIPT.replace('\n', "\r\n");
        let lifted = lift_parking_camera(&crlf).unwrap();
        assert!(lifted.contains("ui::text : _nameless.pic {\r\n layer: 105\r\n"), "{lifted}");
        // A camera element without a layer, or with one that isn't a number: said, not guessed.
        assert!(lift_parking_camera(&CAMERA_SCRIPT.replace(" layer: 4\n", "")).is_err());
        assert!(lift_parking_camera(&CAMERA_SCRIPT.replace(" layer: 4\n", " layer: top\n")).is_err());
    }

    #[test]
    fn puts_the_screen_on_the_part_the_display_shows() {
        // The 7" Scania's: the stock square's full width, rows 12.2%-73.5% from its top.
        let square = SCRIPT.replace("coords_r: 1279", "coords_r: 256").replace("coords_t: 719", "coords_t: 256");
        let shown = SHOWN_PARTS.iter().find(|(id, _)| *id == "generic").map(|(_, part)| part);
        let patched = patch_gps_script(&square, "screen_generic", shown, SCREEN_HEIGHT).unwrap();
        let placed = "screen_generic.mat xscale=stretch yscale=stretch>\"\n coords_l: 1\n coords_r: 255\n coords_t: 225\n coords_b: 68\n";
        assert!(patched.contains(placed), "{patched}");
        // The group itself keeps its coordinates.
        assert!(patched.contains(" coords_r: 256\n coords_t: 256\n coords_b: 0\n id: 100\n"));
    }

    #[test]
    fn puts_the_frames_rows_on_the_screen_and_the_textures_extra_rows_below_it() {
        // The Volvo FH's: its texture 726 rows (its index 6), its display showing all but 0.54-0.55% of each edge.
        let script = SCRIPT.replace("coords_r: 1279", "coords_r: 1280").replace("coords_t: 719", "coords_t: 720");
        let shown = SHOWN_PARTS.iter().find(|(id, _)| *id == "volvo-fh").map(|(_, part)| part);
        let patched = patch_gps_script(&script, "screen_volvo_fh", shown, 726).unwrap();
        // The part shown: 7 to 1273 across, 716 to 4 up; the frame's 720 rows over that, so the texture's 726 reach
        // 6 rows' worth (5.9 units) further down, off the screen.
        let placed = "coords_l: 7\n coords_r: 1273\n coords_t: 716\n coords_b: -2\n";
        assert!(patched.contains(placed), "{patched}");
        // A texture the frame's size: on the part exactly.
        let patched = patch_gps_script(&script, "screen_volvo_fh", shown, SCREEN_HEIGHT).unwrap();
        assert!(patched.contains("coords_l: 7\n coords_r: 1273\n coords_t: 716\n coords_b: 4\n"), "{patched}");
    }

    #[test]
    fn the_s_bevs_screens_file_gets_the_sharp_size() {
        let root = std::env::temp_dir().join(format!("westplay-sharp-bev-{}", std::process::id()));
        let dir = root.join("def/vehicle/truck/scania.s_2024e/interior");
        std::fs::create_dir_all(&dir).unwrap();
        let files = GameFiles::new(vec![westplay_mapdata::files::Mount::Folder(root.clone())]);
        let path = SHARP_SCREENS[0];
        // Not in the game (its DLC not owned): nothing to make.
        assert_eq!(sharp_screens_file(&files, path), Ok(None));
        // As the game has it: the size line made the sharp one, the rest as it was.
        let stock = "\tdashboard_path: \"/ui/dashboard/scania_s_2024e.sii\"\r\n\r\n\tgps_path: \"/ui/gps.sii\"\r\n\tgps_drawable_size: (256, 256)\r\n";
        std::fs::write(root.join(path), stock).unwrap();
        let sharp = String::from_utf8(sharp_screens_file(&files, path).unwrap().unwrap()).unwrap();
        assert_eq!(sharp, stock.replace("(256, 256)", "(1024, 1024)"));
        // Another screen named: left as the game has it.
        std::fs::write(root.join(path), stock.replace("/ui/gps.sii", "/ui/dashboard/other_gps.sii")).unwrap();
        assert!(sharp_screens_file(&files, path).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_coaches_screens_files_draw_their_screens_at_the_volvo_fhs_size() {
        let root = std::env::temp_dir().join(format!("westplay-sharp-coaches-{}", std::process::id()));
        let files = GameFiles::new(vec![westplay_mapdata::files::Mount::Folder(root.clone())]);
        // As the Coaches DLC has them (2026-11-12): the Scania's GPS first, the MAN's after its dashboard.
        let scania = "	gps_path: \"/ui/dashboard/scania_touring_23_gps.sii\"
	gps_drawable_size: (512, 256)
                      	dashboard_path: \"/ui/dashboard/scania_touring_23.sii\"
	dashboard_drawable_size: (512, 512)
";
        let man = "	dashboard_path: \"/ui/dashboard/man_lions_24.sii\"
	dashboard_drawable_size: (1024, 512)
                   	gps_path: \"/ui/dashboard/man_lions_24_gps.sii\"
	gps_drawable_size: (512, 256)
";
        for (&(path, script), text) in BUS_SCREENS.iter().zip([scania, man]) {
            // Not in the game (the DLC not owned): nothing to make.
            assert_eq!(sized_screens_file(&files, path, script, BUS_SIZE), Ok(None));
            std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            std::fs::write(root.join(path), text).unwrap();
            // The GPS screen's size made the Volvo FH's, the dashboard's as it was.
            let sized = sized_screens_file(&files, path, script, BUS_SIZE).unwrap().unwrap();
            let sized = String::from_utf8(sized).unwrap();
            let gps = format!("gps_path: \"/{script}\"");
            assert_eq!(sized, text.replace("gps_drawable_size: (512, 256)", "gps_drawable_size: (1024, 512)"));
            assert!(sized.contains(&gps));
            // Another screen named: left as the game has it.
            std::fs::write(root.join(path), text.replace(script, "ui/dashboard/other_gps.sii")).unwrap();
            assert!(sized_screens_file(&files, path, script, BUS_SIZE).is_err());
        }
        // Each a layout's: the coaches' screens WestPlay's.
        for (_, script) in BUS_SCREENS {
            assert!(screen_layouts().iter().any(|layout| layout.gps_scripts.contains(script)), "{script}");
        }
        std::fs::remove_dir_all(&root).unwrap();
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
        let (bytes, built) = build(&files, &[], &GameFiles::new(Vec::new()), &cab_mark::Marks::default()).unwrap();
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
    fn marks_a_dashboard_in_its_windows_corner_once() {
        let script = "SiiNunit\r\n{\r\nui::window : _nameless.w {\r\n my_children: 0\r\n coords_l: 10\r\n coords_r: 500\r\n coords_t: 0\r\n coords_b: 300\r\n my_parent: null\r\n}\r\n\r\n}\r\n";
        let patched = patch_dashboard_script(script, "cab_mark").unwrap();
        assert!(patched.contains(" my_children: 1\r\n my_children[0]: _nameless.westplay.cab\r\n"), "{patched}");
        // Its coordinates going down the screen: up from the bottom corner, 1/64 of the window each way (rounded up).
        assert!(
            patched.contains(" coords_l: 10\r\n coords_r: 18\r\n coords_t: 295\r\n coords_b: 300\r\n"),
            "{patched}"
        );
        assert!(
            patched.ends_with("my_parent: _nameless.w\r\n}\r\n\r\n}\r\n") && !patched.contains("\r\r"),
            "{patched}"
        );
        assert!(patch_dashboard_script(&patched, "cab_mark").is_err());
        // Two windows: which is the dashboard's isn't known.
        let two = script.replace("\r\n\r\n}", "\r\nui::window : _nameless.v {\r\n my_parent: null\r\n}\r\n}");
        assert!(patch_dashboard_script(&two, "cab_mark").is_err());
    }

    /// A newer dashboard's displays, groups over the whole window: the mark in each of them too, and some pixels big.
    #[test]
    fn marks_each_display_of_a_dashboard_that_has_them() {
        let script = "SiiNunit\n{\nui::window : _nameless.wnd {\n my_children: 2\n my_children[0]: _nameless.d1\n my_children[1]: _nameless.small\n coords_l: 0\n coords_r: 1440\n coords_t: 540\n coords_b: 0\n my_parent: null\n}\n\nui::group : _nameless.d1 {\n my_children: 0\n coords_l: 0\n coords_r: 1440\n coords_t: 540\n coords_b: 0\n my_parent: _nameless.wnd\n}\n\nui::group : _nameless.small {\n my_children: 0\n coords_l: 0\n coords_r: 100\n coords_t: 540\n coords_b: 0\n my_parent: _nameless.wnd\n}\n\n}\n";
        let patched = patch_dashboard_script(script, "cab_mark").unwrap();
        assert!(patched.contains("ui::group : _nameless.d1 {\n my_children: 1\n my_children[0]: _nameless.westplay.cab0\n"), "{patched}");
        assert!(patched.contains("my_parent: _nameless.d1\n"), "{patched}");
        // Not in a group that covers only part of the window.
        assert!(!patched.contains("my_parent: _nameless.small\n}\n\nui::text"), "{patched}");
        assert_eq!(patched.matches("<img src=/material/ui/westplay/cab_mark.mat").count(), 2, "{patched}");
        // 1/64 of 1440 by 540, rounded up: 23 by 9 units, from the bottom left corner up.
        assert!(patched.contains(" coords_l: 0\n coords_r: 23\n coords_t: 9\n coords_b: 0\n"), "{patched}");
    }

    #[test]
    fn keeps_a_scripts_windows_line_endings() {
        let patched = patch_gps_script(&SCRIPT.replace('\n', "\r\n"), "screen_generic", None, SCREEN_HEIGHT).unwrap();
        assert!(patched.contains(" my_children[2]: _nameless.westplay.screen\r\n") && !patched.contains("\r\r"));
    }

    #[test]
    fn refuses_a_script_without_a_screen_on_group() {
        assert_eq!(
            patch_gps_script(&SCRIPT.replace("id: 100", "id: 7"), "x", None, SCREEN_HEIGHT),
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
        let game = crate::game::find(crate::game::Kind::Ets2).into_iter().next().expect("the game installed");
        let files = GameFiles::install(&game.dir).unwrap();
        let (bytes, built) = build(&files, &[], &GameFiles::new(Vec::new()), &cab_mark::Marks::default()).unwrap();
        let out = std::env::temp_dir().join(file_name(game.kind));
        std::fs::write(&out, &bytes).unwrap();
        println!("{} bytes, patched {:?}, missing {:?}: {}", bytes.len(), built.patched, built.missing, out.display());
        let archive = westplay_mapdata::archive::Archive::open(&out).unwrap();
        for layout in screen_layouts().iter().filter(|layout| built.patched.contains(&layout.id)) {
            for script in layout.gps_scripts.iter().filter_map(|script| archive.read(script).unwrap()) {
                let script = String::from_utf8(script).unwrap();
                assert!(script.contains("_nameless.westplay.screen"), "{}", layout.id);
            }
        }
        // The tablet, in the game's trucks.
        assert!(built.patched.contains(&TABLET));
        let actros = "def/vehicle/truck/mercedes.actros2014/accessory/toyhang/westplay_tablet.sii";
        assert!(archive.read(actros).unwrap().is_some());
        assert!(archive.read("vehicle/truck/upgrade/westplay_tablet/westplay_tablet_hang.pmg").unwrap().is_some());
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
        let (bytes, built) = build(&files, &[], &GameFiles::new(Vec::new()), &cab_mark::Marks::default()).unwrap();
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
        let (bytes, built) = build(&files, &[], &GameFiles::new(Vec::new()), &cab_mark::Marks::default()).unwrap();
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

    /// A bus mod's folder: its GPS script (`script`, a 256 x 256 screen) and its interiors (`interiors`: each its
    /// file's name and text).
    fn bus_mod(root: &Path, truck: &str, script: &str, interiors: &[(&str, String)]) -> PathBuf {
        let screen = "SiiNunit\n{\nui::window : _nameless.1 {\n my_children: 1\n my_children[0]: _nameless.2\n}\n\nui::group : _nameless.2 {\n coords_l: 0\n coords_r: 256\n coords_t: 256\n coords_b: 0\n id: 100\n my_children: 1\n my_children[0]: _nameless.3\n my_parent: _nameless.1\n}\n\nui::text : _nameless.3 {\n my_parent: _nameless.2\n}\n\n}\n";
        let folder = root.join(truck);
        let dir = folder.join(format!("def/vehicle/truck/{truck}/interior"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(folder.join(script).parent().unwrap()).unwrap();
        std::fs::write(folder.join(script), screen).unwrap();
        for (name, text) in interiors {
            std::fs::write(dir.join(name), text).unwrap();
        }
        folder
    }

    /// An interior's definition naming `gps` as its screen (as the coaches' are written), its model `model`.
    fn bus_interior(unit: &str, model: &str, gps: &str, size: Option<&str>) -> String {
        let size = size.map(|size| format!("\r\n\tgps_drawable_size: {size}")).unwrap_or_default();
        format!(
            "SiiNunit\r\n{{\r\naccessory_interior_data : {unit}\r\n{{\r\n\tmodel: \"{model}\"\r\n\tdashboard_path: \"/ui/dashboard/d.sii\"\r\n\tgps_path:\"{gps}\"{size}\r\n}}\r\n}}\r\n"
        )
    }

    #[test]
    fn the_coaches_get_screens_of_their_own_on_the_part_their_displays_show() {
        let root = std::env::temp_dir().join(format!("westplay-mod-buses-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let t16 = "/kazanciyan/newt16shd/interior";
        let travego = bus_mod(
            &root,
            "t16shd",
            "ui/dashboard/t16_gps.sii",
            &[
                (
                    "standard.sii",
                    bus_interior("std.t16shd.interior", &format!("{t16}/model.pmd"), "/ui/dashboard/t16_gps.sii", None),
                ),
                (
                    "stdo.sii",
                    bus_interior(
                        "stdo.t16shd.interior",
                        &format!("{t16}/modelo.pmd"),
                        "/ui/dashboard/t16_gps.sii",
                        None,
                    ),
                ),
                (
                    "std23.sii",
                    bus_interior(
                        "std23.t16shd.interior",
                        &format!("{t16}/model23.pmd"),
                        "/ui/dashboard/t16_gps.sii",
                        None,
                    ),
                ),
                // Another screen: not WestPlay's.
                ("other.sii", bus_interior("oth.t16shd.interior", &format!("{t16}/model.pmd"), "/ui/gps.sii", None)),
            ],
        );
        // The 531's script by the Actros's name, its interiors' screen at 512 x 512.
        let setra = bus_mod(
            &root,
            "setradt531",
            "ui/dashboard/actros_gps.sii",
            &[(
                "standard.sii",
                bus_interior(
                    "std.setradt531.interior",
                    "/a/model.pmd",
                    "/ui/dashboard/actros_gps.sii",
                    Some("(512, 512)"),
                ),
            )],
        );
        let screen_mods = mod_screen_mods(&[travego.clone(), root.join("gone.scs"), setra.clone()]);
        let (bytes, built) =
            build(&GameFiles::new(Vec::new()), &[], &screen_mods, &cab_mark::Marks::default()).unwrap();
        for layout in ["travego-16", "travego-16-23", "setra-531"] {
            assert!(built.patched.contains(&layout), "{layout}: {built:?}");
        }
        // The others' mods aren't installed: their screens left out.
        // The Setra S 516 HD/2's screen is the game's toy GPS's, made whatever mods are installed.
        assert!(built.missing.contains(&"man-lions-coach") && built.patched.contains(&"setra-516"));
        let out = root.join("westplay.scs");
        std::fs::write(&out, &bytes).unwrap();
        let archive = Archive::open(&out).unwrap();
        let text = |path: &str| archive.read(path).unwrap().map(|bytes| String::from_utf8(bytes).unwrap());
        // Each layout's own script, WestPlay's image on the band its display shows (13.5%-75.6% from the top of
        // 256 rows, which count upwards): the frame's 720 rows from 221.4 to 62.5, its texture's 728 (720 and the
        // layout's index, 8) reaching 1.8 lower.
        let script = text("ui/dashboard/westplay_travego_16_gps.sii").unwrap();
        assert!(script.contains("/material/ui/westplay/screen_travego_16.mat"), "{script}");
        assert!(script.contains(" coords_t: 221\n coords_b: 61\n"), "{script}");
        // 13.4%-86.6%, 729 rows: the frame's to 34.3, the texture's to 32.0.
        let script = text("ui/dashboard/westplay_travego_16_23_gps.sii").unwrap();
        assert!(script.contains(" coords_t: 222\n coords_b: 32\n"), "{script}");
        // Each interior naming WestPlay's script for its display, drawn at 1024 x 1024; the other left as it is.
        let dir = "def/vehicle/truck/t16shd/interior";
        let standard = text(&format!("{dir}/standard.sii")).unwrap();
        assert!(
            standard.contains(
                "\tgps_path: \"/ui/dashboard/westplay_travego_16_gps.sii\"\r\n\tgps_drawable_size: (1024, 1024)\r\n"
            ),
            "{standard}"
        );
        assert!(standard.contains("accessory_interior_data : std.t16shd.interior\r\n"));
        assert!(text(&format!("{dir}/stdo.sii")).unwrap().contains("westplay_travego_16_gps.sii"));
        assert!(text(&format!("{dir}/std23.sii")).unwrap().contains("westplay_travego_16_23_gps.sii"));
        assert_eq!(text(&format!("{dir}/other.sii")), None);
        // Its size replaced, not added twice.
        let setra_std = text("def/vehicle/truck/setradt531/interior/standard.sii").unwrap();
        assert_eq!(setra_std.matches("gps_drawable_size").count(), 1);
        assert!(
            setra_std.contains("gps_drawable_size: (1024, 1024)") && setra_std.contains("westplay_setra_531_gps.sii")
        );
        // Nothing of the coaches' own scripts: WestPlay's names its own.
        assert_eq!(text("ui/dashboard/actros_gps.sii"), None);
        drop(archive);

        // The built mod is as the installed coaches want it, until one changes or goes.
        assert!(mod_screens_current(&screen_mods, &out));
        let interior = travego.join(format!("{dir}/stdo.sii"));
        // Its dashboard changed in a new version of the mod (a size line alone wouldn't change WestPlay's copy, which
        // sets its own).
        let changed =
            std::fs::read_to_string(&interior).unwrap().replace("/ui/dashboard/d.sii", "/ui/dashboard/d2.sii");
        std::fs::write(&interior, changed).unwrap();
        assert!(!mod_screens_current(&mod_screen_mods(&[travego.clone(), setra.clone()]), &out));
        assert!(!mod_screens_current(&mod_screen_mods(&[travego]), &out));
        assert!(!mod_screens_current(&screen_mods, &root.join("none.scs")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_setra_516s_screen_is_the_games_toy_gps_its_tablet_shows_the_mod_untouched() {
        let root = std::env::temp_dir().join(format!("westplay-mod-setra-toy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // The game, with the toy GPS's script (Cabin Accessories): its window 300 x 168.
        let game = root.join("game");
        std::fs::create_dir_all(game.join("ui/dashboard")).unwrap();
        let toy = "SiiNunit\n{\nui::window : _nameless.wnd {\n my_children: 1\n my_children[0]: _nameless._.map\n coords_l: 0\n coords_r: 300\n coords_t: 168\n coords_b: 0\n my_parent: null\n}\n\nui::text : _nameless._.map {\n my_parent: _nameless.wnd\n}\n\n}\n";
        std::fs::write(game.join("ui/dashboard/gps.dlc_toys.sii"), toy).unwrap();
        let with_toy = GameFiles::new(vec![westplay_mapdata::files::Mount::Folder(game.clone())]);
        let script = "ui/dashboard/gps.dlc_toys.sii";
        let built_script = |game: &GameFiles| {
            let (bytes, built) = build(game, &[], &GameFiles::new(Vec::new()), &cab_mark::Marks::default()).unwrap();
            assert!(built.patched.contains(&"setra-516"), "{built:?}");
            let out = root.join("westplay.scs");
            std::fs::write(&out, bytes).unwrap();
            let archive = westplay_mapdata::archive::Archive::open(&out).unwrap();
            // Nothing of the bus's mod: no part of it, no interior.
            assert_eq!(archive.read("def/vehicle/truck/setra516hd/accessory/knsgps/1.sii").unwrap(), None);
            assert_eq!(archive.read("def/vehicle/truck/setra516hd/interior/standard.sii").unwrap(), None);
            String::from_utf8(archive.read(script).unwrap().unwrap()).unwrap()
        };
        // WestPlay's image over the part of the toy GPS's window the tablet's glass shows (2.14%-97.86% each way), on
        // top of its map: the frame's rows over it to 3.6, its texture's 730 to 1.4, each edge a unit beyond.
        let patched = built_script(&with_toy);
        assert!(
            patched.contains(
                " my_children: 2\n my_children[0]: _nameless._.map\n my_children[1]: _nameless.westplay.screen\n"
            ),
            "{patched}"
        );
        assert!(patched.contains("screen_setra_516.mat"), "{patched}");
        assert!(patched.contains(" coords_l: 5\n coords_r: 295\n coords_t: 166\n coords_b: 0\n"), "{patched}");
        // A player without Cabin Accessories: a window of WestPlay's at its path, its image alone (the texture's rows
        // to 8.1).
        let own = built_script(&GameFiles::new(Vec::new()));
        assert!(own.contains(" coords_l: 20\n coords_r: 980\n coords_t: 980\n coords_b: 7\n"), "{own}");
        // The Setra is one of the vehicles WestPlay draws on (no tablet of WestPlay's in it).
        assert!(mod_screen_trucks().any(|truck| truck == "setra516hd"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_coachs_interior_not_as_read_leaves_its_screen_out() {
        let root = std::env::temp_dir().join(format!("westplay-mod-bus-unlike-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // Two gps_path lines: not as read.
        let twice = bus_interior("std.manlions.interior", "/ob/m.pmd", "/ui/gps/man_e6_gps.sii", None)
            .replace("\tdashboard_path", "\tgps_path: \"/ui/gps/man_e6_gps.sii\"\r\n\tdashboard_path");
        let lions = bus_mod(&root, "manlions", "ui/gps/man_e6_gps.sii", &[("standard.sii", twice)]);
        let (bytes, built) =
            build(&GameFiles::new(Vec::new()), &[], &mod_screen_mods(&[lions]), &cab_mark::Marks::default()).unwrap();
        assert!(built.missing.contains(&"man-lions-coach"), "{built:?}");
        let out = root.join("westplay.scs");
        std::fs::write(&out, &bytes).unwrap();
        let archive = Archive::open(&out).unwrap();
        assert_eq!(archive.read("ui/dashboard/westplay_man_lions_coach_gps.sii").unwrap(), None);
        assert_eq!(archive.read("def/vehicle/truck/manlions/interior/standard.sii").unwrap(), None);
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
