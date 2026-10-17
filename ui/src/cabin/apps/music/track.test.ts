import { describe, expect, it } from 'vitest'
import type { MediaSource } from '../../../shared/media'
import { readyToShow, trackKey } from './track'

function source(overrides: Partial<MediaSource> = {}): MediaSource {
  return {
    id: 'AppleMusic.exe',
    service: 'appleMusic',
    title: 'Kış Güneşi',
    artist: 'Tarkan',
    album: 'Kış Güneşi',
    artwork: 'cover-key',
    status: 'playing',
    positionMs: 0,
    positionAt: 0,
    durationMs: 240_000,
    controls: { playPause: true, next: true, previous: true, shuffle: false, repeat: false },
    shuffle: null,
    repeat: null,
    ...overrides,
  }
}

describe('trackKey', () => {
  it('tells tracks apart, but not progress through one', () => {
    expect(trackKey(source())).toBe(trackKey(source({ positionMs: 90_000, status: 'paused' })))
    expect(trackKey(source())).not.toBe(trackKey(source({ title: 'Şımarık' })))
    expect(trackKey(source())).not.toBe(trackKey(source({ id: 'Spotify.exe' })))
  })
})

describe('readyToShow', () => {
  it('waits for the cover, unless the track has none or the wait is over', () => {
    expect(readyToShow(source(), null, false)).toBe(false)
    expect(readyToShow(source(), 'data:image/jpeg;base64,x', false)).toBe(true)
    expect(readyToShow(source({ artwork: null }), null, false)).toBe(true)
    expect(readyToShow(source(), null, true)).toBe(true)
  })
})
