//! WestPlay host: renders the in-cabin UI in an offscreen WebView2 and feeds it to the game plugin.
//!
//! [`run`] owns one STA thread: it pumps window messages for WebView2, keeps the capture pipeline on the
//! game's GPU, forwards telemetry from the bridge to the page, and beats the host heartbeat.

#[macro_use]
pub mod log;
mod apple_music;
pub mod browser;
mod camera;
mod capture;
pub mod customize;
mod game_settings;
mod gpu;
mod lyrics;
pub mod maps;
mod player;
mod pointer;
pub mod route;
mod route_change;
pub mod settings;
mod signin;
mod speakers;
mod spotify;
pub mod startup;
mod typing;
mod weather;
mod webview;
mod youtube;
mod youtube_music;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use westplay_shared::{BridgeMapping, JobInfo, Telemetry, TruckInfo, screen_layouts};
use windows::Win32::Foundation::LUID;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage};

use capture::Capture;
use gpu::{Gpu, SharedFrame};
use player::{Player, PlayerWatch};
use pointer::Pointer;
use route::{RouteUpdate, RouteWatch};
use settings::SharedSettings;
use weather::{Sky, Weather, WeatherWatch};
use webview::{OffscreenWebView, Stage};
use westplay_mapdata::junctions::Turn;

pub use webview::Error;
/// Size the cabin page is rendered at: the size of the frames the plugin copies into the screen textures.
pub use westplay_shared::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// Built-in page used when no UI URL is given: shows that frames and telemetry arrive.
pub const TEST_PAGE: &str = include_str!("test_page.html");

#[derive(Clone)]
pub enum Content {
    Url(String),
    Html(String),
    /// A built UI folder, served at `https://westplay.cabin/` through a WebView2 virtual host.
    Folder {
        root: PathBuf,
        page: String,
    },
}

/// Host health, reported about once per second.
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub webview_version: String,
    pub adapter: Option<String>,
    pub frames_per_second: u64,
    pub dropped_total: u64,
    pub game_connected: bool,
    pub screen_drawing: bool,
    /// Whether WestPlay is open on the truck's screen (otherwise the truck's original screen shows, with
    /// WestPlay's button to come back), as the cabin page last said; `None` until it has.
    pub westplay_open: Option<bool>,
    /// Whether YouTube is signed in, as its page last said (`None`: not known, YouTube not run yet), and
    /// whether Google's sign-in window is open. Nothing else of the account.
    pub youtube_signed_in: Option<bool>,
    pub youtube_signing_in: bool,
    /// The same of Spotify: signed in or not (`None`: not known, Spotify not run yet), its sign-in window open.
    pub spotify_signed_in: Option<bool>,
    pub spotify_signing_in: bool,
    /// The same of Apple Music: signed in or not (`None`: not known, Apple Music not run yet), Apple's sign-in
    /// window open.
    pub apple_music_signed_in: Option<bool>,
    pub apple_music_signing_in: bool,
    /// The map of the profile played: ready, being built, waiting for the game to close...
    pub map: maps::MapStatus,
    /// The map the cab shows: its folder in WestPlay's maps, and when it was built (the companion's
    /// onboarding shows it too).
    pub map_folder: Option<String>,
    pub map_built: Option<u64>,
    /// Every profile's map: ready, being built, to build, failed...
    pub profile_maps: Vec<maps::ProfileMap>,
    pub error: Option<String>,
}

/// The companion window's requests to sign YouTube, Spotify or Apple Music in or out (or to know whether it
/// is), from any thread, for the host to carry out as the cab's apps would.
#[derive(Debug, Default)]
pub struct AccountRequests {
    pub youtube: AccountRequest,
    pub spotify: AccountRequest,
    pub apple_music: AccountRequest,
}

/// One service's request, the latest not carried out yet.
#[derive(Debug, Default)]
pub struct AccountRequest(AtomicU8);

/// What the companion asks of a service's sign-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountAction {
    SignIn,
    SignOut,
    /// Run it (hidden), so whether it's signed in is known (the onboarding's sign-in steps, shown).
    Check,
}

impl AccountRequest {
    const NONE: u8 = 0;
    const SIGN_IN: u8 = 1;
    const SIGN_OUT: u8 = 2;
    const CHECK: u8 = 3;

    /// Asks for it (replacing a request not carried out yet).
    pub fn request(&self, action: AccountAction) {
        let value = match action {
            AccountAction::SignIn => Self::SIGN_IN,
            AccountAction::SignOut => Self::SIGN_OUT,
            AccountAction::Check => Self::CHECK,
        };
        self.0.store(value, Ordering::Release);
    }

    /// The latest request not carried out yet, if any.
    pub fn take(&self) -> Option<AccountAction> {
        match self.0.swap(Self::NONE, Ordering::AcqRel) {
            Self::SIGN_IN => Some(AccountAction::SignIn),
            Self::SIGN_OUT => Some(AccountAction::SignOut),
            Self::CHECK => Some(AccountAction::Check),
            _ => None,
        }
    }
}

/// A request to open or close WestPlay on the truck's screen (the companion window's switch), from any
/// thread, for the host to pass on to the cabin page, which owns that state (its sidebar's truck button and
/// its WestPlay button change it too).
#[derive(Debug, Default)]
pub struct OpenRequest(AtomicU8);

impl OpenRequest {
    const NONE: u8 = 0;
    const CLOSE: u8 = 1;
    const OPEN: u8 = 2;

    /// Asks for WestPlay to be opened or closed (replacing a request not passed on yet).
    pub fn request(&self, open: bool) {
        self.0.store(if open { Self::OPEN } else { Self::CLOSE }, Ordering::Release);
    }

    /// The latest request not passed on yet, if any.
    pub fn take(&self) -> Option<bool> {
        match self.0.swap(Self::NONE, Ordering::AcqRel) {
            Self::OPEN => Some(true),
            Self::CLOSE => Some(false),
            _ => None,
        }
    }
}

pub struct Config {
    pub width: u32,
    pub height: u32,
    pub content: Content,
    /// WebView2 user data (cache, cookies) lives here.
    pub data_dir: PathBuf,
    /// Requests to open or close WestPlay on the truck's screen, from any thread.
    pub open_request: Arc<OpenRequest>,
    /// Signing YouTube or Spotify in or out (or finding out whether it is), from the companion window.
    pub accounts: Arc<AccountRequests>,
    /// Called on the host thread about once per second, and right after WestPlay opens or closes.
    pub on_status: Box<dyn FnMut(&Status) + Send>,
    /// The player's settings; changes are applied as they happen.
    pub settings: Arc<SharedSettings>,
    /// Called when the cabin page changes a setting itself (its Settings' wallpaper), after it's applied: to
    /// keep it and tell the companion.
    pub on_settings: Box<dyn FnMut(&settings::Settings) + Send>,
    /// Bumped whenever the player's wallpapers or icon sets change (`customize`): the page is told again.
    pub customize_changed: Arc<std::sync::atomic::AtomicU64>,
    /// The game WestPlay is set up in (the companion's setup sets it): its map is built for the cab.
    pub game: Arc<maps::SetUpGame>,
}

struct Pipeline {
    gpu: Gpu,
    /// Holds the shared frame texture (and its name) alive.
    capture: Capture,
}

/// Runs until `stop` is set. Blocks the calling thread, which becomes the WebView2 UI thread.
pub fn run(mut config: Config, stop: Arc<AtomicBool>) -> Result<(), Error> {
    startup::mark("host: started");
    // SAFETY: COM on this thread, once, before anything on it uses COM (WebView2 needs a single-threaded
    // apartment).
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
    let bridge = Arc::new(BridgeMapping::open()?);
    let host = &bridge.bridge().host;
    host.pid.store(std::process::id(), Ordering::Release);
    startup::mark("host: game bridge open");

    let stage = Stage::new(config.width, config.height)?;
    let webview = OffscreenWebView::new(&stage, config.width, config.height, &config.data_dir.join("webview2"))?;
    stage.add(&webview, true)?;
    startup::mark("host: cabin browser (WebView2) ready");
    match &config.content {
        Content::Url(url) => webview.navigate(url)?,
        Content::Html(html) => webview.navigate_to_string(html)?,
        Content::Folder { root, page } => {
            webview.map_folder("westplay.cabin", root)?;
            webview.navigate(&format!("https://westplay.cabin/{page}"))?;
        }
    }
    startup::mark("host: cabin page requested");
    // The player's maps, built from their game (maps.rs), for the cab's Maps app.
    let maps_dir = config.data_dir.join("maps");
    let _ = std::fs::create_dir_all(&maps_dir);
    webview.map_folder_to_any_page("westplay.maps", &maps_dir)?;
    // The player's own wallpapers and icon sets (customize.rs), listed off this thread: deciding whether an
    // image is dark or light decodes it, which mustn't hold the screen up.
    let customize_dir = config.data_dir.join(customize::FOLDER);
    let _ = std::fs::create_dir_all(&customize_dir);
    webview.map_folder_to_any_page("westplay.customize", &customize_dir)?;
    let (customize_list, customize_listed) = std::sync::mpsc::channel::<String>();
    let mut customize_posted: Option<u64> = None;
    let mut customize_message: Option<String> = None;
    let map_watch = maps::MapWatch::start(maps_dir, config.game.clone());
    let mut map_shown: (Option<String>, Option<u64>) = (None, None);
    let mut status = Status { webview_version: webview.version.clone(), ..Default::default() };

    let mut pipeline: Option<Pipeline> = None;
    let mut generation_counter = 0u32;
    let mut last_check = Instant::now() - Duration::from_secs(10);
    let mut last_stats = Instant::now();
    let mut last_telemetry_post = Instant::now();
    let mut last_telemetry: Option<Telemetry> = None;
    let mut last_truck: Option<TruckInfo> = None;
    let mut last_job: Option<JobInfo> = None;
    let mut last_layout: Option<u32> = None;
    let mut pointer = Pointer::new(config.width, config.height);
    // What the player's latest save says (their garages, a map WestPlay doesn't allow), kept to send again
    // when the page reloads.
    let player_watch = PlayerWatch::start();
    let mut player_messages: Vec<String> = Vec::new();
    // The game's GPS route, and the truck's progress along it, kept to send again when the page reloads.
    let route_watch = RouteWatch::start(bridge.clone());
    // The host's number for the route shown (`route::RouteUpdate::Route`).
    let mut route_number: Option<u32> = None;
    let (mut route_message, mut route_paths_message, mut route_next_message): (
        Option<String>,
        Option<String>,
        Option<String>,
    ) = (None, None, None);
    // The game's weather, kept to send again when the page reloads.
    let weather_watch = WeatherWatch::start(bridge.clone());
    let mut weather_message: Option<String> = None;
    // Whether the game's window is in front (as last told the page; sent again to a reloaded page).
    let mut game_focused = true;
    // The page's changes to the game's route, waiting for the plugin's answer.
    let mut route_changes = route_change::RouteChanges::default();
    // Whether the player's keyboard types into the page's field, as last told to the page.
    let mut typing_state: Option<westplay_shared::TypingState> = None;
    // YouTube's player, while the page uses it, and how it last played (sent again when the page reloads).
    let mut youtube: Option<youtube::YouTube> = None;
    // Whether the game's camera is outside the cab (YouTube's sound muffled then), and the player's camera
    // settings in the game (it turns with the steering).
    let mut camera = camera::Camera::default();
    let mut game_settings = game_settings::GameSettings::new();
    let mut youtube_message: Option<String> = None;
    // Whether YouTube's page is signed in (and the picture for it), as last told: sent again to a reloaded
    // page. Only in memory, never kept.
    let mut youtube_account_message: Option<String> = None;
    // Whether YouTube's video was playing at its last report: a video that stops to buffer is logged.
    let mut youtube_was_playing = false;
    // Spotify's web player, while the page uses it, and what it last said (sent again when the page reloads):
    // signed in or not, and what plays.
    let mut spotify: Option<spotify::Spotify> = None;
    let mut spotify_state: Option<spotify::State> = None;
    let mut spotify_signing_in = false;
    // Each service's sign-in confirming after its window closed signed in (youtube.rs, spotify.rs).
    let (mut youtube_window_open, mut youtube_confirming) = (false, false);
    let mut spotify_confirming = false;
    let mut spotify_playback: Option<String> = None;
    // Apple Music's web player, the same way.
    let mut apple_music: Option<apple_music::AppleMusic> = None;
    let mut apple_music_state: Option<apple_music::State> = None;
    let (mut apple_music_window_open, mut apple_music_confirming) = (false, false);
    let mut apple_music_playback: Option<String> = None;
    // YouTube Music's web player, the same way (on the cabin page's profile, sharing YouTube's sign-in).
    let mut youtube_music: Option<youtube_music::YouTubeMusic> = None;
    let mut youtube_music_state: Option<youtube_music::State> = None;
    let (mut youtube_music_window_open, mut youtube_music_confirming) = (false, false);
    let mut youtube_music_playback: Option<String> = None;
    // The settings generation last applied (`None`: apply them once at startup).
    let mut applied_settings = None;
    let mut last_frames = 0;
    // The startup stages still to log (`startup`), checked every turn until all are reached. The game's
    // screen frame counter as found: a session before this one may have left it counting.
    let mut startup_pending = true;
    let screen_frame_before = bridge.bridge().plugin.screen_frame.load(Ordering::Relaxed);

    let mut browser_gone = false;
    let mut game_liveness = Liveness::default();
    while !stop.load(Ordering::Relaxed) {
        pump_messages();
        host.heartbeat.fetch_add(1, Ordering::Release);
        // A captured frame that came before its turn goes out now that it's due.
        if let Some(pipeline) = &pipeline {
            pipeline.capture.flush();
        }
        // The page's process failed (a crash, or hung): load it again, and it gets the current state again
        // as it announces itself. The browser's: this WebView is gone, so start over (`BrowserGone`).
        match webview.take_failure() {
            Some(webview::Failure::Page) => {
                startup::mark("host: the cabin page's process failed; reloading it");
                if let Err(e) = webview.reload() {
                    status.error = Some(format!("the cabin page couldn't be reloaded: {e}"));
                    (config.on_status)(&status);
                }
            }
            Some(webview::Failure::Browser) => {
                browser_gone = true;
                break;
            }
            None => {}
        }
        if startup_pending {
            startup_pending = log_startup(&bridge, pipeline.as_ref(), screen_frame_before);
        }

        // WestPlay's frames go to the truck's screen while the host runs (the page, open or closed, is
        // what shows there).
        host.screen_enabled.store(1, Ordering::Release);
        if let Some(open) = config.open_request.take() {
            let _ = webview.post_json(&westplay_message(open));
        }
        // The companion's YouTube, Spotify and Apple Music sign-ins or sign-outs, as the cab's would be (or only
        // running it, to know whether it's signed in).
        if let Some(action) = config.accounts.youtube.take()
            && let Some(player) = start_youtube(&mut youtube, &stage, &config)
        {
            match action {
                AccountAction::SignIn => drop(player.command(&youtube::Command::SignIn)),
                AccountAction::SignOut => {
                    let _ = player.command(&youtube::Command::SignOut);
                    // YouTube Music shares the sign-in: its page again, signed out.
                    if let Some(music) = &youtube_music {
                        let _ = music.reload();
                    }
                }
                AccountAction::Check => {}
            }
        }
        if let Some(action) = config.accounts.spotify.take()
            && let Some(player) = start_spotify(&mut spotify, &stage, &config)
        {
            let _ = player.command(&match action {
                AccountAction::SignIn => spotify::Command::SignIn,
                AccountAction::SignOut => spotify::Command::SignOut,
                AccountAction::Check => spotify::Command::Start,
            });
        }
        if let Some(action) = config.accounts.apple_music.take()
            && let Some(player) = start_apple_music(&mut apple_music, &stage, &config)
        {
            let _ = player.command(&match action {
                AccountAction::SignIn => apple_music::Command::SignIn,
                AccountAction::SignOut => apple_music::Command::SignOut,
                AccountAction::Check => apple_music::Command::Start,
            });
        }

        // Checked often so the game is picked up right away when it starts or switches GPU.
        if last_check.elapsed() >= Duration::from_millis(200) {
            last_check = Instant::now();
            // The game's window in front or not (tabbed out, another window over it): the page pauses its
            // video meanwhile. Without a game, nothing is away.
            let focused = is_in_front(bridge.bridge().plugin.pid.load(Ordering::Relaxed));
            if focused != game_focused {
                game_focused = focused;
                let _ = webview.post_json(&game_focus_message(focused));
            }
            let wanted = game_adapter(&bridge);
            let needs_rebuild = match &pipeline {
                None => true,
                Some(p) => p.capture.broken() || wanted.is_some_and(|luid| !same_luid(luid, p.gpu.luid)),
            };
            if needs_rebuild {
                host.frame_generation.store(0, Ordering::Release);
                pipeline = None;
                generation_counter = generation_counter.wrapping_add(1);
                let generation = ((std::process::id() & 0x00ff_ffff) << 8) | (generation_counter & 0xff);
                match build_pipeline(&config, &stage, &bridge, wanted, generation) {
                    Ok(built) => {
                        startup::mark(if wanted.is_some() {
                            "host: capturing for the game's GPU"
                        } else {
                            "host: capturing (game not seen yet)"
                        });
                        host.frame_width.store(config.width, Ordering::Relaxed);
                        host.frame_height.store(config.height, Ordering::Relaxed);
                        host.frame_generation.store(generation, Ordering::Release);
                        status.adapter = Some(built.gpu.adapter_name.clone());
                        status.error = None;
                        pipeline = Some(built);
                    }
                    Err(e) => status.error = Some(format!("capture pipeline failed: {e}")),
                }
            }
            if last_stats.elapsed() >= Duration::from_secs(1) {
                last_stats = Instant::now();
                let plugin = &bridge.bridge().plugin;
                let frames = pipeline.as_ref().map_or(0, |p| p.capture.frames.load(Ordering::Relaxed));
                let screen_frame = plugin.screen_frame.load(Ordering::Relaxed);
                let screen_age = plugin.heartbeat.load(Ordering::Relaxed).saturating_sub(screen_frame);
                status.frames_per_second = frames.saturating_sub(last_frames);
                status.dropped_total = pipeline.as_ref().map_or(0, |p| p.capture.dropped.load(Ordering::Relaxed));
                // A game that crashed leaves its pid and counters behind, frozen: alive is its heartbeat
                // moving.
                let alive = game_liveness.update(plugin.heartbeat.load(Ordering::Relaxed), Instant::now());
                status.game_connected = plugin.pid.load(Ordering::Relaxed) != 0 && alive;
                status.screen_drawing = status.game_connected && screen_frame != 0 && screen_age < 120;
                (config.on_status)(&status);
                last_frames = frames;
            }
        }

        let customize_version = config.customize_changed.load(Ordering::Acquire);
        if customize_posted != Some(customize_version) {
            customize_posted = Some(customize_version);
            let (dir, list) = (customize_dir.clone(), customize_list.clone());
            std::thread::spawn(move || {
                let _ = list.send(customize_list_message(&customize::list(&dir)));
            });
        }
        while let Ok(message) = customize_listed.try_recv() {
            let _ = webview.post_json(&message);
            customize_message = Some(message);
        }

        if applied_settings != Some(config.settings.generation()) {
            applied_settings = Some(config.settings.generation());
            let interaction = config.settings.get().interaction;
            pointer.set_sensitivity(interaction.sensitivity.get());
            host.interaction_key.store(interaction.key.virtual_key(), Ordering::Release);
            let _ = webview.post_json(&settings_message(&config.settings.get()));
        }
        pointer.update(&bridge.bridge().plugin.pointer, &webview);

        while let Ok(update) = route_watch.updates.try_recv() {
            // The ways through a route's junctions come later: only for the route still shown.
            if let RouteUpdate::Paths { id, .. } = &update
                && route_number != Some(*id)
            {
                continue;
            }
            let message = route_message_for(&update);
            match update {
                RouteUpdate::Next(_) => route_next_message = Some(message.clone()),
                RouteUpdate::Paths { .. } => route_paths_message = Some(message.clone()),
                RouteUpdate::Route { id, .. } => {
                    route_number = Some(id);
                    (route_message, route_paths_message, route_next_message) = (Some(message.clone()), None, None);
                }
                RouteUpdate::None => {
                    route_number = None;
                    (route_message, route_paths_message, route_next_message) = (Some(message.clone()), None, None);
                }
            }
            let _ = webview.post_json(&message);
        }

        while let Ok(weather) = weather_watch.updates.try_recv() {
            let message = weather_message_for(weather);
            let _ = webview.post_json(&message);
            weather_message = Some(message);
        }

        while let Ok(update) = map_watch.updates.try_recv() {
            if map_shown != (update.folder.clone(), update.built) {
                map_shown = (update.folder.clone(), update.built);
                let _ = webview.post_json(&maps::map_message(map_shown.0.as_deref(), map_shown.1));
                (status.map_folder, status.map_built) = map_shown.clone();
                (config.on_status)(&status);
            }
            if status.map != update.status || status.profile_maps != update.profiles {
                status.map = update.status;
                status.profile_maps = update.profiles;
                (config.on_status)(&status);
            }
        }

        while let Ok(player) = player_watch.updates.try_recv() {
            player_messages = player_messages_for(&player);
            for message in &player_messages {
                let _ = webview.post_json(message);
            }
        }

        for message in webview.take_page_messages() {
            match parse_page_message(&message) {
                // The page announces itself when it (re)loads; it missed everything sent before, so send the
                // current state again (some of it is otherwise only sent when it changes).
                Some(PageMessage::Ready) => {
                    startup::mark("page: running (its scripts loaded)");
                    last_telemetry = None;
                    last_truck = None;
                    last_job = None;
                    last_layout = None;
                    typing_state = None;
                    let route = route_message.iter().chain(&route_paths_message).chain(&route_next_message);
                    let _ = webview.post_json(&game_focus_message(game_focused));
                    let _ = webview.post_json(&maps::map_message(map_shown.0.as_deref(), map_shown.1));
                    if spotify.is_some() {
                        let _ = webview.post_json(&spotify::state_message(spotify_state.as_ref(), spotify_signing_in));
                        if let Some(playback) = &spotify_playback {
                            let _ = webview.post_json(playback);
                        }
                    }
                    if apple_music.is_some() {
                        let signing_in = apple_music_window_open || apple_music_confirming;
                        let message = apple_music::state_message(apple_music_state.as_ref(), signing_in);
                        let _ = webview.post_json(&message);
                        if let Some(playback) = &apple_music_playback {
                            let _ = webview.post_json(playback);
                        }
                    }
                    if youtube_music.is_some() {
                        let signing_in = youtube_music_window_open || youtube_music_confirming;
                        let message = youtube_music::state_message(youtube_music_state.as_ref(), signing_in);
                        let _ = webview.post_json(&message);
                        if let Some(playback) = &youtube_music_playback {
                            let _ = webview.post_json(playback);
                        }
                    }
                    let _ = webview.post_json(&settings_message(&config.settings.get()));
                    if let Some(message) = &customize_message {
                        let _ = webview.post_json(message);
                    }
                    let youtube = youtube_message.iter().chain(&youtube_account_message);
                    let all = player_messages.iter().chain(route).chain(&weather_message).chain(youtube);
                    for message in all {
                        let _ = webview.post_json(message);
                    }
                }
                Some(PageMessage::Westplay { open }) => {
                    if status.westplay_open != Some(open) {
                        status.westplay_open = Some(open);
                        (config.on_status)(&status);
                    }
                }
                Some(PageMessage::Startup { stage }) => startup::mark(&format!("page: {stage}")),
                Some(PageMessage::TruckStrip { strip }) => {
                    let mut settings = config.settings.get();
                    if settings.screen.truck_strip != strip {
                        settings.screen.truck_strip = strip;
                        config.settings.set(settings.clone());
                        (config.on_settings)(&settings);
                    }
                }
                Some(PageMessage::Wallpaper { id }) => {
                    let mut settings = config.settings.get();
                    if settings.look.wallpaper.as_deref() != Some(id.as_str()) && id.len() <= 300 {
                        settings.look.wallpaper = Some(id);
                        config.settings.set(settings.clone());
                        (config.on_settings)(&settings);
                    }
                }
                Some(PageMessage::Youtube { command }) => {
                    if command == youtube::Command::Close {
                        if let Some(closing) = youtube.take() {
                            closing.close(&stage);
                        }
                        youtube_message = None;
                        let _ = webview.post_json(&youtube::state_message(None));
                    } else if youtube.is_none() && matches!(command, youtube::Command::Hide) {
                        // Nothing to hide: YouTube isn't running (a page reloading says it anyway).
                    } else if let Some(player) = start_youtube(&mut youtube, &stage, &config) {
                        let _ = player.command(&command);
                        // YouTube Music shares the sign-in: its page again, signed out.
                        if command == youtube::Command::SignOut
                            && let Some(music) = &youtube_music
                        {
                            let _ = music.reload();
                        }
                    }
                }
                Some(PageMessage::Spotify { command }) => {
                    if spotify.is_none() && matches!(command, spotify::Command::Pause) {
                        // Nothing to pause: Spotify isn't running.
                    } else if let Some(player) = start_spotify(&mut spotify, &stage, &config) {
                        let _ = player.command(&command);
                    }
                }
                Some(PageMessage::AppleMusic { command }) => {
                    if apple_music.is_none() && matches!(command, apple_music::Command::Pause) {
                        // Nothing to pause: Apple Music isn't running.
                    } else if let Some(player) = start_apple_music(&mut apple_music, &stage, &config) {
                        let _ = player.command(&command);
                    }
                }
                Some(PageMessage::YoutubeMusic { command }) => {
                    if youtube_music.is_none() && matches!(command, youtube_music::Command::Pause) {
                        // Nothing to pause: YouTube Music isn't running.
                    } else if let Some(player) = start_youtube_music(&mut youtube_music, &stage, &config) {
                        let _ = player.command(&command);
                        // YouTube shares the sign-in: its page again, signed out.
                        if command == youtube_music::Command::SignOut
                            && let Some(youtube) = &youtube
                        {
                            let _ = youtube.reload();
                        }
                    }
                }
                Some(PageMessage::Typing { selection }) => {
                    bridge.bridge().typing.selection.store(selection, Ordering::Release);
                }
                Some(PageMessage::Route { id, action, node }) => {
                    let node = node.as_deref().and_then(|node| u64::from_str_radix(node, 16).ok()).unwrap_or(0);
                    let block = &bridge.bridge().route;
                    if let Some(overtaken) = route_changes.ask(block, id, action.into(), node, Instant::now()) {
                        let _ = webview.post_json(&overtaken);
                    }
                }
                None => {}
            }
        }
        if let Some(answer) = route_changes.answer(&bridge.bridge().route, Instant::now()) {
            let _ = webview.post_json(&answer);
        }

        // How YouTube's player plays, for the page; its WebView failing closes it (the page opens it again).
        if let Some(player) = &youtube {
            if player.failed() {
                if let Some(failed) = youtube.take() {
                    failed.close(&stage);
                }
                youtube_message = None;
                let _ = webview.post_json(&youtube::state_message(None));
            } else {
                // Every answer, and how the player plays now.
                let mut state = None;
                for report in player.take_reports() {
                    match report {
                        youtube::Report::Answer(answer) => {
                            let _ = webview.post_json(&youtube::answer_message(&answer));
                        }
                        youtube::Report::Account(account) => {
                            let message = youtube::account_message(&account);
                            let _ = webview.post_json(&message);
                            youtube_account_message = Some(message);
                            if status.youtube_signed_in != Some(account.signed_in) {
                                status.youtube_signed_in = Some(account.signed_in);
                                (config.on_status)(&status);
                            }
                        }
                        youtube::Report::SignInWindow(open) => {
                            let _ = webview.post_json(&youtube::sign_in_message(open));
                            youtube_window_open = open;
                            status.youtube_signing_in = youtube_window_open || youtube_confirming;
                            (config.on_status)(&status);
                        }
                        youtube::Report::Confirming(confirming) => {
                            // Signed in in YouTube's window: YouTube Music (the same sign-in) loads again too.
                            if confirming && let Some(music) = &youtube_music {
                                let _ = music.reload();
                            }
                            youtube_confirming = confirming;
                            status.youtube_signing_in = youtube_window_open || youtube_confirming;
                            (config.on_status)(&status);
                        }
                        youtube::Report::State(latest) => {
                            if youtube_was_playing && latest.state == 3 {
                                let video = latest.video.as_deref().unwrap_or("?");
                                log!("[youtube] {video} stopped to buffer at {:.1} s", latest.time);
                            }
                            youtube_was_playing = latest.state == 1;
                            state = Some(latest);
                        }
                    }
                }
                if let Some(state) = state {
                    let message = youtube::state_message(Some(&state));
                    let _ = webview.post_json(&message);
                    youtube_message = Some(message);
                }
            }
        }

        // What Spotify's page says (signed in, playing) and its sign-in window, for the page; its WebView
        // failing closes it (the page opens it again).
        if let Some(player) = &spotify {
            if player.failed() {
                if let Some(failed) = spotify.take() {
                    failed.close(&stage);
                }
                spotify_state = None;
                spotify_playback = None;
                status.spotify_signed_in = None;
                status.spotify_signing_in = false;
                spotify_confirming = false;
                (config.on_status)(&status);
                let _ = webview.post_json(&spotify::state_message(None, false));
                let _ = webview.post_json(&spotify::playback_message(None));
            } else {
                let mut state_changed = false;
                for report in player.take_reports() {
                    match report {
                        spotify::Report::State(state) => {
                            if status.spotify_signed_in != Some(state.signed_in) {
                                status.spotify_signed_in = Some(state.signed_in);
                                (config.on_status)(&status);
                            }
                            spotify_state = Some(state);
                            state_changed = true;
                        }
                        spotify::Report::SignInWindow(open) => {
                            spotify_signing_in = open;
                            status.spotify_signing_in = spotify_signing_in || spotify_confirming;
                            (config.on_status)(&status);
                            state_changed = true;
                        }
                        spotify::Report::Confirming(confirming) => {
                            spotify_confirming = confirming;
                            status.spotify_signing_in = spotify_signing_in || spotify_confirming;
                            (config.on_status)(&status);
                        }
                        // Answers and what plays go straight on to the page (never logged).
                        spotify::Report::Answer(answer) => {
                            let _ = webview.post_json(&spotify::answer_message(&answer));
                        }
                        spotify::Report::Playback(playback) => {
                            let message = spotify::playback_message(playback.as_deref());
                            let _ = webview.post_json(&message);
                            spotify_playback = Some(message);
                        }
                    }
                }
                if state_changed {
                    let _ = webview.post_json(&spotify::state_message(spotify_state.as_ref(), spotify_signing_in));
                }
            }
        }

        // What Apple Music's page says, and its sign-in window, for the page; its WebView failing closes it.
        if let Some(player) = &apple_music {
            if player.failed() {
                if let Some(failed) = apple_music.take() {
                    failed.close(&stage);
                }
                apple_music_state = None;
                apple_music_playback = None;
                (apple_music_window_open, apple_music_confirming) = (false, false);
                status.apple_music_signed_in = None;
                status.apple_music_signing_in = false;
                (config.on_status)(&status);
                let _ = webview.post_json(&apple_music::state_message(None, false));
                let _ = webview.post_json(&apple_music::playback_message(None));
            } else {
                let mut state_changed = false;
                for report in player.take_reports() {
                    match report {
                        apple_music::Report::State(state) => {
                            if status.apple_music_signed_in != Some(state.signed_in) {
                                status.apple_music_signed_in = Some(state.signed_in);
                                (config.on_status)(&status);
                            }
                            apple_music_state = Some(state);
                            state_changed = true;
                        }
                        apple_music::Report::SignInWindow(open) => {
                            apple_music_window_open = open;
                            state_changed = true;
                        }
                        // The cab shows it signing in until the page here says it's signed in (not signed out
                        // for a moment as it loads again).
                        apple_music::Report::Confirming(confirming) => {
                            apple_music_confirming = confirming;
                            state_changed = true;
                        }
                        // Answers and what plays go straight on to the page (never logged).
                        apple_music::Report::Answer(answer) => {
                            let _ = webview.post_json(&apple_music::answer_message(&answer));
                        }
                        // Times only (never what played): for the log.
                        apple_music::Report::Timing(timing) => log!("{}", timing.line()),
                        apple_music::Report::Playback(playback) => {
                            let message = apple_music::playback_message(playback.as_deref());
                            let _ = webview.post_json(&message);
                            apple_music_playback = Some(message);
                        }
                    }
                }
                if state_changed {
                    let signing_in = apple_music_window_open || apple_music_confirming;
                    if status.apple_music_signing_in != signing_in {
                        status.apple_music_signing_in = signing_in;
                        (config.on_status)(&status);
                    }
                    let message = apple_music::state_message(apple_music_state.as_ref(), signing_in);
                    let _ = webview.post_json(&message);
                }
            }
        }

        // What YouTube Music's page says, and its sign-in window, for the page; its WebView failing closes it.
        if let Some(player) = &youtube_music {
            if player.failed() {
                if let Some(failed) = youtube_music.take() {
                    failed.close(&stage);
                }
                youtube_music_state = None;
                youtube_music_playback = None;
                (youtube_music_window_open, youtube_music_confirming) = (false, false);
                let _ = webview.post_json(&youtube_music::state_message(None, false));
                let _ = webview.post_json(&youtube_music::playback_message(None));
            } else {
                let mut state_changed = false;
                for report in player.take_reports() {
                    match report {
                        youtube_music::Report::State(state) => {
                            // One sign-in for both: the companion hears it as YouTube's.
                            if status.youtube_signed_in != Some(state.signed_in) {
                                status.youtube_signed_in = Some(state.signed_in);
                                (config.on_status)(&status);
                            }
                            youtube_music_state = Some(state);
                            state_changed = true;
                        }
                        youtube_music::Report::SignInWindow(open) => {
                            youtube_music_window_open = open;
                            state_changed = true;
                        }
                        // The cab shows it signing in until the page here says it's signed in; YouTube (the same
                        // sign-in) loads again too.
                        youtube_music::Report::Confirming(confirming) => {
                            if confirming && let Some(youtube) = &youtube {
                                let _ = youtube.reload();
                            }
                            youtube_music_confirming = confirming;
                            state_changed = true;
                        }
                        // Answers and what plays go straight on to the page (never logged).
                        youtube_music::Report::Answer(answer) => {
                            let _ = webview.post_json(&youtube_music::answer_message(&answer));
                        }
                        // Times only (never what played): for the log.
                        youtube_music::Report::Timing(timing) => log!("{}", timing.line()),
                        youtube_music::Report::Playback(playback) => {
                            let message = youtube_music::playback_message(playback.as_deref());
                            let _ = webview.post_json(&message);
                            youtube_music_playback = Some(message);
                        }
                    }
                }
                if state_changed {
                    let signing_in = youtube_music_window_open || youtube_music_confirming;
                    let message = youtube_music::state_message(youtube_music_state.as_ref(), signing_in);
                    let _ = webview.post_json(&message);
                }
            }
        }

        // What the player typed on their own keyboard, and whether it types (the plugin decides: the truck
        // standing still, a field selected then).
        let typing_block = &bridge.bridge().typing;
        let typed: Vec<u32> = std::iter::from_fn(|| typing_block.keys.pop()).collect();
        if let Some(message) = typing::typed_message(&typed, typing::clipboard_text) {
            let _ = webview.post_json(&message);
        }
        let state = westplay_shared::TypingState::from_code(typing_block.state.load(Ordering::Acquire));
        if typing_state != Some(state) {
            typing_state = Some(state);
            let _ = webview.post_json(&typing::state_message(state));
        }

        // YouTube's, Apple Music's, YouTube Music's and Spotify's sound from the truck's own speakers, heard from the
        // driver's head (as last told, every 50 ms); muffled with the camera outside the cab, seen on every pass (as
        // quick as the game's cut).
        if let (Some(truck), Some(telemetry)) = (&last_truck, &last_telemetry) {
            let plugin = &bridge.bridge().plugin;
            let screen_frame = plugin.screen_frame.load(Ordering::Acquire);
            let game_frame = plugin.heartbeat.load(Ordering::Acquire);
            let (electrics, paused) = (telemetry.electric_enabled, telemetry.paused);
            let outside = camera.outside(truck.id.as_str(), screen_frame, game_frame, electrics, paused);
            let distance = camera::Camera::distance(telemetry, outside).unwrap_or(0.0) as f32;
            let turn = game_settings.camera().steering_turn(telemetry.steering);
            let head = speakers::Head::of(telemetry).turned(turn);
            if let Some(player) = &youtube {
                let _ = player.cab(truck.id.as_str(), head, outside, distance);
            }
            if let Some(player) = &apple_music {
                let _ = player.cab(truck.id.as_str(), head, outside, distance);
            }
            if let Some(player) = &youtube_music {
                let _ = player.cab(truck.id.as_str(), head, outside, distance);
            }
            if let Some(player) = &spotify {
                let _ = player.cab(truck.id.as_str(), head, outside, distance);
            }
        }

        if last_telemetry_post.elapsed() >= Duration::from_millis(50) {
            last_telemetry_post = Instant::now();
            let plugin = &bridge.bridge().plugin;
            // A write the game never finished (it crashed in the middle) reads as nothing: nothing to send.
            if let Some(telemetry) = plugin.telemetry.read()
                && last_telemetry != Some(telemetry)
            {
                last_telemetry = Some(telemetry);
                let message = serde_json::json!({ "type": "telemetry", "data": telemetry });
                let _ = webview.post_json(&message.to_string());
            }
            if let Some(truck) = plugin.truck.read()
                && last_truck != Some(truck)
            {
                if truck != TruckInfo::default() {
                    startup::mark("host: truck sent to the page");
                }
                last_truck = Some(truck);
                let message = serde_json::json!({ "type": "truck", "data": truck });
                let _ = webview.post_json(&message.to_string());
            }
            if let Some(job) = plugin.job.read()
                && last_job != Some(job)
            {
                last_job = Some(job);
                let message = serde_json::json!({ "type": "job", "data": job });
                let _ = webview.post_json(&message.to_string());
            }
            let layout = plugin.screen_layout.load(Ordering::Acquire);
            if last_layout != Some(layout) {
                last_layout = Some(layout);
                let _ = webview.post_json(&screen_message(layout));
            }
        }

        std::thread::sleep(Duration::from_millis(1));
    }

    host.frame_generation.store(0, Ordering::Release);
    host.screen_enabled.store(0, Ordering::Release);
    host.pid.store(0, Ordering::Release);
    drop(pipeline);
    if browser_gone {
        return Err(Box::new(BrowserGone));
    }
    Ok(())
}

/// Whether the game's plugin is alive, judged by its heartbeat (its frame count) having moved within
/// [`GAME_TIMEOUT`]. The first value seen proves nothing: a game that's gone may have left it.
#[derive(Default)]
struct Liveness {
    last: Option<(u64, Instant)>,
}

/// A game whose heartbeat stands still this long is taken to be gone (crashed, or stopped responding).
const GAME_TIMEOUT: Duration = Duration::from_secs(3);

impl Liveness {
    fn update(&mut self, heartbeat: u64, now: Instant) -> bool {
        match self.last {
            Some((seen, _)) if seen == heartbeat => {}
            Some(_) => self.last = Some((heartbeat, now)),
            None => self.last = Some((heartbeat, now.checked_sub(GAME_TIMEOUT).unwrap_or(now))),
        }
        self.last.is_some_and(|(_, moved)| now.duration_since(moved) < GAME_TIMEOUT)
    }
}

/// Being the one WestPlay running in this Windows session: two would both drive the game's bridge (taking
/// turns at its typed-key queue, which has room for one reader, and flipping the game between their frames).
/// Held for as long as the app runs; a WestPlay that's killed lets go with its process.
pub struct Instance(windows::Win32::Foundation::HANDLE);

// SAFETY: a handle to a named mutex, only ever closed (once, as it goes), from whichever thread.
unsafe impl Send for Instance {}

impl Instance {
    /// Claims being the one WestPlay, or `None` if another is running.
    pub fn claim() -> Option<Self> {
        Self::claim_named(windows::core::w!(r"Local\WestPlay.Instance"))
    }

    fn claim_named(name: windows::core::PCWSTR) -> Option<Self> {
        use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
        // SAFETY: a plain call: the name is a static string, the mutex is the process's own.
        let handle = unsafe { windows::Win32::System::Threading::CreateMutexW(None, false, name) }.ok()?;
        // SAFETY: reads this thread's last error, right after the call it's for.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: `handle` was just opened, and nothing else holds it.
            unsafe {
                let _ = CloseHandle(handle);
            }
            return None;
        }
        Some(Self(handle))
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        // SAFETY: the mutex is this `Instance`'s own, closed once, as it goes.
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

/// Tells the player WestPlay is already running (a second one was started), in a message box.
pub fn tell_already_running() {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_OK, MessageBoxW};
    // SAFETY: a message box with static strings, no owner window.
    unsafe {
        MessageBoxW(
            None,
            windows::core::w!("WestPlay is already running. Find it in the taskbar or the notification area."),
            windows::core::w!("WestPlay"),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

/// [`run`] stopped because WebView2's browser process exited, taking the cabin page's WebView with it: run it
/// again, on a new thread (the thread's composition, set up for the WebView, can't be set up twice).
#[derive(Debug)]
pub struct BrowserGone;

impl std::fmt::Display for BrowserGone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the cabin browser (WebView2) stopped")
    }
}

impl std::error::Error for BrowserGone {}

/// Logs the startup stages the host sees by watching: the game connecting, the first frame captured, the
/// game first drawing one on the truck's screen (its frame counter moving on from `screen_frame_before`).
/// Whether any are still to come.
fn log_startup(bridge: &BridgeMapping, pipeline: Option<&Pipeline>, screen_frame_before: u64) -> bool {
    let plugin = &bridge.bridge().plugin;
    let stages = [
        (plugin.pid.load(Ordering::Relaxed) != 0, "game: connected"),
        (pipeline.is_some_and(|p| p.capture.frames.load(Ordering::Relaxed) > 0), "host: first frame captured"),
        (
            plugin.screen_frame.load(Ordering::Relaxed) != screen_frame_before,
            "game: drawing WestPlay on the truck's screen",
        ),
    ];
    for (reached, stage) in stages {
        if reached {
            startup::mark(stage);
        }
    }
    !stages.iter().all(|(reached, _)| *reached)
}

/// Whether the process `game` (its pid; 0: no game) has the window in front. Without a game, it counts as
/// in front (nothing to be away from).
fn is_in_front(game: u32) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    if game == 0 {
        return true;
    }
    let mut owner = 0;
    // SAFETY: plain queries; `owner` is a local the call writes.
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut owner)) };
    // No window in front (the desktop switching): not known to be away.
    owner == 0 || owner == game
}

/// `{"type":"gameFocus","focused":…}`: whether the game's window is in front.
fn game_focus_message(focused: bool) -> String {
    serde_json::json!({ "type": "gameFocus", "focused": focused }).to_string()
}

/// YouTube, started as it's first needed (on the profile the cabin page's WebView uses), or `None` if it
/// couldn't start.
fn start_youtube<'a>(
    youtube: &'a mut Option<youtube::YouTube>,
    stage: &Stage,
    config: &Config,
) -> Option<&'a youtube::YouTube> {
    if youtube.is_none() {
        let user_data = config.data_dir.join("webview2");
        match youtube::YouTube::start(stage, config.width, config.height, &user_data) {
            Ok(started) => *youtube = Some(started),
            Err(e) => startup::mark(&format!("host: YouTube couldn't start: {e}")),
        }
    }
    youtube.as_ref()
}

/// Apple Music's web player, started as the page first shows it (on a WebView2 profile of its own), or `None`
/// if it couldn't start.
fn start_apple_music<'a>(
    apple_music: &'a mut Option<apple_music::AppleMusic>,
    stage: &Stage,
    config: &Config,
) -> Option<&'a apple_music::AppleMusic> {
    if apple_music.is_none() {
        let user_data = config.data_dir.join("webview2-apple-music");
        match apple_music::AppleMusic::start(stage, config.width, config.height, &user_data) {
            Ok(started) => *apple_music = Some(started),
            Err(e) => startup::mark(&format!("host: Apple Music couldn't start: {e}")),
        }
    }
    apple_music.as_ref()
}

/// YouTube Music's web player, started as the page first shows it (on the cabin page's profile, as YouTube's:
/// they share the sign-in), or `None` if it couldn't start.
fn start_youtube_music<'a>(
    youtube_music: &'a mut Option<youtube_music::YouTubeMusic>,
    stage: &Stage,
    config: &Config,
) -> Option<&'a youtube_music::YouTubeMusic> {
    if youtube_music.is_none() {
        let user_data = config.data_dir.join("webview2");
        match youtube_music::YouTubeMusic::start(stage, config.width, config.height, &user_data) {
            Ok(started) => *youtube_music = Some(started),
            Err(e) => startup::mark(&format!("host: YouTube Music couldn't start: {e}")),
        }
    }
    youtube_music.as_ref()
}

/// Spotify's web player, started as the page first shows it (on a WebView2 profile of its own: signing out
/// of Spotify forgets nothing else), or `None` if it couldn't start.
fn start_spotify<'a>(
    spotify: &'a mut Option<spotify::Spotify>,
    stage: &Stage,
    config: &Config,
) -> Option<&'a spotify::Spotify> {
    if spotify.is_none() {
        let user_data = config.data_dir.join("webview2-spotify");
        match spotify::Spotify::start(stage, config.width, config.height, &user_data) {
            Ok(started) => *spotify = Some(started),
            Err(e) => startup::mark(&format!("host: Spotify couldn't start: {e}")),
        }
    }
    spotify.as_ref()
}

fn build_pipeline(
    config: &Config,
    stage: &Stage,
    bridge: &Arc<BridgeMapping>,
    luid: Option<LUID>,
    generation: u32,
) -> Result<Pipeline, Error> {
    let gpu = Gpu::new(luid)?;
    let shared = Arc::new(SharedFrame::new(&gpu, config.width, config.height, generation)?);
    let capture = Capture::start(&gpu, &stage.visual()?, shared, bridge.clone())?;
    Ok(Pipeline { gpu, capture })
}

/// The GPU the game renders on, once the plugin has published it.
fn game_adapter(bridge: &BridgeMapping) -> Option<LUID> {
    let plugin = &bridge.bridge().plugin;
    if plugin.pid.load(Ordering::Acquire) == 0 {
        return None;
    }
    let luid = LUID {
        LowPart: plugin.adapter_luid_low.load(Ordering::Relaxed),
        HighPart: plugin.adapter_luid_high.load(Ordering::Relaxed),
    };
    (luid.LowPart != 0 || luid.HighPart != 0).then_some(luid)
}

/// `{"type":"screen","layout":"scania-2025"}`: which kind of screen the game draws WestPlay on, from the
/// bridge's `screen_layout` (`null` until a screen has been drawn).
fn screen_message(layout: u32) -> String {
    let id = (layout as usize).checked_sub(1).and_then(|index| screen_layouts().get(index)).map(|l| l.id);
    serde_json::json!({ "type": "screen", "layout": id }).to_string()
}

/// `{"type":"settings","truckStrip":"auto","wallpaper":"plain","iconSet":null,"iconStyle":"auto"}`: the
/// settings the cabin page applies itself.
fn settings_message(settings: &settings::Settings) -> String {
    serde_json::json!({
        "type": "settings",
        "truckStrip": settings.screen.truck_strip,
        "wallpaper": settings.look.wallpaper,
        "iconSet": settings.look.icon_set,
        "iconStyle": settings.look.icon_style,
    })
    .to_string()
}

/// `{"type":"customize","wallpapers":[…],"iconSets":[…]}`: the player's own wallpapers and icon sets
/// (`customize::Customizations`), their files at `https://westplay.customize/`.
fn customize_list_message(customizations: &customize::Customizations) -> String {
    let mut message = serde_json::to_value(customizations).unwrap_or_default();
    message["type"] = "customize".into();
    message.to_string()
}

/// Messages from the cabin page (`PageMessage` in ui/src/shared/host.ts).
#[derive(Debug, PartialEq, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum PageMessage {
    /// Posted when the page (re)loads.
    Ready,
    /// A startup stage the page reached (the boot screens shown), for the startup log (`startup`).
    Startup { stage: String },
    /// Whether WestPlay is open on the truck's screen: posted as the page loads and whenever it changes.
    Westplay { open: bool },
    /// The wallpaper chosen in the cab's Settings (an id: `wallpapers.ts`), to keep with the settings.
    Wallpaper { id: String },
    /// The truck's information bar beside WestPlay, kept or not, chosen in the cab's Settings (Appearance).
    TruckStrip { strip: settings::TruckStrip },
    /// A command for YouTube's player (`youtube`): what to play, where it shows, and so on.
    Youtube { command: youtube::Command },
    /// A command for Spotify's web player (`spotify`): its sign-in, a question, its controls.
    Spotify { command: spotify::Command },
    /// A command for Apple Music's web player (`appleMusic`): its sign-in, a question, its controls.
    AppleMusic { command: apple_music::Command },
    /// A command for YouTube Music's web player (`youtubeMusic`): its sign-in, a question, its controls.
    YoutubeMusic { command: youtube_music::Command },
    /// The text field selected on the page, a number that changes with every selection (0: none), for the
    /// player's own keyboard to type into (`typing`).
    Typing { selection: u32 },
    /// A change to the game's route (Maps' search): at a map node (its uid in hex) to go or add a stop; `id`
    /// comes back with the game's answer (`route_change`).
    Route { id: u32, action: PageRouteAction, node: Option<String> },
}

/// What the page asks of the game's route.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum PageRouteAction {
    AddStop,
    Go,
    End,
}

impl From<PageRouteAction> for westplay_shared::RouteAction {
    fn from(action: PageRouteAction) -> Self {
        match action {
            PageRouteAction::AddStop => Self::AddStop,
            PageRouteAction::Go => Self::Go,
            PageRouteAction::End => Self::End,
        }
    }
}

/// `{"type":"westplay","open":true}`: open (or close) WestPlay on the truck's screen.
fn westplay_message(open: bool) -> String {
    serde_json::json!({ "type": "westplay", "open": open }).to_string()
}

fn parse_page_message(message: &str) -> Option<PageMessage> {
    serde_json::from_str(message).ok()
}

/// What the player's latest save says, for the page: `{"type":"garages","owned":["calais",...],
/// "headquarters":"calais"}` (the cities whose garages they own, and their headquarters', `null` if none),
/// and `{"type":"mapBlock","hvt":true,"language":"tr_tr"}` (they play on a map by HVT, which WestPlay
/// doesn't allow, and the game's language to say so in).
fn player_messages_for(player: &Player) -> Vec<String> {
    let garages = &player.garages;
    vec![
        serde_json::json!({ "type": "garages", "owned": garages.owned, "headquarters": garages.headquarters })
            .to_string(),
        serde_json::json!({ "type": "mapBlock", "hvt": player.hvt_map, "language": player.language }).to_string(),
    ]
}

/// The game's weather for the page:
/// `{"type":"weather","weather":{"now":"clear","coming":"rain","changeIn":102,"wetness":0}}` (the weather
/// there is and the one it's turning to, game minutes to its next change and how wet the roads are in
/// percent, each `null` when not read), or `{"type":"weather","weather":null}` when it can't be read.
fn weather_message_for(weather: Option<Weather>) -> String {
    let sky = |sky: Sky| match sky {
        Sky::Clear => "clear",
        Sky::Rain => "rain",
    };
    let weather = weather.map(|w| {
        serde_json::json!({ "now": sky(w.now), "coming": sky(w.coming), "changeIn": w.change_in, "wetness": w.wetness })
    });
    serde_json::json!({ "type": "weather", "weather": weather }).to_string()
}

/// The game's route for the page: `{"type":"route","id":5,"nodes":[[x,z,forward x,forward z,distance
/// left],...]}` (its map nodes in order, metres; `"nodes":null` without a route), `{"type":"routePaths",
/// "id":5,"paths":[[2,[[x,z],...]],...],"maneuvers":[[2,"roundabout",-90.0,3],...],"shifts":[[3,6.5],...]}` (the ways through its
/// junctions and the turns there, each by the node it starts at: the turn, its angle, a roundabout's exit)
/// and `{"type":"routeNext","next":3}` (the next node ahead of the truck).
fn route_message_for(update: &RouteUpdate) -> String {
    match update {
        RouteUpdate::Route { id, nodes } => {
            let nodes: Vec<[f32; 5]> = nodes
                .iter()
                .map(|n| [n.position[0], n.position[1], n.forward[0], n.forward[1], n.distance_left])
                .collect();
            serde_json::json!({ "type": "route", "id": id, "nodes": nodes }).to_string()
        }
        RouteUpdate::Paths { id, paths, maneuvers, shifts } => {
            let maneuvers: Vec<serde_json::Value> = maneuvers
                .iter()
                .map(|(span, maneuver)| {
                    let exit = match maneuver.turn {
                        Turn::Roundabout(exit) => Some(exit),
                        _ => None,
                    };
                    serde_json::json!([span, maneuver.turn.name(), maneuver.angle.round(), exit])
                })
                .collect();
            let shifts: Vec<(usize, f32)> =
                shifts.iter().map(|&(node, shift)| (node, (shift * 100.0).round() / 100.0)).collect();
            serde_json::json!({ "type": "routePaths", "id": id, "paths": paths, "maneuvers": maneuvers, "shifts": shifts })
                .to_string()
        }
        RouteUpdate::Next(next) => serde_json::json!({ "type": "routeNext", "next": next }).to_string(),
        RouteUpdate::None => serde_json::json!({ "type": "route", "id": null, "nodes": null }).to_string(),
    }
}

fn same_luid(a: LUID, b: LUID) -> bool {
    a.LowPart == b.LowPart && a.HighPart == b.HighPart
}

fn pump_messages() {
    // SAFETY: this thread's own messages, each passed on as Windows hands it; `msg` is a local.
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PageMessage, PageRouteAction, parse_page_message, screen_message, settings_message};

    #[test]
    fn reads_the_pages_messages() {
        assert_eq!(parse_page_message(r#"{"type":"ready"}"#), Some(PageMessage::Ready));
        // The PC's own music apps aren't followed (removed 2026-10-17): their controls are no message.
        assert_eq!(parse_page_message(r#"{"type":"media","source":"AppleMusic.exe","action":"playPause"}"#), None);
        assert_eq!(
            parse_page_message(r#"{"type":"startup","stage":"power splash"}"#),
            Some(PageMessage::Startup { stage: "power splash".into() })
        );
        assert_eq!(
            parse_page_message(r#"{"type":"westplay","open":false}"#),
            Some(PageMessage::Westplay { open: false })
        );
        assert_eq!(parse_page_message(r#"{"type":"westplay"}"#), None);
        assert_eq!(
            parse_page_message(r#"{"type":"wallpaper","id":"custom:Dusk"}"#),
            Some(PageMessage::Wallpaper { id: "custom:Dusk".into() })
        );
        assert_eq!(
            parse_page_message(r#"{"type":"truckStrip","strip":"off"}"#),
            Some(PageMessage::TruckStrip { strip: crate::settings::TruckStrip::Off })
        );
        assert_eq!(parse_page_message(r#"{"type":"truckStrip","strip":"sideways"}"#), None);
        assert_eq!(
            parse_page_message(r#"{"type":"route","id":3,"action":"addStop","node":"478ef4c4cd160002"}"#),
            Some(PageMessage::Route { id: 3, action: PageRouteAction::AddStop, node: Some("478ef4c4cd160002".into()) })
        );
        assert_eq!(
            parse_page_message(r#"{"type":"route","id":4,"action":"end"}"#),
            Some(PageMessage::Route { id: 4, action: PageRouteAction::End, node: None })
        );
        assert_eq!(parse_page_message(r#"{"type":"route","id":5,"action":"teleport"}"#), None);
        assert_eq!(
            parse_page_message(r#"{"type":"typing","selection":3}"#),
            Some(PageMessage::Typing { selection: 3 })
        );
        assert_eq!(
            parse_page_message(r#"{"type":"youtube","command":{"action":"pause"}}"#),
            Some(PageMessage::Youtube { command: crate::youtube::Command::Pause })
        );
        assert_eq!(parse_page_message(r#"{"type":"other"}"#), None);
        assert_eq!(parse_page_message("not json"), None);
    }

    #[test]
    fn only_one_westplay_claims_the_session() {
        // A name of the test's own (a WestPlay may be running on this PC).
        let name = windows::core::w!(r"Local\WestPlay.Instance.Test");
        let first = super::Instance::claim_named(name).expect("the first claims it");
        assert!(super::Instance::claim_named(name).is_none(), "a second must not");
        drop(first);
        assert!(super::Instance::claim_named(name).is_some(), "once let go, it can be claimed again");
    }

    #[test]
    fn a_game_is_alive_while_its_heartbeat_moves() {
        use std::time::{Duration, Instant};
        let start = Instant::now() + Duration::from_secs(60);
        let mut liveness = super::Liveness::default();
        // A heartbeat already there proves nothing (a crashed game leaves it behind).
        assert!(!liveness.update(500, start));
        assert!(!liveness.update(500, start + Duration::from_secs(1)));
        // Moving: alive.
        assert!(liveness.update(560, start + Duration::from_secs(2)));
        assert!(liveness.update(560, start + Duration::from_secs(4)));
        // Standing still past the timeout (the game crashed): gone.
        assert!(!liveness.update(560, start + Duration::from_secs(2) + super::GAME_TIMEOUT));
        // Moving again (a new game): alive.
        assert!(liveness.update(3, start + Duration::from_secs(20)));
    }

    #[test]
    fn passes_the_latest_open_request_on_once() {
        let request = super::OpenRequest::default();
        assert_eq!(request.take(), None);
        request.request(false);
        request.request(true);
        assert_eq!(request.take(), Some(true));
        assert_eq!(request.take(), None);
        request.request(false);
        assert_eq!(request.take(), Some(false));
        assert_eq!(super::westplay_message(true), r#"{"open":true,"type":"westplay"}"#);
    }

    #[test]
    fn passes_on_the_companions_sign_ins_once() {
        use super::{AccountAction, AccountRequests};
        let requests = AccountRequests::default();
        assert_eq!(requests.youtube.take(), None);
        requests.youtube.request(AccountAction::SignOut);
        requests.youtube.request(AccountAction::SignIn);
        requests.spotify.request(AccountAction::Check);
        assert_eq!(requests.youtube.take(), Some(AccountAction::SignIn));
        assert_eq!(requests.youtube.take(), None);
        assert_eq!(requests.spotify.take(), Some(AccountAction::Check));
        assert_eq!(requests.spotify.take(), None);
        // The companion hears whether YouTube is signed in, and nothing more of the account.
        let status = super::Status { youtube_signed_in: Some(true), youtube_signing_in: false, ..Default::default() };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!((json["youtubeSignedIn"].as_bool(), json["youtubeSigningIn"].as_bool()), (Some(true), Some(false)));
        let status = super::Status { spotify_signed_in: Some(false), spotify_signing_in: true, ..Default::default() };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!((json["spotifySignedIn"].as_bool(), json["spotifySigningIn"].as_bool()), (Some(false), Some(true)));
        let status =
            super::Status { apple_music_signed_in: Some(true), apple_music_signing_in: true, ..Default::default() };
        let json = serde_json::to_value(&status).unwrap();
        let apple_music = (json["appleMusicSignedIn"].as_bool(), json["appleMusicSigningIn"].as_bool());
        assert_eq!(apple_music, (Some(true), Some(true)));
    }

    #[test]
    fn reports_whether_westplay_is_open_to_the_companion() {
        let status = super::Status { westplay_open: Some(false), ..Default::default() };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["westplayOpen"], serde_json::json!(false));
        let unknown = serde_json::to_value(super::Status::default()).unwrap();
        assert_eq!(unknown["westplayOpen"], serde_json::Value::Null);
    }

    #[test]
    fn tells_the_page_the_truck_strip_wallpaper_and_icons() {
        let mut settings = crate::settings::Settings::default();
        let message: serde_json::Value = serde_json::from_str(&settings_message(&settings)).unwrap();
        assert_eq!(
            message,
            serde_json::json!({
                "type": "settings",
                "truckStrip": "auto",
                "wallpaper": null,
                "iconSet": null,
                "iconStyle": "auto",
            })
        );
        settings.screen.truck_strip = crate::settings::TruckStrip::Off;
        settings.look.wallpaper = Some("custom:Dusk".into());
        settings.look.icon_set = Some("Retro".into());
        let message: serde_json::Value = serde_json::from_str(&settings_message(&settings)).unwrap();
        assert_eq!(message["truckStrip"], "off");
        assert_eq!(message["wallpaper"], "custom:Dusk");
        assert_eq!(message["iconSet"], "Retro");
    }

    #[test]
    fn tells_the_page_the_players_wallpapers_and_icon_sets() {
        let customizations = crate::customize::Customizations {
            wallpapers: vec![crate::customize::Wallpaper {
                id: "custom:Dusk".into(),
                name: "Dusk".into(),
                dark: Some("wallpapers/Dusk.dark.jpg".into()),
                light: None,
            }],
            icon_sets: vec![crate::customize::IconSet {
                name: "Retro".into(),
                icons: [(
                    "youtubeMusic",
                    crate::customize::Icon { light: None, dark: Some("icons/Retro/youtube-music.dark.png".into()) },
                )]
                .into(),
            }],
        };
        let message: serde_json::Value = serde_json::from_str(&super::customize_list_message(&customizations)).unwrap();
        assert_eq!(
            message,
            serde_json::json!({
                "type": "customize",
                "wallpapers": [{ "id": "custom:Dusk", "name": "Dusk", "dark": "wallpapers/Dusk.dark.jpg", "light": null }],
                "iconSets": [{ "name": "Retro", "icons": { "youtubeMusic": { "light": null, "dark": "icons/Retro/youtube-music.dark.png" } } }],
            })
        );
    }

    #[test]
    fn player_messages_list_the_owned_garages_and_a_blocked_map() {
        let player = crate::player::Player {
            garages: westplay_mapdata::save::Garages {
                owned: ["calais".to_owned(), "dover".to_owned()].into(),
                headquarters: Some("calais".into()),
            },
            hvt_map: true,
            language: Some("tr_tr".into()),
        };
        assert_eq!(
            super::player_messages_for(&player),
            [
                r#"{"headquarters":"calais","owned":["calais","dover"],"type":"garages"}"#,
                r#"{"hvt":true,"language":"tr_tr","type":"mapBlock"}"#
            ]
        );
    }

    #[test]
    fn weather_messages_say_the_weather_there_is_and_coming() {
        use crate::weather::{Sky, Weather};
        let turning = Weather { change_in: Some(102), wetness: Some(0), ..Weather::sky(Sky::Clear, Sky::Rain) };
        let message: serde_json::Value = serde_json::from_str(&super::weather_message_for(Some(turning))).unwrap();
        assert_eq!(
            message,
            serde_json::json!({
                "type": "weather",
                "weather": { "now": "clear", "coming": "rain", "changeIn": 102, "wetness": 0 }
            })
        );
        let unknown: serde_json::Value =
            serde_json::from_str(&super::weather_message_for(Some(Weather::sky(Sky::Rain, Sky::Rain)))).unwrap();
        assert_eq!(unknown["weather"]["changeIn"], serde_json::Value::Null);
        assert_eq!(super::weather_message_for(None), r#"{"type":"weather","weather":null}"#);
    }

    #[test]
    fn route_messages_carry_the_nodes_and_the_progress() {
        use crate::route::{RouteNode, RouteUpdate};
        use westplay_mapdata::junctions::{Maneuver, Turn};
        let node = RouteNode { position: [1.0, 2.0], forward: [0.0, -1.0], uid: 7, distance_left: 1500.0 };
        assert_eq!(
            super::route_message_for(&RouteUpdate::Route { id: 5, nodes: vec![node] }),
            r#"{"id":5,"nodes":[[1.0,2.0,0.0,-1.0,1500.0]],"type":"route"}"#
        );
        assert_eq!(super::route_message_for(&RouteUpdate::Next(3)), r#"{"next":3,"type":"routeNext"}"#);
        assert_eq!(
            super::route_message_for(&RouteUpdate::Paths {
                id: 5,
                paths: vec![(2, vec![[1.0, 2.0], [3.0, 4.0]])],
                maneuvers: vec![
                    (2, Maneuver { turn: Turn::Roundabout(3), angle: -89.6 }),
                    (7, Maneuver { turn: Turn::ExitRight, angle: 17.2 }),
                ],
                shifts: vec![(3, 6.5)],
            }),
            r#"{"id":5,"maneuvers":[[2,"roundabout",-90.0,3],[7,"exit-right",17.0,null]],"paths":[[2,[[1.0,2.0],[3.0,4.0]]]],"shifts":[[3,6.5]],"type":"routePaths"}"#
        );
        assert_eq!(super::route_message_for(&RouteUpdate::None), r#"{"id":null,"nodes":null,"type":"route"}"#);
    }

    #[test]
    fn screen_message_names_the_layout() {
        assert_eq!(screen_message(0), r#"{"layout":null,"type":"screen"}"#);
        assert_eq!(screen_message(1), r#"{"layout":"generic","type":"screen"}"#);
        assert_eq!(screen_message(999), r#"{"layout":null,"type":"screen"}"#);
    }
}
