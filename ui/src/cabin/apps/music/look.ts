import { useText } from '../../state/text'
import type { AppleMusicItem, AppleMusicTrack } from './appleMusic'

/** What the Music app's pages show alike: a song's cover, what a thing is, a time. */

export type Covered = Pick<AppleMusicItem, 'name' | 'kind' | 'image' | 'color'>

export function trackCover(track: AppleMusicTrack): Covered {
  return { name: track.album, kind: 'album', image: track.image, color: track.color }
}

/** "Album · Diesel Hearts", "Playlist · Apple Music", "Artist". */
export function useKindLine() {
  const text = useText()
  return (item: AppleMusicItem) => {
    const kind = text.appleMusic.kinds[item.kind]
    return item.by ? `${kind} · ${item.by}` : kind
  }
}

/** m:ss */
export function clock(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds))
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`
}
