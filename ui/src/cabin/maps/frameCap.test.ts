import { describe, expect, it } from 'vitest'
import { LEAST_PACE, displayClock, displayFrameMs, framesApart, mapFrameDue } from './frameCap'

/** The display's frames over `seconds` at `hz` (each a little early or late by `waver` of a frame, in turn): those
 * the map is drawn on, as liveMap.ts asks. */
function drawn(hz: number, seconds = 2, waver = 0): number[] {
  const clock = displayClock()
  let last = -Infinity
  const at: number[] = []
  for (let tick = 0; tick < hz * seconds; tick++) {
    const now = ((tick + (tick % 2 ? waver : -waver)) * 1000) / hz
    const frameMs = clock.tick(now)
    if (!mapFrameDue(now, last, frameMs)) continue
    last = now
    at.push(now)
  }
  // The second second's: the display measured by then.
  return at.filter((time) => time >= 1000)
}

/** The display frames from each one drawn on to the next. */
function apart(hz: number, waver = 0): Set<number> {
  const at = drawn(hz, 2, waver)
  return new Set(at.slice(1).map((time, i) => Math.round(((time - at[i]) * hz) / 1000)))
}

describe('the map drawn evenly, never fewer than 70 a second', () => {
  it('on every frame of a display up to twice that', () => {
    for (const hz of [50, 60, 74, 75, 90, 100, 120, 139]) {
      expect(apart(hz), `${hz} Hz`).toEqual(new Set([1]))
      expect(apart(hz, 0.1), `${hz} Hz, wavering`).toEqual(new Set([1]))
    }
  })

  it('on every 2nd, 3rd... of a faster one, evenly and at least 70 a second', () => {
    for (const [hz, every] of [
      [144, 2],
      [165, 2],
      [180, 2],
      [240, 3],
      [360, 5],
    ] as const) {
      expect(apart(hz), `${hz} Hz`).toEqual(new Set([every]))
      expect(apart(hz, 0.1), `${hz} Hz, wavering`).toEqual(new Set([every]))
      expect(hz / every).toBeGreaterThanOrEqual(LEAST_PACE)
    }
  })

  it("never at exactly 60 a second, which the host's 60 a second would fall out of step with", () => {
    // Every 2nd frame at 120 Hz, every 3rd at 180, 4th at 240: the same pace as the host's, a different phase.
    for (const hz of [120, 180, 240]) expect(1000 / ((1000 / hz) * framesApart(1000 / hz))).toBeGreaterThan(60)
  })

  it('gives the host a new map frame in every one of its 60 a second, whatever its phase', () => {
    // The host (crates/host capture.rs, Mailbox): every display frame captured a millisecond after it's shown (the
    // owner's Windows 10 delivers them all, changed or not), the newest sent when due, a 60th of a second apart,
    // 1.5 ms early at most.
    const handed = (hz: number, phase: number) => {
      const map = drawn(hz, 3, 0.1)
      const arrivals: [number, number][] = []
      for (let tick = Math.ceil((1000 * hz) / 1000); tick < 3 * hz; tick++) {
        const shown = ((tick + (tick % 2 ? 0.1 : -0.1)) * 1000) / hz
        const newest = map.filter((time) => time <= shown).at(-1) ?? 0
        arrivals.push([shown + 1, newest])
      }
      const sent: number[] = []
      let due = 1000 + phase
      let waiting: number | null = null
      let next = 0
      for (let t = 1000; t < 3000; t += 0.25) {
        while (next < arrivals.length && arrivals[next][0] <= t) waiting = arrivals[next++][1]
        if (waiting === null || t + 1.5 < due) continue
        sent.push(waiting)
        waiting = null
        due = t >= due + 1000 / 60 ? t + 1000 / 60 : due + 1000 / 60
      }
      return sent.filter((frame, i) => i > 0 && frame !== sent[i - 1]).length / (sent.length - 1)
    }
    for (const hz of [60, 75, 120, 144, 165, 180, 240]) {
      for (let phase = 0; phase < 1000 / 60; phase += 0.5) expect(handed(hz, phase), `${hz} Hz, phase ${phase}`).toBe(1)
    }
  })

  it('measures the display by its frames, a frame the page was late for not counting', () => {
    expect(displayFrameMs([])).toBeCloseTo(1000 / 60)
    const at75 = 1000 / 75
    expect(displayFrameMs([at75, at75, 3 * at75, at75, at75])).toBeCloseTo(at75)
  })

  it('draws at once after a pause', () => {
    expect(mapFrameDue(5000, 100, 1000 / 144)).toBe(true)
  })
})
