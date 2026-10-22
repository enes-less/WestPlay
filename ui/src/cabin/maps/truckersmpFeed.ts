/**
 * TruckersMP's map feed, read (truckersmp.ts follows it): each entry only as far as it's kept, the player's
 * own truck found by where theirs is, the other trucks' movement from two reads (for a route's traffic), and
 * their last places (the map draws each between them, smoothly: owner 2026-10-21).
 */

/** A truck of another player, as kept: where it is (game x, z), its heading (radians, the feed's), how it
 * moves (m/s along x and z, and its speed, from two reads; `null` until there are two) and when the feed last
 * placed it (s, the feed's clock). */
export interface OtherPlayer {
  id: number
  x: number
  z: number
  heading: number
  vx: number
  vz: number
  speed: number | null
  time: number
}

/** A part of the map, in the game's x and z (m): west, north (smaller z), east, south. */
export type Area = [x0: number, z0: number, x1: number, z1: number]

/** The square `metres` either way of a point. */
export function around(point: { x: number; z: number }, metres: number): Area {
  return [point.x - metres, point.z - metres, point.x + metres, point.z + metres]
}

/** An area as the feed asks it: `x1`, `y1` its top left (the feed's y is the game's z, `y1` the larger),
 * `x2`, `y2` its bottom right, whole metres. */
export function areaQuery([x0, z0, x1, z1]: Area): string {
  const at = (value: number) => Math.round(value)
  return `x1=${at(x0)}&y1=${at(z1)}&x2=${at(x1)}&y2=${at(z0)}`
}

/** Where the feed placed a truck, and when (s, the feed's clock). */
export interface Sample {
  t: number
  x: number
  z: number
}

/** How many of a truck's last places are kept: enough to draw it a few seconds behind. */
export const TRACK_SAMPLES = 6

/**
 * Each other truck's last places, oldest first, with a read's added (`self` left out, and trucks the feed
 * stopped placing). A truck not in the read (out of the part read, or gone) is dropped; one the feed hasn't
 * moved on since keeps its places as they were.
 */
export function withRead(
  tracks: ReadonlyMap<number, readonly Sample[]>,
  read: readonly FeedPlayer[],
  self: number | null,
): Map<number, Sample[]> {
  const newest = read.reduce((latest, player) => Math.max(latest, player.time), -Infinity)
  const next = new Map<number, Sample[]>()
  for (const player of read) {
    if (player.id === self || newest - player.time > STALE_AFTER_S) continue
    const before = tracks.get(player.id) ?? []
    const last = before.at(-1)
    next.set(
      player.id,
      last && last.t >= player.time
        ? [...before]
        : [...before, { t: player.time, x: player.x, z: player.z }].slice(-TRACK_SAMPLES),
    )
  }
  return next
}

/** Where a truck was at `t` (the feed's clock), between the places the feed gave it: before the first, at
 * the first; past the last, at the last (it waits for the next, never driving on by itself). */
export function trackAt(samples: readonly Sample[], t: number): [x: number, z: number] {
  const first = samples[0]
  if (t <= first.t) return [first.x, first.z]
  for (let i = 1; i < samples.length; i++) {
    const [a, b] = [samples[i - 1], samples[i]]
    if (t <= b.t) {
      const k = (t - a.t) / (b.t - a.t)
      return [a.x + (b.x - a.x) * k, a.z + (b.z - a.z) * k]
    }
  }
  const last = samples[samples.length - 1]
  return [last.x, last.z]
}

/** How far (s) the feed's clock is from the page's, kept steady: each read's measure (`measured`, the read's
 * newest time less when it came, in seconds) moves it a little, a jump of more than a few seconds at once
 * (none known yet, or the feed's clock reset). The feed's times are whole seconds: one read's measure is off
 * by up to one. */
export function feedClock(known: number | null, measured: number): number {
  if (known === null || Math.abs(measured - known) > 5) return measured
  return known + (measured - known) * 0.1
}

/** A feed entry, only what's kept of it. */
export interface FeedPlayer {
  id: number
  x: number
  z: number
  heading: number
  time: number
}

/** How near (m) a feed truck must be to the player's to be theirs (the feed lags the game by a few seconds). */
export const SELF_WITHIN = 150
/** Feed trucks this much older (s) than its newest are gone (left the server, or stopped being tracked). */
const STALE_AFTER_S = 30

/** The feed's answer, only its well-formed entries and only what's kept of each. */
export function readFeed(answer: unknown): FeedPlayer[] {
  const data = (answer as { Data?: unknown } | null)?.Data
  if (!Array.isArray(data)) return []
  const players: FeedPlayer[] = []
  for (const entry of data) {
    const { X, Y, Heading, PlayerId, Time } = (entry ?? {}) as Record<string, unknown>
    if ([X, Y, Heading, PlayerId, Time].every((value) => typeof value === 'number' && Number.isFinite(value))) {
      players.push({
        id: PlayerId as number,
        x: X as number,
        z: Y as number,
        heading: Heading as number,
        time: Time as number,
      })
    }
  }
  return players
}

/** The feed truck nearest the player's own (game x, z), if within [`SELF_WITHIN`]. */
export function findSelf(players: readonly FeedPlayer[], truck: { x: number; z: number }): FeedPlayer | null {
  let best: FeedPlayer | null = null
  let bestDistance = SELF_WITHIN
  for (const player of players) {
    const distance = Math.hypot(player.x - truck.x, player.z - truck.z)
    if (distance <= bestDistance) {
      best = player
      bestDistance = distance
    }
  }
  return best
}

/** The other players from a read (`self` left out, and trucks the feed stopped placing), each with its speed
 * from where the last read had it (kept as it was if the feed hasn't moved it on since). */
export function otherPlayers(
  read: readonly FeedPlayer[],
  self: number | null,
  before: ReadonlyMap<number, OtherPlayer>,
): OtherPlayer[] {
  const newest = read.reduce((latest, player) => Math.max(latest, player.time), -Infinity)
  return read
    .filter((player) => player.id !== self && newest - player.time <= STALE_AFTER_S)
    .map((player) => {
      const last = before.get(player.id)
      const seconds = last ? player.time - last.time : 0
      // Moved on since: its movement from the two; else as it was.
      if (last && seconds > 0) {
        const [vx, vz] = [(player.x - last.x) / seconds, (player.z - last.z) / seconds]
        return { ...player, vx, vz, speed: Math.hypot(vx, vz) }
      }
      return { ...player, vx: last?.vx ?? 0, vz: last?.vz ?? 0, speed: last?.speed ?? null }
    })
}
