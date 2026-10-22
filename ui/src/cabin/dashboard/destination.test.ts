import { describe, expect, it } from 'vitest'
import { cabClock, type JobInfo, type Telemetry } from '../../shared/host'
import type { MapLabel } from '../maps/mapData'
import { STRINGS } from '../state/strings'
import { destinationView, formatDeadline, nearestCity } from './destination'

const text = STRINGS.en
// Thursday (day 3), 14:00.
const now = 3 * 1440 + 14 * 60
const telemetry = { gameTime: now, navigationDistance: 214_000, navigationTime: 3 * 3600 } as Telemetry

const job: JobInfo = {
  active: true,
  cargoLoaded: true,
  special: false,
  cargo: 'Tomatoes',
  cargoMass: 18_400,
  sourceCity: 'Bari',
  sourceCompany: 'Agrominta',
  destinationCity: 'Kraków',
  destinationCompany: 'Wielton',
  deliveryTime: now + 4 * 60 + 40,
  plannedDistanceKm: 1480,
  income: 52_000,
}

const label = (name: string, x: number, localized?: string): MapLabel => ({
  name,
  localized,
  kind: 'city',
  at: [x / 111_319.490_793_273_57, 0],
  population: 1,
})

describe('destinationView', () => {
  it('shows a delivery: the company, its city, the deadline and the distance left', () => {
    expect(destinationView(job, null, telemetry, [], text, 0)).toEqual({
      kicker: 'Delivery',
      title: 'Wielton',
      subtitle: 'Kraków',
      detail: 'Arrive by 18:40 · 214 km',
      late: false,
    })
  })

  it('shows times by the cab clock in a Convoy session, and is late by the player’s own time', () => {
    // The player's own 14:00 is the session's 01:11 the next day (11 h 11 min on).
    const convoy = { ...telemetry, multiplayerTimeOffset: 11 * 60 + 11 }
    expect(cabClock(convoy, 0)).toBe(now + 671)
    expect(destinationView(job, null, convoy, [], text, 0)?.detail).toBe('Arrive by 05:51 · 214 km')
    const late = destinationView({ ...job, deliveryTime: now - 10 }, null, convoy, [], text, 0)
    expect(late?.late).toBe(true)
    // A route's arrival, 3 h on by the cab clock.
    expect(destinationView(null, [0, 0], convoy, [label('Bari', 0)], text, 0)?.detail).toBe('04:11 arrival · 3:00 hrs')
    // Out of a session (no offset sent): the game's own time.
    expect(cabClock(telemetry, 0)).toBe(now)
  })

  it('shows times in the time zone where the truck is, and is late by the player’s own time', () => {
    // An hour on (Finland, with the game's time zones on).
    expect(cabClock(telemetry, 60)).toBe(now + 60)
    expect(destinationView(job, null, telemetry, [], text, 60)?.detail).toBe('Arrive by 19:40 · 214 km')
    expect(destinationView({ ...job, deliveryTime: now - 10 }, null, telemetry, [], text, 60)?.late).toBe(true)
    expect(destinationView(null, [0, 0], telemetry, [label('Bari', 0)], text, 60)?.detail).toBe(
      '18:00 arrival · 3:00 hrs',
    )
  })

  it('names the day when the deadline is another day, and says when it has passed', () => {
    expect(destinationView({ ...job, deliveryTime: now + 1440 }, null, telemetry, [], text, 0)?.detail).toBe(
      'Arrive by Fri 14:00 · 214 km',
    )
    const late = destinationView({ ...job, deliveryTime: now - 10 }, null, telemetry, [], text, 0)
    expect(late?.detail).toBe('Late · 214 km')
    expect(late?.late).toBe(true)
  })

  it('shows the pickup with the cargo before it is loaded', () => {
    expect(destinationView({ ...job, cargoLoaded: false }, null, telemetry, [], text, 0)).toEqual({
      kicker: 'Pickup',
      title: 'Agrominta',
      subtitle: 'Bari',
      detail: 'Tomatoes · 18.4 t',
      late: false,
    })
  })

  it('names a special transport by its city alone', () => {
    const special = { ...job, special: true, destinationCompany: '' }
    expect(destinationView(special, null, telemetry, [], text, 0)).toMatchObject({ title: 'Kraków', subtitle: null })
  })

  it('without a job, names a map point after its nearest city', () => {
    const view = destinationView(null, [900, 0], telemetry, [label('Bari', 1000), label('Taranto', 5000)], text, 0)
    expect(view).toMatchObject({ kicker: 'Destination', title: 'Bari' })
    expect(view?.detail).toBe('17:00 arrival · 3:00 hrs')
  })

  it('shows nothing without a job or a route', () => {
    expect(destinationView(null, null, { ...telemetry, navigationDistance: 0 }, [], text, 0)).toBeNull()
  })
})

describe('helpers', () => {
  it('gives a deadline its day only when it is not today', () => {
    expect(formatDeadline(now + 60, now, text.days)).toBe('15:00')
    expect(formatDeadline(now + 2 * 1440, now, STRINGS.tr.days)).toBe('Cmt 14:00')
  })

  it('finds the nearest city, by the name the map shows', () => {
    expect(nearestCity([4000, 0], [label('Bari', 1000), label('Τάραντας', 5000, 'Taranto')])).toBe('Τάραντας')
    expect(nearestCity([0, 0], [])).toBeNull()
  })
})
