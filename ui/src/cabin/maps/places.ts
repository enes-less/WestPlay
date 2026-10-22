import { useEffect, useState } from 'react'
import { useMapData } from './mapData'
import { metresBetween } from './projection'

/**
 * Where the player can be routed (Maps' search, Destinations): every place on the map, and cities, from the
 * map export (crates/mapdata, `places.json` beside the tiles), each with the map node the game's GPS takes it
 * as.
 *
 * DEVELOPMENT ONLY for now, like the map itself (mapData.ts).
 */

export type DestinationKind =
  | 'fuel'
  | 'service'
  | 'garage'
  | 'dealer'
  | 'company'
  | 'parking'
  | 'hotel'
  | 'recruitment'
  | 'weigh'
  | 'ferry'
  | 'train'
  | 'city'

export interface Destination {
  kind: DestinationKind
  /** Its name (in the player's language, or its own), if it has one; a city's own name. */
  name?: string
  /** A city's name in the player's language, when that's different. */
  localized?: string
  /** The city it's in (in the player's language), and its id (as the save names it). */
  city?: string
  cityId?: string
  /** lng, lat */
  at: [number, number]
  /** The map node the game routes to (its uid, hex). */
  node: string
}

const loading = new Map<string, Promise<Destination[] | null>>()

/** The places of a map (its `places` file), each loaded once. */
function loadPlaces(url: string): Promise<Destination[] | null> {
  let places = loading.get(url)
  if (!places) {
    places = fetch(url)
      .then((response) => (response.ok ? (response.json() as Promise<Destination[]>) : null))
      .catch(() => null)
    loading.set(url, places)
  }
  return places
}

/** The places of the map shown, `null` if there are none, `undefined` while they load (first asked for when
 * needed). */
export function usePlaces(): Destination[] | null | undefined {
  const map = useMapData()
  const url = map?.places ?? null
  const [loaded, setLoaded] = useState<{ url: string; places: Destination[] | null } | null>(null)
  useEffect(() => {
    if (!url) return
    let current = true
    loadPlaces(url).then((places) => current && setLoaded({ url, places }))
    return () => {
      current = false
    }
  }, [url])
  if (map === undefined) return undefined
  if (!url) return null
  return loaded?.url === url ? loaded.places : undefined
}

/** A place's name as shown: a city's in the player's language, else its own (or what it is, `generic`). */
export function nameOf(place: Destination, generic: string): string {
  return place.localized ?? place.name ?? generic
}

/** Text folded for matching: lower case, accents off, the Turkish dotted and dotless i alike. */
export function fold(text: string): string {
  return text.toLocaleLowerCase('tr').normalize('NFD').replace(/\p{M}/gu, '').replace(/ı/g, 'i')
}

const words = (text: string) =>
  fold(text)
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean)

/** How far `place` is from `from` (lng, lat), metres; `Infinity` without a `from`. */
export function distanceTo(place: Destination, from: [number, number] | null): number {
  return from ? metresBetween(place.at, from) : Infinity
}

/** At most this many results. */
const RESULTS = 40

/**
 * The places matching `query`: each of its words starts a word of the place's name, its city, or what it is
 * (`kindNames`, in the player's language: "fuel" finds filling stations). Those whose name starts with the
 * whole query come first (cities before the rest), then those whose name has every word, then the rest
 * (places in a city of that name); each group nearest first (from the truck, `from`).
 */
export function searchPlaces(
  places: readonly Destination[],
  query: string,
  from: [number, number] | null,
  kindNames: Record<DestinationKind, string>,
  distanceOf: (place: Destination) => number = (place) => distanceTo(place, from),
): Destination[] {
  const asked = words(query)
  if (asked.length === 0) return []
  const whole = fold(query.trim())
  const found: { place: Destination; group: number; distance: number }[] = []
  for (const place of places) {
    const own = words(`${place.name ?? ''} ${place.localized ?? ''}`)
    const all = [...own, ...words(place.city ?? ''), ...words(kindNames[place.kind])]
    if (!asked.every((word) => all.some((there) => there.startsWith(word)))) continue
    const names = [place.name, place.localized].filter((name): name is string => !!name).map(fold)
    const group = names.some((name) => name.startsWith(whole))
      ? place.kind === 'city'
        ? 0
        : 1
      : asked.every((word) => own.some((there) => there.startsWith(word)))
        ? 2
        : 3
    found.push({ place, group, distance: distanceOf(place) })
  }
  found.sort((a, b) => a.group - b.group || a.distance - b.distance)
  return found.slice(0, RESULTS).map(({ place }) => place)
}

/** The nearest places of `kind` to `from`, at most `count`. */
export function nearestOfKind(
  places: readonly Destination[],
  kind: DestinationKind,
  from: [number, number] | null,
  count = RESULTS,
): Destination[] {
  return places
    .filter((place) => place.kind === kind)
    .map((place) => ({ place, distance: distanceTo(place, from) }))
    .sort((a, b) => a.distance - b.distance)
    .slice(0, count)
    .map(({ place }) => place)
}

/** The player's own garages (in the cities the save says they own one), their headquarters' first. */
export function ownGarages(
  places: readonly Destination[],
  owned: readonly string[],
  headquarters: string | null,
): Destination[] {
  return places
    .filter((place) => place.kind === 'garage' && place.cityId !== undefined && owned.includes(place.cityId))
    .sort((a, b) => Number(b.cityId === headquarters) - Number(a.cityId === headquarters))
}
