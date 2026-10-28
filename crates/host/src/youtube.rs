//! YouTube: the real youtube.com in a WebView of its own, hidden, as the engine that plays the video. The
//! player never sees a YouTube page: the cabin page has the YouTube app (search, results, its own player
//! controls); this WebView's picture shows under the cabin page where the video goes, with YouTube's
//! interface hidden and no ads (`youtube.js`, run before youtube.com's own scripts), and it plays, pauses and
//! seeks as the page asks, telling the page how it plays.
//!
//! It opens on YouTube's home page, with no video: every video asked for is then YouTube's own player asking
//! for it, and that request is written so YouTube neither plays an ad nor holds the video back for one not
//! played (`youtube.js`). (A page opened on a video directly carries that video's data in the page, unasked.)
//! It shares the cabin page's WebView2 profile, so a sign-in there will be its too.
//!
//! It also asks YouTube, as its own site does, what the YouTube app shows: search results, search
//! suggestions, a video's Up next, and for a player signed in their Home and Subscriptions. The answers are
//! cut down there to a video's few fields, and here to only those, as plain text and images from YouTube's
//! own image hosts ([`Answer`]).
//!
//! Signing in happens in Google's own page, in a window on the desktop (signin.rs), on this WebView's
//! profile. WestPlay learns nothing of the account: only whether YouTube's page is signed in, and the picture
//! to show for it ([`Account`]), neither kept.

use std::cell::RefCell;
use std::path::Path;

use crate::signin::{self, Confirming, Progress, SignInWindow};
use crate::speakers::{self, Equalizer, Head, Speaker};
use crate::webview::{self, OffscreenWebView, PageSize, Stage};

/// What the cabin page asks of YouTube's player, or of YouTube (`youtube` in ui/src/shared/host.ts).
#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Command {
    /// Play a video (its 11-character id), from `start` seconds in.
    Load {
        video: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<f64>,
    },
    Play,
    Pause,
    Seek {
        seconds: f64,
    },
    Volume {
        percent: u8,
    },
    /// Captions: this track (YouTube's id for it: `.en`, `a.en` automatic…) on the video playing, its language
    /// on the videos after; `None`: none.
    Captions {
        track: Option<String>,
    },
    /// The picture's quality on every video: YouTube's own choice (`auto`), or the best a video has at or under
    /// this one (`hd720`…).
    Quality {
        quality: String,
    },
    /// The playback speed, on the video playing and the videos after (YouTube's: 0.25 to 2).
    Rate {
        rate: f32,
    },
    /// The video's audio track: a dubbed one, or its original (its id, as the player lists it).
    AudioTrack {
        id: String,
    },
    /// YouTube's Stable volume, on or off (the player's own preference: it keeps it).
    StableVolume {
        on: bool,
    },
    /// YouTube's Voice boost, on or off (likewise).
    VoiceBoost {
        on: bool,
    },
    /// The truck's own speakers, for the sound to come from them (`None`: as it is, a truck whose speakers
    /// aren't known). The host's own (speakers.rs).
    Speakers {
        speakers: Option<Vec<Speaker>>,
    },
    /// The driver's head in the cab, the sound heard from there. The host's own (the telemetry).
    Head {
        #[serde(flatten)]
        head: Head,
    },
    /// The camera is outside the cab (`on`): the sound comes muffled, through its walls, and from as far
    /// off as the camera is from the cab (`distance`, metres; 0: not known). The host's own (camera.rs).
    Outside {
        on: bool,
        #[serde(default)]
        distance: f32,
    },
    /// The player's tone (the cab's Settings › Sound): bass, mid and treble, dB each. The host's own (settings).
    Equalizer {
        #[serde(flatten)]
        equalizer: Equalizer,
    },
    /// The music's spectrum wanted (the cab's Equalizer open) or not: the page says it as it plays. The host's
    /// own.
    Spectrum {
        on: bool,
    },
    /// Show the video at this rectangle of the frame (pixels), under the cabin page; `crop`: filling it,
    /// cropped (otherwise all of it, with bars where its shape isn't the rectangle's).
    Show {
        x: f32,
        y: f32,
        width: u32,
        height: u32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        crop: bool,
    },
    /// The video fills its page, cropped, or shows all of it (`Show`'s `crop`, as YouTube's page hears it).
    Crop {
        on: bool,
    },
    /// Show no video (it plays on, heard).
    Hide,
    /// Done with YouTube: the WebView goes.
    Close,
    /// The language YouTube answers in from now on (the cabin's, e.g. `tr`).
    Language {
        code: String,
    },
    /// Videos for a search, or the next ones of a search (`more`: the token its last answer gave).
    Search {
        id: u32,
        query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        more: Option<String>,
        /// Channels only, or playlists only (YouTube's own filters).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filter: Option<SearchFilter>,
    },
    /// A playlist's page (its id): what it is and its videos, or its next videos (`more`).
    Playlist {
        id: u32,
        playlist: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        more: Option<String>,
    },
    /// A channel's page (its id, `UC…`): what it is and its videos, or its next videos (`more`).
    Channel {
        id: u32,
        channel: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        more: Option<String>,
    },
    /// YouTube's suggestions for what's typed so far.
    Suggest {
        id: u32,
        query: String,
    },
    /// A video's Up next.
    Related {
        id: u32,
        video: String,
    },
    /// The signed-in player's Home or Subscriptions, or their next videos (`more`: the token the last
    /// answer gave).
    Feed {
        id: u32,
        which: Feed,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        more: Option<String>,
    },
    /// The signed-in player subscribes to a channel (`on`), or unsubscribes.
    Subscribe {
        id: u32,
        channel: String,
        on: bool,
    },
    /// Google's sign-in for YouTube, in a window on the desktop (or that window to the front).
    SignIn,
    /// Signs YouTube's page out.
    SignOut,
}

/// What a search finds, besides everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchFilter {
    Channels,
    Playlists,
}

/// A signed-in player's feed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Feed {
    Home,
    Subscriptions,
}

impl Command {
    /// Whether it's a command YouTube's page can take: a video id that is one, text of a sensible length.
    fn is_sound(&self) -> bool {
        let text = |text: &str| text.chars().count() <= 200;
        match self {
            Command::Load { video, .. } | Command::Related { video, .. } => is_video_id(video),
            Command::Language { code } => {
                (2..=8).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-')
            }
            Command::Search { query, more, .. } => text(query) && more.as_deref().is_none_or(is_token),
            Command::Feed { more, .. } => more.as_deref().is_none_or(is_token),
            Command::Channel { channel, more, .. } => is_channel_id(channel) && more.as_deref().is_none_or(is_token),
            Command::Subscribe { channel, .. } => is_channel_id(channel),
            Command::Playlist { playlist, more, .. } => {
                is_playlist_id(playlist) && more.as_deref().is_none_or(is_token)
            }
            Command::Captions { track } => track.as_deref().is_none_or(is_track_id),
            Command::Quality { quality } => quality == "auto" || QUALITIES.contains(&quality.as_str()),
            Command::Speakers { speakers } => speakers.as_deref().is_none_or(speakers::are_sound),
            Command::Rate { rate } => rate.is_finite() && (0.25..=2.0).contains(rate),
            Command::AudioTrack { id } => is_audio_track_id(id),
            Command::Head { head } => head.is_sound(),
            Command::Outside { distance, .. } => distance.is_finite() && (0.0..10_000.0).contains(distance),
            Command::Equalizer { equalizer } => equalizer.clamped() == *equalizer,
            Command::Suggest { query, .. } => !query.trim().is_empty() && text(query),
            _ => true,
        }
    }
}

/// How the player plays, as the page hears it: only these fields, as read here (the YouTube page's message
/// is youtube.com's page talking, so nothing else of it is passed on).
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct State {
    pub video: Option<String>,
    pub title: String,
    pub author: String,
    pub live: bool,
    /// YouTube's player states: -1 unstarted, 0 ended, 1 playing, 2 paused, 3 buffering, 5 cued.
    pub state: i32,
    pub time: f64,
    pub duration: f64,
    pub volume: f64,
    pub ad: bool,
    /// The video's captions, and the one shown (`None`: none).
    #[serde(default)]
    pub captions: Vec<CaptionTrack>,
    #[serde(default)]
    pub caption: Option<String>,
    /// The qualities the video has, best first; the one chosen (`auto`: YouTube's own choice), and the one
    /// playing ("720p60").
    #[serde(default)]
    pub qualities: Vec<QualityLevel>,
    #[serde(default)]
    pub quality: String,
    #[serde(default, rename = "qualityLabel")]
    pub quality_label: String,
    /// The speeds the video can play at, and the one it plays at.
    #[serde(default)]
    pub rates: Vec<f32>,
    #[serde(default)]
    pub rate: f32,
    /// Its audio tracks (none: only its own), and the one playing.
    #[serde(default, rename = "audioTracks")]
    pub audio_tracks: Vec<AudioTrack>,
    #[serde(default, rename = "audioTrack")]
    pub audio_track: Option<String>,
    /// YouTube's Stable volume and Voice boost, on or off (`None`: the video doesn't offer it).
    #[serde(default, rename = "stableVolume")]
    pub stable_volume: Option<bool>,
    #[serde(default, rename = "voiceBoost")]
    pub voice_boost: Option<bool>,
    /// How loud the truck's speakers play in each ear (left, right), 0 to 1, and how much the limiter takes
    /// off (dB, 0 or less) (`None`: the sound plays as it is).
    #[serde(default, rename = "cabLevel", skip_serializing_if = "Option::is_none")]
    pub cab_level: Option<[f32; 3]>,
    /// Before it plays, why it hasn't yet, for the log (`None` once it plays; never passed on to the cabin page).
    #[serde(default, skip_serializing)]
    pub diag: Option<Diagnosis>,
    /// While it plays, the picture not over the whole page, as the page reads it (its video elements and their
    /// boxes), for the log (`None`: it is; never passed on to the cabin page).
    #[serde(default, skip_serializing)]
    pub picture: Option<String>,
}

/// Why a video hasn't started, as the YouTube page reads it (2026-10-27: players' videos stayed loading and the log
/// said nothing): YouTube's playability status (`OK`, `UNPLAYABLE`, `LOGIN_REQUIRED`...) and its reason, the
/// player's last error code, the way its request was written (of how many) and how often YouTube held the video
/// back, and the video element's ready and network states, its media error, the seconds buffered and the frames
/// decoded.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Diagnosis {
    pub playability: Option<String>,
    pub reason: Option<String>,
    pub error: Option<i32>,
    pub way: u32,
    pub ways: u32,
    pub holds: u32,
    pub ready: i32,
    pub network: i32,
    pub media_error: Option<i32>,
    pub buffered: f64,
    pub frames: u64,
}

impl Diagnosis {
    /// Its texts kept to a line of letters and the like (they're youtube.com's page's).
    fn sound(mut self) -> Self {
        let plain = |text: &mut Option<String>, most: usize| {
            if let Some(text) = text {
                text.retain(|c| !c.is_control());
                cut(text, most);
            }
        };
        plain(&mut self.playability, 32);
        plain(&mut self.reason, 80);
        if !self.buffered.is_finite() {
            self.buffered = 0.0;
        }
        self
    }
}

impl std::fmt::Display for Diagnosis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let or = |value: Option<i32>| value.map_or("none".to_owned(), |value| value.to_string());
        let held = if self.holds == 1 { "once".to_owned() } else { format!("{} times", self.holds) };
        write!(
            f,
            "playability {} ({}), player error {}, way {} of {}, held back {held}; ",
            self.playability.as_deref().unwrap_or("?"),
            self.reason.as_deref().unwrap_or("no reason"),
            or(self.error),
            self.way,
            self.ways,
        )?;
        write!(
            f,
            "video element ready {} network {}, media error {}, {:.1} s buffered, {} frames decoded",
            self.ready,
            self.network,
            or(self.media_error),
            self.buffered,
            self.frames
        )
    }
}

/// A video's captions: YouTube's id for them and its name for them ("English (auto-generated)").
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CaptionTrack {
    pub track: String,
    pub name: String,
}

/// One of a video's audio tracks: the player's id for it, YouTube's name ("English (US) original",
/// "Arabic"), and whether it's the original.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AudioTrack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub original: bool,
}

/// A quality a video has: YouTube's name for it (`hd1080`) and its label ("1080p60").
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct QualityLevel {
    pub quality: String,
    #[serde(default)]
    pub label: String,
}

/// YouTube's qualities, from the least.
const QUALITIES: [&str; 10] =
    ["tiny", "small", "medium", "large", "hd720", "hd1080", "hd1440", "hd2160", "hd2880", "highres"];

impl State {
    /// The state with only what can be shown: real track ids and qualities, texts cut.
    fn sound(mut self) -> Self {
        self.captions.retain(|track| is_track_id(&track.track));
        self.captions.truncate(100);
        for track in &mut self.captions {
            cut(&mut track.name, 80);
        }
        if self.caption.as_deref().is_some_and(|track| !is_track_id(track)) {
            self.caption = None;
        }
        self.qualities.retain(|level| QUALITIES.contains(&level.quality.as_str()));
        self.qualities.truncate(QUALITIES.len());
        for level in &mut self.qualities {
            cut(&mut level.label, 20);
        }
        if !QUALITIES.contains(&self.quality.as_str()) {
            self.quality = "auto".into();
        }
        cut(&mut self.quality_label, 20);
        self.rates.retain(|rate| rate.is_finite() && (0.0625..=16.0).contains(rate));
        self.rates.truncate(16);
        if !(self.rate.is_finite() && (0.0625..=16.0).contains(&self.rate)) {
            self.rate = 1.0;
        }
        self.audio_tracks.retain(|track| is_audio_track_id(&track.id));
        self.audio_tracks.truncate(64);
        for track in &mut self.audio_tracks {
            cut(&mut track.name, 80);
        }
        if self.audio_track.as_deref().is_some_and(|id| !is_audio_track_id(id)) {
            self.audio_track = None;
        }
        self.cab_level = self
            .cab_level
            .filter(|told| told.iter().all(|v| v.is_finite()))
            .map(|[left, right, limited]| [left.clamp(0.0, 1.0), right.clamp(0.0, 1.0), limited.clamp(-60.0, 0.0)]);
        self.diag = self.diag.map(Diagnosis::sound);
        if let Some(picture) = &mut self.picture {
            picture.retain(|c| !c.is_control());
            cut(picture, 300);
        }
        self
    }
}

/// `text` cut to `most` characters.
/// YouTube's cookie consent ("Before you continue to YouTube", in the EU, the UK and Switzerland: its own page in
/// YouTube's place, which a hidden player can't get past), answered before YouTube asks, as its Reject all answers
/// (`SOCS` `CAE=`, as NewPipe answers it). A choice the profile holds already is kept; `CAA…` is YouTube's "not
/// answered yet" (as yt-dlp reads it), not a choice. YouTube Music's is the same cookie.
pub fn answer_consent(view: &OffscreenWebView) {
    let kept = |socs: &str| !socs.starts_with("CAA");
    if let Err(error) = view.set_cookie_unless("https://www.youtube.com/", ".youtube.com", "SOCS", "CAE=", kept) {
        log!("YouTube's cookie consent not answered: {error}");
    }
}

fn cut(text: &mut String, most: usize) {
    if let Some((at, _)) = text.char_indices().nth(most) {
        text.truncate(at);
    }
}

/// What YouTube's page said (how the player plays, an answer, whether it's signed in), or whether the
/// sign-in window is open.
#[derive(Debug, PartialEq)]
pub enum Report {
    State(Box<State>),
    Answer(Box<Answer>),
    Account(Account),
    SignInWindow(bool),
    /// The sign-in window closed signed in, and YouTube's page is yet to say it's signed in (`true`), or it
    /// has (or took too long: `false`).
    Confirming(bool),
    /// The music's spectrum, while the cab's Equalizer asks for it ([`speakers::read_spectrum`]).
    Spectrum(Vec<u8>),
}

/// YouTube's answer to the page's question `answer` (its id): videos (and the token for more of them), or
/// suggestions; `failed` when YouTube didn't answer. Only these fields pass (it's youtube.com's page
/// talking): see [`Answer::sound`].
#[derive(Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Answer {
    #[serde(rename(serialize = "id"))]
    pub answer: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub videos: Option<Vec<Video>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub more: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<Vec<String>>,
    /// Channels found by a search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<Channel>>,
    /// The channel a channel's page is (its first answer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<Channel>,
    /// Playlists found by a search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playlists: Option<Vec<Playlist>>,
    /// The playlist a playlist's page is (its first answer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playlist: Option<Playlist>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub failed: bool,
}

/// A channel as the YouTube app shows it: its id, name and pictures, and YouTube's own texts for it
/// ("@LofiGirl", "15.8M subscribers", "468 videos").
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Channel {
    pub channel: String,
    pub name: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub subscribers: String,
    #[serde(default)]
    pub videos: String,
    /// Whether the signed-in player is subscribed to it (`None`: not signed in, or not known).
    #[serde(default)]
    pub subscribed: Option<bool>,
}

/// A playlist as the YouTube app shows it: its id, title, channel and cover, and YouTube's own texts for it
/// ("13 videos", "2M views"); in a search, how many of the search's videos come before it (`after`).
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Playlist {
    pub playlist: String,
    pub title: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default, rename = "channelId", skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub count: String,
    #[serde(default)]
    pub views: String,
    #[serde(default)]
    pub after: u32,
}

/// A video as the YouTube app lists it. Its texts are YouTube's, in the language asked for ("17K", "2 days
/// ago"); `duration` is empty for a live video.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Video {
    pub video: String,
    pub title: String,
    pub channel: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub duration: String,
    #[serde(default)]
    pub views: String,
    #[serde(default)]
    pub age: String,
    #[serde(default)]
    pub live: bool,
    /// Its channel's id, to open the channel (`None`: not known).
    #[serde(default, rename = "channelId", skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
}

impl Answer {
    /// The answer with only what can be shown: videos with a real id, texts cut to a sensible length,
    /// images only from YouTube's image hosts, at most a page's worth.
    fn sound(mut self) -> Self {
        if let Some(videos) = &mut self.videos {
            videos.retain(|video| is_video_id(&video.video));
            // A playlist's page lists a hundred.
            videos.truncate(100);
            for video in videos {
                cut(&mut video.title, 200);
                cut(&mut video.channel, 100);
                for text in [&mut video.duration, &mut video.views, &mut video.age] {
                    cut(text, 40);
                }
                for image in [&mut video.avatar, &mut video.thumbnail] {
                    if image.as_deref().is_some_and(|url| !is_youtube_image(url)) {
                        *image = None;
                    }
                }
                if video.channel_id.as_deref().is_some_and(|id| !is_channel_id(id)) {
                    video.channel_id = None;
                }
            }
        }
        let channel_sound = |channel: &mut Channel| {
            cut(&mut channel.name, 100);
            for text in [&mut channel.handle, &mut channel.subscribers, &mut channel.videos] {
                cut(text, 60);
            }
            for image in [&mut channel.avatar, &mut channel.banner] {
                if image.as_deref().is_some_and(|url| !is_youtube_image(url)) {
                    *image = None;
                }
            }
        };
        if let Some(channels) = &mut self.channels {
            channels.retain(|channel| is_channel_id(&channel.channel));
            channels.truncate(20);
            channels.iter_mut().for_each(channel_sound);
        }
        if self.channel.as_ref().is_some_and(|channel| !is_channel_id(&channel.channel)) {
            self.channel = None;
        }
        if let Some(channel) = &mut self.channel {
            channel_sound(channel);
        }
        let playlist_sound = |playlist: &mut Playlist| {
            cut(&mut playlist.title, 200);
            cut(&mut playlist.channel, 100);
            for text in [&mut playlist.count, &mut playlist.views] {
                cut(text, 40);
            }
            if playlist.thumbnail.as_deref().is_some_and(|url| !is_youtube_image(url)) {
                playlist.thumbnail = None;
            }
            if playlist.channel_id.as_deref().is_some_and(|id| !is_channel_id(id)) {
                playlist.channel_id = None;
            }
        };
        if let Some(playlists) = &mut self.playlists {
            playlists.retain(|playlist| is_playlist_id(&playlist.playlist));
            playlists.truncate(20);
            playlists.iter_mut().for_each(playlist_sound);
        }
        if self.playlist.as_ref().is_some_and(|playlist| !is_playlist_id(&playlist.playlist)) {
            self.playlist = None;
        }
        if let Some(playlist) = &mut self.playlist {
            playlist_sound(playlist);
        }
        if let Some(suggestions) = &mut self.suggestions {
            suggestions.truncate(15);
            for suggestion in suggestions {
                cut(suggestion, 200);
            }
        }
        if self.more.as_deref().is_some_and(|more| !is_token(more)) {
            self.more = None;
        }
        self
    }
}

/// An image on YouTube's own image hosts (thumbnails, channel avatars; Google's, for a player's picture).
pub(crate) fn is_youtube_image(url: &str) -> bool {
    [
        "https://i.ytimg.com/",
        "https://i9.ytimg.com/",
        "https://yt3.ggpht.com/",
        "https://yt3.googleusercontent.com/",
        "https://lh3.googleusercontent.com/",
    ]
    .iter()
    .any(|host| url.starts_with(host))
}

/// A YouTube channel id: `UC` and 22 characters of letters, digits, `-` and `_`.
fn is_channel_id(id: &str) -> bool {
    id.len() == 24 && id.starts_with("UC") && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A YouTube playlist id (`PL…`, `OLAK5uy_…`, `WL`…): letters, digits, `-` and `_`, of a sensible length.
fn is_playlist_id(id: &str) -> bool {
    (2..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The player's id for an audio track (`251;Cg8KBWFjb250…`: a format number and some base64).
fn is_audio_track_id(id: &str) -> bool {
    (1..=200).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b";-_=+/.".contains(&b))
}

/// YouTube's id for a video's captions (`.en`, `a.en`, `.en.nP7-2PuUl7o`): letters, digits and `.-_`.
fn is_track_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
}

/// YouTube's token for more of a list: letters, digits and a few signs, of a sensible length.
pub(crate) fn is_token(token: &str) -> bool {
    token.len() <= 4096 && token.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_=%".contains(&b))
}

/// Whether YouTube's page is signed in, and the picture to show for the player (from Google's image hosts).
/// Nothing else of the account: WestPlay neither asks nor keeps.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub signed_in: bool,
    #[serde(default)]
    pub avatar: Option<String>,
}

impl Account {
    fn sound(mut self) -> Self {
        if !self.signed_in || self.avatar.as_deref().is_some_and(|url| !is_youtube_image(url)) {
            self.avatar = None;
        }
        self
    }
}

/// What the page was told last of the cab: the truck (whose speakers it has), the head, whether the camera is
/// outside and how far, and the tone.
type Told = (Option<Option<Vec<Speaker>>>, Option<Head>, Option<(bool, f32)>, Option<Equalizer>, Option<bool>);

pub struct YouTube {
    view: OffscreenWebView,
    /// The page's size, the picture kept within it.
    frame: PageSize,
    outbox: RefCell<Outbox>,
    /// The sign-in window while it's open.
    sign_in: RefCell<Option<SignInWindow>>,
    /// Once it closed signed in, until the page says it's signed in.
    confirming: Confirming,
    /// Reports of the host's own, for the next `take_reports`.
    pending: RefCell<Vec<Report>>,
    /// The speakers the page has, and the head and camera it was told last ([`YouTube::cab`]).
    cab: RefCell<Told>,
}

impl YouTube {
    /// Opens YouTube in a hidden WebView of `stage`'s frame (`width` x `height`), under the cabin page.
    pub fn start(stage: &Stage, width: u32, height: u32, user_data: &Path) -> Result<Self, webview::Error> {
        let view = OffscreenWebView::new(stage, width, height, user_data)?;
        view.log_as("youtube")?;
        view.show(false)?;
        stage.add(&view, false)?;
        // The truck speakers' sound first: the page's script makes its own of it.
        view.add_startup_script(concat!(
            include_str!("cab_sound.js"),
            "\n",
            include_str!("youtube.js"),
            "\n",
            include_str!("consent.js")
        ))?;
        answer_consent(&view);
        view.navigate("https://www.youtube.com/")?;
        Ok(Self {
            view,
            frame: stage.page_size(),
            outbox: RefCell::new(Outbox::default()),
            sign_in: RefCell::new(None),
            confirming: Confirming::default(),
            pending: RefCell::new(Vec::new()),
            cab: RefCell::new((None, None, None, None, None)),
        })
    }

    /// Carries out the page's command (`Close` is the caller's: [`YouTube::close`]).
    pub fn command(&self, command: &Command) -> windows::core::Result<()> {
        match command {
            Command::Show { x, y, width, height, crop } => {
                self.pass(&Command::Crop { on: *crop })?;
                let page = self.frame.get();
                let (x, y, width, height) = within(page, *x, *y, *width, *height);
                // A small picture (Now playing's) is the whole page scaled down: YouTube keeps playing at the
                // screen's quality, ready to be full screen again; a large one is the page at its size.
                if width * 2 < page.0 {
                    self.view.fit(page, x, y, width, height)?;
                } else {
                    self.view.place(x, y, width, height)?;
                }
                self.view.show(true)
            }
            Command::Hide => self.view.show(false),
            Command::Close => Ok(()),
            Command::SignIn => {
                let mut sign_in = self.sign_in.borrow_mut();
                if let Some(window) = sign_in.as_ref() {
                    window.bring_forward();
                } else {
                    match SignInWindow::open(self.view.environment(), &signin::YOUTUBE) {
                        Ok(window) => {
                            *sign_in = Some(window);
                            self.pending.borrow_mut().push(Report::SignInWindow(true));
                        }
                        Err(e) => crate::startup::mark(&format!("host: the YouTube sign-in window couldn't open: {e}")),
                    }
                }
                Ok(())
            }
            Command::SignOut => {
                self.view.delete_cookies()?;
                self.reload()
            }
            command if !command.is_sound() => Ok(()),
            _ => self.pass(command),
        }
    }

    /// The truck's speakers (none: its sound plays as it is), the driver's head and whether the camera is outside
    /// the cab, and how far (metres; 0: not known): the page is told the speakers when they change, the head when it
    /// moves, the camera when it goes in or out or a quarter metre further, the tone when it's set, the spectrum when it's wanted.
    pub fn cab(
        &self,
        speakers: Option<&[Speaker]>,
        head: Head,
        outside: bool,
        distance: f32,
        equalizer: Equalizer,
        spectrum: bool,
    ) -> windows::core::Result<()> {
        let mut cab = self.cab.borrow_mut();
        // The head first: the speakers' loudness is set from where it is as they're placed.
        if cab.1 != Some(head) && head.is_sound() {
            cab.1 = Some(head);
            self.pass(&Command::Head { head })?;
        }
        if cab.0.as_ref().map(Option::as_deref) != Some(speakers) {
            let speakers = speakers.map(<[_]>::to_vec);
            cab.0 = Some(speakers.clone());
            self.pass(&Command::Speakers { speakers })?;
        }
        let camera = (outside, if outside { (distance * 4.0).round() / 4.0 } else { 0.0 });
        if cab.2 != Some(camera) {
            cab.2 = Some(camera);
            self.pass(&Command::Outside { on: camera.0, distance: camera.1 })?;
        }
        if cab.3 != Some(equalizer) {
            cab.3 = Some(equalizer);
            self.pass(&Command::Equalizer { equalizer })?;
        }
        if cab.4 != Some(spectrum) {
            cab.4 = Some(spectrum);
            self.pass(&Command::Spectrum { on: spectrum })?;
        }
        Ok(())
    }

    /// Passes a command on to YouTube's page (held until it's there).
    fn pass(&self, command: &Command) -> windows::core::Result<()> {
        match self.outbox.borrow_mut().send(command) {
            Some(message) => self.view.post_json(&message),
            None => Ok(()),
        }
    }

    /// YouTube's page again, from its home page (signed in or out since, here or in YouTube Music, which shares
    /// the sign-in): it's owed the video and the rest again as it says it's there ([`Outbox::ready`]).
    pub fn reload(&self) -> windows::core::Result<()> {
        self.view.navigate("https://www.youtube.com/")
    }

    /// What YouTube's page said since the last call, oldest first, and whether the sign-in window closed.
    /// (YouTube's page saying it's there gets what it's owed: [`Outbox::ready`]; signed in, it's loaded again.)
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
            if message == r#"{"ready":true}"# {
                for owed in self.outbox.borrow_mut().ready() {
                    let _ = self.view.post_json(&owed);
                }
            } else if let Some(bars) = speakers::read_spectrum(&message) {
                reports.push(Report::Spectrum(bars));
            } else if let Some(report) = read_report(&message) {
                reports.push(report);
            }
        }
        let signed_in = reports.iter().rev().find_map(|report| match report {
            Report::Account(account) => Some(account.signed_in),
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

/// A message of YouTube's page: an answer (it has `answer`), whether it's signed in (`account`), or how the
/// player plays.
fn read_report(message: &str) -> Option<Report> {
    let mut value: serde_json::Value = serde_json::from_str(message).ok()?;
    if let Some(account) = value.get_mut("account") {
        serde_json::from_value::<Account>(account.take()).ok().map(|account| Report::Account(account.sound()))
    } else if value.get("answer").is_some() {
        serde_json::from_value::<Answer>(value).ok().map(|answer| Report::Answer(Box::new(answer.sound())))
    } else {
        serde_json::from_value::<State>(value).ok().map(|state| Report::State(Box::new(state.sound())))
    }
}

/// The commands for YouTube's page, as JSON. A message posted before the page is there goes nowhere, so
/// they're held until it says it is; and a page that loads again gets the video and its volume again
/// (YouTube's home page reloads itself once on a new profile, `?themeRefresh=1`, and the video asked of the
/// page before goes with it).
#[derive(Default)]
struct Outbox {
    ready: bool,
    held: Vec<String>,
    video: Option<String>,
    volume: Option<String>,
    language: Option<String>,
    crop: Option<String>,
    captions: Option<String>,
    quality: Option<String>,
    rate: Option<String>,
    speakers: Option<String>,
    head: Option<String>,
    outside: Option<String>,
    equalizer: Option<String>,
    spectrum: Option<String>,
}

impl Outbox {
    /// `command` as the message to post now, or `None`: held until the page is there.
    fn send(&mut self, command: &Command) -> Option<String> {
        let message = serde_json::to_string(command).expect("commands serialise");
        match command {
            Command::Load { .. } => self.video = Some(message.clone()),
            Command::Volume { .. } => self.volume = Some(message.clone()),
            Command::Language { .. } => self.language = Some(message.clone()),
            Command::Crop { .. } => self.crop = Some(message.clone()),
            Command::Captions { .. } => self.captions = Some(message.clone()),
            Command::Quality { .. } => self.quality = Some(message.clone()),
            Command::Rate { .. } => self.rate = Some(message.clone()),
            Command::Speakers { .. } => self.speakers = Some(message.clone()),
            Command::Head { .. } => self.head = Some(message.clone()),
            Command::Outside { .. } => self.outside = Some(message.clone()),
            Command::Equalizer { .. } => self.equalizer = Some(message.clone()),
            Command::Spectrum { .. } => self.spectrum = Some(message.clone()),
            _ => {}
        }
        if self.ready {
            Some(message)
        } else {
            self.held.push(message);
            None
        }
    }

    /// The page says it's there: what to post to it, in order (the first time, what was held; after, the
    /// language, the cropping, the volume, the captions and quality, and the video again).
    fn ready(&mut self) -> Vec<String> {
        if std::mem::replace(&mut self.ready, true) {
            [
                &self.language,
                &self.crop,
                &self.volume,
                &self.captions,
                &self.quality,
                &self.rate,
                &self.speakers,
                &self.head,
                &self.outside,
                &self.equalizer,
                &self.spectrum,
                &self.video,
            ]
            .into_iter()
            .flatten()
            .cloned()
            .collect()
        } else {
            std::mem::take(&mut self.held)
        }
    }
}

/// A rectangle kept within the frame, at least a pixel each way.
pub(crate) fn within(
    (frame_width, frame_height): (u32, u32),
    x: f32,
    y: f32,
    width: u32,
    height: u32,
) -> (f32, f32, u32, u32) {
    let x = x.clamp(0.0, frame_width as f32 - 1.0);
    let y = y.clamp(0.0, frame_height as f32 - 1.0);
    let width = width.clamp(1, frame_width - x as u32);
    let height = height.clamp(1, frame_height - y as u32);
    (x, y, width, height)
}

/// A YouTube video id: 11 characters of letters, digits, `-` and `_`.
pub(crate) fn is_video_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `{"type":"youtube","state":{...}}`: how YouTube's player plays (`null`: no YouTube running).
pub fn state_message(state: Option<&State>) -> String {
    serde_json::json!({ "type": "youtube", "state": state }).to_string()
}

/// `{"type":"youtubeAccount","signedIn":…,"avatar":…}`: whether YouTube's page is signed in.
pub fn account_message(account: &Account) -> String {
    let mut message = serde_json::to_value(account).expect("accounts serialise");
    message["type"] = "youtubeAccount".into();
    message.to_string()
}

/// `{"type":"youtubeSignIn","open":…}`: whether the sign-in window is open.
pub fn sign_in_message(open: bool) -> String {
    serde_json::json!({ "type": "youtubeSignIn", "open": open }).to_string()
}

/// `{"type":"youtubeAnswer","id":…,"videos":[…],…}`: YouTube's answer to the page's question.
pub fn answer_message(answer: &Answer) -> String {
    let mut message = serde_json::to_value(answer).expect("answers serialise");
    message["type"] = "youtubeAnswer".into();
    message.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_pages_commands() {
        let read = |text: &str| serde_json::from_str::<Command>(text).ok();
        assert_eq!(
            read(r#"{"action":"load","video":"dQw4w9WgXcQ"}"#),
            Some(Command::Load { video: "dQw4w9WgXcQ".into(), start: None })
        );
        assert_eq!(read(r#"{"action":"seek","seconds":61.5}"#), Some(Command::Seek { seconds: 61.5 }));
        assert_eq!(
            read(r#"{"action":"show","x":0,"y":0,"width":1280,"height":720}"#),
            Some(Command::Show { x: 0.0, y: 0.0, width: 1280, height: 720, crop: false })
        );
        assert_eq!(
            read(r#"{"action":"show","x":8,"y":9,"width":372,"height":200,"crop":true}"#),
            Some(Command::Show { x: 8.0, y: 9.0, width: 372, height: 200, crop: true })
        );
        assert_eq!(read(r#"{"action":"close"}"#), Some(Command::Close));
        assert_eq!(read(r#"{"action":"explode"}"#), None);
        // As passed on to the YouTube page.
        assert_eq!(
            serde_json::to_string(&Command::Volume { percent: 40 }).unwrap(),
            r#"{"action":"volume","percent":40}"#
        );
    }

    #[test]
    fn plays_only_what_looks_like_a_video() {
        assert!(is_video_id("dQw4w9WgXcQ") && is_video_id("1-LpQekNa9g"));
        assert!(!is_video_id("dQw4w9WgXc") && !is_video_id("dQw4w9WgXcQ'") && !is_video_id("');alert(1)//"));
    }

    #[test]
    fn holds_commands_for_the_page_and_gives_a_reloaded_page_the_video_again() {
        let mut outbox = Outbox::default();
        let volume = Command::Volume { percent: 40 };
        let load = |video: &str| Command::Load { video: video.into(), start: None };
        // Before the page is there: held, in order.
        assert_eq!(outbox.send(&volume), None);
        assert_eq!(outbox.send(&load("dQw4w9WgXcQ")), None);
        assert_eq!(outbox.send(&Command::Pause), None);
        assert_eq!(
            outbox.ready(),
            [
                r#"{"action":"volume","percent":40}"#,
                r#"{"action":"load","video":"dQw4w9WgXcQ"}"#,
                r#"{"action":"pause"}"#
            ]
        );
        // There: posted at once.
        assert_eq!(outbox.send(&load("kJQP7kiw5Fk")).as_deref(), Some(r#"{"action":"load","video":"kJQP7kiw5Fk"}"#));
        assert_eq!(outbox.send(&Command::Play).as_deref(), Some(r#"{"action":"play"}"#));
        // The page loaded again: the volume and the latest video again, nothing else.
        assert_eq!(
            outbox.ready(),
            [r#"{"action":"volume","percent":40}"#, r#"{"action":"load","video":"kJQP7kiw5Fk"}"#]
        );
        // And the language, once there is one.
        assert!(outbox.send(&Command::Language { code: "tr".into() }).is_some());
        assert_eq!(outbox.ready()[0], r#"{"action":"language","code":"tr"}"#);
    }

    #[test]
    fn takes_only_sound_questions() {
        let sound = |text: &str| serde_json::from_str::<Command>(text).unwrap().is_sound();
        assert!(sound(r#"{"action":"search","id":1,"query":"ets2 scania"}"#));
        assert!(sound(r#"{"action":"search","id":2,"query":"","more":"EvgCEgtldHMy-_%3D"}"#));
        assert!(!sound(r#"{"action":"search","id":2,"query":"","more":"<script>"}"#));
        assert!(sound(r#"{"action":"suggest","id":3,"query":"ets2 sca"}"#));
        assert!(!sound(r#"{"action":"suggest","id":3,"query":"  "}"#));
        assert!(sound(r#"{"action":"related","id":4,"video":"kJQP7kiw5Fk"}"#));
        assert!(!sound(r#"{"action":"related","id":4,"video":"x"}"#));
        assert!(sound(r#"{"action":"language","code":"tr"}"#));
        assert!(!sound(r#"{"action":"language","code":"tr&x=1"}"#));
        assert!(!sound(&format!(r#"{{"action":"search","id":1,"query":"{}"}}"#, "a".repeat(201))));
    }

    #[test]
    fn passes_on_only_a_videos_fields_and_youtubes_images() {
        let message = r#"{"answer":7,"videos":[
            {"video":"s8YcZxzPxLs","title":"ETS 2 1.60 SCANIA","channel":"BALPORSUGU","avatar":"https://yt3.ggpht.com/a","thumbnail":"https://i.ytimg.com/vi/s8YcZxzPxLs/hq720.jpg","duration":"13:34","views":"17K","age":"2mo ago","live":false,"extra":"<img src=x>"},
            {"video":"rFZHOHl-L8A","title":"lofi hip hop radio","channel":"Lofi Girl","avatar":"https://evil.example/a.png","thumbnail":"javascript:alert(1)","duration":"","views":"14M","age":"","live":true},
            {"video":"bad","title":"Not a video","channel":"x"}
        ],"more":"EvgC"}"#;
        let Some(Report::Answer(answer)) = read_report(message) else { panic!("an answer") };
        let videos = answer.videos.as_ref().unwrap();
        assert_eq!(videos.len(), 2);
        assert_eq!(videos[0].avatar.as_deref(), Some("https://yt3.ggpht.com/a"));
        assert_eq!((videos[1].avatar.as_deref(), videos[1].thumbnail.as_deref()), (None, None));
        assert_eq!(
            answer_message(&answer),
            r#"{"id":7,"more":"EvgC","type":"youtubeAnswer","videos":[{"age":"2mo ago","avatar":"https://yt3.ggpht.com/a","channel":"BALPORSUGU","duration":"13:34","live":false,"thumbnail":"https://i.ytimg.com/vi/s8YcZxzPxLs/hq720.jpg","title":"ETS 2 1.60 SCANIA","video":"s8YcZxzPxLs","views":"17K"},{"age":"","avatar":null,"channel":"Lofi Girl","duration":"","live":true,"thumbnail":null,"title":"lofi hip hop radio","video":"rFZHOHl-L8A","views":"14M"}]}"#
        );
        let Some(Report::Answer(answer)) = read_report(r#"{"answer":8,"failed":true}"#) else { panic!() };
        assert_eq!(answer_message(&answer), r#"{"failed":true,"id":8,"type":"youtubeAnswer"}"#);
        // How the player plays is not an answer.
        let state = r#"{"video":"dQw4w9WgXcQ","title":"t","author":"a","live":false,"state":1,"time":1,"duration":2,"volume":3,"ad":false}"#;
        assert!(matches!(read_report(state), Some(Report::State(_))));
    }

    #[test]
    fn passes_on_only_real_channels_and_their_youtube_pictures() {
        let message = r#"{"answer":9,"channels":[
            {"channel":"UCSJ4gkVC6NrvII8umztf0Ow","name":"Lofi Girl","avatar":"https://yt3.googleusercontent.com/a","banner":"https://evil.example/b.png","handle":"@LofiGirl","subscribers":"15.8M subscribers","videos":"468 videos","extra":"x"},
            {"channel":"not-a-channel","name":"Nope"}
        ],"channel":{"channel":"UCSJ4gkVC6NrvII8umztf0Ow","name":"Lofi Girl"},
        "videos":[{"video":"dQw4w9WgXcQ","title":"t","channel":"c","channelId":"javascript:x"}]}"#;
        let Some(Report::Answer(answer)) = read_report(message) else { panic!("an answer") };
        let channels = answer.channels.as_ref().unwrap();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].avatar.as_deref(), Some("https://yt3.googleusercontent.com/a"));
        assert_eq!(channels[0].banner, None);
        assert_eq!(answer.channel.as_ref().map(|c| c.name.as_str()), Some("Lofi Girl"));
        assert_eq!(answer.videos.as_ref().unwrap()[0].channel_id, None);
        let sound = |text: &str| serde_json::from_str::<Command>(text).unwrap().is_sound();
        assert!(sound(r#"{"action":"channel","id":1,"channel":"UCSJ4gkVC6NrvII8umztf0Ow"}"#));
        assert!(!sound(r#"{"action":"channel","id":1,"channel":"@LofiGirl"}"#));
        assert!(sound(r#"{"action":"search","id":2,"query":"ets2","filter":"channels"}"#));
        assert!(
            serde_json::from_str::<Command>(r#"{"action":"search","id":2,"query":"x","filter":"shorts"}"#).is_err()
        );
        assert!(sound(r#"{"action":"subscribe","id":3,"channel":"UCSJ4gkVC6NrvII8umztf0Ow","on":true}"#));
        assert!(!sound(r#"{"action":"subscribe","id":3,"channel":"UC","on":true}"#));
        assert!(is_channel_id("UC0h3_FaL9Vdj1t4Bz3AGblA") && !is_channel_id("UC0h3_FaL9Vdj1t4Bz3AGbl"));
    }

    #[test]
    fn passes_on_only_whether_signed_in_and_a_googles_picture() {
        let read = |text: &str| match read_report(text) {
            Some(Report::Account(account)) => account_message(&account),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            read(
                r#"{"account":{"signedIn":true,"avatar":"https://yt3.ggpht.com/me=s88","name":"Someone","email":"x@y"}}"#
            ),
            r#"{"avatar":"https://yt3.ggpht.com/me=s88","signedIn":true,"type":"youtubeAccount"}"#
        );
        assert_eq!(
            read(r#"{"account":{"signedIn":true,"avatar":"https://evil.example/track.png"}}"#),
            r#"{"avatar":null,"signedIn":true,"type":"youtubeAccount"}"#
        );
        assert_eq!(
            read(r#"{"account":{"signedIn":false,"avatar":"https://yt3.ggpht.com/me"}}"#),
            r#"{"avatar":null,"signedIn":false,"type":"youtubeAccount"}"#
        );
        assert_eq!(sign_in_message(true), r#"{"open":true,"type":"youtubeSignIn"}"#);
        let sound = |text: &str| serde_json::from_str::<Command>(text).unwrap().is_sound();
        assert!(sound(r#"{"action":"feed","id":1,"which":"subscriptions"}"#));
        assert!(!sound(r#"{"action":"feed","id":1,"which":"home","more":"no spaces"}"#));
        assert!(serde_json::from_str::<Command>(r#"{"action":"feed","id":1,"which":"history"}"#).is_err());
    }

    #[test]
    fn keeps_the_video_on_the_frame() {
        assert_eq!(within((1280, 720), 0.0, 0.0, 1280, 720), (0.0, 0.0, 1280, 720));
        assert_eq!(within((1280, 720), 900.0, 600.0, 1280, 720), (900.0, 600.0, 380, 120));
        assert_eq!(within((1280, 720), -50.0, 0.0, 0, 9999), (0.0, 0.0, 1, 720));
    }

    #[test]
    fn passes_on_only_the_players_own_fields() {
        let report = r#"{"video":"dQw4w9WgXcQ","title":"Never Gonna Give You Up","author":"Rick Astley","live":false,
            "state":1,"time":12.3,"duration":213,"volume":100,"ad":false,"extra":"<img src=x>"}"#;
        let state: State = serde_json::from_str(report).unwrap();
        assert_eq!(
            state_message(Some(&state)),
            r#"{"state":{"ad":false,"audioTrack":null,"audioTracks":[],"author":"Rick Astley","caption":null,"captions":[],"duration":213.0,"live":false,"qualities":[],"quality":"","qualityLabel":"","rate":0.0,"rates":[],"stableVolume":null,"state":1,"time":12.3,"title":"Never Gonna Give You Up","video":"dQw4w9WgXcQ","voiceBoost":null,"volume":100.0},"type":"youtube"}"#
        );
        assert_eq!(state_message(None), r#"{"state":null,"type":"youtube"}"#);
    }

    #[test]
    fn passes_on_only_real_captions_and_qualities() {
        let report = r#"{"video":"dQw4w9WgXcQ","title":"t","author":"a","live":false,"state":1,"time":1,"duration":2,
            "volume":3,"ad":false,"captions":[{"track":".en","name":"English"},{"track":"a.en","name":"English (auto-generated)"},
            {"track":"<b>","name":"x"}],"caption":"a.en","qualities":[{"quality":"hd1080","label":"1080p60"},
            {"quality":"evil","label":"x"}],"quality":"hd9999","qualityLabel":"720p"}"#;
        let Some(Report::State(state)) = read_report(report) else { panic!("a state") };
        assert_eq!(state.captions.iter().map(|t| t.track.as_str()).collect::<Vec<_>>(), [".en", "a.en"]);
        assert_eq!(state.caption.as_deref(), Some("a.en"));
        assert_eq!(state.qualities, [QualityLevel { quality: "hd1080".into(), label: "1080p60".into() }]);
        assert_eq!((state.quality.as_str(), state.quality_label.as_str()), ("auto", "720p"));
        // An older page's report, without them.
        let older = r#"{"video":null,"title":"","author":"","live":false,"state":-1,"time":0,"duration":0,"volume":100,"ad":false}"#;
        let Some(Report::State(state)) = read_report(older) else { panic!("a state") };
        assert!(state.captions.is_empty() && state.caption.is_none() && state.quality == "auto");
        let sound = |text: &str| serde_json::from_str::<Command>(text).unwrap().is_sound();
        assert!(sound(r#"{"action":"captions","track":"a.en"}"#) && sound(r#"{"action":"captions","track":null}"#));
        assert!(!sound(r#"{"action":"captions","track":"en'); alert(1)"}"#));
        assert!(
            sound(r#"{"action":"quality","quality":"hd720"}"#) && sound(r#"{"action":"quality","quality":"auto"}"#)
        );
        assert!(!sound(r#"{"action":"quality","quality":"8k"}"#));
        // A reloaded page gets them again, before the video.
        let mut outbox = Outbox::default();
        outbox.ready();
        outbox.send(&Command::Quality { quality: "hd720".into() });
        outbox.send(&Command::Captions { track: Some(".en".into()) });
        outbox.send(&Command::Load { video: "dQw4w9WgXcQ".into(), start: None });
        assert_eq!(
            outbox.ready(),
            [
                r#"{"action":"captions","track":".en"}"#,
                r#"{"action":"quality","quality":"hd720"}"#,
                r#"{"action":"load","video":"dQw4w9WgXcQ"}"#
            ]
        );
    }

    #[test]
    fn tells_the_page_the_trucks_speakers_once_and_the_head_as_it_moves() {
        let head = Head { x: 0.0, y: 0.3, z: 0.1, heading: 0.0, pitch: 0.0 };
        let speakers = Command::Speakers { speakers: speakers::in_truck("scania.s_2016").map(<[_]>::to_vec) };
        let message = serde_json::to_string(&speakers).unwrap();
        assert!(
            message.starts_with(
                r#"{"action":"speakers","speakers":[{"band":[80.0,2000.0],"level":0.0,"channel":"left","at":[-1.065,"#
            ),
            "{message}"
        );
        assert_eq!(
            serde_json::to_string(&Command::Head { head }).unwrap(),
            r#"{"action":"head","x":0.0,"y":0.3,"z":0.1,"heading":0.0,"pitch":0.0}"#
        );
        // A truck whose speakers aren't known: the sound as it is.
        assert_eq!(
            serde_json::to_string(&Command::Speakers { speakers: None }).unwrap(),
            r#"{"action":"speakers","speakers":null}"#
        );
        let sound = |text: &str| serde_json::from_str::<Command>(text).unwrap().is_sound();
        assert!(!sound(r#"{"action":"head","x":1e30,"y":0,"z":0,"heading":0,"pitch":0}"#));
        // A reloaded page gets them again, before the video.
        let mut outbox = Outbox::default();
        outbox.ready();
        outbox.send(&speakers);
        outbox.send(&Command::Head { head });
        outbox.send(&Command::Head { head: Head { x: 0.1, ..head } });
        outbox.send(&Command::Load { video: "dQw4w9WgXcQ".into(), start: None });
        let again = outbox.ready();
        assert_eq!(again.len(), 3);
        assert!(again[0].contains("speakers") && again[1].contains(r#""x":0.1"#) && again[2].contains("load"));
    }

    #[test]
    fn passes_on_only_real_speeds_and_audio_tracks() {
        let report = r#"{"video":"0e3GPea1Tyg","title":"t","author":"a","live":false,"state":1,"time":1,"duration":2,
            "volume":3,"ad":false,"rates":[0.25,1,2,1e9],"rate":1.5,
            "audioTracks":[{"id":"251;Cg8KBWFjb250EgZkdWJiZWQKCgoEbGFuZxICYXI","name":"Arabic","original":false},
                {"id":"<script>","name":"x"}],"audioTrack":"251;Cg8KBWFjb250EgZkdWJiZWQKCgoEbGFuZxICYXI",
            "stableVolume":true,"voiceBoost":null}"#;
        let Some(Report::State(state)) = read_report(report) else { panic!("a state") };
        assert_eq!(state.rates, [0.25, 1.0, 2.0]);
        assert_eq!(
            (state.rate, state.audio_tracks.len(), state.stable_volume, state.voice_boost),
            (1.5, 1, Some(true), None)
        );
        let sound = |text: &str| serde_json::from_str::<Command>(text).unwrap().is_sound();
        assert!(sound(r#"{"action":"rate","rate":1.75}"#) && !sound(r#"{"action":"rate","rate":8}"#));
        assert!(sound(r#"{"action":"audioTrack","id":"251;Cg8KBWFjb250EgZkdWJiZWQKCgoEbGFuZxICYXI"}"#));
        assert!(!sound(r#"{"action":"audioTrack","id":"'); alert(1)"}"#));
        assert!(sound(r#"{"action":"stableVolume","on":false}"#) && sound(r#"{"action":"voiceBoost","on":true}"#));
    }

    #[test]
    fn passes_on_only_real_playlists() {
        let message = r#"{"answer":11,"playlists":[
            {"playlist":"PLrhhD0hhguUTrI2KtNh2pctWsQis_ZsNn","title":"ETS2 - Music","channel":"ProMods","channelId":"UCAcwxWm5d4gTton-uSs3ExA","thumbnail":"https://i.ytimg.com/vi/CKpymgMrwfA/hq720.jpg","count":"13 videos","after":2},
            {"playlist":"PL x","title":"Nope"},
            {"playlist":"PLOzDu-MXXLliO9fBNZOQTBDddoA3FzZUo","title":"lofi","thumbnail":"https://evil.example/a.png","channelId":"@x"}
        ],"playlist":{"playlist":"../VL","title":"x"}}"#;
        let Some(Report::Answer(answer)) = read_report(message) else { panic!("an answer") };
        let playlists = answer.playlists.as_ref().unwrap();
        assert_eq!(playlists.len(), 2);
        assert_eq!((playlists[0].after, playlists[0].count.as_str()), (2, "13 videos"));
        assert_eq!((playlists[1].thumbnail.as_deref(), playlists[1].channel_id.as_deref()), (None, None));
        assert_eq!(answer.playlist, None);
        let sound = |text: &str| serde_json::from_str::<Command>(text).unwrap().is_sound();
        assert!(sound(r#"{"action":"playlist","id":1,"playlist":"PLrhhD0hhguUTrI2KtNh2pctWsQis_ZsNn"}"#));
        assert!(sound(r#"{"action":"playlist","id":1,"playlist":"WL","more":"4qmFsgI"}"#));
        assert!(!sound(r#"{"action":"playlist","id":1,"playlist":"PL/../x"}"#));
        assert!(sound(r#"{"action":"search","id":2,"query":"ets2 music","filter":"playlists"}"#));
    }

    /// The sign-in window's life beside YouTube's page, on its profile: it opens on Google's sign-in, and
    /// closing it is told. Needs WebView2 and the network: `cargo test -p westplay-host -- --ignored sign_in`.
    #[test]
    #[ignore = "opens a window on the desktop"]
    fn opens_and_closes_the_sign_in_window() {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data = std::env::temp_dir().join(format!("westplay-sign-in-test-{}", std::process::id()));
        let stage = Stage::new(1280, 720).unwrap();
        let youtube = YouTube::start(&stage, 1280, 720, &user_data).unwrap();
        let pump = |seconds: u64, reports: &mut Vec<Report>| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                reports.extend(youtube.take_reports());
            }
        };
        let mut reports = Vec::new();
        pump(3, &mut reports);
        youtube.command(&Command::SignIn).unwrap();
        // Asked again while open: brought to the front, not opened twice.
        youtube.command(&Command::SignIn).unwrap();
        pump(8, &mut reports);
        // SAFETY: static strings; the window found (or none) is only posted to below.
        let window = unsafe { FindWindowW(windows::core::w!("WestPlaySignIn"), None) }.expect("the sign-in window");
        assert_eq!(reports.iter().filter(|r| **r == Report::SignInWindow(true)).count(), 1);
        assert_eq!(youtube.sign_in.borrow().as_ref().map(SignInWindow::progress), Some(Progress::Signing));
        // SAFETY: posting to a window of this test's, which may have closed already (then it's an error).
        unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap() };
        pump(2, &mut reports);
        assert!(reports.contains(&Report::SignInWindow(false)), "{reports:?}");
        assert!(youtube.sign_in.borrow().is_none());
        youtube.close(&stage);
        crate::pump_messages();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&user_data);
    }

    /// The real thing: YouTube answers the app's questions, in the language asked. Needs the network and
    /// WebView2: `cargo test -p westplay-host -- --ignored answers`.
    #[test]
    #[ignore = "asks youtube.com"]
    fn answers_searches_suggestions_and_up_next() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data = std::env::temp_dir().join("westplay-youtube-answers");
        let stage = Stage::new(1280, 720).unwrap();
        let youtube = YouTube::start(&stage, 1280, 720, &user_data).unwrap();
        youtube.command(&Command::Language { code: "tr".into() }).unwrap();
        youtube.command(&Command::Search { id: 1, query: "ets2 scania".into(), more: None, filter: None }).unwrap();
        youtube.command(&Command::Suggest { id: 2, query: "ets2 sca".into() }).unwrap();
        youtube.command(&Command::Related { id: 3, video: "kJQP7kiw5Fk".into() }).unwrap();
        youtube
            .command(&Command::Search {
                id: 5,
                query: "lofi girl".into(),
                more: None,
                filter: Some(SearchFilter::Channels),
            })
            .unwrap();
        youtube.command(&Command::Channel { id: 6, channel: "UCSJ4gkVC6NrvII8umztf0Ow".into(), more: None }).unwrap();
        youtube
            .command(&Command::Search { id: 7, query: "lofi hip hop playlist".into(), more: None, filter: None })
            .unwrap();
        youtube
            .command(&Command::Search {
                id: 8,
                query: "ets2 music".into(),
                more: None,
                filter: Some(SearchFilter::Playlists),
            })
            .unwrap();
        // YouTube's own "Popular Music Videos": over a hundred videos.
        youtube
            .command(&Command::Playlist { id: 9, playlist: "PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI".into(), more: None })
            .unwrap();
        let mut answers = std::collections::HashMap::new();
        let started = std::time::Instant::now();
        let until = |answers: &std::collections::HashMap<u32, Answer>, count| {
            answers.len() < count && started.elapsed() < std::time::Duration::from_secs(60)
        };
        let mut asked_more = false;
        let mut account = None;
        let mut asked_more_of_the_playlist = false;
        while until(&answers, 10) || (account.is_none() && started.elapsed() < std::time::Duration::from_secs(60)) {
            crate::pump_messages();
            std::thread::sleep(std::time::Duration::from_millis(20));
            for report in youtube.take_reports() {
                match report {
                    Report::Answer(answer) => {
                        println!(
                            "{:>5.1} s  answer {}: {}",
                            started.elapsed().as_secs_f32(),
                            answer.answer,
                            answer_message(&answer).chars().take(300).collect::<String>()
                        );
                        answers.insert(answer.answer, *answer);
                    }
                    Report::Account(told) => {
                        println!("{:>5.1} s  account: signed in {}", started.elapsed().as_secs_f32(), told.signed_in);
                        account = Some(told);
                    }
                    _ => {}
                }
            }
            // The search's next videos, once the search answered.
            if let (false, Some(more)) = (asked_more, answers.get(&1).and_then(|a| a.more.clone())) {
                youtube
                    .command(&Command::Search { id: 4, query: String::new(), more: Some(more), filter: None })
                    .unwrap();
                asked_more = true;
            }
            if let (false, Some(more)) = (asked_more_of_the_playlist, answers.get(&9).and_then(|a| a.more.clone())) {
                let playlist = "PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI".into();
                youtube.command(&Command::Playlist { id: 10, playlist, more: Some(more) }).unwrap();
                asked_more_of_the_playlist = true;
            }
        }
        let videos = |id| answers.get(&id).and_then(|a: &Answer| a.videos.clone()).unwrap_or_default();
        for (id, what) in [(1, "search"), (3, "Up next"), (4, "more of the search")] {
            let found = videos(id);
            assert!(found.len() >= 5, "{what}: {} videos", found.len());
            assert!(found.iter().all(|v| !v.title.is_empty() && v.thumbnail.is_some()), "{what}: {found:?}");
            assert!(
                found.iter().filter(|v| !v.channel.is_empty() && v.avatar.is_some()).count() >= found.len() / 2,
                "{what}"
            );
        }
        let suggestions = answers.get(&2).and_then(|a| a.suggestions.clone()).unwrap_or_default();
        assert!(suggestions.iter().any(|s| s.starts_with("ets2 sca")), "{suggestions:?}");
        // Channels: found by name, and a channel's page with its videos; videos know their channel.
        let found = answers.get(&5).and_then(|a| a.channels.clone()).unwrap_or_default();
        assert!(found.iter().any(|c| c.channel == "UCSJ4gkVC6NrvII8umztf0Ow"), "{found:?}");
        let page = answers.get(&6).expect("the channel's page");
        let channel = page.channel.as_ref().expect("the channel's header");
        println!("channel: {channel:?}");
        assert_eq!(channel.name, "Lofi Girl");
        assert!(channel.handle.starts_with('@') && !channel.subscribers.is_empty() && channel.banner.is_some());
        // Signed out: nothing said about subscribing (no Subscribe button).
        assert_eq!(channel.subscribed, None);
        assert!(page.videos.as_ref().is_some_and(|v| v.len() >= 10) && page.more.is_some());
        assert!(videos(1).iter().chain(&videos(3)).filter(|v| v.channel_id.is_some()).count() >= 5);
        // Playlists: among a plain search's videos, on their own with YouTube's filter, and a playlist's page with
        // its videos a page at a time.
        let found = |id| answers.get(&id).and_then(|a: &Answer| a.playlists.clone()).unwrap_or_default();
        println!(
            "playlists in a search: {:?}",
            found(7).iter().map(|p| (&p.title, &p.count, p.after)).collect::<Vec<_>>()
        );
        assert!(!found(7).is_empty() && found(8).len() >= 5, "{:?} {:?}", found(7), found(8));
        let whole = |p: &Playlist| {
            !p.title.is_empty() && p.thumbnail.is_some() && !p.count.is_empty() && !p.playlist.starts_with("RD")
        };
        assert!(found(8).iter().all(whole), "{:?}", found(8));
        let page = answers.get(&9).expect("the playlist's page");
        let playlist = page.playlist.as_ref().expect("the playlist's header");
        println!("playlist: {playlist:?}");
        assert!(!playlist.title.is_empty() && !playlist.count.is_empty() && playlist.thumbnail.is_some());
        assert_eq!(page.videos.as_ref().map(Vec::len), Some(100), "a playlist page's hundred");
        assert!(page.more.is_some());
        assert!(videos(10).len() >= 20, "the playlist's next videos: {}", videos(10).len());
        assert!(
            videos(9).iter().all(|v| !v.title.is_empty() && v.thumbnail.is_some() && !v.channel.is_empty()),
            "{:?}",
            videos(9)
        );
        // A new profile isn't signed in, and says so (with no picture).
        assert_eq!(account, Some(Account { signed_in: false, avatar: None }));
        youtube.close(&stage);
    }

    /// The real thing: captions in the language chosen before a video, then none, and the quality held to
    /// what's chosen, then YouTube's own again, as YouTube's player reports them. Needs the network and
    /// WebView2: `cargo test -p westplay-host -- --ignored keeps_captions`.
    #[test]
    #[ignore = "plays a video from youtube.com"]
    fn keeps_captions_and_quality_to_whats_chosen() {
        // Rick Astley's video: its own captions in five languages, and 144p to 2160p.
        const VIDEO: &str = "dQw4w9WgXcQ";
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data = std::env::temp_dir().join(format!("westplay-youtube-captions-{}", std::process::id()));
        let stage = Stage::new(1280, 720).unwrap();
        let youtube = YouTube::start(&stage, 1280, 720, &user_data).unwrap();
        youtube.command(&Command::Volume { percent: 0 }).unwrap();
        // Chosen on an earlier video (Japanese, automatic): this video's own Japanese captions, by language.
        youtube.command(&Command::Captions { track: Some("a.ja".into()) }).unwrap();
        youtube.command(&Command::Quality { quality: "medium".into() }).unwrap();
        youtube.command(&Command::Load { video: VIDEO.into(), start: None }).unwrap();
        let started = std::time::Instant::now();
        let mut last: Option<State> = None;
        // Pumps until `done` holds of the player's state (or panics after `seconds`).
        let mut until = |video: &str, what: &str, seconds: u64, done: &dyn Fn(&State) -> bool| {
            let from = std::time::Instant::now();
            loop {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in youtube.take_reports() {
                    if let Report::State(state) = report {
                        let seen = (state.state, state.caption.clone(), state.quality_label.clone());
                        if last.as_ref().map(|l| (l.state, l.caption.clone(), l.quality_label.clone())) != Some(seen) {
                            println!(
                                "{:>5.1} s  state {} caption {:?} quality {} ({}) of {:?}",
                                started.elapsed().as_secs_f32(),
                                state.state,
                                state.caption,
                                state.quality_label,
                                state.quality,
                                state.qualities.iter().map(|q| q.label.as_str()).collect::<Vec<_>>()
                            );
                        }
                        last = Some(*state);
                    }
                }
                if last.as_ref().is_some_and(|state| state.video.as_deref() == Some(video) && done(state)) {
                    return last.clone().unwrap();
                }
                assert!(from.elapsed() < std::time::Duration::from_secs(seconds), "{what}: {last:?}");
            }
        };
        let state = until(VIDEO, "playing in Japanese at 360p", 45, &|s| {
            s.state == 1 && s.time > 2.0 && s.quality_label == "360p"
        });
        assert_eq!(state.caption.as_deref(), Some(".ja"));
        assert!(state.captions.iter().any(|t| t.track == ".en" && !t.name.is_empty()), "{:?}", state.captions);
        assert!(state.qualities.len() >= 6 && state.qualities[0].quality == "hd2160", "{:?}", state.qualities);
        youtube.command(&Command::Captions { track: None }).unwrap();
        youtube.command(&Command::Quality { quality: "hd720".into() }).unwrap();
        until(VIDEO, "no captions, at 720p", 20, &|s| s.caption.is_none() && s.quality_label.starts_with("720p"));
        youtube.command(&Command::Quality { quality: "auto".into() }).unwrap();
        until(VIDEO, "YouTube's own quality", 10, &|s| s.quality == "auto");
        // A video with only YouTube's automatic captions (a Turkish ETS2 video): those, picked in Turkish.
        const AUTOMATIC: &str = "s8YcZxzPxLs";
        youtube.command(&Command::Captions { track: Some(".tr".into()) }).unwrap();
        youtube.command(&Command::Load { video: AUTOMATIC.into(), start: Some(30.0) }).unwrap();
        let state = until(AUTOMATIC, "automatic Turkish captions", 30, &|s| s.state == 1 && s.caption.is_some());
        assert_eq!(state.caption.as_deref(), Some("a.tr"));
        youtube.close(&stage);
        crate::pump_messages();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&user_data);
    }

    /// The real thing: the gear's settings on a video with dubbed audio (MrBeast's): its speeds and audio
    /// tracks told, a speed and another track taken, and the speed kept on the next video. Muted. Needs the
    /// network and WebView2: `cargo test -p westplay-host -- --ignored keeps_speed_and_audio_track`.
    #[test]
    #[ignore = "plays videos from youtube.com"]
    fn keeps_speed_and_audio_track() {
        const DUBBED: &str = "0e3GPea1Tyg";
        const NEXT: &str = "dQw4w9WgXcQ";
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data = std::env::temp_dir().join(format!("westplay-youtube-gear-{}", std::process::id()));
        let stage = Stage::new(1280, 720).unwrap();
        let youtube = YouTube::start(&stage, 1280, 720, &user_data).unwrap();
        youtube.view.set_muted(true).unwrap();
        let mut last: Option<State> = None;
        let mut until = |video: &str, what: &str, seconds: u64, done: &dyn Fn(&State) -> bool| {
            let from = std::time::Instant::now();
            loop {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in youtube.take_reports() {
                    if let Report::State(state) = report {
                        last = Some(*state);
                    }
                }
                if last.as_ref().is_some_and(|s| s.video.as_deref() == Some(video) && done(s)) {
                    return last.clone().unwrap();
                }
                assert!(from.elapsed() < std::time::Duration::from_secs(seconds), "{what}: {last:?}");
            }
        };
        youtube.command(&Command::Load { video: DUBBED.into(), start: Some(30.0) }).unwrap();
        let state = until(DUBBED, "playing with its tracks", 45, &|s| s.state == 1 && s.audio_tracks.len() > 1);
        println!(
            "rates {:?}, tracks {:?}, playing {:?}",
            state.rates,
            state.audio_tracks.iter().map(|t| &t.name).collect::<Vec<_>>(),
            state.audio_track
        );
        assert!(state.rates.contains(&1.5) && state.audio_track.is_some());
        let other = state.audio_tracks.iter().find(|t| Some(&t.id) != state.audio_track.as_ref()).unwrap().clone();
        youtube.command(&Command::Rate { rate: 1.5 }).unwrap();
        youtube.command(&Command::AudioTrack { id: other.id.clone() }).unwrap();
        let state = until(DUBBED, "at 1.5x, in the other track", 20, &|s| {
            s.rate == 1.5 && s.audio_track.as_deref() == Some(other.id.as_str())
        });
        println!("now {} at {}", other.name, state.rate);
        // The next video: still 1.5x.
        youtube.command(&Command::Load { video: NEXT.into(), start: None }).unwrap();
        until(NEXT, "the next video at 1.5x", 45, &|s| s.state == 1 && s.rate == 1.5);
        youtube.close(&stage);
        crate::pump_messages();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&user_data);
    }

    /// The real thing: a music video's sound through the Scania S's speakers, heard from the driver's head:
    /// it plays through them, without the limiter at work from the seat (a loud master: Daft Punk's "Get
    /// Lucky"), louder in the ear on the side the head leans to, at the player's volume; muffled outside; and
    /// as it is in a truck whose speakers aren't known. Muted (what's measured is the page's own sound). Needs
    /// the network and WebView2: `cargo test -p westplay-host -- --ignored through_the_trucks_speakers`.
    #[test]
    #[ignore = "plays a video from youtube.com"]
    fn plays_through_the_trucks_speakers() {
        const VIDEO: &str = "5NV6Rdv1a3I";
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data = std::env::temp_dir().join(format!("westplay-youtube-speakers-{}", std::process::id()));
        let stage = Stage::new(1280, 720).unwrap();
        let youtube = YouTube::start(&stage, 1280, 720, &user_data).unwrap();
        youtube.view.set_muted(true).unwrap();
        // The S's driver, as the game has it.
        let head = Head { x: -0.683, y: 1.41, z: 0.384, heading: 0.0, pitch: 0.0 };
        youtube
            .cab(speakers::in_truck("vehicle.scania.s_2016"), head, false, 0.0, Equalizer::default(), false)
            .unwrap();
        youtube.command(&Command::Volume { percent: 100 }).unwrap();
        youtube.command(&Command::Load { video: VIDEO.into(), start: Some(40.0) }).unwrap();
        // The ears' loudness over `seconds` of playing, averaged (`None`: no speakers' sound told).
        let listen = |seconds: u64| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
            let mut heard = Vec::new();
            let mut none = 0;
            while std::time::Instant::now() < until {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in youtube.take_reports() {
                    if let Report::State(state) = report
                        && state.video.as_deref() == Some(VIDEO)
                        && state.state == 1
                    {
                        match state.cab_level {
                            Some(ears) => heard.push(ears),
                            None => none += 1,
                        }
                    }
                }
            }
            let average = |at: usize| heard.iter().map(|e| e[at]).sum::<f32>() / heard.len().max(1) as f32;
            let most = heard.iter().map(|e| e[2]).fold(0.0f32, f32::min);
            let ears = (!heard.is_empty()).then(|| [average(0), average(1), average(2)]);
            println!("  ears {ears:?}, the limiter at most {most:.1} dB ({} reports, {none} without)", heard.len());
            ears
        };
        // Started, then heard from the driver's seat.
        let started = std::time::Instant::now();
        while started.elapsed() < std::time::Duration::from_secs(45) && listen(1).is_none() {}
        let seat = listen(4).expect("the speakers' sound");
        assert!(seat[0] > 0.01 && seat[1] > 0.01, "silent: {seat:?}");
        assert!(seat[2] > -1.0, "the limiter at work from the seat: {seat:?}");
        // Leaning far right, by the right door: the right ear hears more; far left, the left one.
        youtube
            .cab(
                speakers::in_truck("vehicle.scania.s_2016"),
                Head { x: 0.95, y: 0.8, z: -0.2, ..head },
                false,
                0.0,
                Equalizer::default(),
                false,
            )
            .unwrap();
        listen(1);
        let right = listen(4).unwrap();
        assert!(right[1] > right[0] * 1.3, "leaning right: {right:?}");
        youtube
            .cab(
                speakers::in_truck("vehicle.scania.s_2016"),
                Head { x: -0.95, y: 0.8, z: -0.2, ..head },
                false,
                0.0,
                Equalizer::default(),
                false,
            )
            .unwrap();
        listen(1);
        let left = listen(4).unwrap();
        assert!(left[0] > left[1] * 1.3, "leaning left: {left:?}");
        // The camera outside the cab: muffled, quieter; back in, as before.
        youtube
            .cab(
                speakers::in_truck("vehicle.scania.s_2016"),
                Head { x: -0.95, y: 0.8, z: -0.2, ..head },
                true,
                0.0,
                Equalizer::default(),
                false,
            )
            .unwrap();
        listen(1);
        let outside = listen(4).unwrap();
        assert!(outside[0] + outside[1] < (left[0] + left[1]) * 0.6, "outside: {outside:?} after {left:?}");
        // Further off: quieter still.
        youtube
            .cab(
                speakers::in_truck("vehicle.scania.s_2016"),
                Head { x: -0.95, y: 0.8, z: -0.2, ..head },
                true,
                20.0,
                Equalizer::default(),
                false,
            )
            .unwrap();
        listen(1);
        let far = listen(4).unwrap();
        assert!(far[0] + far[1] < (outside[0] + outside[1]) * 0.5, "20 m out: {far:?} after {outside:?}");
        youtube
            .cab(
                speakers::in_truck("vehicle.scania.s_2016"),
                Head { x: -0.95, y: 0.8, z: -0.2, ..head },
                false,
                0.0,
                Equalizer::default(),
                false,
            )
            .unwrap();
        listen(1);
        // The player's volume still holds.
        youtube.command(&Command::Volume { percent: 10 }).unwrap();
        listen(1);
        let quiet = listen(4).unwrap();
        assert!(quiet[0] + quiet[1] < (left[0] + left[1]) * 0.4, "at 10 %: {quiet:?} after {left:?}");
        // A truck whose speakers aren't known: the sound as it is (no speakers' meter).
        youtube.cab(speakers::in_truck("vehicle.man.tgx_2020"), head, false, 0.0, Equalizer::default(), false).unwrap();
        listen(1);
        assert_eq!(listen(3), None);
        youtube.close(&stage);
        crate::pump_messages();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&user_data);
    }

    /// The real thing: YouTube in a hidden WebView plays a music video muted, with no ad, and without the
    /// wait YouTube puts before it for the ad it carries (15 s: the video held back some 12 s). Needs the
    /// network and WebView2: `cargo test -p westplay-host -- --ignored plays_a_video`.
    #[test]
    #[ignore = "plays a video from youtube.com"]
    fn plays_a_video_with_no_ad_in_a_hidden_webview() {
        // Ed Sheeran's "Perfect", with an unskippable 15 s ad.
        const VIDEO: &str = "2Vv-BfVoq4g";
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        // A new profile, as a player's first YouTube is (its home page reloads itself once).
        let user_data = std::env::temp_dir().join(format!("westplay-youtube-test-{}", std::process::id()));
        let stage = Stage::new(1280, 720).unwrap();
        let youtube = YouTube::start(&stage, 1280, 720, &user_data).unwrap();
        youtube.command(&Command::Volume { percent: 0 }).unwrap();
        youtube.command(&Command::Load { video: VIDEO.into(), start: None }).unwrap();
        let started = std::time::Instant::now();
        let mut playing = None;
        let mut ads = false;
        let mut last = None;
        // Why it hadn't started yet, as the page tells it while it loads (for the log).
        let mut diagnosed = None;
        // When the player first had the video: the wait is counted from there.
        let mut loaded = None;
        while started.elapsed() < std::time::Duration::from_secs(45) && playing.is_none() {
            crate::pump_messages();
            std::thread::sleep(std::time::Duration::from_millis(20));
            for state in youtube.take_reports().into_iter().filter_map(|report| match report {
                Report::State(state) => Some(state),
                _ => None,
            }) {
                ads |= state.ad;
                if let Some(diag) = &state.diag {
                    diagnosed = Some(diag.to_string());
                }
                let seen = (state.video.clone(), state.title.clone(), state.state);
                if last.as_ref().map(|l: &State| (l.video.clone(), l.title.clone(), l.state)) != Some(seen) {
                    println!(
                        "{:>5.1} s  {:?} {:?} state {}",
                        started.elapsed().as_secs_f32(),
                        state.video,
                        state.title,
                        state.state
                    );
                }
                last = Some(*state.clone());
                let ours = state.video.as_deref() == Some(VIDEO) && state.title.contains("Perfect");
                if ours && loaded.is_none() {
                    loaded = Some(started.elapsed());
                }
                if ours && state.state == 1 && state.time > 1.0 {
                    playing = Some((started.elapsed(), state));
                }
            }
        }
        let (after, state) = playing.unwrap_or_else(|| panic!("not playing within 45 s; last: {last:?}"));
        let waited = after - loaded.unwrap();
        println!(
            "playing after {:.1} s ({:.1} s after it loaded): {}",
            after.as_secs_f32(),
            waited.as_secs_f32(),
            state.title
        );
        assert!(!ads, "an ad showed");
        // Told while it loaded, as the log would have it.
        println!("while loading: {}", diagnosed.as_deref().unwrap_or("nothing"));
        assert!(diagnosed.is_some_and(|diag| diag.starts_with("playability OK")), "no diagnosis while loading");
        assert!(state.diag.is_none(), "a diagnosis while playing");
        assert!(waited < std::time::Duration::from_secs(10), "held back for the ad: {waited:?}");
        assert_eq!(state.volume, 0.0);
        youtube.close(&stage);
        // WebView2's processes let go of the profile shortly after.
        crate::pump_messages();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&user_data);
    }

    /// YouTube's cookie consent answered before its first page, as Reject all answers it, and kept: a new
    /// profile has `SOCS` `CAE=` once YouTube's page has loaded (in the EU, its consent page would stand in for
    /// it otherwise; from elsewhere YouTube never asks, so this sees the answer kept, not the page skipped).
    #[test]
    #[ignore = "asks youtube.com"]
    fn answers_youtubes_cookie_consent_before_it_asks() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data = std::env::temp_dir().join(format!("westplay-youtube-consent-{}", std::process::id()));
        let stage = Stage::new(1280, 720).unwrap();
        let youtube = YouTube::start(&stage, 1280, 720, &user_data).unwrap();
        let started = std::time::Instant::now();
        let mut cookie = String::new();
        while started.elapsed() < std::time::Duration::from_secs(30) {
            crate::pump_messages();
            std::thread::sleep(std::time::Duration::from_millis(200));
            let said = youtube
                .view
                .evaluate(
                    "location.hostname + ' ' + (document.cookie.split('; ').find((c) => c.startsWith('SOCS=')) ?? '')",
                )
                .unwrap_or_default();
            if said.contains("www.youtube.com") && started.elapsed() > std::time::Duration::from_secs(8) {
                cookie = said;
                break;
            }
        }
        println!("{cookie}");
        youtube.close(&stage);
        crate::pump_messages();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&user_data);
        assert!(cookie.contains("SOCS=CAE="), "the consent's answer: {cookie}");
    }

    /// A video YouTube warns about before it plays ("this video may contain disturbing images", I understand
    /// and want to continue): plays all the same, as the player's own continue asks for it. A song's video a
    /// player couldn't play (owner 2026-10-27): Sagopa Kajmer's "Merhametine Dön (2021 Tek Edit)", or
    /// WESTPLAY_YOUTUBE_WARNED.
    #[test]
    #[ignore = "plays a video from youtube.com"]
    fn plays_a_video_youtube_warns_about() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let video = std::env::var("WESTPLAY_YOUTUBE_WARNED").unwrap_or_else(|_| "kzOXY8Kj_xc".to_owned());
        let user_data = std::env::temp_dir().join(format!("westplay-youtube-warned-{}", std::process::id()));
        let stage = Stage::new(1280, 720).unwrap();
        let youtube = YouTube::start(&stage, 1280, 720, &user_data).unwrap();
        youtube.command(&Command::Volume { percent: 0 }).unwrap();
        youtube.command(&Command::Load { video: video.clone(), start: None }).unwrap();
        let started = std::time::Instant::now();
        let mut playing = false;
        let mut diagnosed = None;
        while started.elapsed() < std::time::Duration::from_secs(45) && !playing {
            crate::pump_messages();
            std::thread::sleep(std::time::Duration::from_millis(20));
            for report in youtube.take_reports() {
                if let Report::State(state) = report {
                    if let Some(diag) = &state.diag {
                        diagnosed = Some(diag.to_string());
                    }
                    playing |= state.video.as_deref() == Some(video.as_str()) && state.state == 1 && state.time > 1.0;
                }
            }
        }
        println!("after {:.1} s: playing {playing}; while loading: {diagnosed:?}", started.elapsed().as_secs_f32());
        youtube.close(&stage);
        crate::pump_messages();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&user_data);
        assert!(playing, "not playing within 45 s");
    }

    /// The real thing: the picture over the whole page on the first video and on the next one (players, 2026-10-27:
    /// the next video shrank to the page's top left), each video element's box, styles and the player's said.
    /// Muted. Needs the network and WebView2: `cargo test -p westplay-host -- --ignored fills_the_page_on_the_next`.
    #[test]
    #[ignore = "plays videos from youtube.com"]
    fn fills_the_page_on_the_next_video() {
        const FIRST: &str = "dQw4w9WgXcQ";
        const NEXT: &str = "5NV6Rdv1a3I";
        const LOOK: &str = r#"JSON.stringify({
            sheet: [...document.querySelectorAll('style')].some((s) => s.textContent.includes('westplay-crop')),
            player: (() => { const p = document.getElementById('movie_player'); if (!p) return null;
                const r = p.getBoundingClientRect(); return { box: [r.x, r.y, r.width, r.height], class: p.className }; })(),
            videos: [...document.querySelectorAll('video')].map((v) => { const r = v.getBoundingClientRect();
                const c = getComputedStyle(v); return { box: [r.x, r.y, r.width, r.height], class: v.className,
                inline: v.getAttribute('style'), position: c.position, transform: c.transform, fit: c.objectFit,
                parent: v.parentElement?.className, src: !!v.src, paused: v.paused }; }),
            path: location.pathname,
            miniplayer: !!document.querySelector('ytd-miniplayer[active], ytd-miniplayer[enabled]'),
            // Around the player: what makes `position: fixed` its box's instead of the page's.
            holders: (() => { const out = []; let e = document.getElementById('movie_player')?.parentElement;
                while (e && e !== document.documentElement) { const c = getComputedStyle(e);
                    if (c.transform !== 'none' || c.contain !== 'none' || c.filter !== 'none' || c.willChange !== 'auto')
                        out.push({ tag: e.tagName, id: e.id, class: e.className, transform: c.transform, contain: c.contain,
                            filter: c.filter, willChange: c.willChange });
                    e = e.parentElement; } return out; })(),
        })"#;
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let user_data = std::env::temp_dir().join(format!("westplay-youtube-next-{}", std::process::id()));
        let stage = Stage::new(1280, 720).unwrap();
        let youtube = YouTube::start(&stage, 1280, 720, &user_data).unwrap();
        youtube.command(&Command::Volume { percent: 0 }).unwrap();
        let mut last: Option<State> = None;
        let mut until = |video: &str, seconds: u64| {
            let from = std::time::Instant::now();
            loop {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
                for report in youtube.take_reports() {
                    if let Report::State(state) = report {
                        last = Some(*state);
                    }
                }
                if last.as_ref().is_some_and(|s| s.video.as_deref() == Some(video) && s.state == 1 && s.time > 3.0) {
                    let state = last.clone().unwrap();
                    assert_eq!(state.picture, None, "{video}: said not over the page while it is");
                    return state;
                }
                assert!(from.elapsed() < std::time::Duration::from_secs(seconds), "{video}: {last:?}");
            }
        };
        let look = |what: &str| {
            let seen = std::rc::Rc::new(std::cell::RefCell::new(None));
            let into = seen.clone();
            youtube.view.run_script(LOOK, move |json| *into.borrow_mut() = Some(json)).unwrap();
            while seen.borrow().is_none() {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            let json = seen.borrow_mut().take().unwrap();
            // run_script's answer is the script's value as JSON: here a string of JSON.
            let said: String = serde_json::from_str(&json).unwrap();
            println!("{what}: {said}");
            serde_json::from_str::<serde_json::Value>(&said).unwrap()
        };
        let fills = |seen: &serde_json::Value| {
            seen["videos"].as_array().unwrap().iter().filter(|v| v["src"] == true).all(|v| {
                let b: Vec<f64> = v["box"].as_array().unwrap().iter().map(|n| n.as_f64().unwrap()).collect();
                b[0].abs() < 1.0 && b[1].abs() < 1.0 && (b[2] - 1280.0).abs() < 1.0 && (b[3] - 720.0).abs() < 1.0
            })
        };
        let wait = |seconds: u64| {
            let from = std::time::Instant::now();
            while from.elapsed() < std::time::Duration::from_secs(seconds) {
                crate::pump_messages();
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        let show = |width: u32, height: u32| {
            youtube.command(&Command::Show { x: 0.0, y: 0.0, width, height, crop: false }).unwrap();
        };
        // The full-screen player's: the page at the stage's size.
        show(1280, 720);
        youtube.command(&Command::Load { video: FIRST.into(), start: None }).unwrap();
        until(FIRST, 45);
        let first = look("the first video");
        youtube.command(&Command::Load { video: NEXT.into(), start: None }).unwrap();
        until(NEXT, 45);
        let next = look("the next video");
        // To the Dashboard's Now Playing (the page scaled into a small box) and back, then the next.
        show(400, 225);
        wait(2);
        show(1280, 720);
        youtube.command(&Command::Load { video: FIRST.into(), start: None }).unwrap();
        until(FIRST, 45);
        let back = look("after Now Playing, the next video");
        // Skipped on before each starts, then one played.
        for video in ["kJQP7kiw5Fk", "9bZkp7q19f0"] {
            youtube.command(&Command::Load { video: video.into(), start: None }).unwrap();
            wait(1);
        }
        youtube.command(&Command::Load { video: NEXT.into(), start: None }).unwrap();
        until(NEXT, 45);
        let skipped = look("skipped through, the next video");
        // One played to its end, YouTube's own end screen and all, then the next.
        youtube.command(&Command::Seek { seconds: 10_000.0 }).unwrap();
        wait(6);
        look("at the end of a video");
        youtube.command(&Command::Load { video: FIRST.into(), start: None }).unwrap();
        until(FIRST, 45);
        let ended = look("after one ended, the next video");
        // The picture made small in the top left as the players saw it: the page says so.
        let small = "document.head.appendChild(Object.assign(document.createElement('style'), { textContent: \
            '#movie_player video { width: 640px !important; height: 360px !important; }' })); true";
        youtube.view.run_script(small, |_| {}).unwrap();
        let from = std::time::Instant::now();
        let said = loop {
            crate::pump_messages();
            std::thread::sleep(std::time::Duration::from_millis(20));
            let picture = youtube.take_reports().into_iter().find_map(|report| match report {
                Report::State(state) => state.picture,
                _ => None,
            });
            if let Some(picture) = picture {
                break picture;
            }
            assert!(from.elapsed() < std::time::Duration::from_secs(10), "a small picture not said");
        };
        println!("said small: {said}");
        assert!(said.contains("640x360"), "{said}");
        youtube.close(&stage);
        crate::pump_messages();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&user_data);
        for (seen, what) in
            [(&first, "first"), (&next, "next"), (&back, "back"), (&skipped, "skipped"), (&ended, "ended")]
        {
            assert!(fills(seen), "{what}: the video doesn't fill the page");
        }
    }
}
