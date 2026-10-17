export type Appearance = 'dark' | 'light'

/** `auto` follows the truck like a car's light sensor does; `dark`/`light` are fixed. */
export type AppearanceMode = 'auto' | Appearance

/** Game-clock window (minutes since midnight) in which `auto` is dark. */
export const NIGHT_STARTS = 20 * 60
export const NIGHT_ENDS = 6 * 60 + 30

export interface AppearanceConditions {
  /** Game time in minutes since day 0 (from telemetry), or `null` when the game isn't connected. */
  gameTime: number | null
  lowBeam: boolean
}

export function isNight(gameTime: number): boolean {
  const minuteOfDay = ((gameTime % 1440) + 1440) % 1440
  return minuteOfDay >= NIGHT_STARTS || minuteOfDay < NIGHT_ENDS
}

/** What `auto` responds to: daylight (light), or night, headlights or no game (dark). */
export type AutoReason = 'daylight' | 'night' | 'headlights' | 'no-game'

export function autoReason(conditions: AppearanceConditions): AutoReason {
  if (conditions.gameTime === null) return 'no-game'
  if (isNight(conditions.gameTime)) return 'night'
  return conditions.lowBeam ? 'headlights' : 'daylight'
}

/**
 * The appearance the UI should use. `auto` is dark at night or whenever the headlights are on (dusk,
 * rain, tunnels), the way cars switch CarPlay with their light sensor. Without a game it stays dark: the
 * screen sits in a cab.
 */
export function resolveAppearance(mode: AppearanceMode, conditions: AppearanceConditions): Appearance {
  if (mode !== 'auto') return mode
  return autoReason(conditions) === 'daylight' ? 'light' : 'dark'
}
