import { describe, expect, it } from 'vitest'
import { buildingFilter, canopiesOver } from './buildings'

const square = (x: number, y: number, size: number): [number, number][] => [
  [x, y],
  [x + size, y],
  [x + size, y + size],
  [x, y + size],
  [x, y],
]

describe('canopies over the truck', () => {
  it('finds the canopies the truck is under, each once', () => {
    const canopies = [
      { id: 7, rings: [square(0, 0, 1)] },
      { id: 3, rings: [square(0.5, 0.5, 1)] },
      { id: 9, rings: [square(5, 5, 1)] },
      // The same canopy in a second tile.
      { id: 7, rings: [square(0, 0, 1)] },
    ]
    expect(canopiesOver(canopies, [0.75, 0.75])).toEqual([3, 7])
    expect(canopiesOver(canopies, [0.25, 0.25])).toEqual([7])
    // In the open: none.
    expect(canopiesOver(canopies, [3, 3])).toEqual([])
  })

  it('leaves out only the canopies the truck is under', () => {
    expect(buildingFilter([])).toBeNull()
    expect(buildingFilter([3, 7])).toEqual(['!', ['in', ['get', 'id'], ['literal', [3, 7]]]])
  })
})
