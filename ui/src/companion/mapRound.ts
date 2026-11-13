import type { HostStatus, MapStatus, ProfileMap } from './useHostStatus'

/**
 * A round of maps being prepared: the maps waiting or being made since the round began (each by [`mapKey`]), in
 * the order first seen. Prepare Maps Again makes every profile's map, so all of them; a profile whose mods changed,
 * just its own. The Map page's bar shows the round, each map an equal share (ten maps: a tenth each), however many
 * profiles play it: the host lists a map once for each of its profiles, and counted so, eleven profiles on one map
 * and one on another put the bar at 92% once the first was made, the second's whole build then a twelfth of it
 * (review of 2026-11-11, players reporting "stuck at 92%").
 */
export type MapRound = string[]

/** Waiting its turn (for the game to close, or memory too) or being made: not done in this round yet. */
export function pending(status: MapStatus): boolean {
  return (
    status.state === 'building' ||
    status.state === 'queued' ||
    status.state === 'waitingForGame' ||
    status.state === 'waitingForMemory'
  )
}

/** A map, told apart from the others: its folder (profiles sharing a map share it), else its profile's (none
 * known), and its game and ProMods' or not (TruckersMP's maps, ETS2's, ATS's and ProMods', all have TruckersMP's
 * installation for their profile: ATS's being made counted as ETS2's, done). */
export function mapKey(map: ProfileMap): string {
  return `${map.game}|${map.folder || map.dir}|${map.promods ? 'promods' : ''}`
}

/** The round after the host's latest word on each profile's map: those pending join it; none pending, it's over. */
export function nextRound(round: MapRound, maps: ProfileMap[]): MapRound {
  const now = maps.filter((each) => pending(each.status)).map(mapKey)
  if (now.length === 0) return []
  return [...round, ...now.filter((key, at) => !round.includes(key) && now.indexOf(key) === at)]
}

/**
 * How far the round is, 0 to 1: each of its profiles an equal share, a map done (made, or failed) whole, the
 * one being made by `making` (its own progress, as shown), one waiting nothing. `null` without a round.
 */
export function roundProgress(round: MapRound, maps: ProfileMap[], making: number): number | null {
  if (round.length === 0) return null
  const share = (key: string) => {
    const status = maps.find((each) => mapKey(each) === key)?.status
    if (!status) return 1
    if (status.state === 'building') return making
    return pending(status) ? 0 : 1
  }
  return round.reduce((sum, key) => sum + share(key), 0) / round.length
}

/** The round as the companion has followed it, across every status the host has sent. */
let current: MapRound = []

/** Each profile's map; before any profile (a new player), the played one's. */
function mapsOf(status: HostStatus): ProfileMap[] {
  return status.profileMaps.length ? status.profileMaps : [{ game: 'ets2', dir: '', folder: '', status: status.map }]
}

/** Follows the round with the host's latest status (`useHostStatus` calls it with each). */
export function followRound(status: HostStatus): void {
  current = nextRound(current, mapsOf(status))
}

/** How far the round followed is, the map being made at `making` (`null`: no round). */
export function roundShown(status: HostStatus | null, making: number): number | null {
  return status ? roundProgress(current, mapsOf(status), making) : null
}

/**
 * Whether a build's word on how far it is went back (`last`, then `done`): it started over, the same map built again
 * from its start (a map mod left out of it; the game closed after it stopped one). The bar starts over with it: held at
 * its old peak, it sat still through the whole new build (review of 2026-11-11, minor 31). Within one build the host's
 * word only goes up.
 */
export function startedOver(last: number | null, done: number | null): boolean {
  return last !== null && done !== null && done < last
}
