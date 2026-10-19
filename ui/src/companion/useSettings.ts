import type { IconStyle } from '../shared/customize'
import { call, on } from './bridge'
import { useCallback, useEffect, useState } from 'react'

/** The player's settings. Mirrors `westplay_host::settings::Settings`; the app owns and saves them. */
export interface Settings {
  interaction: {
    key: InteractionKey
    /** Cursor speed multiplier, `SENSITIVITY.min`..`SENSITIVITY.max`. */
    sensitivity: number
  }
  screen: {
    truckStrip: TruckStrip
  }
  /** How the screen looks (Customize): a wallpaper's id and one of the player's icon sets, by name; `null`:
   * none chosen (the cab's default wallpaper, WestPlay's own icons). */
  look: {
    wallpaper: string | null
    iconSet: string | null
    /** Light or dark icons, or as the screen is. */
    iconStyle: IconStyle
  }
  /** The graphics card the screen is drawn on: one the player chose (an `Adapter`'s id), or `null`:
   * WestPlay picks (the game's). */
  graphics: {
    gpu: string | null
  }
}

/** Mirrors `westplay_host::settings::TruckStrip`: the truck's own strip beside WestPlay, kept or not. */
export type TruckStrip = 'auto' | 'off'

/** Mirrors `westplay_host::settings::InteractionKey`: the keys a player can hold to use the screen. */
export type InteractionKey = 'leftAlt' | 'rightCtrl' | 'mouseBack' | 'mouseForward'

export const INTERACTION_KEYS: { value: InteractionKey; label: string }[] = [
  { value: 'leftAlt', label: 'Left Alt' },
  { value: 'rightCtrl', label: 'Right Ctrl' },
  { value: 'mouseBack', label: 'Mouse back button' },
  { value: 'mouseForward', label: 'Mouse forward button' },
]

export function interactionKeyLabel(key: InteractionKey): string {
  return INTERACTION_KEYS.find((option) => option.value === key)?.label ?? key
}

/** Mirrors `westplay_host::settings::Sensitivity` (the app corrects anything outside it). */
export const SENSITIVITY = { min: 0.5, max: 3, step: 0.1 }

export interface SettingsState {
  /** `null` until loaded. */
  settings: Settings | null
  /** Saves new settings; the app applies them at once. */
  update(settings: Settings): Promise<void>
  /** Why the last save failed, if it did. */
  error: string | null
}

/** The current settings, kept in step with every change (from this window or another). */
export function useSettings(): SettingsState {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let active = true
    call<Settings>('get_settings').then((loaded) => active && setSettings(loaded))
    const stop = on<Settings>('settings-changed', setSettings)
    return () => {
      active = false
      stop()
    }
  }, [])

  const update = useCallback(async (next: Settings) => {
    setSettings(next)
    try {
      setSettings(await call<Settings>('update_settings', { settings: next }))
      setError(null)
    } catch (e) {
      setError(String(e))
      setSettings(await call<Settings>('get_settings'))
    }
  }, [])

  return { settings, update, error }
}
