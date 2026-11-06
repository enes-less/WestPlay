//! How hard a map build works. As it is, never the whole processor: a build runs for minutes beside whatever the
//! player does, the game too if they start it meanwhile. With the companion's Prepare Maps Faster (owner 2026-11-06:
//! players with big map mods and PCs to match), every thread, and blocks drawn side by side as long as the memory
//! has room for them ([`Effort`]). How the work is shared doesn't change the map.

use std::sync::{Arc, RwLock};

/// Whether there's room in memory for more of the map to be worked on at once (the caller's measure).
pub type Room = Arc<dyn Fn() -> bool + Send + Sync>;

/// How hard the builds work: set by the caller before a build, for that build (one at a time, as WestPlay builds
/// them). `None` is the default: half the processor, the blocks kept to their budget.
static EFFORT: RwLock<Option<Room>> = RwLock::new(None);

/// The most of the PC's memory a build working with everything fills with blocks drawn at once: four fifths (owner
/// 2026-11-06). Not a limit on the build: past it, its blocks are only drawn no more at once than as usual.
pub const FULL_SHARE: (u64, u64) = (4, 5);

/// What a block of the map takes as it's drawn, at the most, bytes: the densest (a city's) about half a gigabyte in
/// the height pass (measured 2026-11-05). More blocks start at once only while one of these for each thread would
/// still fit under [`FULL_SHARE`].
pub const BLOCK_PEAK: u64 = 1 << 29;

/// The room for a build working with everything on a PC of `total` memory (bytes), `taken` saying what the process
/// has taken now: while that, with a block's most ([`BLOCK_PEAK`]) for each of the PC's threads, stays under
/// [`FULL_SHARE`] of `total`. WestPlay's builds and the `build_map` example's `--fast` both use it.
pub fn room_on(total: u64, taken: impl Fn() -> Option<u64> + Send + Sync + 'static) -> Room {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()) as u64;
    let reserve = threads.saturating_mul(BLOCK_PEAK);
    let share = total / FULL_SHARE.1 * FULL_SHARE.0;
    Arc::new(move || taken().is_some_and(|taken| taken.saturating_add(reserve) < share))
}

/// A build working with everything: every thread, and more blocks at once while `room` says there's room. Back to
/// the default as it goes.
pub struct Effort(());

impl Effort {
    pub fn full(room: Room) -> Self {
        *EFFORT.write().unwrap_or_else(|e| e.into_inner()) = Some(room);
        Self(())
    }
}

impl Drop for Effort {
    fn drop(&mut self) {
        *EFFORT.write().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// The threads a map build works on.
pub fn for_a_build() -> usize {
    let available = std::thread::available_parallelism().map_or(1, |n| n.get());
    if full() { available } else { half_of(available) }
}

/// Whether a build works with everything ([`Effort::full`]).
fn full() -> bool {
    EFFORT.read().unwrap_or_else(|e| e.into_inner()).is_some()
}

/// Whether more blocks may be worked on than their budget lets (`surface::Budget`): only with [`Effort::full`],
/// while its measure says there's room.
pub(crate) fn room() -> bool {
    let room = EFFORT.read().unwrap_or_else(|e| e.into_inner()).clone();
    room.is_some_and(|room| room())
}

/// `work` done for each of `jobs` on a build's threads ([`for_a_build`]), the results in the jobs' order: whoever
/// takes them in turn after, takes them as one thread doing them in order would have made them.
pub(crate) fn in_order<J: Sync, R: Send>(jobs: &[J], work: impl Fn(&J) -> R + Sync) -> Vec<R> {
    /// Jobs a thread takes at once: few enough to share them evenly, enough that taking them costs nothing.
    const CHUNK: usize = 64;
    let next = std::sync::atomic::AtomicUsize::new(0);
    let chunks = jobs.len().div_ceil(CHUNK);
    let mut done: Vec<(usize, Vec<R>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..for_a_build().min(chunks.max(1)))
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let chunk = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let start = chunk * CHUNK;
                        if start >= jobs.len() {
                            break;
                        }
                        let these = &jobs[start..(start + CHUNK).min(jobs.len())];
                        done.push((chunk, these.iter().map(&work).collect::<Vec<R>>()));
                    }
                    done
                })
            })
            .collect();
        workers.into_iter().flat_map(|worker| worker.join().expect("a worker of in_order")).collect()
    });
    done.sort_unstable_by_key(|(chunk, _)| *chunk);
    done.into_iter().flat_map(|(_, results)| results).collect()
}

/// Half of a PC's `available` threads, one at the least.
fn half_of(available: usize) -> usize {
    (available / 2).max(1)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Tests that set the effort, one at a time (it's the whole process's).
    pub(crate) static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn a_build_leaves_half_the_processor_free() {
        assert_eq!(half_of(32), 16);
        assert_eq!(half_of(12), 6);
        assert_eq!(half_of(4), 2);
        assert_eq!(half_of(3), 1);
        // A PC of one or two threads still builds.
        assert_eq!(half_of(2), 1);
        assert_eq!(half_of(1), 1);
    }

    #[test]
    fn full_effort_takes_every_thread_and_room_while_it_lasts() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let available = std::thread::available_parallelism().map_or(1, |n| n.get());
        assert_eq!(for_a_build(), half_of(available));
        assert!(!room());
        {
            let _effort = Effort::full(Arc::new(|| true));
            assert_eq!(for_a_build(), available);
            assert!(room());
        }
        assert_eq!(for_a_build(), half_of(available));
        assert!(!room());
        let _effort = Effort::full(Arc::new(|| false));
        assert!(!room());
    }

    #[test]
    fn in_order_gives_the_results_in_the_jobs_order_whatever_thread_did_them() {
        let jobs: Vec<u64> = (0..10_000).collect();
        let results = in_order(&jobs, |job| {
            // Uneven work, so threads finish out of turn.
            if job % 97 == 0 {
                std::thread::sleep(std::time::Duration::from_micros(200));
            }
            job * 3
        });
        assert_eq!(results, jobs.iter().map(|job| job * 3).collect::<Vec<_>>());
        assert!(in_order(&[] as &[u64], |job| *job).is_empty());
    }

    #[test]
    fn room_lasts_while_a_block_for_each_thread_still_fits_under_four_fifths() {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()) as u64;
        let total = 64u64 << 30;
        let line = total / 5 * 4 - threads * BLOCK_PEAK;
        assert!(room_on(total, move || Some(line - 1))());
        assert!(!room_on(total, move || Some(line))());
        // Not known: no room past the usual.
        assert!(!room_on(total, || None)());
    }
}
