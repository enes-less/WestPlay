import { useText } from '../../state/text'
import type { SpotifyItem, SpotifyTrack } from './spotify'
import { useSpotify } from './spotify'

/** What the Spotify app's pages show alike: a song's cover, what a thing is, whether it plays, a time. */

export type Covered = Pick<SpotifyItem, 'name' | 'kind' | 'image' | 'color'>

export function trackCover(track: SpotifyTrack): Covered {
  return { name: track.album.name, kind: 'album', image: track.image, color: track.color }
}

/** "Playlist • Spotify", "Album • Diesel Hearts", "Artist", "Playlist • 128 songs". */
export function useKindLine() {
  const text = useText()
  return (item: SpotifyItem) => {
    const kind = text.spotify.kinds[item.kind === 'track' ? 'song' : item.kind]
    const by = item.songs !== undefined ? text.spotify.songCount(item.songs) : item.by
    return by ? `${kind} • ${by}` : kind
  }
}

/** Whether what plays is from this playlist, album or artist. */
export function usePlaysFrom(uri: string): { from: boolean; playing: boolean } {
  return useSpotify((state) => {
    const from = state.playback?.context?.uri === uri
    return from ? (state.playback!.playing ? PLAYING : FROM) : NOT
  })
}

const PLAYING = { from: true, playing: true }
const FROM = { from: true, playing: false }
const NOT = { from: false, playing: false }

/** m:ss */
export function clock(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds))
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`
}
