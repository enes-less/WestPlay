/**
 * Which country each square of the map is in, and their time zones, as the map export writes them
 * (crates/mapdata countries.rs, `countries.json` beside the tiles): for the cab's clock (state/timeZone.ts).
 */

/** Central Europe's time zone, minutes: the one ETS2's SDK game time is in (a map's before WestPlay wrote its game's). */
const ETS2_ZONE = 120

/** The map's countries on a grid (`countries.json`), and their time zones. */
export interface CountryGrid {
  cell: number
  x: number
  z: number
  width: number
  height: number
  /** Each square's country id, row by row from the north west; 0 for none near. */
  cells: Uint8Array
  /** Each country's time zone, minutes, by id. */
  zones: Map<number, number>
  /** Parts of countries in another time zone than the rest of theirs (ATS's Navajo Nation, El Paso). */
  areas: ZoneArea[]
  /** The zone the SDK's game time is in, minutes: the game's (ETS2's 120, ATS's -360). */
  base: number
}

/** A part of a country in another time zone: its rectangle (west and east x, north and south z, metres). */
export interface ZoneArea {
  country: number
  x: [number, number]
  z: [number, number]
  zone: number
}

/** The grid as the map export writes it, `null` if it isn't one. */
export function parseGrid(json: unknown): CountryGrid | null {
  const { cell, x, z, width, height, cells, zones, areas, base } = (json ?? {}) as {
    cell?: unknown
    x?: unknown
    z?: unknown
    width?: unknown
    height?: unknown
    cells?: unknown
    zones?: Record<string, unknown>
    areas?: unknown
    base?: unknown
  }
  if (
    typeof cell !== 'number' ||
    typeof x !== 'number' ||
    typeof z !== 'number' ||
    typeof width !== 'number' ||
    typeof height !== 'number' ||
    typeof cells !== 'string' ||
    cells.length !== width * height * 2
  ) {
    return null
  }
  const bytes = new Uint8Array(width * height)
  for (let i = 0; i < bytes.length; i++) bytes[i] = parseInt(cells.slice(i * 2, i * 2 + 2), 16) || 0
  const zoneMap = new Map(Object.entries(zones ?? {}).map(([id, minutes]) => [Number(id), Number(minutes)]))
  // Each area that is one; none from a map built before WestPlay read them.
  const isArea = (area: unknown): area is ZoneArea => {
    const { country, x, z, zone } = (area ?? {}) as Partial<ZoneArea>
    const pair = (value: unknown) => Array.isArray(value) && value.length === 2 && value.every(Number.isFinite)
    return Number.isFinite(country) && pair(x) && pair(z) && Number.isFinite(zone)
  }
  return {
    cell,
    x,
    z,
    width,
    height,
    cells: bytes,
    zones: zoneMap,
    areas: Array.isArray(areas) ? areas.filter(isArea) : [],
    base: typeof base === 'number' ? base : ETS2_ZONE,
  }
}

/** The country at `x`, `z` (metres): its square's (the map export gives a square without a country's road the
 * nearest's); 0 off the grid or with none near. */
export function countryAt(grid: CountryGrid, x: number, z: number): number {
  const column = Math.floor((x - grid.x) / grid.cell)
  const row = Math.floor((z - grid.z) / grid.cell)
  if (column < 0 || row < 0 || column >= grid.width || row >= grid.height) return 0
  return grid.cells[row * grid.width + column]
}

/** The time zone at `x`, `z` (minutes): of the part of its country there in another zone, else of its country;
 * `undefined` where neither is known (as the map export's `Countries::zone_at`). */
export function zoneAt(grid: CountryGrid, x: number, z: number): number | undefined {
  const country = countryAt(grid, x, z)
  const area = grid.areas.find(
    (each) => each.country === country && x >= each.x[0] && x <= each.x[1] && z >= each.z[0] && z <= each.z[1],
  )
  return area ? area.zone : grid.zones.get(country)
}

/** How far the clock at `x`, `z` runs from the game's time, minutes: 0 where the country or its zone isn't
 * known. */
export function clockShiftAt(grid: CountryGrid, x: number, z: number): number {
  const zone = zoneAt(grid, x, z)
  return zone === undefined ? 0 : zone - grid.base
}
