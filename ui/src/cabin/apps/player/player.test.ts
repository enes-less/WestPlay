import { describe, expect, it } from 'vitest'
import { coverOf, currentSong, nameOf, positionOf } from './player'

describe('Player', () => {
  it('names a song by its title, else by its file without its extension', () => {
    expect(nameOf({ file: 'a.mp3', title: 'Night Drive', seconds: 1 })).toBe('Night Drive')
    expect(nameOf({ file: 'weigh station (live).mp3', seconds: 1 })).toBe('weigh station (live)')
    expect(nameOf({ file: 'no extension', seconds: 1 })).toBe('no extension')
  })

  it('shows a cover only when the song has one', () => {
    expect(coverOf({ file: 'a.mp3', seconds: 1, cover: '0011.jpg' })).toBe('https://westplay.music-covers/0011.jpg')
    expect(coverOf({ file: 'a.mp3', seconds: 1 })).toBeNull()
    expect(coverOf(null)).toBeNull()
  })

  it('finds the song playing in the folder, or the file alone when the folder no longer lists it', () => {
    const songs = [{ file: 'a.mp3', title: 'A', seconds: 10 }]
    const playing = (file: string) => ({ file, at: 0 }) as Parameters<typeof positionOf>[0]
    expect(currentSong({ songs, playback: playing('a.mp3') })).toBe(songs[0])
    expect(currentSong({ songs, playback: playing('gone.mp3') })).toEqual({ file: 'gone.mp3', seconds: 0 })
    expect(currentSong({ songs, playback: null })).toBeNull()
  })

  it('runs the time on while it plays, never past the song’s end, and holds it paused or loading', () => {
    const base = { file: 'a.mp3', position: 10, duration: 12, shuffle: false, repeat: 'off' as const, volume: 1, index: 0, count: 1, at: 1000 }
    expect(positionOf({ ...base, playing: true, loading: false }, 2500)).toBe(11.5)
    expect(positionOf({ ...base, playing: true, loading: false }, 9000)).toBe(12)
    expect(positionOf({ ...base, playing: false, loading: false }, 9000)).toBe(10)
    expect(positionOf({ ...base, playing: true, loading: true }, 9000)).toBe(10)
    expect(positionOf(null, 9000)).toBe(0)
  })
})
