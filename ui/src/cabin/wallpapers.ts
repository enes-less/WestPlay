import { type CustomWallpaper, fileUrl } from '../shared/customize'
import type { Appearance } from './state/appearance'

/**
 * Wallpapers by id: WestPlay's own here, and the player's (the companion's Customize, `customWallpaper`). Like
 * icons (icons.ts), the UI refers to wallpapers through this registry only.
 *
 * A wallpaper has a dark and/or a light variant. When the appearance has no matching variant, the
 * wallpaper's own variant wins and the UI follows it (dark glass on a dark-only wallpaper), so text and
 * glass always suit the image behind them.
 */
export interface Wallpaper {
  id: string
  name: string
  variants: Partial<Record<Appearance, string>>
}

/** A wallpaper of one colour, at the screen's size (a wallpaper's image declares its size: `prepareWallpaper`). */
function plain(colour: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="720"><rect width="1280" height="720" fill="${colour}"/></svg>`
  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

/** WestPlay's own wallpapers: Silk (the default), and Plain, no artwork, one colour for each look. The player
 * adds their own in the companion's Customize. */
export const BUILT_IN_WALLPAPERS: Wallpaper[] = [
  { id: 'silk', name: 'Silk', variants: { dark: '/wallpapers/silk.dark.jpg', light: '/wallpapers/silk.jpg' } },
  { id: 'plain', name: 'Plain', variants: { dark: plain('#0b0b0d'), light: plain('#eceef2') } },
]

export const DEFAULT_WALLPAPER_ID = BUILT_IN_WALLPAPERS[0].id

/** The wallpaper `id` names among `wallpapers`, or the default (one the player removed, say). */
export function findWallpaper(id: string, wallpapers: Wallpaper[]): Wallpaper {
  return (
    wallpapers.find((wallpaper) => wallpaper.id === id) ??
    wallpapers.find((wallpaper) => wallpaper.id === DEFAULT_WALLPAPER_ID) ??
    BUILT_IN_WALLPAPERS[0]
  )
}

/** One of the player's wallpapers, its files at `base`. */
export function customWallpaper(wallpaper: CustomWallpaper, base: string): Wallpaper {
  const variants: Wallpaper['variants'] = {}
  if (wallpaper.dark) variants.dark = fileUrl(base, wallpaper.dark)
  if (wallpaper.light) variants.light = fileUrl(base, wallpaper.light)
  return { id: wallpaper.id, name: wallpaper.name, variants }
}

/** The variant to show for `appearance`, and the appearance the UI must then use on top of it. */
export function pickVariant(wallpaper: Wallpaper, appearance: Appearance): { appearance: Appearance; src: string } {
  const wanted = wallpaper.variants[appearance]
  if (wanted) return { appearance, src: wanted }
  const other: Appearance = appearance === 'dark' ? 'light' : 'dark'
  const fallback = wallpaper.variants[other]
  if (fallback) return { appearance: other, src: fallback }
  throw new Error(`wallpaper ${wallpaper.id} has no variants`)
}

/** Every image file any of `wallpapers` can show (for preparing them up front). */
export function allWallpaperSources(wallpapers: Wallpaper[]): string[] {
  return wallpapers.flatMap((wallpaper) => Object.values(wallpaper.variants))
}

/**
 * Draws a wallpaper once into a bitmap of exactly `width` x `height` (cover-cropped, high-quality
 * downscale) and returns an object URL for it. Screens then only ever show these prepared bitmaps:
 * huge photos and filter-heavy SVGs would otherwise make crossfades (and the glass blur on top of them)
 * stutter, because the GPU would scale or re-render the full source every frame.
 */
export async function prepareWallpaper(src: string, width: number, height: number): Promise<string> {
  const image = new Image()
  image.src = src
  await image.decode()
  // An SVG without width/height attributes reports a tiny default size (e.g. 267x150) and would be drawn
  // that small, then stretched into a blur. Wallpaper SVGs must declare their size; catch it if not.
  if (src.endsWith('.svg') && image.naturalWidth < width / 2) {
    console.warn(`wallpaper ${src} has no intrinsic size; add width/height to its <svg> element`)
  }

  const scale = Math.max(width / image.naturalWidth, height / image.naturalHeight)
  const cropWidth = width / scale
  const cropHeight = height / scale
  const cropX = (image.naturalWidth - cropWidth) / 2
  const cropY = (image.naturalHeight - cropHeight) / 2

  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const context = canvas.getContext('2d')
  if (!context) return src
  context.imageSmoothingEnabled = true
  context.imageSmoothingQuality = 'high'
  context.drawImage(image, cropX, cropY, cropWidth, cropHeight, 0, 0, width, height)

  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'))
  return blob ? URL.createObjectURL(blob) : src
}
