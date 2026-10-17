/**
 * What one of WestPlay's music players is playing (Apple Music, Spotify, YouTube Music), as Now Playing on the
 * Dashboard shows it, and its controls.
 */

export type MusicService = 'appleMusic' | 'spotify' | 'youTubeMusic'

export interface MediaSource {
  /** Which player it is (`westplay-apple-music`). */
  id: string
  service: MusicService
  title: string
  artist: string
  album: string
  /** Key of the track's artwork; `null` when it has none. */
  artwork: string | null
  status: 'playing' | 'paused' | 'stopped'
  /** The position at `positionAt` (Unix ms); while playing, the clock runs on from there. */
  positionMs: number
  positionAt: number
  durationMs: number
  controls: { playPause: boolean; next: boolean; previous: boolean; shuffle: boolean; repeat: boolean }
  shuffle: boolean | null
  repeat: 'off' | 'track' | 'list' | null
}

export type MediaAction = 'playPause' | 'next' | 'previous' | 'toggleShuffle' | 'cycleRepeat'
