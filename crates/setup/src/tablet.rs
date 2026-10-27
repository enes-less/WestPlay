//! WestPlay's tablet: a toy whose screen shows WestPlay in trucks with no screen of WestPlay's (owner
//! 2026-10-24: "any truck with the tablet accessory"), bought in the truck shop as any toy is, in two places
//! (owner 2026-10-24, not the console's toy stand: "no truck has place for a tablet there"): Hanging Items
//! (`toyhang`), fixed where the truck's hanging toys hang from, and a dashboard set of its own (`set_dashbrd`),
//! standing where a set's toys stand ([`Slot`]).
//!
//! Drawn only with the camera in the cab, as the dashboard's screens are: WestPlay's camera system (the host's
//! camera.rs) takes its screen not being drawn for the camera outside the cab (muffling the music) and the game's
//! menus up (fading it out), and a tablet drawn from outside (through the windows, behind the service screen)
//! kept the music as it was (owner 2026-10-24). So both are defined as the game's parts with a view of their own
//! for outside are (`accessory_addon_data`: the driver's plate, `0.pmd` outside and `0_int.pmd` in): an
//! `interior_model` only, no `exterior_model`, as the game's own dashboard sets and steering wheels are. (The
//! hanging toys' kind, `accessory_addon_int_data`, has its model drawn from outside too, as the first tablet was.)
//!
//! An iPad Air 5 (owner 2026-10-27), Art_bor's model (`assets/tablet`, CC BY 4.0; made into the game's model by
//! `tools/dev/tablet_model.py`): landscape, at its true size, its front facing the model's -z as a toy's does. Its
//! origin moved to the middle of the tablet's base (standing on a dashboard spot) or of its top (hanging), and its
//! glass showing the `tablet` layout's screen texture (`westplay_shared::TABLET`) instead of the game's GPS screen:
//! the truck's own screen keeps the game's navigation, and the plugin knows the tablet by its texture. The glass
//! shows the texture's first SCREEN_HEIGHT rows, the frame whole (the rows past it, which tell the layouts apart,
//! aren't shown), its first row at the top: an ordinary texture's rows run the other way from the GPS screen's
//! render target, which the game flips (the first tablet's, Kedi739's, mapped as made, showed the frame upside
//! down: owner 2026-10-24). Its descriptor is laid out as Kedi739's was (`Kedi739.pmd`, his, kept as the pattern).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use westplay_mapdata::archive::Archive;
use westplay_mapdata::buildings;
use westplay_mapdata::cityhash::city_hash64;
use westplay_mapdata::files::{GameFiles, Mount};
use westplay_mapdata::save;
use westplay_mapdata::token::string_to_token;
use westplay_shared::SCREEN_HEIGHT;

/// The iPad's model (`tools/dev/tablet_model.py`) and its body's colour texture; Kedi739's descriptor, the pattern
/// the tablet's is laid out as.
const SOURCE_PMG: &[u8] = include_bytes!("../../../assets/tablet/ipad.pmg");
const SOURCE_PMD: &[u8] = include_bytes!("../../../assets/tablet/Kedi739.pmd");
const SOURCE_DDS: &[u8] = include_bytes!("../../../assets/tablet/ipad.dds");
/// The shop's icon: WestPlay's own (256 x 256).
const ICON_PNG: &[u8] = include_bytes!("../../../app/icons/128x128@2x.png");

/// Where the tablet's files live in the game's file system.
const DIR: &str = "vehicle/truck/upgrade/westplay_tablet";
/// Its icon (`icon: "westplay_tablet"`: the shop's icons are under `ICON_DIR`).
const ICON_DIR: &str = "material/ui/accessory";
const NAME: &str = "westplay_tablet";
/// The hanging one's model.
const HANGING: &str = "westplay_tablet_hang";
/// The one on a dashboard set's toy spot (a table's red slots: owner 2026-10-26).
const ON_SLOT: &str = "westplay_tablet_slot";
/// Its hookup, beside the game's (`def/vehicle/addon_hookup_storage.sii` asks mods for a file of their own so named).
const HOOKUPS: &str = "def/vehicle/addon_hookup_storage.westplay.sii";
/// Its accessory's name, the first of its unit name's parts: each a token, 12 characters at the most (the game
/// refused `westplay_tablet.scania.s_2016.toystand`: "the unit name ... is in wrong format", 2026-10-24).
const UNIT: &str = "wp_tablet";

/// Where a truck takes the tablet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    /// Its hanging items, the tablet fixed where they hang from.
    Hanging,
    /// Its dashboard sets, the tablet a set of its own, standing where a set's toys stand (a set's spots are a few
    /// centimetres either side of where it's fitted: the DAF XF Euro 6's 14 cm).
    DashboardSet,
}

impl Slot {
    const ALL: [Self; 2] = [Self::Hanging, Self::DashboardSet];

    /// Its folder among a truck's accessories, its definitions' units' last part.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Hanging => "toyhang",
            Self::DashboardSet => "set_dashbrd",
        }
    }
}

/// A truck's slot the tablet is fitted at.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Spot {
    /// The truck's id (`daf.xf`).
    pub truck: String,
    pub slot: Slot,
}

impl Spot {
    pub fn new(truck: &str, slot: Slot) -> Self {
        Self { truck: truck.to_owned(), slot }
    }
}

/// How far the tablet turns toward the driver (owner 2026-10-24: "maybe 20 degrees"), about its upright: its
/// front toward the model's +x. Every truck's hanging point faces a toy's front (-z) straight back into the cab,
/// from the cab's middle (read from the game's interior models, 2026-10-24), and a left-hand drive cab's driver
/// sits to the -x of its middle: the model's +x there.
const TOWARD_DRIVER: f32 = 20.0;
/// How far the tablet turns standing as a dashboard set of its own: a set's place in the cab (`set_dashbrd`) isn't
/// turned (its front the windscreen's way, in every truck read, the game's and the mods', 2026-10-26), its sets
/// turning each toy's spot round to face back into the cab, so the tablet turns round itself, then toward the
/// driver. (It stood facing out of the windscreen: owner 2026-10-26.)
const ON_A_SET: f32 = 180.0 + TOWARD_DRIVER;
/// On its stand: leaning back (degrees), its bottom edge held this high above the stand's underside and this far
/// forward of the spot (metres), in front of the stand's arm, which meets the tablet's back this share of the way
/// up it.
const LEAN_BACK: f32 = 15.0;
const STAND_LIFT: f32 = 0.012;
const STAND_SEAT: f32 = -0.02;
const ARM_UP: f32 = 0.38;
/// Hanging (owner 2026-10-26: "attached to the top dashboard with a bar"; 2026-10-27, the bar "too long, and looks
/// like it's going to break any day": a short sturdy arm, the tablet held): the tablet's top this far below the
/// hanging point, held by a clamp over the middle of its top edge, on an arm from this far above the point (up
/// in the trim it's under: 0 to 2.3 cm in the game's trucks). Metres.
const HANG_DROP: f32 = 0.018;
const BAR_UP: f32 = 0.03;
/// The arm: its width and depth, and the wider foot it stands on the clamp with (its width and height).
const ARM_WIDTH: f32 = 0.024;
const ARM_DEPTH: f32 = 0.012;
const FOOT_WIDTH: f32 = 0.04;
const FOOT_HEIGHT: f32 = 0.006;
/// The clamp: as wide as this along the top edge, its walls this thick, this far off the tablet (so their faces
/// don't meet the tablet's), its lip over the front's black glass and down the back this deep.
const CLAMP_WIDTH: f32 = 0.10;
const CLAMP_WALL: f32 = 0.0025;
const CLAMP_GAP: f32 = 0.0003;
const CLAMP_FRONT: f32 = 0.005;
const CLAMP_BACK: f32 = 0.025;
/// How far behind the glass the camera's bump starts: the back left out as the clamp's back lip is fitted.
const CAMERA_FROM: f32 = 0.007;

/// Where the model's origin is: where it's fixed.
#[derive(Clone, Copy)]
enum Origin {
    /// The middle of its base, standing; how far it turns about its upright (its front, -z, toward +x): on a
    /// dashboard set of its own, [`ON_A_SET`]; on a set's toy spot, which faces back into the cab already (165 to
    /// 180 degrees in the game's sets), [`TOWARD_DRIVER`].
    Base(f32),
    /// Its bar's foot, where hanging toys hang from, the tablet hanging below it; the hanging point's pitch
    /// (degrees, its front up), undone so the tablet hangs upright and its top edge level in the cab (owner
    /// 2026-10-26: a corner was higher than the other, the truck's pitch tilting the tablet turned toward the
    /// driver).
    Top(f32),
}

/// A hanging tablet's turn: toward the driver about its upright, then the hanging point's pitch (`level`)
/// undone (the game turns the model by the point's: 180 degrees about the upright, and that pitch).
fn hanging_turn(p: [f32; 3], level: f32) -> [f32; 3] {
    rotate_x(rotate_y(p, TOWARD_DRIVER), -level)
}

/// The source model's pieces: the tablet's body and its glass.
const BODY: usize = 0;
const SCREEN: usize = 1;

/// A `.pmg` (version 0x15) as the source is laid out (as Kedi739's was): its header's counts and offsets
/// (words), each piece's record (100 bytes) and its fields (words: its index and vertex counts, its material, its bounds (a
/// sphere's centre and radius, a box's least and greatest corners), its vertices' stride and where their
/// position, normal, colour and texture coordinate are, where its indices are).
const PMG_MAGIC: u32 = 0x506d_6715;
const PIECES: usize = 1;
const PARTS: usize = 2;
const BOUNDS: usize = 8;
const PARTS_AT: usize = 19;
const LOCATORS_AT: usize = 20;
const PIECES_AT: usize = 21;
const STRINGS_AT: usize = 22;
const VERTICES_AT: usize = 24;
const INDICES_AT: usize = 26;
const HEADER_SIZE: usize = 0x70;
const PART_SIZE: usize = 24;
const PIECE_SIZE: usize = 100;
const PIECE_INDICES: usize = 0;
const PIECE_VERTICES: usize = 1;
const PIECE_MATERIAL: usize = 4;
const PIECE_BOUNDS: usize = 5;
const PIECE_STRIDE: usize = 15;
const PIECE_POSITIONS: usize = 16;
const PIECE_NORMALS: usize = 17;
const PIECE_UVS: usize = 18;
const PIECE_COLOURS: usize = 19;
const PIECE_INDEX_AT: usize = 24;

/// The tablet's files, for `spots` (the trucks' slots for it: [`trucks`], the game's and its mods'): its models
/// (standing, and hanging for each truck's hanging point's pitch in `pitches`: [`hanging_pitches`], none for a
/// truck not in it), materials and texture, its icon, and each spot's accessory. `screen_texture` is the `tablet`
/// layout's texture object (a game path), `texture_height` its texture's height.
pub fn files(
    spots: &[Spot],
    pitches: &BTreeMap<String, f32>,
    screen_texture: &str,
    texture_height: u32,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    if spots.is_empty() {
        return Err("no truck with hanging items or dashboard sets".into());
    }
    let tenths = |truck: &str| pitches.get(truck).map_or(0, |pitch| (pitch * 10.0).round() as i32);
    let hanging: BTreeSet<i32> =
        spots.iter().filter(|spot| spot.slot == Slot::Hanging).map(|spot| tenths(&spot.truck)).collect();
    let mut files = vec![
        (format!("{DIR}/{NAME}.pmg"), model(texture_height, Origin::Base(ON_A_SET))?),
        (format!("{DIR}/{NAME}.pmd"), descriptor()?),
        (format!("{DIR}/{ON_SLOT}.pmg"), model(texture_height, Origin::Base(TOWARD_DRIVER))?),
        (format!("{DIR}/{ON_SLOT}.pmd"), descriptor()?),
        (HOOKUPS.to_owned(), hookup().into_bytes()),
        (format!("{DIR}/body.mat"), body_material().into_bytes()),
        (format!("{DIR}/screen.mat"), screen_material(screen_texture).into_bytes()),
        (format!("{DIR}/{NAME}.tobj"), crate::game_mod::tobj(&format!("/{DIR}/{NAME}.dds"))),
        (format!("{DIR}/{NAME}.dds"), SOURCE_DDS.to_vec()),
        (format!("{ICON_DIR}/{NAME}.mat"), crate::game_mod::material(NAME).into_bytes()),
        (format!("{ICON_DIR}/{NAME}.tobj"), crate::game_mod::tobj(&format!("/{ICON_DIR}/{NAME}.dds"))),
        (format!("{ICON_DIR}/{NAME}.dds"), icon()?),
    ];
    for &pitch in &hanging {
        let name = hanging_model(pitch);
        files.push((format!("{DIR}/{name}.pmg"), model(texture_height, Origin::Top(pitch as f32 / 10.0))?));
        files.push((format!("{DIR}/{name}.pmd"), descriptor()?));
    }
    for spot in spots.iter().filter(|spot| !vehicle_apart(&spot.truck) && !own_screen(&spot.truck)) {
        let model = match spot.slot {
            Slot::Hanging => hanging_model(tenths(&spot.truck)),
            Slot::DashboardSet => NAME.to_owned(),
        };
        files.push((definition_path(spot), accessory(spot, &model).into_bytes()));
    }
    Ok(files)
}

/// The hanging tablet's model for a hanging point pitched `tenths` of a degree: `westplay_tablet_hang`, or with
/// the pitch (`_p80`: 8 degrees up, `_m34`: 3.4 down).
fn hanging_model(tenths: i32) -> String {
    match tenths {
        0 => HANGING.to_owned(),
        up if up > 0 => format!("{HANGING}_p{up}"),
        down => format!("{HANGING}_m{}", -down),
    }
}

/// Each of `spots`' hanging trucks' hanging point's pitch (degrees, its front up), read from `files` (the game):
/// the `toyhang` locator of its interiors' models (the pitch most of them have; the game's trucks' interiors
/// all agree). A truck whose interiors `files` hasn't, or none of which has the locator, isn't in it.
pub fn hanging_pitches(files: &GameFiles, spots: &[Spot]) -> BTreeMap<String, f32> {
    let mut pitches = BTreeMap::new();
    for spot in spots.iter().filter(|spot| spot.slot == Slot::Hanging) {
        let dir = format!("def/vehicle/truck/{}/interior", spot.truck);
        let models: BTreeSet<String> = files
            .list(&dir)
            .unwrap_or_default()
            .iter()
            .filter(|name| name.ends_with(".sii"))
            .filter_map(|name| files.read(&format!("{dir}/{name}")).ok().flatten())
            .flat_map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .lines()
                    .filter_map(|line| {
                        line.trim().strip_prefix("model:").map(|rest| rest.trim().trim_matches('"').to_owned())
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut counts: BTreeMap<i32, usize> = BTreeMap::new();
        for model in &models {
            let path = model.trim_start_matches('/').replace(".pmd", ".pmg");
            let Some(pmg) = files.read(&path).ok().flatten() else { continue };
            let Some(locators) = buildings::model_locators(&pmg) else { continue };
            if let Some(hang) = locators.iter().find(|locator| locator.name == Slot::Hanging.folder()) {
                *counts.entry((pitch_of(hang.rotation) * 10.0).round() as i32).or_default() += 1;
            }
        }
        if let Some((&tenths, _)) = counts.iter().max_by_key(|(_, count)| **count) {
            pitches.insert(spot.truck.clone(), tenths as f32 / 10.0);
        }
    }
    pitches
}

/// How far a locator's rotation (a quaternion, w first) tips a toy's front (-z) up, in degrees.
fn pitch_of([w, x, y, z]: [f32; 4]) -> f32 {
    // The front's height: the rotation's matrix's second row, third column (where it takes +z up), negated.
    let up = -(2.0 * (y * z - w * x));
    up.clamp(-1.0, 1.0).asin().to_degrees()
}

/// Whether WestPlay draws on `truck`'s own screen (a mod vehicle's: [`crate::game_mod::mod_screen_trucks`]): no
/// tablet there.
fn own_screen(truck: &str) -> bool {
    crate::game_mod::mod_screen_trucks().any(|own| own == truck)
}

/// Where a spot's accessory is defined.
fn definition_path(spot: &Spot) -> String {
    format!("def/vehicle/truck/{}/accessory/{}/{NAME}.sii", spot.truck, spot.slot.folder())
}

/// The trucks' slots for the tablet in `files` (the game, its DLCs, a mod), sorted: each truck's hanging items
/// and dashboard sets it has, of those whose name the tablet's own can be made of (each part a token: a mod's
/// folder named otherwise is no truck the game has). Not the vehicles WestPlay draws on the own screen of
/// (`game_mod::mod_screen_trucks`: Oyuncuyusbis' coaches; owner 2026-10-24: "Tablets should NOT appear in buses").
pub fn trucks(files: &GameFiles) -> Result<Vec<Spot>, String> {
    let mut spots = Vec::new();
    for truck in files.dirs("def/vehicle/truck").map_err(|error| error.to_string())? {
        if !truck.split('.').all(|part| string_to_token(part).is_some()) || own_screen(&truck) {
            continue;
        }
        let slots = files.dirs(&format!("def/vehicle/truck/{truck}/accessory")).map_err(|error| error.to_string())?;
        for slot in Slot::ALL {
            if slots.iter().any(|folder| folder == slot.folder()) {
                spots.push(Spot::new(&truck, slot));
            }
        }
    }
    Ok(spots)
}

/// The trucks' slots for the tablet in any of `mods` (each an archive or a folder: the installed mods, active in
/// a profile or not; owner 2026-10-24: "any truck, of any mod"), sorted, each once. A mod that can't be opened,
/// or whose folders can't be listed (protected), gives none; nor does a mod whose vehicles the tablet isn't for
/// ([`set_apart`]), and no vehicle such a mod defines gets it from another mod listing it too.
pub fn mod_trucks(mods: &[PathBuf]) -> Vec<Spot> {
    mod_trucks_and_marks(mods).0
}

/// [`mod_trucks`], and the marks of the vehicles the tablet shows nothing in ([`apart_marks`]), from one look at
/// `mods`.
pub fn mod_trucks_and_marks(mods: &[PathBuf]) -> (Vec<Spot>, Vec<u64>) {
    let (mut found, mut apart) = (BTreeSet::new(), BTreeSet::new());
    for files in mods {
        let mount = if files.is_dir() {
            Mount::Folder(files.clone())
        } else {
            match Archive::open(files) {
                Ok(archive) => Mount::Archive(archive),
                Err(_) => continue,
            }
        };
        let mounted = GameFiles::new(vec![mount]);
        if set_apart(files, &mounted) {
            // Every vehicle it names, its hanging items listed or not.
            apart.extend(mounted.dirs("def/vehicle/truck").unwrap_or_default());
        } else {
            found.extend(trucks(&mounted).unwrap_or_default());
        }
    }
    let spots = found.into_iter().filter(|spot: &Spot| !apart.contains(&spot.truck)).collect();
    (spots, apart_marks(&apart))
}

/// The marks of the vehicles WestPlay's tablet shows nothing in (`westplay_shared::TABLET_APART_FILE`): those of
/// [`APART_VEHICLES`], and every vehicle of the mods set apart (`apart`, their ids), by their ids' hashes: the
/// tablet, a hookup on any vehicle's dashboard set too, can't be kept out of them, so the plugin keeps it blank.
fn apart_marks(apart: &BTreeSet<String>) -> Vec<u64> {
    let mut marks: BTreeSet<u64> = APART_VEHICLES.iter().copied().collect();
    marks.extend(apart.iter().map(|id| city_hash64(id.as_bytes())));
    marks.into_iter().collect()
}

/// Writes `marks` ([`mod_trucks_and_marks`]) into WestPlay's own folder `dir`, beside the mod, for the plugin:
/// written beside it first, then moved over it, so the plugin never reads half a file.
pub fn write_apart(marks: &[u64], dir: &Path) -> std::io::Result<()> {
    let path = dir.join(westplay_shared::TABLET_APART_FILE);
    let partial = dir.join(format!("{}.partial", westplay_shared::TABLET_APART_FILE));
    std::fs::write(&partial, westplay_shared::write_marks(marks))?;
    std::fs::rename(&partial, &path)
}

/// Words that set a mod apart, by their hashes (`cityhash::city_hash64` of the word, in lower case): a mod is
/// set apart when every word of one group is among the words of its file's (or folder's) name, or of its
/// manifest's `author` or `display_name` lines.
const APART: &[&[u64]] =
    &[&[0xdd10_e007_18e3_ff30], &[0xc237_0cf4_6903_dd71, 0xa08c_a3fb_d82a_c129, 0x94ab_97db_93df_f806]];

/// Whether `path`'s mod (mounted as `mounted`) is set apart ([`APART`]): its vehicles never get the tablet.
fn set_apart(path: &Path, mounted: &GameFiles) -> bool {
    let words = |text: &str| -> BTreeSet<u64> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(|word| city_hash64(word.as_bytes()))
            .collect()
    };
    let named = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    // Its manifest however it's written (some scrambled, which the game reads as text: `save::read`).
    let manifest = mounted.read("manifest.sii").ok().flatten().and_then(|bytes| save::read(&bytes).ok());
    let mut all = words(&named);
    for unit in manifest.iter().flatten() {
        for said in ["author", "display_name"].into_iter().filter_map(|field| unit.get(field)) {
            all.extend(words(said));
        }
    }
    APART.iter().any(|group| group.iter().all(|word| all.contains(word)))
}

/// Vehicles never given the tablet whatever mod lists them, by their ids' hashes (`city_hash64`).
const APART_VEHICLES: &[u64] = &[0xb162_96d6_78c2_d0a8, 0x1cb7_e722_157f_00ff, 0xa8ee_42b0_2d21_0c49];

/// Whether the vehicle `truck` (its id) is one of [`APART_VEHICLES`] (or any of its parts is).
fn vehicle_apart(truck: &str) -> bool {
    westplay_shared::vehicle_marked(truck, APART_VEHICLES)
}

/// The trucks' slots a built mod (`westplay.scs`'s bytes' file, `path`) has the tablet at, sorted; none if it
/// can't be read.
pub fn trucks_in_mod(path: &Path) -> Vec<Spot> {
    let Ok(archive) = Archive::open(path) else { return Vec::new() };
    let files = GameFiles::new(vec![Mount::Archive(archive)]);
    let truck_dirs = files.dirs("def/vehicle/truck").unwrap_or_default();
    truck_dirs
        .iter()
        .flat_map(|truck| Slot::ALL.map(|slot| Spot::new(truck, slot)))
        .filter(|spot| files.read(&definition_path(spot)).is_ok_and(|found| found.is_some()))
        .collect()
}

/// A spot's tablet, as the game's dashboard sets are defined (`dashbrd_set1.daf.xf_euro6.set_dashbrd`): a part
/// with an `interior_model` and no `exterior_model`, drawn with the camera in the cab only (see the module's
/// header). Hanging, it's fixed where the hanging toys hang from (theirs swing on a rope below it: a screen
/// shouldn't).
fn accessory(spot: &Spot, model: &str) -> String {
    let (truck, slot) = (&spot.truck, spot.slot.folder());
    format!(
        "SiiNunit\n{{\naccessory_addon_data : {UNIT}.{truck}.{slot}\n{{\n\tname: \"WestPlay tablet\"\n\tprice: 1\n\tunlock: 0\n\ticon: \"{NAME}\"\n\tpart_type: aftermarket\n\n\tinterior_model: \"/{DIR}/{model}.pmd\"\n}}\n}}\n"
    )
}

/// The tablet on a dashboard set's toy spot (a table's red slots), as the game's dashboard toys are defined
/// (`gr_owl.addon_hookup`): any truck's set with spots takes it, so it can't be kept out of a vehicle (the plugin
/// keeps its screen blank in those it mustn't show in: [`apart_marks`]). One model for every view, as a hookup's
/// is: drawn from outside the cab too, so the camera system goes by the cab's mark where the truck has one.
fn hookup() -> String {
    format!(
        "SiiNunit
{{
accessory_hookup_int_data : {UNIT}.addon_hookup
{{
	name: \"WestPlay tablet\"
	price: 1
	unlock: 0
	icon: \"{NAME}\"
	part_type: aftermarket

	model: \"/{DIR}/{ON_SLOT}.pmd\"

	suitable_for[]: \"{}\"
}}
}}
",
        Slot::DashboardSet.folder()
    )
}

/// The tablet's body: lit as the game's own parts, on its colour texture (a patch of each look's colour).
fn body_material() -> String {
    format!(
        "effect : \"eut2.dif.spec.rfx\" {{\n\tadditional_ambient : 0.000000\n\tdiffuse : {{ 1.000000 , 1.000000 , 1.000000 }}\n\treflection : 0.000000\n\tshininess : 80.000000\n\tspecular : {{ 1.000000 , 1.000000 , 1.000000 }}\n\ttexture : \"texture_base\" {{\n\t\tsource : \"/{DIR}/{NAME}.tobj\"\n\t\tsampler : default\n\t}}\n}}\n"
    )
}

/// Its screen: the game's own GPS screen material (`/automat/d8/d8187d0c1f48c659.mat`, the Scania's too: lit
/// at night as a screen is), on WestPlay's texture.
fn screen_material(texture: &str) -> String {
    format!(
        "effect : \"eut2.dif.lum.rfx\" {{\n\tadditional_ambient : 0.000000\n\tdiffuse : {{ 0.132868 , 0.132868 , 0.132868 }}\n\tluminance_night : 10.000000\n\tluminance_output : 2000.000000\n\treflection : 0.000000\n\tshininess : 40.000000\n\tspecular : {{ 0.010023 , 0.010023 , 0.010023 }}\n\ttexture : \"texture_base\" {{\n\t\tsource : \"{texture}\"\n\t\tsampler : default\n\t}}\n}}\n"
    )
}

fn word(data: &[u8], index: usize) -> Result<u32, String> {
    data.get(index * 4..index * 4 + 4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .ok_or_else(|| format!("no word {index}"))
}

fn float(data: &[u8], at: usize) -> Result<f32, String> {
    data.get(at..at + 4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .ok_or_else(|| format!("no float at {at}"))
}

fn set(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_float(data: &mut [u8], at: usize, value: f32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// A piece of the source model: its record, and its vertices' and indices' bytes.
struct Piece {
    record: Vec<u8>,
    vertices: Vec<u8>,
    indices: Vec<u8>,
}

fn source_piece(index: usize) -> Result<Piece, String> {
    let table = word(SOURCE_PMG, PIECES_AT)? as usize;
    let record = SOURCE_PMG.get(table + index * PIECE_SIZE..table + (index + 1) * PIECE_SIZE).ok_or("no piece")?;
    let field = |field: usize| word(record, field).map(|value| value as usize);
    let (count, stride, positions) = (field(PIECE_VERTICES)?, field(PIECE_STRIDE)?, field(PIECE_POSITIONS)?);
    // Its vertices interleaved: position, normal (+12), colour (+24), texture coordinate (+28).
    let interleaved = (field(PIECE_NORMALS)?, field(PIECE_COLOURS)?, field(PIECE_UVS)?)
        == (positions + 12, positions + 24, positions + 28)
        && stride == 36
        && (20..24).all(|field| word(record, field) == Ok(u32::MAX));
    if !interleaved {
        return Err(format!("piece {index} isn't laid out as read"));
    }
    let vertices = SOURCE_PMG.get(positions..positions + count * stride).ok_or("vertices past the end")?;
    let at = field(PIECE_INDEX_AT)?;
    let indices = SOURCE_PMG.get(at..at + field(PIECE_INDICES)? * 2).ok_or("indices past the end")?;
    Ok(Piece { record: record.to_vec(), vertices: vertices.to_vec(), indices: indices.to_vec() })
}

/// The box of a piece's record: its least and greatest corners.
fn piece_box(piece: &Piece) -> Result<([f32; 3], [f32; 3]), String> {
    let at = |i: usize| float(&piece.record, (PIECE_BOUNDS + 4 + i) * 4);
    Ok(([at(0)?, at(1)?, at(2)?], [at(3)?, at(4)?, at(5)?]))
}

/// `p` turned about the model's x axis by `degrees`, its upright toward +z (back: a tablet leaning back).
fn rotate_x(p: [f32; 3], degrees: f32) -> [f32; 3] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [p[0], p[1] * cos - p[2] * sin, p[1] * sin + p[2] * cos]
}

/// `p` turned about the model's upright by `degrees`, its front (-z) toward +x.
fn rotate_y(p: [f32; 3], degrees: f32) -> [f32; 3] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [p[0] * cos - p[2] * sin, p[1], p[0] * sin + p[2] * cos]
}

/// A piece's bounds (a box round its vertices, and a sphere round that) from its vertices as they are.
fn set_bounds(piece: &mut Piece) -> Result<(), String> {
    let (mut least, mut most) = ([f32::MAX; 3], [f32::MIN; 3]);
    for vertex in piece.vertices.chunks_exact(36) {
        for axis in 0..3 {
            let value = float(vertex, axis * 4)?;
            least[axis] = least[axis].min(value);
            most[axis] = most[axis].max(value);
        }
    }
    let centre = [0, 1, 2].map(|axis| (least[axis] + most[axis]) / 2.0);
    let radius = (0..3).map(|axis| (most[axis] - centre[axis]).powi(2)).sum::<f32>().sqrt();
    for (i, value) in centre.into_iter().chain([radius]).chain(least).chain(most).enumerate() {
        set_float(&mut piece.record, (PIECE_BOUNDS + i) * 4, value);
    }
    Ok(())
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// The tablet's stand on a dashboard set's spot (owner 2026-10-24: "build a stand for it"): a base plate, a lip
/// in front holding the tablet's bottom edge, and an arm from the base's back up to the tablet's back, turned
/// with the tablet toward the driver. Of the tablet's body's material, coloured as its back (`body`: the body
/// piece, as made: its record and its vertices' colour). `height`, `thickness`: the tablet's, standing.
fn stand(body: &Piece, turn: f32, height: f32, thickness: f32) -> Result<Piece, String> {
    // Boxes: a centre, three axes (each the cross of the two before it), half sizes along them.
    let x = [1.0, 0.0, 0.0];
    let mut boxes = vec![
        ([0.0, 0.004, 0.02], [x, [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.075, 0.004, 0.055]),
        ([0.0, 0.015, -0.030], [x, [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.075, 0.007, 0.004]),
    ];
    // The arm: from the base's back to the tablet's back, ARM_UP of the way up it.
    let from = [0.0, 0.008, 0.07];
    let tablet_back = rotate_x([0.0, height * ARM_UP, thickness / 2.0], LEAN_BACK);
    let to = [0.0, tablet_back[1] + STAND_LIFT, tablet_back[2] + STAND_SEAT];
    let along = [0, 1, 2].map(|k| to[k] - from[k]);
    let length = (0..3).map(|k| along[k] * along[k]).sum::<f32>().sqrt();
    let along = along.map(|v| v / length);
    boxes.push(([0, 1, 2].map(|k| (from[k] + to[k]) / 2.0), [x, along, cross(x, along)], [0.02, length / 2.0, 0.004]));
    blocks(body, &boxes, |v| rotate_y(v, turn))
}

/// The hanging tablet's mount: a clamp over the middle of the tablet's top edge (a plate on it, a lip over the
/// front's black glass and one down its back), and an arm from it up into the trim over the hanging point, on a
/// wider foot; turned with the tablet (`level`: the hanging point's pitch, undone). `front`, `back`: the tablet's
/// front (its glass) and back (the camera's bump left out), from its middle along its depth. As the stand, of the
/// body's material and colour.
fn mount(body: &Piece, front: f32, back: f32, level: f32) -> Result<Piece, String> {
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    // A box from its least corner to its greatest.
    let span = |low: [f32; 3], high: [f32; 3]| -> Block {
        ([0, 1, 2].map(|k| (low[k] + high[k]) / 2.0), axes, [0, 1, 2].map(|k| (high[k] - low[k]) / 2.0))
    };
    let top = -HANG_DROP;
    let (inside_front, inside_back) = (front - CLAMP_GAP, back + CLAMP_GAP);
    let (outside_front, outside_back) = (inside_front - CLAMP_WALL, inside_back + CLAMP_WALL);
    let plate_top = top + CLAMP_GAP + CLAMP_WALL;
    let x = CLAMP_WIDTH / 2.0;
    let middle = (outside_front + outside_back) / 2.0;
    let boxes = [
        // The plate on the top edge, the lips over the front and down the back.
        span([-x, top + CLAMP_GAP, outside_front], [x, plate_top, outside_back]),
        span([-x, top - CLAMP_FRONT, outside_front], [x, top + CLAMP_GAP, inside_front]),
        span([-x, top - CLAMP_BACK, inside_back], [x, top + CLAMP_GAP, outside_back]),
        // The foot, and the arm up into the trim.
        span(
            [-FOOT_WIDTH / 2.0, plate_top, middle - ARM_DEPTH / 2.0],
            [FOOT_WIDTH / 2.0, plate_top + FOOT_HEIGHT, middle + ARM_DEPTH / 2.0],
        ),
        span(
            [-ARM_WIDTH / 2.0, plate_top + FOOT_HEIGHT, middle - ARM_DEPTH / 2.0],
            [ARM_WIDTH / 2.0, BAR_UP, middle + ARM_DEPTH / 2.0],
        ),
    ];
    blocks(body, &boxes, |v| hanging_turn(v, level))
}

/// A box: its centre, three axes (each the cross of the two before it), half its size along each.
type Block = ([f32; 3], [[f32; 3]; 3], [f32; 3]);

/// A piece of `boxes`, each corner and normal turned by `turn`, of the tablet's body's material, coloured as its
/// back (`body`: the body piece, as made: its record), its triangles wound as the tablet's are.
fn blocks(body: &Piece, boxes: &[Block], turn: impl Fn([f32; 3]) -> [f32; 3]) -> Result<Piece, String> {
    let source = source_piece(BODY)?;
    // The back's colour and texture coordinate: the most of the source body's vertices facing +z have (the
    // aluminium's, not the camera's or the connector's beside it).
    let mut looks: BTreeMap<&[u8], usize> = BTreeMap::new();
    for vertex in source.vertices.chunks_exact(36).filter(|vertex| float(vertex, 20).is_ok_and(|z| z > 0.9)) {
        *looks.entry(&vertex[24..36]).or_default() += 1;
    }
    let back =
        looks.into_iter().max_by_key(|(_, count)| *count).map(|(look, _)| look).ok_or("the tablet has no back")?;
    // Which way the source's triangles wind about their vertices' normals (counter-clockwise or not).
    let index = |i: usize| u16::from_le_bytes([source.indices[i * 2], source.indices[i * 2 + 1]]) as usize * 36;
    let corner = |i: usize| -> Result<[f32; 3], String> {
        let at = index(i);
        Ok([float(&source.vertices, at)?, float(&source.vertices, at + 4)?, float(&source.vertices, at + 8)?])
    };
    let (a, b, c) = (corner(0)?, corner(1)?, corner(2)?);
    let facing = cross([0, 1, 2].map(|k| b[k] - a[k]), [0, 1, 2].map(|k| c[k] - a[k]));
    let normal = [
        float(&source.vertices, index(0) + 12)?,
        float(&source.vertices, index(0) + 16)?,
        float(&source.vertices, index(0) + 20)?,
    ];
    let counter_clockwise = (0..3).map(|k| facing[k] * normal[k]).sum::<f32>() > 0.0;

    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    for &(centre, axes, half) in boxes {
        for (k, sign) in [0, 1, 2].into_iter().flat_map(|k| [(k, 1.0f32), (k, -1.0)]) {
            let n = axes[k].map(|v| v * sign);
            // Two axes across the face, `a` x `b` along its normal.
            let (a, b, ha, hb) = if sign > 0.0 {
                (axes[(k + 1) % 3], axes[(k + 2) % 3], half[(k + 1) % 3], half[(k + 2) % 3])
            } else {
                (axes[(k + 2) % 3], axes[(k + 1) % 3], half[(k + 2) % 3], half[(k + 1) % 3])
            };
            let first = (vertices.len() / 36) as u16;
            for (sa, sb) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let p: [f32; 3] = [0, 1, 2].map(|i| centre[i] + n[i] * half[k] + a[i] * ha * sa + b[i] * hb * sb);
                for value in turn(p).into_iter().chain(turn(n)) {
                    vertices.extend(value.to_le_bytes());
                }
                vertices.extend(back);
            }
            let order: [u16; 6] = if counter_clockwise { [0, 1, 2, 0, 2, 3] } else { [0, 2, 1, 0, 3, 2] };
            for corner in order {
                indices.extend((first + corner).to_le_bytes());
            }
        }
    }
    let mut record = body.record.clone();
    set(&mut record, PIECE_INDICES * 4, (indices.len() / 2) as u32);
    set(&mut record, PIECE_VERTICES * 4, (vertices.len() / 36) as u32);
    set(&mut record, PIECE_MATERIAL * 4, 0);
    Ok(Piece { record, vertices, indices })
}

/// The tablet's model: its body and glass, moved so its origin is the middle of the body's base (on a dashboard
/// set, on a stand of its own, leaning back) or its bar's foot (hanging below it), turned toward the driver, the
/// glass showing the screen texture's first SCREEN_HEIGHT rows (of `texture_height`), the first at its top, left
/// to right, across the whole of it (its rounded corners cut the frame's, as an iPad's do).
fn model(texture_height: u32, origin: Origin) -> Result<Vec<u8>, String> {
    let counts = (word(SOURCE_PMG, 0)?, word(SOURCE_PMG, PIECES)?, word(SOURCE_PMG, PARTS)?);
    let empty = (3..8).all(|index| word(SOURCE_PMG, index) == Ok(0)) && word(SOURCE_PMG, STRINGS_AT + 1)? == 0;
    if counts != (PMG_MAGIC, 2, 1) || !empty || word(SOURCE_PMG, PARTS_AT)? as usize != HEADER_SIZE {
        return Err("the tablet's model isn't laid out as read".into());
    }
    let mut pieces = vec![source_piece(BODY)?, source_piece(SCREEN)?];
    let (low, high) = piece_box(&pieces[0])?;
    let height = match origin {
        Origin::Base(_) => low[1],
        Origin::Top(_) => high[1] + HANG_DROP,
    };
    let centre = [(low[0] + high[0]) / 2.0, height, (low[2] + high[2]) / 2.0];
    // Its back: the furthest of the body's points from the glass, the camera's bump left out.
    let mut back = low[2];
    for vertex in pieces[0].vertices.chunks_exact(36) {
        let z = float(vertex, 8)?;
        if z < low[2] + CAMERA_FROM {
            back = back.max(z);
        }
    }
    // Where a point (or, `direction`, a normal) of the tablet goes, from its origin.
    let place = |p: [f32; 3], direction: bool| match origin {
        Origin::Base(turn) => {
            let p = rotate_x(p, LEAN_BACK);
            let p = if direction { p } else { [p[0], p[1] + STAND_LIFT, p[2] + STAND_SEAT] };
            rotate_y(p, turn)
        }
        Origin::Top(level) => hanging_turn(p, level),
    };
    // The texture's rows as its coordinate counts them: from its first (0) down to its last (1).
    let frame_bottom = SCREEN_HEIGHT as f32 / texture_height as f32;
    let (glass_low, glass_high) = piece_box(&pieces[1])?;
    // How far across the glass a point is along `axis`, from its greatest side (0) to its least (1).
    let across =
        |position: [f32; 3], axis: usize| (glass_high[axis] - position[axis]) / (glass_high[axis] - glass_low[axis]);
    for (index, piece) in pieces.iter_mut().enumerate() {
        for vertex in piece.vertices.chunks_exact_mut(36) {
            let position = [float(vertex, 0)?, float(vertex, 4)?, float(vertex, 8)?];
            let normal = [float(vertex, 12)?, float(vertex, 16)?, float(vertex, 20)?];
            let moved = place([0, 1, 2].map(|axis| position[axis] - centre[axis]), false);
            let turned = place(normal, true);
            for axis in 0..3 {
                set_float(vertex, axis * 4, moved[axis]);
                set_float(vertex, 12 + axis * 4, turned[axis]);
            }
            if index == 1 {
                // Facing -z, the glass's left is its +x side; its top its +y.
                set_float(vertex, 28, across(position, 0));
                set_float(vertex, 32, across(position, 1) * frame_bottom);
            }
        }
        set(&mut piece.record, PIECE_MATERIAL * 4, index as u32);
    }
    let added = match origin {
        Origin::Base(turn) => stand(&pieces[0], turn, high[1] - low[1], high[2] - low[2])?,
        Origin::Top(level) => mount(&pieces[0], low[2] - centre[2], back - centre[2], level)?,
    };
    pieces.push(added);
    for piece in &mut pieces {
        set_bounds(piece)?;
    }
    // The header (its parts' table where it was), the part, the pieces' records, their vertices, their indices.
    let table = HEADER_SIZE + PART_SIZE;
    let vertices_at = table + pieces.len() * PIECE_SIZE;
    let vertices_size: usize = pieces.iter().map(|piece| piece.vertices.len()).sum();
    let indices_at = vertices_at + vertices_size;
    let indices_size: usize = pieces.iter().map(|piece| piece.indices.len()).sum();
    let mut out = SOURCE_PMG[..table].to_vec();
    set(&mut out, PIECES * 4, pieces.len() as u32);
    set(&mut out, HEADER_SIZE + 8, pieces.len() as u32);
    for index in [LOCATORS_AT, PIECES_AT] {
        set(&mut out, index * 4, table as u32);
    }
    for (index, value) in [
        (STRINGS_AT, vertices_at),
        (VERTICES_AT, vertices_at),
        (VERTICES_AT + 1, vertices_size),
        (INDICES_AT, indices_at),
        (INDICES_AT + 1, indices_size),
    ] {
        set(&mut out, index * 4, value as u32);
    }
    // The model's bounds: the pieces' boxes', and a sphere round them.
    let (mut least, mut most) = ([f32::MAX; 3], [f32::MIN; 3]);
    for piece in &pieces {
        let (low, high) = piece_box(piece)?;
        for axis in 0..3 {
            least[axis] = least[axis].min(low[axis]);
            most[axis] = most[axis].max(high[axis]);
        }
    }
    let centre = [0, 1, 2].map(|axis| (least[axis] + most[axis]) / 2.0);
    let radius = (0..3).map(|axis| (most[axis] - centre[axis]).powi(2)).sum::<f32>().sqrt();
    for (i, value) in centre.into_iter().chain([radius]).chain(least).chain(most).enumerate() {
        set_float(&mut out, (BOUNDS + i) * 4, value);
    }
    let (mut vertex_at, mut index_at) = (vertices_at, indices_at);
    for piece in &mut pieces {
        for (field, offset) in [(PIECE_POSITIONS, 0), (PIECE_NORMALS, 12), (PIECE_COLOURS, 24), (PIECE_UVS, 28)] {
            set(&mut piece.record, field * 4, (vertex_at + offset) as u32);
        }
        set(&mut piece.record, PIECE_INDEX_AT * 4, index_at as u32);
        vertex_at += piece.vertices.len();
        index_at += piece.indices.len();
        out.extend(&piece.record);
    }
    for piece in &pieces {
        out.extend(&piece.vertices);
    }
    for piece in &pieces {
        out.extend(&piece.indices);
    }
    Ok(out)
}

/// The model's descriptor: Kedi739's, its look and variant `default` (as a toy's), naming its two materials.
fn descriptor() -> Result<Vec<u8>, String> {
    // Counts: version, materials, looks, pieces, variants, parts, attributes, a variant's values' bytes; then the
    // strings' bytes and where the looks, variants, parts' ranges, variants' values, attributes, materials and
    // strings are.
    let header: Vec<u32> = (0..16).map(|index| word(SOURCE_PMD, index)).collect::<Result<_, _>>()?;
    if header[..8] != [4, 4, 1, 0, 1, 1, 1, 4] || header[14] != 0x6c {
        return Err("the tablet's descriptor isn't laid out as read".into());
    }
    let default = string_to_token("default").ok_or("no token")?;
    let materials = [format!("/{DIR}/body.mat"), format!("/{DIR}/screen.mat")];
    let strings_at = 0x6c + 4 * materials.len();
    let mut out = SOURCE_PMD[..0x6c].to_vec();
    for at in [header[9] as usize, header[10] as usize] {
        out[at..at + 8].copy_from_slice(&default.to_le_bytes());
    }
    let mut strings = Vec::new();
    for material in &materials {
        out.extend(((strings_at + strings.len()) as u32).to_le_bytes());
        strings.extend(material.as_bytes());
        strings.push(0);
    }
    set(&mut out, 4, materials.len() as u32);
    set(&mut out, 8 * 4, strings.len() as u32);
    set(&mut out, 15 * 4, strings_at as u32);
    out.extend(strings);
    Ok(out)
}

/// The shop's icon: WestPlay's, as a texture the mod's screens are made of (`game_mod::screen_dds`).
fn icon() -> Result<Vec<u8>, String> {
    let mut reader =
        png::Decoder::new(std::io::Cursor::new(ICON_PNG)).read_info().map_err(|error| error.to_string())?;
    let mut pixels = vec![0; reader.output_buffer_size().ok_or("no size")?];
    let info = reader.next_frame(&mut pixels).map_err(|error| error.to_string())?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err("the icon isn't 8-bit RGBA".into());
    }
    let mut bgra = pixels[..info.buffer_size()].to_vec();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Ok(crate::game_mod::screen_dds(info.width, info.height, &bgra))
}

#[cfg(test)]
mod tests {
    use super::*;
    use westplay_mapdata::buildings;
    use westplay_mapdata::files::Mount;

    const HEIGHT: u32 = SCREEN_HEIGHT + 7;

    /// The tablet's height (its body's, standing as made): 17.9 cm, an iPad Air's width.
    fn tablet_height() -> f32 {
        let (low, high) = piece_box(&source_piece(BODY).unwrap()).unwrap();
        high[1] - low[1]
    }

    /// A source piece's triangles.
    fn triangles(index: usize) -> usize {
        word(&source_piece(index).unwrap().record, PIECE_INDICES).unwrap() as usize / 3
    }

    /// A model's corners, least and greatest, from its triangles.
    fn extent(pmg: &[u8]) -> ([f32; 3], [f32; 3]) {
        let triangles = buildings::model_triangles(pmg).unwrap();
        triangles.iter().flatten().fold(([f32::MAX; 3], [f32::MIN; 3]), |(mut l, mut m), p| {
            for axis in 0..3 {
                l[axis] = l[axis].min(p[axis]);
                m[axis] = m[axis].max(p[axis]);
            }
            (l, m)
        })
    }

    #[test]
    fn the_hanging_tablet_hangs_on_its_bar() {
        let pmg = model(HEIGHT, Origin::Top(0.0)).unwrap();
        let (least, most) = extent(&pmg);
        // The bar from up in the trim, the tablet below it.
        assert!((tablet_height() - 0.1793).abs() < 0.001, "{}", tablet_height());
        let bottom = least[1] + HANG_DROP + tablet_height();
        assert!((most[1] - BAR_UP).abs() < 1e-4 && bottom.abs() < 0.002, "{least:?} {most:?}");
        assert!((least[0] + most[0]).abs() < 1e-3);
        // Its bar, no stand, upright, turned toward the driver.
        assert_eq!(word(&pmg, PIECES).unwrap(), 3);
        let at = word(&pmg, PIECES_AT).unwrap() as usize + PIECE_SIZE;
        let normals = word(&pmg[at..], PIECE_NORMALS).unwrap() as usize;
        let normal = [0, 1, 2].map(|k| float(&pmg, normals + k * 4).unwrap());
        assert!((normal[0].atan2(-normal[2]).to_degrees() - TOWARD_DRIVER).abs() < 3.0, "{normal:?}");
    }

    #[test]
    fn the_hanging_tablet_is_held_by_its_clamp() {
        // The mount where the tablet is (turned back from toward the driver): nothing of it inside the tablet, its
        // plate on the top edge, its lips past the glass and the back, down over each.
        let pmg = model(HEIGHT, Origin::Top(0.0)).unwrap();
        let points = |piece: usize| -> Vec<[f32; 3]> {
            let at = word(&pmg, PIECES_AT).unwrap() as usize + piece * PIECE_SIZE;
            let (positions, count) = (
                word(&pmg[at..], PIECE_POSITIONS).unwrap() as usize,
                word(&pmg[at..], PIECE_VERTICES).unwrap() as usize,
            );
            (0..count)
                .map(|v| [0, 1, 2].map(|k| float(&pmg, positions + v * 36 + k * 4).unwrap()))
                .map(|p| rotate_y(p, -TOWARD_DRIVER))
                .collect()
        };
        let (tablet, mount) = (points(BODY), points(2));
        let low = tablet.iter().fold([f32::MAX; 3], |l, p| [0, 1, 2].map(|k| l[k].min(p[k])));
        let high = tablet.iter().fold([f32::MIN; 3], |m, p| [0, 1, 2].map(|k| m[k].max(p[k])));
        // The tablet's top at the drop below the hanging point; its front (the glass) and back (camera left out).
        assert!((high[1] + HANG_DROP).abs() < 1e-5, "{high:?}");
        let front = low[2];
        let back = tablet.iter().filter(|p| p[2] < front + CAMERA_FROM).map(|p| p[2]).fold(f32::MIN, f32::max);
        let inside =
            |p: &[f32; 3]| p[1] < high[1] && p[2] > front && p[2] < back && p[0].abs() < (high[0] - low[0]) / 2.0;
        assert!(!mount.iter().any(inside), "a part of the mount inside the tablet");
        let over_front = mount.iter().filter(|p| p[2] < front).map(|p| p[1]).fold(f32::MAX, f32::min);
        let down_back = mount.iter().filter(|p| p[2] > back).map(|p| p[1]).fold(f32::MAX, f32::min);
        assert!((over_front - (high[1] - CLAMP_FRONT)).abs() < 1e-5, "{over_front}");
        assert!((down_back - (high[1] - CLAMP_BACK)).abs() < 1e-5, "{down_back}");
        // The lip over the front stays on its black glass, above the screen.
        let (_, glass_high) = piece_box(&source_piece(SCREEN).unwrap()).unwrap();
        let (_, body_high) = piece_box(&source_piece(BODY).unwrap()).unwrap();
        assert!(CLAMP_FRONT < body_high[1] - glass_high[1], "the lip reaches the screen");
        // The arm up into the trim, as wide and deep as made.
        let top = mount.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        assert!((top - BAR_UP).abs() < 1e-5);
    }

    /// `v` turned by a locator's rotation (a quaternion, w first), as the game turns what's fitted there.
    fn locator_turn([w, x, y, z]: [f32; 4], v: [f32; 3]) -> [f32; 3] {
        let u = [x, y, z];
        let t = cross(u, v).map(|c| 2.0 * c);
        let c = cross(u, t);
        [0, 1, 2].map(|i| v[i] + w * t[i] + c[i])
    }

    #[test]
    fn the_hanging_tablet_hangs_level_under_a_pitched_hanging_point() {
        // In the cab, every corner of the tablet and its bar where it is under a hanging point turned round with no
        // pitch (upright, its top edge as level as the model's own: within a millimetre across it).
        let in_cab = |rotation: [f32; 4], level: f32| -> Vec<[f32; 3]> {
            let pmg = model(HEIGHT, Origin::Top(level)).unwrap();
            let pmg = &pmg;
            (0..3)
                .flat_map(|index| {
                    let at = word(pmg, PIECES_AT).unwrap() as usize + index * PIECE_SIZE;
                    let (positions, count) = (
                        word(&pmg[at..], PIECE_POSITIONS).unwrap() as usize,
                        word(&pmg[at..], PIECE_VERTICES).unwrap() as usize,
                    );
                    (0..count)
                        .map(move |v| [0, 1, 2].map(|k| float(pmg, positions + v * 36 + k * 4).unwrap()))
                        .collect::<Vec<_>>()
                })
                .map(|p| locator_turn(rotation, p))
                .collect()
        };
        let upright = in_cab([0.0, 0.0, 1.0, 0.0], 0.0);
        // The game's: the Scania S's (8 degrees up), the Volvo FH's and the Renault T 2024's (3.4 and 4.7 down).
        for (rotation, pitch) in [
            ([4.32e-7, 3.02e-8, -0.997_564_1, 0.069_756_43], 8.0),
            ([1.82e-18, 6.12e-17, 0.999_558_1, 0.029_725_03], -3.4),
            ([2.50e-18, 6.12e-17, 0.999_167_9, 0.040_785_76], -4.7),
        ] {
            let level = pitch_of(rotation);
            assert!((level - pitch).abs() < 0.05, "{level}");
            let hung = in_cab(rotation, level);
            assert_eq!(hung.len(), upright.len());
            for (p, q) in hung.iter().zip(&upright) {
                assert!((0..3).all(|k| (p[k] - q[k]).abs() < 0.001), "{pitch}: {p:?} against {q:?}");
            }
            // Not undone, the pitch would tip it: a corner a centimetre and more out.
            let tipped = in_cab(rotation, 0.0);
            assert!(tipped.iter().zip(&upright).any(|(p, q)| (0..3).any(|k| (p[k] - q[k]).abs() > 0.01)), "{pitch}");
        }
    }

    #[test]
    fn on_a_sets_toy_spot_the_tablet_faces_as_the_spot_does() {
        // A set's spots face back into the cab already: the tablet's front (-z) turned toward +x by TOWARD_DRIVER
        // alone (which the spot's turn round makes toward the driver), and up by its lean.
        let pmg = model(HEIGHT, Origin::Base(TOWARD_DRIVER)).unwrap();
        assert_eq!(word(&pmg, PIECES).unwrap(), 3);
        let at = word(&pmg, PIECES_AT).unwrap() as usize + PIECE_SIZE;
        let normals = word(&pmg[at..], PIECE_NORMALS).unwrap() as usize;
        let normal = [0, 1, 2].map(|k| float(&pmg, normals + k * 4).unwrap());
        let turned = normal[0].atan2(-normal[2]).to_degrees();
        assert!(normal[2] < -0.5 && (turned - TOWARD_DRIVER).abs() < 3.0 && normal[1] > 0.2, "{normal:?}");
    }

    #[test]
    fn the_tablet_stands_on_its_stand_its_screen_showing_the_frame() {
        let pmg = model(HEIGHT, Origin::Base(ON_A_SET)).unwrap();
        assert_eq!(word(&pmg, PIECES).unwrap(), 3);
        // Read as the map's model reader reads any: its triangles (the body's, the glass's, the stand's three
        // boxes' 12 each).
        assert_eq!(buildings::model_triangles(&pmg).unwrap().len(), triangles(BODY) + triangles(SCREEN) + 3 * 12);
        let (least, most) = extent(&pmg);
        // On the spot (the stand's underside), the tablet's top its height up its lean, above the stand's lift.
        let top = STAND_LIFT + tablet_height() * LEAN_BACK.to_radians().cos();
        assert!(least[1].abs() < 1e-4 && (most[1] - top).abs() < 0.003, "{least:?} {most:?}");
        let bounds = buildings::model_box(&pmg).unwrap();
        assert!((bounds.min[1] - least[1]).abs() < 1e-4 && (bounds.max[0] - most[0]).abs() < 1e-4);
        // The glass: the frame across all of it, its left at 0, its right at 1, its top at the texture's first row
        // (0), its bottom where the frame's 720 rows end; and in between as far across as it is.
        let screen = &source_piece(SCREEN).unwrap();
        let at = word(&pmg, PIECES_AT).unwrap() as usize + PIECE_SIZE;
        assert_eq!(word(&pmg[at..], PIECE_MATERIAL).unwrap(), 1);
        let uvs = word(&pmg[at..], PIECE_UVS).unwrap() as usize;
        let count = word(&pmg[at..], PIECE_VERTICES).unwrap() as usize;
        assert_eq!(count as u32, word(&screen.record, PIECE_VERTICES).unwrap());
        let glass: Vec<([f32; 3], f32, f32)> = (0..count)
            .map(|v| {
                let position = word(&pmg[at..], PIECE_POSITIONS).unwrap() as usize + v * 36;
                let point = [0, 1, 2].map(|k| float(&pmg, position + k * 4).unwrap());
                (point, float(&pmg, uvs + v * 36).unwrap(), float(&pmg, uvs + v * 36 + 4).unwrap())
            })
            .collect();
        let bottom = 720.0 / 727.0;
        let (us, vs): (Vec<f32>, Vec<f32>) = glass.iter().map(|&(_, u, v)| (u, v)).unzip();
        let extremes = |values: &[f32]| values.iter().fold((f32::MAX, f32::MIN), |(l, m), &v| (l.min(v), m.max(v)));
        assert_eq!(extremes(&us), (0.0, 1.0));
        assert!(extremes(&vs).0 == 0.0 && (extremes(&vs).1 - bottom).abs() < 1e-6, "{:?}", extremes(&vs));
        // Turned round (ON_A_SET), its left is the model's -x; its top the highest.
        let leftmost = glass.iter().min_by(|a, b| a.0[0].total_cmp(&b.0[0])).unwrap();
        let highest = glass.iter().max_by(|a, b| a.0[1].total_cmp(&b.0[1])).unwrap();
        assert!(leftmost.1 < 0.01 && highest.2 < 0.01, "{leftmost:?} {highest:?}");
        // Facing the driver from a set's unturned spot: its front (-z) turned round to the back of the cab (+z), and
        // from there toward the driver's -x by TOWARD_DRIVER, and up by its lean.
        let normals = word(&pmg[at..], PIECE_NORMALS).unwrap() as usize;
        let normal = [0, 1, 2].map(|k| float(&pmg, normals + k * 4).unwrap());
        let turned = (-normal[0]).atan2(normal[2]).to_degrees();
        assert!(normal[2] > 0.5 && (turned - TOWARD_DRIVER).abs() < 3.0 && normal[1] > 0.2, "{normal:?}");
        // The stand's triangles wind as the tablet's do about their normals (the same faces shown).
        let winding = |piece: usize| -> Vec<bool> {
            let record = word(&pmg, PIECES_AT).unwrap() as usize + piece * PIECE_SIZE;
            let (positions, indices) = (
                word(&pmg[record..], PIECE_POSITIONS).unwrap() as usize,
                word(&pmg[record..], PIECE_INDEX_AT).unwrap() as usize,
            );
            let point = |i: usize, offset: usize| {
                let vertex = u16::from_le_bytes([pmg[indices + i * 2], pmg[indices + i * 2 + 1]]) as usize;
                [0, 1, 2].map(|k| float(&pmg, positions + vertex * 36 + offset + k * 4).unwrap())
            };
            (0..word(&pmg[record..], PIECE_INDICES).unwrap() as usize / 3)
                .map(|t| {
                    let (a, b, c) = (point(t * 3, 0), point(t * 3 + 1, 0), point(t * 3 + 2, 0));
                    let facing = cross([0, 1, 2].map(|k| b[k] - a[k]), [0, 1, 2].map(|k| c[k] - a[k]));
                    let n = point(t * 3, 12);
                    (0..3).map(|k| facing[k] * n[k]).sum::<f32>() > 0.0
                })
                .collect()
        };
        let tablet = winding(0)[0];
        assert!(winding(2).iter().all(|&ccw| ccw == tablet));
        // Its glass, 23.16 x 16.25 cm: 1.425:1, the shape the page is laid out in (`page_height`).
        let width = 1280.0 / westplay_shared::page_height(westplay_shared::tablet_layout()) as f32;
        let (low, high) = piece_box(screen).unwrap();
        assert!(((high[0] - low[0]) / (high[1] - low[1]) - width).abs() < 0.01);
    }

    #[test]
    fn its_descriptor_names_its_two_materials_as_a_toys_look() {
        let pmd = descriptor().unwrap();
        assert_eq!(word(&pmd, 1).unwrap(), 2);
        let default = string_to_token("default").unwrap().to_le_bytes();
        assert_eq!(&pmd[0x40..0x48], default);
        assert_eq!(&pmd[0x48..0x50], default);
        let material = |index: usize| {
            let at = word(&pmd, 0x6c / 4 + index).unwrap() as usize;
            let end = pmd[at..].iter().position(|&b| b == 0).unwrap();
            String::from_utf8(pmd[at..at + end].to_vec()).unwrap()
        };
        assert_eq!(material(0), "/vehicle/truck/upgrade/westplay_tablet/body.mat");
        assert_eq!(material(1), "/vehicle/truck/upgrade/westplay_tablet/screen.mat");
        assert_eq!(pmd.len(), 0x6c + 8 + word(&pmd, 8).unwrap() as usize);
    }

    #[test]
    fn every_truck_gets_one_at_its_hanging_items_and_one_as_its_own_dashboard_set() {
        let root = std::env::temp_dir().join(format!("westplay-tablet-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for slot in [
            "daf.xf/accessory/toyhang",
            "daf.xf/accessory/set_dashbrd",
            "mercedes.actros2014/accessory/toyhang",
            "scania.s_2016/accessory/set_glass",
            "odd.truck/accessory/toystand",
            // A coach WestPlay draws on the own screen of: no tablet.
            "t16shd/accessory/toyhang",
            "t16shd/accessory/set_dashbrd",
        ] {
            std::fs::create_dir_all(root.join("def/vehicle/truck").join(slot)).unwrap();
        }
        let game = GameFiles::new(vec![Mount::Folder(root.clone())]);
        let spots = trucks(&game).unwrap();
        assert_eq!(
            spots,
            [
                Spot::new("daf.xf", Slot::Hanging),
                Spot::new("daf.xf", Slot::DashboardSet),
                Spot::new("mercedes.actros2014", Slot::Hanging)
            ]
        );
        let pitches = BTreeMap::from([("daf.xf".to_owned(), -5.0)]);
        let files = files(&spots, &pitches, "/material/ui/westplay/screen_tablet.tobj", HEIGHT).unwrap();
        let defs: Vec<&str> =
            files.iter().map(|(path, _)| path.as_str()).filter(|path| path.starts_with("def/")).collect();
        assert_eq!(
            defs,
            [
                "def/vehicle/addon_hookup_storage.westplay.sii",
                "def/vehicle/truck/daf.xf/accessory/toyhang/westplay_tablet.sii",
                "def/vehicle/truck/daf.xf/accessory/set_dashbrd/westplay_tablet.sii",
                "def/vehicle/truck/mercedes.actros2014/accessory/toyhang/westplay_tablet.sii"
            ]
        );
        let text =
            |name: &str| String::from_utf8(files.iter().find(|(path, _)| path.contains(name)).unwrap().1.clone());
        // Both the kind of part the game draws with the camera in the cab only: an interior model, none for outside.
        let hanging = text("daf.xf/accessory/toyhang").unwrap();
        assert!(hanging.contains("accessory_addon_data : wp_tablet.daf.xf.toyhang"));
        assert!(
            hanging.contains("interior_model: \"/vehicle/truck/upgrade/westplay_tablet/westplay_tablet_hang_m50.pmd\"")
        );
        let set = text("daf.xf/accessory/set_dashbrd").unwrap();
        assert!(set.contains("accessory_addon_data : wp_tablet.daf.xf.set_dashbrd"));
        assert!(set.contains("interior_model: \"/vehicle/truck/upgrade/westplay_tablet/westplay_tablet.pmd\""));
        for def in [&hanging, &set] {
            assert!(!def.contains("exterior_model") && !def.contains("accessory_addon_int_data"), "{def}");
        }
        // On any set's toy spots (a table's red slots), a hookup: its model the one standing as the spot faces.
        let hookup = text("addon_hookup_storage.westplay.sii").unwrap();
        assert!(hookup.contains("accessory_hookup_int_data : wp_tablet.addon_hookup"));
        assert!(hookup.contains("suitable_for[]: \"set_dashbrd\""));
        assert!(hookup.contains("model: \"/vehicle/truck/upgrade/westplay_tablet/westplay_tablet_slot.pmd\""));
        // Every unit name's parts tokens, as the game reads them (12 characters at the most).
        for (path, text) in files.iter().filter(|(path, _)| path.starts_with("def/")) {
            let text = String::from_utf8_lossy(text);
            let unit = text.lines().find_map(|line| line.split_once(" : ").map(|(_, unit)| unit)).unwrap();
            assert!(unit.split('.').all(|part| string_to_token(part).is_some()), "{path}: {unit}");
        }
        // Both models, each with its descriptor beside it.
        // The standing one, and a hanging one per pitch: the DAF XF's (5 degrees down), the Actros' (none read).
        for model in ["westplay_tablet", "westplay_tablet_slot", "westplay_tablet_hang", "westplay_tablet_hang_m50"] {
            for kind in ["pmg", "pmd"] {
                let path = format!("vehicle/truck/upgrade/westplay_tablet/{model}.{kind}");
                assert!(files.iter().any(|(file, _)| *file == path), "{path}");
            }
        }
        let screen = files.iter().find(|(path, _)| path.ends_with("screen.mat")).unwrap();
        assert!(String::from_utf8_lossy(&screen.1).contains("source : \"/material/ui/westplay/screen_tablet.tobj\""));
        // The icon: 256 x 256.
        let icon =
            &files.iter().find(|(path, _)| path.ends_with("westplay_tablet.dds") && path.contains("ui")).unwrap().1;
        assert_eq!((word(icon, 3).unwrap(), word(icon, 4).unwrap()), (256, 256));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_mods_trucks_with_hanging_items_get_it_too_and_a_built_mod_says_which_it_has() {
        let root = std::env::temp_dir().join(format!("westplay-tablet-mods-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let folder = |name: &str, slots: &[&str]| {
            for slot in slots {
                std::fs::create_dir_all(root.join(name).join("def/vehicle/truck").join(slot)).unwrap();
            }
            root.join(name)
        };
        // The game, a truck mod as a folder (a truck of its own, and the game's with more toys), a truck mod as an
        // archive, a trailer mod, and one gone.
        let game = folder("game", &["daf.xf/accessory/toyhang"]);
        let jasper = folder("jasper", &["jasper.daf_2021/accessory/toyhang", "daf.xf/accessory/toyhang"]);
        let without = folder("without", &["odd.truck/accessory/toystand", "Not A Truck/accessory/toyhang"]);
        let archive = root.join("actros.scs");
        let def = ("def/vehicle/truck/hw.actros_l/accessory/toyhang/cup.sii".to_owned(), b"SiiNunit\n{\n}\n".to_vec());
        std::fs::write(&archive, crate::game_mod::zip(&[def])).unwrap();
        // Set apart: a mod by its file's name (its manifest locked), one by its manifest's author, and another mod
        // listing their vehicles too: none of them gets it.
        let veiled = |bytes: &[u8]| -> String { bytes.iter().map(|byte| (byte ^ 0x5a) as char).collect() };
        let (word, author) = (
            veiled(&[0x32, 0x2c, 0x2e]),
            veiled(&[
                0x12, 0x3f, 0x3b, 0x2c, 0x23, 0x7a, 0x0c, 0x3f, 0x32, 0x33, 0x39, 0x36, 0x3f, 0x29, 0x7a, 0x0e, 0x2f,
                0x28, 0x31, 0x3f, 0x23,
            ]),
        );
        let (first, second) =
            (veiled(&[0x2e, 0x28, 0x2c, 0x3d, 0x35, 0x6b, 0x6c, 0x34]), veiled(&[0x2e, 0x28, 0x29, 0x37, 0x35]));
        let pack = folder(
            &format!("ets-2-{word}-paket"),
            &[&format!("{first}/accessory/toyhang"), &format!("{second}/accessory/seat")],
        );
        let authored = folder("coach", &["man.coach/accessory/toyhang"]);
        std::fs::write(
            authored.join("manifest.sii"),
            format!("SiiNunit\n{{\nmod_package : .package_name\n{{\n\tauthor: \"{author}\"\n}}\n}}\n"),
        )
        .unwrap();
        // A mod named nothing telling, its manifest scrambled (as the game scrambles text) naming the author.
        let renamed = folder("pack_2", &["new.coach/accessory/toyhang"]);
        let text = format!("SiiNunit\n{{\nmod_package : .package_name\n{{\n\tauthor: \"{author}\"\n}}\n}}\n");
        let mut scrambled = b"3nK\x01\x00\x33".to_vec();
        scrambled.extend(text.bytes().enumerate().map(|(place, byte)| {
            let x = 0x33u8.wrapping_add(place as u8);
            byte ^ ((((x << 2) ^ !x) << 3) ^ x)
        }));
        std::fs::write(renamed.join("manifest.sii"), scrambled).unwrap();
        let copier = folder("toys", &[&format!("{first}/accessory/toyhang"), &format!("{second}/accessory/toyhang")]);
        let mods = [jasper, without, archive, root.join("gone.scs"), pack, authored, renamed, copier];
        let (mod_trucks, marks) = mod_trucks_and_marks(&mods);
        // The tablet blank in the set-apart mods' vehicles, whatever else lists them, and in those set apart by id:
        // their ids' hashes, for the plugin.
        for id in [first.as_str(), second.as_str(), "man.coach", "new.coach"] {
            assert!(westplay_shared::vehicle_marked(id, &marks), "{id}");
        }
        assert!(APART_VEHICLES.iter().all(|mark| marks.contains(mark)));
        assert!(westplay_shared::vehicle_marked(&veiled(&[0x2e, 0x6b, 0x6c, 0x29, 0x32, 0x3e]), &marks));
        assert!(!westplay_shared::vehicle_marked("vehicle.daf.xf", &marks));
        let hanging = |trucks: &[&str]| trucks.iter().map(|truck| Spot::new(truck, Slot::Hanging)).collect::<Vec<_>>();
        assert_eq!(mod_trucks, hanging(&["daf.xf", "hw.actros_l", "jasper.daf_2021"]));
        // Their vehicles left out where the accessories are written too, whatever list they come in by.
        let given =
            files(&hanging(&[&first, &second, "daf.xf", "setradt531"]), &BTreeMap::new(), "/s.tobj", HEIGHT).unwrap();
        let written: Vec<&String> =
            given.iter().map(|(path, _)| path).filter(|path| path.starts_with("def/vehicle/truck/")).collect();
        assert_eq!(written, ["def/vehicle/truck/daf.xf/accessory/toyhang/westplay_tablet.sii"]);
        // Built with them: the game's and the mods', each once; read back from the mod's file.
        let files = GameFiles::new(vec![Mount::Folder(game)]);
        let (bytes, built) = crate::game_mod::build(
            &files,
            &mod_trucks,
            &GameFiles::new(Vec::new()),
            &crate::cab_mark::Marks::default(),
        )
        .unwrap();
        assert!(built.patched.contains(&westplay_shared::TABLET));
        let built_at = root.join("westplay.scs");
        std::fs::write(&built_at, bytes).unwrap();
        assert_eq!(trucks_in_mod(&built_at), hanging(&["daf.xf", "hw.actros_l", "jasper.daf_2021"]));
        assert!(trucks_in_mod(&root.join("none.scs")).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
