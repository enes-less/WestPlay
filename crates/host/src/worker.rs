//! A worker's thread ending with its handle: the host's watches (the weather's, the route's, the maps') each run on
//! a thread of their own, their handles held by the host's run. A run ends (WestPlay stopping, or the host started
//! again: the cab moved to another GPU, its browser gone) and its handles go; a thread that only ended on failing to
//! send went on as long as it had nothing new to send, beside the next run's (a player's log, 2026-11-07: two of
//! everything, his map built twice at once into one folder).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Held by a worker's handle: dropped, its thread is told to end ([`Ended`]).
pub(crate) struct Ending(Arc<AtomicBool>);

/// The worker thread's side: whether its handle is gone.
#[derive(Clone)]
pub(crate) struct Ended(Arc<AtomicBool>);

/// A handle's [`Ending`] and its thread's [`Ended`].
pub(crate) fn ending() -> (Ending, Ended) {
    let flag = Arc::new(AtomicBool::new(false));
    (Ending(flag.clone()), Ended(flag))
}

impl Ended {
    pub(crate) fn now(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl Drop for Ending {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thread_hears_its_handle_go() {
        let (ending, ended) = ending();
        let thread = std::thread::spawn(move || {
            while !ended.now() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });
        drop(ending);
        thread.join().unwrap();
    }
}
