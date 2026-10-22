import type { Destination, DestinationKind } from './places'
import { fromLngLat } from './projection'
import type { Point, Progress, Spans } from './route'

/**
 * Stops along the route ahead (PLAN.md: Navigation plan, A): on a route, Maps offers only places beside the
 * rest of it, never behind the truck, nearest along the route first. A place counts when it's within its
 * kind's reach of the route still to drive: filling stations and rest areas are beside the road, companies
 * and dealers in the towns it passes.
 */

/** How far off the route a place of each kind may be (metres of the game's ground). */
export const REACH: Record<DestinationKind, number> = {
  fuel: 1000,
  service: 1000,
  parking: 1000,
  hotel: 1000,
  weigh: 1000,
  recruitment: 2000,
  garage: 3000,
  dealer: 3000,
  company: 3000,
  ferry: 3000,
  train: 3000,
  city: 5000,
}

/** A place along the route: how far along it from the truck, and how far off it (metres). */
export interface Ahead {
  place: Destination
  along: number
  off: number
}

/** The route still to drive, from the truck: its points, and how far along each one is. */
interface Rest {
  points: Point[]
  along: number[]
  /** The segments by the grid cell their middle is in. */
  cells: Map<string, number[]>
}

/** The side of a grid cell (metres): a segment is looked for in the cells within a place's reach. */
const CELL = 1000

/** The route from where the truck is on it (`progress`) to its end. */
function restOf(spans: Spans, progress: Progress): Rest {
  const points: Point[] = [progress.at]
  const first = spans[progress.span] ?? []
  points.push(...first.slice(progress.index + 1))
  for (const span of spans.slice(progress.span + 1)) points.push(...span.slice(1))
  const along = [0]
  const cells = new Map<string, number[]>()
  for (let i = 1; i < points.length; i++) {
    const [a, b] = [points[i - 1], points[i]]
    along.push(along[i - 1] + Math.hypot(b[0] - a[0], b[1] - a[1]))
    const key = cellOf([(a[0] + b[0]) / 2, (a[1] + b[1]) / 2])
    const list = cells.get(key)
    if (list) list.push(i - 1)
    else cells.set(key, [i - 1])
  }
  return { points, along, cells }
}

function cellOf([x, z]: Point): string {
  return `${Math.floor(x / CELL)},${Math.floor(z / CELL)}`
}

/** Where `point` is nearest the rest of the route, within `reach`: how far along, how far off. */
function nearest(rest: Rest, point: Point, reach: number): { along: number; off: number } | null {
  // Segments are short (a few metres), so one in reach has its middle within a cell more than `reach`.
  const span = Math.ceil(reach / CELL) + 1
  const [cx, cz] = [Math.floor(point[0] / CELL), Math.floor(point[1] / CELL)]
  let best: { along: number; off: number } | null = null
  for (let dx = -span; dx <= span; dx++) {
    for (let dz = -span; dz <= span; dz++) {
      for (const i of rest.cells.get(`${cx + dx},${cz + dz}`) ?? []) {
        const [a, b] = [rest.points[i], rest.points[i + 1]]
        const [ex, ez] = [b[0] - a[0], b[1] - a[1]]
        const length = ex * ex + ez * ez
        const t = length > 0 ? Math.max(0, Math.min(1, ((point[0] - a[0]) * ex + (point[1] - a[1]) * ez) / length)) : 0
        const off = Math.hypot(a[0] + ex * t - point[0], a[1] + ez * t - point[1])
        if (off <= reach && (!best || off < best.off)) {
          best = { along: rest.along[i] + Math.sqrt(length) * t, off }
        }
      }
    }
  }
  return best
}

/**
 * Of `places`, those along the route ahead of the truck (`spans`, the truck at `progress`), each within
 * its kind's reach, nearest along the route first. A place the route has reached already (right at the
 * truck) isn't one to stop at.
 */
export function placesAhead(places: readonly Destination[], spans: Spans, progress: Progress): Ahead[] {
  const rest = restOf(spans, progress)
  const far = Math.max(...Object.values(REACH))
  // (A loop: a long route has too many points to spread into Math.min.)
  let [x0, x1, z0, z1] = [Infinity, -Infinity, Infinity, -Infinity]
  for (const [x, z] of rest.points) {
    ;[x0, x1, z0, z1] = [Math.min(x0, x - far), Math.max(x1, x + far), Math.min(z0, z - far), Math.max(z1, z + far)]
  }
  const found: Ahead[] = []
  for (const place of places) {
    const point = fromLngLat(...place.at)
    if (point[0] < x0 || point[0] > x1 || point[1] < z0 || point[1] > z1) continue
    const at = nearest(rest, point, REACH[place.kind])
    if (at && at.along > 1) found.push({ place, ...at })
  }
  return found.sort((a, b) => a.along - b.along)
}
