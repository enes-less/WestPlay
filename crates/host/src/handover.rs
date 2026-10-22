//! Handing the route the player picked to the game's GPS (PLAN.md: Navigation plan, B; the owner's way: we
//! plan, the game drives). The game is given the destination; its route is read back (route.rs) and
//! compared with the one picked, on road pieces only: where the picked route runs on anything else (a
//! junction, a border crossing's or a toll plaza's invisible lanes) the game takes its own lane, as it knows
//! which ones trucks use (owner 2026-10-22: ours went through a gated yard beside a border crossing). Where
//! it goes another way, a waypoint in the middle of the stretch of the picked route it avoids holds it there,
//! on a road piece and nothing else (owner 2026-10-22), and the game plans again, up to the game's own most
//! waypoints. Once it follows, the waypoints it doesn't need are taken out again, one at a time: the fewest
//! that hold it stay (owner 2026-10-21). If it won't follow, its own route stands, and the page is told.

use std::collections::HashSet;

/// The most waypoints put in to hold the game to a route: the most the game takes, ten with the destination
/// (owner 2026-10-21).
pub const MOST_VIAS: usize = westplay_shared::MAX_VIAS;
/// How many nodes past the stretch the game avoids a road piece for a waypoint is looked for, when the
/// stretch has none.
const MIDDLE_WITHIN: usize = 12;

/// Whether a node of the route is on road pieces alone (`westplay_mapdata::routing::Graph::road_piece`: every
/// way to and from it a road the player sees): the only nodes a waypoint goes on, and the only ones the
/// game's route is compared on. Anything else (a junction's edge or inside, an invisible road) is `No`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    Road,
    No,
}

/// What to do after reading the game's route. `Hold`, `TryWithout` and `PutBack` hand the game
/// [`Handover::handed`] again: the waypoints with one more (at `via`, an index into the route picked, where
/// the game left it at `parts`), with one taken out to see if the game still follows, or as they were after
/// the game didn't. `Follows` and `GivesUp` end it.
#[derive(Debug, PartialEq)]
pub enum Step {
    Follows,
    Hold { parts: usize, via: usize },
    TryWithout(u64),
    PutBack,
    GivesUp,
}

/// Where a hand-over is: holding the game to the route with more waypoints, taking out the ones not needed
/// (`trying` the one out now; `needed`, those the game left the route without; `untried`, those not tried yet,
/// in route order), or handing back those that held it after the last one tried turned out to be needed
/// (`pruned`: none are taken out again).
#[derive(Debug)]
enum Phase {
    Holding { pruned: bool },
    Pruning { trying: u64, needed: Vec<u64>, untried: Vec<u64> },
    PuttingBack,
}

/// A route being handed to the game's GPS: the route picked (its map nodes in order, where a waypoint can hold
/// the game at each, and how far along it each is, m) and the waypoints the game was last handed.
#[derive(Debug)]
pub struct Handover {
    picked: Vec<u64>,
    holds: Vec<Hold>,
    along: Vec<f32>,
    handed: Vec<u64>,
    phase: Phase,
}

impl Handover {
    /// The route picked: its map nodes in order, where a waypoint can hold the game at each, and where each is
    /// ([x, z], m). The game is first handed none.
    pub fn new(picked: Vec<u64>, holds: Vec<Hold>, positions: &[[f32; 2]]) -> Self {
        let along = if positions.len() == picked.len() {
            let mut total = 0.0;
            let mut along = Vec::with_capacity(positions.len());
            for (at, here) in positions.iter().enumerate() {
                if let Some(before) = at.checked_sub(1).map(|before| positions[before]) {
                    total += (here[0] - before[0]).hypot(here[1] - before[1]);
                }
                along.push(total);
            }
            along
        } else {
            // Where they are unknown: each node a step on.
            (0..picked.len()).map(|at| at as f32).collect()
        };
        Self { picked, holds, along, handed: Vec::new(), phase: Phase::Holding { pruned: false } }
    }

    /// The route picked.
    pub fn picked(&self) -> &[u64] {
        &self.picked
    }

    /// The waypoints to hand the game (and last handed it), in the order the route reaches them.
    pub fn handed(&self) -> &[u64] {
        &self.handed
    }

    /// Reads the game's route after it was last handed the waypoints (`game`, its map nodes in order: 0 for
    /// points inside junctions): what to do next.
    pub fn read(&mut self, game: &[u64]) -> Step {
        let follows = self.parting(game).is_none();
        match std::mem::replace(&mut self.phase, Phase::Holding { pruned: true }) {
            Phase::Holding { pruned } if follows => {
                if pruned || self.handed.len() < 2 {
                    // One waypoint is needed: without it the game went its own way (the first round).
                    Step::Follows
                } else {
                    let mut untried = self.handed.clone();
                    let trying = untried.remove(0);
                    self.try_without(trying, Vec::new(), untried)
                }
            }
            Phase::Holding { pruned } => {
                self.phase = Phase::Holding { pruned };
                self.hold(game)
            }
            Phase::Pruning { trying, mut needed, mut untried } => {
                // Without `trying` the game left the route: it's needed.
                if !follows {
                    needed.push(trying);
                }
                if untried.is_empty() {
                    if follows {
                        Step::Follows
                    } else {
                        self.handed = self.in_route_order(needed);
                        self.phase = Phase::PuttingBack;
                        Step::PutBack
                    }
                } else if needed.is_empty() && untried.len() == 1 {
                    // The last one left, all the others taken out: needed (without any, the game went its own
                    // way: the first round). The game has it alone, and follows.
                    Step::Follows
                } else {
                    let next = untried.remove(0);
                    self.try_without(next, needed, untried)
                }
            }
            // Handed back the waypoints that held it before: it follows again, or is held anew.
            Phase::PuttingBack if follows => Step::Follows,
            Phase::PuttingBack => self.hold(game),
        }
    }

    /// The game was handed the waypoints but `trying`: whether it still follows is read next.
    fn try_without(&mut self, trying: u64, needed: Vec<u64>, untried: Vec<u64>) -> Step {
        self.handed = self.in_route_order(needed.iter().chain(&untried).copied().collect());
        self.phase = Phase::Pruning { trying, needed, untried };
        Step::TryWithout(trying)
    }

    /// Where the game leaves the route picked: one more waypoint to hold it there, or it won't be held.
    fn hold(&mut self, game: &[u64]) -> Step {
        let Some(parts) = self.parting(game) else { return Step::Follows };
        if self.handed.len() >= MOST_VIAS {
            return Step::GivesUp;
        }
        let Some(via) = self.holding_at(parts, game) else { return Step::GivesUp };
        let mut handed = self.handed.clone();
        handed.push(self.picked[via]);
        self.handed = self.in_route_order(handed);
        Step::Hold { parts, via }
    }

    /// Where the game's route (`game`, its map nodes in order: 0 for points inside junctions) first leaves the
    /// route picked: the index in it of the first road piece the game's doesn't pass (the start aside: the
    /// truck may be past it); `None` if it passes them all.
    fn parting(&self, game: &[u64]) -> Option<usize> {
        let taken: HashSet<u64> = game.iter().copied().filter(|&uid| uid != 0).collect();
        (1..self.picked.len()).find(|&at| self.road(at) && !taken.contains(&self.picked[at]))
    }

    /// Whether the route picked's node at `at` is on road pieces alone.
    fn road(&self, at: usize) -> bool {
        self.holds.get(at) == Some(&Hold::Road)
    }

    /// Where on the route picked a waypoint holds the game to it, the game leaving it at `parts`: in the
    /// stretch it avoids (from there to where its route comes back to the route picked), the waypoint as near
    /// the stretch's middle as can be, so the game takes the whole stretch to reach it. On a road piece and
    /// nothing else (owner 2026-10-22), never the destination: in the stretch if it has one, else the first
    /// one after it, near; else none (the game takes its own way).
    fn holding_at(&self, parts: usize, game: &[u64]) -> Option<usize> {
        let taken: HashSet<u64> = game.iter().copied().filter(|&uid| uid != 0).collect();
        let last = self.picked.len().saturating_sub(1);
        let back = (parts + 1..last).find(|&at| taken.contains(&self.picked[at])).unwrap_or(last);
        let middle = (self.along[parts] + self.along[back]) / 2.0;
        let free = |at: &usize| self.road(*at) && !self.handed.contains(&self.picked[*at]);
        (parts..back.min(last))
            .filter(free)
            .min_by(|&a, &b| (self.along[a] - middle).abs().total_cmp(&(self.along[b] - middle).abs()))
            .or_else(|| (back..last).take(MIDDLE_WITHIN).find(free))
    }

    /// Waypoints in the order the route picked reaches them.
    fn in_route_order(&self, mut vias: Vec<u64>) -> Vec<u64> {
        vias.sort_by_key(|via| self.picked.iter().position(|uid| uid == via).unwrap_or(usize::MAX));
        vias
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A route picked of `count` nodes 1, 2, 3…, all on road pieces, each 100 m on from the last.
    fn route(count: u64) -> Handover {
        let picked: Vec<u64> = (1..=count).collect();
        let positions: Vec<[f32; 2]> = (0..count).map(|at| [at as f32 * 100.0, 0.0]).collect();
        Handover::new(picked, vec![Hold::Road; count as usize], &positions)
    }

    /// The game's way along `picked` but around the nodes in `avoids`: each avoided run replaced by a node of
    /// its own (100 + its first node), as another road would.
    fn game_route(picked: &[u64], avoids: &[u64]) -> Vec<u64> {
        let mut game = Vec::new();
        for &uid in picked {
            if !avoids.contains(&uid) {
                game.push(uid);
            } else if game.last().is_none_or(|&last| last < 100) {
                game.push(100 + uid);
            }
        }
        game
    }

    /// Plays a hand-over through with a game that avoids the nodes `avoids` gives (from the waypoints handed
    /// and how many plans it made so far): the steps taken.
    fn play(handover: &mut Handover, avoids: impl Fn(&[u64], usize) -> Vec<u64>) -> Vec<Step> {
        let mut steps = Vec::new();
        loop {
            let game = game_route(handover.picked(), &avoids(handover.handed(), steps.len()));
            let step = handover.read(&game);
            let done = matches!(step, Step::Follows | Step::GivesUp);
            steps.push(step);
            if done || steps.len() > 40 {
                return steps;
            }
        }
    }

    /// A game that avoids each of `stretches` unless a waypoint is in it.
    fn avoiding(stretches: &[&[u64]], handed: &[u64]) -> Vec<u64> {
        stretches.iter().filter(|stretch| !held(stretch, handed)).flat_map(|stretch| stretch.iter().copied()).collect()
    }

    fn held(stretch: &[u64], handed: &[u64]) -> bool {
        stretch.iter().any(|uid| handed.contains(uid))
    }

    const A: &[u64] = &[3, 4, 5];
    const B: &[u64] = &[8, 9, 10];
    const C: &[u64] = &[13, 14, 15];

    #[test]
    fn a_game_route_that_passes_every_node_follows() {
        // Points inside junctions (0) and the truck already past the start are fine.
        assert_eq!(route(6).read(&[2, 0, 3, 4, 0, 5, 6]), Step::Follows);
    }

    #[test]
    fn a_waypoint_goes_in_the_middle_of_the_stretch_the_game_avoids() {
        // The game goes 1 → 2 → 9 → 7 → 8: it avoids 3 to 6 (200–500 m), back on at 7 (600 m); the waypoint
        // goes at 5, the middle (400 m).
        let mut handover = route(8);
        assert_eq!(handover.read(&[1, 2, 9, 7, 8]), Step::Hold { parts: 2, via: 4 });
        assert_eq!(handover.handed(), [5]);
        // Into the destination: the stretch ends short of it.
        let mut handover = route(5);
        assert_eq!(handover.read(&[1, 2, 9, 5]), Step::Hold { parts: 2, via: 3 });
        assert_eq!(handover.handed(), [4]);
    }

    #[test]
    fn the_middle_is_by_distance_not_by_nodes() {
        // The stretch avoided, 2 to 5 (100 m, 200 m, 1.1 km, 1.2 km; back on at 6, 1.3 km): its middle
        // (700 m) is nearest 4.
        let positions = [[0.0, 0.0], [100.0, 0.0], [200.0, 0.0], [1100.0, 0.0], [1200.0, 0.0], [1300.0, 0.0]];
        let mut handover = Handover::new((1..=6).collect(), vec![Hold::Road; 6], &positions);
        assert_eq!(handover.read(&[1, 9, 6]), Step::Hold { parts: 1, via: 3 });
    }

    #[test]
    fn a_waypoint_goes_only_on_a_road_piece() {
        use Hold::{No, Road};
        let positions: Vec<[f32; 2]> = (0..8).map(|at| [at as f32 * 100.0, 0.0]).collect();
        // The game avoids 3 to 6, back on at 7: of the stretch only 4 is a road piece (3, 5 and 6 a junction's
        // edges and inside): the waypoint goes there, though 5 is nearer the middle.
        let holds = [Road, Road, No, Road, No, No, Road];
        let mut handover = Handover::new((1..=7).collect(), holds.to_vec(), &positions[..7]);
        assert_eq!(handover.read(&[1, 2, 9, 7]), Step::Hold { parts: 3, via: 3 });
        assert_eq!(handover.handed(), [4]);
        // Its road piece already handed and still avoided (the game reaches the waypoint without passing it):
        // the next road piece after the stretch, never anything else.
        let holds = [Road, Road, No, Road, No, No, Road, Road];
        let mut handover = Handover::new((1..=8).collect(), holds.to_vec(), &positions);
        handover.handed = vec![4];
        assert_eq!(handover.read(&[1, 2, 9, 7, 8]), Step::Hold { parts: 3, via: 6 });
        assert_eq!(handover.handed(), [4, 7]);
        // None near: the game takes its own way, nothing handed.
        let holds = [Road, Road, No, Road, No, No, No];
        let mut handover = Handover::new((1..=7).collect(), holds.to_vec(), &positions[..7]);
        handover.handed = vec![4];
        assert_eq!(handover.read(&[1, 2, 9, 7]), Step::GivesUp);
    }

    #[test]
    fn the_games_own_lane_where_the_route_runs_on_no_road_piece_is_followed() {
        use Hold::{No, Road};
        // A border crossing (owner 2026-10-22): the route picked goes 2 → 3…6 (invisible lanes, no road
        // pieces) → 7; the game takes its own lanes 20, 21, 22 there, and the same roads either side.
        let positions: Vec<[f32; 2]> = (0..8).map(|at| [at as f32 * 100.0, 0.0]).collect();
        let holds = [Road, Road, No, No, No, No, Road, Road];
        let mut handover = Handover::new((1..=8).collect(), holds.to_vec(), &positions);
        assert_eq!(handover.read(&[1, 2, 20, 21, 0, 22, 7, 8]), Step::Follows);
        assert!(handover.handed().is_empty());
        // Another way through a junction (only its inside avoided): the same.
        let holds = [Road, Road, No, Road, Road];
        let mut handover = Handover::new((1..=5).collect(), holds.to_vec(), &positions[..5]);
        assert_eq!(handover.read(&[1, 2, 0, 4, 5]), Step::Follows);
    }

    #[test]
    fn one_waypoint_that_holds_the_game_is_kept_without_trying() {
        let mut handover = route(10);
        let steps = play(&mut handover, |handed, _| avoiding(&[&[4, 5, 6]], handed));
        assert_eq!(steps, [Step::Hold { parts: 3, via: 4 }, Step::Follows]);
        assert_eq!(handover.handed(), [5]);
    }

    #[test]
    fn a_waypoint_tried_out_and_needed_is_put_back() {
        // Two stretches, each needing its own waypoint: each is tried out, the last one too (the other one
        // might have held the game without it), and they're put back.
        let mut handover = route(12);
        let steps = play(&mut handover, |handed, _| avoiding(&[A, B], handed));
        assert_eq!(
            steps,
            [
                Step::Hold { parts: 2, via: 3 },
                Step::Hold { parts: 7, via: 8 },
                Step::TryWithout(4),
                Step::TryWithout(9),
                Step::PutBack,
                Step::Follows,
            ]
        );
        assert_eq!(handover.handed(), [4, 9]);
    }

    #[test]
    fn a_later_waypoint_can_make_an_earlier_one_unneeded() {
        // The game avoids A unless held in A or in C (the way to C's waypoint goes through A), and C unless
        // held in it: held at 4, then at 14, it follows without 4.
        let mut handover = route(20);
        let steps = play(&mut handover, |handed, _| {
            let mut avoids = avoiding(&[C], handed);
            if !held(A, handed) && !held(C, handed) {
                avoids.extend(A);
            }
            avoids
        });
        assert_eq!(
            steps,
            [Step::Hold { parts: 2, via: 3 }, Step::Hold { parts: 12, via: 13 }, Step::TryWithout(4), Step::Follows]
        );
        assert_eq!(handover.handed(), [14]);
    }

    #[test]
    fn a_waypoint_not_needed_is_taken_out_and_the_needed_ones_stay() {
        // The game avoids A unless held in it, B only on its first two plans: 4 is tried out (needed), 9 too
        // (not needed).
        let mut handover = route(12);
        let steps = play(&mut handover, |handed, plans| {
            let mut avoids = avoiding(&[A], handed);
            if plans < 2 && !held(B, handed) {
                avoids.extend(B);
            }
            avoids
        });
        assert_eq!(
            steps,
            [
                Step::Hold { parts: 2, via: 3 },
                Step::Hold { parts: 7, via: 8 },
                Step::TryWithout(4),
                Step::TryWithout(9),
                Step::Follows,
            ]
        );
        assert_eq!(handover.handed(), [4]);
    }

    #[test]
    fn gives_up_after_the_most_waypoints() {
        // The game avoids each of one more stretch than the most waypoints unless held in it.
        let stretches: Vec<Vec<u64>> =
            (0..MOST_VIAS as u64 + 1).map(|at| vec![4 * at + 3, 4 * at + 4, 4 * at + 5]).collect();
        let stretches: Vec<&[u64]> = stretches.iter().map(|stretch| stretch.as_slice()).collect();
        let mut handover = route(50);
        let steps = play(&mut handover, |handed, _| avoiding(&stretches, handed));
        assert_eq!(steps.len(), MOST_VIAS + 1);
        assert!(steps[..MOST_VIAS].iter().all(|step| matches!(step, Step::Hold { .. })));
        assert_eq!(steps.last(), Some(&Step::GivesUp));
        assert_eq!(handover.handed().len(), MOST_VIAS);
        // As many as the most waypoints: held by them all, it follows (each tried out and put back).
        let mut handover = route(50);
        let steps = play(&mut handover, |handed, _| avoiding(&stretches[..MOST_VIAS], handed));
        assert_eq!(steps.last(), Some(&Step::Follows));
        assert_eq!(handover.handed().len(), MOST_VIAS);
    }

    #[test]
    fn waypoints_are_handed_in_the_order_the_route_reaches_them() {
        let mut handover = route(20);
        // The game parts late first, then early once held there.
        handover.read(&game_route(handover.picked(), C));
        assert_eq!(handover.handed(), [14]);
        handover.read(&game_route(handover.picked(), A));
        assert_eq!(handover.handed(), [4, 14]);
    }
}
