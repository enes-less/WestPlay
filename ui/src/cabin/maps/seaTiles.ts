import type maplibregl from 'maplibre-gl'

/**
 * The tiles of open sea alone, which a map's build lists rather than writes (crates/mapdata tiles.rs `SeaTiles`): a
 * map whose roads lie far apart is mostly sea between them, hundreds of thousands of tiles of the same few bytes
 * (a player's map of 380 map mods, 1,906 x 1,389 km, 2026-11-12). The map's tiles come through this protocol: one on
 * its map's list is given its zoom's sea tile (the same bytes such a tile has, kept once), any other fetched as ever.
 * A tile not there is nothing shown there, as MapLibre takes a missing tile. The protocol is added by what makes a
 * map ([`addSeaProtocol`]), so that reading a map's data doesn't load MapLibre.
 */

const PROTOCOL = 'westplay-sea'
const PREFIX = `${PROTOCOL}://`

/** A map's list, as its build writes it: by zoom, its sea tile's file and its rows' runs (row, first, last column). */
export interface SeaIndex {
  zooms: Record<string, { tile: string; rows: [number, number, number][] }>
}

interface Sea {
  /** By zoom, by row, each run's first and last column, one after the other. */
  runs: Map<number, Map<number, number[]>>
  /** By zoom, its sea tile's address, and its bytes once fetched. */
  tiles: Map<number, string>
  bytes: Map<number, Promise<ArrayBuffer>>
}

/** The maps' lists, by the address of their tiles' folder. */
const seas = new Map<string, Sea>()

/** The address MapLibre is given for the tiles in `folder`, through this protocol. */
export function seaTilesUrl(folder: string): string {
  return `${PREFIX}${folder}/{z}/{x}/{y}.pbf`
}

/** Takes in the list `index` of the tiles in `folder`. */
export function addSeaTiles(folder: string, index: SeaIndex): void {
  const runs = new Map<number, Map<number, number[]>>()
  const tiles = new Map<number, string>()
  for (const [zoom, { tile, rows }] of Object.entries(index.zooms)) {
    const z = Number(zoom)
    const byRow = new Map<number, number[]>()
    for (const [y, first, last] of rows) {
      const row = byRow.get(y) ?? []
      row.push(first, last)
      byRow.set(y, row)
    }
    runs.set(z, byRow)
    tiles.set(z, `${folder}/${tile}`)
  }
  seas.set(folder, { runs, tiles, bytes: new Map() })
}

/** Fetches the list at `address` of the tiles in `folder` and takes it in. */
export async function loadSeaTiles(folder: string, address: string): Promise<void> {
  const response = await fetch(address, { cache: 'no-store' })
  if (!response.ok) throw new Error(`${response.status} ${address}`)
  addSeaTiles(folder, (await response.json()) as SeaIndex)
}

function listed(sea: Sea, z: number, x: number, y: number): boolean {
  const row = sea.runs.get(z)?.get(y)
  if (!row) return false
  for (let at = 0; at < row.length; at += 2) if (row[at] <= x && x <= row[at + 1]) return true
  return false
}

/** The bytes MapLibre is given for the tile at `url` (this protocol's address). */
export async function seaTile(url: string, signal?: AbortSignal): Promise<ArrayBuffer> {
  const address = url.startsWith(PREFIX) ? url.slice(PREFIX.length) : url
  const match = /^(.*)\/(\d+)\/(\d+)\/(\d+)\.pbf$/.exec(address)
  const sea = match ? seas.get(match[1]) : undefined
  if (match && sea) {
    const [z, x, y] = [Number(match[2]), Number(match[3]), Number(match[4])]
    const tile = sea.tiles.get(z)
    if (tile && listed(sea, z, x, y)) {
      let bytes = sea.bytes.get(z)
      if (!bytes) {
        bytes = fetch(tile).then((response) => {
          if (!response.ok) throw new Error(`${response.status} ${tile}`)
          return response.arrayBuffer()
        })
        // Fetched again next time if it failed.
        bytes.catch(() => sea.bytes.delete(z))
        sea.bytes.set(z, bytes)
      }
      // A copy: what MapLibre is given goes to its worker, and is gone here.
      return (await bytes).slice(0)
    }
  }
  const response = await fetch(address, { signal })
  if (response.ok) return response.arrayBuffer()
  if (response.status === 404) return new ArrayBuffer(0)
  throw new Error(`${response.status} ${address}`)
}

let added = false

/** Lets MapLibre (`library`) fetch tiles through this protocol: once, before the first map asks for any. */
export function addSeaProtocol(library: typeof maplibregl): void {
  if (added) return
  added = true
  library.addProtocol(PROTOCOL, async (params, abort) => ({ data: await seaTile(params.url, abort.signal) }))
}
