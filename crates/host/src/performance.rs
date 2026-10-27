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
use windows::Win32::Foundation::{CloseHandle, FILETIME};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::core::{BOOL, Interface, PWSTR};

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
}

impl Meter {
    pub fn new(cab_host: String) -> Self {
        Self { cab_host, window: None, parts: Rc::default(), renderer: Rc::default(), renderer_asked: false }
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
            // The page's frames since the last line (its count starts again as it's taken: state/frameMeter.ts).
            let asked = cab.run_script("window.westplayFrames?.() ?? -1", {
                let line = line.clone();
                move |json| {
                    let frames = serde_json::from_str::<f64>(&json).ok().filter(|frames| *frames >= 0.0);
                    crate::log!("{}", with_page(&line, frames, seconds));
                }
            });
            if asked.is_err() {
                crate::log!("{}", with_page(&line, None, seconds));
            }
            self.begin(now, &sample, profiles);
        }
    }

    fn begin(&mut self, now: Instant, sample: &Sample, profiles: &[(Profile, &ICoreWebView2Environment)]) {
        self.window = Some(Window {
            started: now,
            game_frames: sample.game_frames,
            sent: sample.sent,
            captured: sample.captured,
            waited: sample.waited,
            seconds: 0,
            in_view: 0,
            cpu: processor_times(&processes()),
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
        format!(
            "performance, the last {seconds:.0} s: the game {game:.0} fps; WestPlay's screen in view {in_view}% of \
             the time, {PAGE}{sent:.0} frames a second sent ({captured:.0} captured, {waited:.0} waited for the game); \
             WestPlay used {total:.2} of {threads} processor threads ({each}); the cab is drawn with {renderer}"
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

/// The line with the page's frames in it (`frames` over `seconds`; `None`: the page didn't say).
fn with_page(line: &str, frames: Option<f64>, seconds: f64) -> String {
    let page =
        frames.map_or(String::new(), |frames| format!("the page drew {:.0} frames a second, ", frames / seconds));
    line.replacen(PAGE, &page, 1)
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
        assert_eq!(
            with_page(&line, Some(580.0), 10.0),
            "the time, the page drew 58 frames a second, 30 frames a second sent"
        );
        assert_eq!(with_page(&line, None, 10.0), "the time, 30 frames a second sent");
    }

    #[test]
    fn a_software_renderer_says_so() {
        assert!(described("ANGLE (Google, Vulkan 1.3.0 (SwiftShader Device))".into()).contains("no GPU"));
        assert!(!described("ANGLE (NVIDIA, NVIDIA GeForce RTX 4080 SUPER)".into()).contains("no GPU"));
    }
}
