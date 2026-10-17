/**
 * Where the Music app's bar puts its tab bar and its search field (MusicApp.tsx), in pixels from the bar's
 * left: worked out here rather than left to the browser's layout, so the change between the two states is a
 * plain move and resize of each, smooth.
 *
 * - Closed: the tab bar and the search circle side by side, centred in the bar, but clear of what's at its
 *   sides (Back on the left, Sign In or the account on the right).
 * - Searching: the tab bar folds into a circle (the close button) at the left, clear of Back, and the field
 *   takes the room from it to the right side's things.
 */

/** The circles' size (the bar's height, `--am-bar` in appleMusic.css). */
export const CIRCLE = 76
/** Between the tab bar and the search circle or field. */
export const INNER = 14
/** Between the bar's sides' things and the middle. */
export const GAP = 16

export interface BarSizes {
  /** The bar's width. */
  bar: number
  /** The width of what's at its left (Back; 0 without it) and right (Sign In, the account; 0 without). */
  start: number
  end: number
  /** The tab bar's own width, its tabs all showing. */
  tabs: number
}

export interface Place {
  left: number
  width: number
}

/**
 * How a piece is drawn, between its two places, without changing its layout (a move or resize of glass,
 * blurred, makes the blur around it, the sidebar's, glitch): laid out once at the larger of its places
 * (`box`), slid (`shift`) to where it shows and cut from the right (`cut`) to its size there, its corners
 * rounded.
 */
export function framed(box: Place, shown: Place): { left: number; width: number; shift: number; cut: number } {
  return { left: box.left, width: box.width, shift: shown.left - box.left, cut: Math.max(0, box.width - shown.width) }
}

export function barLayout(sizes: BarSizes, searching: boolean): { tabs: Place; search: Place } {
  const from = sizes.start > 0 ? sizes.start + GAP : 0
  const to = sizes.bar - (sizes.end > 0 ? sizes.end + GAP : 0)
  if (searching) {
    const field = from + CIRCLE + INNER
    return {
      tabs: { left: from, width: CIRCLE },
      search: { left: field, width: Math.max(CIRCLE, to - field) },
    }
  }
  const group = sizes.tabs + INNER + CIRCLE
  const centred = (sizes.bar - group) / 2
  // Centred if it's clear of the sides; as near the middle as it can be if not; from the left if it can't be.
  const left = Math.max(from, Math.min(centred, to - group))
  return {
    tabs: { left, width: sizes.tabs },
    search: { left: left + sizes.tabs + INNER, width: CIRCLE },
  }
}
