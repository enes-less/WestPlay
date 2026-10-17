import { describe, expect, it } from 'vitest'
import { formatTripDistance, formatTripTime } from './format'

const words = { minutes: (m: number) => `${m} min`, hours: (h: number, m: string) => `${h}:${m} hrs` }

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
})
