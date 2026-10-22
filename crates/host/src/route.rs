//! The game's GPS route, whatever set it (a job, a point the player put on the map, any routing mode),
//! read from the game's memory: read-only, from outside the game (as a debugger reads), on a thread of its
//! own. The SDK gives only the distance and time left; the route itself is the game's own:
//!
//! - its telemetry getter for `truck.navigation.distance` reads a global pointer to an object whose member
//!   at an offset is the navigator. Both are found in the game's executable at startup, not hard-coded:
//!   the getter is registered right next to the channel's name ([`locate`]);
//! - the navigator's route id is at `+0x10`; its pointer at `+0x28` leads to the route's holder, and the
//!   route is at `holder + 0x78`: an array (`+0x08`) of nodes (`+0x10` of them), 0x30 bytes each, and the
//!   next node ahead of the truck at navigator `+0x198`, while its route id at `+0x190` is the route's;
//! - a node is a map node (type 0, `+0x08` points to it) or a point inside a junction (type 1); a map node
//!   is laid out as the map's own: its position (three integers of 1/256 m), its rotation (w, x, y, z
//!   floats at `+0x10`), its id at `+0x30`.
//!
//! Between two map nodes that are ends of one junction, the way is the junction's own lanes: the game's
//! files say which (westplay_mapdata::junctions), read from the install the running game is from with the
//! played profile's map mods over it (as its map was built: a map mod's junctions are its own), on a
//! thread of their own, and sent once found ([`RouteUpdate::Paths`]).
//!
//! Worked out on version 1.61.1.1; what's read is checked (a sane count, nodes where they should be), and
//! a layout that doesn't add up after a game update reads as no route.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use crate::maps::PlayedMap;

use westplay_mapdata::junctions::{self, Junctions, Maneuver};
use westplay_setup::map;
use westplay_shared::BridgeMapping;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, TH32CS_SNAPMODULE,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

/// How often the route is read.
const READ_EVERY: Duration = Duration::from_millis(500);

const NAVIGATOR_ROUTE_ID: usize = 0x10;
const NAVIGATOR_HOLDER: usize = 0x28;
const NAVIGATOR_NEXT_ID: usize = 0x190;
const NAVIGATOR_NEXT: usize = 0x198;
const HOLDER_ROUTE: usize = 0x78;
/// The navigator's waypoints, in the order they were put in (an array: its entries at `+0x08`, their count at
/// `+0x10`); each entry 0x18 bytes, its map node's pointer at `+0x08` (the destination isn't among them).
const NAVIGATOR_WAYPOINTS: usize = 0x248;
const WAYPOINT_SIZE: usize = 0x18;
const WAYPOINT_NODE: usize = 0x08;
/// More waypoints than the game takes: a count past this is a layout misread.
const MAX_WAYPOINTS: usize = 64;
const NODE_SIZE: usize = 0x30;
/// A route node's distance left to the destination (a float).
const ROUTE_NODE_DISTANCE: usize = 0x24;
const MAP_NODE_SIZE: usize = 0x38;
/// More nodes than any route across the map has: a count past this is a layout misread.
const MAX_NODES: usize = 200_000;

/// Where the game keeps its navigator: a global (its offset in the executable) pointing to an object, and
/// the navigator's offset in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub global_rva: usize,
    pub navigator: usize,
}

/// One of the route's map nodes: where it is, which way it points (the way of the roads through it,
/// x and z), and its id (the map's own).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteNode {
    pub position: [f32; 2],
    pub forward: [f32; 2],
    pub uid: u64,
    /// The distance left from the node to the destination, as the game counts distances (its map's
    /// scale, as the SDK's `navigation.distance`), metres.
    pub distance_left: f32,
}

/// What's sent as the route changes: a new route (its map nodes in order, from the start), the ways
/// through its junctions (for each, the index of the node it starts at and its points, x z) and the turns
/// there (by the same index), the truck's progress along it (the index of the next map node ahead), or no
/// route. A route's `id` is the host's number for it (counting up, never the same for two routes), which its
/// junctions' ways carry too.
#[derive(Clone, Debug, PartialEq)]
pub enum RouteUpdate {
    Route {
        id: u32,
        nodes: Vec<RouteNode>,
        /// The game's waypoints (their map nodes' uids, in the order they were put in), the player's stops
        /// and any WestPlay put in to hold a route it handed over; none if they couldn't be read.
        waypoints: Vec<u64>,
    },
    /// The ways through the route's junctions and its turns there (by the node index each starts at), and
    /// how far to the right of its way the route runs at its nodes on roads (on the carriageway it drives
    /// on: westplay_mapdata::junctions::Ways).
    Paths {
        id: u32,
        paths: Vec<(usize, Vec<[f32; 2]>)>,
        maneuvers: Vec<(usize, Maneuver)>,
        shifts: Vec<(usize, f32)>,
    },
    Next(usize),
    None,
}

/// A route whose junctions are to be resolved, from the game installed in the folder.
type JunctionJob = (u32, Vec<RouteNode>, PathBuf);

pub struct RouteWatch {
    pub updates: Receiver<RouteUpdate>,
}

impl RouteWatch {
    /// Follows the route of the game the plugin is in (`bridge`), its junctions read with the map mods of the
    /// profile `played`.
    pub fn start(bridge: Arc<BridgeMapping>, played: PlayedMap) -> Self {
        let (sender, updates) = mpsc::channel();
        let (jobs, pending) = mpsc::channel();
        let paths_sender = sender.clone();
        let _ = std::thread::Builder::new()
            .name("westplay-junctions".into())
            .spawn(move || resolve_junctions(pending, paths_sender, played));
        let _ = std::thread::Builder::new().name("westplay-route".into()).spawn(move || {
            let mut game: Option<Game> = None;
            // A game process that couldn't be read (its navigator not found after an update, or no access):
            // tried again only when it's another process, or after a while.
            let mut failed: Option<(u32, Instant)> = None;
            let mut reader = RouteReader::default();
            loop {
                let pid = bridge.bridge().plugin.pid.load(Ordering::Acquire);
                if game.as_ref().map(|game| game.process.pid) != Some(pid) {
                    game = None;
                    let retry = failed.is_none_or(|(failed_pid, at)| failed_pid != pid || at.elapsed() >= RETRY_AFTER);
                    if pid != 0 && retry {
                        game = Game::open(pid);
                        failed = game.is_none().then(|| (pid, Instant::now()));
                    }
                }
                let updates = match &game {
                    Some(game) => {
                        reader.poll(&game.process, game.process.base + game.layout.global_rva, game.layout.navigator)
                    }
                    None => reader.lost(),
                };
                for update in updates {
                    // The route goes out before its junctions are looked for, so they can't arrive first.
                    if let RouteUpdate::Route { id, nodes, .. } = &update
                        && let Some(install) = game.as_ref().and_then(|game| game.install.clone())
                    {
                        let route = (*id, nodes.clone(), install);
                        if sender.send(update).is_err() {
                            return;
                        }
                        let _ = jobs.send(route);
                        continue;
                    }
                    if sender.send(update).is_err() {
                        return;
                    }
                }
                std::thread::sleep(READ_EVERY);
            }
        });
        Self { updates }
    }
}

/// How long before a game process that couldn't be read is tried again.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// Follows the game's route from poll to poll: reads it whole only when it's a new one (and twice, the same
/// both times, so a route the game is still writing isn't taken), then only the navigator's progress.
/// Routes are numbered by the host, so the ways through a route's junctions, found later, can't be taken
/// for another's (the game's own route ids may come again).
#[derive(Default)]
struct RouteReader {
    /// The route followed: which it is in the game's memory, its map nodes, and for each of its nodes in
    /// memory, the map node at or after it.
    route: Option<(RouteKey, Vec<RouteNode>, Vec<usize>)>,
    /// The host's number for the route last sent.
    number: u32,
    sent_next: Option<usize>,
}

/// Which route the game's memory holds: its id, where its nodes are and how many.
type RouteKey = (u32, usize, usize);

impl RouteReader {
    /// What changed since the last poll, read from the navigator (`navigator` bytes into the object the
    /// global at `global` points to).
    fn poll(&mut self, memory: &impl Memory, global: usize, navigator: usize) -> Vec<RouteUpdate> {
        let Some(header) = read_header(memory, global, navigator) else { return self.lost() };
        let key = (header.id, header.array, header.count);
        let mut updates = Vec::new();
        if self.route.as_ref().map(|(seen, _, _)| *seen) != Some(key) {
            // A new route that doesn't read (it doesn't add up): no route. One that reads differently twice
            // (still being written): looked at again next time.
            let (Some(first), Some(again)) = (read_nodes(memory, &header), read_nodes(memory, &header)) else {
                return self.lost();
            };
            if first != again {
                return updates;
            }
            let (nodes, after) = first;
            self.number = self.number.wrapping_add(1);
            self.sent_next = None;
            // Its waypoints with it: the game plans anew as they change.
            let waypoints = read_waypoints(memory, global, navigator).unwrap_or_default();
            updates.push(RouteUpdate::Route { id: self.number, nodes: nodes.clone(), waypoints });
            self.route = Some((key, nodes, after));
        }
        let Some((_, nodes, after)) = &self.route else { return updates };
        let next = after.get(header.next_index).copied().unwrap_or(nodes.len() - 1);
        if self.sent_next != Some(next) {
            self.sent_next = Some(next);
            updates.push(RouteUpdate::Next(next));
        }
        updates
    }

    /// No route (or no game): says so once.
    fn lost(&mut self) -> Vec<RouteUpdate> {
        self.sent_next = None;
        match self.route.take() {
            Some(_) => vec![RouteUpdate::None],
            None => Vec::new(),
        }
    }
}

/// Reading another process's memory (the game's), or a stand-in for tests.
pub(crate) trait Memory {
    fn read(&self, address: usize, buffer: &mut [u8]) -> bool;

    fn u64_at(&self, address: usize) -> Option<u64> {
        let mut word = [0u8; 8];
        self.read(address, &mut word).then(|| u64::from_le_bytes(word))
    }
}

/// The navigator's route as it stands: its id, where its nodes are and how many, and the index of the
/// node ahead of the truck (0 if the navigator's pointer isn't into this route).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Header {
    id: u32,
    array: usize,
    count: usize,
    next_index: usize,
}

fn read_header(memory: &impl Memory, global: usize, navigator: usize) -> Option<Header> {
    let owner = memory.u64_at(global)? as usize;
    if owner == 0 {
        return None;
    }
    let navigator = owner.checked_add(navigator)?;
    let id = memory.u64_at(navigator.checked_add(NAVIGATOR_ROUTE_ID)?)? as u32;
    let holder = memory.u64_at(navigator.checked_add(NAVIGATOR_HOLDER)?)? as usize;
    if holder == 0 {
        return None;
    }
    let route = holder.checked_add(HOLDER_ROUTE)?;
    let array = memory.u64_at(route.checked_add(8)?)? as usize;
    let count = memory.u64_at(route.checked_add(0x10)?)? as usize;
    if array == 0 || count == 0 || count > MAX_NODES {
        return None;
    }
    let end = array.checked_add(count * NODE_SIZE)?;
    let next_id = memory.u64_at(navigator.checked_add(NAVIGATOR_NEXT_ID)?)? as u32;
    let next = memory.u64_at(navigator.checked_add(NAVIGATOR_NEXT)?)? as usize;
    // The next node ahead: a node of this route (the pointer may still be an old route's).
    let in_route = next >= array && next < end && (next - array).is_multiple_of(NODE_SIZE);
    let next_index = if next_id == id && in_route { (next - array) / NODE_SIZE } else { 0 };
    Some(Header { id, array, count, next_index })
}

/// The navigator's waypoints: their map nodes' uids, in the order they were put in; `None` if what's read
/// doesn't add up.
fn read_waypoints(memory: &impl Memory, global: usize, navigator: usize) -> Option<Vec<u64>> {
    let owner = memory.u64_at(global)? as usize;
    let list = owner.checked_add(navigator)?.checked_add(NAVIGATOR_WAYPOINTS)?;
    let count = memory.u64_at(list.checked_add(0x10)?)? as usize;
    if count == 0 {
        return Some(Vec::new());
    }
    let array = memory.u64_at(list.checked_add(8)?)? as usize;
    if array == 0 || count > MAX_WAYPOINTS {
        return None;
    }
    let mut raw = vec![0u8; count * WAYPOINT_SIZE];
    if !memory.read(array, &mut raw) {
        return None;
    }
    raw.chunks_exact(WAYPOINT_SIZE)
        .map(|entry| {
            let pointer = u64::from_le_bytes(entry[WAYPOINT_NODE..WAYPOINT_NODE + 8].try_into().unwrap()) as usize;
            let mut map_node = [0u8; MAP_NODE_SIZE];
            memory.read(pointer, &mut map_node).then(|| map_node_of(&map_node).uid)
        })
        .collect()
}

/// The route's map nodes, and for each of its nodes in memory, the map node at or after it (where the truck
/// is along the map nodes when it's at that node); `None` without any, or if what's read doesn't add up.
fn read_nodes(memory: &impl Memory, header: &Header) -> Option<(Vec<RouteNode>, Vec<usize>)> {
    let mut raw = vec![0u8; header.count * NODE_SIZE];
    if !memory.read(header.array, &mut raw) {
        return None;
    }
    let mut nodes = Vec::new();
    // For each node in memory, the map node before which it lies.
    let mut before = Vec::with_capacity(header.count);
    for node in raw.chunks_exact(NODE_SIZE) {
        before.push(nodes.len());
        match u32::from_le_bytes(node[0..4].try_into().unwrap()) {
            0 => {
                let pointer = u64::from_le_bytes(node[8..16].try_into().unwrap()) as usize;
                let mut map_node = [0u8; MAP_NODE_SIZE];
                if !memory.read(pointer, &mut map_node) {
                    return None;
                }
                let distance_left =
                    f32::from_le_bytes(node[ROUTE_NODE_DISTANCE..ROUTE_NODE_DISTANCE + 4].try_into().unwrap());
                nodes.push(RouteNode { distance_left, ..map_node_of(&map_node) });
            }
            1 => {}
            // Neither kind of node: the layout isn't what it was.
            _ => return None,
        }
    }
    if nodes.is_empty() {
        return None;
    }
    let last = nodes.len() - 1;
    let after = before.into_iter().map(|index| index.min(last)).collect();
    Some((nodes, after))
}

/// Resolves the ways through each route's junctions as routes come (only the latest, when several wait),
/// opening the game's files once (again when the profile played has other map mods).
fn resolve_junctions(pending: Receiver<JunctionJob>, sender: Sender<RouteUpdate>, played: PlayedMap) {
    let mut opened: Option<(JunctionFiles, Junctions)> = None;
    while let Ok(mut job) = pending.recv() {
        while let Ok(newer) = pending.try_recv() {
            job = newer;
        }
        let (id, nodes, install) = job;
        let files = JunctionFiles::of(&install, played.lock().unwrap_or_else(|e| e.into_inner()).as_ref());
        if opened.as_ref().map(|(known, _)| known) != Some(&files) {
            opened = files.open().map(|junctions| (files, junctions));
        }
        let Some((_, junctions)) = &opened else { continue };
        let route: Vec<junctions::RouteNode> =
            nodes.iter().map(|node| junctions::RouteNode { uid: node.uid, position: node.position }).collect();
        match junctions.junctions(&route) {
            Ok(found) => {
                let maneuvers =
                    found.junctions.iter().filter_map(|junction| Some((junction.span, junction.maneuver?))).collect();
                let paths = found.junctions.into_iter().map(|junction| (junction.span, junction.path)).collect();
                let shifts = found.shifts;
                if sender.send(RouteUpdate::Paths { id, paths, maneuvers, shifts }).is_err() {
                    return;
                }
            }
            Err(error) => log!("route: the route's junctions couldn't be read: {error}"),
        }
    }
}

/// What a route's junctions are read from: the running game's install, the map its profile plays and the
/// map mods over it, in their load order (only these tell two apart: the profile's saves don't count).
#[derive(Clone, Debug)]
struct JunctionFiles {
    install: PathBuf,
    map: String,
    mods: Vec<PathBuf>,
    /// The profile played, to mount its map mods (`None`: not worked out yet, the base game's map).
    played: Option<(westplay_setup::game::Game, westplay_setup::profiles::Profile)>,
}

impl PartialEq for JunctionFiles {
    fn eq(&self, other: &Self) -> bool {
        (&self.install, &self.map, &self.mods) == (&other.install, &other.map, &other.mods)
    }
}

impl JunctionFiles {
    fn of(install: &Path, played: Option<&(westplay_setup::game::Game, westplay_setup::profiles::Profile)>) -> Self {
        // The running game's own install, whichever WestPlay is set up in.
        let played = played.map(|(game, profile)| {
            (westplay_setup::game::Game { dir: install.to_owned(), ..game.clone() }, profile.clone())
        });
        let (map, mods) = match &played {
            Some((_, profile)) => (profile.map.clone(), profile.map_mods().filter_map(|m| m.files.clone()).collect()),
            None => ("europe".to_owned(), Vec::new()),
        };
        Self { install: install.to_owned(), map, mods, played }
    }

    /// The junctions as these files have them. A map mod that can't be read (HVT's, say) leaves the base
    /// game's, so its roads still get their ways.
    fn open(&self) -> Option<Junctions> {
        if let Some((game, profile)) = &self.played
            && !self.mods.is_empty()
        {
            match map::mount(game, profile)
                .map_err(|error| error.to_string())
                .and_then(|files| Junctions::new(files, &self.map).map_err(|error| error.to_string()))
            {
                Ok(junctions) => return Some(junctions),
                Err(error) => log!(
                    "route: the map mods of {:?} couldn't be read, so only the game's own junctions are: {error}",
                    profile.name
                ),
            }
        }
        match Junctions::open(&self.install) {
            Ok(junctions) => Some(junctions),
            Err(error) => {
                log!("route: the game's files in {} couldn't be read: {error}", self.install.display());
                None
            }
        }
    }
}

/// The running game's process, opened to read its memory: where its executable is loaded, and its file.
pub(crate) struct Process {
    pub pid: u32,
    handle: HANDLE,
    pub base: usize,
    pub exe: PathBuf,
}

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY: the process handle is this reader's own, closed once, as it goes.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

impl Process {
    /// `who` names the reader in what's logged.
    pub fn open(pid: u32, who: &str) -> Option<Self> {
        // Windows refuses this for a game running as administrator when WestPlay isn't.
        let (base, exe) = match main_module(pid) {
            Ok(module) => module,
            Err(error) => {
                log!("{who}: the game's process can't be looked into: {error}");
                return None;
            }
        };
        // SAFETY: a plain call: read access to the game's process, by its id.
        match unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) } {
            Ok(handle) => Some(Self { pid, handle, base, exe }),
            Err(error) => {
                log!("{who}: the game's memory can't be read: {error}");
                None
            }
        }
    }
}

impl Memory for Process {
    fn read(&self, address: usize, buffer: &mut [u8]) -> bool {
        let mut read = 0;
        // SAFETY: the handle is open with read access; the buffer is ours and the call writes at most its length.
        let ok = unsafe {
            ReadProcessMemory(
                self.handle,
                address as *const _,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                Some(&mut read),
            )
        };
        ok.is_ok() && read == buffer.len()
    }
}

/// The running game: its process, its navigator's layout, and the folder it's installed in (with
/// `base.scs`).
struct Game {
    process: Process,
    layout: Layout,
    install: Option<PathBuf>,
}

impl Game {
    fn open(pid: u32) -> Option<Self> {
        let process = Process::open(pid, "route")?;
        let Some(layout) = std::fs::read(&process.exe).ok().and_then(|image| locate_in_image(&image)) else {
            log!("route: the game's navigator wasn't found in {} (a new game version?)", process.exe.display());
            return None;
        };
        // The executable is in `bin/win_x64` of the install.
        let install =
            process.exe.ancestors().nth(3).filter(|dir| dir.join("base.scs").is_file()).map(|dir| dir.to_owned());
        Some(Self { process, layout, install })
    }
}

/// A map node as the game keeps it: position (integers of 1/256 m: x, height, z), rotation (w, x, y, z), id.
fn map_node_of(bytes: &[u8; MAP_NODE_SIZE]) -> RouteNode {
    let int = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as f32 / 256.0;
    let float = |at: usize| f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    // The game's forward (0, 0, -1) turned by the rotation's yaw (as the map's nodes: geometry::node_forward).
    let (w, y) = (float(0x10), float(0x18));
    let yaw = 2.0 * y.atan2(w);
    RouteNode {
        position: [int(0), int(8)],
        forward: [-yaw.sin(), -yaw.cos()],
        uid: u64::from_le_bytes(bytes[0x30..0x38].try_into().unwrap()),
        distance_left: 0.0,
    }
}

/// The game's executable: where it's loaded and its file.
fn main_module(pid: u32) -> windows::core::Result<(usize, PathBuf)> {
    // SAFETY: the snapshot is closed once after use; `entry` is a local with its size set, as the call requires.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid)?;
        let mut entry = MODULEENTRY32W { dwSize: size_of::<MODULEENTRY32W>() as u32, ..Default::default() };
        let found = Module32FirstW(snapshot, &mut entry);
        let _ = CloseHandle(snapshot);
        found?;
        let length = entry.szExePath.iter().position(|&c| c == 0).unwrap_or(entry.szExePath.len());
        Ok((entry.modBaseAddr as usize, PathBuf::from(String::from_utf16_lossy(&entry.szExePath[..length]))))
    }
}

/// A section of an executable: where it's mapped (rva) and its bytes in the file.
pub(crate) struct Section<'a> {
    pub rva: usize,
    pub bytes: &'a [u8],
}

/// The navigator's layout, from the game's executable file: its code (`.text`) and constant data (`.rdata`).
fn locate_in_image(image: &[u8]) -> Option<Layout> {
    locate(&section(image, b".text\0")?, &section(image, b".rdata\0")?)
}

/// A section of an executable file (`image`) by its name (NUL-terminated, eight bytes at most).
pub(crate) fn section<'a>(image: &'a [u8], name: &[u8]) -> Option<Section<'a>> {
    let u16_at = |at: usize| image.get(at..at + 2).map(|b| u16::from_le_bytes(b.try_into().unwrap()));
    let u32_at = |at: usize| image.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let header = u32_at(0x3c)?;
    let sections = u16_at(header + 6)? as usize;
    let optional = u16_at(header + 20)? as usize;
    (0..sections).find_map(|i| {
        let at = header + 24 + optional + i * 40;
        let named = image.get(at..at + 8)?.starts_with(name);
        let (rva, size, raw) = (u32_at(at + 12)?, u32_at(at + 16)?, u32_at(at + 20)?);
        named.then(|| Some(Section { rva, bytes: image.get(raw..raw + size)? })).flatten()
    })
}

/// The navigator's layout, from the code: the getter registered beside the name `truck.navigation.distance`
/// (`lea rax, [getter]` a little before `lea …, [name]`) starts `sub rsp, 28h; mov rax, [global]`, and
/// soon `lea rcx, [rax + navigator]`.
fn locate(text: &Section, rdata: &Section) -> Option<Layout> {
    let name = b"truck.navigation.distance\0";
    let name_rva = rdata.rva + rdata.bytes.windows(name.len()).position(|w| w == name)?;
    let code = text.bytes;
    // A RIP-relative lea (48 or 4C, 8D, a register with [rip + disp32]) at `at`, and where it points.
    let lea_target = |at: usize| -> Option<usize> {
        let b = code.get(at..at + 7)?;
        let rip_relative = (b[0] == 0x48 || b[0] == 0x4c) && b[1] == 0x8d && b[2] & 0xc7 == 0x05;
        rip_relative
            .then(|| (text.rva + at + 7).wrapping_add_signed(i32::from_le_bytes(b[3..7].try_into().unwrap()) as isize))
    };
    for site in (0..code.len().saturating_sub(7)).filter(|&at| lea_target(at) == Some(name_rva)) {
        for back in 1..=64 {
            let Some(getter) = site.checked_sub(back).and_then(lea_target) else { continue };
            let Some(start) = getter.checked_sub(text.rva) else { continue };
            if let Some(layout) = read_getter(code, start, getter) {
                return Some(layout);
            }
        }
    }
    None
}

/// The layout a distance getter at `start` in the code (rva `rva`) reads: `48 83 EC 28` (sub rsp, 28h),
/// `48 8B 05 disp32` (mov rax, [rip + disp32]: the global), then within a few instructions `48 8D 88 imm32`
/// (lea rcx, [rax + imm32]: the navigator).
fn read_getter(code: &[u8], start: usize, rva: usize) -> Option<Layout> {
    let b = code.get(start..start + 48)?;
    if b[..4] != [0x48, 0x83, 0xec, 0x28] || b[4..7] != [0x48, 0x8b, 0x05] {
        return None;
    }
    let global_rva = (rva + 11).wrapping_add_signed(i32::from_le_bytes(b[7..11].try_into().unwrap()) as isize);
    let at = (11..b.len() - 7).find(|&i| b[i..i + 3] == [0x48, 0x8d, 0x88])?;
    let navigator = u32::from_le_bytes(b[at + 3..at + 7].try_into().unwrap()) as usize;
    Some(Layout { global_rva, navigator })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::BTreeMap;

    /// A stand-in for the game's memory: blocks of bytes at addresses. `shaky` changes one map node's
    /// position on every read of it (a route the game is still writing).
    #[derive(Default)]
    struct FakeMemory {
        blocks: BTreeMap<usize, Vec<u8>>,
        shaky: Option<usize>,
        reads: Cell<u32>,
    }

    impl Memory for FakeMemory {
        fn read(&self, address: usize, buffer: &mut [u8]) -> bool {
            let Some((&start, block)) = self.blocks.range(..=address).next_back() else { return false };
            let at = address - start;
            let Some(bytes) = block.get(at..at + buffer.len()) else { return false };
            buffer.copy_from_slice(bytes);
            if self.shaky == Some(address) {
                let read = self.reads.get() + 1;
                self.reads.set(read);
                buffer[0..4].copy_from_slice(&(read as i32 * 256).to_le_bytes());
            }
            true
        }
    }

    const GLOBAL: usize = 0x1000;
    const OWNER: usize = 0x10_000;
    const NAVIGATOR: usize = 0x40;
    const HOLDER: usize = 0x20_000;
    const ARRAY: usize = 0x30_000;
    const MAP_NODES: usize = 0x40_000;

    /// A navigator with a route of `kinds` (0 a map node, 1 a junction point), id `id`, the next node ahead
    /// at index `next` (of the route `next_id`).
    fn navigator(id: u32, kinds: &[u32], next: usize, next_id: u32) -> FakeMemory {
        let mut memory = FakeMemory::default();
        memory.blocks.insert(GLOBAL, (OWNER as u64).to_le_bytes().to_vec());
        let mut owner = vec![0u8; NAVIGATOR + NAVIGATOR_NEXT + 8];
        let put = |bytes: &mut Vec<u8>, at: usize, value: u64| bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
        put(&mut owner, NAVIGATOR + NAVIGATOR_ROUTE_ID, u64::from(id));
        put(&mut owner, NAVIGATOR + NAVIGATOR_HOLDER, HOLDER as u64);
        put(&mut owner, NAVIGATOR + NAVIGATOR_NEXT_ID, u64::from(next_id));
        put(&mut owner, NAVIGATOR + NAVIGATOR_NEXT, (ARRAY + next * NODE_SIZE) as u64);
        memory.blocks.insert(OWNER, owner);
        let mut holder = vec![0u8; HOLDER_ROUTE + 0x18];
        put(&mut holder, HOLDER_ROUTE + 8, ARRAY as u64);
        put(&mut holder, HOLDER_ROUTE + 0x10, kinds.len() as u64);
        memory.blocks.insert(HOLDER, holder);
        let mut array = vec![0u8; kinds.len() * NODE_SIZE];
        for (index, &kind) in kinds.iter().enumerate() {
            let node = &mut array[index * NODE_SIZE..(index + 1) * NODE_SIZE];
            node[0..4].copy_from_slice(&kind.to_le_bytes());
            node[8..16].copy_from_slice(&((MAP_NODES + index * 0x100) as u64).to_le_bytes());
            let mut map_node = vec![0u8; MAP_NODE_SIZE];
            map_node[0..4].copy_from_slice(&((index as i32) * 256 * 10).to_le_bytes());
            map_node[0x10..0x14].copy_from_slice(&1.0f32.to_le_bytes());
            map_node[0x30..0x38].copy_from_slice(&(index as u64 + 1).to_le_bytes());
            memory.blocks.insert(MAP_NODES + index * 0x100, map_node);
        }
        memory.blocks.insert(ARRAY, array);
        memory
    }

    fn poll(reader: &mut RouteReader, memory: &FakeMemory) -> Vec<RouteUpdate> {
        reader.poll(memory, GLOBAL, NAVIGATOR)
    }

    fn summary(updates: &[RouteUpdate]) -> Vec<String> {
        updates
            .iter()
            .map(|update| match update {
                RouteUpdate::Route { id, nodes, .. } => format!("route {id}: {} nodes", nodes.len()),
                RouteUpdate::Next(next) => format!("next {next}"),
                RouteUpdate::None => "none".into(),
                RouteUpdate::Paths { .. } => "paths".into(),
            })
            .collect()
    }

    #[test]
    fn reads_the_navigators_waypoints_with_a_new_route() {
        // Map nodes 1, 2 and 3; waypoints at the second and the first map node, in that order.
        let mut memory = navigator(77, &[0, 0, 0], 0, 77);
        const WAYPOINTS: usize = 0x50_000;
        let owner = memory.blocks.get_mut(&OWNER).unwrap();
        owner.resize(NAVIGATOR + NAVIGATOR_WAYPOINTS + 0x18, 0);
        let list = NAVIGATOR + NAVIGATOR_WAYPOINTS;
        owner[list + 8..list + 16].copy_from_slice(&(WAYPOINTS as u64).to_le_bytes());
        owner[list + 0x10..list + 0x18].copy_from_slice(&2u64.to_le_bytes());
        let mut entries = vec![0u8; 2 * WAYPOINT_SIZE];
        for (entry, node) in [1usize, 0].into_iter().enumerate() {
            let at = entry * WAYPOINT_SIZE + WAYPOINT_NODE;
            entries[at..at + 8].copy_from_slice(&((MAP_NODES + node * 0x100) as u64).to_le_bytes());
        }
        memory.blocks.insert(WAYPOINTS, entries);
        let mut reader = RouteReader::default();
        let updates = poll(&mut reader, &memory);
        assert!(matches!(&updates[0], RouteUpdate::Route { waypoints, .. } if waypoints == &[2, 1]), "{updates:?}");
        // A list that can't be read: none known (not an empty list).
        assert_eq!(read_waypoints(&navigator(77, &[0], 0, 77), GLOBAL, NAVIGATOR), None);
        // An empty one: none.
        let mut empty = navigator(77, &[0], 0, 77);
        empty.blocks.get_mut(&OWNER).unwrap().resize(NAVIGATOR + NAVIGATOR_WAYPOINTS + 0x18, 0);
        assert_eq!(read_waypoints(&empty, GLOBAL, NAVIGATOR), Some(Vec::new()));
    }

    #[test]
    fn reads_a_route_once_then_only_the_progress_along_it() {
        // Map nodes at 0, 2 and 3; a junction point at 1. The truck heads for the junction point: the map
        // node at or after it is the second one.
        let mut reader = RouteReader::default();
        let memory = navigator(77, &[0, 1, 0, 0], 1, 77);
        assert_eq!(summary(&poll(&mut reader, &memory)), ["route 1: 3 nodes", "next 1"]);
        // Nothing new: nothing sent.
        assert!(poll(&mut reader, &memory).is_empty());
        // On to the last node.
        let memory = navigator(77, &[0, 1, 0, 0], 3, 77);
        assert_eq!(summary(&poll(&mut reader, &memory)), ["next 2"]);
        // A pointer into another route (the navigator's not caught up): the start.
        let memory = navigator(77, &[0, 1, 0, 0], 3, 76);
        assert_eq!(summary(&poll(&mut reader, &memory)), ["next 0"]);
    }

    #[test]
    fn a_new_route_gets_a_new_number_even_under_the_same_game_id() {
        let mut reader = RouteReader::default();
        assert_eq!(summary(&poll(&mut reader, &navigator(5, &[0, 0], 0, 5))), ["route 1: 2 nodes", "next 0"]);
        // The game's id again, but other nodes: another route, numbered anew (late junction ways for the
        // first can't be taken for it).
        assert_eq!(summary(&poll(&mut reader, &navigator(5, &[0, 0, 0], 0, 5))), ["route 2: 3 nodes", "next 0"]);
        assert_eq!(summary(&poll(&mut reader, &navigator(6, &[0, 0, 0], 0, 6))), ["route 3: 3 nodes", "next 0"]);
    }

    #[test]
    fn a_route_still_being_written_is_left_until_it_reads_the_same_twice() {
        let mut reader = RouteReader::default();
        let mut memory = navigator(9, &[0, 0], 0, 9);
        memory.shaky = Some(MAP_NODES);
        assert!(poll(&mut reader, &memory).is_empty());
        memory.shaky = None;
        assert_eq!(summary(&poll(&mut reader, &memory)), ["route 1: 2 nodes", "next 0"]);
    }

    #[test]
    fn what_does_not_add_up_reads_as_no_route() {
        let mut reader = RouteReader::default();
        assert_eq!(summary(&poll(&mut reader, &navigator(3, &[0, 0], 0, 3))), ["route 1: 2 nodes", "next 0"]);
        // A new route with a node of neither kind (a new game version's layout): no route, said once.
        assert_eq!(summary(&poll(&mut reader, &navigator(4, &[0, 7], 0, 4))), ["none"]);
        assert!(poll(&mut reader, &navigator(4, &[0, 7], 0, 4)).is_empty());
        // Only junction points: nothing to draw.
        assert!(poll(&mut reader, &navigator(4, &[1, 1], 0, 4)).is_empty());
        // No navigator object.
        let mut memory = navigator(4, &[0], 0, 4);
        memory.blocks.insert(GLOBAL, 0u64.to_le_bytes().to_vec());
        assert!(read_header(&memory, GLOBAL, NAVIGATOR).is_none());
        // A count past any real route's.
        let mut memory = navigator(4, &[0], 0, 4);
        let holder = memory.blocks.get_mut(&HOLDER).unwrap();
        holder[HOLDER_ROUTE + 0x10..HOLDER_ROUTE + 0x18].copy_from_slice(&((MAX_NODES + 1) as u64).to_le_bytes());
        assert!(read_header(&memory, GLOBAL, NAVIGATOR).is_none());
    }

    #[test]
    fn locates_the_navigator_from_the_getter_registered_beside_its_name() {
        // Constant data at rva 0x2000 with the name 0x10 in; code at rva 0x1000: the getter at 0x40, and at
        // 0x100 the registration: lea rax, [getter], then 6 bytes on, lea rax, [name].
        let mut rdata = vec![0u8; 0x40];
        rdata[0x10..0x10 + 26].copy_from_slice(b"truck.navigation.distance\0");
        let mut code = vec![0x90u8; 0x200];
        // The getter: sub rsp, 28h; mov rax, [rip + disp] (the global at rva 0x9000); test rax, rax; jnz +8;
        // xorps; add rsp, 28h; ret; lea rcx, [rax + 0x4128].
        let getter = [
            0x48, 0x83, 0xec, 0x28, 0x48, 0x8b, 0x05, 0, 0, 0, 0, 0x48, 0x85, 0xc0, 0x75, 0x08, 0x0f, 0x57, 0xc0, 0x48,
            0x83, 0xc4, 0x28, 0xc3, 0x48, 0x8d, 0x88, 0x28, 0x41, 0x00, 0x00,
        ];
        code[0x40..0x40 + getter.len()].copy_from_slice(&getter);
        let global_disp = (0x9000i64 - (0x1040 + 11)) as i32;
        code[0x47..0x4b].copy_from_slice(&global_disp.to_le_bytes());
        let lea = |code: &mut Vec<u8>, at: usize, target: usize| {
            code[at..at + 3].copy_from_slice(&[0x48, 0x8d, 0x05]);
            let disp = (target as i64 - (0x1000 + at as i64 + 7)) as i32;
            code[at + 3..at + 7].copy_from_slice(&disp.to_le_bytes());
        };
        lea(&mut code, 0x100, 0x1040);
        lea(&mut code, 0x10d, 0x2010);
        let layout = locate(&Section { rva: 0x1000, bytes: &code }, &Section { rva: 0x2000, bytes: &rdata });
        assert_eq!(layout, Some(Layout { global_rva: 0x9000, navigator: 0x4128 }));
        // Without the name, or with a different getter, nothing.
        let other = vec![0u8; 0x40];
        assert_eq!(locate(&Section { rva: 0x1000, bytes: &code }, &Section { rva: 0x2000, bytes: &other }), None);
    }

    #[test]
    fn reads_a_map_node_as_the_game_keeps_it() {
        let mut bytes = [0u8; MAP_NODE_SIZE];
        let x = (24150.95f32 * 256.0).round() as i32;
        let z = (54962.53f32 * 256.0).round() as i32;
        bytes[0..4].copy_from_slice(&x.to_le_bytes());
        bytes[8..12].copy_from_slice(&z.to_le_bytes());
        bytes[0x10..0x14].copy_from_slice(&(-0.8668967f32).to_le_bytes());
        bytes[0x18..0x1c].copy_from_slice(&0.4984878f32.to_le_bytes());
        bytes[0x30..0x38].copy_from_slice(&0x075ffeb53b4b09e5u64.to_le_bytes());
        let node = map_node_of(&bytes);
        assert!((node.position[0] - 24150.95).abs() < 0.01 && (node.position[1] - 54962.53).abs() < 0.01);
        // The map's own reading of this node (geometry::node_forward): (0.864, -0.503).
        assert!((node.forward[0] - 0.864275).abs() < 1e-3 && (node.forward[1] + 0.5030197).abs() < 1e-3);
        assert_eq!(node.uid, 0x075ffeb53b4b09e5);
    }

    #[test]
    fn reads_the_junctions_with_the_played_profiles_map_mods() {
        use westplay_setup::game::{FoundBy, Game};
        use westplay_setup::profiles::{MapContent, Mod, Profile};

        let a_mod = |package: &str, sectors: &[&str], files: bool| Mod {
            package: package.into(),
            name: package.into(),
            files: files.then(|| PathBuf::from(format!("mods/{package}.scs"))),
            categories: vec![],
            author: None,
            map: MapContent {
                sectors: sectors.iter().map(|s| (*s).to_owned()).collect(),
                definitions: false,
                hidden: false,
            },
            problem: None,
        };
        let mut profile = Profile::new_player();
        profile.mods =
            vec![a_mod("promods", &["europe"], true), a_mod("truck", &[], true), a_mod("gone", &["europe"], false)];
        // WestPlay set up in one install, the game running from another: the running one's files.
        let set_up = Game { dir: "set-up".into(), found_by: FoundBy::Steam, version: None, dlcs: vec![] };
        let running = Path::new("running");

        let files = JunctionFiles::of(running, Some(&(set_up.clone(), profile.clone())));
        assert_eq!(files.install, running);
        assert_eq!(files.played.as_ref().map(|(game, _)| game.dir.as_path()), Some(running));
        // Only the map mods it has the files of, in the profile's order.
        assert_eq!((files.map.as_str(), files.mods.clone()), ("europe", vec![PathBuf::from("mods/promods.scs")]));

        // Not worked out yet: the base game's map alone.
        let base = JunctionFiles::of(running, None);
        assert_eq!((base.map.as_str(), base.mods.len()), ("europe", 0));
        assert_ne!(base, files);

        // A save changes nothing read (the files stay open); other map mods do.
        let mut saved = profile.clone();
        saved.last_saved = Some(std::time::SystemTime::now());
        assert_eq!(JunctionFiles::of(running, Some(&(set_up.clone(), saved))), files);
        let mut other = profile;
        other.mods.push(a_mod("turkey", &["europe"], true));
        assert_ne!(JunctionFiles::of(running, Some(&(set_up, other))), files);
    }
}
