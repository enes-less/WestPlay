/**
 * The map's view as the player left it, kept across the map being made anew (a hot reload in development,
 * the page reloading, the app restarting): the zoom they chose for following the truck, and, while they
 * were browsing, where to, at what zoom and turned how. Kept in the page's local storage; a view that
 * can't be read or kept is simply the default one.
 */

export interface BrowsedView {
  center: [number, number]
  zoom: number
  bearing: number
  pitch: number
  /** The zoom when browsing began (liveMap: a different zoom when it ends is one the player chose). */
  from: number
}

export interface SavedView {
  /** How far the player zoomed the following camera from its speed zoom. */
  zoomOffset: number
  browse: BrowsedView | null
}

const KEY = 'westplay.map.view'

type Storage = Pick<globalThis.Storage, 'getItem' | 'setItem'>

function storage(): Storage | null {
  try {
    return window.localStorage
  } catch {
    return null
  }
}

/** The view last kept, if there is one and it reads as one. */
export function loadView(from: Storage | null = storage()): SavedView | null {
  try {
    const value: unknown = JSON.parse(from?.getItem(KEY) ?? 'null')
    if (!isRecord(value) || !finite(value.zoomOffset)) return null
    const browse = value.browse
    if (browse === null || browse === undefined) return { zoomOffset: value.zoomOffset, browse: null }
    if (!isRecord(browse) || !Array.isArray(browse.center) || browse.center.length !== 2) return null
    const [lng, lat] = browse.center
    if (![lng, lat, browse.zoom, browse.bearing, browse.pitch, browse.from].every(finite)) return null
    return {
      zoomOffset: value.zoomOffset,
      browse: {
        center: [lng as number, lat as number],
        zoom: browse.zoom as number,
        bearing: browse.bearing as number,
        pitch: browse.pitch as number,
        from: browse.from as number,
      },
    }
  } catch {
    return null
  }
}

/** Keeps the view (nothing happens where the page can't keep anything). */
export function saveView(view: SavedView, to: Storage | null = storage()) {
  try {
    to?.setItem(KEY, JSON.stringify(view))
  } catch {
    // Storage full or refused: the view just isn't kept.
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function finite(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value)
}
