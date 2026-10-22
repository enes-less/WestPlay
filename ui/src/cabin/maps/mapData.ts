import { useEffect, useState } from 'react'
import { create } from 'zustand'
import { isInHost } from '../../shared/host'

/**
 * The map the Maps app draws, built from the player's own game files (crates/mapdata, built by the app for the
 * profile they play: crates/host maps.rs): vector tiles, and a small manifest with their zooms and bounds,
 * where the map starts, and the place names.
 *
 * In WestPlay the host names the map's folder (`map` messages), served at `https://westplay.maps/`; it names
 * another when the player plays another profile (another map, other map mods), and none until a map is built.
 * A plain browser (development) has the map exported into public/dev-map/ (git-ignored, never shipped).
 */
const DEV_MAP = '/dev-map'
const MAPS = 'https://westplay.maps'

export interface MapLabel {
  /** The place's own name, in its own language and script. */
  name: string
  /** Its name in the player's language (the game's), when that's different. */
  localized?: string
  kind: string
  /** lng, lat */
  at: [number, number]
  /** How many people live there: bigger places' names win when they'd overlap (0: none given). */
  population: number
}

export interface MapData {
  /** The tiles' address, with {z}/{x}/{y}. */
  tiles: string
  /** The road number signs' sprite sheet (its address without `.png`/`.json`), `null` without one. */
  signs: string | null
  minzoom: number
  maxzoom: number
  /** west, south, east, north */
  bounds: [number, number, number, number]
  /** Where the map starts (lng, lat), for when there is no truck to follow. */
  centre: [number, number]
  labels: MapLabel[]
  /** Where the player can be routed (places.ts): the file's address, `null` without one. */
  places: string | null
  /** Which country each part of the map is in, and their time zones (state/timeZone.ts): the file's address,
   * `null` without one (a map built before WestPlay read them). */
  countries: string | null
}

/** Where the map is: its folder's address (absolute: MapLibre fetches tiles from its worker), and which
 * build of it (a map built again keeps its folder); `null`: no map, `undefined`: not known yet (the host
 * hasn't said). */
const useMapFolder = create<{ base: string | null | undefined; built: number | null }>()(() => ({
  base: firstBase(),
  built: null,
}))

/** In WestPlay, not known until the host says; in a browser, the development map; elsewhere (tests), none. */
function firstBase(): string | null | undefined {
  if (typeof window === 'undefined') return null
  return isInHost() ? undefined : `${location.origin}${DEV_MAP}`
}

/** The host names the map's folder in its maps, and when it was built (`null`: none). */
export function receiveMapFolder(folder: string | null, built: number | null): void {
  useMapFolder.setState({ base: folder ? `${MAPS}/${encodeURIComponent(folder)}` : null, built })
}

/** The onboarding's demo names the map's address itself (served by the companion, not the host). */
export function showMapAt(base: string | null, built: number | null): void {
  useMapFolder.setState({ base, built })
}

const loading = new Map<string, Promise<MapData | null>>()

/** The map at `base`, each build of it loaded once. */
function load(base: string, built: number | null): Promise<MapData | null> {
  const key = `${base}#${built ?? ''}`
  let map = loading.get(key)
  if (!map) {
    map = fetch(`${base}/map.json`, { cache: 'no-store' })
      .then((response) => (response.ok ? response.json() : null))
      .then((manifest) =>
        manifest
          ? {
              // The export's own folder (each export writes a new one), or `tiles` from before it named one.
              tiles: `${base}/${manifest.tiles ?? 'tiles'}/{z}/{x}/{y}.pbf`,
              signs: manifest.signs ? `${base}/${manifest.tiles}/${manifest.signs}` : null,
              minzoom: manifest.minzoom,
              maxzoom: manifest.maxzoom,
              bounds: manifest.bounds,
              centre: manifest.centre,
              labels: manifest.labels ?? [],
              places: manifest.places ? `${base}/${manifest.places}` : null,
              countries: manifest.countries ? `${base}/${manifest.countries}` : null,
            }
          : null,
      )
      .catch(() => null)
    loading.set(key, map)
  }
  return map
}

/** The map shown now: `null` if there is none, `undefined` until it's known. */
export function loadMapData(): Promise<MapData | null | undefined> {
  const { base, built } = useMapFolder.getState()
  return base === null || base === undefined ? Promise.resolve(base) : load(base, built)
}

/** Calls `listener` whenever the map shown changes (another map, another build of it, or the host first
 * saying which). Returns the function that stops it. */
export function onMapChange(listener: () => void): () => void {
  return useMapFolder.subscribe(listener)
}

// Fetched as soon as the page knows the map, so it's ready by the time a screen shows it.
useMapFolder.subscribe((state) => {
  if (state.base) load(state.base, state.built)
})
void loadMapData()

/** The map, `null` if there is none, `undefined` while it loads (or until the host says which). Another
 * map (another profile played) comes as it loads. */
export function useMapData(): MapData | null | undefined {
  const base = useMapFolder((state) => state.base)
  const built = useMapFolder((state) => state.built)
  const key = `${base}#${built ?? ''}`
  const [loaded, setLoaded] = useState<{ key: string; data: MapData | null } | null>(null)
  useEffect(() => {
    if (!base) return
    let current = true
    load(base, built).then((data) => current && setLoaded({ key, data }))
    return () => {
      current = false
    }
  }, [base, built, key])
  if (base === null || base === undefined) return base
  return loaded?.key === key ? loaded.data : undefined
}
