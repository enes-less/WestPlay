//! The truck's own speakers: where each sits in its cab and what part of the sound it plays, for YouTube's
//! sound to come from them, around the driver's head (youtube.js plays it so).
//!
//! The game knows nothing of speakers: no truck definition names them and its interior models mark none, so
//! each truck's are placed here by hand, in the SDK's cab space (the head's: metres, x right, y up, z back,
//! from the joint the cab turns on; x = 0 is the cab's middle). A truck's interior model sits in its cab
//! model at its `interior_*` locator, and the SDK's cab space starts at the cab model's `cabin` locator: a
//! point of the interior model is, in cab space, itself plus the one less the other.
//!
//! Each speaker plays its own band (split as a car's crossover splits it: each side's frequencies from one
//! place) at its own level, as a car-audio tuner would set them for where it sits.

use serde::{Deserialize, Serialize};

/// Which of the sound's channels a speaker plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    Left,
    Right,
    /// Both, mixed (a bass unit, a centre fill).
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Speaker {
    /// What it plays: from this many Hz up to that many (0: no limit that way).
    pub band: [f32; 2],
    /// How loud, against the others (dB).
    pub level: f32,
    pub channel: Channel,
    /// Where it is in the cab (cab space, metres).
    pub at: [f32; 3],
    /// How much later it plays than the rest (ms): a rear fill's, so the ear places the music by the front
    /// speakers it hears first ([`assemble`]).
    #[serde(default)]
    pub delay: f32,
}

/// No limit that way (a band's).
const OPEN: f32 = 0.0;

const fn speaker(band: [f32; 2], level: f32, channel: Channel, at: [f32; 3]) -> Speaker {
    Speaker { band, level, channel, at, delay: 0.0 }
}

/// The Scania S (2016): a woofer low in each door, a tweeter in each A-pillar, and the roof's bass unit (its
/// grille, in the middle of the headliner behind the windscreen, has a patch each side of a seam: two
/// units). In the interior model (x = 0 the driver's seat, the cab's middle at x 0.683): the roof's is the
/// model's own grille (X 0.33 to 1.04, Y 0.99 to 1.02, Z -0.33 to -0.27); the doors' (X -0.38 and 1.75,
/// Y -0.72, Z -0.60) and the pillars' (X -0.33 and 1.70, Y 0.05, Z -0.97) are measured against its walls,
/// floor and windscreen from screenshots in the cab (it has no grille of its own there). The S cab places
/// the model at (-0.683, 2.705, -1.862) and turns on (0, 1.295, -2.246): cab space is the model's plus
/// (-0.683, 1.41, 0.384), which is also where the SDK says the driver's head is by default (the model's
/// origin is the head). Symmetric about the cab's middle, so a right-hand-drive S has them where they are.
///
/// A two-way pair each side and a bass box: the doors the body and the voices (80 Hz to 2 kHz), the
/// pillars the highs from 2 kHz (handed over low, so more of the voices come from up there, not the
/// knees: there's no speaker higher up for them), the roof the bass under the doors' (both sides; that low,
/// where it is doesn't pull the picture up). The roof's two at -6 dB each: both play the bass both sides
/// share (most music's), so together they're one speaker's worth, not twice the rest's.
const SCANIA_S: &[Speaker] = &[
    speaker([80.0, 2000.0], 0.0, Channel::Left, [-1.065, 0.69, -0.216]),
    speaker([80.0, 2000.0], 0.0, Channel::Right, [1.065, 0.69, -0.216]),
    speaker([2000.0, OPEN], 0.0, Channel::Left, [-1.015, 1.46, -0.586]),
    speaker([2000.0, OPEN], 0.0, Channel::Right, [1.015, 1.46, -0.586]),
    speaker([OPEN, 80.0], -6.0, Channel::Both, [-0.185, 2.41, 0.084]),
    speaker([OPEN, 80.0], -6.0, Channel::Both, [0.185, 2.41, 0.084]),
];

/// The Scania R (2016): the S's interior models, all four (its speakers the same units), placed by the R's cabs
/// 0.156 lower and 0.001 further forward (`interior_r` at (-0.683, 2.549, -1.863) against the S's `interior_s`
/// at (-0.683, 2.705, -1.862); both cabs' `cabin` at (0, 1.295, -2.246)): the S's speakers, that much lower.
const SCANIA_R: &[Speaker] = &[
    speaker([80.0, 2000.0], 0.0, Channel::Left, [-1.065, 0.534, -0.217]),
    speaker([80.0, 2000.0], 0.0, Channel::Right, [1.065, 0.534, -0.217]),
    speaker([2000.0, OPEN], 0.0, Channel::Left, [-1.015, 1.304, -0.587]),
    speaker([2000.0, OPEN], 0.0, Channel::Right, [1.015, 1.304, -0.587]),
    speaker([OPEN, 80.0], -6.0, Channel::Both, [-0.185, 2.254, 0.083]),
    speaker([OPEN, 80.0], -6.0, Channel::Both, [0.185, 2.254, 0.083]),
];

/// The DAF XF, XG and XG+ (2021, and 2025's): a woofer low in each door's front, a tweeter on each A-pillar
/// over the dashboard, a unit at each end of the headliner's front where it meets the A-pillar, and one in
/// its middle. None has a grille of its own in the interior models:
/// all measured against them (x = 0 the driver's seat, the cab's middle at x 0.722, the side walls at -0.51
/// and 1.955, the floor at -1.2, the windscreen at -1.3) from screenshots in the cab: the doors' at X -0.41
/// and 1.85, Y -0.80, Z -0.62; the pillars' at X -0.36 and 1.80, Y -0.10, Z -1.12; the corners' at X -0.11
/// and 1.55, Y 0.33, Z -0.85; the middle's at Y 0.30, Z -0.88. The front of the cab is the same in all four
/// interiors (only the roof and the back differ), so one set fits them all. Every cab places the model at
/// (-0.722, 2.72, -1.896), and the SDK has the driver's head by default at (-0.722, 1.696, 0.686): the model's
/// origin, as in the Scania. Symmetric about the cab's middle, so a right-hand-drive DAF has them too.
///
/// A three-way front stage and a centre: the doors (low, by the knees) only the mid-bass, so voices don't
/// come from down there; the corners (high and forward, by the ears and the windscreen) the midrange, the
/// voices and most of the music, out in front at eye level; the pillars the highs, close by the corners;
/// each side's frequencies from one place. The middle a centre fill, both sides mixed, in the voices' range
/// and quieter, holding the middle of the picture there (the driver sits to the left). (With the bands
/// overlapping, the same sound came from five places, smeared, and too much of the bass.)
const DAF_2021: &[Speaker] = &[
    speaker([60.0, 400.0], 0.0, Channel::Left, [-1.128, 0.896, 0.066]),
    speaker([60.0, 400.0], 0.0, Channel::Right, [1.128, 0.896, 0.066]),
    speaker([4000.0, OPEN], 0.0, Channel::Left, [-1.078, 1.596, -0.434]),
    speaker([4000.0, OPEN], 0.0, Channel::Right, [1.078, 1.596, -0.434]),
    speaker([400.0, 4000.0], 0.0, Channel::Left, [-0.828, 2.026, -0.164]),
    speaker([400.0, 4000.0], 0.0, Channel::Right, [0.828, 2.026, -0.164]),
    speaker([250.0, 4000.0], -6.0, Channel::Both, [0.0, 1.996, -0.194]),
];

/// The DAF XD (day, sleeper and high sleeper cabs): the XF and XG's units (owner 2026-10-26: "same units as XG/XF but
/// lower"). Its interior models are the 2021's from the roof down: the doors, the A-pillars, the windscreen and the
/// overhead console's front the corners' units sit on, the same in the models to 2 mm (its roof is lower, and its
/// sunshield's 5 cm lower), so the 2021's speakers are where they are in the model. Its cabs place the model at
/// (-0.722, 2.310, -1.898) and turn on (0, 1.383, -2.359): cab space is the model's plus (-0.722, 0.927, 0.461), the
/// 2021's speakers 0.769 lower and 0.225 further forward (as the head is: the model's origin).
const DAF_XD: &[Speaker] = &[
    speaker([60.0, 400.0], 0.0, Channel::Left, [-1.128, 0.127, -0.159]),
    speaker([60.0, 400.0], 0.0, Channel::Right, [1.128, 0.127, -0.159]),
    speaker([4000.0, OPEN], 0.0, Channel::Left, [-1.078, 0.827, -0.659]),
    speaker([4000.0, OPEN], 0.0, Channel::Right, [1.078, 0.827, -0.659]),
    speaker([400.0, 4000.0], 0.0, Channel::Left, [-0.828, 1.257, -0.389]),
    speaker([400.0, 4000.0], 0.0, Channel::Right, [0.828, 1.257, -0.389]),
    speaker([250.0, 4000.0], -6.0, Channel::Both, [0.0, 1.227, -0.419]),
];

/// The Iveco S-Way (its Active Space cab, every interior): two units behind the slatted panel low in the
/// front of each door, a big woofer and a tweeter above and behind it, and nothing else (the slots along the
/// dashboard's top are the windscreen's heaters). In the interior model (x = 0 the driver's seat, the cab's
/// middle at x 0.738) the slats are the detail texture's grey striped strip (X -0.324 and 1.80), tallest at
/// their front (Z -0.75 to -0.45, Y -0.96 to -0.72) and narrowing back and up to a thin strip: the woofer at
/// the middle of the front part (Z -0.60, Y -0.85), the tweeter over the panel,
/// at its top edge as it starts to narrow (Z -0.40, Y -0.73; the edge is at -0.70 there). The cab places the
/// model at (-0.738, 2.708, -1.783) and turns on the chassis's (0, 1.024, -2.582): cab space is the model's
/// plus (-0.738, 1.684, 0.799), which is where the game says the driver's head is by default (checked in the
/// plugin's log). Symmetric about the cab's middle.
///
/// A two-way pair each side: the woofer everything up to 2 kHz (there's no bass unit), the tweeter the
/// highs from 2 kHz (as in the Scania and the Renault).
const IVECO_SWAY: &[Speaker] = &[
    speaker([OPEN, 2000.0], 0.0, Channel::Left, [-1.062, 0.834, 0.199]),
    speaker([OPEN, 2000.0], 0.0, Channel::Right, [1.062, 0.834, 0.199]),
    speaker([2000.0, OPEN], 0.0, Channel::Left, [-1.062, 0.954, 0.399]),
    speaker([2000.0, OPEN], 0.0, Channel::Right, [1.062, 0.954, 0.399]),
];

/// The Volvo FH (2021's and 2024's, every interior): a woofer low in the front of each door and a panel each
/// side of the back wall above the bed (owner 2026-10-22, from their screenshots in the cab: no tweeters, no
/// centre; the plates by the seats, the camera housing and the trim over it aren't speakers, nor the phone's
/// over the driver's door). Both are the interior model's own: the doors' round 21 cm grilles (X -0.353 and
/// 1.737, Y -1.12 to -0.90, Z -1.057 to -0.849, the right one apart in the model, the left one joined to the
/// panel over it), the back wall's 27 x 17 cm panels in the details texture (X -0.243 to 0.023 and 1.361 to
/// 1.627, Y -0.160 to 0.012, Z 0.857), the same in every interior (x = 0 the driver's seat, the cab's middle at
/// x 0.692); the XXL's back wall is 0.25 further back, and the 100 years edition's has a decal there. Every
/// cab places the model at (-0.718, 2.692, -1.894) and the chassis turns it on (0, 1.246, -2.323): cab space is
/// the model's plus (-0.718, 1.446, 0.429). Symmetric about the cab's middle.
///
/// The doors the stage: everything from 80 Hz up, the voices and the highs all from in front (there's no
/// tweeter); the back wall's pair the bass under them, below 80 Hz, where which way it comes from can't be heard,
/// so it doesn't pull the music behind the driver's head. Each side's frequencies from one place, as in the
/// others. (Tried first, owner 2026-10-22: both full range, the back 6 dB down; then the back up to 800 Hz and
/// the doors from 150 Hz, which doubled the low mids: "too muddy, vocals and trebles too faded".)
const VOLVO_FH: &[Speaker] = &[
    speaker([80.0, OPEN], 0.0, Channel::Left, [-1.045, 0.436, -0.524]),
    speaker([80.0, OPEN], 0.0, Channel::Right, [1.045, 0.436, -0.524]),
    speaker([OPEN, 80.0], 0.0, Channel::Left, [-0.802, 1.372, 1.286]),
    speaker([OPEN, 80.0], 0.0, Channel::Right, [0.802, 1.372, 1.286]),
];

/// The MAN TGX (2020's: GM, GN and GX cabs, every interior): three a side (owner 2026-10-26, in the cab), each a
/// round unit behind a shaped fabric grille: a woofer low in the door's front, a smaller unit high in it, and a
/// tweeter on the A-pillar just above the blind-spot sensor's lamp. In the interior models (x = 0 the driver's
/// seat, the cab's middle at x 0.734, the same in all six, the UK ones mirrored) the doors' grilles are the details
/// texture's perforated fabric, each showing its unit's disc through it, placed from the disc (its middle mapped
/// through the grille's texture coordinates, to 4 mm): the woofer 12.5 cm across at X -0.275 and 1.757, Y -1.050,
/// Z -0.736, the high unit 7.4 cm at X -0.308 and 1.776, Y -0.676, Z -0.675 (owner's circle). The tweeters' 5 x 6
/// cm grilles are in the mirrors' interior parts (every one of the ten, the lamp beside them), at (±1.058, 0.043,
/// -0.103) from the interior's `mirror` locator (0.734, 0.049, -0.835). Every cab places the model at (-0.734,
/// 2.804, -1.774) and every chassis turns it on (0, 1.715, -2.310) (the cabs differ only in the roof): cab space is
/// the model's plus (-0.734, 1.089, 0.536). Symmetric about the cab's middle.
///
/// A three-way front, as the DAF's: the woofers the bass and the low mids, up to 400 Hz; the high door units the
/// voices and most of the music, 400 Hz to 4 kHz; the pillars the highs.
const MAN_TGX_2020: &[Speaker] = &[
    speaker([OPEN, 400.0], 0.0, Channel::Left, [-1.016, 0.039, -0.200]),
    speaker([OPEN, 400.0], 0.0, Channel::Right, [1.016, 0.039, -0.200]),
    speaker([400.0, 4000.0], 0.0, Channel::Left, [-1.042, 0.413, -0.139]),
    speaker([400.0, 4000.0], 0.0, Channel::Right, [1.042, 0.413, -0.139]),
    speaker([4000.0, OPEN], 0.0, Channel::Left, [-1.058, 1.181, -0.402]),
    speaker([4000.0, OPEN], 0.0, Channel::Right, [1.058, 1.181, -0.402]),
];

/// The Volvo FH16 (2009's, the FH3: every cab, both interior models): one unit in each door beside the seat and
/// nothing else (owner 2026-10-26, in the cab), behind the door's 17 cm round grille (X -0.400 and 1.839, Y -0.643, Z
/// -0.051, in the interior models: x = 0 the driver's seat, the pair's middle at x 0.72). Every cab places the model
/// at (-0.710, 2.618, -2.056) and every chassis turns it on (0, 1.363, -2.387): cab space is the model's plus (-0.710,
/// 1.255, 0.331). Each plays its side's whole sound.
const VOLVO_FH16_2009: &[Speaker] = &[
    speaker([OPEN, OPEN], 0.0, Channel::Left, [-1.120, 0.612, 0.280]),
    speaker([OPEN, OPEN], 0.0, Channel::Right, [1.120, 0.612, 0.280]),
];

/// The Renault Magnum (every interior, one model: owner 2026-10-26, in the cab): its main unit in each roof corner
/// over the side window, and a tweeter under the overhead console's front either side of the mirror. In the interior
/// model (x = 0 the driver's seat, the cab's middle at x 0.66; the UK one mirrored): the roof's grilles in the
/// dashboard texture, a 15.3 cm driver drawn in each (placed from its middle through the grille's texture
/// coordinates: X -0.386 and 1.718, Y 0.983, Z -0.235); the tweeters' 4 cm discs facing down (X 0.230 and 1.088, Y
/// 0.447, Z -0.720). Its cab places the model at (-0.687, 2.781, -1.726) and turns on (0, 1.572, -2.134): cab space
/// is the model's plus (-0.687, 1.209, 0.408). A two-way pair each side: the roof's units to 2 kHz, the tweeters above.
const RENAULT_MAGNUM: &[Speaker] = &[
    speaker([OPEN, 2000.0], 0.0, Channel::Left, [-1.052, 2.192, 0.173]),
    speaker([OPEN, 2000.0], 0.0, Channel::Right, [1.052, 2.192, 0.173]),
    speaker([2000.0, OPEN], 0.0, Channel::Left, [-0.429, 1.656, -0.312]),
    speaker([2000.0, OPEN], 0.0, Channel::Right, [0.429, 1.656, -0.312]),
];

/// The Iveco Hi-Way (its one cab, one interior model: owner 2026-10-26, in the cab, "the same layout" as the
/// Stralis's, its top unit larger): a unit behind each end of the overhead console (a perforated grille in the
/// dashboard icons texture: X -0.262 and 1.779, Y 0.441, Z -0.418) and one high on each wall beside its lamp (the
/// oval honeycomb grille behind the lamp's: X -0.224 and 1.741, Y 1.0, Z 0.097 and 0.134), in the interior model (x = 0
/// the driver's seat, the cab's middle at x 0.757; the UK one mirrored). Its cab places the model at (-0.745, 2.563,
/// -1.688) and turns on (0, 1.265, -2.111): cab space is the model's plus (-0.745, 1.298, 0.423), each pair placed
/// about the cab's middle. As the Stralis: the console's units to 4 kHz, the top units above.
const IVECO_HIWAY: &[Speaker] = &[
    speaker([OPEN, 4000.0], 0.0, Channel::Left, [-1.020, 1.739, 0.005]),
    speaker([OPEN, 4000.0], 0.0, Channel::Right, [1.020, 1.739, 0.005]),
    speaker([4000.0, OPEN], 0.0, Channel::Left, [-0.983, 2.300, 0.539]),
    speaker([4000.0, OPEN], 0.0, Channel::Right, [0.983, 2.300, 0.539]),
];

/// The Scania R (2009's) and the Streamline (every cab, one interior model: owner 2026-10-26, in the cab, two a side):
/// a woofer in each door beside the seat, behind its mesh grille (the `mesh` piece, 28 x 32 cm, X -0.362 and 1.725,
/// Y -0.645, Z 0.0), and a tweeter in each A-pillar at the mirror's height (the dashboard texture's 5.7 cm round mesh,
/// X -0.371 and 1.734, Y -0.144, Z -0.779), in the interior model (x = 0 the driver's seat, the units' middle at x
/// 0.682; the high roofs' round dark discs behind the head have no grille: lamps). Every cab places the model at
/// (-0.673, 2.636, -2.368) and turns on (0, 1.422, -2.680): cab space is the model's plus (-0.673, 1.214, 0.312), the
/// units' middle a centimetre right of the cab's (each pair placed about the cab's).
///
/// A two-way pair each side, as the S's doors and pillars: the doors everything up to 2 kHz (no bass unit), the
/// pillars the highs.
const SCANIA_R_2009: &[Speaker] = &[
    speaker([OPEN, 2000.0], 0.0, Channel::Left, [-1.044, 0.569, 0.312]),
    speaker([OPEN, 2000.0], 0.0, Channel::Right, [1.044, 0.569, 0.312]),
    speaker([2000.0, OPEN], 0.0, Channel::Left, [-1.053, 1.070, -0.467]),
    speaker([2000.0, OPEN], 0.0, Channel::Right, [1.053, 1.070, -0.467]),
];

/// The Mercedes-Benz Actros (2014's: StreamSpace, BigSpace, GigaSpace and the 8x4's cab, every interior): three a
/// side (owner 2026-10-26, in the cab, both trims): a woofer low in the front of each door, a tweeter in each
/// A-pillar, and a round unit high over each door in the overhead shelf's side. All the interior model's own (x = 0
/// the driver's seat, the cab's middle at x 0.718): the doors' decal grille, its round driver drawn in it (14 cm,
/// placed from its middle through the grille's texture coordinates) at X -0.365 and 1.801, Y -0.659, Z -0.606; the
/// pillars' 4.6 cm mesh discs in the details texture at X -0.353 and 1.784, Y -0.095, Z -0.785; the overhead pair's
/// 12.4 cm honeycomb grilles at Y 0.520, the right one at X 1.773, Z -0.275, the left at X -0.340, Z -0.128 (the
/// model has the driver's 14.7 cm further back), in every roof's part. Every cab places the model at (-0.718, 2.822,
/// -1.954) and turns it on (0, 1.219, -2.502): cab space is the model's plus (-0.718, 1.603, 0.548).
///
/// A three-way front, as the DAF's (owner's approval): the woofers to 400 Hz, the overhead pair the voices from up
/// beside the head, 400 Hz to 4 kHz, the pillars the highs.
const ACTROS_2014: &[Speaker] = &[
    speaker([OPEN, 400.0], 0.0, Channel::Left, [-1.083, 0.944, -0.058]),
    speaker([OPEN, 400.0], 0.0, Channel::Right, [1.083, 0.944, -0.058]),
    speaker([400.0, 4000.0], 0.0, Channel::Left, [-1.058, 2.123, 0.420]),
    speaker([400.0, 4000.0], 0.0, Channel::Right, [1.055, 2.123, 0.273]),
    speaker([4000.0, OPEN], 0.0, Channel::Left, [-1.068, 1.508, -0.237]),
    speaker([4000.0, OPEN], 0.0, Channel::Right, [1.068, 1.508, -0.237]),
];

/// The speakers of the truck with this SDK model id (with or without its `vehicle.` prefix), or `None` for a
/// truck whose speakers aren't known (its sound plays as it is).
pub fn in_truck(id: &str) -> Option<&'static [Speaker]> {
    match id.strip_prefix("vehicle.").unwrap_or(id) {
        // The S BEV: the S's cab, its own interior the S's but for the dashboard and seats (doors, pillars, roof
        // grille alike), turned on the same point (owner 2026-10-26: "same cab with S, no difference").
        "scania.s_2016" | "scania.s_2024e" => Some(SCANIA_S),
        "scania.r_2016" => Some(SCANIA_R),
        "iveco.sway" => Some(IVECO_SWAY),
        "iveco.hiway" => Some(IVECO_HIWAY),
        "man.tgx_2020" => Some(MAN_TGX_2020),
        "mercedes.actros2014" => Some(ACTROS_2014),
        "scania.r" | "scania.streamline" => Some(SCANIA_R_2009),
        "renault.magnum" => Some(RENAULT_MAGNUM),
        // The FH16 2012 (the FH4): the FH 2021's doors and back wall, its cabs placing them alike (owner 2026-10-26:
        // "FH4 matches FH5").
        "volvo.fh_2021" | "volvo.fh_2024" | "volvo.fh16_2012" => Some(VOLVO_FH),
        "volvo.fh16" => Some(VOLVO_FH16_2009),
        // A mod's DAF XG+ ("DAF 2021 Jasper") keeps the 2021's cab: its head sits 2 cm lower, its seat the same.
        // The XF Electric: the 2021 XF's cab and interior (doors, pillars, overhead console alike), placed alike.
        "daf.2021" | "new_daf.2021" | "daf.xf_electric" => Some(DAF_2021),
        "daf.xd" => Some(DAF_XD),
        _ => None,
    }
}

/// What a speaker of a truck's part does where it sits, which sets what it plays ([`assemble`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Low in a door: the body of the sound (and the bass, with nothing behind for it).
    Door,
    /// In an A-pillar: the highs.
    Pillar,
    /// A pair high and forward, over the windscreen: the voices, out in front at eye level (the DAF's corners).
    Overhead,
    /// In the middle: both sides mixed, quieter, holding the middle of the picture.
    Centre,
    /// Behind the head (a rear wall): its biggest units the bass, its smaller ones a quiet fill ([`assemble`]).
    Rear,
}

/// A speaker of a truck's part, in the part's model's own space (metres, x right, y up, z back).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    pub role: Role,
    pub at: [f32; 3],
    /// How wide it is (its ring, cm), where that decides what it plays (a rear wall's units); 0 where its role
    /// alone does.
    pub size: f32,
}

const fn unit(role: Role, at: [f32; 3]) -> Unit {
    Unit { role, at, size: 0.0 }
}

/// A rear wall's unit, `size` cm across.
const fn rear(size: f32, at: [f32; 3]) -> Unit {
    Unit { role: Role::Rear, at, size }
}

/// The speakers of the parts whose models (`.pmd`, as the parts' definitions name them) are these; a name
/// ending in `*` stands for every model starting so.
struct PartModels {
    models: &'static [&'static str],
    units: &'static [Unit],
}

/// The Scania PGR's parts (JuseeTV's mod, `scania.5s`), measured in their models (`examples/cab_model`, its
/// round shapes and speaker materials) and confirmed by the owner's screenshots in the cab (2026-10-24). Each
/// sits at the interior's locator for its slot, and every interior of the truck has those slots.
///
/// - Door panels: the tufted leather and tartan ones a Pioneer unit (its grille: the model's round Hertz
///   texture), the others the truck's stock unit, a 27 x 30 cm cloth grille in the same spot.
/// - A-pillars: a tweeter in every one, in the same spot (all 35 models).
/// - Top console: the centre fill in the middle of every one but the Classic and Highline ones; Dark
///   Diamond's instead has a 16 cm unit at each end, facing down.
/// - Rear walls (each unit's ring measured): JS Customs three a side, 17.8 and 13.1 cm and a 4.7 cm tweeter; V8
///   Power two a side, 17.3 and 13.4 cm; Dark Diamond two a side of 17.3 cm (one high, one lower in); Lights +
///   speakers one a side of 17.3 cm (the pair nearer the middle are its lights).
const SCANIA_PGR: &[PartModels] = &[
    PartModels {
        models: &[
            "/vehicle/truck/upgrade/doorpanel/scania_rcab_5s/doorpanels_tufted.pmd",
            "/vehicle/truck/upgrade/doorpanel/scania_rcab_5s/doorpanels_tufted_pluche_*",
        ],
        units: &[unit(Role::Door, [-0.212, -0.056, 0.328]), unit(Role::Door, [1.919, -0.056, 0.328])],
    },
    PartModels {
        models: &["/vehicle/truck/upgrade/doorpanel/scania_rcab_5s/doorpanels_tartan.pmd"],
        units: &[unit(Role::Door, [-0.238, -0.064, 0.328]), unit(Role::Door, [1.946, -0.064, 0.328])],
    },
    PartModels {
        models: &[
            "/vehicle/truck/upgrade/doorpanel/scania_rcab_5s/doorpanels.pmd",
            "/vehicle/truck/upgrade/doorpanel/scania_rcab_5s/doorpanels_alcantara.pmd",
            "/vehicle/truck/upgrade/doorpanel/scania_rcab_5s/doorpanels_dark_diamond.pmd",
            "/vehicle/truck/upgrade/doorpanel/scania_rcab_5s/doorpanels_pluche.pmd",
            "/vehicle/truck/upgrade/doorpanel/scania_rcab_5s/doorpanels_silver_g.pmd",
            "/vehicle/truck/upgrade/doorpanel/scania_rcab_5s/doorpanels_super70.pmd",
        ],
        units: &[unit(Role::Door, [-0.262, -0.078, 0.317]), unit(Role::Door, [1.971, -0.078, 0.317])],
    },
    PartModels {
        models: &["/vehicle/truck/upgrade/apilar/scania_rcab_5s/pilar_a*"],
        units: &[unit(Role::Pillar, [-2.011, -0.144, 0.004]), unit(Role::Pillar, [0.076, -0.144, 0.004])],
    },
    PartModels {
        models: &[
            "/vehicle/truck/upgrade/topconsole/scania_rcab_5s/bedconsole.pmd",
            "/vehicle/truck/upgrade/topconsole/scania_rcab_5s/bedconsole_custom.pmd",
            "/vehicle/truck/upgrade/topconsole/scania_rcab_5s/bedconsole_n.pmd",
            "/vehicle/truck/upgrade/topconsole/scania_rcab_5s/bedconsole_n_custom.pmd",
        ],
        units: &[unit(Role::Centre, [0.019, -0.106, -0.278])],
    },
    PartModels {
        models: &["/vehicle/truck/upgrade/topconsole/scania_rcab_5s/bedconsole_f.pmd"],
        units: &[unit(Role::Centre, [0.019, -0.134, -0.088])],
    },
    PartModels {
        models: &["/vehicle/truck/upgrade/topconsole/scania_rcab_5s/bedconsole_dark_diamond.pmd"],
        units: &[unit(Role::Overhead, [-0.819, -0.163, -0.144]), unit(Role::Overhead, [0.858, -0.163, -0.144])],
    },
    PartModels {
        models: &["/vehicle/truck/upgrade/rearwall/scania_rcab_5s/rearwall_interior_custom_7.pmd"],
        units: &[
            rear(17.8, [-0.844, 0.021, -0.060]),
            rear(17.8, [0.877, 0.021, -0.060]),
            rear(13.1, [-0.672, 0.020, -0.058]),
            rear(13.1, [0.704, 0.020, -0.058]),
            rear(4.7, [-0.565, 0.020, -0.055]),
            rear(4.7, [0.598, 0.020, -0.055]),
        ],
    },
    PartModels {
        models: &["/vehicle/truck/upgrade/rearwall/scania_rcab_5s/rearwall_interior_v8.pmd"],
        units: &[
            rear(17.3, [-0.857, 0.124, -0.060]),
            rear(17.3, [0.896, 0.124, -0.060]),
            rear(13.4, [-0.664, 0.124, -0.060]),
            rear(13.4, [0.703, 0.124, -0.060]),
        ],
    },
    PartModels {
        models: &["/vehicle/truck/upgrade/rearwall/scania_rcab_5s/rearwall_interior_dark_diamond.pmd"],
        units: &[
            rear(17.3, [-0.857, 0.087, -0.161]),
            rear(17.3, [0.896, 0.087, -0.161]),
            rear(17.3, [-0.578, -0.258, -0.161]),
            rear(17.3, [0.561, -0.258, -0.161]),
        ],
    },
    PartModels {
        models: &["/vehicle/truck/upgrade/rearwall/scania_rcab_5s/rearwall_interior_custom_4.pmd"],
        units: &[rear(17.3, [-0.857, 0.124, -0.060]), rear(17.3, [0.896, 0.124, -0.060])],
    },
];

/// The Renault T's interiors (owner 2026-10-26, in the cab), each its own model with its speakers' grilles in it
/// (x = 0 the driver's seat, the cab's middle at x 0.7; a right-hand-drive model the same mirrored about the
/// driver, its middle at x -0.7):
///
/// - Every interior: a tweeter at the top of each A-pillar, a disc of the details texture's hexagonal mesh (the
///   2013 to 2022 interiors' 6 cm at X -0.330 and 1.730, Y 0.026, Z -0.799; 2024's 5 cm at X -0.317 and 1.717,
///   Z -0.772), and a pair on the back wall over the bed, the same size, a metal bar between them (17 x 9 cm panels
///   of the same mesh: X 0.288 and 1.112, Y -0.072, Z 0.825; 2024's at X 0.277 and 1.123, Y -0.075, Z 0.827).
/// - 2024's only: a big woofer low in the front of each door (the door-speaker material's flat panels, X -0.316
///   and 1.716, Y -1.234 to -0.811, Z -0.769 to -0.502). The older ones have none there.
///
/// A high and a low sleeper have the same speakers in their models (2024's `_slh` and `_sl` alike); the cab places
/// the model, and the game's default head is its origin (a low sleeper's logged at (-0.7, 1.387, 0.545), its
/// `interior` locator less its `cabin`). So the older interiors play from the back wall up to the pillars' 2 kHz
/// (owner 2026-10-26: as the truck does), 2024's from the doors with the back wall the bass ([`assemble`]).
const RENAULT_T: &[PartModels] = &[
    PartModels {
        models: &["/vehicle/truck/renault_t/interior/model.pmd"],
        units: &[
            unit(Role::Pillar, [-0.330, 0.026, -0.799]),
            unit(Role::Pillar, [1.730, 0.026, -0.799]),
            rear(17.0, [0.288, -0.072, 0.825]),
            rear(17.0, [1.112, -0.072, 0.825]),
        ],
    },
    PartModels {
        models: &["/vehicle/truck/renault_t/interior/uk/model_uk.pmd"],
        units: &[
            unit(Role::Pillar, [-1.730, 0.026, -0.799]),
            unit(Role::Pillar, [0.330, 0.026, -0.799]),
            rear(17.0, [-1.112, -0.072, 0.825]),
            rear(17.0, [-0.288, -0.072, 0.825]),
        ],
    },
    PartModels {
        models: &["/vehicle/truck/renault_t/interior/model_24/*"],
        units: &[
            unit(Role::Door, [-0.316, -1.022, -0.636]),
            unit(Role::Door, [1.716, -1.022, -0.636]),
            unit(Role::Pillar, [-0.317, 0.026, -0.772]),
            unit(Role::Pillar, [1.717, 0.026, -0.772]),
            rear(17.0, [0.277, -0.075, 0.827]),
            rear(17.0, [1.123, -0.075, 0.827]),
        ],
    },
    // The E-Tech T (owner 2026-10-26: "confirmed 2024's interior"): the 2024 interior, moved in its own model by
    // (0.005, 0.136, 0.043) (its grilles, discs and panels all), its cab moving it back as much: in the cab, a
    // 2024 low sleeper's speakers.
    PartModels {
        models: &["/vehicle/truck/renault_etech_t/interior/model/etech.pmd"],
        units: &[
            unit(Role::Door, [-0.311, -0.886, -0.593]),
            unit(Role::Door, [1.721, -0.886, -0.593]),
            unit(Role::Pillar, [-0.312, 0.161, -0.729]),
            unit(Role::Pillar, [1.722, 0.161, -0.729]),
            rear(17.0, [0.283, 0.061, 0.870]),
            rear(17.0, [1.128, 0.061, 0.870]),
        ],
    },
    PartModels {
        models: &["/vehicle/truck/renault_etech_t/interior/uk/etech_uk.pmd"],
        units: &[
            unit(Role::Door, [-1.721, -0.886, -0.593]),
            unit(Role::Door, [0.311, -0.886, -0.593]),
            unit(Role::Pillar, [-1.722, 0.161, -0.729]),
            unit(Role::Pillar, [0.312, 0.161, -0.729]),
            rear(17.0, [-1.128, 0.061, 0.870]),
            rear(17.0, [-0.283, 0.061, 0.870]),
        ],
    },
    PartModels {
        models: &["/vehicle/truck/renault_t/interior/uk_24/*"],
        units: &[
            unit(Role::Door, [-1.716, -1.022, -0.636]),
            unit(Role::Door, [0.316, -1.022, -0.636]),
            unit(Role::Pillar, [-1.717, 0.026, -0.772]),
            unit(Role::Pillar, [0.317, 0.026, -0.772]),
            rear(17.0, [-1.123, -0.075, 0.827]),
            rear(17.0, [-0.277, -0.075, 0.827]),
        ],
    },
];

/// The Mercedes-Benz Actros (2009's) interior (owner 2026-10-26, in the cab: two a side, no tweeters): a woofer low
/// in the front of each door and a unit at each front corner of the overhead shelf, both the details texture's round
/// honeycomb grille, in both trims (x = 0 the driver's seat, the units' middle at x 0.775; the UK model mirrored):
/// the doors' 14 cm at X -0.370 and 1.920, Y -0.746, Z -0.692; the shelf's 12 cm at X -0.245 and 1.795, Y 0.43, Z
/// -0.454. (The panel high in each back corner is plain plastic in the model, no grille: left out.) Its high roof's
/// cab places the model 0.295 higher than the others (`interior` at (-0.748, 2.757, -1.876) against 2.462, on the
/// chassis's (0, 1.114, -2.273)), so it's placed by its interior, from the game's default head, as the Renault T.
/// Doors and the shelf's pair: the doors the low end, the pair the rest from up in front ([`assemble`]).
const ACTROS_2009: &[PartModels] = &[
    PartModels {
        models: &["/vehicle/truck/mercedes_actros_2009/interior/model.pmd"],
        units: &[
            unit(Role::Door, [-0.370, -0.746, -0.692]),
            unit(Role::Door, [1.920, -0.746, -0.692]),
            unit(Role::Overhead, [-0.245, 0.435, -0.454]),
            unit(Role::Overhead, [1.795, 0.429, -0.454]),
        ],
    },
    PartModels {
        models: &["/vehicle/truck/mercedes_actros_2009/interior/uk/model.pmd"],
        units: &[
            unit(Role::Door, [-1.920, -0.746, -0.692]),
            unit(Role::Door, [0.370, -0.746, -0.692]),
            unit(Role::Overhead, [-1.795, 0.429, -0.454]),
            unit(Role::Overhead, [0.245, 0.435, -0.454]),
        ],
    },
];

/// The DAF XF (105's: owner 2026-10-26, in the cab), by its interiors' definitions, its cabs' interiors sharing one
/// model with a part for each layout: a tweeter in each A-pillar in every cab (a 6.5 cm round piece, no grille in its
/// texture: X -0.349 and 1.755, Y -0.168, Z -0.935), and no door speakers. The Super Space Cab's (`standard`,
/// `exclusive`: the `super_space` part) two 14.6 cm units a side on the overhead shelf's side walls, one in front (Z
/// -0.339) and one at the head (Z 0.095), at Y 0.389 (X -0.238 and 1.647); the Space Cab and Space Cab Plus's
/// (`space*`: the `space` part) one 14 cm unit on the overhead console beside the tachograph (X -0.196, Y 0.394, Z
/// -0.519). In the interior model (x = 0 the driver's seat, the pillars' middle at x 0.703; the UK one mirrored),
/// every cab placing it at (-0.706, 2.678, -1.860) on the chassis's (0, 1.261, -2.246). The Super Space's front pair
/// the voices over its rear pair's low end, the pillars the highs; the Space cabs' one unit everything but the highs,
/// both sides ([`assemble`]).
const DAF_XF_105: &[PartModels] = &[
    PartModels {
        models: &[
            "/def/vehicle/truck/daf.xf/interior/standard.sii",
            "/def/vehicle/truck/daf.xf/interior/exclusive.sii",
        ],
        units: &[
            unit(Role::Pillar, [-0.349, -0.168, -0.935]),
            unit(Role::Pillar, [1.755, -0.168, -0.935]),
            unit(Role::Overhead, [-0.238, 0.389, -0.339]),
            unit(Role::Overhead, [1.647, 0.389, -0.339]),
            rear(14.6, [-0.242, 0.389, 0.095]),
            rear(14.6, [1.651, 0.389, 0.095]),
        ],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/daf.xf/interior/standard_uk.sii",
            "/def/vehicle/truck/daf.xf/interior/exclusive_uk.sii",
        ],
        units: &[
            unit(Role::Pillar, [-1.755, -0.168, -0.935]),
            unit(Role::Pillar, [0.349, -0.168, -0.935]),
            unit(Role::Overhead, [-1.647, 0.389, -0.339]),
            unit(Role::Overhead, [0.238, 0.389, -0.339]),
            rear(14.6, [-1.651, 0.389, 0.095]),
            rear(14.6, [0.242, 0.389, 0.095]),
        ],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/daf.xf/interior/space.sii",
            "/def/vehicle/truck/daf.xf/interior/space_ex.sii",
            "/def/vehicle/truck/daf.xf/interior/space_plus.sii",
            "/def/vehicle/truck/daf.xf/interior/space_plus_ex.sii",
        ],
        units: &[
            unit(Role::Pillar, [-0.349, -0.168, -0.935]),
            unit(Role::Pillar, [1.755, -0.168, -0.935]),
            unit(Role::Centre, [-0.196, 0.394, -0.519]),
        ],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/daf.xf/interior/space_uk.sii",
            "/def/vehicle/truck/daf.xf/interior/space_ex_uk.sii",
            "/def/vehicle/truck/daf.xf/interior/space_plus_uk.sii",
            "/def/vehicle/truck/daf.xf/interior/space_plus_ex_uk.sii",
        ],
        units: &[
            unit(Role::Pillar, [-1.755, -0.168, -0.935]),
            unit(Role::Pillar, [0.349, -0.168, -0.935]),
            unit(Role::Centre, [0.196, 0.394, -0.519]),
        ],
    },
];

/// The DAF XF Euro 6, by its interiors' definitions (owner 2026-10-26, in the cab), its cabs' interiors sharing one
/// model with a roof part for each layout: in every cab a woofer low in each door's front (no grille in its texture:
/// the door trim's front panel, X -0.274 and 1.826, Y -0.735, Z -0.757) and a tweeter in each A-pillar (X -0.311 and
/// 1.863, Y -0.332, Z -0.893); the Super Space Cab's (`standard`, `exclusive`, `stan_fl`, `excl_fl`: `roof_hi`) two
/// 13 cm units a side on the overhead shelf's side walls, in front (Z -0.378) and at the head (Z 0.159), at Y 0.308
/// (X -0.208 and 1.776); the Space and Space with spoiler cabs' (`*_low`, `*_l_fl`: `roof_low`) one on the overhead
/// console beside the tachograph (X -0.109, Y 0.292, Z -0.505). In the interior model (x = 0 the driver's seat, the
/// cab's middle at x 0.775; the UK one mirrored 1 cm over: as measured in it). Its EU and UK cabs place it at
/// other heights, so it goes by its interior, from the game's default head ([`assemble`]: the Super Space's shelf
/// units the voices in front and the bass at the head, the Space cabs' overhead unit a centre fill).
const DAF_XF_EURO6: &[PartModels] = &[
    PartModels {
        models: &[
            "/def/vehicle/truck/daf.xf_euro6/interior/standard.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/exclusive.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/stan_fl.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/excl_fl.sii",
        ],
        units: &[
            unit(Role::Door, [-0.274, -0.735, -0.757]),
            unit(Role::Door, [1.826, -0.735, -0.757]),
            unit(Role::Pillar, [-0.311, -0.332, -0.893]),
            unit(Role::Pillar, [1.863, -0.332, -0.894]),
            unit(Role::Overhead, [-0.208, 0.308, -0.378]),
            unit(Role::Overhead, [1.776, 0.308, -0.378]),
            rear(13.0, [-0.222, 0.308, 0.159]),
            rear(13.0, [1.790, 0.308, 0.159]),
        ],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/daf.xf_euro6/interior/standard_uk.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/exclusive_uk.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/stan_fl_uk.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/excl_fl_uk.sii",
        ],
        units: &[
            unit(Role::Door, [-1.816, -0.735, -0.757]),
            unit(Role::Door, [0.284, -0.735, -0.757]),
            unit(Role::Pillar, [-1.853, -0.332, -0.894]),
            unit(Role::Pillar, [0.321, -0.332, -0.893]),
            unit(Role::Overhead, [-1.767, 0.308, -0.378]),
            unit(Role::Overhead, [0.218, 0.308, -0.378]),
            rear(13.0, [-1.781, 0.308, 0.159]),
            rear(13.0, [0.231, 0.308, 0.159]),
        ],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/daf.xf_euro6/interior/standard_low.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/exclusive_low.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/stan_l_fl.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/excl_l_fl.sii",
        ],
        units: &[
            unit(Role::Door, [-0.274, -0.735, -0.757]),
            unit(Role::Door, [1.826, -0.735, -0.757]),
            unit(Role::Pillar, [-0.311, -0.332, -0.893]),
            unit(Role::Pillar, [1.863, -0.332, -0.894]),
            unit(Role::Centre, [-0.109, 0.292, -0.505]),
        ],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/daf.xf_euro6/interior/standard_low_uk.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/exclusive_low_uk.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/stan_l_fl_uk.sii",
            "/def/vehicle/truck/daf.xf_euro6/interior/excl_l_fl_uk.sii",
        ],
        units: &[
            unit(Role::Door, [-1.816, -0.735, -0.757]),
            unit(Role::Door, [0.284, -0.735, -0.757]),
            unit(Role::Pillar, [-1.853, -0.332, -0.894]),
            unit(Role::Pillar, [0.321, -0.332, -0.893]),
            unit(Role::Centre, [0.118, 0.292, -0.505]),
        ],
    },
];

/// The MAN TGX (the first one, 2007's), by its interiors' definitions (owner 2026-10-26, in the cab), its cabs'
/// interiors sharing one model with a roof part for each: in every cab a woofer low in each door's front (a 13 x 20 cm
/// grille in the dashboard texture, its driver drawn in its middle: X -0.322 and 1.897, Y -0.749, Z -0.617); the XXL's
/// (`standard`, `exclusive`: `hi_roof`) two small round mesh units high in the front of each side wall, a 6.7 cm one
/// over the side window (Y 0.346, Z -0.710, X -0.374 and 1.950) and a 5.5 cm one in the roof's corner over it (Y
/// 0.476, Z -0.887, X -0.381 and 1.956). (The A-pillar's rectangle the owner wasn't sure of is plain trim in the
/// model: left out.) In the interior model (x = 0 the driver's seat, the cab's middle at x 0.788; the UK one mirrored),
/// every cab placing it at (-0.795, 2.800, -1.788) on the chassis's (0, 1.388, -2.301). The XXL three-way, the corner
/// units the highs (as pillars are), the side-wall ones the voices; the XL and XLX's doors everything.
const MAN_TGX: &[PartModels] = &[
    PartModels {
        models: &[
            "/def/vehicle/truck/man.tgx/interior/standard.sii",
            "/def/vehicle/truck/man.tgx/interior/exclusive.sii",
        ],
        units: &[
            unit(Role::Door, [-0.322, -0.749, -0.617]),
            unit(Role::Door, [1.897, -0.749, -0.617]),
            unit(Role::Overhead, [-0.374, 0.346, -0.710]),
            unit(Role::Overhead, [1.950, 0.346, -0.710]),
            unit(Role::Pillar, [-0.381, 0.476, -0.887]),
            unit(Role::Pillar, [1.956, 0.476, -0.887]),
        ],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/man.tgx/interior/standard_uk.sii",
            "/def/vehicle/truck/man.tgx/interior/exclusive_uk.sii",
        ],
        units: &[
            unit(Role::Door, [0.322, -0.749, -0.617]),
            unit(Role::Door, [-1.897, -0.749, -0.617]),
            unit(Role::Overhead, [0.374, 0.346, -0.710]),
            unit(Role::Overhead, [-1.950, 0.346, -0.710]),
            unit(Role::Pillar, [0.381, 0.476, -0.887]),
            unit(Role::Pillar, [-1.956, 0.476, -0.887]),
        ],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/man.tgx/interior/standard_low.sii",
            "/def/vehicle/truck/man.tgx/interior/standard_mid.sii",
            "/def/vehicle/truck/man.tgx/interior/exclusive_low.sii",
            "/def/vehicle/truck/man.tgx/interior/exclusive_mid.sii",
        ],
        units: &[unit(Role::Door, [-0.322, -0.749, -0.617]), unit(Role::Door, [1.897, -0.749, -0.617])],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/man.tgx/interior/standard_uk_low.sii",
            "/def/vehicle/truck/man.tgx/interior/standard_uk_mid.sii",
            "/def/vehicle/truck/man.tgx/interior/exclusive_uk_low.sii",
            "/def/vehicle/truck/man.tgx/interior/exclusive_uk_mid.sii",
        ],
        units: &[unit(Role::Door, [0.322, -0.749, -0.617]), unit(Role::Door, [-1.897, -0.749, -0.617])],
    },
];

/// The MAN TGX Euro 6, by its interiors' models, one for each cab (and a 2019 one of each, the speakers alike; owner
/// 2026-10-26, in the cab): in every cab a woofer low in each door's front (a 12 x 25 cm grille in the details texture,
/// its driver drawn in its middle: X -0.305 and 1.791, Y -0.687, Z -0.646); the XL's a small unit in the roof's corner
/// over the side window too (a 5 cm round honeycomb grille: X -0.331 and 1.820, Y 0.269, Z -0.456), the XLX's and
/// XXL's none. (The A-pillar's rectangle is plain trim in the model, as in the first TGX: left out.) In the interior
/// models (x = 0 the driver's seat, the cab's middle at x 0.743; the UK ones mirrored). The XL's corner units the
/// highs (as pillars are) over the doors; the others' doors everything.
const MAN_TGX_EURO6: &[PartModels] = &[
    PartModels {
        models: &[
            "/vehicle/truck/man_tgx_euro6/interior/cab_xl.pmd",
            "/vehicle/truck/man_tgx_euro6/interior/cab_xl_2019.pmd",
        ],
        units: &[
            unit(Role::Door, [-0.305, -0.687, -0.646]),
            unit(Role::Door, [1.791, -0.687, -0.646]),
            unit(Role::Pillar, [-0.331, 0.269, -0.456]),
            unit(Role::Pillar, [1.820, 0.269, -0.456]),
        ],
    },
    PartModels {
        models: &[
            "/vehicle/truck/man_tgx_euro6/interior/uk/cab_xl.pmd",
            "/vehicle/truck/man_tgx_euro6/interior/uk/cab_xl_2019.pmd",
        ],
        units: &[
            unit(Role::Door, [0.305, -0.687, -0.646]),
            unit(Role::Door, [-1.791, -0.687, -0.646]),
            unit(Role::Pillar, [0.331, 0.269, -0.456]),
            unit(Role::Pillar, [-1.820, 0.269, -0.456]),
        ],
    },
    PartModels {
        models: &["/vehicle/truck/man_tgx_euro6/interior/cab_xlx*", "/vehicle/truck/man_tgx_euro6/interior/cab_xxl*"],
        units: &[unit(Role::Door, [-0.305, -0.687, -0.646]), unit(Role::Door, [1.791, -0.687, -0.646])],
    },
    PartModels {
        models: &[
            "/vehicle/truck/man_tgx_euro6/interior/uk/cab_xlx*",
            "/vehicle/truck/man_tgx_euro6/interior/uk/cab_xxl*",
        ],
        units: &[unit(Role::Door, [0.305, -0.687, -0.646]), unit(Role::Door, [-1.791, -0.687, -0.646])],
    },
];

/// The Renault Premium, by its interiors' definitions (owner 2026-10-26, in the cab): one unit a side, a square 15 cm
/// grille in the dashboard texture on the roof's side wall over the door, a lamp under it, and nothing else; in the
/// low roof's interiors (`standard`, `expensive_norm`: the `normal` part) 0.368 lower than the high roof's
/// (`standard_high`, `expensive`: `higher`). In the interior model (x = 0 the driver's seat, the cab's middle at x
/// 0.675; the UK one mirrored, its low roof's 4 mm over: as measured): X -0.314 and 1.667, Y 0.823 (high) or 0.455
/// (low), Z -0.130. Every cab placing it at (-0.720, 2.468, -1.757) on (0, 1.326, -2.216). Each plays its side's
/// whole sound.
// 0.318 is a measured x (its UK low roof's grille), not 1/π.
#[allow(clippy::approx_constant)]
const RENAULT_PREMIUM: &[PartModels] = &[
    PartModels {
        models: &[
            "/def/vehicle/truck/renault.premium/interior/standard_high.sii",
            "/def/vehicle/truck/renault.premium/interior/expensive.sii",
        ],
        units: &[unit(Role::Overhead, [-0.314, 0.823, -0.130]), unit(Role::Overhead, [1.667, 0.823, -0.130])],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/renault.premium/interior/standard.sii",
            "/def/vehicle/truck/renault.premium/interior/expensive_norm.sii",
        ],
        units: &[unit(Role::Overhead, [-0.314, 0.455, -0.130]), unit(Role::Overhead, [1.667, 0.455, -0.130])],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/renault.premium/interior/standard_high_uk.sii",
            "/def/vehicle/truck/renault.premium/interior/expensive_uk.sii",
        ],
        units: &[unit(Role::Overhead, [-1.667, 0.823, -0.130]), unit(Role::Overhead, [0.314, 0.823, -0.130])],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/renault.premium/interior/standard_uk.sii",
            "/def/vehicle/truck/renault.premium/interior/expensive_norm_uk.sii",
        ],
        units: &[unit(Role::Overhead, [-1.663, 0.455, -0.130]), unit(Role::Overhead, [0.318, 0.455, -0.130])],
    },
];

/// The Iveco Stralis, by its interiors' definitions (owner 2026-10-26, in the cab): a unit behind each end panel of
/// the overhead console, beside the radios (12 x 15 cm dark covers: X -0.320 and 1.807, Y 0.459, Z -0.462), and in
/// the high roofs' interiors (`standard`, `exclusive`) a small unit high on the passenger's wall alone, beside its
/// lamp (a 6 cm round mesh grille in the dashboard texture: X 1.906, Y 0.895, Z -0.099), none in the low roof's
/// (`standard_low`, `exclusive_low`: its ceiling under where it is). In the interior model (x = 0 the driver's seat,
/// the cab's middle at x 0.727; the UK one mirrored, the small unit on its own passenger's side), every cab placing it
/// at (-0.745, 2.563, -1.688) on (0, 1.309, -2.140). The passenger's side its highs from the small unit, its console
/// unit the rest; the driver's console unit its side's whole sound ([`assemble`], each side by its own units).
const IVECO_STRALIS: &[PartModels] = &[
    PartModels {
        models: &[
            "/def/vehicle/truck/iveco.stralis/interior/standard.sii",
            "/def/vehicle/truck/iveco.stralis/interior/exclusive.sii",
        ],
        units: &[
            unit(Role::Overhead, [-0.320, 0.459, -0.462]),
            unit(Role::Overhead, [1.807, 0.459, -0.462]),
            unit(Role::Pillar, [1.906, 0.895, -0.099]),
        ],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/iveco.stralis/interior/standard_uk.sii",
            "/def/vehicle/truck/iveco.stralis/interior/exclusive_uk.sii",
        ],
        units: &[
            unit(Role::Overhead, [-1.807, 0.459, -0.462]),
            unit(Role::Overhead, [0.320, 0.459, -0.462]),
            unit(Role::Pillar, [-1.906, 0.895, -0.099]),
        ],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/iveco.stralis/interior/standard_low.sii",
            "/def/vehicle/truck/iveco.stralis/interior/exclusive_low.sii",
        ],
        units: &[unit(Role::Overhead, [-0.320, 0.459, -0.462]), unit(Role::Overhead, [1.807, 0.459, -0.462])],
    },
    PartModels {
        models: &[
            "/def/vehicle/truck/iveco.stralis/interior/standard_low_uk.sii",
            "/def/vehicle/truck/iveco.stralis/interior/exclusive_low_uk.sii",
        ],
        units: &[unit(Role::Overhead, [-1.807, 0.459, -0.462]), unit(Role::Overhead, [0.320, 0.459, -0.462])],
    },
];

/// The trucks whose speakers come from their parts (their SDK model ids, with or without `vehicle.`).
pub fn by_parts(id: &str) -> bool {
    matches!(
        id.strip_prefix("vehicle.").unwrap_or(id),
        "scania.5s"
            | "renault.t"
            | "renault.etech_t"
            | "mercedes.actros"
            | "daf.xf"
            | "daf.xf_euro6"
            | "man.tgx"
            | "man.tgx_euro6"
            | "renault.premium"
            | "iveco.stralis"
    )
}

/// The speakers of a part with this model (its `.pmd`, as its definition names it), if it has any known.
pub fn of_model(model: &str) -> Option<&'static [Unit]> {
    let named = |name: &&str| match name.strip_suffix('*') {
        Some(start) => model.starts_with(start),
        None => model == *name,
    };
    SCANIA_PGR
        .iter()
        .chain(RENAULT_T)
        .chain(ACTROS_2009)
        .chain(DAF_XF_105)
        .chain(DAF_XF_EURO6)
        .chain(MAN_TGX)
        .chain(MAN_TGX_EURO6)
        .chain(RENAULT_PREMIUM)
        .chain(IVECO_STRALIS)
        .find(|part| part.models.iter().any(named))
        .map(|part| part.units)
}

/// A part's unit placed in the cab: its role and size ([`Unit`]), which of the truck's parts it's in (their
/// order's number), and where it is (cab space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub role: Role,
    pub size: f32,
    pub part: usize,
    pub at: [f32; 3],
}

/// What a placed unit plays, its role and the units fitted with it deciding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Job {
    Door,
    Pillar,
    Overhead,
    Centre,
    /// A rear wall's biggest units on its side: the bass.
    RearBass,
    /// A smaller one: the rear fill, quiet and late.
    RearFill,
    /// A tweeter's size (6 cm or less): the fill's highs.
    RearHigh,
}

/// How much later the rear fill plays than the front (ms): heard first, the front places the music (the precedence
/// effect), and the fill only makes the cab sound bigger, as a car's sound processor does it.
const FILL_DELAY: f32 = 15.0;

/// Speakers from the units fitted, placed in the cab: each plays its side's sound (its side of the cab's middle;
/// the centre both), and what it plays depends on what's fitted with it on its side (a unit on one side only hands
/// over only that side's sound), so a layout that isn't fitted plays no
/// part. The front stage places the music, each side's frequencies from one place as in the stock trucks: the doors
/// from the bottom (from 80 Hz with rear bass) up to the overhead pair's 400 Hz or else the pillars' 2 kHz, the
/// overhead pair to the pillars' 4 kHz (from the bottom, with nothing under it), the pillars the rest; with no pillars, whatever's highest plays to the top.
/// The centre fills 250 Hz to 4 kHz, 6 dB down; or, the front's only unit under the highs (no doors, overhead pair or
/// rear bass), plays all but the highs as loud as the rest. A rear wall's units by their sizes on each side: the biggest the bass
/// under 80 Hz (the FH's; units within 2 cm of it too, so a wall of one size is all bass), and its smaller ones, if
/// it has any, the rear fill (a sound engineer's, never the music itself from behind, which would put the singer
/// behind the head): a mid-size unit 80 Hz to 4 kHz (to the top without a tweeter) 10 dB down, a tweeter's size the
/// highs from 4 kHz 12 dB down, both [`FILL_DELAY`] late. With no doors, the rear wall's biggest units play what
/// the doors would (up to where the front above them takes over) and the fill starts there: the music from behind,
/// as a truck built so plays it (the Renault T's older interiors, owner 2026-10-26). Units sharing a job on a side,
/// 3 dB down each.
pub fn assemble(placed: &[Placed]) -> Vec<Speaker> {
    let channel = |unit: &Placed| match unit.role {
        Role::Centre => Channel::Both,
        _ if unit.at[0] < 0.0 => Channel::Left,
        _ => Channel::Right,
    };
    // A rear unit's job by its size among its wall's units on its side.
    let same_wall = |a: &Placed, b: &Placed| a.role == b.role && a.part == b.part && channel(a) == channel(b);
    let job = |unit: &Placed| match unit.role {
        Role::Door => Job::Door,
        Role::Pillar => Job::Pillar,
        Role::Overhead => Job::Overhead,
        Role::Centre => Job::Centre,
        Role::Rear => {
            let biggest =
                placed.iter().filter(|other| same_wall(unit, other)).map(|other| other.size).fold(0.0, f32::max);
            if unit.size >= biggest - 2.0 {
                Job::RearBass
            } else if unit.size <= 6.0 {
                Job::RearHigh
            } else {
                Job::RearFill
            }
        }
    };
    // What a side has, deciding its units' bands: its own units (and the centre, both sides'), so a unit on one side
    // only (the Iveco Stralis's one small unit, the passenger's) hands over only that side's sound. The centre goes by
    // both sides'.
    let on = |wanted: Job, side: Channel| {
        placed.iter().any(|unit| {
            let theirs = channel(unit);
            job(unit) == wanted && (side == Channel::Both || theirs == side || theirs == Channel::Both)
        })
    };
    let has =
        |side: Channel| (on(Job::RearBass, side), on(Job::Door, side), on(Job::Overhead, side), on(Job::Pillar, side));
    let alone_centre = {
        let (bass, doors, overhead, _) = has(Channel::Both);
        !doors && !overhead && !bass
    };
    let band = |unit: &Placed| {
        let (bass, doors, overhead, pillar) = has(channel(unit));
        // Where the units at the bottom of the front hand over to those above them.
        let low_top = if overhead {
            400.0
        } else if pillar {
            2000.0
        } else {
            OPEN
        };
        // The rear wall's main units: the bass under the doors, or with no doors, everything the doors would play.
        let rear_top = if doors { 80.0 } else { low_top };
        match job(unit) {
            Job::RearBass => [OPEN, rear_top],
            Job::Door => [if bass { 80.0 } else { OPEN }, low_top],
            // From the doors' top, or with none, the rear bass's; or, nothing under it, from the bottom (the Renault
            // Premium's one unit a side).
            Job::Overhead => [if doors || bass { 400.0 } else { OPEN }, if pillar { 4000.0 } else { OPEN }],
            Job::Pillar => [if overhead { 4000.0 } else { 2000.0 }, OPEN],
            // The front's only unit under the highs (no doors, no overhead pair, no rear bass: the DAF XF 105's Space
            // cabs' one overhead unit): all but the highs, as loud as the rest.
            Job::Centre if alone_centre => [OPEN, low_top],
            Job::Centre => [250.0, 4000.0],
            Job::RearFill => {
                let tweeter = placed.iter().any(|other| same_wall(unit, other) && job(other) == Job::RearHigh);
                [rear_top, if tweeter { 4000.0 } else { OPEN }]
            }
            Job::RearHigh => [4000.0, OPEN],
        }
    };
    placed
        .iter()
        .map(|unit| {
            let (side, done) = (channel(unit), job(unit));
            let sharing = placed.iter().filter(|other| job(other) == done && channel(other) == side).count();
            let shared = if sharing > 1 { (-100.0 * (sharing as f32).log10()).round() / 10.0 } else { 0.0 };
            let (level, delay) = match done {
                Job::Centre if alone_centre => (0.0, 0.0),
                Job::Centre => (-6.0, 0.0),
                Job::RearFill => (-10.0 + shared, FILL_DELAY),
                Job::RearHigh => (-12.0 + shared, FILL_DELAY),
                _ => (shared, 0.0),
            };
            Speaker { delay, ..speaker(band(unit), level, side, unit.at) }
        })
        .collect()
}

/// The driver's head in the cab (cab space, metres) and which way it faces (turns: heading to the left of
/// forward, pitch up), as the telemetry has it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Head {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub heading: f32,
    pub pitch: f32,
}

impl Head {
    pub fn of(telemetry: &westplay_shared::Telemetry) -> Self {
        Self {
            x: telemetry.head_x,
            y: telemetry.head_y,
            z: telemetry.head_z,
            heading: telemetry.head_heading,
            pitch: telemetry.head_pitch,
        }
    }

    /// The head turned `by` (turns, counterclockwise) more: the game's camera turned with the steering.
    pub fn turned(self, by: f32) -> Self {
        Self { heading: (self.heading + by).rem_euclid(1.0), ..self }
    }

    /// Whether it's a place in a cab and a way to face (numbers, of a cab's size).
    pub fn is_sound(&self) -> bool {
        [self.x, self.y, self.z].iter().all(|v| v.is_finite() && v.abs() < 10.0)
            && [self.heading, self.pitch].iter().all(|v| v.is_finite() && v.abs() <= 1.0)
    }
}

/// The player's tone controls, as a car stereo's (the cab's Settings › Sound): how much of the bass, the mids and
/// the treble, in dB each, [`Equalizer::MOST`] either way. One for every truck (owner 2026-10-22), on all the
/// sound the players play, before it's split to the speakers (a truck whose speakers aren't known too).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Equalizer {
    pub bass: i8,
    pub mid: i8,
    pub treble: i8,
}

impl Equalizer {
    /// How far each band goes, dB (owner 2026-10-22: a car stereo's tone range).
    pub const MOST: i8 = 6;

    /// Each band within [`Equalizer::MOST`] either way.
    pub fn clamped(self) -> Self {
        let clamp = |db: i8| db.clamp(-Self::MOST, Self::MOST);
        Self { bass: clamp(self.bass), mid: clamp(self.mid), treble: clamp(self.treble) }
    }
}

/// The music's spectrum as a player's page measures it (cab_sound.js: after the tone, before the speakers), for
/// the cab's Equalizer to draw behind its curve while it's open: each bar's loudness (0 to 255), low to high.
/// Read from the page's `{"spectrum":[…]}`: whole numbers 0 to 255 only, and no more than [`SPECTRUM_MOST`].
pub fn read_spectrum(message: &str) -> Option<Vec<u8>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Said {
        spectrum: Vec<u8>,
    }
    let said: Said = serde_json::from_str(message).ok()?;
    (said.spectrum.len() <= SPECTRUM_MOST).then_some(said.spectrum)
}

/// The most bars a spectrum may have (the page sends 48).
pub const SPECTRUM_MOST: usize = 64;

/// The spectrum for the cab page: `{"type":"spectrum","bars":[…]}`.
pub fn spectrum_message(bars: &[u8]) -> String {
    serde_json::json!({ "type": "spectrum", "bars": bars }).to_string()
}

/// Whether speakers are ones a cab can have: a handful, each in a cab-sized space, playing what can be heard,
/// at a sensible level.
pub fn are_sound(speakers: &[Speaker]) -> bool {
    speakers.len() <= 16
        && speakers.iter().all(|s| {
            s.at.iter().all(|v| v.is_finite() && v.abs() < 10.0)
                && s.band.iter().all(|f| f.is_finite() && (0.0..=24_000.0).contains(f))
                && (s.band[1] == OPEN || s.band[0] < s.band[1])
                && s.level.is_finite()
                && (-40.0..=12.0).contains(&s.level)
                && (0.0..=50.0).contains(&s.delay)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_the_scania_s_by_its_id_with_or_without_the_prefix() {
        assert_eq!(in_truck("vehicle.scania.s_2016").map(<[_]>::len), Some(6));
        assert_eq!(in_truck("scania.s_2016"), in_truck("vehicle.scania.s_2016"));
        assert_eq!(in_truck("vehicle.daf.2021").map(<[_]>::len), Some(7));
        assert_eq!(in_truck("vehicle.new_daf.2021"), in_truck("vehicle.daf.2021"));
        assert_eq!(in_truck("vehicle.daf.xf_electric"), in_truck("vehicle.daf.2021"));
        assert_eq!(in_truck("vehicle.scania.s_2024e"), in_truck("vehicle.scania.s_2016"));
        // The Renault T's come from its interior ([`by_parts`]).
        assert_eq!(in_truck("vehicle.renault.t"), None);
        assert_eq!(in_truck("vehicle.iveco.sway").map(<[_]>::len), Some(4));
        assert_eq!(in_truck("vehicle.volvo.fh_2021").map(<[_]>::len), Some(4));
        assert_eq!(in_truck("vehicle.volvo.fh_2024"), in_truck("vehicle.volvo.fh_2021"));
        assert_eq!(in_truck("vehicle.volvo.fh16_2012"), in_truck("vehicle.volvo.fh_2021"));
        assert_eq!(in_truck("vehicle.volvo.fh16").map(<[_]>::len), Some(2));
        assert_eq!(in_truck("vehicle.man.tgx_2020").map(<[_]>::len), Some(6));
        assert_eq!(in_truck("vehicle.man.tgx"), None);
        assert_eq!(in_truck("vehicle.mercedes.actros2014").map(<[_]>::len), Some(6));
        assert_eq!(in_truck("vehicle.scania.r").map(<[_]>::len), Some(4));
        assert_eq!(in_truck("vehicle.renault.magnum").map(<[_]>::len), Some(4));
        assert_eq!(in_truck("vehicle.iveco.hiway").map(<[_]>::len), Some(4));
        assert_eq!(in_truck("vehicle.scania.streamline"), in_truck("scania.r"));
        // The Actros 2009's come from its interior ([`by_parts`]).
        assert_eq!(in_truck("vehicle.mercedes.actros"), None);
    }

    #[test]
    fn the_daf_xd_has_the_2021s_speakers_where_its_cabs_put_the_same_front() {
        // Its cabs place the 2021's front 0.769 lower and 0.225 further forward, against the cab's turning point.
        let (xf, xd) = (in_truck("daf.2021").unwrap(), in_truck("vehicle.daf.xd").unwrap());
        assert_eq!(xf.len(), xd.len());
        for (xf, xd) in xf.iter().zip(xd) {
            assert_eq!((xf.band, xf.level, xf.channel, xf.at[0]), (xd.band, xd.level, xd.channel, xd.at[0]));
            assert!((xf.at[1] - 0.769 - xd.at[1]).abs() < 1e-4 && (xf.at[2] - 0.225 - xd.at[2]).abs() < 1e-4, "{xd:?}");
        }
    }

    #[test]
    fn the_scania_r_has_the_ss_speakers_where_its_cab_puts_the_same_interior() {
        // Its cabs place the S's interior 0.156 lower, 0.001 further forward.
        let (s, r) = (in_truck("scania.s_2016").unwrap(), in_truck("vehicle.scania.r_2016").unwrap());
        assert_eq!(s.len(), r.len());
        for (s, r) in s.iter().zip(r) {
            assert_eq!((s.band, s.level, s.channel, s.at[0]), (r.band, r.level, r.channel, r.at[0]));
            assert!((s.at[1] - 0.156 - r.at[1]).abs() < 1e-4 && (s.at[2] - 0.001 - r.at[2]).abs() < 1e-4, "{r:?}");
        }
    }

    #[test]
    fn places_each_sides_speakers_on_its_side_of_the_cab() {
        for speakers in [
            SCANIA_S,
            SCANIA_R,
            DAF_2021,
            DAF_XD,
            IVECO_SWAY,
            VOLVO_FH,
            MAN_TGX_2020,
            ACTROS_2014,
            SCANIA_R_2009,
            VOLVO_FH16_2009,
            RENAULT_MAGNUM,
            IVECO_HIWAY,
        ] {
            // The cab's middle is x = 0 (cab space starts there across).
            for speaker in speakers {
                match speaker.channel {
                    // At least 30 cm out (the Magnum's tweeters, either side of the mirror, the nearest).
                    Channel::Left => assert!(speaker.at[0] < -0.3, "{speaker:?}"),
                    Channel::Right => assert!(speaker.at[0] > 0.3, "{speaker:?}"),
                    Channel::Both => assert!(speaker.at[0].abs() < 0.3, "{speaker:?}"),
                }
            }
            // And each left one has its right one mirrored across it (but where the model has them apart: the
            // Actros 2014's overhead pair, the driver's 14.7 cm further back).
            let apart = |s: &Speaker| speakers == ACTROS_2014 && s.band == [400.0, 4000.0];
            for left in speakers.iter().filter(|s| s.channel == Channel::Left && !apart(s)) {
                let mirrored = |right: &&Speaker| {
                    right.channel == Channel::Right
                        && right.band == left.band
                        && right.level == left.level
                        && (left.at[0] + right.at[0]).abs() < 1e-6
                        && left.at[1..] == right.at[1..]
                };
                assert!(speakers.iter().any(|right| mirrored(&right)), "{left:?}");
            }
            assert!(are_sound(speakers));
        }
    }

    #[test]
    fn reads_only_a_spectrum_of_bars() {
        assert_eq!(read_spectrum(r#"{"spectrum":[0,12,255]}"#), Some(vec![0, 12, 255]));
        assert_eq!(read_spectrum(r#"{"spectrum":[]}"#), Some(vec![]));
        // Not a byte each, more than it may have, anything else said, or not a spectrum at all: none.
        assert_eq!(read_spectrum(r#"{"spectrum":[256]}"#), None);
        assert_eq!(read_spectrum(r#"{"spectrum":[-1]}"#), None);
        assert_eq!(read_spectrum(&format!(r#"{{"spectrum":{:?}}}"#, vec![1u8; SPECTRUM_MOST + 1])), None);
        assert_eq!(read_spectrum(r#"{"spectrum":[1],"title":"x"}"#), None);
        assert_eq!(read_spectrum(r#"{"ready":true}"#), None);
        assert_eq!(spectrum_message(&[3, 4]), r#"{"bars":[3,4],"type":"spectrum"}"#);
    }

    #[test]
    fn keeps_the_tone_within_a_car_stereos_range() {
        let wild = Equalizer { bass: -40, mid: 3, treble: 100 };
        assert_eq!(wild.clamped(), Equalizer { bass: -6, mid: 3, treble: 6 });
        assert_eq!(Equalizer::default().clamped(), Equalizer::default());
    }

    #[test]
    fn takes_only_a_head_in_a_cab() {
        let head = Head { x: 0.0, y: 0.3, z: 0.1, heading: 0.05, pitch: -0.02 };
        assert!(head.is_sound());
        assert!(!Head { x: f32::NAN, ..head }.is_sound() && !Head { y: 1e9, ..head }.is_sound());
        let bass = speaker([OPEN, 100.0], 0.0, Channel::Both, [0.0, 1.0, 0.0]);
        assert!(are_sound(&[bass]));
        assert!(!are_sound(&[Speaker { at: [f32::INFINITY, 0.0, 0.0], ..bass }]));
        assert!(!are_sound(&[Speaker { band: [400.0, 60.0], ..bass }]));
        assert!(!are_sound(&[Speaker { level: f32::NAN, ..bass }]) && !are_sound(&[Speaker { level: 40.0, ..bass }]));
    }

    #[test]
    fn splits_each_sides_sound_without_overlap() {
        // Each side's bands (the bass box's too, both sides'), from the bottom: each starts where the one
        // under it ends, up to the top.
        for speakers in [
            SCANIA_S,
            SCANIA_R,
            DAF_2021,
            DAF_XD,
            IVECO_SWAY,
            VOLVO_FH,
            MAN_TGX_2020,
            ACTROS_2014,
            SCANIA_R_2009,
            VOLVO_FH16_2009,
            RENAULT_MAGNUM,
            IVECO_HIWAY,
        ] {
            let mut bands: Vec<[f32; 2]> = speakers
                .iter()
                .filter(|s| s.channel == Channel::Left || (s.channel == Channel::Both && s.band[0] == OPEN))
                .map(|s| s.band)
                .collect();
            bands.sort_by(|a, b| a[0].total_cmp(&b[0]));
            bands.dedup();
            assert!(bands.windows(2).all(|pair| pair[0][1] == pair[1][0]), "{bands:?}");
            assert_eq!(bands.last().unwrap()[1], OPEN);
        }
    }

    /// The PGR's speakers with these parts (their models' files), placed as in its "Treated new gauges GPS"
    /// interior (`pgr2/6s_model`: its slots' locators, unturned) from its default head.
    fn pgr(parts: &[(&str, &str)]) -> Vec<Speaker> {
        const HEAD: [f32; 3] = [-0.699, 1.151, -0.373];
        let mut placed = Vec::new();
        for (part, (slot, file)) in parts.iter().enumerate() {
            let (folder, at) = match *slot {
                "doorpanel" => ("doorpanel", [-0.157, -0.547, -0.288]),
                "apilar" => ("apilar", [1.665, 0.0, -0.742]),
                "topconsole" => ("topconsole", [0.678, 0.390, -0.458]),
                _ => ("rearwall", [0.681, -0.025, 1.115]),
            };
            let model = format!("/vehicle/truck/upgrade/{folder}/scania_rcab_5s/{file}.pmd");
            let units = of_model(&model).unwrap_or_else(|| panic!("{model}"));
            placed.extend(units.iter().map(|u| Placed {
                role: u.role,
                size: u.size,
                part,
                at: [0, 1, 2].map(|i| HEAD[i] + at[i] + u.at[i]),
            }));
        }
        assemble(&placed)
    }

    /// Each side's bands, low to high, each once (the rear fill's aside: it repeats the front's, late and quiet).
    fn bands(speakers: &[Speaker], side: Channel) -> Vec<[f32; 2]> {
        let mut bands: Vec<[f32; 2]> =
            speakers.iter().filter(|s| s.channel == side && s.delay == 0.0).map(|s| s.band).collect();
        bands.sort_by(|a, b| a[0].total_cmp(&b[0]));
        bands.dedup();
        bands
    }

    #[test]
    fn knows_the_pgrs_parts_by_their_models() {
        let model = |folder: &str, file: &str| format!("/vehicle/truck/upgrade/{folder}/scania_rcab_5s/{file}.pmd");
        assert_eq!(of_model(&model("doorpanel", "doorpanels_tufted_pluche_red")).map(<[_]>::len), Some(2));
        assert_eq!(of_model(&model("apilar", "pilar_a_leather_vents_top_b_p")).map(<[_]>::len), Some(2));
        assert_eq!(of_model(&model("rearwall", "rearwall_interior_custom_7")).map(<[_]>::len), Some(6));
        // No speakers: the Classic and Highline consoles, the plain rear walls, the door sides.
        for (folder, file) in [
            ("topconsole", "bedconsole_4s_custom"),
            ("topconsole", "bedconsole_h"),
            ("rearwall", "rearwall_interior_custom"),
            ("door", "doors"),
        ] {
            assert_eq!(of_model(&model(folder, file)), None, "{file}");
        }
        assert!(by_parts("vehicle.scania.5s") && by_parts("scania.5s") && !by_parts("vehicle.scania.s_2016"));
    }

    #[test]
    fn places_the_pgrs_speakers_where_its_parts_have_them() {
        // The owner's truck on 2026-10-24: tufted doors, leather pillars, the custom console, a plain rear wall.
        let speakers = pgr(&[
            ("doorpanel", "doorpanels_tufted"),
            ("apilar", "pilar_a_leather_vents_top_b"),
            ("topconsole", "bedconsole_custom"),
        ]);
        let at = |channel: Channel, band: [f32; 2]| {
            let found = speakers.iter().find(|s| s.channel == channel && s.band == band);
            found.map(|s| s.at.map(|v| (v * 1000.0).round() / 1000.0))
        };
        // The doors from the bottom (no rear) to the pillars' 2 kHz, the pillars above, the centre 6 dB down.
        assert_eq!(at(Channel::Left, [OPEN, 2000.0]), Some([-1.068, 0.548, -0.333]));
        assert_eq!(at(Channel::Right, [OPEN, 2000.0]), Some([1.063, 0.548, -0.333]));
        assert_eq!(at(Channel::Left, [2000.0, OPEN]), Some([-1.045, 1.007, -1.111]));
        assert_eq!(at(Channel::Both, [250.0, 4000.0]), Some([-0.002, 1.435, -1.109]));
        assert_eq!(speakers.iter().find(|s| s.channel == Channel::Both).map(|s| s.level), Some(-6.0));
        assert_eq!(speakers.len(), 5);

        // V8 Power's rear wall, a 17.3 and a 13.4 cm unit a side: the big one the bass under 80 Hz (the doors from
        // there), the smaller the fill, from 80 Hz to the top (no tweeter), 10 dB down and late.
        let speakers = pgr(&[
            ("doorpanel", "doorpanels"),
            ("apilar", "pilar_a"),
            ("topconsole", "bedconsole"),
            ("int_rearwall", "rearwall_interior_v8"),
        ]);
        let rear: Vec<&Speaker> = speakers.iter().filter(|s| s.band == [OPEN, 80.0]).collect();
        assert_eq!(rear.len(), 2);
        assert!(rear.iter().all(|s| s.level == 0.0 && s.delay == 0.0 && (s.at[2] - 0.682).abs() < 0.002), "{rear:?}");
        assert!(rear.iter().any(|s| s.channel == Channel::Left && (s.at[0] + 0.875).abs() < 0.002));
        let fill: Vec<&Speaker> = speakers.iter().filter(|s| s.delay > 0.0).collect();
        assert_eq!(fill.len(), 2);
        assert!(fill.iter().all(|s| s.band == [80.0, OPEN] && s.level == -10.0 && s.delay == FILL_DELAY), "{fill:?}");
        assert!(fill.iter().any(|s| s.channel == Channel::Left && (s.at[0] + 0.682).abs() < 0.002));
        assert!(speakers.iter().any(|s| s.band == [80.0, 2000.0] && s.level == 0.0));

        // JS Customs', three a side of 17.8, 13.1 and 4.7 cm: the bass, the fill to 4 kHz, the tweeter's highs.
        let speakers = pgr(&[
            ("doorpanel", "doorpanels_tufted"),
            ("apilar", "pilar_a"),
            ("int_rearwall", "rearwall_interior_custom_7"),
        ]);
        let left = |band: [f32; 2]| speakers.iter().find(|s| s.channel == Channel::Left && s.band == band).copied();
        let (bass, fill, high) =
            (left([OPEN, 80.0]).unwrap(), left([80.0, 4000.0]).unwrap(), left([4000.0, OPEN]).unwrap());
        assert_eq!((bass.level, bass.delay), (0.0, 0.0));
        assert_eq!((fill.level, fill.delay), (-10.0, FILL_DELAY));
        assert_eq!((high.level, high.delay), (-12.0, FILL_DELAY));
        assert!((high.at[0] + 0.583).abs() < 0.002, "{high:?}");
        assert_eq!(speakers.len(), 10);

        // A wall of one size is all bass, no fill: Dark Diamond's two a side share it, 3 dB down each, and Lights +
        // speakers' one a side.
        for (wall, each) in [("rearwall_interior_dark_diamond", -3.0), ("rearwall_interior_custom_4", 0.0)] {
            let speakers = pgr(&[("doorpanel", "doorpanels"), ("apilar", "pilar_a"), ("int_rearwall", wall)]);
            assert!(speakers.iter().all(|s| s.delay == 0.0), "{wall}");
            let rear: Vec<&Speaker> = speakers.iter().filter(|s| s.band == [OPEN, 80.0]).collect();
            assert!(!rear.is_empty() && rear.iter().all(|s| s.level == each), "{wall}: {rear:?}");
        }

        // Dark Diamond's console: its pair the voices, the doors under them, the pillars over.
        let speakers = pgr(&[
            ("doorpanel", "doorpanels_dark_diamond"),
            ("apilar", "pilar_a_dark_diamond"),
            ("topconsole", "bedconsole_dark_diamond"),
            ("int_rearwall", "rearwall_interior_dark_diamond"),
        ]);
        let split = vec![[OPEN, 80.0], [80.0, 400.0], [400.0, 4000.0], [4000.0, OPEN]];
        assert_eq!(bands(&speakers, Channel::Left), split);
        assert_eq!(bands(&speakers, Channel::Right), split);
        assert!(speakers.iter().all(|s| s.channel != Channel::Both));
    }

    #[test]
    fn every_pgr_configuration_splits_each_side_and_places_it_on_its_side() {
        let doors = ["doorpanels", "doorpanels_tufted", "doorpanels_tartan"];
        let consoles = [None, Some("bedconsole_n"), Some("bedconsole_dark_diamond")];
        let walls = [
            None,
            Some("rearwall_interior_v8"),
            Some("rearwall_interior_custom_4"),
            Some("rearwall_interior_custom_7"),
            Some("rearwall_interior_dark_diamond"),
        ];
        for door in doors {
            for console in consoles {
                for wall in walls {
                    let mut parts = vec![("doorpanel", door), ("apilar", "pilar_a_tartan_vents")];
                    parts.extend(console.map(|file| ("topconsole", file)));
                    parts.extend(wall.map(|file| ("int_rearwall", file)));
                    let speakers = pgr(&parts);
                    assert!(are_sound(&speakers), "{parts:?}");
                    for side in [Channel::Left, Channel::Right] {
                        let bands = bands(&speakers, side);
                        assert_eq!(bands.first().map(|b| b[0]), Some(OPEN), "{parts:?}");
                        assert!(bands.windows(2).all(|pair| pair[0][1] == pair[1][0]), "{parts:?}: {bands:?}");
                        assert_eq!(bands.last().map(|b| b[1]), Some(OPEN), "{parts:?}");
                    }
                    for speaker in &speakers {
                        match speaker.channel {
                            Channel::Left => assert!(speaker.at[0] < -0.5, "{speaker:?}"),
                            Channel::Right => assert!(speaker.at[0] > 0.5, "{speaker:?}"),
                            Channel::Both => assert!(speaker.at[0].abs() < 0.3, "{speaker:?}"),
                        }
                    }
                }
            }
        }
    }

    /// The Renault T's speakers with this interior (its model's file under `interior/`), placed from `head` (the
    /// game's default: the model's origin).
    fn renault_t(model: &str, head: [f32; 3]) -> Vec<Speaker> {
        renault_t_model(&format!("renault_t/interior/{model}"), head)
    }

    /// The speakers with this interior model (its path under `/vehicle/truck/`), placed from `head`.
    fn renault_t_model(model: &str, head: [f32; 3]) -> Vec<Speaker> {
        let model = format!("/vehicle/truck/{model}");
        let units = of_model(&model).unwrap_or_else(|| panic!("{model}"));
        let placed: Vec<Placed> = units
            .iter()
            .map(|u| Placed { role: u.role, size: u.size, part: 0, at: [0, 1, 2].map(|i| head[i] + u.at[i]) })
            .collect();
        assemble(&placed)
    }

    /// The T's low sleeper's default head (logged 2026-10-26), and a right-hand-drive one's (the same mirrored).
    const T_HEAD: [f32; 3] = [-0.7, 1.387, 0.545];
    const T_HEAD_UK: [f32; 3] = [0.7, 1.387, 0.545];

    #[test]
    fn knows_the_renault_ts_interiors_by_their_models() {
        assert!(by_parts("vehicle.renault.t") && by_parts("renault.t"));
        let units = |file: &str| of_model(&format!("/vehicle/truck/renault_t/interior/{file}")).map(<[_]>::len);
        assert_eq!(units("model.pmd"), Some(4));
        assert_eq!(units("uk/model_uk.pmd"), Some(4));
        for file in ["antracite_sl_24", "antracite_slh_24", "carbon_sl_24", "carbon_slh_24"] {
            assert_eq!(units(&format!("model_24/{file}.pmd")), Some(6), "{file}");
            assert_eq!(units(&format!("uk_24/{file}.pmd")), Some(6), "{file}");
        }
        assert_eq!(units("anim.pmd"), None);
    }

    #[test]
    fn plays_the_older_renault_t_from_its_back_wall_and_2024s_from_its_doors() {
        // 2013 to 2022: no doors, so the back wall's pair everything up to the pillars' 2 kHz (owner 2026-10-26).
        let speakers = renault_t("model.pmd", T_HEAD);
        assert_eq!(speakers.len(), 4);
        for side in [Channel::Left, Channel::Right] {
            assert_eq!(bands(&speakers, side), vec![[OPEN, 2000.0], [2000.0, OPEN]]);
        }
        let rear = speakers.iter().find(|s| s.channel == Channel::Left && s.band == [OPEN, 2000.0]).unwrap();
        assert!(rear.at.iter().zip([-0.412, 1.315, 1.370]).all(|(a, b)| (a - b).abs() < 1e-3), "{rear:?}");
        assert!(speakers.iter().all(|s| s.level == 0.0 && s.delay == 0.0));

        // 2024: the doors from 80 Hz to 2 kHz, the back wall the bass under them, the pillars the highs.
        let speakers = renault_t("model_24/antracite_slh_24.pmd", T_HEAD);
        assert_eq!(speakers.len(), 6);
        for side in [Channel::Left, Channel::Right] {
            assert_eq!(bands(&speakers, side), vec![[OPEN, 80.0], [80.0, 2000.0], [2000.0, OPEN]]);
        }
        let door = speakers.iter().find(|s| s.channel == Channel::Left && s.band == [80.0, 2000.0]).unwrap();
        assert!(door.at.iter().zip([-1.016, 0.365, -0.091]).all(|(a, b)| (a - b).abs() < 1e-3), "{door:?}");
    }

    #[test]
    fn plays_the_actros_2009_from_its_doors_and_its_shelfs_corners() {
        assert!(by_parts("vehicle.mercedes.actros"));
        // The lower cabs' default head (their `interior` less the chassis's `cabin`), and a right-hand drive one.
        for (model, head) in [
            ("mercedes_actros_2009/interior/model.pmd", [-0.748, 1.348, 0.397]),
            ("mercedes_actros_2009/interior/uk/model.pmd", [0.748, 1.348, 0.397]),
        ] {
            let speakers = renault_t_model(model, head);
            assert_eq!(speakers.len(), 4);
            for side in [Channel::Left, Channel::Right] {
                // The doors the low end, the shelf's pair the rest.
                assert_eq!(bands(&speakers, side), vec![[OPEN, 400.0], [400.0, OPEN]], "{model}");
            }
            for speaker in &speakers {
                match speaker.channel {
                    Channel::Left => assert!(speaker.at[0] < -0.9, "{speaker:?}"),
                    _ => assert!(speaker.at[0] > 0.9, "{speaker:?}"),
                }
            }
        }
    }

    /// The speakers of the part known by `key` (its model's or, the DAF XF 105's, its definition's path), placed
    /// from `head`.
    fn placed(key: &str, head: [f32; 3]) -> Vec<Speaker> {
        let units = of_model(key).unwrap_or_else(|| panic!("{key}"));
        let placed: Vec<Placed> = units
            .iter()
            .map(|u| Placed { role: u.role, size: u.size, part: 0, at: [0, 1, 2].map(|i| head[i] + u.at[i]) })
            .collect();
        assemble(&placed)
    }

    #[test]
    fn plays_the_daf_xf_105_as_each_cab_has_its_speakers() {
        assert!(by_parts("vehicle.daf.xf"));
        let def = |file: &str| format!("/def/vehicle/truck/daf.xf/interior/{file}.sii");
        // Every cab's default head: its `interior` less the chassis's `cabin`; a right-hand drive cab's mirrored.
        for (side, uk) in [(-0.706, ""), (0.706, "_uk")] {
            let head = [side, 1.417, 0.386];
            // The Super Space Cab: the shelf's rear pair the low end, its front pair the voices, the pillars the highs.
            for interior in ["standard", "exclusive"] {
                let speakers = placed(&def(&format!("{interior}{uk}")), head);
                assert_eq!(speakers.len(), 6);
                for channel in [Channel::Left, Channel::Right] {
                    let split = vec![[OPEN, 400.0], [400.0, 4000.0], [4000.0, OPEN]];
                    assert_eq!(bands(&speakers, channel), split, "{interior}{uk}");
                }
            }
            // The Space cabs: their one overhead unit all but the highs, both sides, as loud; the pillars the highs.
            for interior in ["space", "space_ex", "space_plus", "space_plus_ex"] {
                let speakers = placed(&def(&format!("{interior}{uk}")), head);
                let centre: Vec<&Speaker> = speakers.iter().filter(|s| s.channel == Channel::Both).collect();
                assert_eq!(centre.len(), 1, "{interior}{uk}");
                assert_eq!((centre[0].band, centre[0].level), ([OPEN, 2000.0], 0.0));
                assert!(speakers.iter().filter(|s| s.channel != Channel::Both).all(|s| s.band == [2000.0, OPEN]));
                assert!(are_sound(&speakers));
            }
        }
        // The model alone (another truck's sharing it) tells nothing.
        assert_eq!(of_model("/vehicle/truck/daf_xf/interior/model.pmd"), None);
    }

    #[test]
    fn plays_the_daf_xf_euro6_as_each_cab_has_its_speakers() {
        assert!(by_parts("vehicle.daf.xf_euro6"));
        let def = |file: &str| format!("/def/vehicle/truck/daf.xf_euro6/interior/{file}.sii");
        // Its cabs' default heads, the EU's and the UK's (their `interior` less the chassis's `cabin`).
        for (head, uk) in [([-0.775, 1.562, 0.297], ""), ([0.728, 1.444, 0.342], "_uk")] {
            // The Super Space Cab: four ways, the shelf's back pair the bass, the doors, the shelf's front pair, the pillars.
            for interior in ["standard", "exclusive", "stan_fl", "excl_fl"] {
                let speakers = placed(&def(&format!("{interior}{uk}")), head);
                assert_eq!(speakers.len(), 8, "{interior}{uk}");
                let split = vec![[OPEN, 80.0], [80.0, 400.0], [400.0, 4000.0], [4000.0, OPEN]];
                for channel in [Channel::Left, Channel::Right] {
                    assert_eq!(bands(&speakers, channel), split, "{interior}{uk}");
                }
                assert!(speakers.iter().all(|s| (s.at[0] < 0.0) == (s.channel == Channel::Left)), "{interior}{uk}");
            }
            // The Space cabs: doors and pillars, the overhead unit a centre fill.
            for interior in ["standard_low", "exclusive_low", "stan_l_fl", "excl_l_fl"] {
                let speakers = placed(&def(&format!("{interior}{uk}")), head);
                assert_eq!(speakers.len(), 5, "{interior}{uk}");
                for channel in [Channel::Left, Channel::Right] {
                    assert_eq!(bands(&speakers, channel), vec![[OPEN, 2000.0], [2000.0, OPEN]], "{interior}{uk}");
                }
                let centre = speakers.iter().find(|s| s.channel == Channel::Both).unwrap();
                assert_eq!((centre.band, centre.level), ([250.0, 4000.0], -6.0));
                // Both sides from beside the tachograph, over the driver's side: not the cab's middle.
                let drivers_side = if uk.is_empty() { centre.at[0] < -0.8 } else { centre.at[0] > 0.8 };
                assert!(drivers_side, "{centre:?}");
            }
        }
    }

    #[test]
    fn plays_the_man_tgx_as_each_cab_has_its_speakers() {
        assert!(by_parts("vehicle.man.tgx"));
        let def = |file: &str| format!("/def/vehicle/truck/man.tgx/interior/{file}.sii");
        for (side, uk) in [(-0.795, ""), (0.795, "_uk")] {
            let head = [side, 1.412, 0.513];
            // The XXL: the doors to 400 Hz, the side walls' units to 4 kHz, the roof corners' the highs.
            for interior in ["standard", "exclusive"] {
                let speakers = placed(&def(&format!("{interior}{uk}")), head);
                for channel in [Channel::Left, Channel::Right] {
                    let split = vec![[OPEN, 400.0], [400.0, 4000.0], [4000.0, OPEN]];
                    assert_eq!(bands(&speakers, channel), split, "{interior}{uk}");
                }
            }
            // The XL and XLX: the doors everything.
            for interior in ["standard_low", "standard_mid", "exclusive_low", "exclusive_mid"] {
                let file = if uk.is_empty() { interior.to_owned() } else { interior.replacen('_', "_uk_", 1) };
                let speakers = placed(&def(&file), head);
                assert_eq!(speakers.len(), 2, "{file}");
                assert!(speakers.iter().all(|s| s.band == [OPEN, OPEN]), "{file}");
            }
        }
    }

    #[test]
    fn plays_the_man_tgx_euro6_as_each_cab_has_its_speakers() {
        assert!(by_parts("vehicle.man.tgx_euro6"));
        let model = |file: &str| format!("/vehicle/truck/man_tgx_euro6/interior/{file}.pmd");
        for (side, uk) in [(-0.743, ""), (0.743, "uk/")] {
            let head = [side, 1.4, 0.5];
            // The XL: the doors to 2 kHz, the roof corners' units the highs.
            for file in ["cab_xl", "cab_xl_2019"] {
                let speakers = placed(&model(&format!("{uk}{file}")), head);
                for channel in [Channel::Left, Channel::Right] {
                    assert_eq!(bands(&speakers, channel), vec![[OPEN, 2000.0], [2000.0, OPEN]], "{uk}{file}");
                }
            }
            // The XLX and XXL: the doors everything.
            for file in ["cab_xlx", "cab_xlx_2019", "cab_xxl", "cab_xxl_2019"] {
                let speakers = placed(&model(&format!("{uk}{file}")), head);
                assert_eq!(speakers.len(), 2, "{uk}{file}");
                assert!(speakers.iter().all(|s| s.band == [OPEN, OPEN]), "{uk}{file}");
            }
        }
    }

    #[test]
    fn plays_the_renault_premium_from_its_one_unit_a_side_up_in_either_roof() {
        assert!(by_parts("vehicle.renault.premium"));
        let def = |file: &str| format!("/def/vehicle/truck/renault.premium/interior/{file}.sii");
        let height = |file: &str, head: [f32; 3]| {
            let speakers = placed(&def(file), head);
            assert_eq!(speakers.len(), 2, "{file}");
            // Each its side's whole sound.
            for channel in [Channel::Left, Channel::Right] {
                assert_eq!(bands(&speakers, channel), vec![[OPEN, OPEN]], "{file}");
            }
            speakers[0].at[1]
        };
        let (eu, uk) = ([-0.720, 1.142, 0.459], [0.720, 1.142, 0.459]);
        for (high, low, head) in [("standard_high", "standard", eu), ("expensive_uk", "expensive_norm_uk", uk)] {
            // The low roof's 0.368 lower.
            assert!((height(high, head) - height(low, head) - 0.368).abs() < 1e-3);
        }
    }

    #[test]
    fn plays_the_iveco_stralis_from_its_console_and_the_passengers_small_unit() {
        assert!(by_parts("vehicle.iveco.stralis"));
        let def = |file: &str| format!("/def/vehicle/truck/iveco.stralis/interior/{file}.sii");
        // The passenger's side the right in a left-hand drive cab, the left in a right-hand drive one.
        for (side, uk, passenger, driver) in
            [(-0.745, "", Channel::Right, Channel::Left), (0.745, "_uk", Channel::Left, Channel::Right)]
        {
            let head = [side, 1.254, 0.452];
            // The high roofs: the passenger's highs from the small unit, the driver's console unit everything.
            for interior in ["standard", "exclusive"] {
                let speakers = placed(&def(&format!("{interior}{uk}")), head);
                assert_eq!(speakers.len(), 3, "{interior}{uk}");
                assert_eq!(bands(&speakers, passenger), vec![[OPEN, 4000.0], [4000.0, OPEN]], "{interior}{uk}");
                assert_eq!(bands(&speakers, driver), vec![[OPEN, OPEN]], "{interior}{uk}");
            }
            // The low roof: the console units, each its side's whole sound.
            for interior in ["standard_low", "exclusive_low"] {
                let speakers = placed(&def(&format!("{interior}{uk}")), head);
                assert_eq!(speakers.len(), 2, "{interior}{uk}");
                assert!(speakers.iter().all(|s| s.band == [OPEN, OPEN]), "{interior}{uk}");
            }
        }
    }

    #[test]
    fn the_e_tech_t_plays_as_a_2024_low_sleeper() {
        assert!(by_parts("vehicle.renault.etech_t"));
        // Each from its own default head (its cab's `interior` locator less the chassis's `cabin`).
        for (etech, t, side) in [
            ("renault_etech_t/interior/model/etech.pmd", "renault_t/interior/model_24/antracite_sl_24.pmd", -0.705),
            ("renault_etech_t/interior/uk/etech_uk.pmd", "renault_t/interior/uk_24/antracite_sl_24.pmd", 0.705),
        ] {
            let etech = renault_t_model(etech, [side, 1.251, 0.502]);
            let t = renault_t_model(t, [side / 0.705 * 0.7, 1.387, 0.545]);
            assert_eq!(etech.len(), t.len());
            for (a, b) in etech.iter().zip(&t) {
                assert_eq!((a.band, a.channel), (b.band, b.channel));
                assert!((0..3).all(|k| (a.at[k] - b.at[k]).abs() < 0.002), "{a:?} against {b:?}");
            }
        }
    }

    #[test]
    fn places_every_renault_t_interiors_speakers_on_their_sides_mirrored() {
        for (model, head) in [
            ("model.pmd", T_HEAD),
            ("uk/model_uk.pmd", T_HEAD_UK),
            ("model_24/carbon_sl_24.pmd", T_HEAD),
            ("uk_24/carbon_sl_24.pmd", T_HEAD_UK),
        ] {
            let speakers = renault_t(model, head);
            assert!(are_sound(&speakers), "{model}");
            for left in speakers.iter().filter(|s| s.channel == Channel::Left) {
                assert!(left.at[0] < -0.3, "{model}: {left:?}");
                let mirrored = |right: &&Speaker| {
                    right.channel == Channel::Right
                        && right.band == left.band
                        && (left.at[0] + right.at[0]).abs() < 1e-3
                        && left.at[1..] == right.at[1..]
                };
                assert!(speakers.iter().any(|right| mirrored(&right)), "{model}: {left:?}");
            }
        }
        // A right-hand-drive interior's speakers where the left-hand one has them, in the cab.
        assert_eq!(renault_t("uk/model_uk.pmd", T_HEAD_UK).len(), renault_t("model.pmd", T_HEAD).len());
    }
}
