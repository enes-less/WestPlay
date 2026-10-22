import { describe, expect, it } from 'vitest'
import { keybinds, stepped } from './keybinds'
import type { MusicPlayer } from './musicWithGame'

/** A player at `volume`, playing here or not; `start()` as it begins playing here. */
function player(volume: number | null, playing = false) {
  const listeners = new Set<(started: boolean) => void>()
  const state = { playing, volume, pauses: 0, resumes: 0 }
  const it: MusicPlayer = {
    name: `player-${Math.random()}`,
    playingHere: () => state.playing,
    pauseHere: () => {
      state.playing = false
      state.pauses += 1
    },
    resumeHere: () => {
      state.playing = true
      state.resumes += 1
    },
    volume: () => state.volume,
    sendVolume: () => {},
    setVolume: (share) => {
      state.volume = share
    },
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

describe('keybinds', () => {
  it('steps the volume of the player playing here by 5%', () => {
    const spotify = player(0.5)
    const youtube = player(0.8, true)
    const binds = keybinds([spotify.it, youtube.it])
    expect(binds.press('volumeUp')).toEqual({ volume: 0.85 })
    expect(binds.press('volumeDown')).toEqual({ volume: 0.8 })
    expect(spotify.state.volume).toBe(0.5)
  })

  it('pauses the player playing here, and plays the one that last played on', () => {
    const spotify = player(0.5)
    const youtube = player(0.8)
    const binds = keybinds([spotify.it, youtube.it])
    spotify.start()
    binds.press('playPause')
    expect(spotify.state).toMatchObject({ playing: false, pauses: 1 })
    // Paused, it's still the one: played on.
    binds.press('playPause')
    expect(spotify.state).toMatchObject({ playing: true, resumes: 1 })
    expect(youtube.state.resumes).toBe(0)
    // Its volume too, though paused.
    spotify.it.pauseHere()
    expect(binds.press('volumeUp')).toEqual({ volume: 0.55 })
  })

  it('does nothing with no player played yet, or one without a volume', () => {
    const spotify = player(null)
    const binds = keybinds([spotify.it])
    expect(binds.press('playPause')).toBeNull()
    expect(spotify.state.resumes).toBe(0)
    spotify.start()
    expect(binds.press('volumeUp')).toBeNull()
  })

  it('stops following the players once stopped', () => {
    const spotify = player(0.5)
    const binds = keybinds([spotify.it])
    binds.stop()
    expect(spotify.listeners.size).toBe(0)
  })
})

describe('volume steps', () => {
  it('goes a step at a time, onto the steps, between none and full', () => {
    expect(stepped(0.6, 1)).toBeCloseTo(0.65)
    expect(stepped(0.6000001, -1)).toBeCloseTo(0.55)
    // Between two steps: to the next one that way.
    expect(stepped(0.62, 1)).toBeCloseTo(0.65)
    expect(stepped(0.62, -1)).toBeCloseTo(0.6)
    expect(stepped(1, 1)).toBe(1)
    expect(stepped(0.02, -1)).toBe(0)
    expect(stepped(0, -1)).toBe(0)
  })
})
