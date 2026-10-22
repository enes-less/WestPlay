import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { fadeOutAndPause, playAndFadeIn } from './fade'
import type { MusicPlayer } from './musicWithGame'

/** A player at `volume`, playing or not, recording what it was sent; `start()` as it plays again. */
function player(volume: number | null, playing = true) {
  const listeners = new Set<(started: boolean) => void>()
  const state = { playing, sent: [] as number[], pauses: 0, resumes: 0 }
  const it: MusicPlayer = {
    name: `player-${Math.random()}`,
    playingHere: () => state.playing,
    pauseHere: () => {
      state.playing = false
      state.pauses += 1
    },
    resumeHere: () => {
      state.resumes += 1
    },
    volume: () => volume,
    sendVolume: (share) => state.sent.push(share),
    setVolume: () => {},
    subscribe: (changed) => {
      listeners.add(changed)
      return () => listeners.delete(changed)
    },
  }
  const start = () => {
    state.playing = true
    for (const listener of listeners) listener(true)
  }
  return { it, state, start, listeners }
}

describe('the music with the game', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('fades out over half a second, pauses, and puts the volume back unheard', () => {
    const { it: music, state } = player(0.8)
    fadeOutAndPause(music)
    expect(state.pauses).toBe(0)
    vi.advanceTimersByTime(250)
    expect(state.sent.at(-1)).toBeLessThan(0.8)
    expect(state.pauses).toBe(0)
    vi.advanceTimersByTime(300)
    expect(state.pauses).toBe(1)
    // Faded to nothing, then back to the player's own volume once paused.
    expect(state.sent.slice(-2)).toEqual([0, 0.8])
  })

  it('back in the game, fades in once the player plays again, not before', () => {
    const { it: music, state, start } = player(0.8, false)
    playAndFadeIn(music)
    expect(state.resumes).toBe(1)
    expect(state.sent).toEqual([0])
    // Still starting: held silent.
    vi.advanceTimersByTime(500)
    expect(state.sent).toEqual([0])
    start()
    vi.advanceTimersByTime(150)
    expect(state.sent.at(-1)).toBeGreaterThan(0)
    expect(state.sent.at(-1)).toBeLessThan(0.8)
    vi.advanceTimersByTime(200)
    expect(state.sent.at(-1)).toBe(0.8)
  })

  it('a player that says it plays again as it is asked to fades in at once, not after the wait', () => {
    // Spotify, Apple Music, YouTube Music: playing shown the moment it's asked.
    const { it: music, state, start } = player(0.8, false)
    music.resumeHere = () => {
      state.resumes += 1
      start()
    }
    playAndFadeIn(music)
    vi.advanceTimersByTime(350)
    expect(state.sent.at(-1)).toBe(0.8)
  })

  it("a player that doesn't say it plays again is given its volume back anyway", () => {
    const { it: music, state, listeners } = player(0.6, false)
    playAndFadeIn(music)
    vi.advanceTimersByTime(2400)
    expect(state.sent.at(-1)).toBe(0.6)
    expect(listeners.size).toBe(0)
  })

  it('back before the fade out ended: no pause, faded back up from where it got to', () => {
    const { it: music, state } = player(0.8)
    fadeOutAndPause(music)
    vi.advanceTimersByTime(250)
    const reached = state.sent.at(-1) ?? 0
    playAndFadeIn(music)
    expect(state.resumes).toBe(0)
    vi.advanceTimersByTime(400)
    expect(state.pauses).toBe(0)
    expect(state.sent.at(-1)).toBe(0.8)
    // Up from where it was, not from silence.
    expect(Math.min(...state.sent.slice(state.sent.indexOf(reached)))).toBeGreaterThan(0)
  })

  it('away again before it sounded: paused, and the fade in waiting for its sound called off', () => {
    // Asked to play, it says it plays at once, its sound coming later (Apple Music loading, YouTube buffering).
    const { it: music, state, listeners } = player(0.8, false)
    playAndFadeIn(music)
    state.playing = true
    const waitingFor = [...listeners]
    // The game paused again meanwhile: it's playing, so it's faded out (from silence: paused at once).
    fadeOutAndPause(music)
    vi.advanceTimersByTime(100)
    // Its sound comes (as it was asked to play before the pause): no fade in now.
    for (const listener of waitingFor) listener(true)
    vi.advanceTimersByTime(2500)
    expect(state.pauses).toBe(1)
    expect(state.playing).toBe(false)
    // Its own volume back, unheard, and nothing sent after it.
    expect(state.sent.at(-1)).toBe(0.8)
    expect(listeners.size).toBe(0)
  })

  it('away again before it played at all: the fade in waiting called off, nothing turned up', () => {
    const { it: music, state, listeners } = player(0.8, false)
    playAndFadeIn(music)
    // Not playing yet: nothing to pause (musicWithGame's rule), but the game is left; the wait is called off by
    // the next fade or stop.
    fadeOutAndPause(music)
    const sent = state.sent.length
    vi.advanceTimersByTime(2500)
    expect(state.sent.slice(sent).filter((share) => share > 0 && share < 0.8)).toEqual([])
    expect(listeners.size).toBe(0)
  })

  it('with no volume to fade, pauses and plays on at once', () => {
    const { it: music, state } = player(null)
    fadeOutAndPause(music)
    expect(state.pauses).toBe(1)
    playAndFadeIn(music)
    expect(state.resumes).toBe(1)
    expect(state.sent).toEqual([])
  })
})
