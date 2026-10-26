import { afterEach, describe, expect, it, vi } from 'vitest'
import type { PageMessage, SpotifyPlayback } from '../../../shared/host'

/** What the page sends the host, in WebView2's place. */
function inHost() {
  const posted: PageMessage[] = []
  vi.stubGlobal('window', { chrome: { webview: { postMessage: (message: PageMessage) => posted.push(message) } } })
  vi.stubGlobal('localStorage', { getItem: () => null, setItem: () => {}, removeItem: () => {} })
  return { commands: () => posted.flatMap((message) => (message.type === 'spotify' ? [message.command] : [])) }
}

const SONG = {
  uri: 'spotify:track:1',
  name: 'Şımarık',
  artists: [{ uri: 'spotify:artist:2', name: 'Tarkan' }],
  album: { uri: 'spotify:album:3', name: 'Ölürüm Sana' },
  image: null,
  color: '#000000',
  duration: 240,
  explicit: false,
}

/** Spotify's servers say it, here: playing or not, at a volume of 0.6. */
function said(playing: boolean): SpotifyPlayback {
  return {
    track: SONG,
    context: null,
    playing,
    position: 30,
    duration: 240,
    shuffle: false,
    repeat: 'off',
    volume: 0.6,
    here: true,
    next: [],
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.resetModules()
})

describe('Spotify playing here', () => {
  it('goes by its player in its page, not by what Spotify’s servers say a moment later', async () => {
    inHost()
    const { playingHere, receivePlayback, receiveState, useSpotify } = await import('./spotify')
    receivePlayback(said(true))
    expect(playingHere()).toBe(true)
    // Paused (the game left): the page's player says so at once.
    receiveState(true, false, false)
    expect(playingHere()).toBe(false)
    expect(useSpotify.getState().playback?.playing).toBe(false)
    // What Spotify's servers sent before the pause comes after it: still paused.
    receivePlayback(said(true))
    expect(playingHere()).toBe(false)
    expect(useSpotify.getState().playback?.playing).toBe(false)
  })

  it('asks the page to play on whatever the app shows, the page playing on only if its player isn’t', async () => {
    const host = inHost()
    const { receivePlayback, resumeHere } = await import('./spotify')
    // Shown playing (the servers' word, the page's not said yet): asked all the same.
    receivePlayback(said(true))
    resumeHere()
    expect(host.commands()).toEqual([{ action: 'play' }])
  })

  it('fades in the page’s own player, Spotify’s own volume left as chosen', async () => {
    const host = inHost()
    const { receivePlayback } = await import('./spotify')
    const { spotifyPlayer } = await import('./pauseWithGame')
    receivePlayback(said(true))
    spotifyPlayer.sendVolume(0.3)
    spotifyPlayer.sendVolume(0.6)
    expect(host.commands()).toEqual([
      { action: 'fade', level: 0.5 },
      { action: 'fade', level: 1 },
    ])
  })

  it('says it started as its player in its page starts', async () => {
    inHost()
    const { receivePlayback, receiveState } = await import('./spotify')
    const { spotifyPlayer } = await import('./pauseWithGame')
    receivePlayback(said(false))
    receiveState(true, false, false)
    const started: boolean[] = []
    const stop = spotifyPlayer.subscribe((now) => started.push(now))
    receiveState(true, true, false)
    stop()
    expect(started).toContain(true)
  })
})
