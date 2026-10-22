import { describe, expect, it } from 'vitest'
import { smoothThrough } from './curve'

/** The path's points along each of its cubic spans (`M x,y C … x,y`), sampled finely. */
function sampled(path: string): [number, number][] {
  const numbers = path.match(/-?\d+(\.\d+)?/g)!.map(Number)
  let at: [number, number] = [numbers[0], numbers[1]]
  const points: [number, number][] = [at]
  for (let i = 2; i < numbers.length; i += 6) {
    const [c1, c2, end] = [
      [numbers[i], numbers[i + 1]],
      [numbers[i + 2], numbers[i + 3]],
      [numbers[i + 4], numbers[i + 5]],
    ]
    for (let t = 0.05; t <= 1.0001; t += 0.05) {
      const u = 1 - t
      const point = [0, 1].map(
        (k) => u * u * u * at[k] + 3 * u * u * t * c1[k] + 3 * u * t * t * c2[k] + t * t * t * end[k],
      ) as [number, number]
      points.push(point)
    }
    at = end as [number, number]
  }
  return points
}

describe("the equalizer's curve", () => {
  it('goes through every point, and never past the highest or lowest', () => {
    // The graph's: flat to the edges, the bands at 100, 300 and 500.
    const points: [number, number][] = [
      [0, 60],
      [100, 60],
      [300, 130],
      [500, 200],
      [600, 200],
    ]
    const path = smoothThrough(points)
    for (const [x, y] of points) expect(path).toContain(`${x},${y}`)
    const ys = sampled(path).map(([, y]) => y)
    expect(Math.min(...ys)).toBeGreaterThanOrEqual(60 - 1e-9)
    expect(Math.max(...ys)).toBeLessThanOrEqual(200 + 1e-9)
  })

  it('stays level where two points are', () => {
    const ys = sampled(
      smoothThrough([
        [0, 90],
        [100, 90],
        [300, 40],
        [500, 90],
        [600, 90],
      ]),
    ).map(([, y]) => y)
    // From the left edge to the first band: level, no bulge.
    expect(ys.slice(0, 21).every((y) => Math.abs(y - 90) < 1e-9)).toBe(true)
  })
})
