//! The game's weather, read from the game's memory as the route is (route.rs): read-only, from outside the
//! game, on a thread of its own. The SDK doesn't give it.
//!
//! The game has two weathers: nice (0) and bad (1: rain). Its weather object holds the one there is and the
//! one it's turning to (they differ while it turns, which takes about a minute), its timer to the next change
//! and how wet the roads are. A global points to an object holding a pointer to it. Saving, the game copies
//! it into the save's `economy` unit (`stored_current_weather`, `stored_weather_change_timer`, ...), and that
//! code is how it's found in the game's executable ([`locate`]): `mov rdi, [global]`, then
//!
//! ```text
//! mov rcx, [rdi + weather]; cmp [rcx + changing], r13d; je ...; mov eax, [rcx + target];
//! cmp [rcx + current], eax; jne ...; movss xmm9, [rcx + timer]; movss [rsi + ...], xmm9;
//! mov eax, [rcx + current]; mov [rsi + ...], eax; mov eax, [rcx + wetness]
//! ```
//!
//! Worked out on version 1.61.1.1, watching the weather turn to rain; what's read is checked (each weather
//! 0 or 1), and a game whose code doesn't match shows no weather.
//!
//! The timer falls by about 20 each game minute (the drive autosaves, 15 game minutes apart, have it about
//! 298 lower each time: [`TIMER_PER_GAME_MINUTE`]); the wetness is 0 on dry roads (0.59 in a save made in
//! the rain). Either one that doesn't read as such is left out, the weather kept.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use westplay_shared::BridgeMapping;

use crate::route::{Memory, Process, Section, section};

/// How often the weather is read: it changes every few minutes, over about one.
const READ_EVERY: Duration = Duration::from_secs(2);
/// How long before a game whose weather couldn't be found is looked at again.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// One of the game's two weathers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sky {
    Clear,
    Rain,
}

/// The weather there is, and the one it's turning to (the same once it has); how long until it changes next,
/// and how wet the roads are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weather {
    pub now: Sky,
    pub coming: Sky,
    /// Game minutes to the next change (`None`: not read).
    pub change_in: Option<u32>,
    /// How wet the roads are, percent (`None`: not read).
    pub wetness: Option<u8>,
}

/// How much the game's weather timer falls in a game minute (see the module's notes).
pub const TIMER_PER_GAME_MINUTE: f32 = 20.0;

#[cfg(test)]
impl Weather {
    /// Just the sky: what there is and what's coming.
    pub fn sky(now: Sky, coming: Sky) -> Self {
        Self { now, coming, change_in: None, wetness: None }
    }
}

/// Where the game keeps its weather: a global (its offset in the executable) pointing to an object, the
/// weather object's pointer in it, and the weather there is and the one coming in that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub global_rva: usize,
    pub weather: usize,
    pub current: usize,
    pub target: usize,
    pub timer: usize,
    pub wetness: usize,
}

/// The weather as it changes (`None`: no game, or its weather can't be read).
pub struct WeatherWatch {
    pub updates: Receiver<Option<Weather>>,
}

impl WeatherWatch {
    pub fn start(bridge: Arc<BridgeMapping>) -> Self {
        let (sender, updates) = mpsc::channel();
        let _ = std::thread::Builder::new().name("westplay-weather".into()).spawn(move || {
            let mut game: Option<(Process, Layout)> = None;
            let mut failed: Option<(u32, Instant)> = None;
            let mut sent: Option<Option<Weather>> = None;
            loop {
                let pid = bridge.bridge().plugin.pid.load(Ordering::Acquire);
                if game.as_ref().map(|(process, _)| process.pid) != Some(pid) {
                    game = None;
                    let retry = failed.is_none_or(|(failed_pid, at)| failed_pid != pid || at.elapsed() >= RETRY_AFTER);
                    if pid != 0 && retry {
                        game = open(pid);
                        failed = game.is_none().then(|| (pid, Instant::now()));
                    }
                }
                let weather = game.as_ref().and_then(|(process, layout)| read(process, process.base, layout));
                if sent != Some(weather) {
                    // The sky in the log as it changes (not every game minute the timer takes off).
                    let sky = |weather: Option<Weather>| weather.map(|w| (w.now, w.coming));
                    if sent.is_none_or(|before| sky(before) != sky(weather)) {
                        match weather {
                            Some(w) => log!(
                                "weather: {:?}, turning to {:?}; next change in {:?} game minutes, roads {:?}% wet",
                                w.now,
                                w.coming,
                                w.change_in,
                                w.wetness
                            ),
                            None => log!("weather: none read"),
                        }
                    }
                    if sender.send(weather).is_err() {
                        return;
                    }
                    sent = Some(weather);
                }
                std::thread::sleep(READ_EVERY);
            }
        });
        Self { updates }
    }
}

/// The game's process, and where it keeps its weather (from its executable file).
fn open(pid: u32) -> Option<(Process, Layout)> {
    let process = Process::open(pid, "weather")?;
    let image = std::fs::read(&process.exe).ok()?;
    let Some(layout) = section(&image, b".text\0").and_then(|text| locate(&text)) else {
        log!("weather: the game's weather wasn't found in {} (a new game version?)", process.exe.display());
        return None;
    };
    log!("weather: found in the game ({layout:x?})");
    Some((process, layout))
}

/// The weather, read through `layout` from the game loaded at `base`; `None` if it doesn't read, or doesn't
/// add up.
fn read(memory: &impl Memory, base: usize, layout: &Layout) -> Option<Weather> {
    let owner = memory.u64_at(base.checked_add(layout.global_rva)?)? as usize;
    if owner == 0 {
        return None;
    }
    let weather = memory.u64_at(owner.checked_add(layout.weather)?)? as usize;
    if weather == 0 {
        return None;
    }
    let sky = |at: usize| -> Option<Sky> {
        let mut word = [0u8; 4];
        if !memory.read(weather.checked_add(at)?, &mut word) {
            return None;
        }
        match u32::from_le_bytes(word) {
            0 => Some(Sky::Clear),
            1 => Some(Sky::Rain),
            _ => None,
        }
    };
    let float = |at: usize| -> Option<f32> {
        let mut word = [0u8; 4];
        memory.read(weather.checked_add(at)?, &mut word).then(|| f32::from_le_bytes(word))
    };
    let change_in = float(layout.timer)
        .filter(|timer| (0.0..1e6).contains(timer))
        .map(|timer| (timer / TIMER_PER_GAME_MINUTE).round() as u32);
    let wetness = float(layout.wetness).filter(|wet| (0.0..=1.0).contains(wet)).map(|wet| (wet * 100.0).round() as u8);
    Some(Weather { now: sky(layout.current)?, coming: sky(layout.target)?, change_in, wetness })
}

/// How far before the save's copy of the weather the global it's read through may be loaded, bytes.
const GLOBAL_BEFORE: usize = 0x800;

/// The weather's layout, from the game's code (`.text`): the save's copy of the weather (see the module's
/// notes), with the weather object's pointer and the weather there is and coming in it, and before it the
/// load of the global (`48 8B 3D disp32`: mov rdi, [rip + disp32]) nearest it.
fn locate(text: &Section) -> Option<Layout> {
    let code = text.bytes;
    let word = |at: usize| u32::from_le_bytes(code[at..at + 4].try_into().unwrap()) as usize;
    // The copy: its fixed bytes at their places, the rest (offsets, jumps) anything.
    const FIXED: [(usize, &[u8]); 11] = [
        (0, &[0x48, 0x8b, 0x8f]),              // mov rcx, [rdi + weather]
        (7, &[0x44, 0x39, 0xa9]),              // cmp [rcx + changing], r13d
        (14, &[0x74]),                         // je
        (16, &[0x8b, 0x81]),                   // mov eax, [rcx + target]
        (22, &[0x39, 0x81]),                   // cmp [rcx + current], eax
        (28, &[0x75]),                         // jne
        (30, &[0xf3, 0x44, 0x0f, 0x10, 0x89]), // movss xmm9, [rcx + timer]
        (39, &[0xf3, 0x44, 0x0f, 0x11, 0x8e]), // movss [rsi + ...], xmm9
        (48, &[0x8b, 0x81]),                   // mov eax, [rcx + current]
        (54, &[0x89, 0x86]),                   // mov [rsi + ...], eax
        (60, &[0x8b, 0x81]),                   // mov eax, [rcx + wetness]
    ];
    const LENGTH: usize = 66;
    let copy = (0..code.len().saturating_sub(LENGTH)).find(|&at| {
        FIXED.iter().all(|&(offset, bytes)| code[at + offset..].starts_with(bytes))
            // The weather there is, compared and then copied: the same field.
            && word(at + 24) == word(at + 50)
    })?;
    let global = (copy.saturating_sub(GLOBAL_BEFORE)..copy).rev().find(|&at| code[at..at + 3] == [0x48, 0x8b, 0x3d])?;
    let disp = i32::from_le_bytes(code[global + 3..global + 7].try_into().unwrap()) as isize;
    Some(Layout {
        global_rva: (text.rva + global + 7).wrapping_add_signed(disp),
        weather: word(copy + 3),
        current: word(copy + 24),
        target: word(copy + 18),
        timer: word(copy + 35),
        wetness: word(copy + 62),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// The game's copy of its weather as the code has it (1.61.1.1), the global loaded `before` bytes ahead.
    fn code(before: usize) -> Vec<u8> {
        let mut code = vec![0x90u8; 0x1000];
        let load = 0x400;
        code[load..load + 3].copy_from_slice(&[0x48, 0x8b, 0x3d]);
        code[load + 3..load + 7].copy_from_slice(&0x0012_3450i32.to_le_bytes());
        let copy = load + before;
        let bytes: [u8; 66] = [
            0x48, 0x8b, 0x8f, 0x98, 0x09, 0x00, 0x00, // mov rcx, [rdi + 0x998]
            0x44, 0x39, 0xa9, 0x7c, 0x45, 0x00, 0x00, // cmp [rcx + 0x457c], r13d
            0x74, 0x0e, // je
            0x8b, 0x81, 0x7c, 0x3e, 0x00, 0x00, // mov eax, [rcx + 0x3e7c]
            0x39, 0x81, 0x78, 0x3e, 0x00, 0x00, // cmp [rcx + 0x3e78], eax
            0x75, 0x09, // jne
            0xf3, 0x44, 0x0f, 0x10, 0x89, 0x78, 0x45, 0x00, 0x00, // movss xmm9, [rcx + 0x4578]
            0xf3, 0x44, 0x0f, 0x11, 0x8e, 0xe0, 0x05, 0x00, 0x00, // movss [rsi + 0x5e0], xmm9
            0x8b, 0x81, 0x78, 0x3e, 0x00, 0x00, // mov eax, [rcx + 0x3e78]
            0x89, 0x86, 0xe4, 0x05, 0x00, 0x00, // mov [rsi + 0x5e4], eax
            0x8b, 0x81, 0xe8, 0x45, 0x00, 0x00, // mov eax, [rcx + 0x45e8]
        ];
        code[copy..copy + 66].copy_from_slice(&bytes);
        code
    }

    #[test]
    fn finds_the_weather_through_the_saves_copy_of_it() {
        let code = code(0x3c4);
        let layout = locate(&Section { rva: 0x1000, bytes: &code }).unwrap();
        assert_eq!(
            layout,
            Layout {
                global_rva: 0x1000 + 0x400 + 7 + 0x0012_3450,
                weather: 0x998,
                current: 0x3e78,
                target: 0x3e7c,
                timer: 0x4578,
                wetness: 0x45e8,
            }
        );
    }

    #[test]
    fn code_that_isnt_the_copy_finds_nothing() {
        // The weather compared and then copied are different fields: not the copy.
        let mut other = code(0x3c4);
        other[0x400 + 0x3c4 + 50] = 0x79;
        assert_eq!(locate(&Section { rva: 0x1000, bytes: &other }), None);
        // The global loaded too far before it.
        assert_eq!(locate(&Section { rva: 0x1000, bytes: &code(0x900) }), None);
    }

    #[derive(Default)]
    struct FakeMemory {
        blocks: BTreeMap<usize, Vec<u8>>,
    }

    impl Memory for FakeMemory {
        fn read(&self, address: usize, buffer: &mut [u8]) -> bool {
            let Some((&start, block)) = self.blocks.range(..=address).next_back() else { return false };
            let Some(bytes) = block.get(address - start..address - start + buffer.len()) else { return false };
            buffer.copy_from_slice(bytes);
            true
        }
    }

    const LAYOUT: Layout =
        Layout { global_rva: 0x100, weather: 0x998, current: 0x3e78, target: 0x3e7c, timer: 0x4578, wetness: 0x45e8 };

    /// The game loaded at 0x1000, its weather object at 0x30000 with the weathers `now` and `coming`, the
    /// timer at 2032.8 (as an autosave had it) and dry roads.
    fn game(now: u32, coming: u32) -> FakeMemory {
        game_with(now, coming, 2032.8, 0.0)
    }

    fn game_with(now: u32, coming: u32, timer: f32, wetness: f32) -> FakeMemory {
        let mut memory = FakeMemory::default();
        memory.blocks.insert(0x1100, 0x20000u64.to_le_bytes().to_vec());
        let mut owner = vec![0u8; 0x9a0];
        owner[0x998..0x9a0].copy_from_slice(&0x30000u64.to_le_bytes());
        memory.blocks.insert(0x20000, owner);
        let mut weather = vec![0u8; 0x45f0];
        weather[0x3e78..0x3e7c].copy_from_slice(&now.to_le_bytes());
        weather[0x3e7c..0x3e80].copy_from_slice(&coming.to_le_bytes());
        weather[0x4578..0x457c].copy_from_slice(&timer.to_le_bytes());
        weather[0x45e8..0x45ec].copy_from_slice(&wetness.to_le_bytes());
        memory.blocks.insert(0x30000, weather);
        memory
    }

    #[test]
    fn reads_the_weather_there_is_and_the_one_coming() {
        let dry = |now, coming| Some(Weather { change_in: Some(102), wetness: Some(0), ..Weather::sky(now, coming) });
        assert_eq!(read(&game(0, 0), 0x1000, &LAYOUT), dry(Sky::Clear, Sky::Clear));
        assert_eq!(read(&game(0, 1), 0x1000, &LAYOUT), dry(Sky::Clear, Sky::Rain));
        assert_eq!(read(&game(1, 1), 0x1000, &LAYOUT), dry(Sky::Rain, Sky::Rain));
    }

    #[test]
    fn reads_when_it_changes_next_and_how_wet_the_roads_are() {
        // In the rain, as a save had it: 127.3 on the timer, the roads 0.588 wet.
        let raining = read(&game_with(1, 1, 127.28, 0.5877), 0x1000, &LAYOUT).unwrap();
        assert_eq!((raining.change_in, raining.wetness), (Some(6), Some(59)));
        // Values that aren't a timer or a wetness are left out; the weather is still read.
        let odd = read(&game_with(0, 0, f32::NAN, 7.5), 0x1000, &LAYOUT).unwrap();
        assert_eq!(odd, Weather::sky(Sky::Clear, Sky::Clear));
        let negative = read(&game_with(0, 0, -3.0, -0.2), 0x1000, &LAYOUT).unwrap();
        assert_eq!(negative, Weather::sky(Sky::Clear, Sky::Clear));
    }

    #[test]
    fn weather_that_doesnt_add_up_reads_as_none() {
        // Neither weather (a layout that isn't what it was).
        assert_eq!(read(&game(7, 1), 0x1000, &LAYOUT), None);
        // No weather object yet (a game loading).
        let mut loading = game(0, 0);
        loading.blocks.insert(0x20000, vec![0u8; 0x9a0]);
        assert_eq!(read(&loading, 0x1000, &LAYOUT), None);
        // Nothing where the global is.
        assert_eq!(read(&FakeMemory::default(), 0x1000, &LAYOUT), None);
    }
}
