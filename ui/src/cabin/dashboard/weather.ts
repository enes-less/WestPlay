import type { Sky } from '../../shared/host'
import type { Strings } from '../state/strings'

/**
 * The game's weather, for the Dashboard's weather widget, as CarPlay's: where the truck is (the nearest
 * city) and the weather there, with its symbol. The game has two weathers, clear and rain, and turns from
 * one to the other over about a minute (the host reads both: crates/host weather.rs). It has no
 * temperature, so none is shown.
 */
export interface WeatherView {
  place: string | null
  condition: string
  symbol: WeatherSymbol
}

export type WeatherSymbol = 'sun' | 'moon' | 'rain'

/** Night, for a clear sky's symbol: from 21:00 to 6:00 game time (about when the game's sun is down). */
export function isNight(gameTime: number): boolean {
  const hour = Math.floor((((gameTime % 1440) + 1440) % 1440) / 60)
  return hour >= 21 || hour < 6
}

export function weatherView(
  weather: { now: Sky; coming: Sky },
  gameTime: number,
  place: string | null,
  text: Strings,
): WeatherView {
  const turning = weather.now !== weather.coming
  const condition = turning ? text.weatherTurning[weather.coming] : text.weather[weather.now]
  // Rain while it rains, and while it's coming; a clear sky's sun or moon otherwise.
  const rain = weather.now === 'rain' || weather.coming === 'rain'
  return { place, condition, symbol: rain ? 'rain' : isNight(gameTime) ? 'moon' : 'sun' }
}

/** How wet the roads are: dry, damp or wet, and which way it's going (`null`: staying as they are). */
export interface Roads {
  state: 'dry' | 'damp' | 'wet'
  trend: 'drying' | 'gettingWet' | null
  /** Percent, for the meter. */
  wetness: number
}

/** From this much wetness (percent) the roads count as wet, below it damp (anything above none). */
export const WET_FROM = 30

/** The roads, from the game's wetness (percent) and whether it rains or is starting to. */
export function roads(wetness: number, rain: boolean): Roads {
  const state = wetness <= 0 ? 'dry' : wetness < WET_FROM ? 'damp' : 'wet'
  // In the rain they get wetter until they're soaked; after it, any wetness dries.
  const trend = rain ? (wetness < 100 ? 'gettingWet' : null) : wetness > 0 ? 'drying' : null
  return { state, trend, wetness: Math.min(100, Math.max(0, wetness)) }
}

/** The game's next change of weather: when (game time, minutes since day 0) and how far off (game minutes). */
export interface NextChange {
  at: number
  in: number
}

export function nextChange(gameTime: number, changeIn: number | null | undefined): NextChange | null {
  if (changeIn === null || changeIn === undefined || !Number.isFinite(changeIn) || changeIn < 0) return null
  return { at: gameTime + changeIn, in: changeIn }
}
