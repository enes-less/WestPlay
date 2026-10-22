import { describe, expect, it } from 'vitest'
import type { MusicPlayer } from './musicWithGame'
import { oneAtATime } from './oneAtATime'

/** A player for the rule: playing here or not, told when its music changes by `change`. */
function player(playing = false) {
  const listeners = new Set<(started: boolean) => void>()
  const state = { playing, pauses: 0 }
  const it: MusicPlayer = {
    name: 'player',
    playingHere: () => state.playing,
    pauseHere: () => {
      state.playing = false
      state.pauses += 1
    },
    resumeHere: () => {
      state.playing = true
    },
    volume: () => null,
    sendVolume: () => {},
    setVolume: () => {},
    subscribe: (changed) => {
      listeners.add(changed)
      return () => listeners.delete(changed)
    },
  }
  /** Its music changed: `started` as it begins playing here. */
  const change = (started: boolean) => {
    if (started) state.playing = true
    for (const listener of listeners) listener(started)
  }
  return { it, state, change, listeners }
}

describe('one player at a time', () => {
  it('pauses the others playing here when one starts, and the newest plays on', () => {
    const spotify = player(true)
    const youtubeMusic = player()
    const appleMusic = player()
    oneAtATime([spotify.it, youtubeMusic.it, appleMusic.it])
    youtubeMusic.change(true)
    expect(spotify.state).toEqual({ playing: false, pauses: 1 })
    expect(youtubeMusic.state).toEqual({ playing: true, pauses: 0 })
    // Not playing: left alone.
    expect(appleMusic.state.pauses).toBe(0)
  })

  it('does nothing for a change that is not a start', () => {
    const spotify = player(true)
    const youtube = player(true)
    oneAtATime([spotify.it, youtube.it])
    youtube.change(false)
    expect(spotify.state).toEqual({ playing: true, pauses: 0 })
  })

  it('follows the newest each time', () => {
    const spotify = player()
    const youtube = player()
    oneAtATime([spotify.it, youtube.it])
    spotify.change(true)
    youtube.change(true)
    expect(spotify.state).toEqual({ playing: false, pauses: 1 })
    spotify.change(true)
    expect(youtube.state).toEqual({ playing: false, pauses: 1 })
    expect(spotify.state.playing).toBe(true)
  })

  it('stops listening once stopped', () => {
    const spotify = player(true)
    const youtube = player()
    const stop = oneAtATime([spotify.it, youtube.it])
    stop()
    expect(spotify.listeners.size + youtube.listeners.size).toBe(0)
    youtube.change(true)
    expect(spotify.state.pauses).toBe(0)
  })
})
