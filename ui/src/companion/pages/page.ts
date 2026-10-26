import { useEffect, useState } from 'react'
import { checkGame, type GameCheck, type SetupState } from '../setup'
import type { SignedIn } from '../accounts'
import type { Customizations } from '../../shared/customize'
import type { Offer } from '../update'
import type { MapProgress } from '../mapProgress'
import type { HostStatus } from '../useHostStatus'
import type { SettingsState } from '../useSettings'
import type { CompanionWords } from './words'

/** What each of the companion's pages is given. */
export interface PageProps {
  status: HostStatus | null
  settings: SettingsState
  setup: SetupState
  cw: CompanionWords
  /** What a check of the game found (the Setup and Map pages show it), once it's been checked. */
  check: GameCheck | null
  /** The maps being prepared, as the Map page's bar and the sidebar show them (mapProgress.ts). */
  mapProgress: MapProgress
  checked(check: GameCheck): void
  /** Whether each service is signed in, as last known (`null`: being found out). */
  signedIn: SignedIn
  /** Opens the uninstall sheet. */
  uninstall(): void
  /** The player's wallpapers and icon sets (`null` until listed), and listing them again after a change. */
  custom: Customizations | null
  refreshCustom(): void
  /** The update found, if one is, and its sheet. */
  update: Offer | null
  showUpdate(): void
  /** The tutorial again, from its start. */
  tutorial(): void
}

/** Where WestPlay is now, from the engine's status: the list's line under its name, the Screen page's title. */
export type ScreenState = 'starting' | 'stopped' | 'noGame' | 'unsupported' | 'notInCab' | 'live' | 'original'

export function screenState(status: HostStatus | null): ScreenState {
  if (!status) return 'starting'
  if (status.error) return 'stopped'
  if (!status.gameConnected) return 'noGame'
  // The player's truck isn't one WestPlay shows in (the plugin's say: its screen the game's own, or none of
  // WestPlay's at all), whether in its cab or not.
  if (status.truckSupported === false) return 'unsupported'
  if (!status.screenDrawing) return 'notInCab'
  return status.westplayOpen === false ? 'original' : 'live'
}

/** The note for a map waiting for free memory: how much it needs and how much is free, in gigabytes as the
 * player's language writes them. */
export function memoryNote(cw: CompanionWords, locale: string, needed: number, free: number): string {
  const gb = (bytes: number) => new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(bytes / 1e9)
  return cw.map.memory.note.text.replace('{needed}', gb(needed)).replace('{free}', gb(free))
}

/** Checks the game the first time a page that shows it opens (the check is kept for the others); the error,
 * if it couldn't be checked. */
export function useGameCheck({ setup, check, checked, cw }: PageProps): string | null {
  const [error, setError] = useState<string | null>(null)
  const dir = setup.game?.dir
  useEffect(() => {
    if (check || !dir) return
    let active = true
    checkGame(dir)
      .then((result) => active && checked(result))
      .catch((e: unknown) => active && setError(String(e)))
    return () => {
      active = false
    }
  }, [dir, check, checked])
  // No game set up: nothing to check (rather than waiting for it).
  return dir ? error : cw.values.noGame
}
