import { describe, expect, it } from 'vitest'
import { nextRound, roundProgress } from './mapRound'
import type { MapStatus, ProfileMap } from './useHostStatus'

const ready: MapStatus = { state: 'ready' }
const queued: MapStatus = { state: 'queued' }
const building = (done: number): MapStatus => ({ state: 'building', done, stage: 'drawing' })
const maps = (...statuses: MapStatus[]): ProfileMap[] => statuses.map((status, i) => ({ dir: `p${i}`, status }))

describe('a round of maps being prepared', () => {
  it('gives each of ten profiles a tenth of the bar', () => {
    // Prepare Maps Again: the first made, the second half way, eight waiting.
    const now = maps(ready, building(0.5), ...Array<MapStatus>(8).fill(queued))
    // Seen from the start: all ten in the round.
    const round = nextRound(nextRound([], maps(building(0.2), ...Array<MapStatus>(9).fill(queued))), now)
    expect(round).toHaveLength(10)
    expect(roundProgress(round, now, 0.5)).toBeCloseTo(0.15)
  })

  it('counts six profiles a sixth each', () => {
    const round = nextRound([], maps(...Array<MapStatus>(6).fill(queued)))
    const halfway = maps(ready, ready, ready, building(0), queued, queued)
    expect(roundProgress(nextRound(round, halfway), halfway, 0)).toBeCloseTo(0.5)
  })

  it('is just the changed profile when only its map is made', () => {
    // Nine maps made before; one profile's mods changed.
    const round = nextRound([], maps(...Array<MapStatus>(9).fill(ready), building(0.3)))
    expect(round).toEqual(['p9'])
    expect(roundProgress(round, maps(...Array<MapStatus>(9).fill(ready), building(0.3)), 0.3)).toBeCloseTo(0.3)
  })

  it('ends once no map is waiting or being made, a failed one counted done', () => {
    let round = nextRound([], maps(building(0.1), queued))
    const done = maps(ready, { state: 'failed', error: 'x' })
    expect(roundProgress(round, done, 0)).toBe(1)
    round = nextRound(round, done)
    expect(round).toEqual([])
    expect(roundProgress(round, done, 0)).toBeNull()
  })
})
