//! What WestPlay costs while the game runs, written to the log every 10 seconds (PLAN.md: Low-end PCs): the
//! game's frame rate, how much of the time the game showed WestPlay's screen, the cab page's own frames (its
//! animation frames), how many Windows captured of it and how many went to the game, the processor time each of
//! WestPlay's processes took, and what Chromium draws the cab with (a GPU, or the processor itself where it
//! doesn't trust the graphics driver). The same line from a player's log as from a measurement on a fast PC, so
//! the two compare; short enough a window for a few seconds' freeze to show (players' screens at a few frames a
//! second, 2026-10-27, were a minute's average).
//!
//! The page's frames beside those captured say which is slow when the screen is: the page itself (Chromium
//! given too little of the processor or the graphics card), or Windows' capture of it.
//!
//! Processor time is in cores: a process busy all the time on one core took 1.0. Nothing of what the player
//! plays or browses: a page is named by what it is (the cab, YouTube), never by its address.
//!
//! Once a run, the Windows build and the displays' rates, which change what the lines mean.
//!
//! Also, as Windows counts it for Task Manager, how busy the graphics card's 3D engine was, and how much of that was
//! the game's and WestPlay's: a page waiting while the game keeps the card busy (a player's Maps at 5-15 frames a
//! second with the page using almost nothing, 2026-11-09) shows as such. And which screen the cab showed, and how
//! many of the page's frames came late.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use webview2_com::GetProcessExtendedInfosCompletedHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PROCESS_KIND, COREWEBVIEW2_PROCESS_KIND_BROWSER, COREWEBVIEW2_PROCESS_KIND_GPU,
    COREWEBVIEW2_PROCESS_KIND_RENDERER, ICoreWebView2Environment, ICoreWebView2Environment13,
    ICoreWebView2ProcessExtendedInfo, ICoreWebView2ProcessExtendedInfoCollection,
};
use westplay_setup::places::registry_text;
use windows::Win32::Foundation::{CloseHandle, FILETIME};
use windows::Win32::Graphics::Gdi::{
    DEVMODEW, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS, EnumDisplayDevicesW,
    EnumDisplaySettingsW,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW,
    PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};
use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows::Win32::System::Threading::{
    GetProcessTimes, GetSystemTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::{BOOL, Interface, PCWSTR, PWSTR, w};

use crate::webview::OffscreenWebView;

/// How often the line is written.
const WINDOW: Duration = Duration::from_secs(10);
/// Where the page's frames go in the line, once the page has said them.
const PAGE: &str = "\u{1}page\u{1}";

/// A WebView2 profile WestPlay runs, its processes counted under its name.
#[derive(Clone, Copy)]
pub enum Profile {
    /// The cab's own, which YouTube and YouTube Music share.
    Cab,
    Spotify,
    AppleMusic,
}

/// What the host knows each second while the game runs.
pub struct Sample {
    /// The game's frames presented so far (the plugin's heartbeat).
    pub game_frames: u64,
    /// Whether the game drew WestPlay's screen lately.
    pub in_view: bool,
    /// Frames sent to the game so far.
    pub sent: u64,
    /// Frames Windows' capture delivered so far, and times one waited for the game (copying the one before) to
    /// go out: a screen that freezes is either captured too rarely or kept from the game.
    pub captured: u64,
    pub waited: u64,
}

pub struct Meter {
    /// The cab page's host (`westplay.cabin`; a development server's), to know its renderer by.
    cab_host: String,
    window: Option<Window>,
    /// The part of WestPlay each WebView2 process is, as WebView2 last said (process id → part).
    parts: Rc<RefCell<HashMap<u32, &'static str>>>,
    /// What Chromium draws the cab with (WebGL's renderer), once the page has said.
    renderer: Rc<RefCell<Option<String>>>,
    renderer_asked: bool,
    /// The graphics card's engines' use, as Windows counts it (`None`: it can't be read here).
    gpu: Option<GpuUse>,
    gpu_tried: bool,
}

/// A minute being measured: where each count stood at its start.
struct Window {
    started: Instant,
    game_frames: u64,
    sent: u64,
    captured: u64,
    waited: u64,
    seconds: u32,
    in_view: u32,
    cpu: HashMap<u32, u64>,
    /// The whole processor's times at its start (idle, busy: Windows' own count, every program's).
    system: Option<(u64, u64)>,
    /// The game's processes' processor times at its start.
    game_cpu: HashMap<u32, u64>,
}

impl Meter {
    pub fn new(cab_host: String) -> Self {
        Self {
            cab_host,
            window: None,
            parts: Rc::default(),
            renderer: Rc::default(),
            renderer_asked: false,
            gpu: None,
            gpu_tried: false,
        }
    }

    /// Called each second: `sample` while the game runs (`None`: it doesn't, and nothing is measured).
    /// `profiles` are the WebView2 profiles running, to name their processes by.
    pub fn second(
        &mut self,
        now: Instant,
        sample: Option<Sample>,
        cab: &OffscreenWebView,
        profiles: &[(Profile, &ICoreWebView2Environment)],
    ) {
        let Some(sample) = sample else {
            self.window = None;
            return;
        };
        if !self.renderer_asked {
            self.renderer_asked = true;
            self.ask_renderer(cab);
            crate::log!("this PC: Windows {}; displays {}", windows_build(), displays());
        }
        let Some(window) = self.window.as_mut() else {
            self.begin(now, &sample, profiles);
            return;
        };
        window.seconds += 1;
        window.in_view += u32::from(sample.in_view);
        if now.duration_since(window.started) >= WINDOW {
            let seconds = now.duration_since(window.started).as_secs_f64();
            let line = self.line(now, &sample);
            // The page's frames since the last line, its late ones and the screens it showed (its counts start again
            // as they're taken: state/frameMeter.ts).
            let asked = cab.run_script("window.westplayReport?.() ?? null", {
                let line = line.clone();
                move |json| {
                    let report = serde_json::from_str::<PageReport>(&json).ok();
                    crate::log!("{}", with_page(&line, report.as_ref(), seconds));
                }
            });
            if asked.is_err() {
                crate::log!("{}", with_page(&line, None, seconds));
            }
            self.begin(now, &sample, profiles);
        }
    }

    fn begin(&mut self, now: Instant, sample: &Sample, profiles: &[(Profile, &ICoreWebView2Environment)]) {
        if !self.gpu_tried {
            self.gpu_tried = true;
            self.gpu = GpuUse::open();
            if self.gpu.is_none() {
                crate::log!("the graphics card's use can't be read here (Windows' GPU counters)");
            }
        }
        // Its counts from now: the next line's are over the window.
        if let Some(gpu) = &self.gpu {
            gpu.collect();
        }
        self.window = Some(Window {
            started: now,
            game_frames: sample.game_frames,
            sent: sample.sent,
            captured: sample.captured,
            waited: sample.waited,
            seconds: 0,
            in_view: 0,
            cpu: processor_times(&processes()),
            system: system_times(),
            game_cpu: processor_times(&game_processes()),
        });
        self.parts.borrow_mut().clear();
        for &(profile, environment) in profiles {
            self.ask_parts(profile, environment);
        }
    }

    /// The minute's line.
    fn line(&self, now: Instant, sample: &Sample) -> String {
        let Some(window) = &self.window else { return String::new() };
        let seconds = now.duration_since(window.started).as_secs_f64();
        let game = sample.game_frames.saturating_sub(window.game_frames) as f64 / seconds;
        let sent = sample.sent.saturating_sub(window.sent) as f64 / seconds;
        let captured = sample.captured.saturating_sub(window.captured) as f64 / seconds;
        let waited = sample.waited.saturating_sub(window.waited) as f64 / seconds;
        let in_view = 100 * window.in_view / window.seconds.max(1);
        let ours = std::process::id();
        let parts = self.parts.borrow();
        let tree = process_tree();
        let browsers = crate::webview::host_browsers();
        let used = cores_by_part(&window.cpu, &processor_times(&descendants(ours, &tree)), seconds, |pid| {
            part_of(pid, ours, &tree, &parts, &browsers)
        });
        let total: f64 = used.iter().map(|(_, cores)| cores).sum();
        let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
        let each = used
            .iter()
            .filter(|(_, cores)| *cores >= 0.005)
            .map(|(part, cores)| format!("{part} {cores:.2}"))
            .collect::<Vec<_>>()
            .join(", ");
        let renderer = self.renderer.borrow().clone().unwrap_or_else(|| "not known yet".into());
        // How busy the whole processor was, every program's: a page waiting for a free core shows as such (a
        // player's processor busy, the graphics card at 96%, 2026-11-09).
        let system = window
            .system
            .zip(system_times())
            .and_then(|((idle0, busy0), (idle1, busy1))| {
                let (idle, busy) = (idle1.saturating_sub(idle0), busy1.saturating_sub(busy0));
                (busy > 0).then(|| 100.0 * busy.saturating_sub(idle) as f64 / busy as f64)
            })
            .map_or(String::new(), |percent| format!("; the processor {percent:.0}% busy (every program)"));
        // The game's own share, in threads: one of its threads busy all the time (ETS2's own, held up by its mods)
        // is a limit the whole processor's figure hides.
        let game_now = processor_times(&game_processes());
        let game_cpu: u64 = game_now
            .iter()
            .filter_map(|(pid, now)| window.game_cpu.get(pid).map(|then| now.saturating_sub(*then)))
            .sum();
        let system = if game_now.is_empty() {
            system
        } else {
            format!("{system}, the game {:.2} threads", game_cpu as f64 / 1e7 / seconds)
        };
        let gpu = self.gpu.as_ref().and_then(|gpu| gpu.read()).map_or(String::new(), |used| {
            let ours: Vec<u32> = descendants(ours, &tree);
            let games = game_processes();
            gpu_said(&used, &games, &ours)
        });
        format!(
            "performance, the last {seconds:.0} s: the game {game:.0} fps; WestPlay's screen in view {in_view}% of \
             the time, {PAGE}{sent:.0} frames a second sent ({captured:.0} captured, {waited:.0} waited for the game); \
             WestPlay used {total:.2} of {threads} processor threads ({each}){system}{gpu}; the cab is drawn with \
             {renderer}"
        )
    }

    /// Asks the cab page what Chromium draws with: the GPU WebGL runs on, or its software renderer.
    fn ask_renderer(&self, cab: &OffscreenWebView) {
        let renderer = self.renderer.clone();
        let script = "(() => { try { const gl = document.createElement('canvas').getContext('webgl'); \
                      if (!gl) return 'no WebGL'; \
                      const info = gl.getExtension('WEBGL_debug_renderer_info'); \
                      return gl.getParameter(info ? info.UNMASKED_RENDERER_WEBGL : gl.RENDERER) } \
                      catch (e) { return 'no WebGL' } })()";
        let asked = cab.run_script(script, move |json| {
            let named = serde_json::from_str::<String>(&json).unwrap_or(json);
            crate::log!("the cab is drawn with {named}");
            *renderer.borrow_mut() = Some(described(named));
        });
        if let Err(e) = asked {
            crate::log!("the cab's renderer couldn't be asked: {e}");
        }
    }

    /// Asks WebView2 which of `profile`'s processes is which (it answers later, on this thread).
    fn ask_parts(&self, profile: Profile, environment: &ICoreWebView2Environment) {
        // Older WebView2 runtimes can't say: their processes count as "other".
        let Ok(environment) = environment.cast::<ICoreWebView2Environment13>() else { return };
        let parts = self.parts.clone();
        let cab_host = self.cab_host.clone();
        let handler = GetProcessExtendedInfosCompletedHandler::create(Box::new(move |error, infos| {
            if let (Ok(()), Some(infos)) = (error, infos) {
                for (pid, part) in parts_of(&infos, profile, &cab_host) {
                    parts.borrow_mut().insert(pid, part);
                }
            }
            Ok(())
        }));
        // SAFETY: the handler is a local for the call; WebView2 keeps its own reference to it.
        let _ = unsafe { environment.GetProcessExtendedInfos(&handler) };
    }
}

/// The Windows build: capture delivers frames differently from one to another (on Windows 10 at full rate over a
/// page that doesn't change; on a player's PC, none).
fn windows_build() -> String {
    let key = w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");
    let build = registry_text(HKEY_LOCAL_MACHINE, key, w!("CurrentBuild"));
    let version = registry_text(HKEY_LOCAL_MACHINE, key, w!("DisplayVersion"));
    match (build, version) {
        (Some(build), Some(version)) => format!("build {build} ({version})"),
        (Some(build), None) => format!("build {build}"),
        (None, _) => "of an unknown build".into(),
    }
}

/// The displays in use, each with its rate: the cab's map draws more often on a fast one.
fn displays() -> String {
    let mut found = Vec::new();
    let mut index = 0;
    loop {
        let mut display = DISPLAY_DEVICEW { cb: size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
        // SAFETY: `display` is a local the call fills, its size said.
        if !unsafe { EnumDisplayDevicesW(None, index, &mut display, 0) }.as_bool() {
            break;
        }
        index += 1;
        if !display.StateFlags.contains(DISPLAY_DEVICE_ATTACHED_TO_DESKTOP) {
            continue;
        }
        let mut mode = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..Default::default() };
        let name = PCWSTR(display.DeviceName.as_ptr());
        // SAFETY: `name` is the display's, ended by a zero as Windows wrote it; `mode` is a local the call fills,
        // its size said.
        if unsafe { EnumDisplaySettingsW(name, ENUM_CURRENT_SETTINGS, &mut mode) }.as_bool() {
            found.push(format!("{}x{} at {} Hz", mode.dmPelsWidth, mode.dmPelsHeight, mode.dmDisplayFrequency));
        }
    }
    found.join(", ")
}

/// The renderer as the line says it: a software one named for what it means.
fn described(renderer: String) -> String {
    let software = ["SwiftShader", "Microsoft Basic Render", "llvmpipe"];
    if software.iter().any(|name| renderer.contains(name)) {
        format!("{renderer} (no GPU: Chromium draws on the processor)")
    } else {
        renderer
    }
}

/// Each process of a WebView2 profile, and the part of WestPlay it is.
fn parts_of(
    infos: &ICoreWebView2ProcessExtendedInfoCollection,
    profile: Profile,
    cab_host: &str,
) -> Vec<(u32, &'static str)> {
    let mut count = 0;
    // SAFETY: `count` is a local the call writes.
    if unsafe { infos.Count(&mut count) }.is_err() {
        return Vec::new();
    }
    (0..count)
        .filter_map(|index| {
            // SAFETY: an index below the collection's count.
            let info = unsafe { infos.GetValueAtIndex(index) }.ok()?;
            let (pid, kind) = process_of(&info)?;
            let part = match profile {
                Profile::Spotify => "Spotify",
                Profile::AppleMusic => "Apple Music",
                Profile::Cab => match kind {
                    COREWEBVIEW2_PROCESS_KIND_BROWSER => "browser",
                    COREWEBVIEW2_PROCESS_KIND_GPU => "GPU",
                    COREWEBVIEW2_PROCESS_KIND_RENDERER => page_part(&frame_hosts(&info), cab_host),
                    _ => "helpers",
                },
            };
            Some((pid, part))
        })
        .collect()
}

/// What a renderer of the cab's profile draws, from the hosts of its pages.
fn page_part(hosts: &[String], cab_host: &str) -> &'static str {
    if hosts.iter().any(|host| host == cab_host) {
        "cab page"
    } else if hosts.iter().any(|host| host == "youtube.com" || host.ends_with(".youtube.com")) {
        "YouTube"
    } else {
        "other pages"
    }
}

fn process_of(info: &ICoreWebView2ProcessExtendedInfo) -> Option<(u32, COREWEBVIEW2_PROCESS_KIND)> {
    // SAFETY: plain queries; `pid` and `kind` are locals the calls write.
    unsafe {
        let process = info.ProcessInfo().ok()?;
        let (mut pid, mut kind) = (0, COREWEBVIEW2_PROCESS_KIND::default());
        process.ProcessId(&mut pid).ok()?;
        process.Kind(&mut kind).ok()?;
        Some((pid as u32, kind))
    }
}

/// The hosts of the pages a renderer draws.
fn frame_hosts(info: &ICoreWebView2ProcessExtendedInfo) -> Vec<String> {
    let mut hosts = Vec::new();
    // SAFETY: plain queries of WebView2's own collection; the strings it writes are freed by `CoTaskMemPWSTR`.
    unsafe {
        let Ok(frames) = info.AssociatedFrameInfos() else { return hosts };
        let Ok(frames) = frames.GetIterator() else { return hosts };
        let mut more = BOOL::default();
        while frames.HasCurrent(&mut more).is_ok() && more.as_bool() {
            if let Ok(frame) = frames.GetCurrent() {
                let mut source = PWSTR::null();
                if frame.Source(&mut source).is_ok() {
                    hosts.extend(host_of(&webview2_com::CoTaskMemPWSTR::from(source).to_string()));
                }
            }
            if frames.MoveNext(&mut more).is_err() {
                break;
            }
        }
    }
    hosts
}

/// The host of an address (`https://westplay.cabin/index.html`: `westplay.cabin`).
pub fn host_of(address: &str) -> Option<String> {
    let (_, rest) = address.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit_once('@').map_or(host, |(_, host)| host);
    let host = host.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// The processor time each part took between `before` and `after` (each process's, by its id), in cores over
/// `seconds`, the most first. A process new since `before` counts all its time.
fn cores_by_part<'a>(
    before: &HashMap<u32, u64>,
    after: &HashMap<u32, u64>,
    seconds: f64,
    part: impl Fn(u32) -> &'a str,
) -> Vec<(&'a str, f64)> {
    let mut parts: Vec<(&str, f64)> = Vec::new();
    for (&pid, &time) in after {
        let used = time.saturating_sub(before.get(&pid).copied().unwrap_or(0));
        let cores = used as f64 / 10_000_000.0 / seconds;
        let name = part(pid);
        match parts.iter_mut().find(|(each, _)| *each == name) {
            Some((_, sum)) => *sum += cores,
            None => parts.push((name, cores)),
        }
    }
    parts.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
    parts
}

/// What the page says with each line (state/frameMeter.ts `FrameReport`).
#[derive(Debug, Default, serde::Deserialize)]
struct PageReport {
    frames: f64,
    #[serde(default)]
    slow: f64,
    /// The seconds each screen was shown.
    #[serde(default)]
    screens: std::collections::BTreeMap<String, u32>,
}

/// The line with the page's frames in it (over `seconds`), those that came late, and the screens it showed, most
/// shown first (`None`: the page didn't say).
fn with_page(line: &str, report: Option<&PageReport>, seconds: f64) -> String {
    let page = report.map_or(String::new(), |report| {
        let mut screens: Vec<(&String, &u32)> = report.screens.iter().collect();
        screens.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let shown = screens
            .iter()
            // A screen's own name only (an app's id): nothing the player typed or played.
            .map(|(name, seconds)| {
                format!(
                    "{} {seconds} s",
                    name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(24).collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let shown = if shown.is_empty() { String::new() } else { format!(", showing {shown}") };
        format!(
            "the page drew {:.0} frames a second ({:.0} of them late, 50 ms or more){shown}, ",
            report.frames / seconds,
            report.slow
        )
    });
    line.replacen(PAGE, &page, 1)
}

/// How busy the graphics card's 3D engine was over the window, as Windows counts it for each process (process id →
/// percent of the time): all of it, the game's, and WestPlay's (`ours`).
fn gpu_said(used: &HashMap<u32, f64>, games: &[u32], ours: &[u32]) -> String {
    let all: f64 = used.values().sum();
    let of = |pids: &[u32]| pids.iter().filter_map(|pid| used.get(pid)).sum::<f64>();
    format!(
        "; the graphics card's 3D engine busy {:.0}% of the time (the game {:.0}%, WestPlay {:.0}%)",
        all.min(100.0),
        of(games),
        of(ours)
    )
}

/// The whole processor's idle time and its total time so far (kernel time includes idle), every core's, 100 ns
/// units: `GetSystemTimes`.
fn system_times() -> Option<(u64, u64)> {
    let ticks = |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: the times are locals it writes.
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()? };
    Some((ticks(idle), ticks(kernel) + ticks(user)))
}

/// The game's processes (Euro Truck Simulator 2's, American Truck Simulator's), as Windows lists them now.
fn game_processes() -> Vec<u32> {
    let mut found = Vec::new();
    // SAFETY: a snapshot of the process list, walked with an entry sized as the calls require, then closed.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return found };
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let length = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..length]).to_ascii_lowercase();
            if name == "eurotrucks2.exe" || name == "amtrucks.exe" {
                found.push(entry.th32ProcessID);
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    found
}

/// The graphics card's 3D engines' use per process, as Windows counts it (`\GPU Engine(*)\Utilization Percentage`,
/// Task Manager's): over the time between two collections.
struct GpuUse {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

impl GpuUse {
    fn open() -> Option<Self> {
        let mut query = PDH_HQUERY::default();
        let mut counter = PDH_HCOUNTER::default();
        // SAFETY: a query of our own, its handles written into locals; closed as it's dropped.
        unsafe {
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
                return None;
            }
            if PdhAddEnglishCounterW(query, w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage"), 0, &mut counter)
                != 0
            {
                let _ = PdhCloseQuery(query);
                return None;
            }
        }
        let gpu = Self { query, counter };
        gpu.collect();
        Some(gpu)
    }

    /// Counts from now.
    fn collect(&self) {
        // SAFETY: the query is ours and open.
        unsafe {
            let _ = PdhCollectQueryData(self.query);
        }
    }

    /// The use since the last [`GpuUse::collect`], per process (percent of the time), and counting from now.
    fn read(&self) -> Option<HashMap<u32, f64>> {
        // SAFETY: the query and counter are ours and open; the items are read within the buffer PDH filled, sized as it
        // asked, their names its strings within it.
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            let (mut size, mut count) = (0u32, 0u32);
            let status = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
            if status != PDH_MORE_DATA || size == 0 {
                return None;
            }
            let item = size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            let mut buffer = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); (size as usize).div_ceil(item)];
            let status = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut size,
                &mut count,
                Some(buffer.as_mut_ptr()),
            );
            if status != 0 {
                return None;
            }
            let mut used: HashMap<u32, f64> = HashMap::new();
            for item in buffer.iter().take(count as usize) {
                let Ok(name) = item.szName.to_string() else { continue };
                let Some(pid) = pid_of_engine(&name) else { continue };
                let value = item.FmtValue.Anonymous.doubleValue;
                if item.FmtValue.CStatus == 0 && value.is_finite() && value > 0.0 {
                    *used.entry(pid).or_default() += value;
                }
            }
            Some(used)
        }
    }
}

impl Drop for GpuUse {
    fn drop(&mut self) {
        // SAFETY: the query is ours; closed once.
        unsafe {
            let _ = PdhCloseQuery(self.query);
        }
    }
}

/// The process an engine counter's instance is of (`pid_1234_luid_0x0_0xA0AAED6_phys_0_eng_0_engtype_3D`).
fn pid_of_engine(instance: &str) -> Option<u32> {
    instance.strip_prefix("pid_")?.split('_').next()?.parse().ok()
}

/// The part of WestPlay a process is: the app itself, the part WebView2 named it, else by the browser it's
/// under. A WebView2 browser WestPlay started that isn't one of the host's (`browsers`) is the companion
/// window's own (Tauri's), and so is everything under it; the rest is "other" (a process WebView2 hasn't
/// named yet, or can't: older runtimes).
fn part_of(
    pid: u32,
    ours: u32,
    parents: &[(u32, u32)],
    named: &HashMap<u32, &'static str>,
    browsers: &[u32],
) -> &'static str {
    if pid == ours {
        return "WestPlay app";
    }
    if let Some(part) = named.get(&pid) {
        return part;
    }
    // Up to the process WestPlay started itself: a browser (the tree is shallow; a loop isn't followed).
    let mut at = pid;
    for _ in 0..16 {
        let Some(&(_, parent)) = parents.iter().find(|(each, _)| *each == at) else { break };
        if parent == ours {
            return if browsers.contains(&at) { "other" } else { "companion" };
        }
        if parent == at {
            break;
        }
        at = parent;
    }
    "other"
}

/// Every process's id and its parent's, as Windows lists them now.
fn process_tree() -> Vec<(u32, u32)> {
    let mut parents: Vec<(u32, u32)> = Vec::new();
    // SAFETY: a snapshot of the process list, walked with an entry sized as the calls require, then closed.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return parents };
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            parents.push((entry.th32ProcessID, entry.th32ParentProcessID));
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    parents
}

/// WestPlay's processes: its own and every one started from it (WebView2's), at any depth.
fn processes() -> Vec<u32> {
    descendants(std::process::id(), &process_tree())
}

/// `root` and every process below it in `parents` (each process's id and its parent's).
fn descendants(root: u32, parents: &[(u32, u32)]) -> Vec<u32> {
    let mut found = vec![root];
    let mut at = 0;
    while at < found.len() {
        let parent = found[at];
        for &(pid, of) in parents {
            if of == parent && pid != parent && !found.contains(&pid) {
                found.push(pid);
            }
        }
        at += 1;
    }
    found
}

/// Each process's processor time so far (100 ns units); one that can't be opened is left out.
fn processor_times(pids: &[u32]) -> HashMap<u32, u64> {
    let ticks = |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    pids.iter()
        .filter_map(|&pid| {
            // SAFETY: a handle opened for a plain query and closed after it; the times are locals it writes.
            unsafe {
                let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
                let (mut created, mut exited, mut kernel, mut user) = Default::default();
                let read = GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user);
                let _ = CloseHandle(process);
                read.ok()?;
                Some((pid, ticks(kernel) + ticks(user)))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_pc_says_its_windows_build() {
        let build = windows_build();
        println!("this PC: Windows {build}; displays {}", displays());
        assert!(build.starts_with("build "), "{build}");
    }

    #[test]
    fn a_process_tree_is_followed_to_any_depth() {
        // 1 starts 2 (the browser), which starts 3 and 4; 5 is someone else's; 6 claims to be its own parent.
        let parents = [(2, 1), (3, 2), (4, 2), (5, 9), (6, 6), (1, 0)];
        let mut found = descendants(1, &parents);
        found.sort();
        assert_eq!(found, [1, 2, 3, 4]);
    }

    #[test]
    fn processor_time_is_summed_by_part_in_cores() {
        let before = HashMap::from([(1, 0), (2, 10_000_000), (3, 0)]);
        // Over 10 s: 1 took 6 s, 2 took 2 s, 3 took 1 s, 4 is new and took 2 s.
        let after = HashMap::from([(1, 60_000_000), (2, 30_000_000), (3, 10_000_000), (4, 20_000_000)]);
        let parts = cores_by_part(&before, &after, 10.0, |pid| if pid == 1 { "cab page" } else { "GPU" });
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].0, "cab page");
        assert!((parts[0].1 - 0.6).abs() < 1e-9);
        assert!((parts[1].1 - 0.5).abs() < 1e-9);
    }

    #[test]
    fn pages_are_named_by_what_they_are() {
        assert_eq!(host_of("https://westplay.cabin/index.html").as_deref(), Some("westplay.cabin"));
        assert_eq!(host_of("http://localhost:5173/?game=off").as_deref(), Some("localhost"));
        assert_eq!(host_of("about:blank"), None);
        let hosts = |list: &[&str]| list.iter().map(|host| host.to_string()).collect::<Vec<_>>();
        assert_eq!(page_part(&hosts(&["westplay.cabin"]), "westplay.cabin"), "cab page");
        assert_eq!(page_part(&hosts(&["www.youtube.com"]), "westplay.cabin"), "YouTube");
        assert_eq!(page_part(&hosts(&["music.youtube.com"]), "westplay.cabin"), "YouTube");
        assert_eq!(page_part(&hosts(&["accounts.google.com"]), "westplay.cabin"), "other pages");
    }

    #[test]
    fn names_the_companion_windows_processes_by_its_browser() {
        // WestPlay (1) started the cab's browser (2, the host's) and the companion's (5); each started its
        // renderers. WebView2 named the cab's browser and its renderer 3; 4 it hasn't named yet.
        let parents = [(1, 0), (2, 1), (3, 2), (4, 2), (5, 1), (6, 5), (7, 6), (9, 8)];
        let named = HashMap::from([(2, "browser"), (3, "cab page")]);
        let part = |pid| part_of(pid, 1, &parents, &named, &[2]);
        assert_eq!(part(1), "WestPlay app");
        assert_eq!(part(3), "cab page");
        assert_eq!(part(4), "other");
        assert_eq!([part(5), part(6), part(7)], ["companion"; 3]);
        // Not WestPlay's at all, or a loop in the list: other.
        assert_eq!(part(9), "other");
        assert_eq!(part_of(7, 1, &[(7, 7)], &named, &[2]), "other");
    }

    #[test]
    fn the_pages_frames_go_in_the_line_once_it_says_them() {
        let line = format!("the time, {PAGE}30 frames a second sent");
        let report: PageReport =
            serde_json::from_str(r#"{"frames":580,"slow":12,"screens":{"home":3,"maps":7}}"#).unwrap();
        assert_eq!(
            with_page(&line, Some(&report), 10.0),
            "the time, the page drew 58 frames a second (12 of them late, 50 ms or more), showing maps 7 s, home 3 s, \
             30 frames a second sent"
        );
        assert_eq!(with_page(&line, None, 10.0), "the time, 30 frames a second sent");
    }

    #[test]
    fn says_how_busy_the_graphics_card_was_and_whose_it_was() {
        assert_eq!(pid_of_engine("pid_1234_luid_0x00000000_0x0A0AAED6_phys_0_eng_0_engtype_3D"), Some(1234));
        assert_eq!(pid_of_engine("luid_0x0_phys_0"), None);
        let used = HashMap::from([(10, 80.0), (20, 6.5), (21, 2.0), (30, 4.0)]);
        assert_eq!(
            gpu_said(&used, &[10], &[20, 21]),
            "; the graphics card's 3D engine busy 92% of the time (the game 80%, WestPlay 8%)"
        );
    }

    /// The whole processor's use, read on this PC.
    #[test]
    fn reads_the_whole_processors_use() {
        let (idle0, total0) = system_times().unwrap();
        // Something to count.
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            std::hint::black_box(0u64.wrapping_add(1));
        }
        let (idle1, total1) = system_times().unwrap();
        assert!(total1 > total0 && idle1 >= idle0 && total1 - total0 >= idle1 - idle0);
    }

    /// Windows' GPU counters, read on this PC.
    #[test]
    #[ignore = "reads this PC's counters"]
    fn reads_the_graphics_cards_use() {
        let gpu = GpuUse::open().expect("counters");
        std::thread::sleep(Duration::from_secs(2));
        let used = gpu.read().expect("read");
        println!("3D engine use by process: {used:?}; the game's: {:?}", game_processes());
    }

    #[test]
    fn a_software_renderer_says_so() {
        assert!(described("ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device))".into()).contains("no GPU"));
        assert!(!described("ANGLE (NVIDIA, NVIDIA GeForce RTX 4080 SUPER)".into()).contains("no GPU"));
    }
}
