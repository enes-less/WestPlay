import { describe, expect, it } from 'vitest'
import { guidanceOf, type Maneuver } from './guidance'

// A route of 6 nodes, 1000 m apart (as the game counts them): 5000 m from the first to the destination.
const distances = [5000, 4000, 3000, 2000, 1000, 0]
const turn = (node: number, kind: Maneuver['turn'] = 'right'): Maneuver => ({ node, turn: kind, angle: 90, exit: null })

describe('guidanceOf', () => {
  it('gives the next turn and how far it is', () => {
    const guidance = guidanceOf(distances, [turn(2), turn(4, 'left')], 1, 4600)
    expect(guidance?.step).toEqual({ kind: 'turn', maneuver: turn(2) })
    expect(guidance?.distance).toBe(1600)
    expect(guidance?.then).toBeNull()
  })

  it('keeps a turn next while the truck is in its junction, as now', () => {
    const guidance = guidanceOf(distances, [turn(2), turn(4)], 3, 2950)
    expect(guidance?.step).toEqual({ kind: 'turn', maneuver: turn(2) })
    expect(guidance?.distance).toBe(0)
  })

  it('moves on once the truck is through it', () => {
    const guidance = guidanceOf(distances, [turn(2), turn(4, 'left')], 4, 1900)
    expect(guidance?.step).toEqual({ kind: 'turn', maneuver: turn(4, 'left') })
  })

  it('shows what comes right after a turn with it', () => {
    const close = [5000, 4000, 3000, 2800, 1000, 0]
    expect(guidanceOf(close, [turn(2), turn(3, 'left')], 1, 4000)?.then).toEqual({
      kind: 'turn',
      maneuver: turn(3, 'left'),
    })
    // The destination right after the last turn.
    expect(guidanceOf([5000, 4000, 3000, 2000, 300, 0], [turn(4)], 3, 1500)?.then).toEqual({ kind: 'arrive' })
  })

  it('leads to the destination after the last turn, and nothing without a route', () => {
    expect(guidanceOf(distances, [turn(2)], 4, 1500)).toEqual({ step: { kind: 'arrive' }, distance: 1500, then: null })
    expect(guidanceOf(distances, [], 0, 0)).toBeNull()
  })
})
