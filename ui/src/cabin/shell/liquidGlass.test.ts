import { describe, expect, it } from 'vitest'
import { drawnIn, edgeOf, lensFor, OUT, profile, rimLight, rimNumbers, shiftAt, surface, tintOf } from './liquidGlass'

describe('the Liquid Glass lens', () => {
  it('measures a point in from the rounded edge, with the way out from it', () => {
    // A 400 x 76 capsule: the middle of its top edge, of its left end, and a point 10 px in from the left end.
    expect(edgeOf(200, 0, 400, 76, 38)).toMatchObject({ distance: 0, nx: 0, ny: -1 })
    const end = edgeOf(0, 38, 400, 76, 38)
    expect(end.distance).toBeCloseTo(0)
    expect(end.nx).toBeCloseTo(-1)
    expect(edgeOf(10, 38, 400, 76, 38).distance).toBeCloseTo(10)
    // Square corners: the nearer side's distance.
    expect(edgeOf(5, 30, 200, 100, 0)).toMatchObject({ distance: 5, nx: -1, ny: 0 })
  })

  it('rounds its rim over as a convex squircle: no height at the edge, all of it and level at the middle', () => {
    expect(surface(0)).toBe(0)
    expect(surface(1)).toBe(1)
    // Convex: above the straight line from the edge to the middle, rising all the way.
    for (let t = 0.05; t < 1; t += 0.05) {
      expect(surface(t)).toBeGreaterThan(t)
      expect(surface(t + 0.05)).toBeGreaterThanOrEqual(surface(t))
    }
    expect(surface(0.999)).toBeCloseTo(1, 6)
  })

  it('bends a ray by Snell’s law across the glass under it', () => {
    const lens = { bezel: 28, thick: 40 }
    // Half way across the rim, by hand: the slope there, the ray turned on entering glass of index 1.5, and the
    // glass's height there times the tangent of that turn.
    const t = 0.5
    const height = (1 - (1 - t) ** 4) ** 0.25
    const slope = ((1 - t) ** 3 * (1 - (1 - t) ** 4) ** -0.75 * lens.thick) / lens.bezel
    const incidence = Math.atan(slope)
    const turned = incidence - Math.asin(Math.sin(incidence) / 1.5)
    expect(shiftAt(t, lens)).toBeCloseTo(lens.thick * height * Math.tan(turned), 3)
    // Nothing where the glass is flat, and (next to) nothing at the very edge, where there's no glass under the ray.
    expect(shiftAt(1, lens)).toBeCloseTo(0, 3)
    expect(shiftAt(0, lens)).toBeLessThan(shiftAt(0.03, lens) / 2)
  })

  it('shows what is further inside: most just in from the edge, falling away to the flat middle', () => {
    const lens = lensFor(104, 104, 52)
    const { shifts, most } = profile(lens, 256)
    const peak = shifts.indexOf(most)
    // Apple's bar (104 px: a 28 px rim, 40 high) bends by up to about 19.7 px.
    expect(lens.bezel).toBeCloseTo(28, 0)
    expect(most).toBeGreaterThan(19)
    expect(most).toBeLessThan(20.5)
    expect(peak / shifts.length).toBeLessThan(0.1)
    for (let i = peak; i < shifts.length - 1; i++) expect(shifts[i + 1]).toBeLessThanOrEqual(shifts[i])
    expect(shifts[shifts.length - 1]).toBeLessThan(0.01)
    for (const shift of shifts) expect(shift).toBeGreaterThanOrEqual(0)
  })

  it('never reaches past the glass: the bend stays inside its own box', () => {
    for (const [w, h, r] of [
      [40, 40, 20],
      [64, 64, 32],
      [76, 76, 38],
      [408, 76, 38],
      [139, 688, 62],
      [320, 240, 26],
      [500, 600, 30],
      [1200, 300, 40],
      [20, 300, 4],
    ]) {
      const lens = lensFor(w, h, r)
      expect(lens.bezel).toBeLessThanOrEqual(Math.min(w, h) / 2)
      // The furthest it draws from: the point's own depth plus its shift, short of the far side.
      const { shifts } = profile(lens, 256)
      shifts.forEach((shift, i) => {
        expect(((i + 0.5) / shifts.length) * lens.bezel + shift).toBeLessThan(Math.min(w, h))
      })
    }
  })

  it('keeps the rim no wider than the corners are round, so two sides’ bends never meet in a crease', () => {
    expect(lensFor(500, 600, 30).bezel).toBeLessThanOrEqual(30)
    expect(lensFor(1200, 300, 40).bezel).toBeLessThanOrEqual(40)
  })

  it('makes larger glass thicker: a wider rim, higher glass, a greater bend', () => {
    const small = lensFor(76, 76, 38)
    const large = lensFor(139, 688, 62)
    expect(large.bezel).toBeGreaterThan(small.bezel)
    expect(large.thick).toBeGreaterThan(small.thick)
    expect(profile(large).most).toBeGreaterThan(profile(small).most)
  })
})

describe('CarPlay’s own glass', () => {
  it('draws in what is beyond its edge, easing to nothing where the rim meets the middle', () => {
    expect(drawnIn(0)).toBe(OUT)
    expect(drawnIn(1)).toBe(0)
    // Level where it ends (no line where the band meets the body), and never a fold: what a point shows moves on with
    // the point.
    expect(drawnIn(0.99)).toBeLessThan(0.01)
    const bezel = 36
    for (let d = 0; d < bezel; d += 0.5) {
      const shown = (at: number) => at - drawnIn(at / bezel)
      expect(shown(d + 0.5)).toBeGreaterThan(shown(d))
    }
  })
})

describe('the rim’s light', () => {
  const corner = Math.SQRT1_2
  it('is most where the edge faces the light and opposite it, some all round, fading in from the edge', () => {
    const facing = rimLight(0, -corner, -corner, false)
    const far = rimLight(0, corner, corner, false)
    const between = rimLight(0, corner, -corner, false)
    expect(facing).toBeCloseTo(1)
    expect(far).toBeLessThan(facing)
    expect(far).toBeGreaterThan(between)
    expect(between).toBeGreaterThan(0)
    expect(rimLight(0, 0, -1, false)).toBeGreaterThan(between)
    expect(rimLight(3, -corner, -corner, false)).toBeLessThan(0.01)
    for (let d = 0; d < 4; d += 0.25) {
      expect(rimLight(d + 0.25, -corner, -corner, false)).toBeLessThan(rimLight(d, -corner, -corner, false))
    }
  })

  it('is shade on a control’s arcs between the light, and never on the arcs that face it', () => {
    expect(rimLight(0.5, corner, -corner, true)).toBeLessThan(0)
    expect(rimLight(0.5, -corner, corner, true)).toBeLessThan(0)
    expect(rimLight(0.5, -corner, -corner, true)).toBeGreaterThan(0.5)
    expect(rimLight(0.5, corner, -corner, false)).toBeGreaterThan(0)
  })

  it('reads a tint as the page works it out', () => {
    expect(tintOf('rgba(52, 50, 58, 0.6)')).toEqual({ light: (52 + 50 + 58) / 3 / 255, solid: 0.6 })
    expect(tintOf('rgb(28, 28, 30)').solid).toBe(1)
    expect(tintOf('rgba(255, 255, 255, 0)')).toEqual({ light: 1, solid: 0 })
    expect(tintOf('transparent')).toEqual({ light: 0, solid: 0 })
  })

  it('makes up for the tint drawn over it', () => {
    // No tint: as asked. Under a tint that lets 40% through, the light and the shade are 1 / 0.4 of what's asked,
    // the shade never more than all.
    expect(rimNumbers(1, 0.2, { light: 0, solid: 0 })).toEqual({ gain: 1, shade: 0.2 })
    const tinted = rimNumbers(1, 0.3, { light: 0.2, solid: 0.6 })
    expect(tinted.gain).toBeCloseTo(2.5)
    expect(tinted.shade).toBeCloseTo(0.75)
    expect(rimNumbers(1, 0.5, { light: 0.2, solid: 0.6 }).shade).toBe(1)
    // All but solid: bounded.
    expect(rimNumbers(1, 0.2, { light: 1, solid: 1 }).gain).toBeLessThanOrEqual(3)
    // Only what's behind is lit, whatever the tint's own lightness: the light keeps the backdrop's colour.
    expect(rimNumbers(1, 0.2, { light: 1, solid: 0.5 })).toEqual(rimNumbers(1, 0.2, { light: 0, solid: 0.5 }))
  })
})
