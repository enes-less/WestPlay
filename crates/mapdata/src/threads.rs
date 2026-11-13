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

/// The room for a build working with everything on a PC of `total` memory (bytes), `free` saying what the PC has free
/// now (the least of its free memory and what Windows can still lend): while what everything on it uses, with a
/// block's most ([`BLOCK_PEAK`]) for each of the PC's threads, stays under [`FULL_SHARE`] of `total`. WestPlay's
/// builds and the `build_map` example's `--fast` both use it.
///
/// Measured by the build's own memory, what other programs held was left out: a laptop of 15.6 GB with 6 GB taken by
/// others started a block on each of its threads and ran out ("memory allocation of 570425360 bytes failed", a
/// player's map of 380 map mods, 2026-11-12).
pub fn room_on(total: u64, free: impl Fn() -> Option<u64> + Send + Sync + 'static) -> Room {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()) as u64;
    let reserve = threads.saturating_mul(BLOCK_PEAK);
    let kept = total - total / FULL_SHARE.1 * FULL_SHARE.0;
    Arc::new(move || free().is_some_and(|free| free > reserve.saturating_add(kept)))
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
/// takes them in turn after, takes them as one thread doing them in order would have made them. A thread takes
/// `chunk` jobs at once (64 for quick ones, enough that taking them costs nothing; 1 for ones that take long and
/// unevenly: a model traced); `progress` is told the share of the jobs done as they're done, a few times a second.
pub(crate) fn in_order_telling<J: Sync, R: Send>(
    jobs: &[J],
    chunk: usize,
    work: impl Fn(&J) -> R + Sync,
    progress: &mut dyn FnMut(f32),
) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let chunk = chunk.max(1);
    let next = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    let chunks = jobs.len().div_ceil(chunk);
    let threads = for_a_build().min(chunks.max(1));
    let working = AtomicUsize::new(threads);
    let mut done: Vec<(usize, Vec<R>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let _leaving = Leaving(&working);
                    let mut done = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let start = at * chunk;
                        if start >= jobs.len() {
                            break;
                        }
                        let these = &jobs[start..(start + chunk).min(jobs.len())];
                        done.push((at, these.iter().map(&work).collect::<Vec<R>>()));
                        finished.fetch_add(these.len(), Ordering::Relaxed);
                    }
                    done
                })
            })
            .collect();
        // Told how far while any worker works (never waited on for a count alone: a worker that panics never counts
        // its jobs done, [`Leaving`]).
        while any_working(&working) {
            progress(finished.load(Ordering::Relaxed) as f32 / jobs.len().max(1) as f32);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        workers.into_iter().flat_map(|worker| worker.join().expect("a worker of in_order")).collect()
    });
    progress(1.0);
    done.sort_unstable_by_key(|(chunk, _)| *chunk);
    done.into_iter().flat_map(|(_, results)| results).collect()
}

/// Counts a worker out of `working` when it ends, however it ends (a panic too). A wait on workers (telling how far
/// they've got meanwhile) waits while any is still working, never on a count of jobs done alone: a worker that panics
/// never counts its job done, and a wait on that would wait for ever (the review of 2026-11-11: the build sitting still
/// at a share, its builder never ending, the next profiles' maps never made). `working` starts at the number of
/// workers, each taking its guard first thing.
pub(crate) struct Leaving<'a>(pub(crate) &'a std::sync::atomic::AtomicUsize);

impl Drop for Leaving<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::Release);
    }
}

/// Slots that work done on threads takes while it's done: two kinds of work on threads of their own, each taking one
/// as it works, so no more than the slots work at once (a build's: [`for_a_build`]). The painted water's threads
/// worked beside the items' threads, each as many as the build's, so a build of half the PC used all of it (the review
/// of 2026-11-11, minor 12).
pub(crate) struct Gate {
    free: std::sync::Mutex<usize>,
    freed: std::sync::Condvar,
}

impl Gate {
    pub(crate) fn new(slots: usize) -> Self {
        Self { free: std::sync::Mutex::new(slots.max(1)), freed: std::sync::Condvar::new() }
    }

    /// A slot, once one's free: given back as it goes.
    pub(crate) fn enter(&self) -> Slot<'_> {
        let mut free = self.free.lock().unwrap_or_else(|e| e.into_inner());
        while *free == 0 {
            free = self.freed.wait(free).unwrap_or_else(|e| e.into_inner());
        }
        *free -= 1;
        Slot(self)
    }
}

/// A [`Gate`]'s slot, given back when it goes (a panic too).
pub(crate) struct Slot<'a>(&'a Gate);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        *self.0.free.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        self.0.freed.notify_one();
    }
}

/// Whether any of the workers counted by `working` ([`Leaving`]) is still working.
pub(crate) fn any_working(working: &std::sync::atomic::AtomicUsize) -> bool {
    working.load(std::sync::atomic::Ordering::Acquire) > 0
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
    fn a_gate_lets_no_more_than_its_slots_work_at_once() {
        let gate = Gate::new(3);
        let (now, most) = (std::sync::atomic::AtomicUsize::new(0), std::sync::atomic::AtomicUsize::new(0));
        std::thread::scope(|scope| {
            for _ in 0..12 {
                scope.spawn(|| {
                    for _ in 0..20 {
                        let _slot = gate.enter();
                        let held = now.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                        most.fetch_max(held, std::sync::atomic::Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_micros(300));
                        now.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    }
                });
            }
        });
        assert_eq!(most.load(std::sync::atomic::Ordering::SeqCst), 3);
        assert_eq!(*gate.free.lock().unwrap(), 3, "every slot given back");
    }

    #[test]
    fn in_order_gives_the_results_in_the_jobs_order_whatever_thread_did_them() {
        let jobs: Vec<u64> = (0..10_000).collect();
        for chunk in [1, 64] {
            let mut told = Vec::new();
            let results = in_order_telling(
                &jobs,
                chunk,
                |job| {
                    // Uneven work, so threads finish out of turn.
                    if job % 97 == 0 {
                        std::thread::sleep(std::time::Duration::from_micros(200));
                    }
                    job * 3
                },
                &mut |done| told.push(done),
            );
            assert_eq!(results, jobs.iter().map(|job| job * 3).collect::<Vec<_>>(), "{chunk} at once");
            // Told going up, ending at the whole.
            assert!(told.windows(2).all(|pair| pair[0] <= pair[1]), "{chunk} at once: {told:?}");
            assert_eq!(told.last(), Some(&1.0));
        }
        assert!(in_order_telling(&[] as &[u64], 64, |job| *job, &mut |_| {}).is_empty());
    }

    #[test]
    fn room_lasts_while_a_block_for_each_thread_still_fits_under_four_fifths() {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()) as u64;
        let total = 64u64 << 30;
        // What's free past a block for each thread and the fifth kept free: what everything on the PC uses, the
        // build and every other program alike.
        let line = threads * BLOCK_PEAK + (total - total / 5 * 4);
        assert!(room_on(total, move || Some(line + 1))());
        assert!(!room_on(total, move || Some(line))());
        // Others' use counts: on a PC of 15.6 GB with 9.4 GB free (a player's laptop, 2026-11-12), a block on each of
        // eight threads and the fifth (4 + 3.1 GB) fit; on sixteen (8 + 3.1 GB) they don't.
        let laptop = 15_600_000_000u64;
        let room_with = |threads: u64| threads * BLOCK_PEAK + (laptop - laptop / 5 * 4) < 9_400_000_000;
        assert!(room_with(8) && !room_with(16));
        // Not known: no room past the usual.
        assert!(!room_on(total, || None)());
    }
}
