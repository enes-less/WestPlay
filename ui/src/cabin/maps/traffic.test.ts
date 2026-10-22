import { describe, expect, it } from 'vitest'
import type { Point } from './route'
import { JAMMED_BELOW, SLOW_BELOW, STRETCH, trafficAlong } from './traffic'
import type { OtherPlayer } from './truckersmpFeed'

// A straight route east along z = 0, 2 km long.
const route: Point[] = [
  [0, 0],
  [2000, 0],
]
// Heading east (+x) in the feed's terms: 0 is north (−z), turning west; east is three quarters round.
const EAST = (3 * Math.PI) / 2
const WEST = Math.PI / 2

let next = 0
const truck = (x: number, z: number, speed: number | null, heading = EAST): OtherPlayer => ({
  id: next++,
  x,
  z,
  heading,
  vx: 0,
  vz: 0,
  speed,
  time: 0,
})

describe('trafficAlong', () => {
  it('marks the stretch where trucks crawl, by how slow the middle of them is', () => {
    const jammed = [truck(1010, 3, 1), truck(1050, -2, 2), truck(1100, 4, 3)]
    const slow = [truck(410, 2, 8), truck(450, 0, 9), truck(480, -3, 10)]
    const stretches = trafficAlong(route, [...jammed, ...slow])
    expect(stretches.map(({ level }) => level)).toEqual(['slow', 'jammed'])
    // Each where its trucks are, a stretch long.
    expect(stretches[0].line[0][0]).toBe(2 * STRETCH)
    expect(stretches[1].line[0][0]).toBe(5 * STRETCH)
    expect(stretches[1].line.at(-1)?.[0]).toBe(6 * STRETCH)
  })

  it('leaves out trucks going the other way, off the road, without a speed yet, or too few', () => {
    const otherWay = [truck(1010, 3, 1, WEST), truck(1050, -2, 1, WEST), truck(1100, 4, 1, WEST)]
    const offRoad = [truck(1010, 60, 1), truck(1050, 70, 1), truck(1100, -80, 1)]
    const unknown = [truck(1010, 0, null), truck(1050, 0, null), truck(1100, 0, null)]
    const few = [truck(300, 0, 1), truck(320, 0, 1)]
    expect(trafficAlong(route, [...otherWay, ...offRoad, ...unknown, ...few])).toEqual([])
  })

  it('takes the middle speed: a parked truck among moving ones makes no jam', () => {
    const moving = [truck(1010, 0, 20), truck(1050, 0, 22), truck(1100, 0, 0), truck(1150, 0, 21)]
    expect(trafficAlong(route, moving)).toEqual([])
  })

  it('joins neighbouring stretches alike into one line, along the route as it turns', () => {
    const bend: Point[] = [
      [0, 0],
      [1000, 0],
      [1000, 1000],
    ]
    // Crawling either side of the bend: on the eastward leg, then the southward (+z: heading half round).
    const SOUTH = Math.PI
    const trucks = [
      ...[850, 900, 950].map((x) => truck(x, 0, JAMMED_BELOW / 2)),
      ...[30, 80, 130, 230, 280, 330].map((z) => truck(1000, z, JAMMED_BELOW / 2, SOUTH)),
    ]
    const stretches = trafficAlong(bend, trucks)
    expect(stretches).toHaveLength(1)
    expect(stretches[0].level).toBe('jammed')
    // It follows the route round the corner.
    expect(stretches[0].line).toContainEqual([1000, 0])
  })

  it('reads busy but moving traffic as none', () => {
    const flowing = [1010, 1040, 1070, 1100, 1130].map((x) => truck(x, 0, SLOW_BELOW + 5))
    expect(trafficAlong(route, flowing)).toEqual([])
  })
})
