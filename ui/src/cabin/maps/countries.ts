/**
 * Which country each square of the map is in, and their time zones, as the map export writes them
 * (crates/mapdata countries.rs, `countries.json` beside the tiles): for the cab's clock (state/timeZone.ts).
 */

/** Central Europe's time zone, minutes: the one the SDK's game time is in. */
const GAME_ZONE = 120

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
}

/** The grid as the map export writes it, `null` if it isn't one. */
export function parseGrid(json: unknown): CountryGrid | null {
  const { cell, x, z, width, height, cells, zones } = (json ?? {}) as {
    cell?: unknown
    x?: unknown
    z?: unknown
    width?: unknown
    height?: unknown
    cells?: unknown
    zones?: Record<string, unknown>
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
  return { cell, x, z, width, height, cells: bytes, zones: zoneMap }
}

/** The country at `x`, `z` (metres): its square's (the map export gives a square without a country's road the
 * nearest's); 0 off the grid or with none near. */
export function countryAt(grid: CountryGrid, x: number, z: number): number {
  const column = Math.floor((x - grid.x) / grid.cell)
  const row = Math.floor((z - grid.z) / grid.cell)
  if (column < 0 || row < 0 || column >= grid.width || row >= grid.height) return 0
  return grid.cells[row * grid.width + column]
}

/** How far the clock at `x`, `z` runs from the game's time, minutes: 0 where the country or its zone isn't
 * known. */
export function clockShiftAt(grid: CountryGrid, x: number, z: number): number {
  const zone = grid.zones.get(countryAt(grid, x, z))
  return zone === undefined ? 0 : zone - GAME_ZONE
}
