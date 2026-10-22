import type { Point } from './route'
import type { Area, OtherPlayer } from './truckersmpFeed'

/**
 * Traffic along a route, from TruckersMP's other players (truckersmp.ts; owner 2026-10-21, as YMN One shows
 * it): the stretches where trucks heading the route's way are bunched up and crawl turn yellow (slow) or red
 * (jammed). The route is cut into short stretches; each truck on the route's road, heading its way, counts
 * for the stretch it's on; a stretch with enough of them is slow or jammed by how fast they go, the middle of
 * their speeds (a parked truck or two among moving ones change nothing). A first guess, to tune on the road.
 */

export type Jam = 'slow' | 'jammed'

/** A stretch of the route with traffic: its line and how bad it is. */
export interface TrafficStretch {
  line: Point[]
  level: Jam
}

/** How long (m) each stretch of the route is. */
export const STRETCH = 200
/** How far (m) from the route's line a truck can be and still be on its road (its lanes either way). */
export const ON_ROAD = 12
/** Trucks heading within this angle of the route's way (cos) are going its way. */
const SAME_WAY = Math.cos(Math.PI / 3)
/** Trucks a stretch needs before its traffic counts. */
export const ENOUGH_TRUCKS = 3
/** The middle speed (m/s) below which a stretch is slow (50 km/h), and jammed (20 km/h): a little readier than
 * the first guess (45 and 15), red seen only once on the road (owner 2026-10-21). */
export const SLOW_BELOW = 50 / 3.6
export const JAMMED_BELOW = 20 / 3.6

/** The corridor of a route's line ([x, z] m): its extent, with room for its road either side (`null`: no line).
 * Its traffic is read from the trucks in it. */
export function corridorOf(line: readonly Point[]): Area | null {
  if (line.length < 2) return null
  let [x0, x1, z0, z1] = [Infinity, -Infinity, Infinity, -Infinity]
  for (const [x, z] of line) [x0, x1, z0, z1] = [Math.min(x0, x), Math.max(x1, x), Math.min(z0, z), Math.max(z1, z)]
  return [x0 - ON_ROAD, z0 - ON_ROAD, x1 + ON_ROAD, z1 + ON_ROAD]
}

/** The way a truck with the feed's heading (radians, the game's: 0 north, which is −z, turning west, −x)
 * faces, as a unit [x, z]. */
function facing(heading: number): Point {
  return [-Math.sin(heading), -Math.cos(heading)]
}

/** The stretches of `line` ([x, z] m) with traffic, from `players` (those with a speed). */
export function trafficAlong(line: readonly Point[], players: readonly OtherPlayer[]): TrafficStretch[] {
  if (line.length < 2) return []
  // How far along the line each of its points is.
  const along = [0]
  for (let i = 1; i < line.length; i++) {
    along.push(along[i - 1] + Math.hypot(line[i][0] - line[i - 1][0], line[i][1] - line[i - 1][1]))
  }
  const length = along[along.length - 1]
  const stretches = Math.max(1, Math.ceil(length / STRETCH))
  const speeds: number[][] = Array.from({ length: stretches }, () => [])

  // Trucks well away from the whole route are left out at once.
  let [x0, x1, z0, z1] = [Infinity, -Infinity, Infinity, -Infinity]
  for (const [x, z] of line) [x0, x1, z0, z1] = [Math.min(x0, x), Math.max(x1, x), Math.min(z0, z), Math.max(z1, z)]
  for (const player of players) {
    if (player.speed === null) continue
    if (player.x < x0 - ON_ROAD || player.x > x1 + ON_ROAD || player.z < z0 - ON_ROAD || player.z > z1 + ON_ROAD)
      continue
    const at = onRoute(line, along, player)
    if (at !== null) speeds[Math.min(stretches - 1, Math.floor(at / STRETCH))].push(player.speed)
  }

  const levels = speeds.map((stretch): Jam | null => {
    if (stretch.length < ENOUGH_TRUCKS) return null
    const middle = median(stretch)
    return middle < JAMMED_BELOW ? 'jammed' : middle < SLOW_BELOW ? 'slow' : null
  })
  // Neighbouring stretches alike, one line.
  const found: TrafficStretch[] = []
  for (let i = 0; i < stretches; i++) {
    const level = levels[i]
    if (!level) continue
    let end = i
    while (end + 1 < stretches && levels[end + 1] === level) end++
    found.push({ line: cut(line, along, i * STRETCH, Math.min(length, (end + 1) * STRETCH)), level })
    i = end
  }
  return found
}

/** How far along the line `player` is, if on its road and heading its way; `null` if not. */
function onRoute(line: readonly Point[], along: readonly number[], player: OtherPlayer): number | null {
  const [fx, fz] = facing(player.heading)
  let best: number | null = null
  let bestDistance = ON_ROAD
  for (let i = 1; i < line.length; i++) {
    const [ax, az] = line[i - 1]
    const [dx, dz] = [line[i][0] - ax, line[i][1] - az]
    const segment = Math.hypot(dx, dz)
    if (segment === 0) continue
    const t = Math.max(0, Math.min(1, ((player.x - ax) * dx + (player.z - az) * dz) / (segment * segment)))
    const distance = Math.hypot(player.x - (ax + dx * t), player.z - (az + dz * t))
    if (distance > bestDistance) continue
    if ((fx * dx + fz * dz) / segment < SAME_WAY) continue
    best = along[i - 1] + segment * t
    bestDistance = distance
  }
  return best
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b)
  const middle = sorted.length >> 1
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2
}

/** The part of the line from `from` to `to` (m along it). */
function cut(line: readonly Point[], along: readonly number[], from: number, to: number): Point[] {
  const pointAt = (distance: number): Point => {
    let i = 1
    while (i < line.length - 1 && along[i] < distance) i++
    const span = along[i] - along[i - 1]
    const t = span > 0 ? (distance - along[i - 1]) / span : 0
    return [line[i - 1][0] + (line[i][0] - line[i - 1][0]) * t, line[i - 1][1] + (line[i][1] - line[i - 1][1]) * t]
  }
  const points: Point[] = [pointAt(from)]
  for (let i = 0; i < line.length; i++) if (along[i] > from && along[i] < to) points.push(line[i])
  points.push(pointAt(to))
  return points
}
