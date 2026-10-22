/**
 * The next of the player's stops along the game's route (owner 2026-10-21: the time to the next stop only, and
 * once it's reached, to the one after): the first stop still ahead of the truck, how far it is and how long
 * it takes, in the game's own figures (its map's scale, its time scale), as its arrival pill counts them.
 *
 * The game gives a time only for the whole route; the stop's is its share of it, by distance (the stretch to
 * it as fast as the rest).
 */

export interface NextStop {
  /** Metres to it, as the game counts them. */
  distance: number
  /** Minutes to it, on the game's clock. */
  minutes: number
}

/**
 * `stops`: the stops' map node indices in the route (ascending); `distances`: each node's distance left to the
 * destination; `next`: the index of the map node ahead of the truck; `left` and `seconds`: the distance and time
 * left to the destination (the SDK's). `null` with no stop ahead.
 */
export function nextStop(
  stops: readonly number[],
  distances: readonly number[],
  next: number,
  left: number,
  seconds: number,
): NextStop | null {
  if (left <= 0) return null
  for (const stop of stops) {
    if (stop < next) continue
    const distance = left - (distances[stop] ?? left)
    if (distance <= 0) continue
    return { distance, minutes: (seconds / 60) * (distance / left) }
  }
  return null
}
