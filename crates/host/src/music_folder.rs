//! Player (ui/src/cabin/apps/player): the game's own music folder (`<the game's Documents folder>/music`, one for
//! all its profiles: the songs the player put there for the game's radio), its songs listed with what their
//! tags say (a song without tags: its file's name), and played in a WebView of its own, hidden under the cabin
//! page, its sound through the truck's own speakers as the music services' is (`cab_sound.js`).
//!
//! Listed off the host's thread (reading tags takes a while with many songs), again whenever the folder
//! changes. Covers come from the songs' tags, kept in WestPlay's data folder for the cab to show
//! (`https://westplay.music-covers/`); the songs themselves are served to the player's page from the folder as
//! it is (`https://westplay.music/`). Nothing is copied, moved or written in the game's folder.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, PictureType};
use lofty::tag::Accessor;
use westplay_setup::game::Kind;
use westplay_setup::places;

use crate::speakers::{self, Equalizer, Head, Speaker};
use crate::webview::{self, OffscreenWebView, Stage};

/// Where the songs are served to the player's page.
pub const MUSIC_HOST: &str = "westplay.music";
/// Where their covers are served to the cabin page.
pub const COVERS_HOST: &str = "westplay.music-covers";
/// The player's own page.
const PAGE_HOST: &str = "westplay.player";

/// The kinds of file Player plays (the browser's own: MP3, Ogg Vorbis and Opus, FLAC, WAV, AAC), by extension.
const KINDS: [&str; 8] = ["mp3", "ogg", "oga", "opus", "flac", "wav", "m4a", "aac"];

/// How often the folder is looked at for songs added, removed or changed.
const LOOK_EVERY: Duration = Duration::from_secs(3);

/// The game `kind`'s music folder (it may not be there: the game makes it only when asked to).
pub fn folder_of(kind: Kind) -> Option<PathBuf> {
    Some(places::game_documents(kind)?.join("music"))
}

/// A song of the folder: its file's name, and what its tags say (none of them on a file without tags): its title,
/// its artist, how long it is (seconds; 0: not known) and its cover (a file of the covers' folder).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    pub seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
}

/// Whether `name` is a file Player plays (by its extension, in any case).
fn plays(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| KINDS.iter().any(|kind| kind.eq_ignore_ascii_case(ext)))
}

/// A folder's songs as they are on disk: each file's name, size and when it was written (to see a change).
type Files = Vec<(String, u64, Option<SystemTime>)>;

/// The folder's songs, as they are on disk now.
fn look(folder: &Path) -> Files {
    let mut files: Vec<_> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let meta = entry.metadata().ok()?;
            plays(&name).then(|| (name, meta.len(), meta.modified().ok()))
        })
        .collect();
    files.sort_by(|a, b| natural(&a.0, &b.0));
    files
}

/// File names in the order Explorer lists them: case left aside, and numbers by their value (`2 x` before `10 x`).
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let number = |chars: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = chars.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        chars.next();
                    }
                    digits
                };
                let (x, y) = (number(&mut a), number(&mut b));
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

/// A song's tags (none that can't be read: its file's name stands), its cover kept in `covers` (named by its
/// bytes, so a cover shared by an album's songs is kept once).
fn read_song(folder: &Path, file: &str, covers: &Path, kept: &mut BTreeSet<String>) -> Song {
    let mut song = Song { file: file.to_owned(), title: None, artist: None, seconds: 0.0, cover: None };
    let Ok(tagged) = lofty::read_from_path(folder.join(file)) else { return song };
    song.seconds = tagged.properties().duration().as_secs_f64();
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else { return song };
    let text = |value: Option<std::borrow::Cow<str>>| value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    song.title = text(tag.title());
    song.artist = text(tag.artist());
    let pictures = tag.pictures();
    let picture = pictures
        .iter()
        .find(|picture| picture.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures.first());
    if let Some(picture) = picture {
        let extension = match picture.mime_type() {
            Some(MimeType::Png) => Some("png"),
            Some(MimeType::Jpeg) => Some("jpg"),
            Some(MimeType::Gif) => Some("gif"),
            Some(MimeType::Bmp) => Some("bmp"),
            _ => None,
        };
        if let Some(extension) = extension {
            let name = format!("{:016x}.{extension}", fnv(picture.data()));
            let path = covers.join(&name);
            if path.is_file() || std::fs::write(&path, picture.data()).is_ok() {
                kept.insert(name.clone());
                song.cover = Some(name);
            }
        }
    }
    song
}

/// FNV-1a, 64-bit: a cover's name from its bytes.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3))
}

/// The folder's songs with their tags (covers kept in `covers`, the ones no song has any more removed).
pub fn read(folder: &Path, covers: &Path) -> Vec<Song> {
    let _ = std::fs::create_dir_all(covers);
    let mut kept = BTreeSet::new();
    let songs = look(folder).iter().map(|(file, _, _)| read_song(folder, file, covers, &mut kept)).collect();
    for entry in std::fs::read_dir(covers).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !kept.contains(&name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    songs
}

/// `{"type":"musicFolder","folder":"…","songs":[…]}` for the cabin page: where the folder is (`null`: the game's
/// folder isn't known) and its songs.
pub fn folder_message(folder: Option<&Path>, songs: &[Song]) -> String {
    serde_json::json!({
        "type": "musicFolder",
        "folder": folder.map(|folder| folder.display().to_string()),
        "songs": songs,
    })
    .to_string()
}

/// The folder, watched off the host's thread: listed for the game the player plays (`game`), and again as its
/// songs change.
pub struct FolderWatch {
    games: mpsc::Sender<Kind>,
    listed: mpsc::Receiver<(Option<PathBuf>, String)>,
}

impl FolderWatch {
    /// Watches ETS2's folder until the host says another game plays; covers kept in `covers`.
    pub fn start(covers: PathBuf) -> Self {
        let (games, game_changes) = mpsc::channel::<Kind>();
        let (lists, listed) = mpsc::channel();
        let _ = std::thread::Builder::new().name("westplay-music-folder".into()).spawn(move || {
            let mut kind = Kind::Ets2;
            let mut folder = folder_of(kind);
            let mut seen: Option<(Option<PathBuf>, Files)> = None;
            loop {
                loop {
                    match game_changes.try_recv() {
                        Ok(played) if played != kind => {
                            kind = played;
                            folder = folder_of(kind);
                        }
                        Ok(_) => {}
                        Err(mpsc::TryRecvError::Empty) => break,
                        // Its handle gone (the host's run ended): so does the watch (`worker`).
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                let now = folder.as_deref().map(look).unwrap_or_default();
                if seen.as_ref() != Some(&(folder.clone(), now.clone())) {
                    let songs = folder.as_deref().map(|dir| read(dir, &covers)).unwrap_or_default();
                    log!("[player] {} songs in {:?}", songs.len(), folder.as_deref().map(Path::display));
                    if lists.send((folder.clone(), folder_message(folder.as_deref(), &songs))).is_err() {
                        return;
                    }
                    seen = Some((folder.clone(), now));
                }
                std::thread::sleep(LOOK_EVERY);
            }
        });
        Self { games, listed }
    }

    /// The game the player plays now: its folder is the one listed.
    pub fn game(&self, kind: Kind) {
        let _ = self.games.send(kind);
    }

    /// The folder and its songs (the message for the page), as last listed since the last call.
    pub fn take(&self) -> Option<(Option<PathBuf>, String)> {
        self.listed.try_iter().last()
    }
}

/// What the cabin page asks of the player (`player` in ui/src/shared/host.ts).
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "action", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Command {
    /// Play `files` (the folder's, by name, in its order) from the one at `index` (shuffled, from any if none);
    /// with none, play on.
    Play {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        files: Option<Vec<String>>,
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
    /// The player's volume, 0 to 100.
    Volume {
        percent: f64,
    },
    Shuffle {
        on: bool,
    },
    Repeat {
        mode: Repeat,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Repeat {
    Off,
    All,
    One,
}

impl Command {
    /// Only songs of the folder pass: names, never paths (nothing outside the folder is served anyway).
    fn checked(&self) -> Self {
        match self {
            Command::Play { files: Some(files), index, shuffle } => Command::Play {
                files: Some(files.iter().filter(|file| is_name(file)).cloned().collect()),
                index: *index,
                shuffle: *shuffle,
            },
            other => other.clone(),
        }
    }
}

/// A file's name alone: no folder in it, nor a way up out of the folder.
fn is_name(file: &str) -> bool {
    !file.is_empty() && !file.contains(['/', '\\', ':']) && file != "." && file != ".."
}

/// What plays: the song (its file's name), playing or not (and loading: asked to play, its sound not come yet),
/// where (seconds), how long, the options, the volume (0 to 1), and where it is in the songs playing.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playback {
    pub file: String,
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
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub count: u32,
}

/// `{"type":"playerPlayback","playback":{…}}` for the cabin page (`null`: nothing plays).
pub fn playback_message(playback: Option<&Playback>) -> String {
    serde_json::json!({ "type": "playerPlayback", "playback": playback }).to_string()
}

/// What the host tells the player's page of the cab, as it tells the music services' (apple_music.rs).
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
    Equalizer {
        #[serde(flatten)]
        equalizer: Equalizer,
    },
    Spectrum {
        on: bool,
    },
}

/// What the page was told last of the cab (sent again to a page that loaded anew).
#[derive(Debug, Default)]
struct Told {
    speakers: Option<Option<Vec<Speaker>>>,
    head: Option<Head>,
    camera: Option<(bool, f32)>,
    equalizer: Option<Equalizer>,
    spectrum: Option<bool>,
    messages: [Option<String>; 5],
}

/// What the player's page said.
pub enum Report {
    /// The music's spectrum, while the cab's Equalizer asks for it ([`speakers::read_spectrum`]).
    Spectrum(Vec<u8>),
    /// What plays (`None`: nothing).
    Playback(Option<Box<Playback>>),
}

/// The player: its WebView, playing the folder's songs.
pub struct Player {
    view: OffscreenWebView,
    told: RefCell<Told>,
}

impl Player {
    /// Opens the player's page in a WebView of `stage`'s frame, hidden under the cabin page, on the cabin page's
    /// profile (`user_data`), the songs of `music` served to it; its page kept in `page_dir`.
    pub fn start(
        stage: &Stage,
        width: u32,
        height: u32,
        user_data: &Path,
        page_dir: &Path,
        music: Option<&Path>,
    ) -> Result<Self, webview::Error> {
        let view = OffscreenWebView::new(stage, width, height, user_data)?;
        view.log_as("player")?;
        view.show(false)?;
        stage.add(&view, false)?;
        view.add_startup_script(concat!(include_str!("cab_sound.js"), "\n", include_str!("music_folder.js")))?;
        let _ = std::fs::create_dir_all(page_dir);
        let _ = std::fs::write(page_dir.join("index.html"), "<!doctype html><meta charset=\"utf-8\"><title>Player</title>");
        view.map_folder(PAGE_HOST, page_dir)?;
        let player = Self { view, told: RefCell::default() };
        player.serve(music)?;
        player.view.navigate(&format!("https://{PAGE_HOST}/index.html"))?;
        Ok(player)
    }

    /// The music folder whose songs it plays (none: the game's folder isn't known, nothing is served).
    pub fn serve(&self, music: Option<&Path>) -> windows::core::Result<()> {
        self.view.unmap_folder(MUSIC_HOST);
        match music {
            Some(folder) => self.view.map_folder_to_any_page(MUSIC_HOST, folder),
            None => Ok(()),
        }
    }

    /// The truck's speakers, the driver's head, the camera and the tone, told as they change (as Apple Music's
    /// page is: apple_music.rs).
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
        self.view.post_json(&serde_json::to_string(&command.checked()).unwrap_or_default())
    }

    /// What the page said since the last call.
    pub fn take_reports(&self) -> Vec<Report> {
        let mut reports = Vec::new();
        for message in self.view.take_page_messages() {
            if let Some(bars) = speakers::read_spectrum(&message) {
                reports.push(Report::Spectrum(bars));
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&message) else { continue };
            if value.get("ready").and_then(serde_json::Value::as_bool) == Some(true) {
                // A page loaded anew: the cab as it is.
                for told in self.told.borrow().messages.iter().flatten() {
                    let _ = self.view.post_json(told);
                }
            } else if let Some(playback) = value.get("playback") {
                let playback = serde_json::from_value::<Playback>(playback.clone()).ok().map(Box::new);
                reports.push(Report::Playback(playback));
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plays_the_kinds_the_browser_plays_in_any_case() {
        assert!(plays("Night Drive.mp3"));
        assert!(plays("road.OGG"));
        assert!(plays("live.flac"));
        assert!(!plays("cover.jpg"));
        assert!(!plays("readme"));
        assert!(!plays("song.wma"));
    }

    #[test]
    fn lists_names_as_explorer_does() {
        let mut names = vec!["10 b.mp3", "2 a.mp3", "B.mp3", "a.mp3", "1 c.mp3"];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(names, ["1 c.mp3", "2 a.mp3", "10 b.mp3", "a.mp3", "B.mp3"]);
    }

    #[test]
    fn reads_the_cabin_pages_commands_and_lets_only_names_through() {
        let play: Command =
            serde_json::from_str(r#"{"action":"play","files":["a.mp3","..\\x.mp3","b.ogg"],"index":1}"#).unwrap();
        assert_eq!(
            play.checked(),
            Command::Play { files: Some(vec!["a.mp3".into(), "b.ogg".into()]), index: Some(1), shuffle: None }
        );
        assert_eq!(serde_json::from_str::<Command>(r#"{"action":"play"}"#).unwrap(), Command::Play {
            files: None,
            index: None,
            shuffle: None
        });
        assert_eq!(
            serde_json::from_str::<Command>(r#"{"action":"repeat","mode":"one"}"#).unwrap(),
            Command::Repeat { mode: Repeat::One }
        );
        assert!(!is_name("C:x.mp3") && !is_name("..") && !is_name("a/b.mp3") && is_name("a.b.mp3"));
    }

    #[test]
    fn lists_a_folder_without_tags_by_its_files_names() {
        let dir = std::env::temp_dir().join(format!("westplay-music-folder-{}", std::process::id()));
        let covers = dir.join("covers");
        let music = dir.join("music");
        std::fs::create_dir_all(&music).unwrap();
        std::fs::write(music.join("2 Second.mp3"), b"not really a song").unwrap();
        std::fs::write(music.join("10 Tenth.ogg"), b"nor this").unwrap();
        std::fs::write(music.join("notes.txt"), b"not a song").unwrap();
        let songs = read(&music, &covers);
        let files: Vec<_> = songs.iter().map(|song| song.file.as_str()).collect();
        assert_eq!(files, ["2 Second.mp3", "10 Tenth.ogg"]);
        assert!(songs.iter().all(|song| song.title.is_none() && song.cover.is_none() && song.seconds == 0.0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The player's own music folder, read as Player lists it (nothing written there): run by hand,
    /// `cargo test -p westplay-host real_music_folder -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_music_folder() {
        // Another folder to read instead: `WESTPLAY_MUSIC_FOLDER`.
        let folder = std::env::var_os("WESTPLAY_MUSIC_FOLDER")
            .map(PathBuf::from)
            .unwrap_or_else(|| folder_of(Kind::Ets2).expect("the game's Documents folder"));
        let covers = std::env::temp_dir().join(format!("westplay-real-covers-{}", std::process::id()));
        let started = std::time::Instant::now();
        let songs = read(&folder, &covers);
        println!("{} songs in {} ms ({})", songs.len(), started.elapsed().as_millis(), folder.display());
        let tagged = songs.iter().filter(|song| song.title.is_some()).count();
        let timed = songs.iter().filter(|song| song.seconds > 0.0).count();
        let covered = songs.iter().filter(|song| song.cover.is_some()).count();
        println!("{tagged} with a title, {timed} with a length, {covered} with a cover");
        let kinds: BTreeSet<_> = songs.iter().filter_map(|song| song.file.rsplit('.').next()).map(str::to_lowercase).collect();
        println!("kinds: {kinds:?}");
        if std::env::var_os("WESTPLAY_MUSIC_FOLDER").is_some() {
            println!("{}", folder_message(Some(&folder), &songs));
        }
        let _ = std::fs::remove_dir_all(&covers);
    }

    #[test]
    fn says_where_the_folder_is_and_its_songs() {
        let song = Song { file: "a.mp3".into(), title: Some("A".into()), artist: None, seconds: 12.5, cover: None };
        let message: serde_json::Value =
            serde_json::from_str(&folder_message(Some(Path::new("music")), &[song])).unwrap();
        assert_eq!(message["type"], "musicFolder");
        assert_eq!(message["folder"], "music");
        assert_eq!(message["songs"][0], serde_json::json!({ "file": "a.mp3", "title": "A", "seconds": 12.5 }));
        assert_eq!(serde_json::from_str::<serde_json::Value>(&folder_message(None, &[])).unwrap()["folder"], serde_json::Value::Null);
    }
}
