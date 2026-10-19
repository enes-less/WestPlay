//! WestPlay's settings: one store the app keeps on disk, read and changed by the companion window (and, later,
//! the in-cabin Settings app), applied live by the host.
//!
//! Saved as JSON with a version number (`settings.json` in the app's config folder). Loading is forgiving, so
//! the app always starts: unknown fields are ignored, a missing or invalid value takes its default, and an
//! unreadable file is set aside as `settings.json.bad`. Files from older versions are migrated in [`migrate`].

use std::io;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

/// Version of the settings file format. Bump it, and teach [`migrate`] the step, when a setting's meaning
/// or shape changes (adding a setting needs neither: it takes its default).
pub const SETTINGS_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    #[serde(deserialize_with = "lenient")]
    pub interaction: InteractionSettings,
    #[serde(deserialize_with = "lenient")]
    pub screen: ScreenSettings,
    #[serde(deserialize_with = "lenient")]
    pub look: LookSettings,
    #[serde(deserialize_with = "lenient")]
    pub graphics: GraphicsSettings,
}

/// Which GPU the cab is drawn on (the companion's Truck Screen): one the player chose, by its id
/// (`graphics::Adapter::id`), or (`None`) the one WestPlay picks. A chosen GPU that's gone (an external one
/// unplugged) leaves WestPlay picking.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GraphicsSettings {
    #[serde(deserialize_with = "lenient")]
    pub gpu: Option<String>,
}

/// How the screen looks: its wallpaper (chosen in the cab's Settings or the companion's Customize) and its
/// icons (the companion's Customize).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LookSettings {
    /// A wallpaper's id: one of WestPlay's (`plain`) or the player's (`custom:…`, `customize`). None:
    /// the cabin page's own default.
    #[serde(deserialize_with = "lenient")]
    pub wallpaper: Option<String>,
    /// One of the player's icon sets, by name. None: WestPlay's own icons.
    #[serde(deserialize_with = "lenient")]
    pub icon_set: Option<String>,
    /// Light or dark icons, or (`auto`) as the screen is.
    #[serde(deserialize_with = "lenient")]
    pub icon_style: IconStyle,
}

/// Which of an icon's looks the screen shows: as the screen is (light or dark), or always one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IconStyle {
    #[default]
    Auto,
    Light,
    Dark,
}

/// Using the truck screen: hold the interaction key, and the mouse moves a cursor on the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InteractionSettings {
    #[serde(deserialize_with = "lenient")]
    pub key: InteractionKey,
    #[serde(deserialize_with = "lenient")]
    pub sensitivity: Sensitivity,
}

/// The keys the player can hold to use the screen. A curated list: each is free in the game's default
/// controls and comfortable to hold while driving. Not Right Alt: on many layouts (Turkish among them) it is
/// AltGr, which also sends a Ctrl press, a gearbox key for many players.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InteractionKey {
    #[default]
    LeftAlt,
    RightCtrl,
    MouseBack,
    MouseForward,
}

impl InteractionKey {
    /// The Windows virtual-key code the plugin checks.
    pub fn virtual_key(self) -> u32 {
        match self {
            Self::LeftAlt => 0xa4,      // VK_LMENU
            Self::RightCtrl => 0xa3,    // VK_RCONTROL
            Self::MouseBack => 0x05,    // VK_XBUTTON1
            Self::MouseForward => 0x06, // VK_XBUTTON2
        }
    }
}

/// How WestPlay sits on the truck's screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScreenSettings {
    #[serde(deserialize_with = "lenient")]
    pub truck_strip: TruckStrip,
}

/// The truck's own strip beside WestPlay (the Scania Smart Dash's, the DAF's status bar): kept where the
/// truck has one, or WestPlay takes the whole screen. The cabin page applies it (`oemMode` there), and its
/// Settings can change it (Appearance › Vehicle Bar), as the companion's Controls can.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TruckStrip {
    #[default]
    Auto,
    Off,
}

/// How fast the cursor moves: a multiplier on its whole speed curve. Always within [`Self::MIN`] and
/// [`Self::MAX`]: values from a file or the companion window are corrected on the way in.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "f32", into = "f32")]
pub struct Sensitivity(f32);

impl Sensitivity {
    pub const MIN: f32 = 0.5;
    pub const MAX: f32 = 3.0;

    /// Out-of-range values are clamped; anything that isn't a number is the default.
    pub fn new(value: f32) -> Self {
        if value.is_finite() { Self(value.clamp(Self::MIN, Self::MAX)) } else { Self::default() }
    }

    pub fn get(self) -> f32 {
        self.0
    }
}

impl From<f32> for Sensitivity {
    fn from(value: f32) -> Self {
        Self::new(value)
    }
}

impl From<Sensitivity> for f32 {
    fn from(sensitivity: Sensitivity) -> Self {
        sensitivity.0
    }
}

impl Default for Sensitivity {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Deserializes a field, falling back to its default when the stored value doesn't fit (a typo, or a value
/// a newer version of WestPlay wrote), instead of failing the whole file.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(deserializer: D) -> Result<T, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(T::deserialize(value).unwrap_or_default())
}

/// The file's content: the version, then the settings.
#[derive(Serialize)]
struct FileContent<'a> {
    version: u32,
    #[serde(flatten)]
    settings: &'a Settings,
}

/// Brings settings written by an older version up to [`SETTINGS_VERSION`].
fn migrate(file: serde_json::Value, version: u32) -> serde_json::Value {
    // No format changes yet. A future step looks like: `if version < 2 { /* rename or convert a field */ }`.
    let _ = version;
    file
}

/// Reads settings from the JSON of a settings file.
pub fn parse(json: &str) -> Result<Settings, serde_json::Error> {
    let file: serde_json::Value = serde_json::from_str(json)?;
    let version = file.get("version").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
    serde_json::from_value(migrate(file, version))
}

pub fn to_json(settings: &Settings) -> String {
    let content = FileContent { version: SETTINGS_VERSION, settings };
    serde_json::to_string_pretty(&content).expect("settings always serialize")
}

/// The settings file on disk.
pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub fn in_dir(dir: &Path) -> Self {
        Self { path: dir.join("settings.json") }
    }

    /// The saved settings, or the defaults when there are none. A file that can't be read is set aside.
    pub fn load(&self) -> Settings {
        let json = match std::fs::read_to_string(&self.path) {
            Ok(json) => json,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Settings::default(),
            Err(e) => {
                log!("cannot read {}: {e}", self.path.display());
                return Settings::default();
            }
        };
        parse(&json).unwrap_or_else(|e| {
            let aside = self.path.with_extension("json.bad");
            log!("{} is not valid ({e}); moved to {}", self.path.display(), aside.display());
            let _ = std::fs::rename(&self.path, aside);
            Settings::default()
        })
    }

    /// Writes the settings; a crash midway leaves the previous file intact.
    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let partial = self.path.with_extension("json.partial");
        std::fs::write(&partial, to_json(settings))?;
        std::fs::rename(partial, &self.path)
    }
}

/// The current settings, shared between the app (which changes them) and the host thread (which applies
/// them). The generation tells the host something changed.
#[derive(Default)]
pub struct SharedSettings {
    current: RwLock<Settings>,
    generation: AtomicU64,
}

impl SharedSettings {
    pub fn new(settings: Settings) -> Self {
        Self { current: RwLock::new(settings), generation: AtomicU64::new(1) }
    }

    pub fn get(&self) -> Settings {
        self.current.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set(&self, settings: Settings) {
        *self.current.write().unwrap_or_else(|e| e.into_inner()) = settings;
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Changes whenever the settings do.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_file_format() {
        let settings = Settings {
            interaction: InteractionSettings { key: InteractionKey::MouseBack, sensitivity: Sensitivity::new(1.6) },
            screen: ScreenSettings { truck_strip: TruckStrip::Off },
            look: LookSettings {
                wallpaper: Some("custom:Dusk".into()),
                icon_set: Some("Retro".into()),
                icon_style: IconStyle::Dark,
            },
            graphics: GraphicsSettings { gpu: Some("10de:2702:00000000:161".into()) },
        };
        let json = to_json(&settings);
        assert!(json.contains("\"version\": 1"), "{json}");
        assert!(json.contains("\"mouseBack\""), "{json}");
        assert!(json.contains("\"truckStrip\": \"off\""), "{json}");
        assert_eq!(parse(&json).unwrap(), settings);
    }

    #[test]
    fn missing_settings_take_their_defaults() {
        assert_eq!(parse(r#"{"version": 1}"#).unwrap(), Settings::default());
        let partial = parse(r#"{"version": 1, "interaction": {"sensitivity": 2.0}}"#).unwrap();
        assert_eq!(partial.interaction.key, InteractionKey::LeftAlt);
        assert_eq!(partial.interaction.sensitivity.get(), 2.0);
        // Files from before the truck strip was a setting keep it on Auto.
        assert_eq!(partial.screen.truck_strip, TruckStrip::Auto);
        // And from before the GPU was: WestPlay picks it.
        assert_eq!(partial.graphics.gpu, None);
    }

    #[test]
    fn a_bad_value_falls_back_to_its_default_without_losing_the_others() {
        let settings = parse(r#"{"version": 1, "interaction": {"key": "capsLock", "sensitivity": 2.0}}"#).unwrap();
        assert_eq!(settings.interaction.key, InteractionKey::LeftAlt);
        assert_eq!(settings.interaction.sensitivity.get(), 2.0);
        let settings = parse(r#"{"interaction": {"key": "rightCtrl", "sensitivity": "fast"}}"#).unwrap();
        assert_eq!(settings.interaction.key, InteractionKey::RightCtrl);
        assert_eq!(settings.interaction.sensitivity, Sensitivity::default());
        let settings = parse(r#"{"screen": {"truckStrip": "sideways"}, "interaction": {"key": "rightCtrl"}}"#).unwrap();
        assert_eq!(settings.screen.truck_strip, TruckStrip::Auto);
        assert_eq!(settings.interaction.key, InteractionKey::RightCtrl);
    }

    #[test]
    fn sensitivity_stays_in_range() {
        assert_eq!(Sensitivity::new(10.0).get(), Sensitivity::MAX);
        assert_eq!(Sensitivity::new(0.0).get(), Sensitivity::MIN);
        assert_eq!(Sensitivity::new(f32::NAN), Sensitivity::default());
        let stored = parse(r#"{"interaction": {"sensitivity": 99}}"#).unwrap();
        assert_eq!(stored.interaction.sensitivity.get(), Sensitivity::MAX);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        assert_eq!(parse(r#"{"version": 9, "futureThing": [1, 2]}"#).unwrap(), Settings::default());
    }

    #[test]
    fn an_unreadable_file_is_set_aside() {
        let dir = std::env::temp_dir().join(format!("westplay-settings-test-{}", std::process::id()));
        let file = SettingsFile::in_dir(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), "{ not json").unwrap();
        assert_eq!(file.load(), Settings::default());
        assert!(dir.join("settings.json.bad").exists());

        let settings = Settings {
            interaction: InteractionSettings { key: InteractionKey::RightCtrl, sensitivity: Sensitivity::new(2.0) },
            ..Settings::default()
        };
        file.save(&settings).unwrap();
        assert_eq!(file.load(), settings);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn shared_settings_count_changes() {
        let shared = SharedSettings::new(Settings::default());
        let before = shared.generation();
        let mut changed = Settings::default();
        changed.interaction.key = InteractionKey::MouseForward;
        shared.set(changed.clone());
        assert_ne!(shared.generation(), before);
        assert_eq!(shared.get(), changed);
    }
}
