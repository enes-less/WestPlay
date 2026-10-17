import { frameRowAt, SCREEN_HEIGHT, type ScreenLayoutId } from './screen'

/**
 * The truck brands' own screen strips that stay on screen around CarPlay (Glass style).
 *
 * WestPlay never draws them: it leaves the strip's area transparent, and the game's own screen shows
 * through there, so the strip is the truck's real one (Shell.tsx).
 */
export type OemStripId = 'daf-2021' | 'scania-2025'

/** How much of the screen's left and bottom edge a strip takes (on the 1280x720 canvas). */
export interface OemStripInsets {
  left: number
  bottom: number
}

export interface OemStrip {
  name: string
  insets: OemStripInsets
}

/** Insets from each layout's stock map area in the game's GPS script; CarPlay takes the map's place. */
export const OEM_STRIPS: Record<OemStripId, OemStrip> = {
  // ui/dashboard/daf_2021_gps.sii (1280x720): the map spans 106 to 656 from the bottom. CarPlay also covers
  // the DAF's status bar above it.
  'daf-2021': { name: 'DAF 2021', insets: { left: 0, bottom: 106 } },
  // ui/dashboard/scania_2025_gps.sii (1440x810): the map starts 108 from the left and 90 from the bottom.
  'scania-2025': { name: 'Scania Smart Dash', insets: { left: 96, bottom: 80 } },
}

export const NO_INSETS: OemStripInsets = { left: 0, bottom: 0 }

/**
 * How far the CarPlay area reaches into a strip, in frame pixels. The game draws screens into a 1024x512
 * target, so our edge is filtered across about 1.4 frame pixels; ending exactly at the strip would leave a
 * half-transparent line over the stock screen beneath (its map shows through). Reaching 2 px in keeps that
 * soft edge over the strip's own dark border instead.
 */
export const STRIP_OVERLAP = 2

/**
 * A strip's insets in frame pixels on the given layout's screen: the CarPlay area reaches into the strip by
 * `STRIP_OVERLAP`, however the game stretches the frame (see `frameRowAt`). Widths are never stretched.
 */
export function frameInsets(insets: OemStripInsets, layout: ScreenLayoutId | null): OemStripInsets {
  const reach = (inset: number) => Math.max(0, inset - STRIP_OVERLAP)
  return {
    left: reach(insets.left),
    bottom: reach(insets.bottom && SCREEN_HEIGHT - frameRowAt(SCREEN_HEIGHT - insets.bottom, layout)),
  }
}

/** `auto` keeps the truck's strip; `off` gives CarPlay the whole screen. */
export type OemMode = 'auto' | 'off'

/** `profileStrip` is the strip of the current screen profile (`null` when the truck has none we know). */
export function resolveOemStrip(mode: OemMode, profileStrip: OemStripId | null): OemStripId | null {
  return mode === 'off' ? null : profileStrip
}
