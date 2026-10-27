//! Spotify: its own web player (open.spotify.com), in a WebView of its own, hidden under the cabin page. The
//! Spotify app the player sees is WestPlay's own (ui/src/cabin/apps/spotify); the web player plays (its audio
//! is DRM-protected, Widevine, which WebView2 has) and answers the app's questions, from its own page: a
//! script in it (`spotify.js`) asks Spotify's own API with the page's own session, and plays through Spotify
//! Connect, as the page itself does.
//!
//! Signing in is Spotify's own page, in a window on the PC's desktop (`signin.rs`), on this WebView's profile
//! (one of its own: signing out forgets Spotify's cookies alone). Nothing of the account passes through
//! WestPlay: the page's session stays in the page; what comes out is what the app shows (names, covers, what
//! plays), only the fields below (anything else the page might say is dropped here), and whether it's signed
//! in. Nothing of it is kept or logged.
//!
//! Its sound comes from the truck's own speakers, as Apple Music's does (`cab_sound.js`, told the truck's
//! speakers, the driver's head and the camera). Its audio is protected, and plays through the page's own sound
//! processing all the same (heard in the cab, 2026-10-17), as Apple Music's does.

use std::cell::RefCell;
use std::path::Path;

use crate::signin::{self, Confirming, Progress, SignInWindow};
use crate::speakers::{self, Equalizer, Head, Speaker};
use crate::webview::{self, OffscreenWebView, Stage};

/// Spotify's web player.
pub const HOME: &str = "https://open.spotify.com/";

/// What the cabin page asks of Spotify (`spotify` in ui/src/shared/host.ts).
#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "action", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Command {
    /// Run (hidden), saying whether it's signed in: the Spotify app is open.
    Start,
    /// Spotify's sign-in, in a window on the desktop.
    SignIn,
    /// Spotify's sign-up, in that window: a new account, signed in once made.
    SignUp,
    /// Forget the sign-in, and start again signed out.
    SignOut,
    /// A question for Spotify, answered with the same `id`.
    Ask {
        id: u32,
        question: Question,
    },
    /// Play a playlist, album or artist (`context`), from a song of it (`track`); without one, play on.
    Play {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        track: Option<String>,
    },
    Pause,
    Next,
    Previous,
    Seek {
        seconds: f64,
    },
    /// The playing device's volume (Spotify's own, as its app sets it), 0 to 100.
    Volume {
        percent: f64,
    },
    /// The cab's fade, in the page's own player: its volume times `level` (0 to 1; 1, as Spotify has it).
    Fade {
        level: f64,
    },
    Shuffle {
        on: bool,
    },
    Repeat {
        mode: Repeat,
    },
    /// Into Liked Songs or Your Library (`on`), or out of it.
    Save {
        uri: String,
        on: bool,
    },
}

impl Command {
    /// For Spotify's page (all but the host's own: running, signing in and out).
    fn for_page(&self) -> bool {
        !matches!(self, Command::Start | Command::SignIn | Command::SignUp | Command::SignOut)
    }
}

/// What the Spotify app asks Spotify.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "ask", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Question {
    /// Home, a facet of it (`""`: all). Signed in or not, Spotify answers for the account it has.
    Home {
        #[serde(default)]
        facet: String,
        #[serde(default)]
        signed_in: bool,
    },
    Library,
    Browse,
    Search {
        query: String,
    },
    /// A playlist's, album's or Liked Songs' page, or more of its songs: from `more`, the offset its last
    /// answer said they go on at.
    Collection {
        uri: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        more: Option<String>,
    },
    Artist {
        uri: String,
    },
    /// A song's words (`spotify:track:…`).
    Lyrics {
        uri: String,
    },
}

/// More of a page's songs from where its last answer said: a plain offset.
fn is_offset(more: &str) -> bool {
    (1..=6).contains(&more.len()) && more.bytes().all(|b| b.is_ascii_digit())
}

/// A Spotify song's address: `spotify:track:` and its 22-character id.
fn is_track(uri: &str) -> bool {
    uri.strip_prefix("spotify:track:").is_some_and(|id| id.len() == 22 && id.bytes().all(|b| b.is_ascii_alphanumeric()))
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Repeat {
    Off,
    All,
    One,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Playlist,
    Album,
    Artist,
    Show,
    Liked,
    Track,
}

/// A row, a card or a shortcut: what it is, its name, who it's by, its cover and the cover's colour.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub uri: String,
    pub kind: Kind,
    pub name: String,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub songs: Option<u32>,
    #[serde(default)]
    pub image: Option<String>,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<bool>,
    /// A song's album.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Named {
    pub uri: String,
    pub name: String,
}

/// A song: its name, artists and album, cover, length (seconds), and whether it's explicit or liked.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub uri: String,
    pub name: String,
    #[serde(default)]
    pub artists: Vec<Named>,
    pub album: Named,
    #[serde(default)]
    pub image: Option<String>,
    pub color: String,
    pub duration: f64,
    #[serde(default)]
    pub explicit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<bool>,
}

/// A playlist's or album's page.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Collection {
    pub item: Item,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub tracks: Vec<Track>,
    /// Where its songs go on (an offset, for the question's `more`), while there are more than these.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more: Option<String>,
}

/// An artist's page.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ArtistPage {
    pub item: Item,
    #[serde(default)]
    pub listeners: u64,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub popular: Vec<Track>,
    #[serde(default)]
    pub releases: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Chip {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Section {
    pub title: String,
    pub items: Vec<Item>,
    /// Spotify's Recently played shelf (the app keeps it, to show at once next time).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recent: bool,
}

/// Home: Spotify's chips, shortcuts and shelves.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Home {
    #[serde(default)]
    pub chips: Vec<Chip>,
    #[serde(default)]
    pub shortcuts: Vec<Item>,
    #[serde(default)]
    pub sections: Vec<Section>,
}

/// A tile of Browse all: its name, colour and the cover leaning in its corner.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Category {
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub cover: Option<Item>,
}

/// A search's results.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Results {
    #[serde(default)]
    pub top: Option<Item>,
    #[serde(default)]
    pub top_track: Option<Track>,
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub artists: Vec<Item>,
    #[serde(default)]
    pub playlists: Vec<Item>,
    #[serde(default)]
    pub albums: Vec<Item>,
}

/// Spotify's answer to the app's question `answer` (its id), under the question's name; `failed` when
/// Spotify didn't answer.
#[derive(Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Answer {
    #[serde(rename(serialize = "id"))]
    pub answer: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<Home>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<Vec<Item>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browse: Option<Vec<Category>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<Results>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<Collection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<ArtistPage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<crate::lyrics::Lyrics>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub failed: bool,
}

/// Where a song plays from.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Context {
    pub uri: String,
    #[serde(default)]
    pub name: String,
    pub kind: Kind,
}

/// What Spotify plays: the song, where from, how far (seconds, as the page said it), the options, and the
/// songs to come.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Playback {
    #[serde(default)]
    pub track: Option<Track>,
    #[serde(default)]
    pub context: Option<Context>,
    pub playing: bool,
    #[serde(default)]
    pub position: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub shuffle: bool,
    pub repeat: Repeat,
    /// The playing device's volume, 0 to 1 (`None`: none Spotify can set).
    #[serde(default)]
    pub volume: Option<f64>,
    /// Playing on WestPlay's own device, not another of the account's (`None`: not known, taken as here).
    #[serde(default)]
    pub here: Option<bool>,
    #[serde(default)]
    pub next: Vec<Track>,
}

/// Whether the page is signed in, and playing.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub signed_in: bool,
    pub playing: bool,
}

#[derive(Debug, PartialEq)]
pub enum Report {
    /// The music's spectrum, while the cab's Equalizer asks for it ([`speakers::read_spectrum`]).
    Spectrum(Vec<u8>),
    State(State),
    /// The sign-in window opened (`true`) or closed.
    SignInWindow(bool),
    /// The sign-in window closed signed in, and Spotify's page is yet to say it's signed in (`true`), or it
    /// has (or took too long: `false`).
    Confirming(bool),
    Answer(Box<Answer>),
    /// What plays (`None`: nothing).
    Playback(Option<Box<Playback>>),
    /// How long a play, a play on, a next or a previous took.
    Timing(Timing),
}

/// How long a play (a play on, a next, a previous) took: until the song's time moved in the page's player
/// (`None`: not in 20 s), in milliseconds. Times only, for the log.
#[derive(Clone, Debug, PartialEq)]
pub struct Timing {
    pub action: &'static str,
    pub sounding: Option<u64>,
    /// Its steps and their times (`command> 3, command 412, cluster:playing 655, playing 1830`): kinds only.
    pub steps: String,
}

/// A play's steps as the page said them, only what a step is made of (its kind's letters, its time's digits,
/// `> ! :` and the separators), and not too long.
fn steps_of(said: &str) -> String {
    said.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | ',' | '>' | '!' | ':')).take(2400).collect()
}

/// A play slower than this logs its steps.
const SLOW_MS: u64 = 1500;

impl Timing {
    /// `[spotify] resume: sound in 240 ms`
    pub fn line(&self) -> String {
        let sound = self.sounding.map_or_else(|| "no sound in 20 s".to_string(), |ms| format!("sound in {ms} ms"));
        // The steps only when it was slow: where the time went.
        let slow = self.sounding.is_none_or(|ms| ms > SLOW_MS);
        let steps = if slow && !self.steps.is_empty() { format!(" (steps: {})", self.steps) } else { String::new() };
        format!("[spotify] {}: {sound}{steps}", self.action)
    }
}

pub struct Spotify {
    view: OffscreenWebView,
    sign_in: RefCell<Option<SignInWindow>>,
    /// Once it closed signed in, until the page says it's signed in.
    confirming: Confirming,
    pending: RefCell<Vec<Report>>,
    /// What the page was told of the cab, sent again when it loads anew.
    told: RefCell<Told>,
}

/// What the host tells Spotify's page of the cab (never the cabin page's to send): the truck's speakers (`None`:
/// as it is, a truck whose speakers aren't known), the driver's head, and the camera outside (its distance,
/// metres; 0: not known). As Apple Music's (apple_music.rs).
#[derive(Debug, PartialEq, serde::Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum Cab {
    Speakers {
        speakers: Option<Vec<Speaker>>,
    },
    Head {
        #[serde(flatten)]
        head: Head,
    },
    Outside {
        on: bool,
        distance: f32,
    },
    /// The player's tone (the cab's Settings › Sound): bass, mid and treble, dB each.
    Equalizer {
        #[serde(flatten)]
        equalizer: Equalizer,
    },
    /// The music's spectrum wanted (the cab's Equalizer open) or not: the page says it as it plays.
    Spectrum {
        on: bool,
    },
}

/// What the page was told last of the cab (sent again to a page that loaded anew): the truck's speakers (none:
/// its sound as it is), and the speakers, head, camera and tone messages.
#[derive(Debug, Default)]
struct Told {
    speakers: Option<Option<Vec<Speaker>>>,
    head: Option<Head>,
    camera: Option<(bool, f32)>,
    equalizer: Option<Equalizer>,
    spectrum: Option<bool>,
    messages: [Option<String>; 5],
}

impl Spotify {
    /// Its WebView (performance.rs counts its processes).
    pub fn view(&self) -> &OffscreenWebView {
        &self.view
    }

    /// Opens Spotify's web player in a WebView of `stage`'s frame (`width` x `height`), hidden under the cabin
    /// page.
    pub fn start(stage: &Stage, width: u32, height: u32, user_data: &Path) -> Result<Self, webview::Error> {
        let view = OffscreenWebView::new(stage, width, height, user_data)?;
        view.log_as("spotify")?;
        view.show(false)?;
        stage.add(&view, false)?;
        // The truck speakers' sound first: the page's script makes its own of it.
        view.add_startup_script(concat!(include_str!("cab_sound.js"), "\n", include_str!("spotify.js")))?;
        view.navigate(HOME)?;
        Ok(Self {
            view,
            sign_in: RefCell::new(None),
            confirming: Confirming::default(),
            pending: RefCell::new(Vec::new()),
            told: RefCell::default(),
        })
    }

    pub fn command(&self, command: &Command) -> windows::core::Result<()> {
        match command {
            Command::Start => Ok(()),
            Command::SignIn => {
                self.open_sign_in(&signin::SPOTIFY);
                Ok(())
            }
            Command::SignUp => {
                self.open_sign_in(&signin::SPOTIFY_SIGN_UP);
                Ok(())
            }
            Command::SignOut => {
                self.view.delete_cookies()?;
                self.view.navigate(HOME)
            }
            // A song's words, asked only of a song's address; more of a page's songs, only from an offset.
            Command::Ask { question: Question::Lyrics { uri }, .. } if !is_track(uri) => Ok(()),
            Command::Ask { question: Question::Collection { more: Some(more), .. }, .. } if !is_offset(more) => Ok(()),
            _ if command.for_page() => self.view.post_json(&serde_json::to_string(command).unwrap_or_default()),
            _ => Ok(()),
        }
    }

    /// Spotify's sign-in (or sign-up) window, opened (or brought forward if it is).
    fn open_sign_in(&self, site: &'static signin::Site) {
        let mut sign_in = self.sign_in.borrow_mut();
        if let Some(window) = sign_in.as_ref() {
            window.bring_forward();
            return;
        }
        match SignInWindow::open(self.view.environment(), site) {
            Ok(window) => {
                *sign_in = Some(window);
                self.pending.borrow_mut().push(Report::SignInWindow(true));
            }
            Err(e) => crate::startup::mark(&format!("host: the Spotify sign-in window couldn't open: {e}")),
        }
    }

    /// The truck's speakers (none: its sound plays as it is), the driver's head, and whether the camera is outside
    /// the cab and how far (metres; 0: not known): the page is told the speakers as they change, the head as it moves,
    /// the camera as it goes in or out or a quarter metre further, the tone as it's set and the spectrum as it's wanted (as Apple Music's:
    /// apple_music.rs).
    pub fn cab(
        &self,
        speakers: Option<&[Speaker]>,
        head: Head,
        outside: bool,
        distance: f32,
        equalizer: Equalizer,
        spectrum: bool,
    ) -> windows::core::Result<()> {
        let mut told = self.told.borrow_mut();
        // The head first: the speakers' loudness is set from where it is as they're placed.
        if told.head != Some(head) && head.is_sound() {
            told.head = Some(head);
            let message = serde_json::to_string(&Cab::Head { head }).unwrap_or_default();
            told.messages[1] = Some(message.clone());
            self.view.post_json(&message)?;
        }
        if told.speakers.as_ref().map(Option::as_deref) != Some(speakers) {
            let speakers = speakers.map(<[_]>::to_vec);
            told.speakers = Some(speakers.clone());
            let message = serde_json::to_string(&Cab::Speakers { speakers }).unwrap_or_default();
            told.messages[0] = Some(message.clone());
            self.view.post_json(&message)?;
        }
        let camera = (outside, if outside { (distance * 4.0).round() / 4.0 } else { 0.0 });
        if told.camera != Some(camera) && camera.1.is_finite() {
            told.camera = Some(camera);
            let message = serde_json::to_string(&Cab::Outside { on: camera.0, distance: camera.1 }).unwrap_or_default();
            told.messages[2] = Some(message.clone());
            self.view.post_json(&message)?;
        }
        if told.equalizer != Some(equalizer) {
            told.equalizer = Some(equalizer);
            let message = serde_json::to_string(&Cab::Equalizer { equalizer }).unwrap_or_default();
            told.messages[3] = Some(message.clone());
            self.view.post_json(&message)?;
        }
        if told.spectrum != Some(spectrum) {
            told.spectrum = Some(spectrum);
            let message = serde_json::to_string(&Cab::Spectrum { on: spectrum }).unwrap_or_default();
            told.messages[4] = Some(message.clone());
            self.view.post_json(&message)?;
        }
        Ok(())
    }

    /// What Spotify's page said since the last call, and the sign-in window opening or closing (signed in,
    /// the web player loads again, signed in).
    pub fn take_reports(&self) -> Vec<Report> {
        let mut reports = std::mem::take(&mut *self.pending.borrow_mut());
        let progress = self.sign_in.borrow().as_ref().map(SignInWindow::progress);
        if let Some(progress @ (Progress::SignedIn | Progress::Closed)) = progress {
            self.sign_in.borrow_mut().take();
            reports.push(Report::SignInWindow(false));
            if progress == Progress::SignedIn {
                let _ = self.view.navigate(HOME);
                self.confirming.start();
                reports.push(Report::Confirming(true));
            }
        }
        for message in self.view.take_page_messages() {
            if let Some(bars) = speakers::read_spectrum(&message) {
                reports.push(Report::Spectrum(bars));
                continue;
            }
            match read_message(&message) {
                Some(Message::SignIn) => self.open_sign_in(&signin::SPOTIFY),
                // A page loaded anew: the cab as it is (speakers, head, camera).
                Some(Message::Ready) => {
                    for told in self.told.borrow().messages.iter().flatten() {
                        let _ = self.view.post_json(told);
                    }
                }
                Some(Message::Speakers(said)) => log!("[spotify] speakers: {said}"),
                Some(Message::State(state)) => reports.push(Report::State(state)),
                Some(Message::Answer(answer)) => reports.push(Report::Answer(answer)),
                Some(Message::Playback(playback)) => reports.push(Report::Playback(playback)),
                Some(Message::Timing(timing)) => reports.push(Report::Timing(timing)),
                None => {}
            }
        }
        reports.extend(self.pending.borrow_mut().drain(..));
        let signed_in = reports.iter().rev().find_map(|report| match report {
            Report::State(state) => Some(state.signed_in),
            _ => None,
        });
        if self.confirming.ended(signed_in) {
            reports.push(Report::Confirming(false));
        }
        reports
    }

    /// Whether the WebView failed since the last call (it's then closed and started again when needed).
    pub fn failed(&self) -> bool {
        self.view.take_failure().is_some()
    }

    /// Takes the WebView off the frame; it closes as it's dropped.
    pub fn close(self, stage: &Stage) {
        let _ = stage.remove(&self.view);
    }
}

/// A message of Spotify's page (`spotify.js`).
#[derive(Debug, PartialEq)]
enum Message {
    /// Its "Log in" pressed: the sign-in window, please.
    SignIn,
    /// The page loaded (anew): it's told the cab again.
    Ready,
    /// What the truck's speakers made of its sound, for the log: through them, no element found, or silent
    /// while Spotify plays.
    Speakers(String),
    State(State),
    Answer(Box<Answer>),
    Playback(Option<Box<Playback>>),
    Timing(Timing),
}

fn read_message(message: &str) -> Option<Message> {
    let value: serde_json::Value = serde_json::from_str(message).ok()?;
    if value.get("signIn").and_then(serde_json::Value::as_bool) == Some(true) {
        return Some(Message::SignIn);
    }
    if value.get("ready").and_then(serde_json::Value::as_bool) == Some(true) {
        return Some(Message::Ready);
    }
    if let Some(said) = value.get("speakers").and_then(serde_json::Value::as_str) {
        // Only the page's own two sayings, and short: nothing else of the page reaches the log.
        let known = said == "no media element found"
            || said.starts_with("through the speakers (")
            || said.starts_with("silent while playing (");
        return (known && said.len() <= 60).then(|| Message::Speakers(said.to_owned()));
    }
    if value.get("answer").is_some() {
        return match serde_json::from_value::<Answer>(value.clone()) {
            Ok(mut answer) => {
                answer.lyrics = answer.lyrics.map(crate::lyrics::Lyrics::checked);
                Some(Message::Answer(Box::new(answer)))
            }
            // An answer that doesn't read (Spotify's changed what it says): failed, so the app isn't left
            // waiting.
            Err(_) => {
                let id = value.get("answer").and_then(serde_json::Value::as_u64)?;
                Some(Message::Answer(Box::new(Answer {
                    answer: u32::try_from(id).ok()?,
                    failed: true,
                    ..Default::default()
                })))
            }
        };
    }
    if let Some(timing) = value.get("timing") {
        let action = match timing.get("action").and_then(serde_json::Value::as_str) {
            Some("resume") => "resume",
            Some("next") => "next",
            Some("previous") => "previous",
            _ => "play",
        };
        return Some(Message::Timing(Timing {
            action,
            sounding: timing.get("sounding").and_then(serde_json::Value::as_u64),
            steps: steps_of(timing.get("steps").and_then(serde_json::Value::as_str).unwrap_or("")),
        }));
    }
    if let Some(playback) = value.get("playback") {
        if playback.is_null() {
            return Some(Message::Playback(None));
        }
        return serde_json::from_value::<Playback>(playback.clone()).ok().map(|p| Message::Playback(Some(Box::new(p))));
    }
    serde_json::from_value::<State>(value).ok().map(Message::State)
}

/// `{"type":"spotify","signedIn":true,"playing":false,"signingIn":false}` for the cabin page (`null` fields
/// when Spotify isn't running).
pub fn state_message(state: Option<&State>, signing_in: bool) -> String {
    serde_json::json!({
        "type": "spotify",
        "signedIn": state.map(|s| s.signed_in),
        "playing": state.map(|s| s.playing),
        "signingIn": signing_in,
    })
    .to_string()
}

/// `{"type":"spotifyAnswer","id":…,"home":{…}}`: Spotify's answer to the app's question.
pub fn answer_message(answer: &Answer) -> String {
    let mut message = serde_json::to_value(answer).unwrap_or_default();
    message["type"] = "spotifyAnswer".into();
    message.to_string()
}

/// `{"type":"spotifyPlayback","playback":{…}}` (`null`: nothing plays).
pub fn playback_message(playback: Option<&Playback>) -> String {
    serde_json::json!({ "type": "spotifyPlayback", "playback": playback }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_cabin_pages_commands() {
        let ask: Command =
            serde_json::from_str(r#"{"action":"ask","id":3,"question":{"ask":"home","facet":"","signedIn":true}}"#)
                .unwrap();
        assert_eq!(ask, Command::Ask { id: 3, question: Question::Home { facet: String::new(), signed_in: true } });
        let play: Command =
            serde_json::from_str(r#"{"action":"play","context":"spotify:album:1","track":"spotify:track:2"}"#).unwrap();
        assert_eq!(
            play,
            Command::Play { context: Some("spotify:album:1".into()), track: Some("spotify:track:2".into()) }
        );
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"action":"play"}"#).unwrap(),
            Command::Play { context: None, track: None }
        );
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"action":"repeat","mode":"one"}"#).unwrap(),
            Command::Repeat { mode: Repeat::One }
        );
        assert_eq!(serde_json::from_str::<Command>(r#"{"action":"signUp"}"#).unwrap(), Command::SignUp);
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"action":"volume","percent":40}"#).unwrap(),
            Command::Volume { percent: 40.0 }
        );
        assert_eq!(serde_json::from_str::<Command>(r#"{"action":"start"}"#).unwrap(), Command::Start);
        assert!(serde_json::from_str::<Command>(r#"{"action":"explode"}"#).is_err());
    }

    #[test]
    fn passes_the_page_its_commands_as_the_app_sent_them() {
        assert!(is_track("spotify:track:4cOdK2wGLETKBW3PvgPWqT"));
        assert!(
            !is_track("spotify:track:4cOdK2wGLETKBW3PvgPWqT/../x") && !is_track("spotify:album:4cOdK2wGLETKBW3PvgPWqT")
        );
        let ask = Command::Ask { id: 9, question: Question::Search { query: "tarkan".into() } };
        assert_eq!(
            serde_json::to_string(&ask).unwrap(),
            r#"{"action":"ask","id":9,"question":{"ask":"search","query":"tarkan"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Command::Play { context: None, track: None }).unwrap(),
            r#"{"action":"play"}"#
        );
        assert!(ask.for_page() && !Command::SignOut.for_page() && !Command::Start.for_page());
    }

    #[test]
    fn reads_spotifys_page() {
        assert_eq!(read_message(r#"{"signIn":true}"#), Some(Message::SignIn));
        assert_eq!(
            read_message(r#"{"signedIn":true,"playing":false}"#),
            Some(Message::State(State { signed_in: true, playing: false }))
        );
        assert_eq!(read_message(r#"{"playback":null}"#), Some(Message::Playback(None)));
        assert_eq!(read_message(r#"{"ready":true}"#), Some(Message::Ready));
        // A play's time, its steps only what a step is made of.
        let Some(Message::Timing(timing)) = read_message(
            r#"{"timing":{"action":"resume","sounding":2400,"steps":"button 2, session:playing 30, <b>https://x 9"}}"#,
        ) else {
            panic!("not a timing")
        };
        assert_eq!(
            timing.line(),
            "[spotify] resume: sound in 2400 ms (steps: button 2, session:playing 30, b>https:x 9)"
        );
        let Some(Message::Timing(quick)) =
            read_message(r#"{"timing":{"action":"next","sounding":300,"steps":"command> 1, command 200"}}"#)
        else {
            panic!("not a timing")
        };
        assert_eq!(quick.line(), "[spotify] next: sound in 300 ms");
        let Some(Message::Timing(silent)) = read_message(r#"{"timing":{"action":"play","sounding":null,"steps":""}}"#)
        else {
            panic!("not a timing")
        };
        assert_eq!(silent.line(), "[spotify] play: no sound in 20 s");
        assert_eq!(serde_json::to_string(&Command::Fade { level: 0.5 }).unwrap(), r#"{"action":"fade","level":0.5}"#);
        assert_eq!(
            read_message(r#"{"speakers":"through the speakers (video)"}"#),
            Some(Message::Speakers("through the speakers (video)".into()))
        );
        assert_eq!(
            read_message(r#"{"speakers":"silent while playing (2 elements, taken one paused)"}"#),
            Some(Message::Speakers("silent while playing (2 elements, taken one paused)".into()))
        );
        assert_eq!(read_message(r#"{"speakers":"anything else"}"#), None);
        assert_eq!(read_message("not json"), None);
    }

    /// Only the fields the app shows pass: whatever else the page says (here, an account's name) is dropped.
    #[test]
    fn passes_on_only_what_the_app_shows() {
        let page = r##"{"answer":4,"library":[{"uri":"spotify:collection:tracks","kind":"liked","name":"Liked Songs",
            "songs":128,"image":null,"color":"#5038a0","pinned":true,"owner":{"username":"someone"}}],
            "account":"someone@example.com"}"##;
        let Some(Message::Answer(answer)) = read_message(page) else { panic!("not an answer") };
        assert_eq!(
            answer_message(&answer),
            r##"{"id":4,"library":[{"by":"","color":"#5038a0","description":"","image":null,"kind":"liked","name":"Liked Songs","pinned":true,"songs":128,"uri":"spotify:collection:tracks"}],"type":"spotifyAnswer"}"##
        );
    }

    #[test]
    fn says_when_an_answer_doesnt_read() {
        let Some(Message::Answer(answer)) = read_message(r#"{"answer":5,"home":{"sections":"not a list"}}"#) else {
            panic!("not an answer")
        };
        assert_eq!(answer_message(&answer), r#"{"failed":true,"id":5,"type":"spotifyAnswer"}"#);
    }

    #[test]
    fn passes_on_what_plays() {
        let page = r##"{"playback":{"track":{"uri":"spotify:track:1","name":"Şımarık","artists":[{"uri":"spotify:artist:2","name":"Tarkan"}],
            "album":{"uri":"spotify:album:3","name":"Ölürüm Sana"},"image":null,"color":"#C01818","duration":240.5,"explicit":false,
            "saved":true},"context":{"uri":"spotify:album:3","name":"Ölürüm Sana","kind":"album"},"playing":true,"position":12.5,
            "duration":240.5,"shuffle":false,"repeat":"all","volume":0.5,"next":[],"devices":[{"name":"Someone's phone"}]}}"##;
        let Some(Message::Playback(Some(playback))) = read_message(page) else { panic!("not a playback") };
        assert_eq!(playback.repeat, Repeat::All);
        let message = playback_message(Some(&playback));
        assert!(message.contains(r#""name":"Şımarık""#) && message.contains(r#""saved":true"#));
        assert!(message.contains(r#""volume":0.5"#), "the volume passes: {message}");
        assert!(!message.contains("phone"), "devices never pass: {message}");
    }

    /// A song's words, asked of the hidden web player as the cab asks them (a search's songs). Muted; only
    /// whether they're synced, how many lines there are and where they're from is printed (WESTPLAY_SPOTIFY_PROFILE:
    /// a signed-in profile, the player's own).
    #[test]
    #[ignore = "asks open.spotify.com"]
    fn answers_a_songs_words() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data =
            std::path::PathBuf::from(std::env::var_os("WESTPLAY_SPOTIFY_PROFILE").expect("a signed-in profile"));
        let stage = Stage::new(1280, 720).unwrap();
        let spotify = Spotify::start(&stage, 1280, 720, &user_data).unwrap();
        spotify.view.set_muted(true).unwrap();
        let ask = |id: u32, question: Question| -> Option<Box<Answer>> {
            spotify.command(&Command::Ask { id, question }).unwrap();
            let until = std::time::Instant::now() + std::time::Duration::from_secs(40);
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in spotify.take_reports() {
                    if let Report::Answer(answer) = report
                        && answer.answer == id
                    {
                        return Some(answer);
                    }
                }
            }
            None
        };
        // Spotify's page asked before it's ready answers once it is.
        std::thread::sleep(std::time::Duration::from_secs(1));
        let search =
            ask(1, Question::Search { query: "tarkan".into() }).and_then(|answer| answer.search).expect("a search");
        let uris: Vec<String> = search.tracks.iter().take(4).map(|track| track.uri.clone()).collect();
        let mut with_words = 0;
        for (n, uri) in uris.iter().enumerate() {
            let lyrics = ask(10 + n as u32, Question::Lyrics { uri: uri.clone() })
                .and_then(|answer| answer.lyrics)
                .expect("an answer");
            println!(
                "track: synced {}, {} lines, from {:?}, colours {}",
                lyrics.synced,
                lyrics.lines.len(),
                lyrics.provider,
                lyrics.colors.is_some()
            );
            if !lyrics.lines.is_empty() {
                with_words += 1;
            }
        }
        assert!(with_words > 0, "none of {} songs had words", uris.len());
        spotify.close(&stage);
    }

    /// A long playlist's songs past its first answer (a player's Liked Songs stopped at the first 50): signed
    /// out, in a profile of its own, a public playlist of more than 100 (WESTPLAY_SPOTIFY_PLAYLIST, else Spotify's
    /// All Out 80s), its next songs asked from where the first answer says they go on.
    #[test]
    #[ignore = "asks open.spotify.com"]
    fn pages_a_long_playlist() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data = std::env::temp_dir().join(format!("westplay-spotify-paging-{}", std::process::id()));
        let uri = std::env::var("WESTPLAY_SPOTIFY_PLAYLIST")
            .unwrap_or_else(|_| "spotify:playlist:37i9dQZF1DX4UtSsGT1Sbe".to_owned());
        let stage = Stage::new(1280, 720).unwrap();
        let spotify = Spotify::start(&stage, 1280, 720, &user_data).unwrap();
        spotify.view.set_muted(true).unwrap();
        let ask = |id: u32, more: Option<String>| -> Collection {
            let question = Question::Collection { uri: uri.clone(), more };
            spotify.command(&Command::Ask { id, question }).unwrap();
            let until = std::time::Instant::now() + std::time::Duration::from_secs(40);
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in spotify.take_reports() {
                    if let Report::Answer(answer) = report
                        && answer.answer == id
                    {
                        return answer.collection.expect("the playlist");
                    }
                }
            }
            panic!("no answer")
        };
        std::thread::sleep(std::time::Duration::from_secs(1));
        let first = ask(1, None);
        println!("first: {} songs of {:?}, more from {:?}", first.tracks.len(), first.item.songs, first.more);
        assert_eq!(first.tracks.len(), 100);
        assert_eq!(first.more.as_deref(), Some("100"));
        let next = ask(2, first.more.clone());
        println!("next: {} songs, more from {:?}", next.tracks.len(), next.more);
        assert!(!next.tracks.is_empty());
        assert_ne!(next.tracks[0].uri, first.tracks[0].uri);
        assert!(first.tracks.iter().all(|track| track.uri != next.tracks[0].uri));
        spotify.close(&stage);
        let _ = std::fs::remove_dir_all(&user_data);
    }

    /// Spotify's sound through the truck's speakers (cab_sound.js), as the cab plays it: an album through a
    /// Scania S's speakers, heard from the driver's seat, across a Next; how loud each ear is printed every
    /// second, and it must not fall silent while playing (once, protected, it did). Muted: never heard
    /// (WESTPLAY_SPOTIFY_PROFILE: a signed-in profile, the player's own; WESTPLAY_SPOTIFY_QUERY: what to play).
    #[test]
    #[ignore = "plays from open.spotify.com"]
    fn sounds_through_the_truck_speakers() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data =
            std::path::PathBuf::from(std::env::var_os("WESTPLAY_SPOTIFY_PROFILE").expect("a signed-in profile"));
        let stage = Stage::new(1280, 720).unwrap();
        let spotify = Spotify::start(&stage, 1280, 720, &user_data).unwrap();
        spotify.view.set_muted(true).unwrap();
        let mut playing = false;
        let pump = |seconds: f64, playing: &mut bool| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in spotify.take_reports() {
                    if let Report::State(state) = report {
                        *playing = state.playing;
                    }
                }
            }
        };
        pump(8.0, &mut playing);
        let head = Head { x: -0.47, y: 1.33, z: 0.32, heading: 0.0, pitch: 0.0 };
        spotify.cab(speakers::in_truck("scania.s_2016"), head, false, 0.0, Equalizer::default(), false).unwrap();
        // An album, from a search's first song.
        let query = std::env::var("WESTPLAY_SPOTIFY_QUERY").unwrap_or_else(|_| "tarkan".into());
        spotify.command(&Command::Ask { id: 1, question: Question::Search { query } }).unwrap();
        let mut song = None;
        let until = std::time::Instant::now() + std::time::Duration::from_secs(40);
        while song.is_none() && std::time::Instant::now() < until {
            crate::pump_messages();
            std::thread::sleep(std::time::Duration::from_millis(20));
            for report in spotify.take_reports() {
                if let Report::Answer(answer) = report {
                    song = answer.search.and_then(|search| search.tracks.into_iter().next());
                }
            }
        }
        let song = song.expect("a song to play");
        println!("playing {} ({})", song.name, song.album.name);
        // Loud enough to tell quiet from silent at the level's 4 decimals (at 10%, quiet passages read 0).
        spotify.command(&Command::Volume { percent: 50.0 }).unwrap();
        spotify.command(&Command::Play { context: Some(song.album.uri), track: Some(song.uri) }).unwrap();
        let (mut sounded, mut silent_while_playing) = (0, 0);
        for second in 0..40 {
            if second == 20 {
                println!("next");
                spotify.command(&Command::Next).unwrap();
            }
            pump(1.0, &mut playing);
            let level = spotify.view.evaluate("window.westplaySpeakerLevel?.() ?? null").unwrap_or_default();
            println!("  {second:2} s: playing={playing} level={level}");
            let loud =
                serde_json::from_str::<Vec<f64>>(&level).is_ok_and(|ears| ears.iter().take(2).any(|e| *e > 0.00001));
            if loud {
                sounded += 1;
            } else if playing && second > 5 {
                silent_while_playing += 1;
            }
        }
        spotify.command(&Command::Pause).unwrap();
        pump(1.0, &mut playing);
        spotify.close(&stage);
        assert!(sounded > 0, "never heard through the speakers");
        assert!(silent_while_playing <= 3, "silent through the speakers for {silent_while_playing} s while playing");
    }

    /// Playing here as the cab plays it leaving the game and coming back: a song started (its time logged step
    /// by step), faded in the page's own player (its element's volume, Spotify's own left as it was), paused and
    /// played on by the page's own button (the page's state at once), a pause and a play on sent one right after
    /// the other ending as the last asked, and a Next (WESTPLAY_SPOTIFY_PROFILE: a signed-in profile, a copy of
    /// the player's own; WESTPLAY_SPOTIFY_QUERY: what to play). Muted: never heard.
    #[test]
    #[ignore = "plays from open.spotify.com"]
    fn plays_pauses_and_fades_in_its_own_page() {
        use std::time::{Duration, Instant};
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data =
            std::path::PathBuf::from(std::env::var_os("WESTPLAY_SPOTIFY_PROFILE").expect("a signed-in profile"));
        let stage = Stage::new(1280, 720).unwrap();
        let spotify = Spotify::start(&stage, 1280, 720, &user_data).unwrap();
        spotify.view.set_muted(true).unwrap();
        let started = Instant::now();
        // The page's own state as it says it (`playing`), and each play's time, printed as the host logs it.
        let playing = std::cell::Cell::new(false);
        let pump_until = |seconds: f64, until: &dyn Fn() -> bool| -> Option<Duration> {
            let from = Instant::now();
            while from.elapsed().as_secs_f64() < seconds {
                crate::pump_messages();
                std::thread::sleep(Duration::from_millis(10));
                for report in spotify.take_reports() {
                    match report {
                        Report::State(state) => {
                            if state.playing != playing.get() {
                                println!(
                                    "  {:6.2} s: the page says playing={}",
                                    started.elapsed().as_secs_f64(),
                                    state.playing
                                );
                            }
                            playing.set(state.playing);
                        }
                        Report::Timing(timing) => println!("  {}", timing.line()),
                        _ => {}
                    }
                }
                if until() {
                    return Some(from.elapsed());
                }
            }
            None
        };
        let element = || -> serde_json::Value {
            serde_json::from_str(&spotify.view.evaluate("window.westplayPlaying?.() ?? null").unwrap_or_default())
                .unwrap_or_default()
        };
        // How loud through the speakers, the louder ear, over `seconds`.
        let loudness = |seconds: f64| {
            let mut most: f64 = 0.0;
            let from = Instant::now();
            while from.elapsed().as_secs_f64() < seconds {
                pump_until(0.1, &|| false);
                let level = spotify.view.evaluate("window.westplaySpeakerLevel?.() ?? null").unwrap_or_default();
                let ears = serde_json::from_str::<Vec<f64>>(&level).unwrap_or_default();
                most = ears.iter().take(2).fold(most, |a, b| a.max(*b));
            }
            most
        };
        pump_until(8.0, &|| false);
        let head = Head { x: -0.47, y: 1.33, z: 0.32, heading: 0.0, pitch: 0.0 };
        spotify.cab(speakers::in_truck("scania.s_2016"), head, false, 0.0, Equalizer::default(), false).unwrap();
        let query = std::env::var("WESTPLAY_SPOTIFY_QUERY").unwrap_or_else(|_| "tarkan".into());
        spotify.command(&Command::Ask { id: 1, question: Question::Search { query } }).unwrap();
        let mut song = None;
        let until = Instant::now() + Duration::from_secs(40);
        while song.is_none() && Instant::now() < until {
            crate::pump_messages();
            std::thread::sleep(Duration::from_millis(20));
            for report in spotify.take_reports() {
                if let Report::Answer(answer) = report {
                    song = answer.search.and_then(|search| search.tracks.into_iter().next());
                }
            }
        }
        let song = song.expect("a song to play");
        println!("playing a song of {}", song.album.name);
        spotify.command(&Command::Play { context: Some(song.album.uri), track: Some(song.uri) }).unwrap();
        let took = pump_until(20.0, &|| playing.get());
        println!("play: the page playing after {took:?}");
        assert!(took.is_some(), "never played");
        pump_until(3.0, &|| false);

        // The fade: the element's volume, and the sound through the speakers, down and back.
        let full = element()["volume"].as_f64().expect("an element playing");
        let loud = loudness(1.5);
        spotify.command(&Command::Fade { level: 0.1 }).unwrap();
        pump_until(0.2, &|| false);
        let faded = element()["volume"].as_f64().unwrap_or(-1.0);
        let quiet = loudness(1.5);
        spotify.command(&Command::Fade { level: 1.0 }).unwrap();
        pump_until(0.2, &|| false);
        let back = element()["volume"].as_f64().unwrap_or(-1.0);
        println!("fade: volume {full:.3} → {faded:.3} → {back:.3}; loudest ear {loud:.4} → {quiet:.4}");
        assert!((faded - full * 0.1).abs() < 0.01 && (back - full).abs() < 0.001, "the fade didn't set the volume");
        assert!(quiet < loud * 0.5, "the fade wasn't heard through the speakers");

        // Pause and play on: the page's own player, at once.
        spotify.command(&Command::Pause).unwrap();
        let paused = pump_until(5.0, &|| !playing.get());
        println!("pause: the page paused after {paused:?}");
        assert!(paused.is_some_and(|took| took < Duration::from_secs(2)), "pausing took {paused:?}");
        spotify.command(&Command::Play { context: None, track: None }).unwrap();
        let resumed = pump_until(10.0, &|| playing.get());
        println!("play on: the page playing after {resumed:?}");
        let button = spotify
            .view
            .evaluate("document.querySelector('[data-testid=\"control-button-playpause\"]')?.getAttribute('aria-label') ?? 'none'")
            .unwrap_or_default();
        println!("  element {}, the page's button {button}", element());
        if resumed.is_none() {
            pump_until(12.0, &|| false);
        }
        assert!(resumed.is_some_and(|took| took < Duration::from_secs(2)), "playing on took {resumed:?}");
        pump_until(2.0, &|| false);

        // Leaving the game and coming straight back, and the other way: each ends as the last asked.
        for round in 0..3 {
            spotify.command(&Command::Pause).unwrap();
            pump_until(0.15, &|| false);
            spotify.command(&Command::Play { context: None, track: None }).unwrap();
            pump_until(3.0, &|| false);
            let ended = element();
            println!("pause, then play on: playing={} element {ended}", playing.get());
            assert!(playing.get() && ended["paused"] == false, "round {round}: stopped after a pause and a play on");
            // Paused (away), then back and away again before the play on has landed.
            spotify.command(&Command::Pause).unwrap();
            pump_until(1.0, &|| false);
            spotify.command(&Command::Play { context: None, track: None }).unwrap();
            pump_until(0.1 + 0.1 * round as f64, &|| false);
            spotify.command(&Command::Pause).unwrap();
            // Kept paused as the play on lands, unheard meanwhile.
            let heard = loudness(3.0);
            println!("paused, play on, then pause: playing={} element {} loudest {heard:.4}", playing.get(), element());
            assert!(!playing.get(), "round {round}: playing after a play on and a pause");
            assert!(heard < 0.01, "round {round}: heard ({heard:.4}) as its pause was kept");
            spotify.command(&Command::Play { context: None, track: None }).unwrap();
            pump_until(3.0, &|| playing.get());
        }

        spotify.command(&Command::Next).unwrap();
        pump_until(8.0, &|| false);
        spotify.command(&Command::Pause).unwrap();
        pump_until(1.0, &|| false);
        spotify.close(&stage);
    }
}
