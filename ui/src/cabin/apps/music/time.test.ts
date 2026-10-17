import { describe, expect, it } from 'vitest'
import type { Playback } from './appleMusic'
import { positionOf } from './time'

const playback = (change: Partial<Playback>): Playback => ({
  context: null,
  tracks: [],
  index: 0,
  playing: true,
  loading: false,
  position: 37.6,
  at: 10_000,
  duration: 200,
  preview: false,
  shuffle: false,
  repeat: 'off',
  volume: 1,
  direction: 1,
  ...change,
})

describe('a song’s time', () => {
  it('runs on while it plays', () => {
    expect(positionOf(playback({}), 12_000)).toBeCloseTo(39.6)
  })

  it('never goes back when played on after a pause (the bar’s clock last ticked before the song’s time)', () => {
    // Paused at 37.6 s and played on at 10 s on the page's clock; the bar's clock last ticked before the pause.
    expect(positionOf(playback({}), 7_000)).toBe(37.6)
  })

  it('holds still while paused or loading', () => {
    expect(positionOf(playback({ playing: false }), 20_000)).toBe(37.6)
    expect(positionOf(playback({ loading: true }), 20_000)).toBe(37.6)
  })
})
