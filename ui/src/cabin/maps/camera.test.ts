import { describe, expect, it } from 'vitest'
import {
  bearingDelta,
  bearingOf,
  blendPose,
  type CameraPose,
  clampZoomOffset,
  zoomOffsetFor,
  damp,
  dampBearing,
  easeInOut,
  samePose,
  type TruckSample,
  truckAt,
  zoomForSpeed,
} from './camera'
import { labelRange, metresBetween, pixelsPerMetre, toLngLat } from './projection'

const sample = (time: number, x: number, heading: number): TruckSample => ({ time, x, z: 0, heading, speed: 10 })

describe('bearingOf', () => {
  it("turns the game's counter-clockwise turns into compass degrees", () => {
    expect(bearingOf(0)).toBe(0)
    expect(bearingOf(0.25)).toBe(270) // facing west
    expect(bearingOf(0.75)).toBe(90) // facing east
  })
})

describe('bearingDelta', () => {
  it('takes the short way round', () => {
    expect(bearingDelta(350, 10)).toBe(20)
    expect(bearingDelta(10, 350)).toBe(-20)
  })
})

describe('truckAt', () => {
  const samples = [sample(0, 0, 0.95), sample(50, 10, 0.05)]

  it('moves between samples, turning the short way', () => {
    const middle = truckAt(samples, 25)!
    expect(middle.x).toBe(5)
    expect(middle.heading).toBeCloseTo(0)
  })

  it('holds the last sample when time runs past it, and copes with none', () => {
    expect(truckAt(samples, 80)!.x).toBe(10)
    expect(truckAt([], 0)).toBeNull()
  })
})

describe('zoomForSpeed', () => {
  it('pulls out as speed rises, within bounds', () => {
    expect(zoomForSpeed(0)).toBe(17.2)
    expect(zoomForSpeed(25)).toBeCloseTo(15.9)
    expect(zoomForSpeed(-5)).toBe(17.2)
    expect(zoomForSpeed(10)).toBeLessThan(zoomForSpeed(5))
  })
})

describe('damping', () => {
  it('approaches the target without overshooting, the short way for bearings', () => {
    const next = damp(0, 100, 16, 100)
    expect(next).toBeGreaterThan(0)
    expect(next).toBeLessThan(100)
    const bearing = dampBearing(355, 5, 1e6, 100)
    expect(bearing).toBeCloseTo(5)
  })
})

describe('projection', () => {
  it('maps metres to degrees on the equator, z south', () => {
    const [lng, lat] = toLngLat(111_319.490_793_273_57, 111_319.490_793_273_57)
    expect(lng).toBeCloseTo(1)
    expect(lat).toBeCloseTo(-1)
    // 2^zoom * 512 pixels around the world.
    expect(pixelsPerMetre(0) * 40_075_016.686).toBeCloseTo(512, 1)
  })
})

describe('labels', () => {
  it('show within a range that shrinks as the map zooms in', () => {
    expect(metresBetween(toLngLat(0, 0), toLngLat(300, 400))).toBeCloseTo(500)
    expect(labelRange(16, 1000)).toBeCloseTo(2 * labelRange(17, 1000))
    // The driving view (zoom ~16, ~1000 px): a couple of kilometres, not the next city 12 km away.
    expect(labelRange(16, 1000)).toBeGreaterThan(1500)
    expect(labelRange(16, 1000)).toBeLessThan(3000)
  })
})

describe('recentring', () => {
  const from: CameraPose = { center: [0, 0], zoom: 13, bearing: 350, pitch: 52 }
  const to: CameraPose = { center: [2, 4], zoom: 17, bearing: 10, pitch: 52 }

  it('glides between poses, turning the short way', () => {
    expect(blendPose(from, to, 0)).toEqual(from)
    expect(blendPose(from, to, 1)).toEqual(to)
    const middle = blendPose(from, to, 0.5)
    expect(middle.center).toEqual([1, 2])
    expect(middle.zoom).toBe(15)
    expect(middle.bearing).toBeCloseTo(0)
  })

  it('eases in and out, and stays within its ends', () => {
    expect(easeInOut(0)).toBe(0)
    expect(easeInOut(1)).toBe(1)
    expect(easeInOut(0.5)).toBe(0.5)
    expect(easeInOut(0.1)).toBeLessThan(0.1)
    expect(easeInOut(-1)).toBe(0)
    expect(easeInOut(2)).toBe(1)
  })

  it('keeps the following zoom within reach of its speed zoom', () => {
    expect(clampZoomOffset(1)).toBe(1)
    expect(clampZoomOffset(9)).toBe(2)
    expect(clampZoomOffset(-9)).toBe(-4)
  })
})

describe('samePose', () => {
  const pose: CameraPose = { center: [0.2, -0.5], zoom: 15, bearing: 359.9995, pitch: 60 }

  it('ignores differences too small to see, the bearing across north included', () => {
    expect(samePose(pose, { ...pose, center: [0.2 + 5e-8, -0.5], bearing: 0.0002 })).toBe(true)
  })

  it('notices any that show', () => {
    expect(samePose(pose, { ...pose, center: [0.2 + 1e-6, -0.5] })).toBe(false)
    expect(samePose(pose, { ...pose, zoom: 15.001 })).toBe(false)
    expect(samePose(pose, { ...pose, bearing: 0.1 })).toBe(false)
  })
})

describe('zoomOffsetFor', () => {
  it('keeps a zoom chosen while browsing, as an offset from the speed zoom', () => {
    // Parked (speed zoom 17.2), zoomed in to 18.7: 1.5 closer, which stays 1.5 closer at any speed.
    expect(zoomOffsetFor(18.7, 0)).toBeCloseTo(1.5)
    expect(zoomForSpeed(25) + zoomOffsetFor(18.7, 0)).toBeCloseTo(zoomForSpeed(25) + 1.5)
    // No further than the camera may be pulled in or out.
    expect(zoomOffsetFor(30, 0)).toBe(2)
    expect(zoomOffsetFor(5, 0)).toBe(-4)
  })
})
