import { describe, expect, it } from 'vitest'
import { STRINGS } from '../state/strings'
import { isNight, nextChange, roads, weatherView } from './weather'

const noon = 12 * 60
const midnight = 0

describe('weatherView', () => {
  it('says the weather where the truck is, with its symbol', () => {
    expect(weatherView({ now: 'clear', coming: 'clear' }, noon, 'Taranto', STRINGS.en)).toEqual({
      place: 'Taranto',
      condition: 'Clear',
      symbol: 'sun',
    })
    expect(weatherView({ now: 'rain', coming: 'rain' }, noon, 'Taranto', STRINGS.tr)).toEqual({
      place: 'Taranto',
      condition: 'Yağmur',
      symbol: 'rain',
    })
  })

  it('says what the weather is turning to while it turns', () => {
    const starting = weatherView({ now: 'clear', coming: 'rain' }, noon, null, STRINGS.en)
    expect(starting).toEqual({ place: null, condition: 'Rain starting', symbol: 'rain' })
    const clearing = weatherView({ now: 'rain', coming: 'clear' }, noon, null, STRINGS.en)
    expect(clearing).toEqual({ place: null, condition: 'Clearing up', symbol: 'rain' })
  })

  it('shows a clear night with the moon', () => {
    expect(weatherView({ now: 'clear', coming: 'clear' }, midnight, null, STRINGS.en).symbol).toBe('moon')
    // Rain is rain, day or night.
    expect(weatherView({ now: 'rain', coming: 'rain' }, midnight, null, STRINGS.en).symbol).toBe('rain')
  })
})

describe('isNight', () => {
  it('is night from 21:00 to 6:00, on any day', () => {
    expect([5 * 60 + 59, 6 * 60, 20 * 60 + 59, 21 * 60].map(isNight)).toEqual([true, false, false, true])
    expect(isNight(3 * 1440 + 22 * 60)).toBe(true)
    expect(isNight(3 * 1440 + 9 * 60)).toBe(false)
  })
})

describe('roads', () => {
  it('are dry, damp or wet by how wet the game has them', () => {
    expect(roads(0, false).state).toBe('dry')
    expect(roads(12, false).state).toBe('damp')
    expect(roads(59, true).state).toBe('wet')
  })

  it('get wetter in the rain until soaked, and dry after it', () => {
    expect(roads(0, true).trend).toBe('gettingWet')
    expect(roads(100, true).trend).toBeNull()
    expect(roads(40, false).trend).toBe('drying')
    expect(roads(0, false).trend).toBeNull()
  })
})

describe('nextChange', () => {
  it('is the game time of the next change, from the minutes left', () => {
    // Thu 14:00, the change 102 game minutes off: at 15:42.
    const thursday = 3 * 1440 + 14 * 60
    expect(nextChange(thursday, 102)).toEqual({ at: thursday + 102, in: 102 })
  })

  it('is unknown when the host can’t read it', () => {
    expect(nextChange(600, null)).toBeNull()
    expect(nextChange(600, undefined)).toBeNull()
    expect(nextChange(600, -5)).toBeNull()
  })
})
