import { describe, expect, it } from 'vitest'
import { clockShiftAt, countryAt, parseGrid, zoneAt } from '../maps/countries'

// Three squares of 1000 m west to east: Germany (5), the square between (the export gives it the nearest's,
// Germany), Finland (23); a square with none near; and their time zones.
const grid = parseGrid({
  cell: 1000,
  x: -1000,
  z: 0,
  width: 4,
  height: 1,
  cells: '05051700',
  zones: { '5': 120, '23': 180, '10': 60 },
})!

describe('the clock where the truck is', () => {
  it("reads the map export's grid", () => {
    expect(grid.cells).toEqual(new Uint8Array([5, 5, 23, 0]))
    expect(parseGrid({ cell: 1000, x: 0, z: 0, width: 2, height: 1, cells: '05' })).toBeNull()
    expect(parseGrid(null)).toBeNull()
  })

  it("finds the country of the truck's square", () => {
    expect(countryAt(grid, -500, 500)).toBe(5)
    expect(countryAt(grid, 1500, 500)).toBe(23)
    expect(countryAt(grid, 500, 500)).toBe(5)
    // None near, and off the grid: none.
    expect(countryAt(grid, 2500, 500)).toBe(0)
    expect(countryAt(grid, 90_000, 500)).toBe(0)
    expect(countryAt(grid, -1500, 500)).toBe(0)
  })

  it("runs an hour on in Finland, as the game's clock, and as the game's time in central Europe", () => {
    expect(clockShiftAt(grid, 1500, 500)).toBe(60)
    expect(clockShiftAt(grid, -500, 500)).toBe(0)
    // Off the grid, or a country without a zone: the game's time.
    expect(clockShiftAt(grid, 90_000, 500)).toBe(0)
  })

  it('takes the zone of a part of a country in another (ATS: El Paso on Mountain time in Texas)', () => {
    // Texas (43) over two squares, El Paso's corner of it on Mountain time.
    const texas = parseGrid({
      cell: 1000,
      x: 0,
      z: 0,
      width: 2,
      height: 1,
      cells: '2b2b',
      zones: { '43': -300 },
      areas: [
        { country: 43, x: [0, 900], z: [0, 1000], zone: -360 },
        { country: 43, x: 'not one' },
      ],
    })!
    expect(texas.areas).toHaveLength(1)
    expect(zoneAt(texas, 500, 500)).toBe(-360)
    expect(zoneAt(texas, 1500, 500)).toBe(-300)
    // A map built before the areas were read: the country's zone.
    expect(grid.areas).toEqual([])
    expect(zoneAt(grid, 1500, 500)).toBe(180)
  })

  it("runs from the game's own base: ATS's clock an hour behind its SDK time in Arizona", () => {
    // Arizona (3), on Mountain Standard Time; ATS's SDK time is -360's (the game's 4:03 PM at the SDK's 17:03).
    const arizona = parseGrid({
      cell: 1000,
      x: 0,
      z: 0,
      width: 1,
      height: 1,
      cells: '03',
      zones: { '3': -420 },
      base: -360,
    })!
    expect(clockShiftAt(arizona, 500, 500)).toBe(-60)
    // A map that says none: ETS2's central Europe.
    expect(grid.base).toBe(120)
  })
})
