import type { SpotifyItem } from '../../../shared/host'

/**
 * What the Spotify app keeps of the account between WestPlay's runs (Spotify is slow to
 * answer): Home's Recently Played (its top tiles and its Recently played shelf) and Your Library's
 * list, shown at once as the app opens and replaced by Spotify's fresh answer as it comes. Cleared as Spotify
 * signs out. Nothing else of the account is kept.
 */
export interface Kept {
  /** Home's top tiles. */
  shortcuts: SpotifyItem[]
  /** Home's Recently played shelf (its title, its cards). */
  recent: { title: string; items: SpotifyItem[] } | null
  /** Your Library's list. */
  library: SpotifyItem[] | null
}

const KEY = 'westplay.spotify.kept'
const NOTHING: Kept = { shortcuts: [], recent: null, library: null }
const KINDS = new Set(['playlist', 'album', 'artist', 'show', 'liked', 'track'])
/** Spotify's own image hosts (its covers': i.scdn.co, mosaic.scdn.co, image-cdn-*.spotifycdn.com…). */
const PICTURE = /^https:\/\/([a-z0-9-]+\.)*(scdn\.co|spotifycdn\.com)\//

/** An item as it was kept: only what an item is, a picture only from Spotify's hosts; anything else, none. */
function keptItem(kept: unknown): SpotifyItem | null {
  if (!kept || typeof kept !== 'object') return null
  const item = kept as Record<string, unknown>
  if (typeof item.uri !== 'string' || !item.uri.startsWith('spotify:') || item.uri.length > 200) return null
  if (typeof item.name !== 'string' || typeof item.kind !== 'string' || !KINDS.has(item.kind)) return null
  return {
    uri: item.uri,
    kind: item.kind as SpotifyItem['kind'],
    name: item.name.slice(0, 300),
    by: typeof item.by === 'string' ? item.by.slice(0, 300) : '',
    description: typeof item.description === 'string' ? item.description.slice(0, 500) : '',
    ...(typeof item.songs === 'number' ? { songs: item.songs } : {}),
    image: typeof item.image === 'string' && PICTURE.test(item.image) ? item.image : null,
    color: typeof item.color === 'string' && /^#[0-9a-f]{6}$/i.test(item.color) ? item.color : '#333333',
    ...(item.pinned === true ? { pinned: true } : {}),
    ...(item.downloaded === true ? { downloaded: true } : {}),
  }
}

function keptItems(kept: unknown, most: number): SpotifyItem[] {
  return Array.isArray(kept)
    ? kept
        .map(keptItem)
        .filter((item): item is SpotifyItem => item !== null)
        .slice(0, most)
    : []
}

/** What's kept, as it reads back (only what's sound). */
export function readKept(stored: string | null = load()): Kept {
  if (!stored) return NOTHING
  try {
    const kept = JSON.parse(stored) as Record<string, unknown>
    const recent = kept.recent as Record<string, unknown> | null | undefined
    return {
      shortcuts: keptItems(kept.shortcuts, 8),
      recent:
        recent && typeof recent.title === 'string'
          ? { title: recent.title.slice(0, 200), items: keptItems(recent.items, 20) }
          : null,
      library: Array.isArray(kept.library) ? keptItems(kept.library, 500) : null,
    }
  } catch {
    return NOTHING
  }
}

function load(): string | null {
  try {
    return localStorage.getItem(KEY)
  } catch {
    return null
  }
}

/** Keeps a part (Home's, the Library's), the rest as it was. */
export function keep(part: Partial<Kept>): void {
  try {
    localStorage.setItem(KEY, JSON.stringify({ ...readKept(), ...part }))
  } catch {
    // Not kept: Spotify is asked as ever.
  }
}

/** Forgets it all (signed out). */
export function forgetKept(): void {
  try {
    localStorage.removeItem(KEY)
  } catch {
    // Nothing kept, or nothing to remove.
  }
}
