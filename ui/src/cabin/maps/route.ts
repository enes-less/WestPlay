/**
 * The game's GPS route as a line on the map. The host reads the route the game plans (its map nodes in
 * order, every node of every road on the way: crates/host route.rs) and the page draws it along the
 * roads: between two nodes the game's own road curve (a cubic Hermite curve leaving and arriving along
 * the nodes' directions, tangents as long as the distance between them, as crates/mapdata geometry.rs
 * draws the roads), so the line lies on the road it follows. Through a junction (from one of its ends to
 * another) it takes the junction's own lanes, round a roundabout the way traffic goes, once the host has
 * found them in the game's files (crates/mapdata junctions.rs); until then the same curve stands in. The
 * game's route runs on its roads' lines, which on a road drawn as two carriageways (a motorway) is the
 * central reservation: the host says how far across each node to run (on the carriageway driven on), and
 * the line runs there, easing between nodes.
 *
 * The part already driven isn't drawn: the line starts where the truck is on the road, as on Apple's
 * (and, until the truck reaches the node the game plans from, from the truck to it: approachLine).
 * And on the route, the truck is shown on it (snapToRoute): in the middle of the road whatever lane it's
 * in, as real navigation does.
 */

/** A point on the game's ground, metres (x east, z south). */
export type Point = [x: number, z: number]

/** A map node of the route: where it is and its direction (either way along the road). */
export interface RouteNode {
  position: Point
  forward: Point
}

/** The route between consecutive nodes: `spans[i]` runs from node `i` to node `i + 1`, as points. */
export type Spans = Point[][]

/**
 * The route's spans: the ways through junctions where `paths` has them (by the node each starts at, already
 * ending where the route runs), the road curve elsewhere, moved `shifts` metres to the right of the way
 * the route goes at each of its nodes (easing between them). A node's direction is the road's, which may
 * point either way: each is turned to face along the route (the way from the node before it to the node
 * after it).
 */
export function routeSpans(
  nodes: readonly RouteNode[],
  paths: ReadonlyMap<number, Point[]> = new Map(),
  shifts: ReadonlyMap<number, number> = new Map(),
): Spans {
  const along = nodes.map((node, i) => {
    const before = nodes[i - 1]?.position
    const after = nodes[i + 1]?.position
    const travel = add(
      before ? unit(sub(node.position, before)) : [0, 0],
      after ? unit(sub(after, node.position)) : [0, 0],
    )
    return dot(node.forward, travel) < 0 ? scale(node.forward, -1) : node.forward
  })
  return nodes.slice(1).map((node, i) => {
    const path = paths.get(i)
    if (path) return path
    const curve = roadCurve(nodes[i].position, along[i], node.position, along[i + 1])
    const [from, to] = [shifts.get(i) ?? 0, shifts.get(i + 1) ?? 0]
    return from === 0 && to === 0 ? curve : offsetLine(curve, from, to)
  })
}

/** `points` moved to the right of the way they run (x east, z south), `from` metres at the first easing to
 * `to` at the last along them, each across the line there (along the bisector of its two segments). */
export function offsetLine(points: Point[], from: number, to: number): Point[] {
  const lengths = [0]
  for (let i = 1; i < points.length; i++) lengths.push(lengths[i - 1] + Math.hypot(...sub(points[i], points[i - 1])))
  const total = lengths[lengths.length - 1]
  return points.map((point, i) => {
    const before = i > 0 ? unit(sub(point, points[i - 1])) : null
    const after = i + 1 < points.length ? unit(sub(points[i + 1], point)) : null
    const way = unit(add(before ?? [0, 0], after ?? [0, 0]))
    const shift = total > 0 ? from + ((to - from) * lengths[i]) / total : from
    return [point[0] - way[1] * shift, point[1] + way[0] * shift]
  })
}

/**
 * The game's road curve from `p0` (leaving along `f0`) to `p1` (arriving along `f1`): about a point
 * every 8 m, as the map's roads (crates/mapdata geometry.rs road_curve).
 */
export function roadCurve(p0: Point, f0: Point, p1: Point, f1: Point): Point[] {
  const length = Math.hypot(p1[0] - p0[0], p1[1] - p0[1])
  const [m0, m1] = [scale(f0, length), scale(f1, length)]
  const segments = Math.min(64, Math.max(1, Math.ceil(length / 8)))
  const points: Point[] = []
  for (let i = 0; i <= segments; i++) {
    const s = i / segments
    const [s2, s3] = [s * s, s * s * s]
    const [h00, h10, h01, h11] = [2 * s3 - 3 * s2 + 1, s3 - 2 * s2 + s, -2 * s3 + 3 * s2, s3 - s2]
    points.push([
      h00 * p0[0] + h10 * m0[0] + h01 * p1[0] + h11 * m1[0],
      h00 * p0[1] + h10 * m0[1] + h01 * p1[1] + h11 * m1[1],
    ])
  }
  return points
}

/** Where the truck is along the route: on span `span`, at `at`, before its point `index + 1`, `distance`
 * metres from the truck. */
export interface Progress {
  span: number
  index: number
  at: Point
  distance: number
}

/** How many spans past the host's next node the truck is looked for: it reports every half second, and
 * the truck may pass a few short spans meanwhile. */
const SPANS_AHEAD = 3

/** A part of the route heading more than this far from the way the truck faces (the cosine: about 120°)
 * runs the other way: where the route comes back along the same road (a U-turn round a roundabout), its
 * way back lies right on its way there, and only the way the truck faces tells them apart. */
const AGAINST = -0.5

/**
 * Where the truck at `truck`, facing `facing` (a unit direction), is along the route: the nearest point of
 * the spans around `next` (the node ahead of it, as the host last reported) running its way (any, if none
 * does: say it has turned round off the route).
 */
export function progressOf(spans: Spans, next: number, truck: Point, facing?: Point): Progress | null {
  if (spans.length === 0) return null
  const first = Math.min(Math.max(next - 1, 0), spans.length - 1)
  const last = Math.min(first + SPANS_AHEAD, spans.length - 1)
  let along: Progress | null = null
  let any: Progress | null = null
  for (let span = first; span <= last; span++) {
    const points = spans[span]
    for (let index = 0; index < points.length - 1; index++) {
      const [a, b] = [points[index], points[index + 1]]
      const at = closestOnSegment(a, b, truck)
      const distance = Math.hypot(at[0] - truck[0], at[1] - truck[1])
      const candidate = { span, index, at, distance }
      // A tie goes to the later point: the truck is past a node it stands on.
      if (!any || distance <= any.distance) any = candidate
      const against = facing !== undefined && dot(unit(sub(b, a)), facing) < AGAINST
      if (!against && (!along || distance <= along.distance)) along = candidate
    }
  }
  return along ?? any
}

/** Within this distance of the route (metres), the truck is shown on it; beyond `SNAP_NONE`, where it
 * is; easing between the two, so leaving the route (or joining it) never jumps. A lane of a wide road is
 * well within it. */
const SNAP_FULL = 10
const SNAP_NONE = 18

/**
 * Where to show the truck at `truck` while it's on the route: in the middle of the road. (Facing its own
 * way: the truck's heading is smooth where the drawn line bends at a junction or at its points.)
 */
export function snapToRoute(progress: Progress, truck: Point): Point {
  const weight = Math.min(1, Math.max(0, (SNAP_NONE - progress.distance) / (SNAP_NONE - SNAP_FULL)))
  return [truck[0] + (progress.at[0] - truck[0]) * weight, truck[1] + (progress.at[1] - truck[1]) * weight]
}

/** The route ahead of the truck to the end of the span it's on. */
export function headLine(spans: Spans, progress: Progress): Point[] {
  return [progress.at, ...spans[progress.span].slice(progress.index + 1)]
}

/** How far short of the route's first node the truck must be for the line to run to it from the truck,
 * metres (closer, it's there). */
const APPROACH_MIN = 1

/**
 * The line from the truck at `truck`, facing `facing` (a unit direction), to the route's start, while it's
 * still short of it (`progress` at the very start): the game plans its route from the node ahead of the
 * truck, which may lie a long road away, off the screen, and Apple's line starts at the car. Along the
 * road the truck is on, as the game's road curve runs (leaving the way the truck faces, arriving along the
 * route). `null` once the truck is on the route, or where it isn't heading for its start (turned round).
 */
export function approachLine(spans: Spans, progress: Progress, truck: Point, facing: Point): Point[] | null {
  const first = spans[0]
  if (progress.span !== 0 || progress.index !== 0 || !first || first.length < 2) return null
  const start = first[0]
  const way = unit(sub(first[1], start))
  const ahead = sub(start, truck)
  if (dot(ahead, way) < APPROACH_MIN || dot(ahead, facing) <= 0 || dot(way, facing) <= 0) return null
  return roadCurve(truck, facing, start, way)
}

/** A point's height drawn over the flat map (raisedRoads.ts), metres: those of the route's on raised roads, as found. */
export type Heights = ReadonlyMap<Point, number>

/** The raised road at a point, of those within `reach` of the height `near` in the game. */
export type SurfaceAt = (x: number, z: number, near: number, reach: number) => { drawn: number; game: number } | null

/** How far off the height it's expected at the route can be on a raised road, m: less than a road under it is. */
const ON_SURFACE = 4
/** The most of the route between two points on raised roads that's raised with them, not being on one itself, m: a
 * point in the gap between two faces, off a road's edge in a tight curve. */
const SURFACE_GAP = 30

/**
 * How high the points of `spans` are drawn: on the raised road each is on. Nothing else says which of the roads at a
 * place that is, so each span goes from its first node's height (`game`: the nodes') keeping to the surface it's found,
 * and failing that along the way to the next node's.
 */
export function routeHeights(spans: Spans, game: readonly number[], surfaceAt: SurfaceAt): Heights {
  const heights = new Map<Point, number>()
  spans.forEach((points, span) => {
    const [from, to] = [game[span] ?? 0, game[span + 1] ?? game[span] ?? 0]
    let known: number | null = null
    points.forEach((point, index) => {
      const guess = known ?? from + ((to - from) * index) / Math.max(points.length - 1, 1)
      const found = surfaceAt(point[0], point[1], guess, ON_SURFACE)
      known = found?.game ?? null
      if (found) heights.set(point, found.drawn)
    })
  })
  const line = spans.flat()
  let last = -1
  let gap = 0
  line.forEach((point, index) => {
    if (index > 0) gap += Math.hypot(point[0] - line[index - 1][0], point[1] - line[index - 1][1])
    const height = heights.get(point)
    if (height === undefined) return
    if (last >= 0 && index > last + 1 && gap <= SURFACE_GAP) {
      const start = heights.get(line[last]) ?? 0
      for (let i = last + 1; i < index; i++) {
        heights.set(line[i], start + ((height - start) * (i - last)) / (index - last))
      }
    }
    last = index
    gap = 0
  })
  return heights
}

/** How high the truck's place on the route is drawn: between the points it's between. */
export function heightOnSpan(points: readonly Point[], progress: Progress, heights: Heights): number {
  const [a, b] = [points[progress.index], points[progress.index + 1]]
  const [from, to] = [heights.get(a) ?? 0, heights.get(b) ?? 0]
  const length = Math.hypot(b[0] - a[0], b[1] - a[1])
  const along = Math.hypot(progress.at[0] - a[0], progress.at[1] - a[1])
  return length > 0 ? from + ((to - from) * along) / length : from
}

/** A stretch of the line on raised roads: x, z and how high it's drawn. */
export type RaisedLine = [x: number, z: number, height: number][]

/** `line` cut into what lies on the flat map and what's over it, each ending at the point the next starts from. */
export function cutRaised(
  line: Point[],
  heightOf: (point: Point) => number,
): { flat: Point[][]; raised: RaisedLine[] } {
  const flat: Point[][] = []
  const raised: RaisedLine[] = []
  const high = (index: number) => heightOf(line[index]) > 0
  let index = 0
  while (index < line.length) {
    const start = index
    if (high(index)) {
      while (index < line.length && high(index)) index++
      const stretch = line.slice(Math.max(start - 1, 0), Math.min(index + 1, line.length))
      raised.push(stretch.map((point) => [point[0], point[1], heightOf(point)]))
    } else {
      while (index < line.length && !high(index)) index++
      if (index - start > 1) flat.push(line.slice(start, index))
    }
  }
  return { flat, raised }
}

/** The route after the span `span` to its end (the head is drawn on its own, as the truck moves). */
export function restLine(spans: Spans, span: number): Point[] {
  return spans.slice(span + 1).flatMap((points, i) => (i === 0 ? points : points.slice(1)))
}

/** The route from the span `span` on, as far as `metres` along it (the route ahead of a truck on that span). */
export function routeAhead(spans: Spans, span: number, metres: number): Point[] {
  const line = spans.slice(span).flatMap((points, i) => (i === 0 ? points : points.slice(1)))
  if (line.length < 2) return line
  const ahead: Point[] = [line[0]]
  let left = metres
  for (let i = 1; i < line.length && left > 0; i++) {
    const [a, b] = [line[i - 1], line[i]]
    const length = Math.hypot(b[0] - a[0], b[1] - a[1])
    if (length <= left) {
      ahead.push(b)
    } else {
      const t = left / length
      ahead.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
    }
    left -= length
  }
  return ahead
}

function closestOnSegment(a: Point, b: Point, p: Point): Point {
  const ab = sub(b, a)
  const length2 = dot(ab, ab)
  const t = length2 === 0 ? 0 : Math.min(1, Math.max(0, dot(sub(p, a), ab) / length2))
  return [a[0] + ab[0] * t, a[1] + ab[1] * t]
}

function add(a: Point, b: Point): Point {
  return [a[0] + b[0], a[1] + b[1]]
}

function sub(a: Point, b: Point): Point {
  return [a[0] - b[0], a[1] - b[1]]
}

function scale(a: Point, k: number): Point {
  return [a[0] * k, a[1] * k]
}

function dot(a: Point, b: Point): number {
  return a[0] * b[0] + a[1] * b[1]
}

function unit(a: Point): Point {
  const length = Math.hypot(a[0], a[1])
  return length === 0 ? [0, 0] : [a[0] / length, a[1] / length]
}
