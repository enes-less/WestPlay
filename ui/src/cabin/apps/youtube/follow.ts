/** How long a video asked for may take to be the one YouTube's player reports, ms (YouTube starting up,
 * a slow connection). */
export const LOAD_MS = 20_000

/** A video asked for (`play`), not yet reported by YouTube's player, and when it was asked for. */
export interface Loading {
  video: string
  at: number
}

/**
 * What the app shows as playing, given what YouTube's player (in the host, which outlives the page) says it
 * plays: the host is right. `reported` is its video (`null`: YouTube isn't running). Keeps the video shown
 * while one asked for is on its way; otherwise the video shown follows the one reported (`restore`, after a
 * reload of the page, say), or goes when nothing plays (`clear`).
 */
export function follow(
  shown: string | null,
  reported: string | null,
  loading: Loading | null,
  now: number,
): { shown: 'keep' | 'restore' | 'clear'; loading: Loading | null } {
  const waiting = loading !== null && now - loading.at < LOAD_MS
  if (reported === null)
    return waiting ? { shown: 'keep', loading } : { shown: shown ? 'clear' : 'keep', loading: null }
  if (loading && reported === loading.video) return { shown: 'keep', loading: null }
  if (waiting) return { shown: 'keep', loading }
  return { shown: reported === shown ? 'keep' : 'restore', loading: null }
}
