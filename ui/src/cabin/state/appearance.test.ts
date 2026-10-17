import { describe, expect, it } from 'vitest'
import { autoReason, isNight, NIGHT_ENDS, NIGHT_STARTS, resolveAppearance } from './appearance'

const DAY = 1440
const noon = 3 * DAY + 12 * 60

describe('isNight', () => {
  it('uses the time of day on any game day', () => {
    expect(isNight(noon)).toBe(false)
    expect(isNight(5 * DAY + 23 * 60)).toBe(true)
    expect(isNight(5 * DAY + 2 * 60)).toBe(true)
  })

  it('has inclusive start and exclusive end', () => {
    expect(isNight(NIGHT_STARTS)).toBe(true)
    expect(isNight(NIGHT_STARTS - 1)).toBe(false)
    expect(isNight(NIGHT_ENDS)).toBe(false)
    expect(isNight(NIGHT_ENDS - 1)).toBe(true)
  })
})

describe('resolveAppearance', () => {
  it('keeps a fixed mode regardless of conditions', () => {
    expect(resolveAppearance('light', { gameTime: 23 * 60, lowBeam: true })).toBe('light')
    expect(resolveAppearance('dark', { gameTime: noon, lowBeam: false })).toBe('dark')
  })

  it('is light in daylight with the headlights off', () => {
    expect(resolveAppearance('auto', { gameTime: noon, lowBeam: false })).toBe('light')
  })

  it('is dark at night', () => {
    expect(resolveAppearance('auto', { gameTime: 22 * 60, lowBeam: false })).toBe('dark')
  })

  it('is dark whenever the headlights are on, like a light sensor', () => {
    expect(resolveAppearance('auto', { gameTime: noon, lowBeam: true })).toBe('dark')
  })

  it('is dark without a game', () => {
    expect(resolveAppearance('auto', { gameTime: null, lowBeam: false })).toBe('dark')
  })
})

describe('autoReason', () => {
  it('names what auto responds to', () => {
    expect(autoReason({ gameTime: noon, lowBeam: false })).toBe('daylight')
    expect(autoReason({ gameTime: noon, lowBeam: true })).toBe('headlights')
    expect(autoReason({ gameTime: 23 * 60, lowBeam: true })).toBe('night')
    expect(autoReason({ gameTime: null, lowBeam: false })).toBe('no-game')
  })
})
