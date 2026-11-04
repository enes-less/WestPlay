//! How many threads a map build works on: never the whole processor. A build runs for minutes beside whatever the
//! player does, the game too if they start it meanwhile. How the work is shared doesn't change the map.

/// The threads a map build works on.
pub fn for_a_build() -> usize {
    half_of(std::thread::available_parallelism().map_or(1, |n| n.get()))
}

/// Half of a PC's `available` threads, one at the least.
fn half_of(available: usize) -> usize {
    (available / 2).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
