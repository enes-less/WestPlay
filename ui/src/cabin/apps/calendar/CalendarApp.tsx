import { useCalendarDay } from '../../state/calendarIcon'
import { useText } from '../../state/text'
import './calendar.css'

/** A Monday (1 January 2024), to name a weekday in the cab's language. */
const A_MONDAY = new Date(2024, 0, 1)

/**
 * The Calendar app, in Apple's Calendar look: today at the top, the weekday in Calendar's red over the date, and
 * the day's events below. Its day is the game's weekday on the PC's date (owner 2026-11-03: the game has no
 * dates); its events none (owner: nothing from the game goes in it), so the day says so as Apple's does.
 */
export function CalendarApp() {
  const text = useText()
  const today = useCalendarDay()
  const weekday = new Date(A_MONDAY.getFullYear(), A_MONDAY.getMonth(), A_MONDAY.getDate() + today.weekday)
  const weekdayName = new Intl.DateTimeFormat(text.locale, { weekday: 'long' }).format(weekday)
  const date = new Intl.DateTimeFormat(text.locale, { day: 'numeric', month: 'long' }).format(today.date)
  return (
    <section className="app-panel calendar-app" aria-label={text.apps.calendar}>
      <header className="cal-head">
        <div className="cal-weekday">{weekdayName}</div>
        <h1 className="cal-date">{date}</h1>
      </header>
      <p className="cal-empty">{text.noEvents}</p>
    </section>
  )
}
