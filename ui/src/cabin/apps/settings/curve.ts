/** A smooth line through points, from left to right, never past them (a monotone cubic, Fritsch and Carlson's:
 * flat where two points are level, as at the graph's ends, rather than bulging over them). */
export function smoothThrough(points: [number, number][]): string {
  const slopes = points.slice(1).map(([x, y], i) => (y - points[i][1]) / (x - points[i][0]))
  const tangents = points.map((_, i) => {
    if (i === 0) return slopes[0]
    if (i === points.length - 1) return slopes[i - 1]
    const [before, after] = [slopes[i - 1], slopes[i]]
    return before * after <= 0 ? 0 : (before + after) / 2
  })
  // Kept from overshooting: within three times each span's slope.
  slopes.forEach((slope, i) => {
    if (slope === 0) {
      tangents[i] = 0
      tangents[i + 1] = 0
      return
    }
    const [a, b] = [tangents[i] / slope, tangents[i + 1] / slope]
    const length = Math.hypot(a, b)
    if (length > 3) {
      tangents[i] = (3 / length) * a * slope
      tangents[i + 1] = (3 / length) * b * slope
    }
  })
  let d = `M${points[0][0]},${points[0][1]}`
  for (let i = 0; i < points.length - 1; i++) {
    const [[x1, y1], [x2, y2]] = [points[i], points[i + 1]]
    const third = (x2 - x1) / 3
    d += ` C${x1 + third},${y1 + tangents[i] * third} ${x2 - third},${y2 - tangents[i + 1] * third} ${x2},${y2}`
  }
  return d
}
