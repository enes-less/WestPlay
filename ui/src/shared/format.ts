const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

function minuteOfDay(gameTime: number): number {
  return ((gameTime % 1440) + 1440) % 1440
}

/** "14:22" — the clock as shown in the sidebar. */
export function formatGameTime(gameTime: number): string {
  const minutes = minuteOfDay(gameTime)
  return `${String(Math.floor(minutes / 60)).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}`
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

/** Truck speed (m/s, negative when reversing) as whole km/h. */
export function speedKmh(speed: number): number {
  return Math.round(Math.abs(speed) * 3.6)
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
 * language's decimal separator). Close by, to the nearest 10 m, from 100 m the nearest 50. */
export function formatTripDistance(metres: number, decimal = '.'): string {
  if (metres < 100) return `${Math.max(0, Math.round(metres / 10) * 10)} m`
  if (metres < 975) return `${Math.round(metres / 50) * 50} m`
  if (metres < 9950) return `${(metres / 1000).toFixed(1).replace('.', decimal)} km`
  return `${Math.round(metres / 1000)} km`
}
