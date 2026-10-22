import { describe, expect, it } from 'vitest'
import { nextStop } from './nextStop'

describe('the next stop', () => {
  // A route of six map nodes, 1000 m apart, 5000 m long; stops at nodes 2 and 4.
  const distances = [5000, 4000, 3000, 2000, 1000, 0]
  const stops = [2, 4]

  it('is the first stop ahead, its time a share of the whole by distance', () => {
    // At the start: 5000 m and 50 minutes left; the first stop 2000 m on.
    expect(nextStop(stops, distances, 0, 5000, 3000)).toEqual({ distance: 2000, minutes: 20 })
  })

  it('once passed, the one after', () => {
    // Past node 2 (the next node ahead is 3): the stop at 4, 2500 m on of 3500.
    expect(nextStop(stops, distances, 3, 3500, 2100)).toEqual({ distance: 2500, minutes: 25 })
  })

  it('none after the last stop, without stops, or without a route left', () => {
    expect(nextStop(stops, distances, 5, 500, 300)).toBeNull()
    expect(nextStop([], distances, 0, 5000, 3000)).toBeNull()
    expect(nextStop(stops, distances, 0, 0, 0)).toBeNull()
  })

  it('a stop the truck is at (no way left to it) is passed over', () => {
    expect(nextStop(stops, distances, 2, 3000, 1800)).toEqual({ distance: 2000, minutes: 20 })
  })
})
