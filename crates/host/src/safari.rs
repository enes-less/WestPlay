//! Safari: the web in the cab (owner 2026-11-07). Each tab's page is a WebView of its own, placed under the cabin
//! page where the Safari app leaves room for it (the app draws Safari's bars around it); the player's mouse
//! reaches the page shown there (pointer.rs), and the cab's keyboard types into the field pressed in it.
//!
//! Every tab shares one WebView2 profile of Safari's own (`webview2-safari`), so a site's sign-in and cookies are
//! kept as a browser keeps them, apart from WestPlay's own players'. The tabs themselves and the history are the
//! cab's (it keeps them); here a tab is only its page while it's live: the shown one, and the few used last
//! ([`MOST_LIVE`]); one let go is loaded again from its address as it's shown.
//!
//! A page's sound comes through the truck's speakers as the music apps' does (cab_sound.js, safari.js), where the
//! browser lets a page have its own sound; it pauses with the game and for the other players, as the cab asks.
//! A car screen, not a PC: no downloads, no other apps opened from a page (`mailto:`, `steam:`…), and every
//! permission a page asks for (camera, location, notifications…) refused.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_PROFILE, COREWEBVIEW2_PERMISSION_STATE_DENY, COREWEBVIEW2_SCRIPT_DIALOG_KIND,
    COREWEBVIEW2_SCRIPT_DIALOG_KIND_ALERT, COREWEBVIEW2_SCRIPT_DIALOG_KIND_BEFOREUNLOAD,
    COREWEBVIEW2_SCRIPT_DIALOG_KIND_CONFIRM, COREWEBVIEW2_SCRIPT_DIALOG_KIND_PROMPT, ICoreWebView2, ICoreWebView2_4,
    ICoreWebView2_13, ICoreWebView2_15, ICoreWebView2_18, ICoreWebView2Deferral, ICoreWebView2Environment,
    ICoreWebView2NavigationCompletedEventArgs2, ICoreWebView2NewWindowRequestedEventArgs, ICoreWebView2Profile2,
    ICoreWebView2ScriptDialogOpeningEventArgs, ICoreWebView2Settings4,
};
use webview2_com::{
    ClearBrowsingDataCompletedHandler, ContainsFullScreenElementChangedEventHandler, DocumentTitleChangedEventHandler,
    DownloadStartingEventHandler, FaviconChangedEventHandler, HistoryChangedEventHandler,
    LaunchingExternalUriSchemeEventHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, PermissionRequestedEventHandler, ScriptDialogOpeningEventHandler,
    SourceChangedEventHandler, WindowCloseRequestedEventHandler,
};
use windows::core::{BOOL, HSTRING, Interface, PWSTR};

use crate::speakers::{self, Equalizer, Head, Speaker};
use crate::webview::{self, OffscreenWebView, Stage};

/// Where the tabs' pictures are served to the cabin page (the tab overview's).
pub const TABS_HOST: &str = "westplay.safari-tabs";

/// The most tabs whose pages stay live: the one shown and those used last (each is a browser's page, its memory
/// and its process). A tab playing sound stays live whatever this says.
const MOST_LIVE: usize = 4;

/// Where a tab the page opens itself (a link to a new window) is numbered from: the cab's own tabs are numbered
/// under it. The count goes on while WestPlay runs (Safari closing with the game and opening again), so a tab the
/// cab still has is never numbered twice.
const OPENED_FROM: u32 = 1 << 30;
static OPENED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(OPENED_FROM);

/// A page of Safari's own, made only to clear the websites' data with no tab open: never told the cab.
const FORGETTING: u32 = u32::MAX;

/// The addresses whose cookies are Google's sign-in (YouTube's and YouTube Music's, on the cab's profile): given to
/// Safari's profile as it starts (owner 2026-11-08: "can safari use our tokens for google we had with yt and yt
/// music"), so Google, YouTube, Gmail and the rest open signed in as the cab's apps are.
pub const GOOGLE_SIGN_IN: [&str; 6] = [
    "https://accounts.google.com/",
    "https://www.google.com/",
    "https://myaccount.google.com/",
    "https://mail.google.com/",
    "https://www.youtube.com/",
    "https://music.youtube.com/",
];

/// The cookies for `uris` as `from`'s profile has them, each once (the same name, domain and path).
pub fn sign_in_of(from: &OffscreenWebView, uris: &[&str]) -> Vec<webview::Cookie> {
    let mut found: Vec<webview::Cookie> = Vec::new();
    for uri in uris {
        match from.cookies(uri) {
            Ok(cookies) => {
                for cookie in cookies {
                    if !found
                        .iter()
                        .any(|c| c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path)
                    {
                        found.push(cookie);
                    }
                }
            }
            Err(e) => log!("[safari] couldn't read the cab's cookies for {uri}: {e}"),
        }
    }
    found
}

/// What runs in every frame of Safari's pages before their own scripts (cab_sound.js, safari.js), but in a site's
/// bot check (Cloudflare's, hCaptcha's, reCAPTCHA's: safari.js `BOT_CHECK` too), left as a browser has it but for the
/// mark of a WebView (`chrome.webview`, which Chrome has not): a check looks for changes to the browser's own
/// functions (sound's, here) and anything else of a page's not its own (owner 2026-11-08: Cloudflare's check
/// failed, "Doğrulama başarısız").
const STARTUP_SCRIPT: &str = concat!(
    "if (!(/(^|\\.)(challenges\\.cloudflare\\.com|hcaptcha\\.com|recaptcha\\.net)$/.test(location.hostname) || ",
    "(location.hostname === 'www.google.com' && location.pathname.startsWith('/recaptcha/')))) {\n",
    include_str!("cab_sound.js"),
    "\n",
    include_str!("safari.js"),
    "\n} else {\n  try {\n    delete window.chrome.webview\n  } catch {}\n}\n",
);

/// The times a page is loaded again at most for the cab's appearance before its site is taken to keep its own colours.
const SCHEME_RELOADS: u8 = 2;

/// The longest address or title passed to the cab (a page's own title can be anything).
const TEXT_MOST: usize = 2048;

/// What the Safari app asks (`safari` in ui/src/shared/host.ts).
#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "action", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Command {
    /// Show tab `tab` at this rectangle of the frame (pixels), its page zoomed by `zoom` (the cab's scale); its
    /// page loaded from `url` if it isn't live (a tab kept from before, or let go). `covers`: where the cab draws
    /// over it (a menu, a dialog), the mouse the cab's there.
    Show {
        tab: u32,
        url: String,
        x: f32,
        y: f32,
        width: u32,
        height: u32,
        zoom: f64,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        covers: Vec<Rect>,
        /// The cab's appearance, dark or light: the pages' too ([`Command::Appearance`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dark: Option<bool>,
        /// Its bottom corners' rounding (frame pixels): the page cut to it, nothing of it past the cab's corners.
        #[serde(default)]
        radius: f32,
        /// The cab's language (`tr-TR`): the pages are asked for in it, as an iPad's Safari asks in the device's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
    },
    /// The cab's appearance changed: every page sees it as the system's (CSS `prefers-color-scheme`), whatever
    /// Windows' own is (owner 2026-11-08: "any website should follow our light/dark switch STRICTLY").
    Appearance {
        dark: bool,
    },
    /// No tab shown (Safari closed, or its tab overview up): the pages live on, heard. `tab`: only if it's that one
    /// shown (a hide coming after the cab has drawn itself over the page never hides a tab shown since).
    Hide {
        #[serde(default)]
        tab: Option<u32>,
    },
    /// Load `url` in tab `tab` (made live if it isn't).
    Open {
        tab: u32,
        url: String,
    },
    /// The tab closed: its page goes.
    Close {
        tab: u32,
    },
    Back {
        tab: u32,
    },
    Forward {
        tab: u32,
    },
    Reload {
        tab: u32,
    },
    Stop {
        tab: u32,
    },
    /// Text typed (the cab's keyboard, or the player's own) into the field pressed in the tab shown.
    Type {
        text: String,
    },
    /// A key pressed there.
    Key {
        key: Key,
    },
    /// Pause every page's sound (the game away, another player started)…
    Pause,
    /// …and play on what that paused.
    Resume,
    /// Every page's sound times this (0 to 1): the cab's fade.
    Level {
        level: f64,
    },
    /// The answer to a page's dialog in tab `tab` (`text`: a prompt's).
    Answer {
        tab: u32,
        accept: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// The option picked from the page's drop-down list `list` (its [`List::id`]), or none (`index` none: the list
    /// put away).
    Pick {
        tab: u32,
        list: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<u32>,
    },
    /// The tab's page out of full screen.
    ExitFullScreen {
        tab: u32,
    },
    /// A picture of the tab's page, for the tab overview (answered `safariPicture`).
    Picture {
        tab: u32,
    },
    /// Clear History and Website Data: the sites' cookies, storage and cache from `since` on (ms since 1970, as the
    /// cab's clock counts; 0: all of it). The history itself is the cab's.
    ForgetWebsites {
        #[serde(default)]
        since: f64,
    },
}

/// A rectangle of the frame (pixels).
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Key {
    Backspace,
    Enter,
}

/// A tab as the cab is told it.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabState {
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub back: bool,
    pub forward: bool,
    /// The page's icon's address (the cab shows it as a browser does), or none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub favicon: String,
    /// The page's colour at its top (its `theme-color`, or its background), for Safari's bars.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub theme: String,
    /// The page's own ground (its background), for what of the cab edges it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ground: String,
    pub full_screen: bool,
    /// The page has drawn something (its document's content loaded, or the load done): a WebView draws nothing at all
    /// till then, so the cab is cut open over it only after (owner 2026-11-08: the game's own screen showed through
    /// as a new tab opened).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub painted: bool,
    /// The field pressed in the page: what it holds (a password's as dots) and its kind; none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<Field>,
    /// The page failed to load (the browser's reason), shown in place of it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
    /// A drop-down list pressed in the page, for the cab to show its options (its own list: the browser's would be a
    /// window on the PC's screen).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<List>,
}

/// A page's drop-down list (`<select>`) as the cab shows it: its options, the one picked, and where it is on the page
/// (frame pixels from the page's top left; none: somewhere the page couldn't say, in a frame of another's).
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct List {
    /// Which list it is, as the page names it (the answer goes back with it).
    pub id: String,
    pub options: Vec<ListOption>,
    /// The option picked (-1: none).
    pub selected: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<Rect>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListOption {
    pub label: String,
    /// The group it's in (`<optgroup>`'s label), or none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}

/// The most options of a list passed to the cab, and the longest label.
const LIST_MOST: usize = 1000;
const LABEL_MOST: usize = 200;

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Field {
    pub kind: String,
    pub value: String,
    /// Which press of a field it is (the page counts them): a field pressed again is told again.
    #[serde(default)]
    pub n: u32,
}

/// A page's dialog (`alert`, `confirm`, `prompt`; a page asking to stay as it's left is let go).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dialog {
    pub kind: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// What the pages did, for the cab.
#[derive(Debug, PartialEq)]
pub enum Report {
    /// A tab as it is now.
    Tab { tab: u32, state: Box<TabState> },
    /// A page opened a new window: a new tab (live, with the page in it), after `opener`.
    Opened { tab: u32, opener: u32 },
    /// A page closed its own window (`window.close()`, as a sign-in's window does when done).
    Closed { tab: u32 },
    /// A tab's page let go (the cab's tab stays, loaded again as shown).
    LetGo { tab: u32 },
    /// A page's dialog, waiting for the player's answer ([`Command::Answer`]).
    Dialog { tab: u32, dialog: Dialog },
    /// A tab's picture written, at `TABS_HOST` (`tab-<tab>.png`); `version` new with each.
    Picture { tab: u32, version: u32 },
    /// A page tried to download a file, which a cab doesn't.
    NoDownload { tab: u32 },
    /// Whether any page plays sound.
    Playing(bool),
    /// The music's spectrum, while the cab's Equalizer asks for it.
    Spectrum(Vec<u8>),
}

impl Report {
    /// The cab's message for it.
    pub fn message(&self) -> String {
        use serde_json::json;
        match self {
            Report::Tab { tab, state } => json!({ "type": "safariTab", "tab": tab, "state": state }),
            Report::Opened { tab, opener } => json!({ "type": "safariOpened", "tab": tab, "opener": opener }),
            Report::Closed { tab } => json!({ "type": "safariClosed", "tab": tab }),
            Report::LetGo { tab } => json!({ "type": "safariLetGo", "tab": tab }),
            Report::Dialog { tab, dialog } => json!({ "type": "safariDialog", "tab": tab, "dialog": dialog }),
            Report::Picture { tab, version } => json!({ "type": "safariPicture", "tab": tab, "version": version }),
            Report::NoDownload { tab } => json!({ "type": "safariNoDownload", "tab": tab }),
            Report::Playing(playing) => json!({ "type": "safariPlaying", "playing": playing }),
            Report::Spectrum(bars) => return speakers::spectrum_message(bars),
        }
        .to_string()
    }
}

/// What a tab's page's events left for the next poll.
#[derive(Default)]
struct Events {
    /// The page has drawn something ([`TabState::painted`]).
    painted: bool,
    /// Something of the tab changed (its address, title, loading, history, icon, full screen).
    changed: bool,
    loading: bool,
    failed: Option<String>,
    closed: bool,
    downloads: u32,
    /// Windows the page asked for, waiting for a tab of their own.
    opened: Vec<(ICoreWebView2NewWindowRequestedEventArgs, ICoreWebView2Deferral)>,
    /// A dialog waiting for the player.
    dialog: Option<(ICoreWebView2ScriptDialogOpeningEventArgs, ICoreWebView2Deferral, Dialog)>,
    dialog_told: bool,
}

struct Tab {
    id: u32,
    view: OffscreenWebView,
    events: Rc<RefCell<Events>>,
    /// What the page itself said (safari.js): its field, colour, sound.
    field: Option<Field>,
    theme: String,
    /// The page's own ground (its background), as it said.
    ground: String,
    playing: bool,
    /// As last told the cab.
    told: Option<TabState>,
    /// When it was last shown or used (a count), to let the oldest go.
    used: u64,
    /// Safari's frame for it as last given (width, height, zoom: [`fit_to_width`]).
    viewport: Option<(u32, u32, f64)>,
    /// The page's window as last given (width, height, pixel ratio, a phone's).
    given: Option<(f64, f64, f64, bool)>,
    /// Its site (`www.example.com`), as its page last said.
    site: String,
    /// Its pages asked for as a phone's Chrome asks ([`Safari::set_mobile`]).
    mobile: bool,
    /// How many times its page has been loaded again for the cab's appearance (it didn't follow it as it ran), to
    /// see if it does as it loads ([`SCHEME_RELOADS`] at most).
    scheme_reloads: u8,
    /// A drop-down list pressed in its page, waiting for the player's pick.
    list: Option<List>,
    /// How far its page is scaled in its window (1: not; a phone's page laid out wider than it, as the page says):
    /// the mouse's place on the page is the frame's pixels over the zoom and this.
    scale: f64,
}

/// What the cab was last told of the cab's sound, for each page (and a page loaded anew).
#[derive(Debug, Default)]
struct Told {
    speakers: Option<Option<Vec<Speaker>>>,
    head: Option<Head>,
    camera: Option<(bool, f32)>,
    equalizer: Option<Equalizer>,
    spectrum: Option<bool>,
    level: Option<f64>,
    messages: [Option<String>; 6],
}

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
    Level {
        level: f64,
    },
}

/// Where the tab shown is on the frame, for the mouse (page pixels) and its page, and where the cab covers it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub tab: u32,
    pub x: f32,
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub covers: Vec<Rect>,
    /// The page's zoom: its CSS pixels this many of the frame's (the mouse's place on the page is given in them).
    pub zoom: f64,
}

pub struct Safari {
    environment: ICoreWebView2Environment,
    tabs: Vec<Tab>,
    shown: Option<Shown>,
    zoom: f64,
    /// The size a page is made at before it's shown (the frame's).
    size: (u32, u32),
    pictures: PathBuf,
    picture_versions: Rc<RefCell<Vec<(u32, u32)>>>,
    pictures_done: Rc<RefCell<Vec<u32>>>,
    told: Told,
    playing: bool,
    used: u64,
    reports: Vec<Report>,
    /// Safari's browser itself failed (its process gone): every page with it, and a new one needed.
    browser_gone: bool,
    /// The cab's Google sign-in, given to Safari's profile with its first page (then emptied).
    sign_in: Vec<webview::Cookie>,
    /// The websites' data cleared, as last asked (the page made for it goes then).
    forgotten: Rc<std::cell::Cell<bool>>,
    /// The cab's appearance, as last told (`None`: not yet): the pages' colour scheme.
    dark: Option<bool>,
    /// The cab's language, as last told (`None`: not yet): the language the pages are asked for in.
    language: Option<String>,
    /// Sites whose pages keep their own colours whatever the cab's appearance (loaded again for it, still not
    /// following it): not loaded again for it, while Safari's browser runs.
    keep_their_colours: std::collections::BTreeSet<String>,
    /// The browser's own identity, as a page sees it ([`own_identity`]), read once; `None`: not yet.
    identity: Option<Identity>,
    /// Tests: a site of their own, served to every tab's page.
    #[cfg(test)]
    test_site: Option<PathBuf>,
}

impl Safari {
    /// Safari's browser, on its own profile (`user_data`), its tabs' pictures kept in `pictures`; no tab yet.
    pub fn start(
        width: u32,
        height: u32,
        user_data: &Path,
        pictures: &Path,
        sign_in: Vec<webview::Cookie>,
    ) -> Result<Self, webview::Error> {
        let environment = webview::browsing_environment(user_data)?;
        let _ = std::fs::create_dir_all(pictures);
        Ok(Self {
            environment,
            tabs: Vec::new(),
            shown: None,
            zoom: 1.0,
            size: (width, height),
            pictures: pictures.to_path_buf(),
            picture_versions: Rc::default(),
            pictures_done: Rc::default(),
            told: Told::default(),
            playing: false,
            used: 0,
            reports: Vec::new(),
            browser_gone: false,
            sign_in,
            forgotten: Rc::default(),
            dark: None,
            language: None,
            keep_their_colours: std::collections::BTreeSet::new(),
            identity: None,
            #[cfg(test)]
            test_site: None,
        })
    }

    /// The tab shown and where, for the mouse to reach its page (`None`: none).
    pub fn shown(&self) -> Option<(&Shown, &OffscreenWebView)> {
        let shown = self.shown.as_ref()?;
        self.tabs.iter().find(|tab| tab.id == shown.tab).map(|tab| (shown, &tab.view))
    }

    pub fn command(&mut self, stage: &Stage, command: &Command) -> Result<(), webview::Error> {
        match command {
            Command::Show { tab, url, x, y, width, height, zoom, covers, dark, radius, language } => {
                if let Some(dark) = dark {
                    self.set_dark(*dark);
                }
                if let Some(language) = language.as_deref().filter(|language| is_language_tag(language)) {
                    self.set_language(language);
                }
                let (width, height) = ((*width).max(1), (*height).max(1));
                self.zoom = zoom.clamp(0.25, 5.0);
                self.size = (width, height);
                if let Some(before) = self.shown.as_ref().filter(|shown| shown.tab != *tab).map(|shown| shown.tab)
                    && let Some(old) = self.tab(before)
                {
                    let _ = old.view.show(false);
                }
                if self.tab(*tab).is_none() {
                    self.make(stage, *tab)?;
                    self.load(*tab, url);
                }
                let zoom = self.zoom;
                self.used += 1;
                let used = self.used;
                if let Some(shown) = self.tab_mut(*tab) {
                    shown.used = used;
                    shown.view.place(*x, *y, width, height)?;
                    shown.view.round_bottom_corners(width as f32, height as f32, radius.max(0.0))?;
                    fit_to_width(shown, width, height, zoom);
                    shown.view.show(true)?;
                }
                let covers = covers.iter().take(16).copied().collect();
                self.shown = Some(Shown { tab: *tab, x: *x, y: *y, width, height, covers, zoom });
                self.let_go_of_the_oldest(stage);
            }
            Command::Appearance { dark } => self.set_dark(*dark),
            Command::Hide { tab } => {
                if tab.is_none_or(|tab| self.shown.as_ref().is_some_and(|shown| shown.tab == tab))
                    && let Some(shown) = self.shown.take()
                    && let Some(tab) = self.tab(shown.tab)
                {
                    tab.view.show(false)?;
                }
            }
            Command::Open { tab, url } => {
                if self.tab(*tab).is_none() {
                    self.make(stage, *tab)?;
                    self.let_go_of_the_oldest(stage);
                }
                self.load(*tab, url);
            }
            Command::Close { tab } => {
                if let Some(at) = self.tabs.iter().position(|t| t.id == *tab) {
                    let closed = self.tabs.remove(at);
                    let _ = stage.remove(&closed.view);
                }
                if self.shown.as_ref().is_some_and(|shown| shown.tab == *tab) {
                    self.shown = None;
                }
                let _ = std::fs::remove_file(self.picture_path(*tab));
                self.tell_playing();
            }
            // SAFETY: the tab's own WebView, alive while the tab is; no arguments.
            Command::Back { tab } => self.with(*tab, |view| unsafe { view.GoBack() }),
            // SAFETY: as above.
            Command::Forward { tab } => self.with(*tab, |view| unsafe { view.GoForward() }),
            // SAFETY: as above.
            Command::Reload { tab } => self.with(*tab, |view| unsafe { view.Reload() }),
            // SAFETY: as above.
            Command::Stop { tab } => self.with(*tab, |view| unsafe { view.Stop() }),
            Command::Type { text } => {
                let text: String = text.chars().filter(|c| !c.is_control()).collect();
                if let Some((_, view)) = self.shown()
                    && !text.is_empty()
                {
                    view.devtools("Input.insertText", &serde_json::json!({ "text": text }).to_string())?;
                }
            }
            Command::Key { key } => {
                if let Some((_, view)) = self.shown() {
                    for event in key_events(*key) {
                        view.devtools("Input.dispatchKeyEvent", &event)?;
                    }
                }
            }
            Command::Pause => self.post_all(r#"{"action":"pauseMedia"}"#),
            Command::Resume => self.post_all(r#"{"action":"resumeMedia"}"#),
            Command::Level { level } => {
                let level = if level.is_finite() { level.clamp(0.0, 1.0) } else { 1.0 };
                if self.told.level != Some(level) {
                    self.told.level = Some(level);
                    let message = serde_json::to_string(&Cab::Level { level }).unwrap_or_default();
                    self.told.messages[5] = Some(message.clone());
                    self.post_all(&message);
                }
            }
            Command::Answer { tab, accept, text } => {
                if let Some(tab) = self.tab(*tab)
                    && let Some((args, deferral, dialog)) = tab.events.borrow_mut().dialog.take()
                {
                    // SAFETY: the dialog's own arguments and deferral, kept for this answer; strings live through
                    // the calls.
                    unsafe {
                        if *accept {
                            if dialog.kind == "prompt" {
                                let _ = args.SetResultText(&HSTRING::from(text.as_deref().unwrap_or("")));
                            }
                            let _ = args.Accept();
                        }
                        let _ = deferral.Complete();
                    }
                }
            }
            Command::Pick { tab, list, index } => {
                if let Some(tab) = self.tab_mut(*tab)
                    && tab.list.as_ref().is_some_and(|shown| &shown.id == list)
                {
                    tab.list = None;
                    tab.events.borrow_mut().changed = true;
                    if let Some(index) = index {
                        let message = serde_json::json!({ "action": "pick", "list": list, "index": index });
                        let _ = tab.view.post_json(&message.to_string());
                    }
                }
            }
            Command::ExitFullScreen { tab } => {
                if let Some(tab) = self.tab(*tab) {
                    tab.view.run_script("document.fullscreenElement && document.exitFullscreen()", |_| ())?;
                }
            }
            Command::Picture { tab } => self.picture(*tab)?,
            Command::ForgetWebsites { since } => {
                // Through a tab's page (every tab shares the profile); with none, one of its own for as long as it
                // takes, gone once it's done (`take_reports`).
                if self.tabs.iter().all(|tab| tab.id == FORGETTING) {
                    self.make(stage, FORGETTING)?;
                }
                let Some(tab) = self.tabs.iter().find(|tab| tab.id != FORGETTING).or_else(|| self.tab(FORGETTING))
                else {
                    return Ok(());
                };
                let done = self.forgotten.clone();
                done.set(false);
                let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |_| {
                    done.set(true);
                    Ok(())
                }));
                // SAFETY: the tab's own WebView's profile; WebView2 keeps its own reference to the handler.
                unsafe {
                    let profile = tab.view.core().cast::<ICoreWebView2_13>()?.Profile()?;
                    let profile = profile.cast::<ICoreWebView2Profile2>()?;
                    if *since > 0.0 {
                        // From then until now (seconds since 1970, as WebView2 counts them).
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0.0, |now| now.as_secs_f64());
                        profile.ClearBrowsingDataInTimeRange(
                            COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_PROFILE,
                            since / 1000.0,
                            now + 60.0,
                            &handler,
                        )?;
                    } else {
                        profile.ClearBrowsingDataAll(&handler)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// The truck's speakers, the driver's head, the camera and the tone, told every page as they change.
    pub fn cab(
        &mut self,
        speakers: Option<&[Speaker]>,
        head: Head,
        outside: bool,
        distance: f32,
        equalizer: Equalizer,
        spectrum: bool,
    ) {
        let mut messages = Vec::new();
        let told = &mut self.told;
        // The head first: the speakers' loudness is set from where it is as they're placed.
        if told.head != Some(head) && head.is_sound() {
            told.head = Some(head);
            let message = serde_json::to_string(&Cab::Head { head }).unwrap_or_default();
            told.messages[1] = Some(message.clone());
            messages.push(message);
        }
        if told.speakers.as_ref().map(Option::as_deref) != Some(speakers) {
            let speakers = speakers.map(<[_]>::to_vec);
            told.speakers = Some(speakers.clone());
            let message = serde_json::to_string(&Cab::Speakers { speakers }).unwrap_or_default();
            told.messages[0] = Some(message.clone());
            messages.push(message);
        }
        let camera = (outside, if outside { (distance * 4.0).round() / 4.0 } else { 0.0 });
        if told.camera != Some(camera) && camera.1.is_finite() {
            told.camera = Some(camera);
            let message = serde_json::to_string(&Cab::Outside { on: camera.0, distance: camera.1 }).unwrap_or_default();
            told.messages[2] = Some(message.clone());
            messages.push(message);
        }
        if told.equalizer != Some(equalizer) {
            told.equalizer = Some(equalizer);
            let message = serde_json::to_string(&Cab::Equalizer { equalizer }).unwrap_or_default();
            told.messages[3] = Some(message.clone());
            messages.push(message);
        }
        if told.spectrum != Some(spectrum) {
            told.spectrum = Some(spectrum);
            let message = serde_json::to_string(&Cab::Spectrum { on: spectrum }).unwrap_or_default();
            told.messages[4] = Some(message.clone());
            messages.push(message);
        }
        for message in messages {
            self.post_all(&message);
        }
    }

    /// What the pages did since the last call, for the cab.
    pub fn take_reports(&mut self, stage: &Stage) -> Vec<Report> {
        // Windows the pages asked for: each a new tab, its page the window's.
        let mut asked = Vec::new();
        for tab in &self.tabs {
            for (args, deferral) in std::mem::take(&mut tab.events.borrow_mut().opened) {
                asked.push((tab.id, args, deferral));
            }
        }
        for (opener, args, deferral) in asked {
            let id = OPENED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let made = self.make(stage, id);
            // SAFETY: the request's own arguments and deferral, kept for this; the new WebView not navigated yet,
            // as WebView2 wants it.
            unsafe {
                match (made, self.tab(id)) {
                    (Ok(()), Some(tab)) => {
                        let _ = args.SetNewWindow(tab.view.core());
                        let _ = args.SetHandled(true);
                        self.reports.push(Report::Opened { tab: id, opener });
                    }
                    _ => {
                        let _ = args.SetHandled(true);
                    }
                }
                let _ = deferral.Complete();
            }
        }
        if !self.tabs.is_empty() {
            self.let_go_of_the_oldest(stage);
        }

        // The page made to clear the websites' data, once that's done.
        if self.forgotten.get()
            && let Some(at) = self.tabs.iter().position(|tab| tab.id == FORGETTING)
        {
            let gone = self.tabs.remove(at);
            let _ = stage.remove(&gone.view);
        }
        let mut playing_changed = false;
        for at in 0..self.tabs.len() {
            if self.tabs[at].id == FORGETTING {
                continue;
            }
            let tab = &mut self.tabs[at];
            // Read again only what may have changed: the page's events, or what it said.
            let mut dirty = tab.told.is_none() || std::mem::take(&mut tab.events.borrow_mut().changed);
            // What the page itself said.
            for message in tab.view.take_page_messages() {
                if let Some(bars) = speakers::read_spectrum(&message) {
                    self.reports.push(Report::Spectrum(bars));
                    continue;
                }
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&message) else { continue };
                if value.get("ready").and_then(serde_json::Value::as_bool) == Some(true) {
                    // A page loaded anew: the cab as it is, and nothing pressed in it yet; laid out at the width its
                    // site's pages needed (none: Safari's).
                    for told in self.told.messages.iter().flatten() {
                        let _ = tab.view.post_json(told);
                    }
                    tab.field = None;
                    tab.list = None;
                    tab.theme.clear();
                    tab.ground.clear();
                    tab.site = value
                        .get("site")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .chars()
                        .take(256)
                        .collect();
                    tab.scale = 1.0;
                    dirty = true;
                } else if let Some(follows) = value.get("follows").and_then(serde_json::Value::as_bool) {
                    // Whether the page's colours follow the cab's appearance: one that doesn't as it changes is
                    // loaded again (it may choose them as it loads: owner 2026-11-09, "website theme should be asked
                    // to reload. if possible. if not, reload it"), and again if it still doesn't: a site may keep the
                    // appearance it was last shown in in a cookie its next page is drawn by, and let it go only with
                    // that page (measured: m.youtube.com's PREF `f6`, dark, its reloaded page still dark, the one after
                    // light). One that doesn't then keeps its own colours.
                    let changed = value.get("changed").and_then(serde_json::Value::as_bool) == Some(true);
                    if follows {
                        tab.scheme_reloads = 0;
                    } else if changed {
                        if !tab.site.is_empty() && !self.keep_their_colours.contains(&tab.site) {
                            tab.scheme_reloads = 1;
                            let _ = tab.view.reload();
                        }
                    } else if tab.scheme_reloads > 0 && !tab.site.is_empty() {
                        if tab.scheme_reloads < SCHEME_RELOADS {
                            tab.scheme_reloads += 1;
                            let _ = tab.view.reload();
                        } else {
                            tab.scheme_reloads = 0;
                            log!("[safari] {} keeps its own colours, whatever the cab's appearance", tab.site);
                            self.keep_their_colours.insert(tab.site.clone());
                        }
                    }
                } else if let Some(scale) = value.get("scale").and_then(serde_json::Value::as_f64) {
                    // A phone's page laid out wider than its window (one made for no particular width), scaled to it.
                    if tab.mobile && scale.is_finite() && scale > 0.0 {
                        tab.scale = scale.clamp(0.05, 10.0);
                    }
                } else if let Some(list) = value.get("list") {
                    tab.list = serde_json::from_value::<Option<List>>(list.clone()).ok().flatten().map(|mut list| {
                        cut(&mut list.id, 64);
                        list.options.truncate(LIST_MOST);
                        for option in &mut list.options {
                            cut(&mut option.label, LABEL_MOST);
                            cut(&mut option.group, LABEL_MOST);
                        }
                        // The page's CSS pixels as the frame's: its window's pixel ratio, and its scale in it.
                        let ratio = (tab.given.map_or(1.0, |given| given.2) * tab.scale) as f32;
                        list.rect = list
                            .rect
                            .filter(|rect| [rect.x, rect.y, rect.width, rect.height].iter().all(|n| n.is_finite()))
                            .map(|rect| Rect {
                                x: rect.x * ratio,
                                y: rect.y * ratio,
                                width: rect.width * ratio,
                                height: rect.height * ratio,
                            });
                        list
                    });
                    dirty = true;
                } else if let Some(field) = value.get("field") {
                    tab.field =
                        serde_json::from_value::<Option<Field>>(field.clone()).ok().flatten().map(|mut field| {
                            cut(&mut field.value, TEXT_MOST);
                            cut(&mut field.kind, 16);
                            field
                        });
                    dirty = true;
                } else if let Some(ground) = value.get("ground").and_then(serde_json::Value::as_str) {
                    tab.ground = ground.chars().take(64).collect();
                    dirty = true;
                } else if let Some(theme) = value.get("theme").and_then(serde_json::Value::as_str) {
                    tab.theme = theme.chars().take(64).collect();
                    dirty = true;
                } else if let Some(playing) = value.get("playing").and_then(serde_json::Value::as_bool)
                    && tab.playing != playing
                {
                    tab.playing = playing;
                    playing_changed = true;
                }
            }
            // A page's process gone: the page loaded again; the browser's, every page gone with it.
            match tab.view.take_failure() {
                Some(webview::Failure::Page) => {
                    log!("[safari] a page's process failed: loaded again");
                    let _ = tab.view.reload();
                }
                Some(webview::Failure::Browser) => self.browser_gone = true,
                None => {}
            }
            let (closed, downloads, dialog) = {
                let mut events = tab.events.borrow_mut();
                let dialog = match &events.dialog {
                    Some((_, _, dialog)) if !events.dialog_told => Some(dialog.clone()),
                    _ => None,
                };
                if dialog.is_some() {
                    events.dialog_told = true;
                }
                (std::mem::take(&mut events.closed), std::mem::take(&mut events.downloads), dialog)
            };
            if let Some(dialog) = dialog {
                self.reports.push(Report::Dialog { tab: tab.id, dialog });
            }
            if downloads > 0 {
                self.reports.push(Report::NoDownload { tab: tab.id });
            }
            if closed {
                self.reports.push(Report::Closed { tab: tab.id });
            }
            if !dirty {
                continue;
            }
            let state = tab_state(tab);
            if tab.told.as_ref() != Some(&state) {
                tab.told = Some(state.clone());
                self.reports.push(Report::Tab { tab: tab.id, state: Box::new(state) });
            }
        }
        if playing_changed {
            self.tell_playing();
        }
        for tab in self.pictures_done.borrow_mut().drain(..) {
            let mut versions = self.picture_versions.borrow_mut();
            let version = match versions.iter_mut().find(|(id, _)| *id == tab) {
                Some((_, version)) => {
                    *version += 1;
                    *version
                }
                None => {
                    versions.push((tab, 1));
                    1
                }
            };
            self.reports.push(Report::Picture { tab, version });
        }
        std::mem::take(&mut self.reports)
    }

    /// The pages' colour scheme, the cab's appearance: set on the profile every tab shares (a page made later gets it
    /// as it's made, `make`).
    fn set_dark(&mut self, dark: bool) {
        if self.dark == Some(dark) {
            return;
        }
        self.dark = Some(dark);
        if let Some(tab) = self.tabs.first() {
            set_colour_scheme(tab.view.core(), dark);
        }
    }

    /// The language the pages are asked for in, the cab's: every page's identity told again with it.
    fn set_language(&mut self, language: &str) {
        if self.language.as_deref() == Some(language) {
            return;
        }
        self.language = Some(language.to_owned());
        for tab in &self.tabs {
            let identity = identity_of(self.identity.as_ref(), &tab.view.version, Some(language), tab.mobile);
            let _ = tab.view.devtools("Emulation.setUserAgentOverride", &identity);
        }
    }

    /// Tab `id`'s pages asked for as a phone's Chrome asks (`mobile`), or as a desktop's: a phone's identity (Chrome on
    /// Android, of the same build), its touch screen, and a phone's window (a page made for phones laid out at
    /// Safari's width, one made for no particular width scaled to it). Every tab's pages are a phone's: a site's
    /// mobile site, laid out at Safari's width, never past its sides (owner 2026-11-09: "we MUST fit it horizontally…
    /// this doesn't mean that scale it down… make it ask for mobile ui"; "Why don't we have mobile redirect? I go to
    /// youtube.com and it opens desktop page").
    fn set_mobile(&mut self, id: u32, mobile: bool) {
        let identity = self.identity.clone();
        let language = self.language.clone();
        let Some(tab) = self.tab_mut(id) else { return };
        if tab.mobile == mobile {
            return;
        }
        tab.mobile = mobile;
        tab.scale = 1.0;
        let said = identity_of(identity.as_ref(), &tab.view.version, language.as_deref(), mobile);
        let _ = tab.view.devtools("Emulation.setUserAgentOverride", &said);
        let touch = serde_json::json!({ "enabled": mobile, "maxTouchPoints": if mobile { 5 } else { 1 } });
        let _ = tab.view.devtools("Emulation.setTouchEmulationEnabled", &touch.to_string());
        // The mouse as a finger on a phone's page: a drag scrolls it (and flicks it on), a press is a tap. As a
        // mouse's, a drag selected the page's text instead (owner 2026-11-09: "touch-scrolling makes me select
        // text"). Measured: a drag of 240 scrolled 260, nothing selected, a click still a click.
        let fingers = serde_json::json!({ "enabled": mobile, "configuration": "mobile" });
        let _ = tab.view.devtools("Emulation.setEmitTouchEventsForMouse", &fingers.to_string());
        lay_out(tab);
    }

    /// How many of the frame's pixels tab `tab`'s page has for each of its CSS pixels: its window's pixel ratio and
    /// its scale in it (`None`: not laid out yet). The mouse's place on the page is the frame's pixels over it.
    pub fn page_zoom(&self, tab: u32) -> Option<f64> {
        self.tab(tab).and_then(|tab| tab.given.map(|given| given.2 * tab.scale))
    }

    /// How far tab `tab`'s page is scaled in its window (1: not, or not said yet).
    #[cfg(test)]
    fn page_scale(&self, tab: u32) -> f64 {
        self.tab(tab).map_or(1.0, |tab| tab.scale)
    }

    /// Whether Safari's browser failed (it's then closed, its tabs let go, and started again when needed).
    pub fn failed(&self) -> bool {
        self.browser_gone
    }

    /// The tabs whose pages are live.
    pub fn live(&self) -> Vec<u32> {
        self.tabs.iter().map(|tab| tab.id).filter(|&id| id != FORGETTING).collect()
    }

    /// Takes the pages off the frame; they close as they're dropped.
    pub fn close(self, stage: &Stage) {
        for tab in &self.tabs {
            let _ = stage.remove(&tab.view);
        }
    }

    fn tab(&self, id: u32) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    fn tab_mut(&mut self, id: u32) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    fn with(&self, id: u32, act: impl FnOnce(&ICoreWebView2) -> windows::core::Result<()>) {
        if let Some(tab) = self.tab(id) {
            let _ = act(tab.view.core());
        }
    }

    fn post_all(&self, message: &str) {
        for tab in &self.tabs {
            let _ = tab.view.post_json(message);
        }
    }

    fn tell_playing(&mut self) {
        let playing = self.tabs.iter().any(|tab| tab.playing);
        if playing != self.playing {
            self.playing = playing;
            self.reports.push(Report::Playing(playing));
        }
    }

    fn picture_path(&self, tab: u32) -> PathBuf {
        self.pictures.join(format!("tab-{tab}.png"))
    }

    fn picture(&self, id: u32) -> Result<(), webview::Error> {
        let Some(tab) = self.tab(id) else { return Ok(()) };
        let done = self.pictures_done.clone();
        tab.view.capture_png_later(&self.picture_path(id), move |written| {
            if written {
                done.borrow_mut().push(id);
            }
        })
    }

    /// Loads `url` in tab `id`, if it's a web address (anything else, nothing: no `file:`, `javascript:`…).
    fn load(&mut self, id: u32, url: &str) {
        if !is_web_address(url) {
            return;
        }
        if let Some(tab) = self.tab(id) {
            let _ = tab.view.navigate(url);
        }
    }

    /// Lets the pages used longest ago go, past [`MOST_LIVE`]: never the one shown, nor one playing sound.
    fn let_go_of_the_oldest(&mut self, stage: &Stage) {
        while self.tabs.len() > MOST_LIVE {
            let shown = self.shown.as_ref().map(|shown| shown.tab);
            let oldest = self
                .tabs
                .iter()
                .enumerate()
                .filter(|(_, tab)| Some(tab.id) != shown && !tab.playing && tab.events.borrow().opened.is_empty())
                .min_by_key(|(_, tab)| tab.used)
                .map(|(at, _)| at);
            let Some(at) = oldest else { break };
            let gone = self.tabs.remove(at);
            let _ = stage.remove(&gone.view);
            self.reports.push(Report::LetGo { tab: gone.id });
        }
    }

    /// A live page for tab `id`, on the frame under the cabin page, hidden, its events followed.
    fn make(&mut self, stage: &Stage, id: u32) -> Result<(), webview::Error> {
        if self.identity.is_none() {
            self.identity = own_identity(stage, &self.environment, &self.pictures);
        }
        let (width, height) = self.size;
        let view = OffscreenWebView::on_screen(stage, width, height, self.environment.clone())?;
        view.show(false)?;
        // A web page's own ground, where it draws none: white, as a browser's.
        view.set_background(255, 255, 255)?;
        if let Some(dark) = self.dark {
            set_colour_scheme(view.core(), dark);
        }
        // The cab's Google sign-in, before any page loads (the profile keeps it: once a start).
        if !self.sign_in.is_empty() {
            let cookies = std::mem::take(&mut self.sign_in);
            match view.add_cookies(&cookies) {
                Ok(()) => log!("[safari] the cab's Google sign-in given ({} cookies)", cookies.len()),
                Err(e) => log!("[safari] couldn't give the cab's Google sign-in: {e}"),
            }
        }
        stage.add(&view, false)?;
        view.add_startup_script(STARTUP_SCRIPT)?;
        // The page thinks itself in front (it is, on the cab's screen): a field pressed in it takes the typing.
        view.devtools("Emulation.setFocusEmulationEnabled", r#"{"enabled":true}"#)?;
        // No scrollbars, as iPad Safari has none: a desktop's took its width off the page's window, a page sized to
        // the whole window (Google's footer) then ran past its side by it, and a scrollbar showed for that too
        // (owner 2026-11-09: "why the fuck do i have a horizontal scrollbar?").
        view.devtools("Emulation.setScrollbarsHidden", r#"{"hidden":true}"#)?;
        // Chrome of the same build, as the pages see it: WebView2 says Microsoft Edge (its user agent's "Edg/", its
        // brands' "Microsoft Edge WebView2"), and Google asks an Edge to download Chrome (owner 2026-11-08: "huge red
        // flag that should be blocked"). The engine is Chromium either way.
        let identity = identity_of(self.identity.as_ref(), &view.version, self.language.as_deref(), false);
        view.devtools("Emulation.setUserAgentOverride", &identity)?;
        #[cfg(test)]
        {
            if let Some(site) = &self.test_site {
                view.map_folder_to_any_page("westplay.safari-test", site)?;
            }
            // Tests never heard: a page's sound plays (and is paused and played on as asked), but none of it leaves
            // for the PC's speakers (owner 2026-11-08).
            view.set_muted(true)?;
        }
        let events = Rc::new(RefCell::new(Events::default()));
        follow(view.core(), &events)?;
        self.used += 1;
        self.tabs.push(Tab {
            id,
            view,
            events,
            field: None,
            theme: String::new(),
            ground: String::new(),
            playing: false,
            told: None,
            used: self.used,
            viewport: None,
            given: None,
            site: String::new(),
            mobile: false,
            scheme_reloads: 0,
            list: None,
            scale: 1.0,
        });
        self.set_mobile(id, true);
        let (width, height, zoom) = (self.size.0, self.size.1, self.zoom);
        if let Some(tab) = self.tab_mut(id) {
            fit_to_width(tab, width, height, zoom);
        }
        Ok(())
    }
}

/// The tab's page laid out in Safari's frame (`width` x `height` pixels at the cab's `zoom`): [`lay_out`], again as
/// the frame changes.
fn fit_to_width(tab: &mut Tab, width: u32, height: u32, zoom: f64) {
    let zoom = if zoom.is_finite() { zoom.clamp(0.25, 5.0) } else { 1.0 };
    if tab.viewport == Some((width, height, zoom)) {
        return;
    }
    tab.viewport = Some((width, height, zoom));
    // The page's zoom is its window's pixel ratio here, not the browser's own zoom on top of it.
    let _ = tab.view.set_zoom(1.0);
    lay_out(tab);
}

/// The tab's page given a window of Safari's frame: as wide as the frame in CSS pixels at the cab's zoom (its pixel
/// ratio), never wider (a desktop's page wider than it is asked for as a phone's instead: [`Safari::set_mobile`]); a
/// phone's window for a phone's identity. Its screen, window and pixel ratio agree, as bot checks look for. Given
/// again only as it changes.
fn lay_out(tab: &mut Tab) {
    let Some((width, height, zoom)) = tab.viewport else { return };
    // Whole CSS pixels covering the frame exactly: the width's at the zoom, the pixel ratio made to fit it, the height
    // the frame's at least. Rounded at the zoom itself, the page fell short of the frame by part of a pixel, the
    // WebView's white showing there as lines along a dark page's edges (owner 2026-11-09).
    let wide = (f64::from(width) / zoom).round().max(1.0);
    let ratio = f64::from(width) / wide;
    let given = (wide, (f64::from(height) / ratio).ceil(), ratio, tab.mobile);
    if tab.given == Some(given) {
        return;
    }
    tab.given = Some(given);
    // The pixel ratio as the WebView's own drawing scale, as a monitor's: the frame the cab gets is drawn at it. Given
    // to the emulated window instead (`deviceScaleFactor`), the page only said it: WebView2 drew the window at one
    // frame pixel a CSS pixel, the rest of the frame white (measured in the frame captured as the cab gets it,
    // owner 2026-11-09: white bands at the page's right and bottom, the page too small). The window's own scale left
    // as the WebView's (0).
    let _ = tab.view.set_rasterization_scale(given.2);
    let metrics = serde_json::json!({
        "width": given.0,
        "height": given.1,
        "deviceScaleFactor": 0,
        "mobile": given.3,
    });
    let _ = tab.view.devtools("Emulation.setDeviceMetricsOverride", &metrics.to_string());
}

/// The tab as the cab is told it, from its WebView and what its page said.
fn tab_state(tab: &Tab) -> TabState {
    let view = tab.view.core();
    let events = tab.events.borrow();
    let text = |read: &dyn Fn(&mut PWSTR) -> windows::core::Result<()>| {
        let mut value = PWSTR::null();
        if read(&mut value).is_err() || value.is_null() {
            return String::new();
        }
        let mut text = webview2_com::CoTaskMemPWSTR::from(value).to_string();
        cut(&mut text, TEXT_MOST);
        text
    };
    let flag = |read: &dyn Fn(&mut BOOL) -> windows::core::Result<()>| {
        let mut value = BOOL::default();
        read(&mut value).is_ok() && value.as_bool()
    };
    // SAFETY: the tab's own WebView, alive while it is; WebView2 writes the values (strings CoTaskMem, freed
    // by `CoTaskMemPWSTR`).
    unsafe {
        TabState {
            url: text(&|value| view.Source(value)),
            title: text(&|value| view.DocumentTitle(value)),
            loading: events.loading,
            back: flag(&|value| view.CanGoBack(value)),
            forward: flag(&|value| view.CanGoForward(value)),
            favicon: view
                .cast::<ICoreWebView2_15>()
                .map(|view| text(&|value| view.FaviconUri(value)))
                .unwrap_or_default(),
            theme: tab.theme.clone(),
            ground: tab.ground.clone(),
            full_screen: flag(&|value| view.ContainsFullScreenElement(value)),
            painted: events.painted,
            field: tab.field.clone(),
            list: tab.list.clone(),
            failed: events.failed.clone(),
        }
    }
}

/// Follows a tab's page: its navigation, title, history, icon and full screen; the windows, dialogs, downloads,
/// other apps and permissions it asks for.
fn follow(view: &ICoreWebView2, events: &Rc<RefCell<Events>>) -> windows::core::Result<()> {
    let mut token = 0;
    // SAFETY: every handler and token is a local for its call; WebView2 keeps its own reference to each handler,
    // which holds its own `Rc` of the events; the arguments are the events', alive through each callback.
    unsafe {
        let settings = view.Settings()?;
        // Its dialogs the cab's own (the page's would be windows on the PC's desktop).
        settings.SetAreDefaultScriptDialogsEnabled(false)?;
        if let Ok(settings) = settings.cast::<ICoreWebView2Settings4>() {
            // Autofill's suggestions and "save password?" are windows of their own too.
            let _ = settings.SetIsPasswordAutosaveEnabled(false);
            let _ = settings.SetIsGeneralAutofillEnabled(false);
        }

        let changed = events.clone();
        view.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, _| {
                let mut events = changed.borrow_mut();
                events.loading = true;
                events.failed = None;
                events.changed = true;
                Ok(())
            })),
            &mut token,
        )?;
        if let Ok(view) = view.cast::<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2>() {
            let drawn = events.clone();
            view.add_DOMContentLoaded(
                &webview2_com::DOMContentLoadedEventHandler::create(Box::new(move |_, _| {
                    let mut events = drawn.borrow_mut();
                    if !events.painted {
                        events.painted = true;
                        events.changed = true;
                    }
                    Ok(())
                })),
                &mut token,
            )?;
        }
        let changed = events.clone();
        view.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |_, args| {
                let mut events = changed.borrow_mut();
                events.loading = false;
                events.painted = true;
                events.changed = true;
                if let Some(args) = args {
                    let mut success = BOOL::default();
                    let _ = args.IsSuccess(&mut success);
                    let mut status = Default::default();
                    let _ = args.WebErrorStatus(&mut status);
                    // The server's answer, if one came (0: none). WebView2 calls a page with an error status a failure
                    // (Google's "unusual traffic" page with its CAPTCHA comes as 429, owner 2026-11-08), but the page
                    // is the server's own and shows: only a page no server sent (no network, a name not found, a
                    // connection cut) is Safari's "can't open the page".
                    let mut http = 0;
                    if let Ok(args) = args.cast::<ICoreWebView2NavigationCompletedEventArgs2>() {
                        let _ = args.HttpStatusCode(&mut http);
                    }
                    events.failed = failure(success.as_bool(), status, http).then(|| webview::web_error(status));
                }
                Ok(())
            })),
            &mut token,
        )?;
        type Add = fn(&ICoreWebView2, Rc<RefCell<Events>>, &mut i64) -> windows::core::Result<()>;
        let adds: [Add; 4] = [
            |view, events, token| {
                view.add_SourceChanged(
                    &SourceChangedEventHandler::create(Box::new(move |_, _| {
                        events.borrow_mut().changed = true;
                        Ok(())
                    })),
                    token,
                )
            },
            |view, events, token| {
                view.add_DocumentTitleChanged(
                    &DocumentTitleChangedEventHandler::create(Box::new(move |_, _| {
                        events.borrow_mut().changed = true;
                        Ok(())
                    })),
                    token,
                )
            },
            |view, events, token| {
                view.add_HistoryChanged(
                    &HistoryChangedEventHandler::create(Box::new(move |_, _| {
                        events.borrow_mut().changed = true;
                        Ok(())
                    })),
                    token,
                )
            },
            |view, events, token| {
                view.add_ContainsFullScreenElementChanged(
                    &ContainsFullScreenElementChangedEventHandler::create(Box::new(move |_, _| {
                        events.borrow_mut().changed = true;
                        Ok(())
                    })),
                    token,
                )
            },
        ];
        for add in adds {
            add(view, events.clone(), &mut token)?;
        }
        if let Ok(view) = view.cast::<ICoreWebView2_15>() {
            let changed = events.clone();
            view.add_FaviconChanged(
                &FaviconChangedEventHandler::create(Box::new(move |_, _| {
                    changed.borrow_mut().changed = true;
                    Ok(())
                })),
                &mut token,
            )?;
        }
        let closed = events.clone();
        view.add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                closed.borrow_mut().closed = true;
                Ok(())
            })),
            &mut token,
        )?;
        let opened = events.clone();
        view.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    // The new tab is made after this (WebView2 waits for it).
                    let deferral = args.GetDeferral()?;
                    opened.borrow_mut().opened.push((args, deferral));
                }
                Ok(())
            })),
            &mut token,
        )?;
        let asked = events.clone();
        view.add_ScriptDialogOpening(
            &ScriptDialogOpeningEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_SCRIPT_DIALOG_KIND::default();
                args.Kind(&mut kind)?;
                let Some(kind) = dialog_kind(kind) else {
                    // A page asking to stay as it's left: let go (a car screen asks nothing of the kind).
                    return args.Accept();
                };
                let mut events = asked.borrow_mut();
                if events.dialog.is_some() {
                    // One at a time: a page asking again before the answer gets none.
                    return Ok(());
                }
                let read = |read: &dyn Fn(&mut PWSTR) -> windows::core::Result<()>| {
                    let mut value = PWSTR::null();
                    if read(&mut value).is_err() || value.is_null() {
                        return String::new();
                    }
                    let mut text = webview2_com::CoTaskMemPWSTR::from(value).to_string();
                    cut(&mut text, TEXT_MOST);
                    text
                };
                let message = read(&|value| args.Message(value));
                let text = (kind == "prompt").then(|| read(&|value| args.DefaultText(value)));
                let deferral = args.GetDeferral()?;
                events.dialog = Some((args, deferral, Dialog { kind, message, text }));
                events.dialog_told = false;
                Ok(())
            })),
            &mut token,
        )?;
        view.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                }
                Ok(())
            })),
            &mut token,
        )?;
        if let Ok(view) = view.cast::<ICoreWebView2_4>() {
            let downloads = events.clone();
            view.add_DownloadStarting(
                &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        args.SetCancel(true)?;
                        args.SetHandled(true)?;
                        downloads.borrow_mut().downloads += 1;
                    }
                    Ok(())
                })),
                &mut token,
            )?;
        }
        if let Ok(view) = view.cast::<ICoreWebView2_18>() {
            view.add_LaunchingExternalUriScheme(
                &LaunchingExternalUriSchemeEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        args.SetCancel(true)?;
                    }
                    Ok(())
                })),
                &mut token,
            )?;
        }
    }
    Ok(())
}

/// The colour scheme the pages of `view`'s profile see as the system's: dark or light.
fn set_colour_scheme(view: &ICoreWebView2, dark: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK, COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT,
    };
    let scheme =
        if dark { COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK } else { COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT };
    // SAFETY: the WebView's own profile, alive while it is; a plain value.
    let set =
        unsafe { view.cast::<ICoreWebView2_13>().and_then(|view| view.Profile()?.SetPreferredColorScheme(scheme)) };
    if let Err(e) = set {
        log!("[safari] couldn't give the pages the cab's appearance: {e}");
    }
}

/// Whether a navigation ended without a page of the server's to show: failed (`success` false), not stopped by the
/// next one, and no answer from a server (`http` 0; one with an error status still sent its page).
fn failure(
    success: bool,
    status: webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_ERROR_STATUS,
    http: i32,
) -> bool {
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED;
    !success && status != COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED && http == 0
}

/// The browser's own identity as a page sees it: its user agent, and what `navigator.userAgentData` says of it in
/// full (its brands, the full versions of each: Chromium's own build among them, which nothing on disk says).
#[derive(Clone, Debug, PartialEq)]
struct Identity {
    user_agent: String,
    /// `getHighEntropyValues`' answer: `brands`, `fullVersionList`, `platform`, `platformVersion`, `architecture`,
    /// `bitness`, `model`, `mobile`, `wow64`.
    data: serde_json::Value,
}

/// The browser's own identity, read from a page of WestPlay's own (an https one: a browser tells its brands' versions
/// only to a secure page), in a WebView of Safari's browser made for it and let go; `None` if it can't be read.
fn own_identity(stage: &Stage, environment: &ICoreWebView2Environment, folder: &Path) -> Option<Identity> {
    let read = || -> Result<Identity, webview::Error> {
        std::fs::create_dir_all(folder)?;
        std::fs::write(folder.join("identity.html"), "<!doctype html><title>identity</title>")?;
        let view = OffscreenWebView::on_screen(stage, 64, 64, environment.clone())?;
        view.map_folder("westplay-identity.invalid", folder)?;
        view.navigate("https://westplay-identity.invalid/identity.html")?;
        let ask = r#"{"expression":"(async () => location.protocol === 'https:' && navigator.userAgentData ? JSON.stringify({ userAgent: navigator.userAgent, data: await navigator.userAgentData.getHighEntropyValues(['fullVersionList', 'platformVersion', 'architecture', 'bitness', 'model', 'wow64']) }) : '')()","awaitPromise":true,"returnByValue":true}"#;
        // The page loads in a moment: asked again till it has (each ask lets the browser's messages through).
        let started = std::time::Instant::now();
        while started.elapsed() < std::time::Duration::from_secs(5) {
            let answer: serde_json::Value = serde_json::from_str(&view.devtools_answer("Runtime.evaluate", ask)?)?;
            if let Some(said) = answer["result"]["value"].as_str().filter(|said| !said.is_empty()) {
                let said: serde_json::Value = serde_json::from_str(said)?;
                let user_agent = said["userAgent"].as_str().unwrap_or_default().to_owned();
                if user_agent.is_empty() || !said["data"]["fullVersionList"].is_array() {
                    break;
                }
                return Ok(Identity { user_agent, data: said["data"].clone() });
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Err("the browser's identity not read".into())
    };
    match read() {
        Ok(identity) => Some(identity),
        Err(e) => {
            log!("[safari] the browser's own identity not read ({e}): Chrome of its version as best known");
            None
        }
    }
}

/// DevTools' `Emulation.setUserAgentOverride` for Chrome of the browser's own build: what Chrome of it says
/// (Cloudflare's check fails a browser saying what no Chrome does, owner 2026-11-08), from the browser's own identity
/// ([`own_identity`]) with Microsoft Edge's names taken out: its user agent less "Edg/", its brands Chromium's,
/// Google Chrome's (Chromium's version, as Chrome's is) and the made-up one each Chromium says ("Not:A-Brand",
/// whose name, version and place follow from its version), each version as the browser gives it. Without it read:
/// Chrome of `version` (WebView2's own), the made-up brand as Chromium makes it. `language`: the cab's (`tr-TR`),
/// asked for first, then its language alone and English (none: English).
fn chrome_identity(identity: Option<&Identity>, version: &str, language: Option<&str>) -> String {
    let edge = |brand: &str| brand.starts_with("Microsoft Edge");
    let (user_agent, data) = match identity {
        Some(identity) => {
            let mut data = identity.data.clone();
            let chromium = |list: &serde_json::Value| {
                list.as_array()
                    .and_then(|list| list.iter().find(|brand| brand["brand"] == "Chromium"))
                    .and_then(|brand| brand["version"].as_str())
                    .map(str::to_owned)
            };
            let full = chromium(&data["fullVersionList"]);
            for (list, version) in [("brands", chromium(&data["brands"])), ("fullVersionList", full.clone())] {
                let Some(brands) = data[list].as_array() else { continue };
                let mut chrome = Vec::new();
                let mut named = false;
                for brand in brands {
                    let name = brand["brand"].as_str().unwrap_or_default();
                    if edge(name) {
                        // Microsoft Edge's place, Google Chrome's (once; WebView2's own name gone).
                        if !named && let Some(version) = &version {
                            chrome.push(serde_json::json!({ "brand": "Google Chrome", "version": version }));
                            named = true;
                        }
                    } else {
                        chrome.push(brand.clone());
                    }
                }
                data[list] = serde_json::Value::Array(chrome);
            }
            if let Some(full) = full {
                data["fullVersion"] = serde_json::Value::String(full);
            }
            let user_agent = match identity.user_agent.find(" Edg/") {
                Some(at) => identity.user_agent[..at].to_owned(),
                None => identity.user_agent.clone(),
            };
            (user_agent, data)
        }
        None => {
            let major = version.split('.').next().and_then(|major| major.parse::<u32>().ok()).unwrap_or(142);
            let (grease, grease_version) = grease_brand(major);
            let ours = [
                ("Chromium", major.to_string(), format!("{major}.0.0.0")),
                ("Google Chrome", major.to_string(), format!("{major}.0.0.0")),
                (grease.as_str(), grease_version.to_owned(), format!("{grease_version}.0.0.0")),
            ];
            let order = grease_order(major);
            let list = |full: bool| {
                serde_json::Value::Array(
                    order
                        .iter()
                        .map(|&at| {
                            let (brand, version, full_version) = &ours[at];
                            serde_json::json!({ "brand": brand, "version": if full { full_version } else { version } })
                        })
                        .collect(),
                )
            };
            let data = serde_json::json!({
                "brands": list(false),
                "fullVersionList": list(true),
                "fullVersion": format!("{major}.0.0.0"),
                "platform": "Windows",
                "platformVersion": "10.0.0",
                "architecture": "x86",
                "bitness": "64",
                "model": "",
                "mobile": false,
                "wow64": false,
            });
            let user_agent = format!(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{major}.0.0.0 Safari/537.36"
            );
            (user_agent, data)
        }
    };
    let field = |name: &str, or: serde_json::Value| if data[name].is_null() { or } else { data[name].clone() };
    serde_json::json!({
        "userAgent": user_agent,
        "acceptLanguage": accept_language(language),
        "platform": "Win32",
        "userAgentMetadata": {
            "brands": field("brands", serde_json::json!([])),
            "fullVersionList": field("fullVersionList", serde_json::json!([])),
            "fullVersion": field("fullVersion", serde_json::json!("")),
            "platform": field("platform", serde_json::json!("Windows")),
            "platformVersion": field("platformVersion", serde_json::json!("10.0.0")),
            "architecture": field("architecture", serde_json::json!("x86")),
            "bitness": field("bitness", serde_json::json!("64")),
            "model": field("model", serde_json::json!("")),
            "mobile": field("mobile", serde_json::json!(false)),
            "wow64": field("wow64", serde_json::json!(false)),
        },
    })
    .to_string()
}

/// The made-up brand Chromium of `major` says among its own (its name and version), as Chromium makes it
/// (`GetGreasedUserAgentBrandVersion`: the characters and version picked by the major version).
fn grease_brand(major: u32) -> (String, &'static str) {
    const CHARACTERS: [char; 11] = [' ', '(', ':', '-', '.', '/', ')', ';', '=', '?', '_'];
    const VERSIONS: [&str; 3] = ["8", "99", "24"];
    let at = major as usize;
    let name = format!("Not{}A{}Brand", CHARACTERS[at % CHARACTERS.len()], CHARACTERS[(at + 1) % CHARACTERS.len()]);
    (name, VERSIONS[at % VERSIONS.len()])
}

/// The order Chromium of `major` says its brands in: indices into (Chromium, the browser's own, the made-up one).
fn grease_order(major: u32) -> [usize; 3] {
    // Chromium's six orders, picked by the major version (`GenerateBrandVersionList`): where the made-up brand,
    // Chromium and the browser's own go.
    const ORDERS: [[usize; 3]; 6] = [[2, 0, 1], [2, 1, 0], [0, 2, 1], [1, 2, 0], [0, 1, 2], [1, 0, 2]];
    let places = ORDERS[major as usize % ORDERS.len()];
    // `places[i]`: where brand i goes (0 the made-up one, 1 Chromium, 2 the browser's own).
    let mut order = [0; 3];
    for (brand, place) in [(2usize, places[0]), (0, places[1]), (1, places[2])] {
        order[place] = brand;
    }
    order
}

/// The identity a tab's pages are asked for with: Chrome of the browser's own build, on Windows
/// ([`chrome_identity`]) or, `mobile`, on an Android phone ([`phone_identity`]).
fn identity_of(identity: Option<&Identity>, version: &str, language: Option<&str>, mobile: bool) -> String {
    let desktop = chrome_identity(identity, version, language);
    if mobile { phone_identity(&desktop) } else { desktop }
}

/// Chrome of the same build on an Android phone, from its desktop identity (`desktop`, [`chrome_identity`]'s): its
/// user agent as Chrome on Android gives it (the reduced one, as every Chrome now sends), Android its platform, a
/// phone (`mobile`), the brands and versions the same.
fn phone_identity(desktop: &str) -> String {
    let Ok(mut said) = serde_json::from_str::<serde_json::Value>(desktop) else { return desktop.to_owned() };
    let major = said["userAgentMetadata"]["brands"]
        .as_array()
        .and_then(|brands| brands.iter().find(|brand| brand["brand"] == "Chromium"))
        .and_then(|brand| brand["version"].as_str())
        .unwrap_or("142")
        .to_owned();
    said["userAgent"] = serde_json::Value::String(format!(
        "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{major}.0.0.0 Mobile Safari/537.36"
    ));
    said["platform"] = serde_json::json!("Linux armv81");
    let meta = &mut said["userAgentMetadata"];
    meta["platform"] = serde_json::json!("Android");
    meta["platformVersion"] = serde_json::json!("14.0.0");
    meta["architecture"] = serde_json::json!("");
    meta["bitness"] = serde_json::json!("");
    meta["model"] = serde_json::json!("");
    meta["mobile"] = serde_json::json!(true);
    meta["wow64"] = serde_json::json!(false);
    said.to_string()
}

/// The languages a page is asked for in, best first: the cab's (`tr-TR`), its language alone (`tr`), then English.
fn accept_language(language: Option<&str>) -> String {
    let mut wanted: Vec<String> = Vec::new();
    if let Some(language) = language {
        wanted.push(language.to_owned());
        if let Some((alone, _)) = language.split_once('-') {
            wanted.push(alone.to_owned());
        }
    }
    for english in ["en-US", "en"] {
        if !wanted.iter().any(|known| known.eq_ignore_ascii_case(english)) {
            wanted.push(english.to_owned());
        }
    }
    wanted.join(",")
}

/// A language tag as the cab gives it (`tr-TR`, `zh-Hans-CN`): letters, then parts of letters or digits after `-`.
fn is_language_tag(tag: &str) -> bool {
    let mut parts = tag.split('-');
    parts.next().is_some_and(|first| (2..=3).contains(&first.len()) && first.chars().all(|c| c.is_ascii_alphabetic()))
        && parts.all(|part| (1..=8).contains(&part.len()) && part.chars().all(|c| c.is_ascii_alphanumeric()))
        && tag.len() <= 35
}

/// A dialog's kind as the cab names it; `None` for a page asking to stay as it's left.
fn dialog_kind(kind: COREWEBVIEW2_SCRIPT_DIALOG_KIND) -> Option<&'static str> {
    match kind {
        COREWEBVIEW2_SCRIPT_DIALOG_KIND_ALERT => Some("alert"),
        COREWEBVIEW2_SCRIPT_DIALOG_KIND_CONFIRM => Some("confirm"),
        COREWEBVIEW2_SCRIPT_DIALOG_KIND_PROMPT => Some("prompt"),
        COREWEBVIEW2_SCRIPT_DIALOG_KIND_BEFOREUNLOAD => None,
        _ => None,
    }
}

/// A web address the cab may load: `http:` or `https:` (the cab makes a search or an address of what's typed).
fn is_web_address(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

/// The key's DevTools events, as a keyboard sends them.
fn key_events(key: Key) -> Vec<String> {
    let (name, code, text) = match key {
        Key::Backspace => ("Backspace", 8, ""),
        Key::Enter => ("Enter", 13, "\r"),
    };
    let down = if text.is_empty() {
        serde_json::json!({ "type": "rawKeyDown", "key": name, "code": name, "windowsVirtualKeyCode": code })
    } else {
        serde_json::json!({ "type": "keyDown", "key": name, "code": name, "windowsVirtualKeyCode": code, "text": text, "unmodifiedText": text })
    };
    let up = serde_json::json!({ "type": "keyUp", "key": name, "code": name, "windowsVirtualKeyCode": code });
    vec![down.to_string(), up.to_string()]
}

/// `text` cut to at most `most` bytes, at a character's edge.
fn cut(text: &mut String, most: usize) {
    if text.len() > most {
        let mut at = most;
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        text.truncate(at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_safari_apps_commands() {
        let read = |json: &str| serde_json::from_str::<Command>(json).unwrap();
        assert_eq!(
            read(
                r#"{"action":"show","tab":3,"url":"https://example.com/","x":120,"y":80,"width":900,"height":540,"zoom":1.25}"#
            ),
            Command::Show {
                tab: 3,
                url: "https://example.com/".into(),
                x: 120.0,
                y: 80.0,
                width: 900,
                height: 540,
                zoom: 1.25,
                covers: Vec::new(),
                dark: None,
                radius: 0.0,
                language: None
            }
        );
        assert_eq!(read(r#"{"action":"key","key":"backspace"}"#), Command::Key { key: Key::Backspace });
        assert_eq!(
            read(r#"{"action":"answer","tab":2,"accept":true,"text":"hi"}"#),
            Command::Answer { tab: 2, accept: true, text: Some("hi".into()) }
        );
        assert_eq!(read(r#"{"action":"forgetWebsites"}"#), Command::ForgetWebsites { since: 0.0 });
        assert_eq!(
            read(r#"{"action":"forgetWebsites","since":1762560000000}"#),
            Command::ForgetWebsites { since: 1_762_560_000_000.0 }
        );
        assert_eq!(read(r#"{"action":"exitFullScreen","tab":1}"#), Command::ExitFullScreen { tab: 1 });
    }

    #[test]
    fn a_page_a_server_sent_shows_whatever_its_status() {
        use webview2_com::Microsoft::Web::WebView2::Win32::*;
        // Google's CAPTCHA page (429), a 404 page: the server's own pages, shown.
        assert!(!failure(false, COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN, 429));
        assert!(!failure(false, COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN, 404));
        // No answer from any server: Safari's "can't open the page".
        assert!(failure(false, COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED, 0));
        assert!(failure(false, COREWEBVIEW2_WEB_ERROR_STATUS_DISCONNECTED, 0));
        // Stopped by the next navigation, or done: nothing to say.
        assert!(!failure(false, COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED, 0));
        assert!(!failure(true, COREWEBVIEW2_WEB_ERROR_STATUS_UNKNOWN, 200));
    }

    /// Chrome of the browser's own build, from what the browser says of itself (WebView2 156's, as read
    /// 2026-11-08): Microsoft Edge's names taken out, Google Chrome in Edge's place at Chromium's versions, the
    /// made-up brand as the browser says it.
    #[test]
    fn says_chrome_of_the_browsers_own_build() {
        let identity = Identity {
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
                         Chrome/156.0.0.0 Safari/537.36 Edg/156.0.0.0"
                .into(),
            data: serde_json::json!({
                "architecture": "x86", "bitness": "64", "mobile": false, "model": "", "platform": "Windows",
                "platformVersion": "10.0.0", "wow64": false,
                "brands": [
                    { "brand": "Chromium", "version": "156" }, { "brand": "Microsoft Edge", "version": "156" },
                    { "brand": "Not:A-Brand", "version": "8" }, { "brand": "Microsoft Edge WebView2", "version": "156" },
                ],
                "fullVersionList": [
                    { "brand": "Chromium", "version": "156.0.8078.80" },
                    { "brand": "Microsoft Edge", "version": "156.0.4314.58" },
                    { "brand": "Not:A-Brand", "version": "8.0.0.0" },
                    { "brand": "Microsoft Edge WebView2", "version": "156.0.4314.58" },
                ],
            }),
        };
        let said: serde_json::Value =
            serde_json::from_str(&chrome_identity(Some(&identity), "156.0.4314.58", Some("tr-TR"))).unwrap();
        assert_eq!(
            said["userAgent"],
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36"
        );
        assert_eq!(said["acceptLanguage"], "tr-TR,tr,en-US,en");
        let meta = &said["userAgentMetadata"];
        assert_eq!(
            meta["brands"],
            serde_json::json!([
                { "brand": "Chromium", "version": "156" }, { "brand": "Google Chrome", "version": "156" },
                { "brand": "Not:A-Brand", "version": "8" },
            ])
        );
        assert_eq!(
            meta["fullVersionList"],
            serde_json::json!([
                { "brand": "Chromium", "version": "156.0.8078.80" },
                { "brand": "Google Chrome", "version": "156.0.8078.80" },
                { "brand": "Not:A-Brand", "version": "8.0.0.0" },
            ])
        );
        assert_eq!(meta["fullVersion"], "156.0.8078.80");
        // Not read: the made-up brand and the order as the browser says them for 156 (above).
        let said: serde_json::Value = serde_json::from_str(&chrome_identity(None, "156.0.4314.58", None)).unwrap();
        assert_eq!(
            said["userAgentMetadata"]["brands"],
            serde_json::json!([
                { "brand": "Chromium", "version": "156" }, { "brand": "Google Chrome", "version": "156" },
                { "brand": "Not:A-Brand", "version": "8" },
            ])
        );
    }

    #[test]
    fn says_a_phones_chrome_of_the_same_build() {
        let desktop = chrome_identity(None, "156.0.4314.58", Some("tr-TR"));
        let phone: serde_json::Value = serde_json::from_str(&phone_identity(&desktop)).unwrap();
        let desktop: serde_json::Value = serde_json::from_str(&desktop).unwrap();
        assert_eq!(
            phone["userAgent"],
            "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Mobile Safari/537.36"
        );
        assert_eq!(phone["userAgentMetadata"]["platform"], "Android");
        assert_eq!(phone["userAgentMetadata"]["mobile"], true);
        assert_eq!(phone["userAgentMetadata"]["brands"], desktop["userAgentMetadata"]["brands"]);
        assert_eq!(phone["userAgentMetadata"]["fullVersionList"], desktop["userAgentMetadata"]["fullVersionList"]);
        assert_eq!(phone["acceptLanguage"], desktop["acceptLanguage"]);
    }

    #[test]
    fn asks_for_pages_in_the_cabs_language_then_english() {
        assert_eq!(accept_language(Some("tr-TR")), "tr-TR,tr,en-US,en");
        assert_eq!(accept_language(Some("en-US")), "en-US,en");
        assert_eq!(accept_language(None), "en-US,en");
        assert!(is_language_tag("tr-TR") && is_language_tag("zh-Hans-CN") && is_language_tag("es-419"));
        assert!(!is_language_tag("tr TR") && !is_language_tag("x") && !is_language_tag("tr-TR,en;q=0.5"));
    }

    #[test]
    fn loads_only_web_addresses() {
        assert!(is_web_address("https://example.com"));
        assert!(is_web_address("HTTP://example.com"));
        for not in
            ["file:///C:/Windows", "javascript:alert(1)", "data:text/html,hi", "steam://run/227300", "about:blank"]
        {
            assert!(!is_web_address(not), "{not}");
        }
    }

    #[test]
    fn tells_the_cab_a_tab_without_what_it_lacks() {
        let state = TabState { url: "https://example.com/".into(), title: "Example".into(), ..Default::default() };
        let message = Report::Tab { tab: 1, state: Box::new(state) }.message();
        assert_eq!(
            message,
            r#"{"state":{"back":false,"forward":false,"fullScreen":false,"loading":false,"title":"Example","url":"https://example.com/"},"tab":1,"type":"safariTab"}"#
        );
    }

    /// Runs this thread's messages for `seconds` (WebView2's work comes through them).
    fn pump(seconds: f64) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
        while std::time::Instant::now() < until {
            crate::pump_messages();
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
    }

    /// A click of the player's mouse at `(x, y)` of the tab's page, its CSS pixels, as pointer.rs sends it (DevTools'
    /// mouse events, no keys held).
    fn click(view: &OffscreenWebView, x: i32, y: i32) {
        for kind in ["mouseMoved", "mousePressed", "mouseReleased"] {
            let event =
                serde_json::json!({ "type": kind, "x": x, "y": y, "button": "left", "clickCount": 1, "modifiers": 0 });
            view.devtools("Input.dispatchMouseEvent", &event.to_string()).unwrap();
            pump(0.05);
        }
    }

    /// A short tone, as a WAV file (16-bit mono, 44.1 kHz).
    fn tone(seconds: f32) -> Vec<u8> {
        let rate = 44_100u32;
        let samples: Vec<i16> = (0..(rate as f32 * seconds) as u32)
            .map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 8000.0) as i16)
            .collect();
        let data = (samples.len() * 2) as u32;
        let mut wav = Vec::new();
        for part in [&b"RIFF"[..], &(36 + data).to_le_bytes(), b"WAVEfmt ", &16u32.to_le_bytes(), &1u16.to_le_bytes()] {
            wav.extend_from_slice(part);
        }
        for part in [&1u16.to_le_bytes()[..], &rate.to_le_bytes(), &(rate * 2).to_le_bytes(), &2u16.to_le_bytes()] {
            wav.extend_from_slice(part);
        }
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        for sample in samples {
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        wav
    }

    /// A tab as the player uses it, in a real WebView2, on a page of its own (no network): the field pressed is
    /// told and typed into (the cab's keyboard: text, Backspace, Enter), a window the page opens is a new tab and
    /// closing itself says so, a dialog waits for the answer, the page's sound is told and paused and played on.
    #[test]
    #[ignore = "runs WebView2"]
    fn a_tab_types_opens_windows_asks_and_plays() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-safari-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(site.join("tone.wav"), tone(8.0)).unwrap();
        std::fs::write(
            site.join("index.html"),
            r#"<!doctype html><title>Test page</title>
            <form id="form" onsubmit="event.preventDefault(); window.submitted = q.value">
              <input id="q" style="position:absolute;left:10px;top:10px;width:300px;height:30px">
            </form>
            <button id="play" style="position:absolute;left:10px;top:60px;width:100px;height:30px"
              onclick="window.sound = new Audio('tone.wav'); sound.play()">play</button>
            <button id="open" style="position:absolute;left:10px;top:110px;width:100px;height:30px"
              onclick="window.child = window.open('/second.html')">open</button>"#,
        )
        .unwrap();
        std::fs::write(site.join("second.html"), "<!doctype html><title>Second</title>").unwrap();

        let stage = Stage::new(800, 600).unwrap();
        let mut safari = Safari::start(800, 600, &root.join("profile"), &root.join("tabs"), Vec::new()).unwrap();
        safari.test_site = Some(site.clone());
        safari.make(&stage, 1).unwrap();
        let show = Command::Show {
            tab: 1,
            url: String::new(),
            x: 0.0,
            y: 0.0,
            width: 800,
            height: 600,
            zoom: 1.0,
            covers: Vec::new(),
            dark: Some(true),
            radius: 24.0,
            language: None,
        };
        safari.command(&stage, &show).unwrap();
        safari
            .command(&stage, &Command::Open { tab: 1, url: "https://westplay.safari-test/index.html".into() })
            .unwrap();
        pump(2.5);
        let reports = safari.take_reports(&stage);
        let state = |reports: &[Report], id: u32| {
            reports.iter().rev().find_map(|report| match report {
                Report::Tab { tab, state } if *tab == id => Some(state.clone()),
                _ => None,
            })
        };
        let loaded = state(&reports, 1).expect("the tab's state");
        assert_eq!(loaded.title, "Test page");
        assert!(loaded.painted, "a loaded page says it has drawn");
        assert_eq!(loaded.url, "https://westplay.safari-test/index.html");
        assert!(!loaded.loading);

        // The field pressed, told; typed into.
        click(&safari.tab(1).unwrap().view, 100, 25);
        pump(0.5);
        let reports = safari.take_reports(&stage);
        let field = state(&reports, 1).and_then(|state| state.field).expect("the field pressed");
        assert_eq!((field.kind.as_str(), field.value.as_str()), ("text", ""));
        safari.command(&stage, &Command::Type { text: "hello".into() }).unwrap();
        pump(0.3);
        safari.command(&stage, &Command::Key { key: Key::Backspace }).unwrap();
        pump(0.3);
        let view = &safari.tab(1).unwrap().view;
        assert_eq!(view.evaluate("q.value").unwrap(), r#""hell""#);
        safari.command(&stage, &Command::Key { key: Key::Enter }).unwrap();
        pump(0.3);
        assert_eq!(safari.tab(1).unwrap().view.evaluate("window.submitted").unwrap(), r#""hell""#);

        // The page's sound: told, paused and played on.
        click(&safari.tab(1).unwrap().view, 60, 75);
        pump(1.0);
        let reports = safari.take_reports(&stage);
        assert!(reports.contains(&Report::Playing(true)), "{reports:?}");
        safari.command(&stage, &Command::Pause).unwrap();
        pump(0.5);
        assert_eq!(safari.tab(1).unwrap().view.evaluate("sound.paused").unwrap(), "true");
        assert!(safari.take_reports(&stage).contains(&Report::Playing(false)));
        safari.command(&stage, &Command::Resume).unwrap();
        pump(0.5);
        assert_eq!(safari.tab(1).unwrap().view.evaluate("sound.paused").unwrap(), "false");
        // The cab's fade: the page sees its own volume, the sound plays at the fade's.
        safari.command(&stage, &Command::Level { level: 0.5 }).unwrap();
        pump(0.3);
        assert_eq!(safari.tab(1).unwrap().view.evaluate("sound.volume").unwrap(), "1");
        safari.command(&stage, &Command::Pause).unwrap();
        pump(0.3);
        safari.take_reports(&stage);

        // A window the page opens: a tab of its own, after the page's; closing itself, said.
        click(&safari.tab(1).unwrap().view, 60, 125);
        pump(2.0);
        let reports = safari.take_reports(&stage);
        let opened = reports
            .iter()
            .find_map(|report| match report {
                Report::Opened { tab, opener: 1 } => Some(*tab),
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "a new tab: {reports:?}; opened: {}",
                    safari.tab(1).unwrap().view.evaluate("String(window.child)").unwrap()
                )
            });
        // Its page loads in it once it's the window's.
        pump(1.5);
        let reports = safari.take_reports(&stage);
        assert_eq!(state(&reports, opened).map(|state| state.title).as_deref(), Some("Second"), "{reports:?}");
        safari.tab(1).unwrap().view.evaluate("child.close()").unwrap();
        pump(0.5);
        assert!(safari.take_reports(&stage).contains(&Report::Closed { tab: opened }));
        safari.command(&stage, &Command::Close { tab: opened }).unwrap();

        // A dialog: told, and the page waits for the answer.
        safari.tab(1).unwrap().view.run_script("window.answer = confirm('Leave?')", |_| ()).unwrap();
        pump(0.5);
        let reports = safari.take_reports(&stage);
        assert!(
            reports.iter().any(|report| matches!(report,
            Report::Dialog { tab: 1, dialog } if dialog.kind == "confirm" && dialog.message == "Leave?")),
            "{reports:?}"
        );
        safari.command(&stage, &Command::Answer { tab: 1, accept: true, text: None }).unwrap();
        pump(0.3);
        assert_eq!(safari.tab(1).unwrap().view.evaluate("window.answer").unwrap(), "true");

        // Clear History and Website Data for the last hour: a cookie the site set just now is gone.
        safari.tab(1).unwrap().view.evaluate("document.cookie = 'signed=in; max-age=3600; path=/'").unwrap();
        assert!(safari.tab(1).unwrap().view.evaluate("document.cookie").unwrap().contains("signed=in"));
        let hour_ago = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as f64
            - 3_600_000.0;
        safari.command(&stage, &Command::ForgetWebsites { since: hour_ago }).unwrap();
        pump(1.5);
        safari.command(&stage, &Command::Reload { tab: 1 }).unwrap();
        pump(1.5);
        assert_eq!(safari.tab(1).unwrap().view.evaluate("document.cookie").unwrap(), "\"\"");

        // The cab's appearance is the pages': dark as shown, light once it turns.
        assert_eq!(
            safari.tab(1).unwrap().view.evaluate("matchMedia('(prefers-color-scheme: dark)').matches").unwrap(),
            "true"
        );
        safari.command(&stage, &Command::Appearance { dark: false }).unwrap();
        pump(0.3);
        assert_eq!(
            safari.tab(1).unwrap().view.evaluate("matchMedia('(prefers-color-scheme: dark)').matches").unwrap(),
            "false"
        );

        // Anything but a web address isn't loaded.
        safari.command(&stage, &Command::Open { tab: 1, url: "file:///C:/Windows/win.ini".into() }).unwrap();
        pump(0.5);
        assert_eq!(state(&safari.take_reports(&stage), 1).map(|state| state.url), None);
        safari.close(&stage);
    }

    /// Real websites load in a tab as the cab asks (network): their address, title, no failure, no download.
    #[test]
    #[ignore = "runs WebView2 on real websites"]
    fn real_websites_load() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-safari-real-{}", std::process::id()));
        let stage = Stage::new(1024, 600).unwrap();
        let mut safari = Safari::start(1024, 600, &root.join("profile"), &root.join("tabs"), Vec::new()).unwrap();
        for (tab, url) in
            [(3, "https://www.google.com/"), (1, "https://example.com/"), (2, "https://www.google.com/search?q=truck")]
        {
            let show = Command::Show {
                tab,
                url: url.into(),
                x: 0.0,
                y: 0.0,
                width: 1024,
                height: 600,
                zoom: 1.3,
                covers: Vec::new(),
                dark: Some(false),
                radius: 24.0,
                language: Some("tr-TR".into()),
            };
            safari.command(&stage, &show).unwrap();
            pump(6.0);
            let reports = safari.take_reports(&stage);
            println!("{url}: {reports:#?}");
            assert!(!reports.contains(&Report::NoDownload { tab }), "{url}: taken for a download");
        }
        // A desktop site wider than Safari (owner 2026-11-08, sahibinden.com: "safari pages must never overflow towards
        // sides"): shown whole in Safari's width, nothing of it past its sides.
        let show = Command::Show {
            tab: 4,
            url: "https://www.sahibinden.com/".into(),
            x: 0.0,
            y: 0.0,
            width: 1024,
            height: 600,
            zoom: 1.3,
            covers: Vec::new(),
            dark: Some(false),
            radius: 24.0,
            language: Some("tr-TR".into()),
        };
        safari.command(&stage, &show).unwrap();
        // What the page says taken as the cab takes it, every frame.
        for _ in 0..40 {
            pump(0.2);
            safari.take_reports(&stage);
        }
        let view = &safari.tab(4).unwrap().view;
        let measured = view
            .evaluate(
                "JSON.stringify([location.href, innerWidth, document.documentElement.scrollWidth,                  Math.round(innerWidth * devicePixelRatio)])",
            )
            .unwrap();
        println!("sahibinden.com: address, window, content width, window in the frame's pixels: {measured}");
        let _ = view.capture_png(&root.join("sahibinden.png"));
        println!("its picture: {}", root.join("sahibinden.png").display());
        let measured: serde_json::Value =
            serde_json::from_str(&serde_json::from_str::<String>(&measured).unwrap()).unwrap();
        let (window, content, in_frame) =
            (measured[1].as_f64().unwrap(), measured[2].as_f64().unwrap(), measured[3].as_f64().unwrap());
        assert!(content <= window + 1.0, "sahibinden.com runs past Safari's sides: {measured}");
        assert!((in_frame - 1024.0).abs() <= 1.0, "its window Safari's frame: {measured}");
        // Google's home: all of its window its own (no scrollbar's width taken off it), nothing past its sides, its
        // phone's page (owner 2026-11-09: a horizontal scrollbar on google.com; every site a phone's).
        let widths = safari
            .tab(3)
            .unwrap()
            .view
            .evaluate(
                "JSON.stringify([innerWidth, document.documentElement.clientWidth, document.documentElement.scrollWidth, \
                 / Mobile /.test(navigator.userAgent)])",
            )
            .unwrap();
        println!("google.com: window, its width less a scrollbar, content, phone's: {widths}");
        let widths: serde_json::Value =
            serde_json::from_str(&serde_json::from_str::<String>(&widths).unwrap()).unwrap();
        assert_eq!(widths[0], widths[1], "no scrollbar: {widths}");
        assert!(widths[2].as_f64() <= widths[1].as_f64(), "nothing past its sides: {widths}");
        assert_eq!(widths[3], true, "google.com a phone's: {widths}");
        // Asked for in the cab's language: Google in Turkish.
        let lang = safari
            .tab(3)
            .unwrap()
            .view
            .evaluate("[document.documentElement.lang, navigator.languages.join(',')].join(' / ')")
            .unwrap();
        println!("google.com's language / the page's languages: {lang}");
        assert!(lang.starts_with("\"tr"), "google.com in Turkish: {lang}");
        // Google's home asks nothing of the kind of a Chrome.
        let home = safari.tab(3).unwrap().view.evaluate("document.body.innerText").unwrap();
        println!("google.com mentions Chrome: {}", home.contains("Chrome"));
        assert!(!home.contains("Chrome"), "google.com still asks for Chrome: {home}");
        // Google sees Chrome (not Edge, which it asks to download Chrome).
        let view = &safari.tab(2).unwrap().view;
        let identity = view
            .evaluate("JSON.stringify([navigator.userAgent, navigator.userAgentData.brands.map((b) => b.brand)])")
            .unwrap();
        println!("as Google sees it: {identity}");
        assert!(identity.contains("Google Chrome") && !identity.contains("Edg") && !identity.contains("Edge"));
        // A search's first result clicked with the mouse, as the cab clicks (owner 2026-11-08: every result opened was
        // taken for a download): it opens.
        let view = &safari.tab(2).unwrap().view;
        view.evaluate("document.querySelector('a h3')?.scrollIntoView({ block: 'center' })").unwrap();
        pump(0.5);
        let at = view
            .evaluate(
                "(() => { const h = document.querySelector('a h3'); if (!h) return null; \
                 const r = h.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2, h.closest('a').href] })()",
            )
            .unwrap();
        println!("first result: {at}");
        let at: serde_json::Value = serde_json::from_str(&at).unwrap();
        if at.is_null() {
            // No results to click: Google's "unusual traffic" check after many searches from this PC.
            let title = view.evaluate("document.title + ' ' + location.href").unwrap();
            println!("no results to click (Google's check?): {title}");
            safari.close(&stage);
            return;
        }
        // Clicked as the cab now clicks a web page: DevTools mouse events, no keys held (pointer.rs).
        let (x, y) = (at[0].as_f64().unwrap(), at[1].as_f64().unwrap());
        for kind in ["mouseMoved", "mousePressed", "mouseReleased"] {
            let event =
                serde_json::json!({ "type": kind, "x": x, "y": y, "button": "left", "clickCount": 1, "modifiers": 0 });
            view.devtools("Input.dispatchMouseEvent", &event.to_string()).unwrap();
            pump(0.05);
        }
        pump(6.0);
        let reports = safari.take_reports(&stage);
        println!("after the click: {reports:#?}");
        assert!(
            !reports.iter().any(|report| matches!(report, Report::NoDownload { .. })),
            "the result taken for a download"
        );
        pump(6.0);
        let reports = safari.take_reports(&stage);
        println!("then: {reports:#?}");
        assert!(
            !reports.iter().any(|report| matches!(report, Report::NoDownload { .. })),
            "the result taken for a download"
        );
        safari.close(&stage);
    }

    /// A sign-in's cookie on one profile (the cab's) given to Safari's as it starts: its first page sends it.
    #[test]
    #[ignore = "runs WebView2"]
    fn safari_is_given_the_cabs_sign_in() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-safari-sign-in-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(site.join("index.html"), "<!doctype html><title>signed</title>").unwrap();
        let stage = Stage::new(400, 300).unwrap();
        // The cab's profile, signed in (a cookie its page set).
        let cab = OffscreenWebView::new(&stage, 400, 300, &root.join("cab")).unwrap();
        cab.map_folder_to_any_page("westplay.safari-test", &site).unwrap();
        cab.navigate("https://westplay.safari-test/index.html").unwrap();
        pump(1.5);
        cab.evaluate("document.cookie = 'SID=signed-in; max-age=3600; path=/; secure'").unwrap();
        pump(0.3);
        let given = sign_in_of(&cab, &["https://westplay.safari-test/"]);
        assert!(given.iter().any(|cookie| cookie.name == "SID" && cookie.value == "signed-in"), "read from the cab's");
        // Safari's own profile, given it: its first page has it.
        let mut safari = Safari::start(400, 300, &root.join("profile"), &root.join("tabs"), given).unwrap();
        safari.test_site = Some(site.clone());
        safari
            .command(&stage, &Command::Open { tab: 1, url: "https://westplay.safari-test/index.html".into() })
            .unwrap();
        pump(1.5);
        let cookie = safari.tab(1).unwrap().view.evaluate("document.cookie").unwrap();
        assert!(cookie.contains("SID=signed-in"), "Safari's page: {cookie}");
        safari.close(&stage);
    }

    /// Every page asked for as a phone's (owner 2026-11-09): a site with a desktop page of its own width serves its
    /// phone's page, at Safari's width; a page taller than its window has no scrollbar and nothing past its sides; a
    /// page made for no particular width is scaled to Safari's width by the browser, a click landing where it's aimed.
    #[test]
    #[ignore = "runs WebView2"]
    fn pages_fit_safaris_width() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-safari-fit-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).unwrap();
        // A site serving a desktop page of its own width, and a phone's that fits any (as sahibinden.com's).
        std::fs::write(
            site.join("site.html"),
            "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>site</title>\
             <body style='margin:0'><script>if (!/ Mobile /.test(navigator.userAgent)) \
             document.body.style.minWidth = '1170px'</script><div style='height:300px;background:#08c'></div>",
        )
        .unwrap();
        // Taller than its window, its footer as wide as the window (as Google's).
        std::fs::write(
            site.join("tall.html"),
            "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>tall</title>\
             <body style='margin:0'><div style='height:2000px'></div><footer style='width:100vw;height:40px;background:#333'>",
        )
        .unwrap();
        std::fs::write(
            site.join("fixed.html"),
            "<!doctype html><title>fixed</title><body style='margin:0'>\
             <div style='display:flex;width:1400px;height:300px'><div style='flex:1;background:red'></div>\
             <button id=right style='width:200px' onclick='window.hit = true'>right</button></div>",
        )
        .unwrap();
        let stage = Stage::new(1040, 560).unwrap();
        let mut safari = Safari::start(1040, 560, &root.join("profile"), &root.join("tabs"), Vec::new()).unwrap();
        safari.test_site = Some(site.clone());
        let measure = "JSON.stringify([innerWidth, document.documentElement.clientWidth, \
                       document.documentElement.scrollWidth, Math.round(innerWidth * devicePixelRatio), \
                       / Mobile /.test(navigator.userAgent), navigator.userAgentData.mobile])";
        let measured = |safari: &Safari, tab: u32| -> serde_json::Value {
            serde_json::from_str(
                &serde_json::from_str::<String>(&safari.tab(tab).unwrap().view.evaluate(measure).unwrap()).unwrap(),
            )
            .unwrap()
        };
        for (tab, page) in [(1, "site.html"), (2, "tall.html"), (3, "fixed.html")] {
            let show = Command::Show {
                tab,
                url: format!("https://westplay.safari-test/{page}"),
                x: 0.0,
                y: 0.0,
                width: 1040,
                height: 560,
                zoom: 1.3,
                covers: Vec::new(),
                dark: Some(false),
                radius: 0.0,
                language: None,
            };
            safari.command(&stage, &show).unwrap();
            for _ in 0..12 {
                pump(0.25);
                safari.take_reports(&stage);
            }
        }
        // Safari's frame is 1040 px wide at the zoom 1.3: 800 CSS pixels, at that zoom.
        let site_page = measured(&safari, 1);
        println!("the site's page: window, less a scrollbar, content, in the frame, phone's: {site_page}");
        assert_eq!(
            site_page,
            serde_json::json!([800, 800, 800, 1040, true, true]),
            "its phone's page at Safari's width"
        );
        let tall = measured(&safari, 2);
        println!("a tall page: {tall}");
        assert_eq!(tall, serde_json::json!([800, 800, 800, 1040, true, true]), "no scrollbar, nothing past its sides");
        // Made for no particular width: scaled to Safari's by the browser, the mouse placed by the scale it says.
        let scale = safari.page_scale(3);
        println!("a page made for no width, its scale: {scale}");
        assert!((scale - 800.0 / 1400.0).abs() < 0.01, "{scale}");
        let view = &safari.tab(3).unwrap().view;
        let in_frame = (1350.0 * 1040.0 / 1400.0, 150.0 * 1040.0 / 1400.0);
        let zoom = safari.page_zoom(3).unwrap();
        let (x, y) = (in_frame.0 / zoom, in_frame.1 / zoom);
        for kind in ["mouseMoved", "mousePressed", "mouseReleased"] {
            let event =
                serde_json::json!({ "type": kind, "x": x, "y": y, "button": "left", "clickCount": 1, "modifiers": 0 });
            view.devtools("Input.dispatchMouseEvent", &event.to_string()).unwrap();
            pump(0.05);
        }
        pump(0.3);
        assert_eq!(view.evaluate("window.hit === true").unwrap(), "true", "the click lands on the button aimed at");
        safari.close(&stage);
    }

    /// A page sees a phone's Chrome, all of it agreeing, as bot checks look (owner 2026-11-08: Cloudflare's check
    /// failed): its window on the screen and the mouse where the window is (not off the desktop, where WebViews' hidden
    /// windows are), in its frames too; a phone's screen, its window as big; `window.chrome` as Chrome's.
    #[test]
    #[ignore = "runs WebView2"]
    fn pages_see_a_phones_chrome() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-safari-desktop-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(
            site.join("page.html"),
            "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>page</title>             <body style='margin:0'>\
             <iframe src='https://westplay.safari-test/inner.html' \
             style='position:absolute;left:100px;top:100px;width:300px;height:100px;border:0'></iframe>",
        )
        .unwrap();
        std::fs::write(
            site.join("inner.html"),
            "<!doctype html><html style='height:100%' onmousedown='parent.postMessage([event.screenX, event.screenY, \
             event.clientX, event.clientY, event.isTrusted, typeof window.chrome?.webview], \"*\")'>",
        )
        .unwrap();
        let stage = Stage::new(1040, 560).unwrap();
        let mut safari = Safari::start(1040, 560, &root.join("profile"), &root.join("tabs"), Vec::new()).unwrap();
        safari.test_site = Some(site.clone());
        let show = Command::Show {
            tab: 1,
            url: "https://westplay.safari-test/page.html".into(),
            x: 0.0,
            y: 0.0,
            width: 1040,
            height: 560,
            zoom: 1.3,
            covers: Vec::new(),
            dark: Some(false),
            radius: 0.0,
            language: None,
        };
        safari.command(&stage, &show).unwrap();
        for _ in 0..8 {
            pump(0.25);
            safari.take_reports(&stage);
        }
        let view = &safari.tab(1).unwrap().view;
        let read = |script: &str| -> serde_json::Value {
            serde_json::from_str(&serde_json::from_str::<String>(&view.evaluate(script).unwrap()).unwrap()).unwrap()
        };
        let seen = read(
            "JSON.stringify({ screen: [screen.width, screen.height], window: [screenX, screenY, outerWidth, outerHeight], \
             page: [innerWidth, innerHeight], chrome: Object.keys(window.chrome ?? {}) })",
        );
        println!("the page sees: {seen}");
        // A phone's: its browser's window its whole screen, the page all of it.
        assert_eq!(seen["window"], serde_json::json!([0, 0, 800, 431]), "its window on the screen, Safari's frame");
        assert_eq!(seen["screen"], serde_json::json!([800, 431]), "the screen the window's");
        assert_eq!(seen["page"], serde_json::json!([800, 431]), "the page the window's");
        assert_eq!(seen["chrome"], serde_json::json!(["loadTimes", "csi", "app"]), "window.chrome as Chrome's");
        // Chrome of the browser's own build: Chromium's full version Google Chrome's too, nothing of Edge's.
        view.evaluate(
            "navigator.userAgentData.getHighEntropyValues(['fullVersionList']).then((v) => window.versions = v.fullVersionList)",
        )
        .unwrap();
        pump(0.3);
        let versions = read("JSON.stringify(window.versions)");
        println!("the page's brands' versions: {versions}");
        let version_of = |brand: &str| {
            versions.as_array().unwrap().iter().find(|b| b["brand"] == brand).map(|b| b["version"].clone())
        };
        assert!(version_of("Chromium").is_some() && version_of("Chromium") == version_of("Google Chrome"));
        assert!(!versions.to_string().contains("Edge"), "{versions}");
        view.evaluate(
            "addEventListener('message', (e) => window.inFrame = e.data); \
             addEventListener('mousedown', (e) => window.onPage = [e.screenX, e.screenY, e.clientX, e.clientY])",
        )
        .unwrap();
        // Clicked as the cab clicks (DevTools, the page's pixels): on the page, then in the frame.
        click(view, 50, 50);
        click(view, 150, 150);
        pump(0.5);
        let (on_page, in_frame) = (read("JSON.stringify(window.onPage)"), read("JSON.stringify(window.inFrame)"));
        println!("the mouse on the page (screen, client): {on_page}; in the frame: {in_frame}");
        assert_eq!(on_page, serde_json::json!([50, 50, 50, 50]), "the mouse where the window is");
        assert_eq!(in_frame, serde_json::json!([150, 150, 50, 50, true, "undefined"]), "in a frame: the same");
        safari.close(&stage);
    }

    /// A page's drop-down list pressed opens no window of the browser's on the PC's screen (its own list would: Safari's
    /// window is on the screen, owner 2026-11-08), nor does a colour field's picker or a file field's dialog: the
    /// list's options go to the cab, where its field is in the frame; the option picked there is the page's, told
    /// as a page's own pick is (`input`, `change`); pressed elsewhere on the page, the list goes.
    #[test]
    #[ignore = "runs WebView2"]
    fn drop_down_lists_are_the_cabs() {
        use windows::Win32::Foundation::{HWND, LPARAM};
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        // The windows shown on the screen.
        fn shown() -> Vec<isize> {
            unsafe extern "system" fn each(hwnd: HWND, list: LPARAM) -> windows::core::BOOL {
                // SAFETY: `list` is the Vec below, for this call only.
                unsafe {
                    if IsWindowVisible(hwnd).as_bool() {
                        (*(list.0 as *mut Vec<isize>)).push(hwnd.0 as isize);
                    }
                }
                true.into()
            }
            let mut list: Vec<isize> = Vec::new();
            // SAFETY: the callback above, given the list for this call only.
            unsafe {
                let _ = EnumWindows(Some(each), LPARAM(&mut list as *mut _ as isize));
            }
            list
        }
        let root = std::env::temp_dir().join(format!("westplay-safari-lists-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(
            site.join("page.html"),
            "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>page</title><body style='margin:0'>\
             <select id=s style='position:absolute;left:20px;top:20px;width:200px;height:40px' \
             onchange='window.changed = this.value'>\
             <option>one</option><optgroup label=More><option>two</option><option disabled>three</option>\
             <option>four</option></optgroup></select>\
             <input id=c type=color style='position:absolute;left:300px;top:100px;width:100px;height:40px'>\
             <input id=f type=file style='position:absolute;left:20px;top:180px;width:200px;height:40px'>",
        )
        .unwrap();
        let stage = Stage::new(1040, 560).unwrap();
        let mut safari = Safari::start(1040, 560, &root.join("profile"), &root.join("tabs"), Vec::new()).unwrap();
        safari.test_site = Some(site.clone());
        let show = Command::Show {
            tab: 1,
            url: "https://westplay.safari-test/page.html".into(),
            x: 0.0,
            y: 0.0,
            width: 1040,
            height: 560,
            zoom: 1.3,
            covers: Vec::new(),
            dark: Some(false),
            radius: 0.0,
            language: None,
        };
        safari.command(&stage, &show).unwrap();
        for _ in 0..6 {
            pump(0.25);
            safari.take_reports(&stage);
        }
        let list_of = |reports: &[Report]| {
            reports.iter().rev().find_map(|report| match report {
                Report::Tab { tab: 1, state } => Some(state.list.clone()),
                _ => None,
            })
        };
        let before = shown();
        let view = &safari.tab(1).unwrap().view;
        // The list pressed (as the cab clicks: the page's CSS pixels).
        click(view, 100, 40);
        pump(0.8);
        let reports = safari.take_reports(&stage);
        let new: Vec<_> = shown().into_iter().filter(|window| !before.contains(window)).collect();
        assert!(new.is_empty(), "windows shown with the list pressed: {new:?}");
        let list = list_of(&reports).flatten().expect("the list told the cab");
        println!("the list: {list:?}");
        let labels: Vec<_> =
            list.options.iter().map(|option| (option.label.as_str(), option.group.as_str(), option.disabled)).collect();
        assert_eq!(
            labels,
            [("one", "", false), ("two", "More", false), ("three", "More", true), ("four", "More", false)]
        );
        assert_eq!(list.selected, 0);
        // Where its field is in the frame: its CSS pixels at the page's 1.3.
        let rect = list.rect.unwrap();
        assert!(
            (rect.x - 26.0).abs() < 0.5 && (rect.y - 26.0).abs() < 0.5 && (rect.width - 260.0).abs() < 0.5,
            "{rect:?}"
        );
        // The fourth picked in the cab: the page's.
        safari.command(&stage, &Command::Pick { tab: 1, list: list.id.clone(), index: Some(3) }).unwrap();
        pump(0.3);
        let reports = safari.take_reports(&stage);
        assert_eq!(list_of(&reports), Some(None), "the list gone from the cab");
        let view = &safari.tab(1).unwrap().view;
        assert_eq!(view.evaluate("window.changed").unwrap(), "\"four\"", "picked, as the page's own pick is told");
        // Pressed again, then elsewhere on the page: gone.
        click(&safari.tab(1).unwrap().view, 100, 40);
        pump(0.3);
        assert!(list_of(&safari.take_reports(&stage)).flatten().is_some());
        click(&safari.tab(1).unwrap().view, 600, 300);
        pump(0.3);
        assert_eq!(list_of(&safari.take_reports(&stage)), Some(None), "pressed elsewhere: gone");
        // The colour field's picker and the file field's dialog: neither opened.
        click(&safari.tab(1).unwrap().view, 350, 120);
        pump(0.8);
        click(&safari.tab(1).unwrap().view, 100, 200);
        pump(0.8);
        let new: Vec<_> = shown().into_iter().filter(|window| !before.contains(window)).collect();
        assert!(new.is_empty(), "windows shown for a colour or a file: {new:?}");
        safari.close(&stage);
    }

    /// A site's bot check in a frame (Cloudflare's) finds the browser as it is: none of WestPlay's script in it (the
    /// browser's own sound functions untouched, nothing of WestPlay's on its window, no WebView mark) and none of
    /// the cab's messages sent to it; a frame of any other site has the script as before. The top page keeps
    /// nothing of WestPlay's on its window either.
    #[test]
    #[ignore = "runs WebView2"]
    fn bot_checks_see_the_browser_untouched() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-safari-checks-{}", std::process::id()));
        let site = root.join("site");
        let check = root.join("check");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::create_dir_all(&check).unwrap();
        // What a frame sees of itself, told to the top page, and every message it's sent counted.
        let looks = "<!doctype html><script>\
             window.got = 0; addEventListener('message', () => { window.got++; tell() });\
             const native = (f) => Function.prototype.toString.call(f).includes('[native code]');\
             const tell = () => parent.postMessage({ frame: location.hostname, \
               play: native(HTMLMediaElement.prototype.play), \
               volume: native(Object.getOwnPropertyDescriptor(HTMLMediaElement.prototype, 'volume').set), \
               cabSound: 'westplayCabSound' in window, webview: 'webview' in (window.chrome ?? {}), got: window.got }, '*');\
             tell();</script>";
        std::fs::write(site.join("other.html"), looks).unwrap();
        std::fs::write(check.join("check.html"), looks).unwrap();
        std::fs::write(
            site.join("page.html"),
            "<!doctype html><title>page</title><script>window.seen = {};\
             addEventListener('message', (e) => { if (e.data && e.data.frame) window.seen[e.data.frame] = e.data })</script>\
             <iframe src='https://challenges.cloudflare.com/check.html'></iframe>\
             <iframe src='https://westplay.safari-test/other.html'></iframe>",
        )
        .unwrap();
        let stage = Stage::new(1040, 560).unwrap();
        let mut safari = Safari::start(1040, 560, &root.join("profile"), &root.join("tabs"), Vec::new()).unwrap();
        safari.test_site = Some(site.clone());
        let show = Command::Show {
            tab: 1,
            url: "https://westplay.safari-test/page.html".into(),
            x: 0.0,
            y: 0.0,
            width: 1040,
            height: 560,
            zoom: 1.3,
            covers: Vec::new(),
            dark: Some(false),
            radius: 0.0,
            language: None,
        };
        safari.command(&stage, &show).unwrap();
        // Cloudflare's check's address served here, from a folder of the test's own.
        safari.tab(1).unwrap().view.map_folder_to_any_page("challenges.cloudflare.com", &check).unwrap();
        safari
            .command(&stage, &Command::Open { tab: 1, url: "https://westplay.safari-test/page.html".into() })
            .unwrap();
        for _ in 0..8 {
            pump(0.25);
            safari.take_reports(&stage);
        }
        // The cab's messages to the pages (the sound's level), passed down to the frames.
        safari.command(&stage, &Command::Level { level: 0.5 }).unwrap();
        safari.command(&stage, &Command::Level { level: 0.8 }).unwrap();
        pump(0.5);
        let read = |script: &str| -> serde_json::Value {
            let view = &safari.tab(1).unwrap().view;
            serde_json::from_str(&serde_json::from_str::<String>(&view.evaluate(script).unwrap()).unwrap()).unwrap()
        };
        let seen = read("JSON.stringify(window.seen)");
        println!("the frames see: {seen:#}");
        let check = &seen["challenges.cloudflare.com"];
        assert_eq!(check["play"], true, "the check's play() the browser's own");
        assert_eq!(check["volume"], true, "the check's volume the browser's own");
        assert_eq!(check["cabSound"], false);
        assert_eq!(check["webview"], false, "no WebView mark in the check");
        assert_eq!(check["got"], 0, "none of the cab's messages sent to the check");
        let other = &seen["westplay.safari-test"];
        assert_eq!(other["play"], false, "another site's frame has the cab's script");
        assert!(other["got"].as_u64().unwrap() >= 2, "and the cab's messages");
        let top = read("JSON.stringify(['westplayCabSound' in window, 'webview' in (window.chrome ?? {})])");
        assert_eq!(top, serde_json::json!([false, false]), "nothing of WestPlay's on the page's window");
        safari.close(&stage);
    }

    /// Safari's bars follow the colour at the page's top as the page changes it itself (its header turning black as a
    /// video opens, no new page: owner 2026-11-09, m.youtube.com); and as the cab turns dark, a page following it as it
    /// runs isn't loaded again, one choosing its colours as it loads is (once, then dark), and one keeping its own is
    /// loaded again once, then left alone. The page covers Safari's frame to the pixel, whatever its width at the zoom.
    #[test]
    #[ignore = "runs WebView2"]
    fn page_colours_follow_the_page_and_the_cab() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-safari-colours-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).unwrap();
        let counted = "<meta name=viewport content='width=device-width,initial-scale=1'><script>sessionStorage.loads = String(Number(sessionStorage.loads || 0) + 1)</script>";
        std::fs::write(
            site.join("header.html"),
            format!(
                "<!doctype html><meta name=theme-color content='#ffffff'>{counted}<body style='margin:0'>\
                 <header id=h style='height:60px;background:#ffffff'></header>\
                 <button style='margin:20px' onclick=\"history.pushState({{}}, '', '#video'); \
                 document.getElementById('h').style.background = '#000000'\">video</button>"
            ),
        )
        .unwrap();
        std::fs::write(
            site.join("live.html"),
            format!(
                "<!doctype html>{counted}<style>body {{ background: #fff }} \
                 @media (prefers-color-scheme: dark) {{ body {{ background: #111 }} }}</style><body>"
            ),
        )
        .unwrap();
        std::fs::write(
            site.join("loads.html"),
            format!(
                "<!doctype html>{counted}<body><script>document.body.style.background = \
                 matchMedia('(prefers-color-scheme: dark)').matches ? '#111' : '#fff'</script>"
            ),
        )
        .unwrap();
        std::fs::write(site.join("own.html"), format!("<!doctype html>{counted}<body style='background:#fff'>"))
            .unwrap();
        // A frame whose width isn't whole CSS pixels at the zoom: 1037 / 1.3 = 797.7.
        let stage = Stage::new(1037, 560).unwrap();
        let mut safari = Safari::start(1037, 560, &root.join("profile"), &root.join("tabs"), Vec::new()).unwrap();
        safari.test_site = Some(site.clone());
        let settle = |safari: &mut Safari, seconds: u32| {
            let mut reports = Vec::new();
            for _ in 0..seconds * 4 {
                pump(0.25);
                reports.extend(safari.take_reports(&stage));
            }
            reports
        };
        let theme_of = |reports: &[Report], tab: u32| {
            reports.iter().rev().find_map(|report| match report {
                Report::Tab { tab: id, state } if *id == tab => Some(state.theme.clone()),
                _ => None,
            })
        };
        let mut themes = std::collections::HashMap::new();
        for (tab, page) in [(1, "header.html"), (2, "live.html"), (3, "loads.html"), (4, "own.html")] {
            let show = Command::Show {
                tab,
                url: format!("https://westplay.safari-test/{page}"),
                x: 0.0,
                y: 0.0,
                width: 1037,
                height: 560,
                zoom: 1.3,
                covers: Vec::new(),
                dark: Some(false),
                radius: 0.0,
                language: None,
            };
            safari.command(&stage, &show).unwrap();
            let reports = settle(&mut safari, 2);
            themes.insert(tab, theme_of(&reports, tab));
        }
        // The page keeping its own colours on a site of its own (what's learnt of a site goes for all its pages).
        safari.tab(4).unwrap().view.map_folder_to_any_page("own.safari-test", &site).unwrap();
        safari.command(&stage, &Command::Open { tab: 4, url: "https://own.safari-test/own.html".into() }).unwrap();
        settle(&mut safari, 2);
        println!("light: {themes:?}");
        assert_eq!(themes[&1].as_deref(), Some("rgb(255, 255, 255)"), "the header's colour");
        // Covering the frame to the pixel.
        let view = &safari.tab(4).unwrap().view;
        let covers =
            view.evaluate("JSON.stringify([innerWidth * devicePixelRatio, innerHeight * devicePixelRatio])").unwrap();
        let covers: Vec<f64> = serde_json::from_str(&serde_json::from_str::<String>(&covers).unwrap()).unwrap();
        println!("the page's window in the frame's pixels: {covers:?}");
        assert!((covers[0] - 1037.0).abs() < 0.01, "as wide as the frame, exactly: {covers:?}");
        assert!(covers[1] >= 560.0, "as tall at least: {covers:?}");
        // The header turned black by the page itself (a video opened in it): the bars follow, soon.
        let at = std::time::Instant::now();
        click(&safari.tab(1).unwrap().view, 40, 85);
        let mut black = None;
        while at.elapsed() < std::time::Duration::from_secs(2) && black.is_none() {
            pump(0.05);
            let reports = safari.take_reports(&stage);
            black = theme_of(&reports, 1).filter(|theme| theme == "rgb(0, 0, 0)").map(|_| at.elapsed());
        }
        println!("the header turned black, told the bars in {black:?}");
        assert!(black.is_some_and(|after| after < std::time::Duration::from_millis(600)), "{black:?}");
        // The cab turns dark.
        safari.command(&stage, &Command::Appearance { dark: true }).unwrap();
        let reports = settle(&mut safari, 4);
        // The page's own ground told the cab (what of it edges the page takes it): dark as the page turns dark.
        let ground = reports.iter().rev().find_map(|report| match report {
            Report::Tab { tab: 2, state } => Some(state.ground.clone()),
            _ => None,
        });
        println!("the following page's ground, told the cab: {ground:?}");
        assert_eq!(ground.as_deref(), Some("rgb(17, 17, 17)"));
        let loads = |safari: &Safari, tab: u32| safari.tab(tab).unwrap().view.evaluate("sessionStorage.loads").unwrap();
        let dark_now = |safari: &Safari, tab: u32| {
            safari.tab(tab).unwrap().view.evaluate("getComputedStyle(document.body).backgroundColor").unwrap()
        };
        println!(
            "dark: following as it runs {} loads {}; choosing as it loads {} loads {}; its own {} loads {}",
            dark_now(&safari, 2),
            loads(&safari, 2),
            dark_now(&safari, 3),
            loads(&safari, 3),
            dark_now(&safari, 4),
            loads(&safari, 4)
        );
        assert_eq!((dark_now(&safari, 2), loads(&safari, 2)), ("\"rgb(17, 17, 17)\"".into(), "\"1\"".into()));
        assert_eq!((dark_now(&safari, 3), loads(&safari, 3)), ("\"rgb(17, 17, 17)\"".into(), "\"2\"".into()));
        assert_eq!(loads(&safari, 4), "\"3\"", "its own colours: loaded again twice");
        // Light again: the page keeping its own isn't loaded again.
        safari.command(&stage, &Command::Appearance { dark: false }).unwrap();
        settle(&mut safari, 4);
        println!(
            "light again: its own loads {}; choosing as it loads {} loads {}",
            loads(&safari, 4),
            dark_now(&safari, 3),
            loads(&safari, 3)
        );
        assert_eq!(loads(&safari, 4), "\"3\"", "its own colours: left alone");
        assert_eq!((dark_now(&safari, 3), loads(&safari, 3)), ("\"rgb(255, 255, 255)\"".into(), "\"3\"".into()));
        safari.close(&stage);
    }

    /// The frame as the cab gets it (Windows Graphics Capture of the stage, as capture.rs): its size and BGRA pixels.
    fn grab(stage: &Stage) -> (u32, u32, Vec<u8>) {
        use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
        use windows::Graphics::DirectX::DirectXPixelFormat;
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            ID3D11Texture2D,
        };
        use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
        use windows::core::Interface;
        let gpu = crate::gpu::Gpu::new(None).unwrap();
        let item = GraphicsCaptureItem::CreateFromVisual(&stage.visual().unwrap()).unwrap();
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &gpu.winrt_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            item.Size().unwrap(),
        )
        .unwrap();
        let session = pool.CreateCaptureSession(&item).unwrap();
        session.StartCapture().unwrap();
        let started = std::time::Instant::now();
        let mut frame = None;
        // A few frames in: the last one, everything drawn.
        while started.elapsed() < std::time::Duration::from_secs(3) {
            pump(0.05);
            if let Ok(next) = pool.TryGetNextFrame() {
                frame = Some(next);
                if started.elapsed() > std::time::Duration::from_millis(600) {
                    break;
                }
            }
        }
        let frame = frame.expect("a frame");
        // SAFETY: the frame's own surface, as capture.rs reads it; the staging copy is the test's, mapped for reading
        // and unmapped after, rows copied within the mapped pitch.
        unsafe {
            let texture: ID3D11Texture2D =
                frame.Surface().unwrap().cast::<IDirect3DDxgiInterfaceAccess>().unwrap().GetInterface().unwrap();
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            desc.MiscFlags = 0;
            let mut staging = None;
            gpu.device.CreateTexture2D(&desc, None, Some(&mut staging)).unwrap();
            let staging = staging.unwrap();
            gpu.context.CopyResource(&staging, &texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            gpu.context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).unwrap();
            let (width, height) = (desc.Width, desc.Height);
            let mut read = vec![0u8; (width * height * 4) as usize];
            for row in 0..height as usize {
                let from = (mapped.pData as *const u8).add(row * mapped.RowPitch as usize);
                std::ptr::copy_nonoverlapping(from, read[row * width as usize * 4..].as_mut_ptr(), width as usize * 4);
            }
            gpu.context.Unmap(&staging, 0);
            let _ = session.Close();
            let _ = pool.Close();
            (width, height, read)
        }
    }

    /// The page as the cab gets it (the frame captured, as capture.rs captures it): filling all of Safari's frame,
    /// nothing of the WebView's white anywhere in it, drawn at the cab's zoom (a 100 CSS pixel square 130 frame
    /// pixels at 1.3), at frames and zooms of the cab's kinds (owner 2026-11-09: white bands at a dark page's right
    /// and bottom, the page too small: the pixel ratio was only said to the page, never drawn at).
    #[test]
    #[ignore = "runs WebView2"]
    fn pages_fill_the_frame_the_cab_gets() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-safari-fill-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(
            site.join("dark.html"),
            "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>dark</title>\
             <body style='margin:0;background:#0f0f0f'><div style='width:100px;height:100px;background:#f00'></div>\
             <div style='height:3000px'></div>",
        )
        .unwrap();
        let (fw, fh) = (1280u32, 720u32);
        let stage = Stage::new(fw, fh).unwrap();
        let mut safari = Safari::start(fw, fh, &root.join("profile"), &root.join("tabs"), Vec::new()).unwrap();
        safari.test_site = Some(site.clone());
        for (tab, (x, y, width, height, zoom)) in [
            (40.0f32, 30.0f32, 1040u32, 560u32, 1.3f64),
            (113.0, 61.0, 1101, 593, 1.287),
            (200.0, 100.0, 967, 503, 1.611),
        ]
        .into_iter()
        .enumerate()
        {
            let tab = tab as u32 + 1;
            let show = Command::Show {
                tab,
                url: "https://westplay.safari-test/dark.html".into(),
                x,
                y,
                width,
                height,
                zoom,
                covers: Vec::new(),
                dark: Some(true),
                radius: 30.0,
                language: None,
            };
            safari.command(&stage, &show).unwrap();
            for _ in 0..12 {
                pump(0.25);
                safari.take_reports(&stage);
            }
            let (w, _h, px) = grab(&stage);
            let at = |px_x: u32, px_y: u32| {
                let i = ((px_y * w + px_x) * 4) as usize;
                (px[i + 2], px[i + 1], px[i])
            };
            let (x0, y0) = (x as u32, y as u32);
            // Inside the page's rectangle, above its round bottom corners.
            let (mut light, mut square) = (0, (0, 0));
            for py in y0..y0 + height - 32 {
                for px_x in x0..x0 + width {
                    let (r, g, b) = at(px_x, py);
                    if r > 200 && g < 60 && b < 60 {
                        square = (square.0.max(px_x - x0 + 1), square.1.max(py - y0 + 1));
                    } else if u32::from(r) + u32::from(g) + u32::from(b) > 150 {
                        light += 1;
                    }
                }
            }
            let ratio = safari.page_zoom(tab).unwrap();
            println!(
                "page {width}x{height} at zoom {zoom}: light pixels {light}, the square drawn {square:?} (ratio {ratio:.4})"
            );
            assert_eq!(light, 0, "nothing white in the page");
            let side = (100.0 * ratio).round() as u32;
            assert!(
                square.0.abs_diff(side) <= 1 && square.1.abs_diff(side) <= 1,
                "drawn at the zoom: {square:?} {side}"
            );
            safari.command(&stage, &Command::Hide { tab: None }).unwrap();
        }
        safari.close(&stage);
    }

    /// m.youtube.com follows the cab as it turns dark and light again (owner 2026-11-09: "theme switching doesn't
    /// work"): it keeps the appearance it was last shown in in its PREF cookie, the page it's loaded again with drawn
    /// by it (dark, light asked), the one after that light.
    #[test]
    #[ignore = "runs WebView2 (real website)"]
    fn youtube_follows_the_cabs_light_and_dark() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-safari-yts-{}", std::process::id()));
        let stage = Stage::new(1040, 560).unwrap();
        let mut safari = Safari::start(1040, 560, &root.join("profile"), &root.join("tabs"), Vec::new()).unwrap();
        let show = Command::Show {
            tab: 1,
            url: "https://m.youtube.com/".into(),
            x: 0.0,
            y: 0.0,
            width: 1040,
            height: 560,
            zoom: 1.3,
            covers: Vec::new(),
            dark: Some(false),
            radius: 0.0,
            language: Some("tr-TR".into()),
        };
        safari.command(&stage, &show).unwrap();
        let ground = |safari: &Safari| {
            safari.tab(1).unwrap().view.evaluate("getComputedStyle(document.documentElement).backgroundColor").unwrap()
        };
        // Within `seconds`, the page's ground `wanted`.
        let wait_for = |safari: &mut Safari, wanted: &str, seconds: u32| {
            for _ in 0..seconds * 4 {
                pump(0.25);
                safari.take_reports(&stage);
                if ground(safari) == wanted {
                    return true;
                }
            }
            false
        };
        assert!(wait_for(&mut safari, "\"rgb(255, 255, 255)\"", 8), "light: {}", ground(&safari));
        safari.command(&stage, &Command::Appearance { dark: true }).unwrap();
        assert!(wait_for(&mut safari, "\"rgb(15, 15, 15)\"", 10), "dark: {}", ground(&safari));
        safari.command(&stage, &Command::Appearance { dark: false }).unwrap();
        assert!(wait_for(&mut safari, "\"rgb(255, 255, 255)\"", 12), "light again: {}", ground(&safari));
        // And dark once more: it isn't taken to keep its own colours.
        safari.command(&stage, &Command::Appearance { dark: true }).unwrap();
        assert!(wait_for(&mut safari, "\"rgb(15, 15, 15)\"", 12), "dark again: {}", ground(&safari));
        safari.close(&stage);
    }

    /// A drag with the mouse, as the cab sends it, scrolls a page as a finger would and selects none of its text; a
    /// press on a button still clicks it (owner 2026-11-09: "touch-scrolling makes me select text").
    #[test]
    #[ignore = "runs WebView2"]
    fn a_drag_scrolls_a_page_as_a_finger() {
        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
        // SAFETY: COM on this test's own thread, once, before anything uses it.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap() };
        let root = std::env::temp_dir().join(format!("westplay-safari-touch-{}", std::process::id()));
        let site = root.join("site");
        std::fs::create_dir_all(&site).unwrap();
        let text: String =
            (0..200).map(|i| format!("<p>Line {i} of some text to select or scroll past.</p>")).collect();
        std::fs::write(
            site.join("long.html"),
            format!(
                "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><title>long</title>\
                 <body style='margin:0;font:20px sans-serif'><button id=b style='width:300px;height:60px' \
                 onclick='window.clicked = (window.clicked || 0) + 1'>button</button>{text}"
            ),
        )
        .unwrap();
        let stage = Stage::new(1040, 560).unwrap();
        let mut safari = Safari::start(1040, 560, &root.join("profile"), &root.join("tabs"), Vec::new()).unwrap();
        safari.test_site = Some(site.clone());
        let show = Command::Show {
            tab: 1,
            url: "https://westplay.safari-test/long.html".into(),
            x: 0.0,
            y: 0.0,
            width: 1040,
            height: 560,
            zoom: 1.3,
            covers: Vec::new(),
            dark: Some(false),
            radius: 0.0,
            language: None,
        };
        safari.command(&stage, &show).unwrap();
        for _ in 0..8 {
            pump(0.25);
            safari.take_reports(&stage);
        }
        let view = &safari.tab(1).unwrap().view;
        let send = |kind: &str, x: f64, y: f64, button: &str, buttons: u32| {
            let event = serde_json::json!({ "type": kind, "x": x, "y": y, "button": button, "buttons": buttons,
                "clickCount": 1, "modifiers": 0 });
            view.devtools("Input.dispatchMouseEvent", &event.to_string()).unwrap();
        };
        send("mouseMoved", 400.0, 380.0, "none", 0);
        send("mousePressed", 400.0, 380.0, "left", 1);
        for step in 1..=20 {
            pump(0.016);
            send("mouseMoved", 400.0, 380.0 - f64::from(step) * 12.0, "left", 1);
        }
        send("mouseReleased", 400.0, 140.0, "left", 0);
        pump(0.8);
        let dragged: Vec<f64> = serde_json::from_str(
            &serde_json::from_str::<String>(
                &view.evaluate("JSON.stringify([Math.round(scrollY), getSelection().toString().length])").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        println!("dragged up 240: scrolled {}, selected {}", dragged[0], dragged[1]);
        assert!(dragged[0] >= 200.0, "scrolled as a finger scrolls: {dragged:?}");
        assert_eq!(dragged[1], 0.0, "nothing selected");
        view.evaluate("window.scrollTo(0, 0)").unwrap();
        pump(0.3);
        send("mouseMoved", 100.0, 30.0, "none", 0);
        send("mousePressed", 100.0, 30.0, "left", 1);
        pump(0.05);
        send("mouseReleased", 100.0, 30.0, "left", 0);
        pump(0.5);
        assert_eq!(view.evaluate("window.clicked").unwrap(), "1", "a press still clicks");
        safari.close(&stage);
    }

    #[test]
    fn cuts_long_text_at_a_characters_edge() {
        let mut text = "ğğğ".to_string();
        cut(&mut text, 3);
        assert_eq!(text, "ğ");
    }
}
