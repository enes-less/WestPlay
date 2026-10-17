/**
 * Turn-by-turn: the route's next turn and how far it is. The host works out the turns at the route's
 * junctions from the game's own lanes (crates/mapdata junctions.rs: only where there's a choice, in the
 * terms of the game's voice navigation), and each route node carries the game's own distance left from it
 * to the destination; the SDK's distance left, less the node's, is how far the truck is from the turn, as
 * the game counts distances (its map's scale, so the same figures as its own GPS).
 *
 * The game's scale is about 3 to 1 in cities and 15-19 to 1 between them, so at a steady speed the
 * distance counts down several times faster out of town. That's deliberate: the
 * figures stay the game's own, matching its GPS, its jobs and the trip pill, rather than metres driven.
 */

export type TurnKind =
  | 'slight-left'
  | 'left'
  | 'sharp-left'
  | 'slight-right'
  | 'right'
  | 'sharp-right'
  | 'u-turn'
  | 'keep-left'
  | 'keep-right'
  | 'exit-left'
  | 'exit-right'
  | 'roundabout'

/** A turn of the route, at the junction entered at its node `node`: how far it turns (degrees, right
 * positive), and a roundabout's exit. */
export interface Maneuver {
  node: number
  turn: TurnKind
  angle: number
  exit: number | null
}

/** What to do next: a turn, or arrive. */
export type Step = { kind: 'turn'; maneuver: Maneuver } | { kind: 'arrive' }

export interface Guidance {
  step: Step
  /** How far to it, metres (as the game counts them). */
  distance: number
  /** What comes right after it, when it's that close behind (the game's "…and then"). */
  then: Step | null
}

/** A second step this close behind the first (metres, as the game counts them) is shown with it. */
export const THEN_WITHIN = 400

const ARRIVE: Step = { kind: 'arrive' }

/**
 * The guidance along a route (`distances`: each node's distance left to the destination; `maneuvers` in
 * route order) for a truck before its node `next`, with `distanceLeft` to go; `null` without a route. A
 * turn stays next until the truck is through its junction (it's "now" while inside).
 */
export function guidanceOf(
  distances: readonly number[],
  maneuvers: readonly Maneuver[],
  next: number,
  distanceLeft: number,
): Guidance | null {
  if (distanceLeft <= 0 || distances.length === 0) return null
  const ahead = maneuvers.filter((maneuver) => maneuver.node >= next - 1)
  const to = (maneuver: Maneuver) => Math.max(0, distanceLeft - (distances[maneuver.node] ?? 0))
  const [first, second] = ahead
  if (!first) return { step: ARRIVE, distance: distanceLeft, then: null }
  const distance = to(first)
  const after: [Step, number] = second ? [{ kind: 'turn', maneuver: second }, to(second)] : [ARRIVE, distanceLeft]
  return {
    step: { kind: 'turn', maneuver: first },
    distance,
    then: after[1] - distance <= THEN_WITHIN ? after[0] : null,
  }
}

/** Within this, metres, a step is "now". */
export const NOW_WITHIN = 30
