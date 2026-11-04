import { describe, expect, it } from 'vitest'
import {
  areaQuery,
  around,
  feedClock,
  findSelf,
  type OtherPlayer,
  otherPlayers,
  ownId,
  type PathPoint,
  pathDistance,
  readFeed,
  SELF_WITHIN,
  type Sample,
  selfOnPath,
  TRACK_SAMPLES,
  trackAt,
  withRead,
} from './truckersmpFeed'

describe('readFeed', () => {
  it("keeps only each truck's position, heading, number and time: no names", () => {
    const answer = {
      Success: true,
      Data: [
        {
          Name: 'someone',
          X: -6175,
          Y: 2139,
          Heading: 1.17,
          MpId: 99,
          PlayerId: 113,
          ServerId: 2,
          Time: 100,
          VtcId: 7,
        },
        { Name: 'broken', X: 'x', Y: 1, Heading: 0, PlayerId: 1, Time: 100 },
      ],
    }
    expect(readFeed(answer)).toEqual([{ id: 113, x: -6175, z: 2139, heading: 1.17, time: 100 }])
  })

  it('reads anything else as no trucks', () => {
    expect(readFeed(null)).toEqual([])
    expect(readFeed({ Success: false })).toEqual([])
  })
})

describe('findSelf', () => {
  const read = [
    { id: 1, x: 0, z: 0, heading: 0, time: 0 },
    { id: 113, x: -6175, z: 2139, heading: 1.17, time: 0 },
  ]

  it("finds the player's own truck where theirs is (the game's x and z: the owner's, 0.9 m off)", () => {
    expect(findSelf(read, { x: -6175.76, z: 2139.54 })?.id).toBe(113)
  })

  it('finds none when no truck is near', () => {
    expect(findSelf(read, { x: -6175 + SELF_WITHIN + 1, z: 2139 })).toBeNull()
  })
})

describe('ownId', () => {
  // The player at (0, 0); another player 60 m off, that TruckersMP's SDK has (42); the player's own truck on the
  // feed 120 m behind (the feed's lag at speed, 7).
  const read = [
    { id: 42, x: 60, z: 0, heading: 0, time: 0 },
    { id: 7, x: 0, z: 120, heading: 0, time: 0 },
    { id: 9, x: 5000, z: 0, heading: 0, time: 0 },
  ]
  const truck = { x: 0, z: 0 }

  it("takes the SDK's number when the read has it", () => {
    expect(ownId(read, truck, null, { local: 9, others: [42] })).toBe(9)
  })

  it("finds the player's own truck while the SDK can't name them yet (-1, as they join), passing its others", () => {
    expect(ownId(read, truck, -1, { local: -1, others: [42] })).toBe(7)
  })

  it("finds it too when the SDK's number isn't on the feed", () => {
    expect(ownId(read, truck, 3, { local: 3, others: [42] })).toBe(7)
  })

  it("keeps the SDK's number, or the one known, with nothing near", () => {
    expect(ownId([read[2]], truck, null, { local: 3, others: [] })).toBe(3)
    expect(ownId([read[2]], truck, 5, { local: -1, others: [] })).toBe(5)
  })

  it('keeps the one known without the SDK', () => {
    expect(ownId(read, truck, 9, null)).toBe(9)
  })
})

describe('otherPlayers', () => {
  it("leaves out the player's own truck and trucks the feed stopped placing", () => {
    const read = [
      { id: 113, x: 0, z: 0, heading: 0, time: 100 },
      { id: 2, x: 10, z: 0, heading: 0, time: 100 },
      { id: 3, x: 20, z: 0, heading: 0, time: 60 },
    ]
    expect(otherPlayers(read, 113, new Map()).map((player) => player.id)).toEqual([2])
  })

  it('measures speed between two reads, and keeps it while the feed has not moved a truck on', () => {
    const before = new Map<number, OtherPlayer>([
      [2, { id: 2, x: 0, z: 0, heading: 0, vx: 0, vz: 0, speed: null, time: 100 }],
    ])
    const [moved] = otherPlayers([{ id: 2, x: 30, z: 40, heading: 0, time: 105 }], null, before)
    expect(moved.speed).toBe(10)
    expect([moved.vx, moved.vz]).toEqual([6, 8])
    const [same] = otherPlayers([{ id: 2, x: 30, z: 40, heading: 0, time: 105 }], null, new Map([[2, moved]]))
    expect(same.speed).toBe(10)
    // First seen: no speed yet.
    expect(otherPlayers([{ id: 4, x: 0, z: 0, heading: 0, time: 105 }], null, before)[0].speed).toBeNull()
  })
})

describe('areas', () => {
  it('asks the feed for an area as its own map does: top left (larger y), then bottom right', () => {
    // map.truckersmp.com's own: x1=6455&y1=22710&x2=8655&y2=20510.
    expect(areaQuery([6455, 20510, 8655, 22710])).toBe('x1=6455&y1=22710&x2=8655&y2=20510')
    expect(around({ x: 10, z: 20 }, 1000)).toEqual([-990, -980, 1010, 1020])
  })
})

describe('tracks', () => {
  const at = (id: number, t: number, x: number) => ({ id, x, z: 0, heading: 0, time: t })

  it("keeps each truck's places as the feed gives them, once each, the player's own left out", () => {
    let tracks = withRead(new Map(), [at(1, 100, 0), at(113, 100, 5)], 113)
    tracks = withRead(tracks, [at(1, 100, 0)], 113) // not moved on since
    tracks = withRead(tracks, [at(1, 102, 40)], 113)
    expect(tracks.get(1)).toEqual([
      { t: 100, x: 0, z: 0 },
      { t: 102, x: 40, z: 0 },
    ])
    expect(tracks.has(113)).toBe(false)
    // Only the last few kept; a truck not in a read is dropped.
    for (let t = 103; t < 120; t++) tracks = withRead(tracks, [at(1, t, t)], null)
    expect(tracks.get(1)).toHaveLength(TRACK_SAMPLES)
    expect(withRead(tracks, [at(2, 120, 0)], null).has(1)).toBe(false)
  })

  it('draws a truck between its places, never past the last', () => {
    const samples: Sample[] = [
      { t: 100, x: 0, z: 0 },
      { t: 102, x: 40, z: 20 },
      { t: 103, x: 60, z: 20 },
    ]
    expect(trackAt(samples, 99)).toEqual([0, 0])
    expect(trackAt(samples, 101)).toEqual([20, 10])
    expect(trackAt(samples, 102.5)).toEqual([50, 20])
    expect(trackAt(samples, 110)).toEqual([60, 20])
  })

  it("keeps the feed's clock steady through whole-second times", () => {
    expect(feedClock(null, 1000.2)).toBe(1000.2)
    // A read a second off moves it only a little; a jump of minutes is taken at once.
    expect(feedClock(1000, 1001)).toBeCloseTo(1000.1)
    expect(feedClock(1000, 1600)).toBe(1600)
  })
})

describe('selfOnPath', () => {
  // The player's truck heading east (a quarter turn: 0 is north, turning west, so east is 0.75), 20 m a second,
  // its path every second up to the feed's time 1000 (s).
  const path: PathPoint[] = Array.from({ length: 10 }, (_, i) => ({
    at: (991 + i) * 1000,
    x: 100 + i * 20,
    z: 500,
    heading: 0.75,
  }))
  const east = 0.75 * 2 * Math.PI
  const feed = (id: number, x: number, z: number, heading: number, time = 997) => ({ id, x, z, heading, time })

  it("finds the player's truck where theirs was at the feed's time, facing their way", () => {
    // Placed by the feed three seconds behind: where the truck was then (x 220), not where it is now (x 280).
    expect(pathDistance(feed(7, 222, 503, east), path)).toBeCloseTo(Math.hypot(2, 3))
    expect(selfOnPath([feed(7, 222, 503, east)], path)?.player.id).toBe(7)
  })

  it("takes no stranger near it for theirs (a server's 1,100 players are near anyone)", () => {
    // 120 m off: what any truck within 150 m used to be taken for (and Simulation 1, listed first, with it).
    expect(selfOnPath([feed(3, 220, 620, east)], path)).toBeNull()
    // Beside it but facing the other way (the other carriageway), or parked across it.
    expect(selfOnPath([feed(4, 225, 510, east + Math.PI)], path)).toBeNull()
    expect(selfOnPath([feed(5, 225, 510, 0)], path)).toBeNull()
    // Where it was, but long before or after.
    expect(selfOnPath([feed(6, 222, 503, east, 900)], path)).toBeNull()
    // The nearest of those that fit.
    expect(selfOnPath([feed(8, 240, 515, east), feed(9, 221, 501, east)], path)?.player.id).toBe(9)
    // The feed's heading is the game's in radians either side of north, as its own (−1.56: east).
    expect(pathDistance(feed(10, 220, 500, -1.56), path)).not.toBeNull()
  })
})
