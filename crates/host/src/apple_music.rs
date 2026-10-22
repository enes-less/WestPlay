//! Apple Music: its own web player (music.apple.com), in a WebView of its own, hidden under the cabin page.
//! The Music app the player sees is WestPlay's own (ui/src/cabin/apps/music); the web player plays (MusicKit,
//! Apple's own player, which the page runs) and answers the app's questions, from its own page: a script in
//! it (`apple_music.js`) asks Apple Music's API through MusicKit, with the page's own tokens, and plays
//! through MusicKit's queue, as the page itself does.
//!
//! Signing in is Apple Music's own: its web player's sign-in dialog, in a window on the PC's desktop
//! (`signin.rs`), on this WebView's profile (one of its own: signing out forgets Apple's cookies alone); signed
//! in there, the web player here loads again, signed in. Nothing of the account passes through WestPlay: the
//! page's tokens stay in the page; what comes out is what the app shows (names, covers, what plays), only the fields below
//! (anything else the page might say is dropped here), and whether it's signed in. Nothing of it is kept or
//! logged.
//!
//! Its sound comes from the truck's own speakers, as YouTube's does (`cab_sound.js`, told the truck's speakers,
//! the driver's head and the camera by the host): Apple Music's songs are protected, but unlike Spotify's,
//! the browser lets a page's own sound processing have them (measured on a signed-in account, 2026-10-16).
//! Signed out, or without a subscription, Apple Music plays each song's preview (90 seconds).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use crate::signin::{self, Confirming, Progress, SignInWindow};
use crate::speakers::{self, Equalizer, Head, Speaker};
use crate::webview::{self, OffscreenWebView, Stage};

/// Apple Music's web player.
pub const HOME: &str = "https://music.apple.com/";

/// What the cabin page asks of Apple Music (`appleMusic` in ui/src/shared/host.ts).
#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "action", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Command {
    /// Run (hidden), saying whether it's signed in: the Music app is open.
    Start,
    /// Apple Music's sign-in, in a window on the desktop.
    SignIn,
    /// Forget the sign-in, and start again signed out.
    SignOut,
    /// A question for Apple Music, answered with the same `id`.
    Ask {
        id: u32,
        question: Question,
    },
    /// Play an album, playlist, station, artist or song (`id`), or a list of songs (`songs`), from the song at
    /// `index`; with neither, play on.
    Play {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        songs: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shuffle: Option<bool>,
    },
    Pause,
    Next,
    Previous,
    Seek {
        seconds: f64,
    },
    /// The page's player's volume, 0 to 100.
    Volume {
        percent: f64,
    },
    Shuffle {
        on: bool,
    },
    Repeat {
        mode: Repeat,
    },
    /// A song of the queue, `offset` songs after the one playing.
    Jump {
        offset: u32,
    },
    /// A song favourited (`on`), or not.
    Favorite {
        id: String,
        on: bool,
    },
    /// An album, playlist or song added to the Library.
    Add {
        id: String,
    },
}

impl Command {
    /// For Apple Music's page (all but the host's own: running, and signing in).
    fn for_page(&self) -> bool {
        !matches!(self, Command::Start | Command::SignIn)
    }
}

/// What the host tells Apple Music's page of the cab (never the cabin page's to send): the truck's speakers
/// (`None`: as it is, a truck whose speakers aren't known), the driver's head, and the camera outside (its
/// distance, metres; 0: not known). As YouTube's (youtube.rs).
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

/// What the page was told last of the cab (sent again to a page that loaded anew): the truck, and the
/// speakers, head, camera and tone messages.
#[derive(Debug, Default)]
struct Told {
    truck: Option<String>,
    head: Option<Head>,
    camera: Option<(bool, f32)>,
    equalizer: Option<Equalizer>,
    spectrum: Option<bool>,
    messages: [Option<String>; 5],
}

/// What the Music app asks Apple Music.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "ask", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Question {
    /// Home, signed in: what was played lately, and Apple Music's picks.
    Home,
    New,
    Radio,
    /// Search's start: Apple Music's categories.
    Browse,
    /// A category's page.
    Curator {
        id: String,
    },
    Search {
        query: String,
    },
    /// An album's or playlist's page.
    Collection {
        id: String,
    },
    Artist {
        id: String,
    },
    /// A list of the Library: sorted by `sort` (one of the sorts it said it has), a folder's (playlists), the
    /// sorts' names in `language` (the cab's: `en`, `tr`).
    Library {
        shelf: Shelf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sort: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        folder: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
    },
    /// A song's words (`songs:…`, or the Library's `library-songs:…`).
    Lyrics {
        id: String,
    },
}

impl Question {
    /// The question with only what Apple Music's page may put into its requests' addresses: a sort's name
    /// (letters, `-`, `.`), a folder of the Library's own (`library-playlist-folders:…`), a language the cab
    /// has. Anything else is left out (the list then comes as Apple Music sorts it, from the top).
    fn checked(self) -> Self {
        match self {
            Question::Library { shelf, sort, folder, language } => Question::Library {
                shelf,
                sort: sort.filter(|sort| {
                    (1..=40).contains(&sort.len())
                        && sort.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-' || b == b'.')
                }),
                folder: folder.filter(|folder| {
                    folder.strip_prefix("library-playlist-folders:").is_some_and(|id| {
                        (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
                    })
                }),
                language: language.filter(|language| matches!(language.as_str(), "en" | "tr")),
            },
            // A song of Apple Music's (its number) or of the Library (`i.` and letters); else none (no words).
            Question::Lyrics { id } => {
                let sound = id
                    .strip_prefix("songs:")
                    .is_some_and(|key| !key.is_empty() && key.len() <= 20 && key.bytes().all(|b| b.is_ascii_digit()))
                    || id.strip_prefix("library-songs:").is_some_and(|key| {
                        (1..=64).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
                    });
                Question::Lyrics { id: if sound { id } else { String::new() } }
            }
            other => other,
        }
    }
}

/// A list of the Library.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Shelf {
    Recent,
    Playlists,
    Artists,
    Albums,
    Songs,
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
    Album,
    Playlist,
    Artist,
    Station,
    Song,
    Curator,
    /// A folder of the Library's playlists.
    Folder,
}

/// A card or row: what it is (its id, Apple Music's type and own id: `albums:1440…`), its name, who it's by,
/// its editors' note, its cover (and a wide picture, if Apple's editors made one) and the cover's colour.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub wide: Option<String>,
    pub color: String,
    #[serde(default)]
    pub explicit: bool,
    #[serde(default)]
    pub live: bool,
    /// A big card's label ("UPDATED PLAYLIST").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A favourite (the Library's lists say).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite: Option<bool>,
}

/// A song: its name, artist, album (and their pages), cover, length (seconds), number on its album, and
/// whether it's explicit or a favourite.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    pub album_id: Option<String>,
    #[serde(default)]
    pub artist_id: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    pub color: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub explicit: bool,
    #[serde(default)]
    pub number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Style {
    /// Big cards with Apple's editors' pictures.
    Hero,
    Shelf,
    /// What was played lately (Home's first, untitled).
    Recent,
    Songs,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Section {
    #[serde(default)]
    pub title: String,
    pub style: Style,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub songs: Vec<Track>,
}

/// Home, New, Radio or a category: its sections, in order.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Page {
    #[serde(default)]
    pub sections: Vec<Section>,
}

/// An album's or playlist's page.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Collection {
    pub item: Item,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub genre: String,
    #[serde(default)]
    pub year: Option<i32>,
    #[serde(default)]
    pub tracks: Vec<Track>,
}

/// An artist's page: their top songs, then their albums and more.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ArtistPage {
    pub item: Item,
    #[serde(default)]
    pub top: Vec<Track>,
    #[serde(default)]
    pub sections: Vec<Section>,
}

/// A search's results.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Results {
    #[serde(default)]
    pub top: Vec<Item>,
    #[serde(default)]
    pub songs: Vec<Track>,
    #[serde(default)]
    pub artists: Vec<Item>,
    #[serde(default)]
    pub albums: Vec<Item>,
    #[serde(default)]
    pub playlists: Vec<Item>,
    #[serde(default)]
    pub stations: Vec<Item>,
}

/// A list of the Library: its cards, or its songs; the sorts it has (Apple Music's, named in the cab's
/// language; a playlist's, named by the cab) and the one it's in.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct LibraryShelf {
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub songs: Vec<Track>,
    #[serde(default)]
    pub sorts: Vec<SortOption>,
    #[serde(default)]
    pub sort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SortOption {
    pub id: String,
    #[serde(default)]
    pub label: String,
}

/// Apple Music's answer to the app's question `answer` (its id), under the question's name; `failed` when
/// Apple Music didn't answer.
#[derive(Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Answer {
    #[serde(rename(serialize = "id"))]
    pub answer: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<Page>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<Page>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radio: Option<Page>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browse: Option<Vec<Item>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curator: Option<Page>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<Results>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<Collection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<ArtistPage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<LibraryShelf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<crate::lyrics::Lyrics>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub failed: bool,
}

/// Where a song plays from (as the app asked for it).
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Context {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub kind: Kind,
}

/// What Apple Music plays: the song, where from, how far (seconds), whether it's the song's preview, the
/// options, the volume and the songs to come.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Playback {
    #[serde(default)]
    pub track: Option<Track>,
    #[serde(default)]
    pub context: Option<Context>,
    pub playing: bool,
    /// Playing as asked, its sound not come yet (loading, waiting for its stream, seeking).
    #[serde(default)]
    pub loading: bool,
    #[serde(default)]
    pub position: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub shuffle: bool,
    pub repeat: Repeat,
    /// The page's player's volume, 0 to 1.
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub next: Vec<Track>,
}

/// Whether the page is signed in (and the account's picture, its Apple Music profile's, on Apple's own image
/// hosts only), and playing.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub signed_in: bool,
    pub playing: bool,
    #[serde(default)]
    pub avatar: Option<String>,
}

impl State {
    /// Only a picture on Apple's image hosts passes, and only while signed in.
    fn checked(mut self) -> Self {
        if !self.signed_in || !self.avatar.as_deref().is_some_and(is_apple_image) {
            self.avatar = None;
        }
        self
    }
}

/// An image on Apple's own image hosts (`https://is1-ssl.mzstatic.com/…`).
fn is_apple_image(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://is") else { return false };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && rest[digits..].starts_with("-ssl.mzstatic.com/")
}

#[derive(Debug, PartialEq)]
pub enum Report {
    /// The music's spectrum, while the cab's Equalizer asks for it ([`speakers::read_spectrum`]).
    Spectrum(Vec<u8>),
    State(State),
    /// The sign-in window opened (`true`) or closed.
    SignInWindow(bool),
    /// Signed in in its window, and Apple Music's page here is yet to say it's signed in (`true`), or it has
    /// (or took too long: `false`).
    Confirming(bool),
    Answer(Box<Answer>),
    /// What plays (`None`: nothing).
    Playback(Option<Box<Playback>>),
    /// How long a play, next or previous took.
    Timing(Timing),
}

/// How long a play (or a next, a previous) took, in milliseconds: until MusicKit had it, until the song
/// sounded (`None`: not in 20 s); a full song or a preview. Times only, for the log.
#[derive(Clone, Debug, PartialEq)]
pub struct Timing {
    pub action: &'static str,
    pub queued: u64,
    pub sounding: Option<u64>,
    pub full: bool,
    /// Its steps and their times (`licence> 120, licence 4980, canplay 5010`): kinds of steps only.
    pub steps: String,
}

/// A play's steps as the page said them, only what a step is made of (its kind's letters, its time's digits,
/// `> ! :` and the separators), and not too long.
fn steps_of(said: &str) -> String {
    said.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | ',' | '>' | '!' | ':')).take(2400).collect()
}

/// A play slower than this logs its steps.
const SLOW_MS: u64 = 3000;

impl Timing {
    /// `[apple music] next, a full song: ready in 120 ms, sound in 5030 ms`
    pub fn line(&self) -> String {
        let kind = if self.full { "a full song" } else { "a preview" };
        let sound = self.sounding.map_or_else(|| "no sound in 20 s".to_string(), |ms| format!("sound in {ms} ms"));
        // The steps only when it was slow: where the time went.
        let slow = self.sounding.is_none_or(|ms| ms > SLOW_MS);
        let steps = if slow && !self.steps.is_empty() { format!(" (steps: {})", self.steps) } else { String::new() };
        format!("[apple music] {}, {kind}: ready in {} ms, {sound}{steps}", self.action, self.queued)
    }
}

pub struct AppleMusic {
    view: OffscreenWebView,
    sign_in: RefCell<Option<SignInWindow>>,
    /// Signed in in its window, until the page here says it's signed in.
    confirming: Confirming,
    pending: RefCell<Vec<Report>>,
    /// The app's questions not answered yet: asked again as the page is ready anew (a page that loads again,
    /// its country changed or signed in, misses what was sent while it loaded).
    unanswered: RefCell<Unanswered>,
    /// The cab as the page was told it ([`AppleMusic::cab`]).
    told: RefCell<Told>,
}

/// Questions asked of the page, by id, until answered (the latest `MOST`: the app gives up on older ones).
#[derive(Debug, Default)]
struct Unanswered(BTreeMap<u32, String>);

impl Unanswered {
    const MOST: usize = 32;

    fn asked(&mut self, id: u32, sent: String) {
        self.0.insert(id, sent);
        while self.0.len() > Self::MOST {
            self.0.pop_first();
        }
    }

    fn answered(&mut self, id: u32) {
        self.0.remove(&id);
    }

    /// What to ask again, oldest first.
    fn all(&self) -> Vec<String> {
        self.0.values().cloned().collect()
    }
}

impl AppleMusic {
    /// Its WebView (performance.rs counts its processes).
    pub fn view(&self) -> &OffscreenWebView {
        &self.view
    }

    /// Opens Apple Music's web player in a WebView of `stage`'s frame (`width` x `height`), hidden under the
    /// cabin page.
    pub fn start(stage: &Stage, width: u32, height: u32, user_data: &Path) -> Result<Self, webview::Error> {
        let view = OffscreenWebView::new(stage, width, height, user_data)?;
        view.log_as("apple music")?;
        view.show(false)?;
        stage.add(&view, false)?;
        // The truck speakers' sound first: the page's script makes its own of it.
        view.add_startup_script(concat!(include_str!("cab_sound.js"), "\n", include_str!("apple_music.js")))?;
        view.navigate(HOME)?;
        Ok(Self {
            view,
            sign_in: RefCell::new(None),
            confirming: Confirming::default(),
            pending: RefCell::new(Vec::new()),
            unanswered: RefCell::default(),
            told: RefCell::default(),
        })
    }

    /// The truck (its SDK model id), the driver's head, and whether the camera is outside the cab and how far
    /// (metres; 0: not known): the page is told the truck's speakers as the truck changes, the head as it moves,
    /// the camera as it goes in or out or a quarter metre further, the tone as it's set and the spectrum as it's wanted (as YouTube's: youtube.rs).
    pub fn cab(
        &self,
        truck: &str,
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
        if told.truck.as_deref() != Some(truck) {
            told.truck = Some(truck.to_owned());
            let speakers = speakers::in_truck(truck).map(<[_]>::to_vec);
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

    pub fn command(&self, command: &Command) -> windows::core::Result<()> {
        match command {
            Command::SignIn => {
                self.open_sign_in();
                Ok(())
            }
            Command::Ask { id, question } => {
                let checked = Command::Ask { id: *id, question: question.clone().checked() };
                let sent = serde_json::to_string(&checked).unwrap_or_default();
                self.unanswered.borrow_mut().asked(*id, sent.clone());
                self.view.post_json(&sent)
            }
            _ if command.for_page() => self.view.post_json(&serde_json::to_string(command).unwrap_or_default()),
            _ => Ok(()),
        }
    }

    /// Apple Music's sign-in window, opened (or brought forward if it is).
    fn open_sign_in(&self) {
        let mut sign_in = self.sign_in.borrow_mut();
        if let Some(window) = sign_in.as_ref() {
            window.bring_forward();
            return;
        }
        match SignInWindow::open(self.view.environment(), &signin::APPLE_MUSIC) {
            Ok(window) => {
                *sign_in = Some(window);
                self.pending.borrow_mut().push(Report::SignInWindow(true));
            }
            Err(e) => crate::startup::mark(&format!("host: the Apple Music sign-in window couldn't open: {e}")),
        }
    }

    /// What Apple Music's page said since the last call, and the sign-in window opening or closing (signed in,
    /// the web player here loads again, signed in).
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
                Some(Message::SignedOut) => {
                    let _ = self.view.delete_cookies();
                    let _ = self.view.navigate(HOME);
                }
                Some(Message::Timing(timing)) => reports.push(Report::Timing(timing)),
                Some(Message::Ready) => {
                    // A page loaded anew: the cab as it is (speakers, head, camera), and the questions it missed.
                    for told in self.told.borrow().messages.iter().flatten() {
                        let _ = self.view.post_json(told);
                    }
                    for sent in self.unanswered.borrow().all() {
                        let _ = self.view.post_json(&sent);
                    }
                }
                Some(Message::State(state)) => reports.push(Report::State(state)),
                Some(Message::Answer(answer)) => {
                    self.unanswered.borrow_mut().answered(answer.answer);
                    reports.push(Report::Answer(answer));
                }
                Some(Message::Playback(playback)) => reports.push(Report::Playback(playback)),
                None => {}
            }
        }
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

/// A message of Apple Music's page (`apple_music.js`).
#[derive(Debug, PartialEq)]
enum Message {
    /// MusicKit forgot the account: Apple's cookies next, and the page again.
    SignedOut,
    /// The page is ready (loaded anew).
    Ready,
    Timing(Timing),
    State(State),
    Answer(Box<Answer>),
    Playback(Option<Box<Playback>>),
}

fn read_message(message: &str) -> Option<Message> {
    let value: serde_json::Value = serde_json::from_str(message).ok()?;
    if value.get("signedOut").and_then(serde_json::Value::as_bool) == Some(true) {
        return Some(Message::SignedOut);
    }
    if value.get("ready").and_then(serde_json::Value::as_bool) == Some(true) {
        return Some(Message::Ready);
    }
    if let Some(timing) = value.get("timing") {
        let action = match timing.get("action").and_then(serde_json::Value::as_str) {
            Some("next") => "next",
            Some("previous") => "previous",
            _ => "play",
        };
        return Some(Message::Timing(Timing {
            action,
            queued: timing.get("queued")?.as_u64()?,
            sounding: timing.get("sounding").and_then(serde_json::Value::as_u64),
            full: timing.get("full").and_then(serde_json::Value::as_bool).unwrap_or(false),
            steps: steps_of(timing.get("steps").and_then(serde_json::Value::as_str).unwrap_or("")),
        }));
    }
    if value.get("answer").is_some() {
        return match serde_json::from_value::<Answer>(value.clone()) {
            Ok(mut answer) => {
                answer.lyrics = answer.lyrics.map(crate::lyrics::Lyrics::checked);
                Some(Message::Answer(Box::new(answer)))
            }
            // An answer that doesn't read (Apple Music changed what it says): failed, so the app isn't left
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
    if let Some(playback) = value.get("playback") {
        if playback.is_null() {
            return Some(Message::Playback(None));
        }
        return serde_json::from_value::<Playback>(playback.clone()).ok().map(|p| Message::Playback(Some(Box::new(p))));
    }
    serde_json::from_value::<State>(value).ok().map(|state| Message::State(state.checked()))
}

/// `{"type":"appleMusic","signedIn":true,"playing":false,"signingIn":false,"avatar":…}` for the cabin page
/// (`null` fields when Apple Music isn't running).
pub fn state_message(state: Option<&State>, signing_in: bool) -> String {
    serde_json::json!({
        "type": "appleMusic",
        "signedIn": state.map(|s| s.signed_in),
        "playing": state.map(|s| s.playing),
        "signingIn": signing_in,
        "avatar": state.and_then(|s| s.avatar.as_deref()),
    })
    .to_string()
}

/// `{"type":"appleMusicAnswer","id":…,"new":{…}}`: Apple Music's answer to the app's question.
pub fn answer_message(answer: &Answer) -> String {
    let mut message = serde_json::to_value(answer).unwrap_or_default();
    message["type"] = "appleMusicAnswer".into();
    message.to_string()
}

/// `{"type":"appleMusicPlayback","playback":{…}}` (`null`: nothing plays).
pub fn playback_message(playback: Option<&Playback>) -> String {
    serde_json::json!({ "type": "appleMusicPlayback", "playback": playback }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_cabin_pages_commands() {
        let ask: Command =
            serde_json::from_str(r#"{"action":"ask","id":3,"question":{"ask":"library","shelf":"albums"}}"#).unwrap();
        assert_eq!(
            ask,
            Command::Ask {
                id: 3,
                question: Question::Library { shelf: Shelf::Albums, sort: None, folder: None, language: None }
            }
        );
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"action":"ask","id":4,"question":{"ask":"new"}}"#).unwrap(),
            Command::Ask { id: 4, question: Question::New }
        );
        let play: Command = serde_json::from_str(r#"{"action":"play","id":"albums:1","index":2}"#).unwrap();
        assert_eq!(play, Command::Play { id: Some("albums:1".into()), songs: None, index: Some(2), shuffle: None });
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"action":"play"}"#).unwrap(),
            Command::Play { id: None, songs: None, index: None, shuffle: None }
        );
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"action":"favorite","id":"songs:9","on":true}"#).unwrap(),
            Command::Favorite { id: "songs:9".into(), on: true }
        );
        assert_eq!(serde_json::from_str::<Command>(r#"{"action":"start"}"#).unwrap(), Command::Start);
        assert!(serde_json::from_str::<Command>(r#"{"action":"explode"}"#).is_err());
    }

    #[test]
    fn passes_the_page_its_commands_as_the_app_sent_them() {
        let ask = Command::Ask { id: 9, question: Question::Search { query: "tarkan".into() } };
        assert_eq!(
            serde_json::to_string(&ask).unwrap(),
            r#"{"action":"ask","id":9,"question":{"ask":"search","query":"tarkan"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Command::Play {
                id: None,
                songs: Some(vec!["songs:1".into()]),
                index: Some(0),
                shuffle: None
            })
            .unwrap(),
            r#"{"action":"play","songs":["songs:1"],"index":0}"#
        );
        // Signing out is MusicKit's own, in the page; signing in, Apple's window.
        assert!(ask.for_page() && Command::SignOut.for_page());
        assert!(!Command::Start.for_page() && !Command::SignIn.for_page());
    }

    /// Only a song's id of Apple Music's shapes reaches the page's request for its words.
    #[test]
    fn passes_the_page_only_a_songs_id_for_its_words() {
        let checked = |id: &str| match (Question::Lyrics { id: id.into() }).checked() {
            Question::Lyrics { id } => id,
            other => panic!("{other:?}"),
        };
        assert_eq!(checked("songs:1440818839"), "songs:1440818839");
        assert_eq!(checked("library-songs:i.4ZqWbGRCo0k6"), "library-songs:i.4ZqWbGRCo0k6");
        assert_eq!(checked("songs:1/../me"), "");
        assert_eq!(checked("albums:1"), "");
    }

    /// Only a sort's name, a folder of the Library's and a language the cab has reach the page's requests.
    #[test]
    fn passes_the_page_only_safe_parts_of_a_librarys_question() {
        let asked = |sort: &str, folder: &str, language: &str| {
            Question::Library {
                shelf: Shelf::Playlists,
                sort: Some(sort.into()),
                folder: Some(folder.into()),
                language: Some(language.into()),
            }
            .checked()
        };
        assert_eq!(
            asked("-dateAdded", "library-playlist-folders:p.Ab12", "tr"),
            Question::Library {
                shelf: Shelf::Playlists,
                sort: Some("-dateAdded".into()),
                folder: Some("library-playlist-folders:p.Ab12".into()),
                language: Some("tr".into()),
            }
        );
        assert_eq!(
            asked("name&ids=x", "library-playlist-folders:../../me/x", "xx"),
            Question::Library { shelf: Shelf::Playlists, sort: None, folder: None, language: None }
        );
        assert_eq!(
            asked("name", "p.1", "en"),
            Question::Library {
                shelf: Shelf::Playlists,
                sort: Some("name".into()),
                folder: None,
                language: Some("en".into()),
            }
        );
    }

    /// How long playing takes in the hidden WebView, Apple Music's previews (signed out): a play, then next
    /// and previous, each timed from the page's command to the song's time moving.
    #[test]
    #[ignore = "plays from music.apple.com"]
    fn times_playing_in_the_hidden_web_player() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        // A signed-in profile to time full songs with (WESTPLAY_APPLE_MUSIC_PROFILE, the player's own: never
        // deleted), or a fresh one, signed out (previews), deleted after.
        let own = std::env::var_os("WESTPLAY_APPLE_MUSIC_PROFILE").map(std::path::PathBuf::from);
        let user_data = own.clone().unwrap_or_else(|| {
            std::env::temp_dir().join(format!("westplay-apple-music-timing-{}", std::process::id()))
        });
        let stage = Stage::new(1280, 720).unwrap();
        let music = AppleMusic::start(&stage, 1280, 720, &user_data).unwrap();
        // Never heard: the WebView makes no sound (the page still makes it, and measures it).
        music.view.set_muted(true).unwrap();
        let mut timings = Vec::new();
        let mut ready = false;
        let pump = |seconds: u64, timings: &mut Vec<Timing>, ready: &mut bool| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in music.take_reports() {
                    match report {
                        Report::Timing(timing) => {
                            println!("{} [steps: {}]", timing.line(), timing.steps);
                            timings.push(timing);
                        }
                        Report::State(_) => *ready = true,
                        // What the app is told as a song starts: loading (its time held), then sounding.
                        Report::Playback(Some(playback)) => println!(
                            "  playback: {} playing={} loading={} at {:.2} s",
                            playback.track.as_ref().map_or("-", |t| t.name.as_str()),
                            playback.playing,
                            playback.loading,
                            playback.position
                        ),
                        _ => {}
                    }
                }
            }
        };
        // The first load of the web player, on a fresh profile.
        for _ in 0..30 {
            pump(1, &mut timings, &mut ready);
            if ready {
                break;
            }
        }
        assert!(ready, "the web player said nothing");
        music.command(&Command::Volume { percent: 2.0 }).unwrap();
        // Through a Scania S's speakers, heard from the driver's seat (the way the cab plays it).
        let head = Head { x: -0.47, y: 1.33, z: 0.32, heading: 0.0, pitch: 0.0 };
        music.cab("scania.s_2016", head, false, 0.0, Equalizer::default(), false).unwrap();
        music
            .command(&Command::Play {
                id: Some(std::env::var("WESTPLAY_ALBUM").unwrap_or_else(|_| "albums:499332675".into())),
                songs: None,
                index: None,
                shuffle: Some(false),
            })
            .unwrap();
        pump(12, &mut timings, &mut ready);
        for _ in 0..2 {
            music.command(&Command::Next).unwrap();
            pump(10, &mut timings, &mut ready);
        }
        music.command(&Command::Previous).unwrap();
        pump(10, &mut timings, &mut ready);
        music.command(&Command::Pause).unwrap();
        pump(1, &mut timings, &mut ready);
        music.close(&stage);
        if own.is_none() {
            let _ = std::fs::remove_dir_all(&user_data);
        }
        // A play and two nexts (the previous, late in a song, goes back to its start: not timed).
        assert_eq!(timings.len(), 3, "{timings:?}");
        for timing in &timings {
            assert!(timing.sounding.is_some(), "{} never sounded", timing.action);
            assert!(!timing.steps.is_empty(), "{} has no steps", timing.action);
        }
    }

    /// The page is told the cab as YouTube's is: the speakers by the host's own table, the head flat.
    #[test]
    fn tells_the_page_the_cab() {
        let head = Head { x: 0.0, y: 0.3, z: 0.1, heading: 0.0, pitch: 0.0 };
        assert_eq!(
            serde_json::to_string(&Cab::Head { head }).unwrap(),
            r#"{"action":"head","x":0.0,"y":0.3,"z":0.1,"heading":0.0,"pitch":0.0}"#
        );
        assert_eq!(
            serde_json::to_string(&Cab::Speakers { speakers: None }).unwrap(),
            r#"{"action":"speakers","speakers":null}"#
        );
        assert!(
            serde_json::to_string(&Cab::Speakers { speakers: speakers::in_truck("scania.s_2016").map(<[_]>::to_vec) })
                .unwrap()
                .starts_with(r#"{"action":"speakers","speakers":[{"#)
        );
        assert_eq!(
            serde_json::to_string(&Cab::Outside { on: true, distance: 6.25 }).unwrap(),
            r#"{"action":"outside","on":true,"distance":6.25}"#
        );
        // Not the cabin page's to send.
        assert!(serde_json::from_str::<Command>(r#"{"action":"speakers","speakers":null}"#).is_err());
    }

    /// Paused mid-second and played on, the song's time carries on from where it paused: the page never says
    /// a time behind it (nor a whole second for a time between).
    #[test]
    #[ignore = "plays from music.apple.com"]
    fn plays_on_from_where_it_paused() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let own = std::env::var_os("WESTPLAY_APPLE_MUSIC_PROFILE").map(std::path::PathBuf::from);
        let user_data = own.clone().unwrap_or_else(|| {
            std::env::temp_dir().join(format!("westplay-apple-music-resume-{}", std::process::id()))
        });
        let stage = Stage::new(1280, 720).unwrap();
        let music = AppleMusic::start(&stage, 1280, 720, &user_data).unwrap();
        // Never heard: the WebView makes no sound (the page still makes it, and measures it).
        music.view.set_muted(true).unwrap();
        // Every time the page says, with when it said it.
        let started = std::time::Instant::now();
        let mut said: Vec<(f64, bool, bool, f64)> = Vec::new();
        let mut ready = false;
        let pump = |seconds: f64, said: &mut Vec<(f64, bool, bool, f64)>, ready: &mut bool| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in music.take_reports() {
                    match report {
                        Report::State(_) => *ready = true,
                        Report::Playback(Some(playback)) => {
                            let at = started.elapsed().as_secs_f64();
                            println!(
                                "  {at:6.2} s: playing={} loading={} position={:.2}",
                                playback.playing, playback.loading, playback.position
                            );
                            said.push((at, playback.playing, playback.loading, playback.position));
                        }
                        _ => {}
                    }
                }
            }
        };
        for _ in 0..30 {
            pump(1.0, &mut said, &mut ready);
            if ready {
                break;
            }
        }
        assert!(ready, "the web player said nothing");
        music.command(&Command::Volume { percent: 2.0 }).unwrap();
        let album = std::env::var("WESTPLAY_ALBUM").unwrap_or_else(|_| "albums:499332675".into());
        music.command(&Command::Play { id: Some(album), songs: None, index: None, shuffle: Some(false) }).unwrap();
        pump(14.6, &mut said, &mut ready);
        println!("pause");
        music.command(&Command::Pause).unwrap();
        pump(3.0, &mut said, &mut ready);
        println!("play on");
        music.command(&Command::Play { id: None, songs: None, index: None, shuffle: None }).unwrap();
        pump(5.0, &mut said, &mut ready);
        music.command(&Command::Pause).unwrap();
        pump(1.0, &mut said, &mut ready);
        music.close(&stage);
        if own.is_none() {
            let _ = std::fs::remove_dir_all(&user_data);
        }
        // Where it paused, and every time said after it while playing: none behind it.
        let paused = said.iter().find(|(_, playing, ..)| !playing).expect("it paused").3;
        let after: Vec<f64> = said.iter().skip_while(|(_, playing, ..)| *playing).map(|said| said.3).collect();
        for position in &after {
            assert!(*position >= paused - 0.25, "said {position:.2} s after pausing at {paused:.2} s: {said:?}");
        }
    }

    /// Questions are held until answered, to be asked again of a page loaded anew; only the latest.
    #[test]
    fn holds_the_questions_not_answered_yet() {
        let mut unanswered = Unanswered::default();
        unanswered.asked(1, "one".into());
        unanswered.asked(2, "two".into());
        unanswered.answered(1);
        assert_eq!(unanswered.all(), vec!["two".to_string()]);
        for id in 3..60 {
            unanswered.asked(id, id.to_string());
        }
        assert_eq!(unanswered.all().len(), Unanswered::MOST);
        assert_eq!(unanswered.all().last().map(String::as_str), Some("59"));
        assert_eq!(read_message(r#"{"ready":true}"#), Some(Message::Ready));
        assert_eq!(
            read_message(
                r#"{"timing":{"action":"next","queued":420,"sounding":1830,"full":true,"steps":"licence> 12, licence 5020, <b>https://x/?t=1</b>"}}"#
            ),
            Some(Message::Timing(Timing {
                action: "next",
                queued: 420,
                sounding: Some(1830),
                full: true,
                // Only a step's letters, digits and signs: no address, nothing else the page might say.
                steps: "licence> 12, licence 5020, b>https:xt1b>".into(),
            }))
        );
        assert_eq!(
            Timing { action: "play", queued: 300, sounding: None, full: false, steps: String::new() }.line(),
            "[apple music] play, a preview: ready in 300 ms, no sound in 20 s"
        );
    }

    #[test]
    fn reads_apple_musics_page() {
        assert_eq!(read_message(r#"{"signedOut":true}"#), Some(Message::SignedOut));
        assert_eq!(
            read_message(r#"{"signedIn":true,"playing":false}"#),
            Some(Message::State(State { signed_in: true, playing: false, avatar: None }))
        );
        assert_eq!(read_message(r#"{"playback":null}"#), Some(Message::Playback(None)));
        assert_eq!(read_message("not json"), None);
    }

    /// Only the fields the app shows pass: whatever else the page says (here, an account's storefront and
    /// token) is dropped.
    #[test]
    fn passes_on_only_what_the_app_shows() {
        let page = r##"{"answer":4,"library":{"items":[{"id":"library-playlists:p.1","kind":"playlist","name":"Road",
            "by":"","image":null,"color":"#2c2c2e","storefront":"tr","owner":"someone"}],"songs":[]},
            "musicUserToken":"secret"}"##;
        let Some(Message::Answer(answer)) = read_message(page) else { panic!("not an answer") };
        assert_eq!(
            answer_message(&answer),
            r##"{"id":4,"library":{"items":[{"by":"","color":"#2c2c2e","explicit":false,"id":"library-playlists:p.1","image":null,"kind":"playlist","live":false,"name":"Road","note":"","wide":null}],"songs":[],"sort":null,"sorts":[]},"type":"appleMusicAnswer"}"##
        );
    }

    /// The account's picture passes only from Apple's image hosts, and only signed in; nothing else of the
    /// account does (here, a name).
    #[test]
    fn passes_on_only_the_accounts_picture() {
        let picture = "https://is1-ssl.mzstatic.com/image/thumb/a/160x160bb.jpg";
        let Some(Message::State(state)) =
            read_message(&format!(r#"{{"signedIn":true,"playing":false,"avatar":"{picture}","name":"Someone"}}"#))
        else {
            panic!("not a state")
        };
        assert_eq!(state.avatar.as_deref(), Some(picture));
        let message = state_message(Some(&state), false);
        assert!(message.contains(picture) && !message.contains("Someone"), "{message}");
        for elsewhere in [
            "https://evil.example/a.jpg",
            "https://is1-ssl.mzstatic.com.evil.example/a.jpg",
            "http://is1-ssl.mzstatic.com/a.jpg",
        ] {
            let Some(Message::State(state)) =
                read_message(&format!(r#"{{"signedIn":true,"playing":false,"avatar":"{elsewhere}"}}"#))
            else {
                panic!("not a state")
            };
            assert_eq!(state.avatar, None, "{elsewhere}");
        }
        let Some(Message::State(state)) =
            read_message(&format!(r#"{{"signedIn":false,"playing":false,"avatar":"{picture}"}}"#))
        else {
            panic!("not a state")
        };
        assert_eq!(state.avatar, None, "signed out: no picture");
    }

    #[test]
    fn says_when_an_answer_doesnt_read() {
        let Some(Message::Answer(answer)) = read_message(r#"{"answer":5,"new":{"sections":"not a list"}}"#) else {
            panic!("not an answer")
        };
        assert_eq!(answer_message(&answer), r#"{"failed":true,"id":5,"type":"appleMusicAnswer"}"#);
    }

    #[test]
    fn passes_on_what_plays() {
        let page = r##"{"playback":{"track":{"id":"songs:499332705","name":"Şımarık","artist":"Tarkan","album":"Ölürüm Sana",
            "albumId":"albums:499332675","artistId":null,"image":null,"color":"#e0d8cd","duration":235.2,"explicit":false,
            "number":1,"favorite":true},"context":{"id":"albums:499332675","name":"Ölürüm Sana","kind":"album"},"playing":true,
            "position":12.5,"duration":90,"preview":true,"shuffle":false,"repeat":"all","volume":0.5,"next":[],
            "storefront":"tr"}}"##;
        let Some(Message::Playback(Some(playback))) = read_message(page) else { panic!("not a playback") };
        assert_eq!(playback.repeat, Repeat::All);
        assert!(playback.preview);
        let message = playback_message(Some(&playback));
        assert!(message.contains(r#""name":"Şımarık""#) && message.contains(r#""favorite":true"#));
        assert!(message.contains(r#""volume":0.5"#), "the volume passes: {message}");
        assert!(!message.contains("storefront"), "the account's storefront never passes: {message}");
    }

    /// A song's words, asked of the hidden web player as the cab asks them: of the Library's songs (by their Apple
    /// Music ones) and of Apple Music's (a search's). Muted; only whether they're synced and how many lines and
    /// writers there are is printed (WESTPLAY_APPLE_MUSIC_PROFILE: a signed-in profile, the player's own).
    #[test]
    #[ignore = "asks music.apple.com"]
    fn answers_a_songs_words() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data =
            std::path::PathBuf::from(std::env::var_os("WESTPLAY_APPLE_MUSIC_PROFILE").expect("a signed-in profile"));
        let stage = Stage::new(1280, 720).unwrap();
        let music = AppleMusic::start(&stage, 1280, 720, &user_data).unwrap();
        music.view.set_muted(true).unwrap();
        let ask = |id: u32, question: Question| -> Option<Box<Answer>> {
            music.command(&Command::Ask { id, question }).unwrap();
            let until = std::time::Instant::now() + std::time::Duration::from_secs(40);
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in music.take_reports() {
                    if let Report::Answer(answer) = report
                        && answer.answer == id
                    {
                        return Some(answer);
                    }
                }
            }
            None
        };
        let library = ask(1, Question::Library { shelf: Shelf::Songs, sort: None, folder: None, language: None })
            .and_then(|answer| answer.library)
            .expect("the Library's songs");
        let search =
            ask(2, Question::Search { query: "tarkan".into() }).and_then(|answer| answer.search).expect("a search");
        let ids: Vec<String> =
            library.songs.iter().take(3).chain(search.songs.iter().take(2)).map(|song| song.id.clone()).collect();
        let mut with_words = 0;
        for (n, id) in ids.iter().enumerate() {
            let lyrics = ask(10 + n as u32, Question::Lyrics { id: id.clone() })
                .and_then(|answer| answer.lyrics)
                .expect("an answer");
            let kind = id.split(':').next().unwrap_or_default();
            println!(
                "{kind}: synced {}, {} lines, {} writers",
                lyrics.synced,
                lyrics.lines.len(),
                lyrics.writers.len()
            );
            if !lyrics.lines.is_empty() {
                with_words += 1;
            }
        }
        assert!(with_words > 0, "none of {} songs had words", ids.len());
        music.close(&stage);
    }
}
