import { useState } from 'react'
import type { HostStatus, MapStatus } from './useHostStatus'

/** Whether a map's preparation was stopped by the game starting (`waitingForGame` with `stopped`), any profile's. */
export function gameStoppedMap(status: HostStatus | null): boolean {
  if (!status) return false
  const stopped = (map: MapStatus) => map.state === 'waitingForGame' && map.stopped === true
  return stopped(status.map) || status.profileMaps.some((profile) => stopped(profile.status))
}

/** The sheet saying so: whether it's up, and whether a map was stopped when last looked. */
export interface StoppedSheet {
  shown: boolean
  was: boolean
}

/**
 * The sheet once a map's stopped is `stopped`: put up each time one is stopped (or at the first look while one
 * is), kept up until it's closed even once it's preparing again (the game closed before the player looked).
 */
export function nextSheet(sheet: StoppedSheet, stopped: boolean): StoppedSheet {
  if (stopped === sheet.was) return sheet
  return { was: stopped, shown: sheet.shown || stopped }
}

/** Whether the sheet is up ([`nextSheet`] by the host's status), and its close. */
export function useGameStopped(status: HostStatus | null): [boolean, () => void] {
  const stopped = gameStoppedMap(status)
  const [sheet, setSheet] = useState<StoppedSheet>({ shown: false, was: false })
  const next = nextSheet(sheet, stopped)
  // Kept as React keeps what it learned from a render before (set while rendering, rendered again at once).
  if (next !== sheet) setSheet(next)
  return [next.shown, () => setSheet((now) => ({ ...now, shown: false }))]
}
