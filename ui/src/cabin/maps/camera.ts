/**
 * The driving camera: follows the truck smoothly between telemetry updates (~20 a second), heading up,
 * with the map's rotation damped so it never twitches, and zoomed out as the truck speeds up.
 */

export interface TruckSample {
  /** When it arrived, ms (performance.now()). */
  time: number
  x: number
  z: number
  /** Turns, counter-clockwise from north (the game's telemetry). */
  heading: number
  /** m/s */
  speed: number
}

/** How far behind the latest sample the camera runs, so it always has two samples to move between. */
export const FOLLOW_DELAY_MS = 70

/** The truck's compass bearing in degrees (clockwise from north, as MapLibre's), from its heading. */
export function bearingOf(heading: number): number {
  return ((((1 - heading) * 360) % 360) + 360) % 360
}

/** The signed difference `to - from` between two bearings, -180..180. */
export function bearingDelta(from: number, to: number): number {
  return ((((to - from) % 360) + 540) % 360) - 180
}

/** The truck at `time`, between the samples around it (the last one if `time` is past it). */
export function truckAt(samples: readonly TruckSample[], time: number): TruckSample | null {
  if (samples.length === 0) return null
  const last = samples[samples.length - 1]
  if (time >= last.time) return last
  for (let i = samples.length - 1; i > 0; i--) {
    const a = samples[i - 1]
    const b = samples[i]
    if (time >= a.time) {
      const t = b.time === a.time ? 1 : (time - a.time) / (b.time - a.time)
      const lerp = (u: number, v: number) => u + (v - u) * t
      // Turn the short way round.
      const turn = ((((b.heading - a.heading) % 1) + 1.5) % 1) - 0.5
      return {
        time,
        x: lerp(a.x, b.x),
        z: lerp(a.z, b.z),
        heading: (((a.heading + turn * t) % 1) + 1) % 1,
        speed: lerp(a.speed, b.speed),
      }
    }
  }
  return samples[0]
}

/** Zoom for a speed: close in town, further out on the motorway, so the road ahead keeps the same time. */
export function zoomForSpeed(speed: number): number {
  const kmh = Math.abs(speed) * 3.6
  const t = Math.min(1, Math.max(0, (kmh - 20) / 70))
  const eased = t * t * (3 - 2 * t)
  return 17.2 - eased * 1.3
}

/** Exponential smoothing towards `target` with time constant `tau` (ms): frame-rate independent. */
export function damp(current: number, target: number, dt: number, tau: number): number {
  return target + (current - target) * Math.exp(-dt / tau)
}

/** `damp` for bearings, the short way round. */
export function dampBearing(current: number, target: number, dt: number, tau: number): number {
  const next = current + bearingDelta(current, target) * (1 - Math.exp(-dt / tau))
  return ((next % 360) + 360) % 360
}

/** Where the map looks from: its centre (lng, lat), zoom, bearing (degrees) and tilt (degrees). */
export interface CameraPose {
  center: [number, number]
  zoom: number
  bearing: number
  pitch: number
}

/** How long recentring glides back onto the truck, ms. */
export const GLIDE_MS = 900

/** How long the map is left where it was moved before it recentres itself, ms (like CarPlay's). */
export const AUTO_RECENTRE_MS = 15_000

/**
 * How far the player may zoom the following camera in or out from its speed-based zoom (zoom levels):
 * pulled out over a region, or in to a junction, without losing the camera's feel for speed.
 */
export const ZOOM_OFFSET_LIMITS: readonly [number, number] = [-4, 2]

export function clampZoomOffset(offset: number): number {
  return Math.min(ZOOM_OFFSET_LIMITS[1], Math.max(ZOOM_OFFSET_LIMITS[0], offset))
}

/**
 * The zoom offset that puts the following camera at `zoom` at this speed: a zoom the player chose while
 * browsing, kept when the map goes back to following (and still varying with speed from there).
 */
export function zoomOffsetFor(zoom: number, speed: number): number {
  return clampZoomOffset(zoom - zoomForSpeed(speed))
}

/** Eases a glide's progress (0..1): gentle out, gentle in. */
export function easeInOut(t: number): number {
  const u = Math.min(1, Math.max(0, t))
  return u < 0.5 ? 4 * u * u * u : 1 - (-2 * u + 2) ** 3 / 2
}

/**
 * Whether two poses look the same on screen: nearer than about a tenth of a pixel at the closest zoom
 * (1e-7 degrees is a centimetre here). Moving the map to the same pose still makes it draw a whole frame.
 */
export function samePose(a: CameraPose, b: CameraPose): boolean {
  return (
    Math.abs(a.center[0] - b.center[0]) < 1e-7 &&
    Math.abs(a.center[1] - b.center[1]) < 1e-7 &&
    Math.abs(a.zoom - b.zoom) < 1e-4 &&
    Math.abs(bearingDelta(a.bearing, b.bearing)) < 1e-3 &&
    Math.abs(a.pitch - b.pitch) < 1e-3
  )
}

/** The pose `t` of the way (0..1) from `from` to `to`, turning the short way round. */
export function blendPose(from: CameraPose, to: CameraPose, t: number): CameraPose {
  const lerp = (a: number, b: number) => a + (b - a) * t
  return {
    center: [lerp(from.center[0], to.center[0]), lerp(from.center[1], to.center[1])],
    zoom: lerp(from.zoom, to.zoom),
    bearing: (((from.bearing + bearingDelta(from.bearing, to.bearing) * t) % 360) + 360) % 360,
    pitch: lerp(from.pitch, to.pitch),
  }
}
