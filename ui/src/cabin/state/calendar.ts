/**
 * The Calendar's day and icon, worked out (calendarIcon.ts keeps them current). Its day, as the owner chose
 * (2026-11-03): the weekday the game's (its clock's day, day 0 a Monday; the PC's while no game runs), the day of
 * the month and the month the PC's (the game has no dates).
 */

/** The Calendar's weekday, from Monday (0): the game's day's (`gameDay`, its clock's days since day 0, a Monday),
 * or the PC's `date`'s while there's no game. */
export function calendarWeekday(gameDay: number | null, date: Date): number {
  // The PC's: Sunday 0, so Monday's 1 to 0.
  return gameDay === null ? (date.getDay() + 6) % 7 : ((gameDay % 7) + 7) % 7
}

/** Apple's red (iOS 26's, shell/apple.css `--apple-red`), each look's, and the day's number's colour on the card. */
const RED = { light: 'rgb(255 56 60)', dark: 'rgb(255 66 69)' }
const NUMBER = { light: '#1c1c1e', dark: '#ffffff' }
const FONT = "'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif"

/** The card (tools/dev/icons.py) with the day written on it, as Apple's icon has it: the weekday short, red, over
 * the day's number. */
export function calendarIconSvg(cardSvg: string, look: 'light' | 'dark', weekday: string, day: number): string {
  const escape = (value: string) => value.replace(/&/g, '&amp;').replace(/</g, '&lt;')
  const date =
    `<text x="512" y="318" text-anchor="middle" font-family="${FONT}" font-size="168" font-weight="600" ` +
    `fill="${RED[look]}">${escape(weekday)}</text>` +
    `<text x="512" y="806" text-anchor="middle" font-family="${FONT}" font-size="500" font-weight="300" ` +
    `letter-spacing="-12" fill="${NUMBER[look]}">${day}</text>`
  return cardSvg.replace('<!-- date -->', date)
}
