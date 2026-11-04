import { describe, expect, it } from 'vitest'
import { formatGameTime, formatTripDistance, formatTripTime, roadSpeed } from './format'

const words = { minutes: (m: number) => `${m} min`, hours: (h: number, m: string) => `${h}:${m} hrs` }

describe('the clock', () => {
  const period = (time: string, afternoon: boolean) => `${time} ${afternoon ? 'PM' : 'AM'}`
  it('reads 24-hour time', () => {
    expect(formatGameTime(3 * 1440 + 9 * 60 + 5)).toBe('09:05')
    expect(formatGameTime(-60)).toBe('23:00')
  })
  it('reads 12-hour time, midnight and noon as 12, with its time of day or without', () => {
    const style = { hours24: false, period }
    expect(formatGameTime(0, style)).toBe('12:00 AM')
    expect(formatGameTime(9 * 60 + 5, style)).toBe('9:05 AM')
    expect(formatGameTime(12 * 60, style)).toBe('12:00 PM')
    expect(formatGameTime(19 * 60 + 14, style)).toBe('7:14 PM')
    expect(formatGameTime(19 * 60 + 14, { hours24: false, period: null })).toBe('7:14')
  })
})

describe('trip', () => {
  it('shows the time left in minutes, then hours and minutes', () => {
    expect(formatTripTime(0.4, words)).toBe('0 min')
    expect(formatTripTime(12.2, words)).toBe('12 min')
    expect(formatTripTime(59.4, words)).toBe('59 min')
    expect(formatTripTime(65, words)).toBe('1:05 hrs')
    expect(formatTripTime(754, words)).toBe('12:34 hrs')
  })

  it('shows a distance in metres close by, then kilometres, in the language’s decimals', () => {
    expect(formatTripDistance(42)).toBe('40 m')
    expect(formatTripDistance(830)).toBe('850 m')
    expect(formatTripDistance(990)).toBe('1.0 km')
    expect(formatTripDistance(9420)).toBe('9.4 km')
    expect(formatTripDistance(9420, ',')).toBe('9,4 km')
    expect(formatTripDistance(67_703)).toBe('68 km')
  })

  it('shows a distance in feet close by, then miles, in a profile in miles', () => {
    expect(formatTripDistance(12, '.', true)).toBe('40 ft')
    expect(formatTripDistance(100, '.', true)).toBe('350 ft')
    expect(formatTripDistance(160, '.', true)).toBe('500 ft')
    expect(formatTripDistance(170, '.', true)).toBe('0.1 mi')
    expect(formatTripDistance(9420, '.', true)).toBe('5.9 mi')
    expect(formatTripDistance(9420, ',', true)).toBe('5,9 mi')
    expect(formatTripDistance(67_703, '.', true)).toBe('42 mi')
  })

  it('shows the truck’s speed in km/h, or mph in miles', () => {
    expect(roadSpeed(25)).toBe(90)
    expect(roadSpeed(-25, true)).toBe(56)
  })
})
