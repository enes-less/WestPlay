import { describe, expect, it } from 'vitest'
import {
  approachLine,
  headLine,
  progressOf,
  restLine,
  type RaisedRoad,
  routeAhead,
  roadCurve,
  routeLevels,
  type RouteNode,
  routeSpans,
  snapToRoute,
} from './route'

const node = (x: number, z: number, fx: number, fz: number): RouteNode => ({ position: [x, z], forward: [fx, fz] })

describe('roadCurve', () => {
  it('runs straight between nodes facing each other along the road, a point every 8 m', () => {
    const points = roadCurve([0, 0], [1, 0], [80, 0], [1, 0])
    expect(points).toHaveLength(11)
    expect(points[0]).toEqual([0, 0])
    expect(points[10]).toEqual([80, 0])
    for (const [, z] of points) expect(z).toBeCloseTo(0)
  })

  it('bends out along the directions it leaves and arrives in (the game road curve)', () => {
    // A quarter turn: east, then south.
    const points = roadCurve([0, 0], [1, 0], [50, 50], [0, 1])
    const middle = points[Math.floor(points.length / 2)]
    // Bulging outwards (past the chord, towards the corner at [50, 0]).
    expect(middle[0]).toBeGreaterThan(middle[1])
  })
})

describe('routeSpans', () => {
  it('turns a direction pointing back down the route to face along it', () => {
    // The middle node's road was laid the other way.
    const spans = routeSpans([node(0, 0, 1, 0), node(100, 0, -1, 0), node(200, 0, 1, 0)])
    expect(spans).toHaveLength(2)
    // Facing along, the line stays on the straight road (facing back, it would loop).
    for (const span of spans)
      for (const [x, z] of span) {
        expect(z).toBeCloseTo(0)
        expect(x).toBeGreaterThanOrEqual(-1e-9)
        expect(x).toBeLessThanOrEqual(200)
      }
  })

  it('runs on the carriageway driven on: moved right of its way where the host says', () => {
    // A motorway heading east (right is +z), its line the central reservation; the route 6.5 m right.
    const shifts = new Map([
      [0, 6.5],
      [1, 6.5],
    ])
    const [span] = routeSpans([node(0, 0, 1, 0), node(100, 0, 1, 0)], new Map(), shifts)
    for (const [, z] of span) expect(z).toBeCloseTo(6.5)
    // Going the other way (west, right is -z): the node directions are turned along the route.
    const [back] = routeSpans([node(100, 0, 1, 0), node(0, 0, 1, 0)], new Map(), shifts)
    for (const [, z] of back) expect(z).toBeCloseTo(-6.5)
    // Easing from one node's to the next's.
    const [easing] = routeSpans([node(0, 0, 1, 0), node(80, 0, 1, 0)], new Map(), new Map([[1, 4]]))
    expect(easing[0][1]).toBeCloseTo(0)
    expect(easing[easing.length - 1][1]).toBeCloseTo(4)
    expect(easing[Math.floor(easing.length / 2)][1]).toBeCloseTo(2)
  })

  it('has nothing to draw without two nodes', () => {
    expect(routeSpans([])).toEqual([])
    expect(routeSpans([node(0, 0, 1, 0)])).toEqual([])
  })
})

describe('progress', () => {
  const spans = routeSpans([node(0, 0, 1, 0), node(80, 0, 1, 0), node(160, 0, 1, 0), node(240, 0, 1, 0)])

  it('finds the truck on the road, a little off its centre line', () => {
    const progress = progressOf(spans, 1, [30, 3])
    expect(progress?.span).toBe(0)
    expect(progress?.at[0]).toBeCloseTo(30)
    expect(progress?.at[1]).toBeCloseTo(0)
  })

  it('finds the truck past the node the host last reported ahead of it', () => {
    expect(progressOf(spans, 1, [100, 0])?.span).toBe(1)
  })

  it('starts the line where the truck is, and draws the rest after its span', () => {
    const progress = progressOf(spans, 1, [30, 2])!
    const head = headLine(spans, progress)
    expect(head[0][0]).toBeCloseTo(30)
    expect(head.at(-1)).toEqual([80, 0])
    const rest = restLine(spans, progress.span)
    expect(rest[0]).toEqual([80, 0])
    expect(rest.at(-1)).toEqual([240, 0])
    // Each node once: spans share their end points.
    expect(rest.filter(([x]) => x === 160)).toHaveLength(1)
  })

  it('tells the way back from the way there by the way the truck faces (a U-turn round a roundabout)', () => {
    // Out east along the road, round, and back west along the same road.
    const uTurn = routeSpans([
      node(0, 0, 1, 0),
      node(40, 0, 1, 0),
      node(60, -10, 0, -1),
      node(40, 0, -1, 0),
      node(0, 0, -1, 0),
    ])
    // Heading east, the truck is on the way out (span 0), not on the way back lying right on it (span 3).
    expect(progressOf(uTurn, 1, [20, 0], [1, 0])?.span).toBe(0)
    // Heading west, on the way back.
    expect(progressOf(uTurn, 1, [20, 0], [-1, 0])?.span).toBe(3)
  })

  it('has no progress without a route', () => {
    expect(progressOf([], 0, [0, 0])).toBeNull()
  })
})

describe('snapToRoute', () => {
  const spans = routeSpans([node(0, 0, 1, 0), node(80, 0, 1, 0), node(160, 0, 1, 0)])
  const shown = (x: number, z: number) => snapToRoute(progressOf(spans, 1, [x, z])!, [x, z])

  it('shows the truck in the middle of its road, whatever lane it is in', () => {
    // The road runs east; the truck is in a lane 4 m to its side.
    expect(shown(30, 4)[0]).toBeCloseTo(30)
    expect(shown(30, 4)[1]).toBeCloseTo(0)
  })

  it('lets go gradually as the truck leaves the route', () => {
    expect(shown(30, 14)[1]).toBeCloseTo(7)
    expect(shown(30, 30)[1]).toBeCloseTo(30)
  })
})

describe('ways through junctions', () => {
  it('takes the junction’s own way where the host found one, the road curve elsewhere', () => {
    const nodes = [node(0, 0, 1, 0), node(40, 0, 1, 0), node(80, 0, 1, 0)]
    // Round a roundabout between the second and third nodes.
    const round: [number, number][] = [
      [40, 0],
      [60, 20],
      [80, 0],
    ]
    const spans = routeSpans(nodes, new Map([[1, round]]))
    expect(spans[1]).toEqual(round)
    expect(spans[0].at(-1)).toEqual([40, 0])
    for (const [, z] of spans[0]) expect(z).toBeCloseTo(0)
  })
})

describe('approachLine', () => {
  // The game plans from the node ahead: the route starts 200 m up the road (east) from the truck.
  const spans = routeSpans([node(200, 0, 1, 0), node(300, 0, 1, 0), node(400, 50, 0, 1)])

  it('runs from the truck along its road to the route, while it is short of its start', () => {
    const truck: [number, number] = [0, 0]
    const progress = progressOf(spans, 0, truck, [1, 0])
    expect(progress && progress.at).toEqual([200, 0])
    const line = progress && approachLine(spans, progress, truck, [1, 0])
    expect(line?.[0]).toEqual([0, 0])
    expect(line?.at(-1)).toEqual([200, 0])
    for (const [, z] of line ?? []) expect(z).toBeCloseTo(0)
  })

  it('bends round from the way the truck faces', () => {
    // Coming up from the south (heading north, -z), onto the road east.
    const truck: [number, number] = [150, 60]
    const facing: [number, number] = [0.6, -0.8]
    const progress = progressOf(spans, 0, truck, facing)
    const line = progress && approachLine(spans, progress, truck, facing)
    expect(line).not.toBeNull()
    expect(line?.[0]).toEqual(truck)
    const second = line?.[1] ?? [0, 0]
    // Leaving the truck the way it faces.
    expect(second[0] - truck[0]).toBeGreaterThan(0)
    expect(second[1] - truck[1]).toBeLessThan(0)
  })

  it('is no more once the truck is on the route, or turned away from it', () => {
    const on: [number, number] = [250, 2]
    const progress = progressOf(spans, 1, on, [1, 0])
    expect(progress && approachLine(spans, progress, on, [1, 0])).toBeNull()
    const behind: [number, number] = [0, 0]
    const back = progressOf(spans, 0, behind, [-1, 0])
    expect(back && approachLine(spans, back, behind, [-1, 0])).toBeNull()
  })
})

describe('routeLevels', () => {
  // A local road on a bridge from x -20 to 20 (heading +x), over a motorway heading +z.
  const bridge: RaisedRoad[] = [
    {
      points: [
        [-20, 0],
        [20, 0],
      ],
      width: 8,
      level: 1,
    },
  ]

  it('draws the route on the ground where there are no raised roads', () => {
    const line: [number, number][] = [
      [0, 0],
      [50, 0],
    ]
    expect(routeLevels(line, [])).toEqual([{ points: line, level: 0 }])
  })

  it('raises the route where it runs over a bridge, and nowhere else', () => {
    const parts = routeLevels(
      [
        [-100, 1],
        [100, 1],
      ],
      bridge,
    )
    expect(parts.map((part) => part.level)).toEqual([0, 1, 0])
    // On it along the bridge (and no further than the road's edge and a little past its ends).
    const [start, end] = [parts[1].points[0], parts[1].points.at(-1)]
    expect(start[0]).toBeLessThanOrEqual(-20)
    expect(start[0]).toBeGreaterThanOrEqual(-28)
    expect(end?.[0]).toBeGreaterThanOrEqual(20)
    expect(end?.[0]).toBeLessThanOrEqual(28)
    // One line, end to end.
    expect(parts[0].points[0]).toEqual([-100, 1])
    expect(parts[2].points.at(-1)).toEqual([100, 1])
    for (let i = 0; i + 1 < parts.length; i++) expect(parts[i].points.at(-1)).toEqual(parts[i + 1].points[0])
  })

  it('keeps the route on the ground under a bridge, and beside it', () => {
    // Along the motorway beneath, crossing under it.
    expect(
      routeLevels(
        [
          [0, -100],
          [0, 100],
        ],
        bridge,
      ).map((part) => part.level),
    ).toEqual([0])
    // Along a road beside it, past its edge.
    expect(
      routeLevels(
        [
          [-100, 12],
          [100, 12],
        ],
        bridge,
      ).map((part) => part.level),
    ).toEqual([0])
  })
})

describe('routeAhead', () => {
  // Three spans east along z = 0: 0-100, 100-300, 300-1000 m.
  const spans: [number, number][][] = [
    [
      [0, 0],
      [100, 0],
    ],
    [
      [100, 0],
      [300, 0],
    ],
    [
      [300, 0],
      [1000, 0],
    ],
  ]

  it("takes the route from the truck's span on, as far as asked, cut within a stretch", () => {
    expect(routeAhead(spans, 1, 400)).toEqual([
      [100, 0],
      [300, 0],
      [500, 0],
    ])
  })

  it("stops at the route's end when it's nearer", () => {
    expect(routeAhead(spans, 2, 5000)).toEqual([
      [300, 0],
      [1000, 0],
    ])
  })
})
