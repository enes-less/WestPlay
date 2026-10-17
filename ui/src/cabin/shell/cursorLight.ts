import { goesLight, luminance, pictureAt } from '../apps/music/contrast'

/**
 * Whether the cursor is over something light, so it goes dark (as Apple Music's mini player does over light
 * covers): a picture by its own pixels where it points (a cover, the wallpaper), anything else by the first solid
 * background under it; YouTube's full-screen player as dark. A video elsewhere or the map (drawn where the page
 * can't read) says nothing: the cursor stays as it was.
 */

/** A colour's luminance, if it's solid enough to be what's seen (more than half opaque). */
export function solidLuminance(color: string): number | null {
  const alpha = /^rgba?\([^)]*[,/]\s*([\d.]+)(%?)\s*\)$/i.exec(color.trim())
  if (alpha) {
    const value = Number(alpha[1]) / (alpha[2] ? 100 : 1)
    if (!(value > 0.5)) return null
  }
  if (color === 'transparent') return null
  return luminance(color)
}

/** How light the page is at `x, y` (0 dark, 1 light); `null` if it can't be told there. */
export function lightAt(x: number, y: number): number | null {
  for (const element of document.elementsFromPoint(x, y)) {
    // YouTube's player, full screen: dark, as its own white controls take it to be, whatever the video. Its
    // cover (the thumbnail dimmed on black, while a video loads) can't be read, and the cursor kept the
    // dark it took from the light thumbnail that opened it.
    if (element.closest('.yt-player')) return 0
    if (element instanceof HTMLImageElement) return pictureAt(element, x, y)
    if (element instanceof HTMLVideoElement || element instanceof HTMLCanvasElement) return null
    const seen = solidLuminance(getComputedStyle(element).backgroundColor)
    if (seen !== null) return seen
  }
  return null
}

/** Points around the cursor's centre it's judged by (its ring's reach), page pixels. */
const AROUND: [number, number][] = [
  [0, 0],
  [-12, 0],
  [12, 0],
  [0, -12],
  [0, 12],
]

/** Whether to show the cursor dark at `x, y`, from how light it is around there and what showed (`null`:
 * nothing to tell by there, as it was). */
export function lightUnder(x: number, y: number, wasLight: boolean): boolean | null {
  const seen = AROUND.map(([dx, dy]) => lightAt(x + dx, y + dy)).filter((each): each is number => each !== null)
  if (seen.length === 0) return null
  const average = seen.reduce((sum, each) => sum + each, 0) / seen.length
  return goesLight(average, wasLight)
}
