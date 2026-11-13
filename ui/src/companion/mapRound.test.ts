import { describe, expect, it } from 'vitest'
import { nextRound, pending, roundProgress, startedOver } from './mapRound'
import type { MapStatus, ProfileMap } from './useHostStatus'

const ready: MapStatus = { state: 'ready' }
const queued: MapStatus = { state: 'queued' }
const building = (done: number): MapStatus => ({ state: 'building', done, stage: 'drawing' })
const maps = (...statuses: MapStatus[]): ProfileMap[] =>
  statuses.map((status, i) => ({ game: 'ets2', dir: `p${i}`, folder: `m${i}`, status }))

describe('a round of maps being prepared', () => {
  it('starts the bar over when the host says less than it did (the build started over)', () => {
    expect(startedOver(0.6, 0.02)).toBe(true)
    expect(startedOver(0.6, 0.6)).toBe(false)
    expect(startedOver(0.6, 0.61)).toBe(false)
    expect(startedOver(null, 0.1)).toBe(false)
    expect(startedOver(0.4, null)).toBe(false)
  })

  it('counts a map waiting for the game or for memory as not done, as the bar does', () => {
    expect(pending({ state: 'waitingForGame' })).toBe(true)
    expect(pending({ state: 'waitingForGame', stopped: true })).toBe(true)
    expect(pending({ state: 'waitingForMemory', needed: 3, free: 1 })).toBe(true)
    expect(pending(queued)).toBe(true)
    expect(pending(building(0.2))).toBe(true)
    expect(pending(ready)).toBe(false)
    expect(pending({ state: 'failed', error: 'e' })).toBe(false)
  })

  it("tells TruckersMP's maps apart, all in TruckersMP's folder", () => {
    const tmp = 'C:\TruckersMP\installation'
    // ETS2's made, ATS's being made: the round is ATS's, half way (it read ETS2's, made, as 100%).
    const before: ProfileMap[] = [
      { game: 'ets2', dir: tmp, folder: '', status: ready, truckersmp: true },
      { game: 'ats', dir: tmp, folder: '', status: queued, truckersmp: true },
    ]
    const now: ProfileMap[] = [
      { game: 'ets2', dir: tmp, folder: '', status: ready, truckersmp: true },
      { game: 'ats', dir: tmp, folder: '', status: building(0.5), truckersmp: true },
      { game: 'ets2', dir: tmp, folder: '', status: queued, truckersmp: true, promods: true },
    ]
    const round = nextRound(nextRound([], before), now)
    expect(round).toHaveLength(2)
    expect(roundProgress(round, now, 0.5)).toBeCloseTo(0.25)
  })

  it('gives a map one share however many profiles play it', () => {
    // Eleven profiles on one map, one on another: the first made is half the bar, not eleven twelfths.
    const shared = (status: MapStatus, i: number): ProfileMap => ({
      game: 'ets2',
      dir: `p${i}`,
      folder: 'europe-a',
      status,
    })
    const other = (status: MapStatus): ProfileMap => ({ game: 'ets2', dir: 'p11', folder: 'europe-b', status })
    const before = [...Array.from({ length: 11 }, (_, i) => shared(building(0.5), i)), other(queued)]
    let round = nextRound([], before)
    expect(round).toHaveLength(2)
    expect(roundProgress(round, before, 0.5)).toBeCloseTo(0.25)
    const now = [...Array.from({ length: 11 }, (_, i) => shared(ready, i)), other(building(0.2))]
    round = nextRound(round, now)
    expect(round).toHaveLength(2)
    expect(roundProgress(round, now, 0.2)).toBeCloseTo(0.6)
  })

  it('gives each of ten maps a tenth of the bar', () => {
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
    expect(round).toEqual(['ets2|m9|'])
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
