//! Changes the game's GPS route as the player does on the game's map (Maps' search: go to a place, add a stop,
//! end the route): the app asks (the bridge's [`westplay_shared::RouteBlock`]), and on the game's main thread
//! (the end of a frame) the plugin gives the game's navigator its new waypoints, through the game's own
//! function, as the game's map screen does. The game then plans the route through them; they are its own
//! waypoints, on its map, changed there as any other.
//!
//! The game keeps them in its navigator (found as the host finds it, crates/host/src/route.rs): lists
//! (`array_t`: data at `+0x08`, count at `+0x10`) of 0x18-byte targets, `{0, -1, map node, 2}`:
//! - `+0x1F8`: the waypoints in route order, then, on a job, the job's destination;
//! - `+0x248`: the waypoints, in the order they were added;
//! - `+0x1D0`: given empty (the game fills it as it plans), `+0x220`: the places to avoid, kept;
//! - the routing mode at `+0x1C8`: a job's (1-4, 6, 7) is kept; otherwise waypoints alone are mode 5, and no
//!   route at all mode 0. Without a job, the last waypoint is where the route ends.
//!
//! Its "set targets" function (`navigator, mode, four lists`) copies the lists in and plans the route again;
//! a list passed as the navigator's own is left as it is. Map nodes come from the game's node table by
//! uid (the function the `goto` console command uses). The game allows ten waypoints.
//!
//! Worked out on ETS2 1.61.1.1. Every piece is found in the running game's code by its shape, not by
//! address, and checked; anything not found, or a route not as described, and nothing is done.

use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use westplay_shared::{RouteAction, RouteAnswer};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

use crate::{bridge, log};

/// The game's limit on waypoints.
const MAX_WAYPOINTS: u64 = 10;
/// Offsets in the navigator.
const MODE: usize = 0x1c8;
const ROUTE: usize = 0x1f8;
const AVOID: usize = 0x220;
const WAYPOINTS: usize = 0x248;
/// More targets than a route can hold: a count past this is a misread.
const MAX_TARGETS: u64 = 64;

/// A target: `{u32 0, i32 -1, map node, u32 2 (a map node), padding}`.
type Target = [u8; 0x18];

/// The game's `array_t` as its functions read it.
#[repr(C)]
struct List {
    allocator: usize,
    data: *const Target,
    count: u64,
    capacity: u64,
}

type SetTargets = unsafe extern "system" fn(*mut u8, u32, *const List, *const List, *const List, *const List);
type FindNode = unsafe extern "system" fn(*mut u8, u64) -> *mut u8;

struct Game {
    /// The global holding the game's core object (the navigator's owner, and the node table's).
    core: *const *mut u8,
    navigator: usize,
    set_targets: SetTargets,
    find_node: FindNode,
}

// SAFETY: the pointers are into the game's own executable, fixed for the process's life, and only used on
// the game's main thread (`serve`).
unsafe impl Send for Game {}
// SAFETY: as for `Send`: never used from two threads at once.
unsafe impl Sync for Game {}

static GAME: OnceLock<Option<Game>> = OnceLock::new();

/// Answers the app's route request, if there's a new one.
///
/// # Safety
/// On the game's main thread only (the end of a frame, where the game itself changes its route).
pub unsafe fn serve() {
    let Some(bridge) = bridge::get() else { return };
    let block = &bridge.route;
    let request = block.request.load(Ordering::Acquire);
    if request == block.answered.load(Ordering::Relaxed) {
        return;
    }
    let action = RouteAction::from_code(block.action.load(Ordering::Acquire));
    let node = block.node.load(Ordering::Acquire);
    // SAFETY: on the game's main thread (`serve`'s contract), with pieces `locate` found and checked.
    let answer = match (GAME.get_or_init(|| unsafe { locate() }), action) {
        // SAFETY: as above: on the game's main thread, with `game` as `locate` found it.
        (Some(game), Some(action)) => unsafe { change(game, action, node) },
        (None, _) => RouteAnswer::Unsupported,
        (_, None) => RouteAnswer::Unexpected,
    };
    log!("route: {action:?} node {node:#x}: {answer:?}");
    block.answer.store(answer as i32, Ordering::Relaxed);
    block.answered.store(request, Ordering::Release);
}

/// Reads the navigator's route, plans the change ([`plan`]) and gives it the result.
///
/// # Safety
/// On the game's main thread, with `game` as `locate` found it.
unsafe fn change(game: &Game, action: RouteAction, uid: u64) -> RouteAnswer {
    // SAFETY: the global, the navigator's offset and the two functions were found in the game's own code by
    // their exact shape (`locate`); the core object is read only when set, and its lists only as `targets`
    // checks them. The lists given to the game live through its call, which copies them.
    unsafe {
        let core = *game.core;
        if core.is_null() {
            return RouteAnswer::Unexpected;
        }
        let navigator = core.add(game.navigator);
        let node = match action {
            RouteAction::End => None,
            RouteAction::AddStop | RouteAction::Go => match (game.find_node)(core, uid) {
                node if node.is_null() => return RouteAnswer::NoSuchNode,
                node => Some(target(node as usize)),
            },
        };
        let (Some(route), Some(waypoints)) = (targets(navigator.add(ROUTE)), targets(navigator.add(WAYPOINTS))) else {
            return RouteAnswer::Unexpected;
        };
        let mode = (navigator.add(MODE) as *const u32).read();
        let Planned { mode, route, waypoints } = match plan(action, mode, &route, &waypoints, node) {
            Ok(planned) => planned,
            Err(answer) => return answer,
        };
        let allocator = (navigator.add(WAYPOINTS) as *const List).read().allocator;
        let list = |targets: &[Target]| List {
            allocator,
            data: if targets.is_empty() { std::ptr::null() } else { targets.as_ptr() },
            count: targets.len() as u64,
            capacity: targets.len() as u64,
        };
        (game.set_targets)(
            navigator,
            mode,
            &list(&[]),
            &list(&route),
            navigator.add(AVOID) as *const List,
            &list(&waypoints),
        );
    }
    RouteAnswer::Done
}

/// A route as the navigator takes it: its mode, the waypoints in route order (and a job's destination last),
/// and the waypoints in the order they were added.
#[derive(Debug, PartialEq)]
struct Planned {
    mode: u32,
    route: Vec<Target>,
    waypoints: Vec<Target>,
}

/// The route after `action`, from the navigator's: its `mode`, `route` and `waypoints` (as [`Planned`]), and
/// the place asked for (`node`, for going or adding). A refusal when the game wouldn't allow it.
fn plan(
    action: RouteAction,
    mode: u32,
    route: &[Target],
    waypoints: &[Target],
    node: Option<Target>,
) -> Result<Planned, RouteAnswer> {
    let job = matches!(mode, 1..=4 | 6 | 7);
    // On a job the route is its waypoints and then the job's destination; anything else is a misread.
    let destination = match (job, route.split_last()) {
        (true, Some((destination, before))) if before.len() == waypoints.len() => Some(*destination),
        (true, _) => return Err(RouteAnswer::Unexpected),
        (false, _) => None,
    };
    let ordered = &route[..route.len() - destination.is_some() as usize];
    let with_destination = |stops: &[Target]| [stops, destination.as_slice()].concat();
    let keep = |route: Vec<Target>, waypoints: Vec<Target>| {
        let mode = match (job, waypoints.is_empty()) {
            (true, _) => mode,
            (false, true) => 0,
            (false, false) => 5,
        };
        Ok(Planned { mode, route, waypoints })
    };
    match (action, node) {
        (RouteAction::End, _) => keep(with_destination(&[]), Vec::new()),
        (RouteAction::Go, _) if job => Err(RouteAnswer::OnJob),
        (RouteAction::Go, Some(node)) => keep(vec![node], vec![node]),
        (RouteAction::AddStop, Some(node)) => {
            if waypoints.iter().any(|stop| node_of(stop) == node_of(&node)) {
                return Err(RouteAnswer::AlreadyThere);
            }
            if waypoints.len() as u64 >= MAX_WAYPOINTS {
                return Err(RouteAnswer::Full);
            }
            // The new stop goes after the others, before a job's destination (for now: where it adds the
            // least detour, as the game's map chooses, is to come).
            keep(with_destination(&[ordered, &[node]].concat()), [waypoints, &[node]].concat())
        }
        (RouteAction::Go | RouteAction::AddStop, None) => Err(RouteAnswer::Unexpected),
    }
}

/// A list's targets, if it reads as one.
///
/// # Safety
/// `list` points at a game `array_t` of targets (the navigator's).
unsafe fn targets(list: *const u8) -> Option<Vec<Target>> {
    // SAFETY: `list` is an `array_t` (the caller's contract); its data is read only when its count is
    // plausible and its data set.
    let targets = unsafe {
        let list = &*(list as *const List);
        if list.count > MAX_TARGETS || (list.count > 0 && list.data.is_null()) {
            return None;
        }
        std::slice::from_raw_parts(list.data, list.count as usize).to_vec()
    };
    targets.iter().all(|target| target[16..20] == [2, 0, 0, 0] && node_of(target) != 0).then_some(targets)
}

fn node_of(target: &Target) -> usize {
    usize::from_le_bytes(target[8..16].try_into().unwrap())
}

fn target(node: usize) -> Target {
    let mut target = [0u8; 0x18];
    target[4..8].copy_from_slice(&(-1i32).to_le_bytes());
    target[8..16].copy_from_slice(&node.to_le_bytes());
    target[16..20].copy_from_slice(&2u32.to_le_bytes());
    target
}

/// Finds the pieces in the running game's code; `None` (logged) if any isn't there exactly once.
///
/// # Safety
/// In the game's process (the plugin is loaded in it).
unsafe fn locate() -> Option<Game> {
    let found = (|| {
        // SAFETY: no name: the game's own executable, mapped for the process's life.
        let base = unsafe { GetModuleHandleW(None) }.ok()?.0 as usize;
        // SAFETY: `base` is the game's executable, as mapped.
        let image = unsafe { Image::of(base) }?;
        let (core, navigator) = image.navigator()?;
        let set_targets = image.unique(SET_TARGETS)?;
        let find_node = image.find_node()?;
        log!(
            "waypoint: game code found: core {:#x}, navigator +{navigator:#x}, set targets {:#x}, find node {:#x}",
            core - base,
            set_targets - base,
            find_node - base
        );
        // SAFETY: each address is the start of the function its pattern names, found exactly once in the
        // game's code; the patterns pin their calling conventions (`SetTargets`, `FindNode`).
        let (set_targets, find_node) = unsafe {
            (std::mem::transmute::<usize, SetTargets>(set_targets), std::mem::transmute::<usize, FindNode>(find_node))
        };
        Some(Game { core: core as *const *mut u8, navigator, set_targets, find_node })
    })();
    if found.is_none() {
        log!("waypoint: the game's waypoint code isn't the version WestPlay knows; waypoints can't be added");
    }
    found
}

/// The start of the navigator's "set targets": it saves its registers, keeps the mode
/// (`mov [rcx+1C8h], edx`) and starts on the first list (`add rcx, 1D0h`).
const SET_TARGETS: &[u8] = &[
    0x48, 0x89, 0x5c, 0x24, 0x08, 0x48, 0x89, 0x74, 0x24, 0x10, 0x57, 0x48, 0x83, 0xec, 0x20, 0x48, 0x8b, 0xf9, 0x89,
    0x91, 0xc8, 0x01, 0x00, 0x00, 0x48, 0x81, 0xc1, 0xd0, 0x01, 0x00, 0x00,
];

/// The start of a lookup by uid in a table at `+0x38` of its object (`lea rbx, [rcx+38h]`).
const FIND_IN_TABLE_38: &[u8] = &[
    0x48, 0x89, 0x54, 0x24, 0x10, 0x53, 0x48, 0x83, 0xec, 0x40, 0x48, 0x8d, 0x59, 0x38, 0x48, 0x8b, 0xcb, 0x48, 0x8d,
    0x54, 0x24, 0x58, 0xe8,
];

/// The game's executable, as mapped in its process.
pub(crate) struct Image {
    text: (usize, usize),
    rdata: (usize, usize),
}

impl Image {
    /// The image mapped at `base`: its code and its constants.
    ///
    /// # Safety
    /// `base` is the start of a module mapped in this process (a PE image), for as long as the `Image` lives.
    pub(crate) unsafe fn of(base: usize) -> Option<Self> {
        // SAFETY: the PE headers lie at the start of the mapped image (the caller's contract), read as they lie.
        let u16_at = |at: usize| unsafe { (base as *const u8).add(at).cast::<u16>().read_unaligned() } as usize;
        // SAFETY: as above.
        let u32_at = |at: usize| unsafe { (base as *const u8).add(at).cast::<u32>().read_unaligned() } as usize;
        let header = u32_at(0x3c);
        let (count, optional) = (u16_at(header + 6), u16_at(header + 20));
        let section = |name: &[u8]| {
            (0..count).find_map(|i| {
                let at = header + 24 + optional + i * 40;
                // SAFETY: a section header's name, within the headers the image maps.
                let bytes = unsafe { std::slice::from_raw_parts((base + at) as *const u8, 8) };
                bytes.starts_with(name).then(|| (base + u32_at(at + 12), u32_at(at + 8)))
            })
        };
        Some(Image { text: section(b".text\0")?, rdata: section(b".rdata\0")? })
    }

    fn code(&self) -> &[u8] {
        // SAFETY: the image's code section, mapped for the process's life (`Image::of`).
        unsafe { std::slice::from_raw_parts(self.text.0 as *const u8, self.text.1) }
    }

    fn constants(&self) -> &[u8] {
        // SAFETY: the image's constants section, mapped for the process's life (`Image::of`).
        unsafe { std::slice::from_raw_parts(self.rdata.0 as *const u8, self.rdata.1) }
    }

    /// Where `pattern` starts in the code, if exactly once (`None` in it: any byte).
    pub(crate) fn unique_masked(&self, pattern: &[Option<u8>]) -> Option<usize> {
        let fits = |window: &[u8]| window.iter().zip(pattern).all(|(byte, wanted)| wanted.is_none_or(|w| *byte == w));
        let mut found = self.code().windows(pattern.len()).enumerate().filter(|(_, w)| fits(w));
        let (at, _) = found.next()?;
        found.next().is_none().then_some(self.text.0 + at)
    }

    /// Where `pattern` starts in the code, if exactly once.
    fn unique(&self, pattern: &[u8]) -> Option<usize> {
        let mut found = self.code().windows(pattern.len()).enumerate().filter(|(_, w)| *w == pattern);
        let (at, _) = found.next()?;
        found.next().is_none().then_some(self.text.0 + at)
    }

    /// Where a RIP-relative lea at `at` (an address in the code) points.
    fn lea_target(&self, at: usize) -> Option<usize> {
        let offset = at.checked_sub(self.text.0)?;
        let b = self.code().get(offset..offset + 7)?;
        let rip_relative = (b[0] == 0x48 || b[0] == 0x4c) && b[1] == 0x8d && b[2] & 0xc7 == 0x05;
        rip_relative.then(|| (at + 7).wrapping_add_signed(i32::from_le_bytes(b[3..7].try_into().unwrap()) as isize))
    }

    fn constant(&self, text: &[u8]) -> Option<usize> {
        self.constants().windows(text.len()).position(|w| w == text).map(|at| self.rdata.0 + at)
    }

    /// The game's node lookup: of the lookups shaped like it, the one that names the node table
    /// (`array_t<node_item_t*>`) in its bounds check.
    fn find_node(&self) -> Option<usize> {
        let name = self.constant(b"??A?$array_t@PEAVnode_item_t@prism@@@prism@@QEAAAEAPEAVnode_item_t@1@_K@Z\0")?;
        let code = self.code();
        let mut found =
            code.windows(FIND_IN_TABLE_38.len()).enumerate().filter(|(_, w)| *w == FIND_IN_TABLE_38).filter(
                |&(at, _)| {
                    (at..(at + 0x80).min(code.len() - 7)).any(|i| self.lea_target(self.text.0 + i) == Some(name))
                },
            );
        let (at, _) = found.next()?;
        found.next().is_none().then_some(self.text.0 + at)
    }

    /// The global holding the core object and the navigator's offset in it, from the telemetry getter for
    /// `truck.navigation.distance` (registered right beside its name): `sub rsp, 28h; mov rax, [global]`,
    /// then `lea rcx, [rax + navigator]` (as crates/host/src/route.rs finds them).
    fn navigator(&self) -> Option<(usize, usize)> {
        let name = self.constant(b"truck.navigation.distance\0")?;
        let code = self.code();
        for site in (0..code.len() - 7).map(|i| self.text.0 + i).filter(|&at| self.lea_target(at) == Some(name)) {
            for back in 1..=64 {
                let Some(getter) = self.lea_target(site - back) else { continue };
                let Some(b) = getter.checked_sub(self.text.0).and_then(|start| code.get(start..start + 48)) else {
                    continue;
                };
                if b[..4] != [0x48, 0x83, 0xec, 0x28] || b[4..7] != [0x48, 0x8b, 0x05] {
                    continue;
                }
                let global =
                    (getter + 11).wrapping_add_signed(i32::from_le_bytes(b[7..11].try_into().unwrap()) as isize);
                let at = (11..41).find(|&i| b[i..i + 3] == [0x48, 0x8d, 0x88])?;
                return Some((global, u32::from_le_bytes(b[at + 3..at + 7].try_into().unwrap()) as usize));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(targets: &[Target]) -> Vec<usize> {
        targets.iter().map(node_of).collect()
    }

    #[test]
    fn adds_a_stop_before_a_jobs_destination_and_keeps_the_jobs_mode() {
        let (a, b, destination, new) = (target(0xa0), target(0xb0), target(0xd0), target(0xe0));
        let planned = plan(RouteAction::AddStop, 1, &[a, b, destination], &[b, a], Some(new)).unwrap();
        assert_eq!(planned.mode, 1);
        assert_eq!(nodes(&planned.route), [0xa0, 0xb0, 0xe0, 0xd0]);
        // The order they were added in, the new one last.
        assert_eq!(nodes(&planned.waypoints), [0xb0, 0xa0, 0xe0]);
        // A job with no stops yet.
        let planned = plan(RouteAction::AddStop, 1, &[destination], &[], Some(new)).unwrap();
        assert_eq!(nodes(&planned.route), [0xe0, 0xd0]);
    }

    #[test]
    fn without_a_job_the_last_stop_is_where_the_route_ends() {
        let (a, new) = (target(0xa0), target(0xe0));
        // No route: the stop is the route (mode 5, the game's for waypoints alone).
        let planned = plan(RouteAction::AddStop, 0, &[], &[], Some(new)).unwrap();
        assert_eq!((planned.mode, nodes(&planned.route)), (5, vec![0xe0]));
        let planned = plan(RouteAction::AddStop, 5, &[a], &[a], Some(new)).unwrap();
        assert_eq!(nodes(&planned.route), [0xa0, 0xe0]);
        // Go: that place alone.
        let planned = plan(RouteAction::Go, 5, &[a], &[a], Some(new)).unwrap();
        assert_eq!((planned.mode, nodes(&planned.route), nodes(&planned.waypoints)), (5, vec![0xe0], vec![0xe0]));
    }

    #[test]
    fn ends_a_route_or_a_jobs_stops() {
        let (a, destination) = (target(0xa0), target(0xd0));
        let planned = plan(RouteAction::End, 5, &[a], &[a], None).unwrap();
        assert_eq!(planned, Planned { mode: 0, route: vec![], waypoints: vec![] });
        // On a job, the way to its destination stays.
        let planned = plan(RouteAction::End, 2, &[a, destination], &[a], None).unwrap();
        assert_eq!((planned.mode, nodes(&planned.route), planned.waypoints.len()), (2, vec![0xd0], 0));
    }

    #[test]
    fn refuses_what_the_game_wouldnt_allow() {
        let (a, destination, new) = (target(0xa0), target(0xd0), target(0xe0));
        // A job's destination stays the job's.
        assert_eq!(plan(RouteAction::Go, 1, &[destination], &[], Some(new)), Err(RouteAnswer::OnJob));
        assert_eq!(plan(RouteAction::AddStop, 5, &[a], &[a], Some(a)), Err(RouteAnswer::AlreadyThere));
        let ten: Vec<Target> = (1..=10).map(|i| target(i * 0x10)).collect();
        assert_eq!(plan(RouteAction::AddStop, 5, &ten, &ten, Some(new)), Err(RouteAnswer::Full));
        // A job's route not its stops and then its destination: misread, so nothing is done.
        assert_eq!(plan(RouteAction::AddStop, 1, &[a], &[a], Some(new)), Err(RouteAnswer::Unexpected));
    }

    #[test]
    fn a_target_is_a_map_node_as_the_game_writes_it() {
        // As read from the game: a waypoint at map node 0x2432d8e4660 (the last four bytes are padding).
        let mut seen = target(0x2432d8e4660);
        seen[20..24].copy_from_slice(&0xd4u32.to_le_bytes());
        assert_eq!(&seen[..20], &hex("00000000ffffffff60468e2d4302000002000000")[..]);
        assert_eq!(node_of(&seen), 0x2432d8e4660);
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
    }
}
