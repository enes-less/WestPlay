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
}

/// No limit that way (a band's).
const OPEN: f32 = 0.0;

const fn speaker(band: [f32; 2], level: f32, channel: Channel, at: [f32; 3]) -> Speaker {
    Speaker { band, level, channel, at }
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

/// The Renault T (2024's interior): a big woofer low in the front of each door and a tweeter at the top of
/// each A-pillar, and nothing else (no centre, no bass unit). Both are the interior model's own grilles
/// (x = 0 the driver's seat, the cab's middle at x 0.7): the doors' flat panels in the door-speaker material
/// (X -0.316 and 1.716, Y -1.234 to -0.811, Z -0.769 to -0.502), the pillars' 5 cm discs of the details
/// texture's hexagonal mesh (X -0.317 and 1.717, Y 0.026, Z -0.772), the same in every 2024 interior. The
/// high sleeper places the model at (-0.7, 2.98, -1.75) and turns on (0, 1.352, -2.295): cab space is the
/// model's plus (-0.7, 1.628, 0.545). (The low sleeper places it 0.241 lower: there they're that much above
/// the head. The T's older interiors are other models, with their speakers about there.) Symmetric about the
/// cab's middle.
///
/// A two-way pair each side: the doors everything up to 2 kHz (there's no bass unit to hand the bass to),
/// the pillars the highs from 2 kHz (handed over low, as in the Scania, so the voices come from up there by
/// the ears, not the knees).
const RENAULT_T: &[Speaker] = &[
    speaker([OPEN, 2000.0], 0.0, Channel::Left, [-1.016, 0.606, -0.091]),
    speaker([OPEN, 2000.0], 0.0, Channel::Right, [1.016, 0.606, -0.091]),
    speaker([2000.0, OPEN], 0.0, Channel::Left, [-1.017, 1.654, -0.227]),
    speaker([2000.0, OPEN], 0.0, Channel::Right, [1.017, 1.654, -0.227]),
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

/// The speakers of the truck with this SDK model id (with or without its `vehicle.` prefix), or `None` for a
/// truck whose speakers aren't known (its sound plays as it is).
pub fn in_truck(id: &str) -> Option<&'static [Speaker]> {
    match id.strip_prefix("vehicle.").unwrap_or(id) {
        "scania.s_2016" => Some(SCANIA_S),
        "renault.t" => Some(RENAULT_T),
        "iveco.sway" => Some(IVECO_SWAY),
        // A mod's DAF XG+ ("DAF 2021 Jasper") keeps the 2021's cab: its head sits 2 cm lower, its seat the same.
        "daf.2021" | "new_daf.2021" => Some(DAF_2021),
        _ => None,
    }
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
        assert_eq!(in_truck("vehicle.renault.t").map(<[_]>::len), Some(4));
        assert_eq!(in_truck("vehicle.iveco.sway").map(<[_]>::len), Some(4));
        assert_eq!(in_truck("vehicle.man.tgx_2020"), None);
    }

    #[test]
    fn places_each_sides_speakers_on_its_side_of_the_cab() {
        for speakers in [SCANIA_S, DAF_2021, RENAULT_T, IVECO_SWAY] {
            // The cab's middle is x = 0 (cab space starts there across).
            for speaker in speakers {
                match speaker.channel {
                    Channel::Left => assert!(speaker.at[0] < -0.5, "{speaker:?}"),
                    Channel::Right => assert!(speaker.at[0] > 0.5, "{speaker:?}"),
                    Channel::Both => assert!(speaker.at[0].abs() < 0.3, "{speaker:?}"),
                }
            }
            // And each left one has its right one mirrored across it.
            for left in speakers.iter().filter(|s| s.channel == Channel::Left) {
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
        for speakers in [SCANIA_S, DAF_2021, RENAULT_T, IVECO_SWAY] {
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
}
