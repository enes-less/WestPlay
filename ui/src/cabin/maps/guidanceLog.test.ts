import { describe, expect, it } from 'vitest'
import { guidanceLine } from './guidance'

describe("the cab's guidance for the log", () => {
  it('says the turn taken as the next, its node, both distances and what is shown', () => {
    const route = {
      id: 7,
      distances: [9000, 5000, 4990, 0],
      maneuvers: [{ node: 2, turn: 'right' as const, angle: 90, exit: null }],
    }
    expect(guidanceLine(route, 1, 6200)).toBe(
      "route 7: 4 nodes, 1 turns; node ahead 1, distance left 6200 m (the game's telemetry); " +
        'next: right at node 2 (its own distance left 4990 m); shown: 1210 m',
    )
    expect(guidanceLine(route, 1, 4995)).toMatch(/shown: now$/)
    expect(guidanceLine(route, 1, 0)).toMatch(/no guidance shown$/)
  })
})
