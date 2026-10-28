//! YouTube Music: its own web player (music.youtube.com), in a WebView of its own, hidden under the cabin page.
//! The YouTube Music app the player sees is WestPlay's own (ui/src/cabin/apps/youtubeMusic); the web player
//! plays (YouTube's own player, which the page runs) and answers the app's questions, from its own page: a
//! script in it (`youtube_music.js`) asks YouTube Music's API as its own site does, and plays through the page's
//! own queue and player bar, as its buttons do.
//!
//! It shares the cabin page's WebView2 profile with YouTube (youtube.rs), as a phone's two apps share one
//! Google account: a sign-in in either is both's. Signing in is Google's own page, in a window on the desktop
//! (signin.rs); signing out forgets the profile's cookies (both are signed out). Nothing of the account passes
//! through WestPlay: what comes out is what the app shows (names, covers, what plays), only the fields below
//! (anything else the page might say is dropped here), whether it's signed in, and the account's picture (from
//! Google's image hosts). Nothing of it is kept or logged.
//!
//! As YouTube's, the page asks for its songs as a player YouTube serves without ads (`youtube_music.js`), and its
//! sound comes from the truck's own speakers (`cab_sound.js`, told the truck's speakers, the driver's head and
//! the camera by the host).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use crate::signin::{self, Confirming, Progress, SignInWindow};
use crate::speakers::{self, Equalizer, Head, Speaker};
use crate::webview::{self, OffscreenWebView, PageSize, Stage};
use crate::youtube::{is_token, is_video_id, is_youtube_image, within};

/// YouTube Music's web player.
pub const HOME: &str = "https://music.youtube.com/";

/// What the cabin page asks of YouTube Music (`youtubeMusic` in ui/src/shared/host.ts).
#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "action", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Command {
    /// Run (hidden), saying whether it's signed in: the app is open.
    Start,
    /// Google's sign-in, in a window on the desktop.
    SignIn,
    /// Forget the sign-in (YouTube's too: they share it), and start again signed out.
    SignOut,
    /// A question for YouTube Music, answered with the same `id`.
    Ask {
        id: u32,
        question: Question,
    },
    /// The language YouTube Music answers in from now on (the cab's: `en`, `tr`).
    Language {
        code: String,
    },
    /// Play what `watch` says (a song, a playlist or album from a song, a radio), shuffled or not; with nothing,
    /// play on.
    Play {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        watch: Option<Watch>,
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
    /// The queue's song at `index` (its place in the queue as the playback said it).
    Jump {
        index: u32,
    },
    /// A song liked, disliked, or neither.
    Like {
        video: String,
        like: Rating,
    },
    /// An album or playlist saved to the Library (`on`), or taken out.
    Save {
        playlist: String,
        on: bool,
    },
    /// The Song/Video switch: the music video's picture (`true`), or the song's sound only.
    Video {
        on: bool,
    },
    /// A song added to the queue: right after the one playing (`next`), or at its end (a song's ⋮: Play next,
    /// Add to queue).
    Queue {
        video: String,
        next: bool,
    },
    /// Show the music video's picture at this rectangle of the frame (pixels), under the cabin page (Now
    /// Playing's, cut open there). The host's own: the page only ever shows its video.
    Show {
        x: f32,
        y: f32,
        width: u32,
        height: u32,
    },
    /// Show no picture (it plays on, heard).
    Hide,
}

impl Command {
    /// For YouTube Music's page (all but the host's own: running, and signing in and out), and only what's
    /// sound: ids, tokens and text of the shapes YouTube's have.
    fn for_page(&self) -> bool {
        match self {
            Command::Start | Command::SignIn | Command::SignOut | Command::Show { .. } | Command::Hide => false,
            Command::Language { code } => matches!(code.as_str(), "en" | "tr"),
            Command::Play { watch } => watch.as_ref().is_none_or(Watch::is_sound),
            Command::Like { video, .. } | Command::Queue { video, .. } => is_video_id(video),
            Command::Save { playlist, .. } => is_id(playlist),
            Command::Seek { seconds } => seconds.is_finite(),
            Command::Volume { percent } => percent.is_finite(),
            _ => true,
        }
    }
}

/// What starts playing: a song (`video`), from a list (`playlist`, at `index`), with YouTube's own `params`
/// for it (a radio, a shuffle), as YouTube Music's answers give them.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Watch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playlist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
}

impl Watch {
    /// A song or a list to play, every part of the shape YouTube's have.
    fn is_sound(&self) -> bool {
        (self.video.is_some() || self.playlist.is_some())
            && self.video.as_deref().is_none_or(is_video_id)
            && self.playlist.as_deref().is_none_or(is_id)
            && self.params.as_deref().is_none_or(is_params)
    }
}

/// A page of YouTube Music's (its `browseId`, and the `params` of a page of it: a mood, a chart).
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Browse {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<String>,
}

impl Browse {
    fn is_sound(&self) -> bool {
        is_id(&self.id) && self.params.as_deref().is_none_or(is_params)
    }
}

/// YouTube Music's id for something (a song's, a playlist's `VLPL…`, an album's `MPREb_…`, a channel's
/// `UC…`, a page's `FEmusic_…`): letters, digits, `-` and `_`, of a sensible length.
fn is_id(id: &str) -> bool {
    (1..=128).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// YouTube's `params` for a page or a play (base64, perhaps escaped): as its tokens are.
fn is_params(params: &str) -> bool {
    !params.is_empty() && params.len() <= 512 && is_token(params)
}

/// What the YouTube Music app asks.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "ask", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Question {
    /// Home: its moods (`chip`: one of them, its params), or the rest of it (`more`).
    Home {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chip: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        more: Option<String>,
    },
    Explore,
    /// A page of YouTube Music's (a mood's, new releases, charts, a shelf's "More").
    Browse {
        page: Browse,
    },
    /// A search, or one of its kinds only (`filter`: the chip's params), or more of it (`more`).
    Search {
        query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        more: Option<String>,
    },
    /// YouTube Music's suggestions for what's typed so far.
    Suggest {
        query: String,
    },
    /// An album's or playlist's page, or more of its songs (`more`).
    Collection {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        more: Option<String>,
    },
    Artist {
        id: String,
    },
    /// A list of the Library (signed in).
    Library {
        shelf: Shelf,
    },
    /// A song's words.
    Lyrics {
        video: String,
    },
    /// What goes with a song (more like it, its artist, playlists).
    Related {
        video: String,
    },
}

impl Question {
    /// Whether it's a question for the page: ids, tokens and text of the shapes YouTube's have.
    fn is_sound(&self) -> bool {
        let token = |more: &Option<String>| more.as_deref().is_none_or(|more| !more.is_empty() && is_token(more));
        match self {
            Question::Home { chip, more } => chip.as_deref().is_none_or(is_params) && token(more),
            Question::Explore | Question::Library { .. } => true,
            Question::Browse { page } => page.is_sound(),
            Question::Search { query, filter, more } => {
                (1..=200).contains(&query.chars().count()) && filter.as_deref().is_none_or(is_params) && token(more)
            }
            Question::Suggest { query } => query.chars().count() <= 200,
            Question::Collection { id, more } => is_id(id) && token(more),
            Question::Artist { id } => is_id(id),
            Question::Lyrics { video } | Question::Related { video } => is_video_id(video),
        }
    }
}

/// A list of the Library.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Shelf {
    Playlists,
    Songs,
    Albums,
    Artists,
    Subscriptions,
    History,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Repeat {
    Off,
    All,
    One,
}

/// A song's like: YouTube's thumbs up, down, or neither.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Rating {
    Like,
    Dislike,
    #[default]
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Song,
    Video,
    Episode,
    Album,
    Playlist,
    Artist,
    /// A profile (a user's channel).
    Channel,
    Podcast,
    /// A mood's or genre's button.
    Mood,
}

/// A card or a row: what it is, its title and YouTube Music's own line under it ("Album • Tarkan • 2010"), its
/// picture, its page, playing it, a song's artist and album pages and length, a mood's colour.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub kind: Kind,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browse: Option<Browse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play: Option<Watch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<Browse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<Browse>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub duration: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub explicit: bool,
    /// A mood's colour (`#rrggbb`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Its place on its album (a number), or in a chart.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rank: String,
}

impl Item {
    fn checked(mut self) -> Self {
        self.image = self.image.filter(|url| is_youtube_image(url));
        self.browse = self.browse.filter(Browse::is_sound);
        self.play = self.play.filter(Watch::is_sound);
        self.video = self.video.filter(|video| is_video_id(video));
        self.artist = self.artist.filter(Browse::is_sound);
        self.album = self.album.filter(Browse::is_sound);
        self.color = self.color.filter(|color| is_color(color));
        self
    }
}

fn is_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn items(list: Vec<Item>) -> Vec<Item> {
    list.into_iter().map(Item::checked).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Style {
    /// Square (or round) covers, side by side.
    Shelf,
    /// Songs in columns of four (Home's Quick picks).
    Columns,
    /// Songs one under another.
    Songs,
    /// Mood and genre buttons, all of them (a page of them).
    Moods,
    /// Mood and genre buttons in rows, side by side (Explore's shelf of them).
    MoodRows,
    /// Words (an artist's About).
    About,
}

/// A shelf of a page: its title (and the small line over it), its cards or songs or words, and its "More".
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub strapline: String,
    pub style: Style,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more: Option<Browse>,
}

impl Section {
    fn checked(mut self) -> Self {
        self.items = items(self.items);
        self.more = self.more.filter(Browse::is_sound);
        self
    }
}

/// One of a list's choices (Home's moods, a search's kinds): its words, its params, whether it's chosen.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chip {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub selected: bool,
}

fn chips(list: Vec<Chip>) -> Vec<Chip> {
    list.into_iter()
        .map(|mut chip| {
            chip.params = chip.params.filter(|params| is_params(params));
            chip
        })
        .collect()
}

fn token(more: Option<String>) -> Option<String> {
    more.filter(|more| !more.is_empty() && is_token(more))
}

/// Home, Explore, a page of YouTube Music's, a song's Related: its choices, its shelves, the token for more, and
/// the picture YouTube Music shows behind its top (Home's, as its mood is).
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default)]
    pub chips: Vec<Chip>,
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more: Option<String>,
}

impl Page {
    fn checked(mut self) -> Self {
        self.background = self.background.filter(|url| is_youtube_image(url));
        self.chips = chips(self.chips);
        self.sections = self.sections.into_iter().map(Section::checked).collect();
        self.more = token(self.more);
        self
    }
}

/// An album's or playlist's page: what it is (its kind, title, its two lines, who made it, its cover and
/// words), playing and shuffling it, whether it's in the Library, its songs and the token for more.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub kind: Kind,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub second: String,
    #[serde(default)]
    pub by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_page: Option<Browse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play: Option<Watch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shuffle: Option<Watch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radio: Option<Watch>,
    /// Its playlist's id (an album's too), for saving it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playlist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<bool>,
    #[serde(default)]
    pub tracks: Vec<Item>,
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more: Option<String>,
}

impl Collection {
    fn checked(mut self) -> Self {
        self.by_page = self.by_page.filter(Browse::is_sound);
        self.image = self.image.filter(|url| is_youtube_image(url));
        self.play = self.play.filter(Watch::is_sound);
        self.shuffle = self.shuffle.filter(Watch::is_sound);
        self.radio = self.radio.filter(Watch::is_sound);
        self.playlist = self.playlist.filter(|id| is_id(id));
        self.tracks = items(self.tracks);
        self.sections = self.sections.into_iter().map(Section::checked).collect();
        self.more = token(self.more);
        self
    }
}

/// An artist's page: their name, words, wide picture and subscribers, shuffling their songs and their radio,
/// then their songs, albums, videos and more.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default)]
    pub subscribers: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shuffle: Option<Watch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radio: Option<Watch>,
    #[serde(default)]
    pub sections: Vec<Section>,
}

impl Artist {
    fn checked(mut self) -> Self {
        self.image = self.image.filter(|url| is_youtube_image(url));
        self.shuffle = self.shuffle.filter(Watch::is_sound);
        self.radio = self.radio.filter(Watch::is_sound);
        self.sections = self.sections.into_iter().map(Section::checked).collect();
        self
    }
}

/// A search's results: its best match (and that one's songs), its kinds to choose from, everything else, the
/// token for more.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Results {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<Item>,
    #[serde(default)]
    pub top_songs: Vec<Item>,
    #[serde(default)]
    pub chips: Vec<Chip>,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more: Option<String>,
}

impl Results {
    fn checked(mut self) -> Self {
        self.top = self.top.map(Item::checked);
        self.top_songs = items(self.top_songs);
        self.chips = chips(self.chips);
        self.items = items(self.items);
        self.more = token(self.more);
        self
    }
}

/// What's typed, completed: YouTube Music's words, and the songs, albums and artists it names straight away.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestions {
    #[serde(default)]
    pub texts: Vec<String>,
    #[serde(default)]
    pub items: Vec<Item>,
}

/// A song's words, and where they're from (YouTube Music names it: "Source: LyricFind"); none, empty.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub source: String,
}

/// YouTube Music's answer to the app's question `answer` (its id), under the question's name; `failed` when
/// YouTube Music didn't answer.
#[derive(Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Answer {
    #[serde(rename(serialize = "id"))]
    pub answer: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<Page>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explore: Option<Page>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browse: Option<Page>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<Results>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggest: Option<Suggestions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<Collection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<Artist>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<Page>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<Lyrics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related: Option<Page>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub failed: bool,
}

impl Answer {
    fn checked(mut self) -> Self {
        self.home = self.home.map(Page::checked);
        self.explore = self.explore.map(Page::checked);
        self.browse = self.browse.map(Page::checked);
        self.search = self.search.map(Results::checked);
        self.suggest = self.suggest.map(|mut found| {
            found.items = items(found.items);
            found
        });
        self.collection = self.collection.map(Collection::checked);
        self.artist = self.artist.map(Artist::checked);
        self.library = self.library.map(Page::checked);
        self.related = self.related.map(Page::checked);
        self
    }
}

/// The song playing: its id, title, artists (and their page), album (and its page), cover, length (seconds),
/// whether it's a song, a music video or an episode, its like, and whether it has a picture to show (a music
/// video's, or a song's own counterpart).
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub video: String,
    pub title: String,
    #[serde(default)]
    pub artists: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<Browse>,
    #[serde(default)]
    pub album: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album_page: Option<Browse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default)]
    pub duration: f64,
    pub kind: Kind,
    #[serde(default)]
    pub like: Rating,
    #[serde(default)]
    pub has_video: bool,
}

impl Track {
    fn checked(mut self) -> Option<Self> {
        if !is_video_id(&self.video) {
            return None;
        }
        self.artist = self.artist.filter(Browse::is_sound);
        self.album_page = self.album_page.filter(Browse::is_sound);
        self.image = self.image.filter(|url| is_youtube_image(url));
        Some(self)
    }
}

/// What YouTube Music plays: the song, playing or not (and waiting for its sound), how far (seconds) of how long,
/// the options, the volume, the picture shown or not, the queue around the song (`queue`: from its song at
/// `start`; `index`: this song's place in the whole queue; a radio's goes on and on), what it plays from
/// ("Çok Güzel Gülüyorsun Mix"), and whether an ad plays.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playback {
    pub track: Track,
    pub playing: bool,
    #[serde(default)]
    pub loading: bool,
    #[serde(default)]
    pub position: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub shuffle: bool,
    pub repeat: Repeat,
    /// The page's player's volume, 0 to 1.
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub video: bool,
    #[serde(default)]
    pub queue: Vec<Item>,
    #[serde(default)]
    pub start: u32,
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub ad: bool,
}

impl Playback {
    fn checked(mut self) -> Option<Self> {
        self.track = self.track.checked()?;
        self.queue = items(self.queue);
        Some(self)
    }
}

/// Whether the page is signed in (and the account's picture, on Google's image hosts only), and playing.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub signed_in: bool,
    pub playing: bool,
    #[serde(default)]
    pub avatar: Option<String>,
}

impl State {
    /// Only a picture on Google's image hosts passes, and only while signed in.
    fn checked(mut self) -> Self {
        if !self.signed_in || !self.avatar.as_deref().is_some_and(is_youtube_image) {
            self.avatar = None;
        }
        self
    }
}

#[derive(Debug, PartialEq)]
pub enum Report {
    /// The music's spectrum, while the cab's Equalizer asks for it ([`speakers::read_spectrum`]).
    Spectrum(Vec<u8>),
    State(State),
    /// The sign-in window opened (`true`) or closed.
    SignInWindow(bool),
    /// Signed in in its window, and YouTube Music's page here is yet to say it's signed in (`true`), or it has
    /// (or took too long: `false`).
    Confirming(bool),
    Answer(Box<Answer>),
    /// What plays (`None`: nothing).
    Playback(Option<Box<Playback>>),
    /// How long a play, next or previous took.
    Timing(Timing),
}

/// How long a play (or a next, a previous) took, in milliseconds: until the page had it, until the song
/// sounded (`None`: not in 20 s). Times only, for the log.
#[derive(Clone, Debug, PartialEq)]
pub struct Timing {
    pub action: &'static str,
    pub queued: u64,
    pub sounding: Option<u64>,
    /// Its steps and their times (`player> 120, player 480, playing 900`): kinds of steps only.
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
    /// `[youtube music] next: ready in 120 ms, sound in 630 ms`
    pub fn line(&self) -> String {
        let sound = self.sounding.map_or_else(|| "no sound in 20 s".to_string(), |ms| format!("sound in {ms} ms"));
        // The steps only when it was slow: where the time went.
        let slow = self.sounding.is_none_or(|ms| ms > SLOW_MS);
        let steps = if slow && !self.steps.is_empty() { format!(" (steps: {})", self.steps) } else { String::new() };
        format!("[youtube music] {}: ready in {} ms, {sound}{steps}", self.action, self.queued)
    }
}

/// What the host tells YouTube Music's page of the cab (never the cabin page's to send): the truck's speakers
/// (`None`: as it is, a truck whose speakers aren't known), the driver's head, and the camera outside (its
/// distance, metres; 0: not known). As Apple Music's (apple_music.rs).
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

pub struct YouTubeMusic {
    view: OffscreenWebView,
    /// The page's size, the picture kept within it.
    frame: PageSize,
    sign_in: RefCell<Option<SignInWindow>>,
    /// Signed in in its window, until the page here says it's signed in.
    confirming: Confirming,
    pending: RefCell<Vec<Report>>,
    /// The app's questions not answered yet: asked again as the page is ready anew (a page that loads again,
    /// signed in or out, misses what was sent while it loaded).
    unanswered: RefCell<Unanswered>,
    /// The cab's language, sent again to a page that loaded anew.
    language: RefCell<Option<String>>,
    /// The cab as the page was told it ([`YouTubeMusic::cab`]).
    told: RefCell<Told>,
}

impl YouTubeMusic {
    /// Opens YouTube Music's web player in a WebView of `stage`'s frame (`width` x `height`), hidden under the
    /// cabin page, on `user_data`'s profile (the cabin page's, which YouTube's shares).
    pub fn start(stage: &Stage, width: u32, height: u32, user_data: &Path) -> Result<Self, webview::Error> {
        let view = OffscreenWebView::new(stage, width, height, user_data)?;
        view.log_as("youtube music")?;
        view.show(false)?;
        stage.add(&view, false)?;
        // The truck speakers' sound first: the page's script makes its own of it.
        view.add_startup_script(concat!(
            include_str!("cab_sound.js"),
            "\n",
            include_str!("youtube_music.js"),
            "\n",
            include_str!("consent.js")
        ))?;
        crate::youtube::answer_consent(&view);
        view.navigate(HOME)?;
        Ok(Self {
            view,
            frame: stage.page_size(),
            sign_in: RefCell::new(None),
            confirming: Confirming::default(),
            pending: RefCell::new(Vec::new()),
            unanswered: RefCell::default(),
            language: RefCell::new(None),
            told: RefCell::default(),
        })
    }

    /// The truck's speakers (none: its sound plays as it is), the driver's head, and whether the camera is outside
    /// the cab and how far (metres; 0: not known): the page is told the speakers as they change, the head as it moves,
    /// the camera as it goes in or out or a quarter metre further, the tone as it's set and the spectrum as it's wanted (as Apple Music's).
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

    pub fn command(&self, command: &Command) -> windows::core::Result<()> {
        match command {
            Command::SignIn => {
                self.open_sign_in();
                Ok(())
            }
            // The whole page scaled down into the picture's rectangle, shown (the page itself shows only its video;
            // scaled, not resized: YouTube Music keeps its own layout, its player bar's buttons and all).
            Command::Show { x, y, width, height } => {
                let page = self.frame.get();
                let (x, y, width, height) = within(page, *x, *y, *width, *height);
                self.view.fit(page, x, y, width, height)?;
                self.view.show(true)
            }
            Command::Hide => self.view.show(false),
            // The profile's cookies (YouTube's too), then the page again, signed out.
            Command::SignOut => {
                self.view.delete_cookies()?;
                self.reload()
            }
            Command::Ask { id, question } => {
                if !question.is_sound() {
                    // Not a question for YouTube Music: failed at once, so the app isn't left waiting.
                    let answer = Answer { answer: *id, failed: true, ..Default::default() };
                    self.pending.borrow_mut().push(Report::Answer(Box::new(answer)));
                    return Ok(());
                }
                let sent = serde_json::to_string(command).unwrap_or_default();
                self.unanswered.borrow_mut().asked(*id, sent.clone());
                self.view.post_json(&sent)
            }
            Command::Language { .. } if command.for_page() => {
                let sent = serde_json::to_string(command).unwrap_or_default();
                *self.language.borrow_mut() = Some(sent.clone());
                self.view.post_json(&sent)
            }
            _ if command.for_page() => self.view.post_json(&serde_json::to_string(command).unwrap_or_default()),
            _ => Ok(()),
        }
    }

    /// The page again (signed in or out elsewhere: YouTube's page shares the sign-in).
    pub fn reload(&self) -> windows::core::Result<()> {
        self.view.navigate(HOME)
    }

    /// Google's sign-in window, opened (or brought forward if it is).
    fn open_sign_in(&self) {
        let mut sign_in = self.sign_in.borrow_mut();
        if let Some(window) = sign_in.as_ref() {
            window.bring_forward();
            return;
        }
        match SignInWindow::open(self.view.environment(), &signin::YOUTUBE_MUSIC) {
            Ok(window) => {
                *sign_in = Some(window);
                self.pending.borrow_mut().push(Report::SignInWindow(true));
            }
            Err(e) => crate::startup::mark(&format!("host: the YouTube Music sign-in window couldn't open: {e}")),
        }
    }

    /// What YouTube Music's page said since the last call, and the sign-in window opening or closing (signed in,
    /// the web player here loads again, signed in).
    pub fn take_reports(&self) -> Vec<Report> {
        let mut reports = std::mem::take(&mut *self.pending.borrow_mut());
        let progress = self.sign_in.borrow().as_ref().map(SignInWindow::progress);
        if let Some(progress @ (Progress::SignedIn | Progress::Closed)) = progress {
            self.sign_in.borrow_mut().take();
            reports.push(Report::SignInWindow(false));
            if progress == Progress::SignedIn {
                let _ = self.reload();
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
                Some(Message::Timing(timing)) => reports.push(Report::Timing(timing)),
                Some(Message::Ready) => {
                    // A page loaded anew: the language, the cab as it is, and the questions it missed.
                    if let Some(language) = self.language.borrow().as_deref() {
                        let _ = self.view.post_json(language);
                    }
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

/// A message of YouTube Music's page (`youtube_music.js`).
#[derive(Debug, PartialEq)]
enum Message {
    /// The page is ready (loaded anew).
    Ready,
    Timing(Timing),
    State(State),
    Answer(Box<Answer>),
    Playback(Option<Box<Playback>>),
}

fn read_message(message: &str) -> Option<Message> {
    let value: serde_json::Value = serde_json::from_str(message).ok()?;
    if value.get("ready").and_then(serde_json::Value::as_bool) == Some(true) {
        return Some(Message::Ready);
    }
    if let Some(timing) = value.get("timing") {
        let action = match timing.get("action").and_then(serde_json::Value::as_str) {
            Some("next") => "next",
            Some("previous") => "previous",
            Some("jump") => "jump",
            Some("shuffle") => "shuffle",
            _ => "play",
        };
        return Some(Message::Timing(Timing {
            action,
            queued: timing.get("queued")?.as_u64()?,
            sounding: timing.get("sounding").and_then(serde_json::Value::as_u64),
            steps: steps_of(timing.get("steps").and_then(serde_json::Value::as_str).unwrap_or("")),
        }));
    }
    if value.get("answer").is_some() {
        return match serde_json::from_value::<Answer>(value.clone()) {
            Ok(answer) => Some(Message::Answer(Box::new(answer.checked()))),
            // An answer that doesn't read (YouTube Music changed what it says): failed, so the app isn't left
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
        return serde_json::from_value::<Playback>(playback.clone())
            .ok()
            .and_then(Playback::checked)
            .map(|p| Message::Playback(Some(Box::new(p))));
    }
    serde_json::from_value::<State>(value).ok().map(|state| Message::State(state.checked()))
}

/// `{"type":"youtubeMusic","signedIn":true,"playing":false,"signingIn":false,"avatar":…}` for the cabin page
/// (`null` fields when YouTube Music isn't running).
pub fn state_message(state: Option<&State>, signing_in: bool) -> String {
    serde_json::json!({
        "type": "youtubeMusic",
        "signedIn": state.map(|s| s.signed_in),
        "playing": state.map(|s| s.playing),
        "signingIn": signing_in,
        "avatar": state.and_then(|s| s.avatar.as_deref()),
    })
    .to_string()
}

/// `{"type":"youtubeMusicAnswer","id":…,"home":{…}}`: YouTube Music's answer to the app's question.
pub fn answer_message(answer: &Answer) -> String {
    let mut message = serde_json::to_value(answer).unwrap_or_default();
    message["type"] = "youtubeMusicAnswer".into();
    message.to_string()
}

/// `{"type":"youtubeMusicPlayback","playback":{…}}` (`null`: nothing plays).
pub fn playback_message(playback: Option<&Playback>) -> String {
    serde_json::json!({ "type": "youtubeMusicPlayback", "playback": playback }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_cabin_pages_commands() {
        let ask: Command =
            serde_json::from_str(r#"{"action":"ask","id":3,"question":{"ask":"library","shelf":"albums"}}"#).unwrap();
        assert_eq!(ask, Command::Ask { id: 3, question: Question::Library { shelf: Shelf::Albums } });
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"action":"ask","id":4,"question":{"ask":"explore"}}"#).unwrap(),
            Command::Ask { id: 4, question: Question::Explore }
        );
        let play: Command = serde_json::from_str(
            r#"{"action":"play","watch":{"video":"aQ6TGi2m88A","playlist":"RDAMVMaQ6TGi2m88A","params":"wAEB"}}"#,
        )
        .unwrap();
        assert_eq!(
            play,
            Command::Play {
                watch: Some(Watch {
                    video: Some("aQ6TGi2m88A".into()),
                    playlist: Some("RDAMVMaQ6TGi2m88A".into()),
                    params: Some("wAEB".into()),
                    index: None,
                })
            }
        );
        assert_eq!(serde_json::from_str::<Command>(r#"{"action":"play"}"#).unwrap(), Command::Play { watch: None });
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"action":"like","video":"aQ6TGi2m88A","like":"dislike"}"#).unwrap(),
            Command::Like { video: "aQ6TGi2m88A".into(), like: Rating::Dislike }
        );
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"action":"repeat","mode":"one"}"#).unwrap(),
            Command::Repeat { mode: Repeat::One }
        );
        let queue: Command = serde_json::from_str(r#"{"action":"queue","video":"aQ6TGi2m88A","next":true}"#).unwrap();
        assert_eq!(queue, Command::Queue { video: "aQ6TGi2m88A".into(), next: true });
        assert!(queue.for_page());
        assert!(!Command::Queue { video: "not a video".into(), next: false }.for_page());
        assert!(serde_json::from_str::<Command>(r#"{"action":"explode"}"#).is_err());
    }

    /// Only ids, params and tokens of YouTube's shapes reach the page's requests.
    #[test]
    fn passes_the_page_only_what_is_sound() {
        let watch = |video: &str, playlist: &str, params: &str| Command::Play {
            watch: Some(Watch {
                video: Some(video.into()),
                playlist: Some(playlist.into()),
                params: Some(params.into()),
                index: Some(3),
            }),
        };
        assert!(watch("aQ6TGi2m88A", "OLAK5uy_lztoHXVL7K8hxR11HHrPoTToI82sTq8bs", "wAEB8gECKAE%3D").for_page());
        assert!(!watch("aQ6TGi2m88A/../x", "OLAK5uy", "wAEB").for_page());
        assert!(!watch("aQ6TGi2m88A", "PL\"x", "wAEB").for_page());
        assert!(!watch("aQ6TGi2m88A", "PL1", "a b").for_page());
        assert!(!Command::Play { watch: Some(Watch::default()) }.for_page());
        assert!(Command::Play { watch: None }.for_page());
        assert!(Command::Language { code: "tr".into() }.for_page());
        assert!(!Command::Language { code: "tr&x=1".into() }.for_page());
        assert!(!Command::SignIn.for_page() && !Command::SignOut.for_page() && !Command::Start.for_page());
        // Placing the picture is the host's own.
        assert!(!Command::Show { x: 0.0, y: 0.0, width: 10, height: 10 }.for_page() && !Command::Hide.for_page());

        let browse = |id: &str, params: Option<&str>| {
            Question::Browse { page: Browse { id: id.into(), params: params.map(Into::into) } }.is_sound()
        };
        assert!(browse("FEmusic_moods_and_genres_category", Some("ggMPOg1uX1JOQWZFeDByc2Jm")));
        assert!(browse("VLPL8BVGIGDrLq5BhO7l_vKl581Q-qoKEGbd", None));
        assert!(!browse("../me", None));
        assert!(!browse("FEmusic_home", Some("x y")));
        assert!(Question::Search { query: "tarkan".into(), filter: None, more: None }.is_sound());
        assert!(!Question::Search { query: String::new(), filter: None, more: None }.is_sound());
        assert!(!Question::Search { query: "x".repeat(201), filter: None, more: None }.is_sound());
        assert!(!Question::Lyrics { video: "short".into() }.is_sound());
        assert!(!Question::Collection { id: "MPREb_jaQYKnBAjG4".into(), more: Some("<script>".into()) }.is_sound());
    }

    /// Commands go to the page as the app wrote them.
    #[test]
    fn writes_the_page_its_commands_as_the_app_sent_them() {
        let ask =
            Command::Ask { id: 9, question: Question::Search { query: "tarkan".into(), filter: None, more: None } };
        assert_eq!(
            serde_json::to_string(&ask).unwrap(),
            r#"{"action":"ask","id":9,"question":{"ask":"search","query":"tarkan"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Command::Ask {
                id: 2,
                question: Question::Browse { page: Browse { id: "FEmusic_charts".into(), params: None } }
            })
            .unwrap(),
            r#"{"action":"ask","id":2,"question":{"ask":"browse","page":{"id":"FEmusic_charts"}}}"#
        );
    }

    /// Only YouTube's own image hosts, colours of one shape and sound ids pass from the page's answers; anything
    /// else it says is dropped.
    #[test]
    fn reads_only_whats_shown_of_the_pages_answers() {
        let message = r##"{"answer":5,"home":{"background":"https://lh3.googleusercontent.com/bg=w1280","chips":[{"label":"Relax","params":"ggMPOg1u"},{"label":"x","params":"a b"}],
            "sections":[{"title":"Quick picks","style":"columns","items":[
              {"kind":"song","title":"Kayıp Kalp","subtitle":"BLOK3","image":"https://lh3.googleusercontent.com/a=w120",
               "play":{"video":"A7ShtPRP2Mc","playlist":"RDAMVMA7ShtPRP2Mc","params":"wAEB"},"video":"A7ShtPRP2Mc",
               "artist":{"id":"UCZpmeLoLLb3vmxgscRyLPgw"},"duration":"2:34","extra":"<img>"},
              {"kind":"mood","title":"Chill","image":"https://evil.example/x.png","color":"#ff00aa",
               "browse":{"id":"FEmusic_moods_and_genres_category","params":"ggMPOg1u"}},
              {"kind":"album","title":"X","color":"red","browse":{"id":"../x"}}]}],"more":"4qmFsgK"}}"##;
        let Some(Message::Answer(answer)) = read_message(message) else { panic!("an answer") };
        let home = answer.home.as_ref().unwrap();
        assert_eq!(home.chips[0].params.as_deref(), Some("ggMPOg1u"));
        assert_eq!(home.chips[1].params, None);
        let items = &home.sections[0].items;
        assert_eq!(items[0].image.as_deref(), Some("https://lh3.googleusercontent.com/a=w120"));
        assert_eq!(items[0].play.as_ref().and_then(|w| w.video.as_deref()), Some("A7ShtPRP2Mc"));
        assert_eq!(items[1].image, None);
        assert_eq!(items[1].color.as_deref(), Some("#ff00aa"));
        assert_eq!(items[2].color, None);
        assert_eq!(items[2].browse, None);
        assert_eq!(home.more.as_deref(), Some("4qmFsgK"));
        assert_eq!(home.background.as_deref(), Some("https://lh3.googleusercontent.com/bg=w1280"));
        let Some(Message::Answer(elsewhere)) =
            read_message(r#"{"answer":7,"home":{"background":"https://evil.example/bg.png","sections":[]}}"#)
        else {
            panic!("an answer")
        };
        assert_eq!(elsewhere.home.unwrap().background, None);
        // Nothing but the fields above goes on to the cabin page.
        assert!(!answer_message(&answer).contains("extra"));
        // An answer that doesn't read is failed.
        let Some(Message::Answer(failed)) = read_message(r#"{"answer":6,"home":{"sections":"no"}}"#) else {
            panic!("an answer")
        };
        assert!(failed.failed && failed.answer == 6);
    }

    #[test]
    fn reads_what_plays() {
        let message = r#"{"playback":{"track":{"video":"aQ6TGi2m88A","title":"Çok Güzel Gülüyorsun","artists":"BLOK3 & Poizi",
            "image":"https://i.ytimg.com/vi/x/hq.jpg","duration":241,"kind":"song","like":"like","hasVideo":false},
            "playing":true,"loading":false,"position":12.5,"duration":241,"shuffle":false,"repeat":"off","volume":0.5,
            "video":false,"queue":[{"kind":"song","title":"Next","video":"A7ShtPRP2Mc"}],"index":0,"from":"Mix","ad":false}}"#;
        let Some(Message::Playback(Some(playback))) = read_message(message) else { panic!("a playback") };
        assert_eq!(playback.track.like, Rating::Like);
        assert_eq!(playback.queue.len(), 1);
        assert_eq!(read_message(r#"{"playback":null}"#), Some(Message::Playback(None)));
        // A song whose id isn't one: nothing read.
        assert_eq!(
            read_message(
                r#"{"playback":{"track":{"video":"x","title":"t","kind":"song"},"playing":true,"repeat":"off"}}"#
            ),
            None
        );
    }

    #[test]
    fn reads_the_account_as_only_signed_in_and_its_picture() {
        let read = |message: &str| match read_message(message) {
            Some(Message::State(state)) => state,
            other => panic!("a state, not {other:?}"),
        };
        let state = read(r#"{"signedIn":true,"playing":false,"avatar":"https://yt3.ggpht.com/me=s88","name":"x"}"#);
        assert_eq!(state.avatar.as_deref(), Some("https://yt3.ggpht.com/me=s88"));
        assert_eq!(read(r#"{"signedIn":true,"playing":false,"avatar":"https://evil.example/me"}"#).avatar, None);
        assert_eq!(read(r#"{"signedIn":false,"playing":false,"avatar":"https://yt3.ggpht.com/me"}"#).avatar, None);
        assert_eq!(
            state_message(Some(&state), false),
            r#"{"avatar":"https://yt3.ggpht.com/me=s88","playing":false,"signedIn":true,"signingIn":false,"type":"youtubeMusic"}"#
        );
    }

    #[test]
    fn logs_only_times_and_kinds_of_steps() {
        let Some(Message::Timing(timing)) = read_message(
            r#"{"timing":{"action":"next","queued":120,"sounding":4200,"steps":"player> 10, player 400, <b>x</b>"}}"#,
        ) else {
            panic!("a timing")
        };
        assert_eq!(
            timing.line(),
            "[youtube music] next: ready in 120 ms, sound in 4200 ms (steps: player> 10, player 400, b>xb>)"
        );
        let quick = Timing { action: "play", queued: 50, sounding: Some(600), steps: "x 1".into() };
        assert_eq!(quick.line(), "[youtube music] play: ready in 50 ms, sound in 600 ms");
    }

    /// The page is told the cab as Apple Music's is.
    #[test]
    fn tells_the_page_the_cab() {
        assert_eq!(
            serde_json::to_string(&Cab::Speakers { speakers: None }).unwrap(),
            r#"{"action":"speakers","speakers":null}"#
        );
        assert_eq!(
            serde_json::to_string(&Cab::Outside { on: true, distance: 6.25 }).unwrap(),
            r#"{"action":"outside","on":true,"distance":6.25}"#
        );
        // Not the cabin page's to send.
        assert!(serde_json::from_str::<Command>(r#"{"action":"speakers","speakers":null}"#).is_err());
    }

    #[test]
    fn holds_questions_until_answered_the_latest_only() {
        let mut unanswered = Unanswered::default();
        for id in 0..40 {
            unanswered.asked(id, format!("q{id}"));
        }
        unanswered.answered(39);
        let all = unanswered.all();
        assert_eq!(all.len(), 31);
        assert_eq!(all[0], "q8");
    }

    /// What an answer holds, in a line: its sections' styles and sizes, its first titles (YouTube Music's
    /// catalogue, never an account's).
    fn summary(answer: &Answer) -> String {
        let shelves = |page: &Page| {
            page.sections
                .iter()
                .map(|s| {
                    let first = s.items.first().map_or(String::new(), |i| format!(" {:?}:{}", i.kind, i.title));
                    format!("[{:?} '{}' x{}{}]", s.style, s.title, s.items.len(), first)
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        if answer.failed {
            return "FAILED".into();
        }
        if let Some(page) = answer.home.as_ref().or(answer.explore.as_ref()).or(answer.browse.as_ref()) {
            return format!(
                "chips {} more {} background {} {}",
                page.chips.len(),
                page.more.is_some(),
                page.background.is_some(),
                shelves(page)
            );
        }
        if let Some(page) = answer.related.as_ref().or(answer.library.as_ref()) {
            return shelves(page);
        }
        if let Some(found) = &answer.search {
            let top = found.top.as_ref().map_or("-".into(), |t| format!("{:?}:{}", t.kind, t.title));
            let kinds: Vec<String> = found.items.iter().take(8).map(|i| format!("{:?}", i.kind)).collect();
            return format!(
                "top {top} (+{} songs), chips {}, {} items {kinds:?}, more {}",
                found.top_songs.len(),
                found.chips.len(),
                found.items.len(),
                found.more.is_some()
            );
        }
        if let Some(found) = &answer.suggest {
            return format!("{} texts, {} items", found.texts.len(), found.items.len());
        }
        if let Some(c) = &answer.collection {
            return format!(
                "{:?} '{}' | {} | {} | by '{}' | play {} shuffle {} radio {} | {} tracks (first {:?}) more {} | {}",
                c.kind,
                c.title,
                c.subtitle,
                c.second,
                c.by,
                c.play.is_some(),
                c.shuffle.is_some(),
                c.radio.is_some(),
                c.tracks.len(),
                c.tracks.first().map(|t| (&t.title, &t.duration, &t.rank, t.play.is_some())),
                c.more.is_some(),
                c.sections.len()
            );
        }
        if let Some(a) = &answer.artist {
            return format!(
                "'{}' subs '{}' image {} shuffle {} radio {} | {}",
                a.name,
                a.subscribers,
                a.image.is_some(),
                a.shuffle.is_some(),
                a.radio.is_some(),
                a.sections
                    .iter()
                    .map(|s| format!("[{:?} '{}' x{}]", s.style, s.title, s.items.len()))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        if let Some(l) = &answer.lyrics {
            return format!("{} chars, source '{}'", l.text.chars().count(), l.source);
        }
        "nothing".into()
    }

    /// The real thing: a song YouTube shows its warning before ("this video may contain disturbing images"), as a
    /// player's (2026-10-27: loading, then paused at 0 s, again and again) plays, its sound running on. Muted. Needs
    /// the network and WebView2: `cargo test -p westplay-host -- --ignored plays_a_song_youtube_warns_about`, or
    /// another song by WESTPLAY_YOUTUBE_WARNED.
    #[test]
    #[ignore = "plays a song from music.youtube.com"]
    fn plays_a_song_youtube_warns_about() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let video = std::env::var("WESTPLAY_YOUTUBE_WARNED").unwrap_or_else(|_| "kzOXY8Kj_xc".to_owned());
        let user_data = std::env::temp_dir().join(format!("westplay-youtube-music-warned-{}", std::process::id()));
        let stage = Stage::new(1280, 720).unwrap();
        let music = YouTubeMusic::start(&stage, 1280, 720, &user_data).unwrap();
        music.view.set_muted(true).unwrap();
        // What the page last said of its playing: playing, loading, where, which song.
        type Said = (bool, bool, f64, String);
        let mut last: Option<Said> = None;
        let mut ready = false;
        // Whether `until` came within `seconds`.
        let mut pump = |seconds: f64, until: &dyn Fn(&Said, bool) -> bool| -> bool {
            let from = std::time::Instant::now();
            while from.elapsed().as_secs_f64() < seconds {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in music.take_reports() {
                    match report {
                        Report::Playback(Some(p)) => {
                            let now = (p.playing, p.loading, p.position, p.track.video.clone());
                            if last.as_ref() != Some(&now) {
                                println!("{:>6.1} s: {now:?}", from.elapsed().as_secs_f32());
                            }
                            last = Some(now);
                        }
                        // The page ready: it says what it is (signed in, playing) once it is.
                        Report::State(_) => ready = true,
                        _ => {}
                    }
                }
                if until(&last.clone().unwrap_or_default(), ready) {
                    return true;
                }
            }
            false
        };
        // Loaded (past YouTube's cookie page, where it shows: a page more to load), ready to be asked.
        let started = std::time::Instant::now();
        assert!(pump(90.0, &|_, ready| ready), "the page not ready in 90 s");
        println!("ready after {:.1} s", started.elapsed().as_secs_f32());
        let watch = Watch { video: Some(video.clone()), ..Default::default() };
        let asked = std::time::Instant::now();
        music.command(&Command::Play { watch: Some(watch) }).unwrap();
        // Playing it, three seconds in.
        pump(40.0, &|(playing, _, position, track), _| *playing && *position > 3.0 && *track == video);
        println!("after {:.1} s: {last:?}", asked.elapsed().as_secs_f32());
        let page = music
            .view
            .evaluate(
                r#"(() => {
                  const v = document.querySelector('video')
                  const p = document.getElementById('movie_player')
                  const r = p?.getPlayerResponse?.()?.playabilityStatus
                  return { url: location.href, title: document.title, time: v?.currentTime, paused: v?.paused,
                    ready: v?.readyState, network: v?.networkState,
                    error: v?.error?.code ?? null, state: p?.getPlayerState?.(), status: r?.status, reason: r?.reason }
                })()"#,
            )
            .unwrap();
        // Where the page is, and YouTube's word on the song (a warning, a sign-in asked for, or none).
        println!("the page: {page}");
        drop(music);
        let _ = std::fs::remove_dir_all(&user_data);
        assert!(
            last.as_ref().is_some_and(|(playing, _, position, track)| *playing && *position > 3.0 && *track == video),
            "not playing 3 s in within 40 s: {last:?}"
        );
    }

    /// A music video's picture as Now Playing shows it (the page scaled into a small rectangle of the screen):
    /// where the page lays its video out, measured, and the page photographed (`WESTPLAY_CAPTURE`, a folder).
    #[test]
    #[ignore = "plays a music video from music.youtube.com"]
    fn a_music_videos_picture_fills_its_rectangle() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data = std::env::temp_dir().join(format!("westplay-youtube-music-picture-{}", std::process::id()));
        let stage = Stage::new(1280, 720).unwrap();
        let music = YouTubeMusic::start(&stage, 1280, 720, &user_data).unwrap();
        music.view.set_muted(true).unwrap();
        let mut playing_video = false;
        let mut pump = |seconds: f64| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in music.take_reports() {
                    if let Report::Playback(Some(p)) = report {
                        playing_video |= p.playing && p.track.has_video;
                    }
                }
            }
        };
        pump(8.0);
        let watch = Watch { video: Some("dQw4w9WgXcQ".into()), ..Default::default() };
        music.command(&Command::Play { watch: Some(watch) }).unwrap();
        pump(8.0);
        music.command(&Command::Video { on: true }).unwrap();
        pump(6.0);
        // As Now Playing asks: a rectangle of the screen, the page scaled into it.
        music.command(&Command::Show { x: 300.0, y: 160.0, width: 460, height: 259 }).unwrap();
        pump(3.0);
        let measured = music
            .view
            .evaluate(
                r#"(() => {
                  const box = (e) => e && (({ x, y, width, height }) => ({ x, y, width, height }))(e.getBoundingClientRect())
                  const video = document.querySelector('video.html5-main-video') ?? document.querySelector('video')
                  const moved = []
                  for (let e = document.querySelector('#movie_player'); e; e = e.parentElement) {
                    const s = getComputedStyle(e)
                    if (s.transform !== 'none' || s.translate !== 'none' || s.scale !== 'none' || s.zoom !== '1')
                      moved.push(`${e.tagName.toLowerCase()}${e.id ? '#' + e.id : ''} ${s.transform} ${s.translate} ${s.scale} ${s.zoom}`)
                  }
                  return {
                    page: { width: innerWidth, height: innerHeight },
                    player: box(document.querySelector('#movie_player')),
                    container: box(document.querySelector('#movie_player .html5-video-container')),
                    video: box(video),
                    videoSize: video && { width: video.videoWidth, height: video.videoHeight },
                    videoStyle: video && video.getAttribute('style'),
                    movedAncestors: moved,
                  }
                })()"#,
            )
            .unwrap();
        println!("playing a video: {playing_video}\n{measured}");
        if let Some(folder) = std::env::var_os("WESTPLAY_CAPTURE") {
            let path = std::path::PathBuf::from(folder).join("youtube-music-video.png");
            music.view.capture_png(&path).unwrap();
            println!("photographed: {}", path.display());
        }
        drop(music);
        let _ = std::fs::remove_dir_all(&user_data);
    }

    /// YouTube Music in the hidden WebView, signed out (a fresh profile, deleted after): every question answered,
    /// then a play, next, previous, pause, play on, seek, repeat, shuffle and a jump, each said once where it ends
    /// up (never paused on the way to the next song).
    #[test]
    #[ignore = "plays from music.youtube.com"]
    fn answers_and_plays_in_the_hidden_web_player() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data = std::env::temp_dir().join(format!("westplay-youtube-music-{}", std::process::id()));
        let stage = Stage::new(1280, 720).unwrap();
        let music = YouTubeMusic::start(&stage, 1280, 720, &user_data).unwrap();
        // Never heard: whatever the page does with its own volume, the WebView makes no sound.
        music.view.set_muted(true).unwrap();
        let started = std::time::Instant::now();
        let mut answers: BTreeMap<u32, Box<Answer>> = BTreeMap::new();
        let mut said: Vec<(f64, Playback)> = Vec::new();
        let mut timings: Vec<Timing> = Vec::new();
        let mut ready = false;
        let pump = |seconds: f64,
                    answers: &mut BTreeMap<u32, Box<Answer>>,
                    said: &mut Vec<(f64, Playback)>,
                    timings: &mut Vec<Timing>,
                    ready: &mut bool| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in music.take_reports() {
                    let at = started.elapsed().as_secs_f64();
                    match report {
                        Report::State(state) => {
                            *ready = true;
                            println!("  {at:6.2} s: state signed in {} playing {}", state.signed_in, state.playing);
                        }
                        Report::Answer(answer) => {
                            answers.insert(answer.answer, answer);
                        }
                        Report::Timing(timing) => {
                            println!("  {at:6.2} s: {} [steps: {}]", timing.line(), timing.steps);
                            timings.push(timing);
                        }
                        Report::Playback(Some(p)) => {
                            println!(
                                "  {at:6.2} s: '{}' ({:?}, video {}) playing={} loading={} at {:.2}/{:.0} s shuffle={} repeat={:?} queue {}+{} index {} from '{}' ad={} like {:?} volume {:?}",
                                p.track.title,
                                p.track.kind,
                                p.track.has_video,
                                p.playing,
                                p.loading,
                                p.position,
                                p.duration,
                                p.shuffle,
                                p.repeat,
                                p.start,
                                p.queue.len(),
                                p.index,
                                p.from,
                                p.ad,
                                p.track.like,
                                p.volume
                            );
                            said.push((at, *p));
                        }
                        Report::Playback(None) => println!("  {at:6.2} s: nothing plays"),
                        _ => {}
                    }
                }
            }
        };
        for _ in 0..40 {
            pump(1.0, &mut answers, &mut said, &mut timings, &mut ready);
            if ready {
                break;
            }
        }
        assert!(ready, "the web player said nothing");
        let mut next_id = 1;
        let mut ask = |question: Question,
                       answers: &mut BTreeMap<u32, Box<Answer>>,
                       said: &mut Vec<(f64, Playback)>,
                       timings: &mut Vec<Timing>,
                       ready: &mut bool| {
            let id = next_id;
            next_id += 1;
            let shown = format!("{question:?}");
            music.command(&Command::Ask { id, question }).unwrap();
            for _ in 0..100 {
                pump(0.1, answers, said, timings, ready);
                if answers.contains_key(&id) {
                    break;
                }
            }
            let answer = answers.remove(&id).unwrap_or_else(|| panic!("no answer to {shown}"));
            println!("{shown}\n    => {}", summary(&answer));
            answer
        };
        let home = ask(Question::Home { chip: None, more: None }, &mut answers, &mut said, &mut timings, &mut ready);
        let page = home.home.clone().expect("Home");
        if let Some(chip) = page.chips.first().and_then(|c| c.params.clone()) {
            ask(Question::Home { chip: Some(chip), more: None }, &mut answers, &mut said, &mut timings, &mut ready);
        }
        if let Some(more) = page.more.clone() {
            ask(Question::Home { chip: None, more: Some(more) }, &mut answers, &mut said, &mut timings, &mut ready);
        }
        let explore =
            ask(Question::Explore, &mut answers, &mut said, &mut timings, &mut ready).explore.expect("Explore");
        let moods = explore.sections.iter().flat_map(|s| &s.items).filter_map(|i| i.browse.clone());
        for page in moods.take(2) {
            ask(Question::Browse { page }, &mut answers, &mut said, &mut timings, &mut ready);
        }
        let search = ask(
            Question::Search { query: "tarkan".into(), filter: None, more: None },
            &mut answers,
            &mut said,
            &mut timings,
            &mut ready,
        )
        .search
        .expect("a search");
        if let Some(filter) = search.chips.iter().find_map(|c| c.params.clone()) {
            let songs = ask(
                Question::Search { query: "tarkan".into(), filter: Some(filter.clone()), more: None },
                &mut answers,
                &mut said,
                &mut timings,
                &mut ready,
            );
            if let Some(more) = songs.search.and_then(|s| s.more) {
                ask(
                    Question::Search { query: "tarkan".into(), filter: Some(filter), more: Some(more) },
                    &mut answers,
                    &mut said,
                    &mut timings,
                    &mut ready,
                );
            }
        }
        ask(Question::Suggest { query: "tark".into() }, &mut answers, &mut said, &mut timings, &mut ready);
        let album = search.items.iter().find(|i| i.kind == Kind::Album).and_then(|i| i.browse.clone());
        let album = album.map(|b| {
            ask(Question::Collection { id: b.id, more: None }, &mut answers, &mut said, &mut timings, &mut ready)
        });
        let playlist = search.items.iter().find(|i| i.kind == Kind::Playlist).and_then(|i| i.browse.clone());
        let mut long = None;
        if let Some(b) = playlist {
            let found = ask(
                Question::Collection { id: b.id.clone(), more: None },
                &mut answers,
                &mut said,
                &mut timings,
                &mut ready,
            );
            long = found.collection.as_ref().and_then(|c| c.tracks.get(2).and_then(|t| t.play.clone()));
            if let Some(more) = found.collection.and_then(|c| c.more) {
                ask(
                    Question::Collection { id: b.id, more: Some(more) },
                    &mut answers,
                    &mut said,
                    &mut timings,
                    &mut ready,
                );
            }
        }
        let artist = search.top.as_ref().filter(|t| t.kind == Kind::Artist).and_then(|t| t.browse.clone());
        if let Some(b) = artist {
            ask(Question::Artist { id: b.id }, &mut answers, &mut said, &mut timings, &mut ready);
        }
        let song = search.items.iter().find(|i| i.kind == Kind::Song).cloned().expect("a song found");
        let video = song.video.clone().expect("its id");
        ask(Question::Lyrics { video: video.clone() }, &mut answers, &mut said, &mut timings, &mut ready);
        ask(Question::Related { video: video.clone() }, &mut answers, &mut said, &mut timings, &mut ready);
        ask(Question::Library { shelf: Shelf::Playlists }, &mut answers, &mut said, &mut timings, &mut ready);

        // Playing: the playlist found (a long queue) from its third song, else the album's first, else the song
        // found; at 2 % (and never heard: the WebView is muted).
        music.command(&Command::Volume { percent: 2.0 }).unwrap();
        let watch = long
            .or_else(|| album.and_then(|a| a.collection).and_then(|c| c.tracks.first().and_then(|t| t.play.clone())))
            .or(song.play.clone())
            .expect("something to play");
        // A command (made from what was said so far), then what's said for `seconds`; where its reports start.
        // What was said so far, by when.
        type Said = [(f64, Playback)];
        let mut act = |name: &str, command: &dyn Fn(&Said) -> Command, seconds: f64| {
            println!("{name}");
            let from = said.len();
            music.command(&command(&said)).unwrap();
            pump(seconds, &mut answers, &mut said, &mut timings, &mut ready);
            from
        };
        let played = act("play", &|_| Command::Play { watch: Some(watch.clone()) }, 12.0);
        let nexted = act("next", &|_| Command::Next, 10.0);
        let previous = act("previous", &|_| Command::Previous, 10.0);
        act("pause", &|_| Command::Pause, 2.0);
        act("play on", &|_| Command::Play { watch: None }, 4.0);
        act("seek", &|_| Command::Seek { seconds: 60.0 }, 3.0);
        act("repeat all", &|_| Command::Repeat { mode: Repeat::All }, 1.5);
        act("repeat one", &|_| Command::Repeat { mode: Repeat::One }, 1.5);
        act("repeat off", &|_| Command::Repeat { mode: Repeat::Off }, 1.5);
        act("shuffle on", &|_| Command::Shuffle { on: true }, 2.0);
        act("shuffle off", &|_| Command::Shuffle { on: false }, 2.0);
        let two_on = |said: &Said| Command::Jump { index: said.last().map_or(0, |(_, p)| p.index) + 2 };
        let jumped = act("jump two on", &two_on, 10.0);
        let queued = act("play next", &|_| Command::Queue { video: video.clone(), next: true }, 2.0);
        act("pause", &|_| Command::Pause, 1.0);
        music.close(&stage);
        let _ = std::fs::remove_dir_all(&user_data);

        // Each change of song is said playing all the way (loading till its sound comes), never paused between.
        for (name, from) in [("play", played), ("next", nexted), ("previous", previous), ("jump", jumped)] {
            let paused = said[from..].iter().take_while(|(_, p)| p.loading || !p.playing).any(|(_, p)| !p.playing);
            let reports: Vec<_> = said[from..].iter().map(|(_, p)| (p.playing, p.loading)).take(6).collect();
            assert!(!paused, "{name} said paused on the way: {reports:?}");
        }
        assert!(timings.iter().any(|t| t.action == "play" && t.sounding.is_some()), "never sounded: {timings:?}");
        assert!(said.iter().all(|(_, p)| !p.ad), "an ad played");
        // Play next: the song right after the one playing.
        let after = said[queued..]
            .last()
            .map(|(_, p)| p.queue.get((p.index + 1 - p.start) as usize).and_then(|i| i.video.clone()));
        assert_eq!(after, Some(Some(video.clone())), "play next didn't queue it");
        // The cab's volume (2 %) held through every song (YouTube Music puts back its own as each loads).
        let volumes: Vec<_> = said[played..].iter().filter_map(|(_, p)| p.volume).collect();
        assert!(volumes.iter().all(|v| (v - 0.02).abs() < 0.005), "the volume didn't hold: {volumes:?}");
    }
}
