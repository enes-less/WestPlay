const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

function minuteOfDay(gameTime: number): number {
  return ((gameTime % 1440) + 1440) % 1440
}

/**
 * How the cab's clock reads (Settings › Date & Time): 24-hour, or 12-hour with the time of day (`period`: "2:22 PM",
 * the language's way) or without it (the sidebar's "2:22", as iOS's status bar).
 */
export interface ClockStyle {
  hours24: boolean
  period: ((time: string, afternoon: boolean) => string) | null
}

export const HOURS_24: ClockStyle = { hours24: true, period: null }

/** "14:22", or "2:22 PM" in 12-hour time (`style`). */
export function formatGameTime(gameTime: number, style: ClockStyle = HOURS_24): string {
  const minutes = minuteOfDay(gameTime)
  const hour = Math.floor(minutes / 60)
  const minute = String(minutes % 60).padStart(2, '0')
  if (style.hours24) return `${String(hour).padStart(2, '0')}:${minute}`
  const time = `${((hour + 11) % 12) + 1}:${minute}`
  return style.period ? style.period(time, hour >= 12) : time
}

/** "Thu 14:22" */
export function formatGameClock(gameTime: number): string {
  return `${DAYS[Math.floor(gameTime / 1440) % 7]} ${formatGameTime(gameTime)}`
}

/** "3", "N", "R1" */
export function formatGear(displayedGear: number): string {
  if (displayedGear > 0) return String(displayedGear)
  if (displayedGear < 0) return `R${-displayedGear}`
  return 'N'
}

/** A mile and a foot, metres. */
const MILE = 1609.344
const FOOT = 0.3048

/** Truck speed (m/s, negative when reversing) as whole km/h, or whole mph in miles (`mph`: the profile's units). */
export function roadSpeed(speed: number, mph = false): number {
  return Math.round(Math.abs(speed) * (mph ? 3600 / MILE : 3.6))
}

/** How a trip's time is said: in minutes, or hours and minutes ("12 min", "1:05 hrs"). */
export interface TimeWords {
  minutes: (minutes: number) => string
  hours: (hours: number, minutes: string) => string
}

/** A trip's time left (game minutes) as Apple shows it: "12 min", "1:05 hrs". */
export function formatTripTime(minutes: number, words: TimeWords): string {
  const whole = Math.max(0, Math.round(minutes))
  if (whole < 60) return words.minutes(whole)
  return words.hours(Math.floor(whole / 60), String(whole % 60).padStart(2, '0'))
}

/** A distance (metres, as the game counts them) as Apple shows it: "850 m", "9.4 km", "214 km" (with the
 * language's decimal separator). Close by, to the nearest 10 m, from 100 m the nearest 50. In miles (`mph`: the
 * profile's units): "500 ft" under a tenth of a mile (to the nearest 10 ft, from 100 ft the nearest 50), then
 * "0.4 mi", "9.4 mi", "133 mi". */
export function formatTripDistance(metres: number, decimal = '.', mph = false): string {
  if (mph) {
    const feet = metres / FOOT
    if (feet < 100) return `${Math.max(0, Math.round(feet / 10) * 10)} ft`
    const rounded = Math.round(feet / 50) * 50
    if (rounded < (0.1 * MILE) / FOOT) return `${rounded} ft`
    const miles = Math.max(0.1, metres / MILE)
    if (miles < 9.95) return `${miles.toFixed(1).replace('.', decimal)} mi`
    return `${Math.round(miles)} mi`
  }
  if (metres < 100) return `${Math.max(0, Math.round(metres / 10) * 10)} m`
  if (metres < 975) return `${Math.round(metres / 50) * 50} m`
  if (metres < 9950) return `${(metres / 1000).toFixed(1).replace('.', decimal)} km`
  return `${Math.round(metres / 1000)} km`
}
