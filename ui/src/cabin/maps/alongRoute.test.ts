import { describe, expect, it } from 'vitest'
import { placesAhead } from './alongRoute'
import type { Destination, DestinationKind } from './places'
import { toLngLat } from './projection'
import type { Point, Spans } from './route'

/** A straight road east along z = 0, from x = 0 to 20 km, in spans of 1 km with a point every 10 m. */
const spans: Spans = Array.from({ length: 20 }, (_, span) =>
  Array.from({ length: 101 }, (_, i): Point => [span * 1000 + i * 10, 0]),
)

const place = (kind: DestinationKind, name: string, x: number, z: number): Destination => ({
  kind,
  name,
  at: toLngLat(x, z),
  node: name,
})

describe('places along the route ahead', () => {
  // The truck 5 km along.
  const progress = { span: 5, index: 0, at: [5000, 0] as Point, distance: 0 }

  it('offers only what’s beside the rest of the route, nearest along it first', () => {
    const found = placesAhead(
      [
        place('fuel', 'far on', 15_000, 300),
        place('fuel', 'next', 8_000, -200),
        place('fuel', 'behind', 3_000, 100),
        place('fuel', 'off the road', 9_000, 4_000),
        place('company', 'in town', 12_000, 2_500),
        place('fuel', 'past the end', 26_000, 0),
      ],
      spans,
      progress,
    )
    expect(found.map(({ place }) => place.name)).toEqual(['next', 'in town', 'far on'])
    expect(found[0].along).toBeCloseTo(3000)
    expect(found[0].off).toBeCloseTo(200)
  })

  it('leaves out a place the truck is at or has just passed', () => {
    const found = placesAhead(
      [place('fuel', 'here', 5_000, 50), place('fuel', 'just passed', 4_990, 30)],
      spans,
      progress,
    )
    expect(found).toEqual([])
  })

  it('has nothing ahead at the route’s end', () => {
    const end = { span: 19, index: 100, at: [20_000, 0] as Point, distance: 0 }
    expect(placesAhead([place('fuel', 'next', 8_000, 0)], spans, end)).toEqual([])
  })
})
