//! The player's camera settings in the game, for YouTube's sound to turn with the camera as the game turns
//! it: with the steering ("steering-follow camera rotation", on or off, and its multiplier). The game keeps
//! them in the player's profile, `config_local.cfg` (`uset g_cam_steering "1"`,
//! `uset g_cam_steering_value "1.0"`); the profile in use is the one written last. Read again now and then:
//! a change in the game's menu applies.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// How far the game turns the interior camera at full lock, at a multiplier of 100% (degrees). The game
/// doesn't say: as it looked in the game (45 each way).
pub const STEERING_LOOK_DEGREES: f32 = 45.0;

/// How often the settings are read again.
const EVERY: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSettings {
    /// Whether the camera turns with the steering, and how much (1.0: the game's 100%).
    pub steering: bool,
    pub steering_multiplier: f32,
}

impl Default for CameraSettings {
    /// The game's own defaults.
    fn default() -> Self {
        Self { steering: true, steering_multiplier: 1.0 }
    }
}

impl CameraSettings {
    /// How far the camera is turned for this steering (the SDK's: -1 to 1, counterclockwise), in turns
    /// counterclockwise (as the head's heading).
    pub fn steering_turn(&self, steering: f32) -> f32 {
        if !self.steering || !steering.is_finite() {
            return 0.0;
        }
        steering.clamp(-1.0, 1.0) * self.steering_multiplier * STEERING_LOOK_DEGREES / 360.0
    }

    /// The settings in a profile's config (`uset name "value"` lines); the defaults where it doesn't say.
    fn read(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            if words.next() != Some("uset") {
                continue;
            }
            let (Some(name), Some(value)) = (words.next(), words.next()) else { continue };
            let value = value.trim_matches('"');
            match name {
                "g_cam_steering" => settings.steering = value != "0",
                "g_cam_steering_value" => {
                    if let Ok(multiplier) = value.parse::<f32>()
                        && multiplier.is_finite()
                    {
                        settings.steering_multiplier = multiplier.clamp(0.0, 4.0);
                    }
                }
                _ => {}
            }
        }
        settings
    }
}

/// The settings, read again every [`EVERY`].
pub struct GameSettings {
    settings: CameraSettings,
    read_at: Option<Instant>,
}

impl GameSettings {
    pub fn new() -> Self {
        Self { settings: CameraSettings::default(), read_at: None }
    }

    pub fn camera(&mut self) -> CameraSettings {
        if self.read_at.is_none_or(|at| at.elapsed() >= EVERY) {
            self.read_at = Some(Instant::now());
            if let Some(text) = latest_profile_config().and_then(|path| std::fs::read_to_string(path).ok()) {
                self.settings = CameraSettings::read(&text);
            }
        }
        self.settings
    }
}

/// The `config_local.cfg` written last, of every profile of either game (ETS2's, ATS's; Steam's and local).
fn latest_profile_config() -> Option<PathBuf> {
    let documents = westplay_setup::places::documents()?;
    let mut latest: Option<(SystemTime, PathBuf)> = None;
    for game in ["Euro Truck Simulator 2", "American Truck Simulator"] {
        for kind in ["steam_profiles", "profiles"] {
            let Ok(profiles) = std::fs::read_dir(documents.join(game).join(kind)) else { continue };
            for profile in profiles.flatten() {
                let config = profile.path().join("config_local.cfg");
                let Ok(written) = std::fs::metadata(&config).and_then(|m| m.modified()) else { continue };
                if latest.as_ref().is_none_or(|(at, _)| written > *at) {
                    latest = Some((written, config));
                }
            }
        }
    }
    latest.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_steering_camera_from_a_profiles_config() {
        let config =
            "uset g_cam_steering_reverse \"1\"\nuset g_cam_steering_value \"0.5\"\nuset g_cam_steering \"1\"\n";
        let settings = CameraSettings::read(config);
        assert_eq!(settings, CameraSettings { steering: true, steering_multiplier: 0.5 });
        // Full left lock (counterclockwise): half the full turn, to the left.
        assert!((settings.steering_turn(1.0) - STEERING_LOOK_DEGREES / 2.0 / 360.0).abs() < 1e-6);
        assert!(settings.steering_turn(-1.0) < 0.0);
        // Off: no turn.
        assert_eq!(CameraSettings::read("uset g_cam_steering \"0\"").steering_turn(1.0), 0.0);
        // Not said: the game's defaults.
        assert_eq!(CameraSettings::read(""), CameraSettings::default());
        assert_eq!(CameraSettings::read("uset g_cam_steering_value \"nan\"").steering_multiplier, 1.0);
    }
}
