import { useEffect, useState } from 'react'
import { cabClock } from '../../shared/host'
import { appIcon } from '../../shared/icons'
import { calendarIconSvg, calendarWeekday } from './calendar'
import { useLive } from './store'

/** The Calendar's day (calendar.ts): `weekday` from Monday (0), and the PC's date. */
export interface CalendarDay {
  weekday: number
  date: Date
}

/** How often the PC's date is looked at again (it changes at midnight). */
const DATE_EVERY_MS = 60_000

/** The Calendar's day, kept current while `on` (each app icon asks; only the Calendar's needs it). */
export function useCalendarDay(on = true): CalendarDay {
  const gameDay = useLive((state) =>
    on && state.telemetry ? Math.floor(cabClock(state.telemetry, state.clockShift) / 1440) : null,
  )
  const [date, setDate] = useState(() => new Date())
  useEffect(() => {
    if (!on) return
    const timer = setInterval(() => {
      const now = new Date()
      setDate((before) => (before.toDateString() === now.toDateString() ? before : now))
    }, DATE_EVERY_MS)
    return () => clearInterval(timer)
  }, [on])
  return { weekday: calendarWeekday(gameDay, date), date }
}

/** The Calendar icon's card (tools/dev/icons.py), each look's, as text. */
const cards = new Map<'light' | 'dark', Promise<string>>()

function card(look: 'light' | 'dark'): Promise<string> {
  let text = cards.get(look)
  if (!text) {
    text = fetch(appIcon('calendar', look)).then((answer) => answer.text())
    cards.set(look, text)
  }
  return text
}

/** The Calendar's icon, today's (`null` until its card has loaded, or while not `on`). */
export function useCalendarIcon(on: boolean, look: 'light' | 'dark', weekday: string, day: number): string | null {
  const [icon, setIcon] = useState<{ url: string } | null>(null)
  useEffect(() => {
    if (!on) return
    let gone = false
    void card(look).then((svg) => {
      if (gone) return
      setIcon({ url: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(calendarIconSvg(svg, look, weekday, day))}` })
    })
    return () => {
      gone = true
    }
  }, [on, look, weekday, day])
  // The day before's, while the new one is made: never the blank card between.
  return on && icon ? icon.url : null
}
