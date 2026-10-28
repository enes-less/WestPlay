import { type RefObject, useEffect, useState } from 'react'

/**
 * Whether what's under a glass bar is light, so its words go dark (Apple's glass does so, smoothly). The bar
 * looks at what's under it: a picture (a cover, an editors' picture) by its own pixels, where they are under
 * the bar; something drawn in a colour (the preview's covers) by that colour (`--cover`); else the page.
 * Apple's image server lets a page read its pictures' pixels (it answers to CORS).
 */

/** Things whose colour is their `--cover` when they have no picture. */
const COVERED = '.am-cover, .am-hero-art, .am-category, .am-artist-head'

/** A colour's relative luminance (0 black, 1 white), as WCAG counts it; `null` if it isn't a colour read
 * here (`#rgb`, `#rrggbb`, `rgb(…)`). */
export function luminance(color: string): number | null {
  const value = color.trim()
  let channels: number[] | null = null
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(value)
  if (hex) {
    const digits = hex[1].length === 3 ? [...hex[1]].map((d) => d + d) : hex[1].match(/../g)!
    channels = digits.map((d) => parseInt(d, 16))
  } else {
    const rgb = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/i.exec(value)
    if (rgb) channels = [rgb[1], rgb[2], rgb[3]].map(Number)
  }
  if (!channels || channels.some((c) => Number.isNaN(c))) return null
  return channelsLuminance(channels[0], channels[1], channels[2])
}

function channelsLuminance(red: number, green: number, blue: number): number {
  const [r, g, b] = [red, green, blue].map((c) => {
    const s = c / 255
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
  })
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}

/** Going dark over this light, and light again under that: apart, so it never flickers at the edge (white
 * words over 0.36 would fall below WCAG's 3:1 for large text). */
export const LIGHT = 0.36
export const DARK = 0.24

/** The glass's own white over what's under it (`--glass`, 12%, in the cab's dark look). */
const GLASS = 0.12

/** How light what's under the bar is, on the whole: the lighter half weighs (words are read where it's
 * lightest), seen through the glass. */
export function lightness(lights: number[]): number {
  if (lights.length === 0) return 0
  const sorted = [...lights].sort((a, b) => b - a)
  const top = sorted.slice(0, Math.ceil(sorted.length / 2))
  const under = top.reduce((sum, each) => sum + each, 0) / top.length
  return under * (1 - GLASS) + GLASS
}

/** Whether to show dark words, from how light it is now and what showed: past the other edge only. */
export function goesLight(now: number, wasLight: boolean): boolean {
  return wasLight ? now > DARK : now > LIGHT
}

/** Pictures' pixels, small (an average of each part is all that's needed), by address; and their reading. */
const pixels = new Map<string, ImageData | 'unreadable'>()
const reading = new Map<string, Promise<void>>()
const SMALL = 24

function pixelsOf(src: string): ImageData | null {
  const known = pixels.get(src)
  if (known instanceof ImageData) return known
  if (!known) void pixelsRead(src)
  return null
}

/** Once `src`'s pixels are read, or found unreadable. */
function pixelsRead(src: string): Promise<void> {
  let read = reading.get(src)
  if (!read) {
    read = new Promise((done) => {
      const image = new Image()
      image.crossOrigin = 'anonymous'
      // Its load (not `decode()`, which waits for a frame to be drawn).
      image.onload = () => {
        try {
          const canvas = document.createElement('canvas')
          canvas.width = SMALL
          canvas.height = SMALL
          const context = canvas.getContext('2d', { willReadFrequently: true })!
          context.drawImage(image, 0, 0, SMALL, SMALL)
          pixels.set(src, context.getImageData(0, 0, SMALL, SMALL))
        } catch {
          pixels.set(src, 'unreadable')
        }
        done()
      }
      image.onerror = () => {
        pixels.set(src, 'unreadable')
        done()
      }
      image.src = src
    })
    reading.set(src, read)
  }
  return read
}

type Rgb = [number, number, number]

/** A picture's average colour (as a heavy blur of it shows), `null` if its pixels can't be read. */
async function averageOf(src: string): Promise<Rgb | null> {
  await pixelsRead(src)
  const data = pixels.get(src)
  if (!(data instanceof ImageData)) return null
  const sum: Rgb = [0, 0, 0]
  for (let at = 0; at < data.data.length; at += 4) {
    for (let c = 0; c < 3; c++) sum[c] += data.data[at + c]
  }
  const count = data.data.length / 4
  return sum.map((each) => each / count) as Rgb
}

/** The cab's two looks: `light` (dark words) and `dark` (white words). */
export type Look = 'light' | 'dark'

/** What shows behind words over a cover: the cover's share of it (its opacity through the veil), and the
 * colour making up the rest (the veil, the glass). */
export interface Backdrop {
  cover: number
  rest: Rgb
}

/** WCAG's contrast between luminances `a` and `b`. */
function contrast(a: number, b: number): number {
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)
}

/** WCAG's least contrast for reading text. */
const READS = 4.5

/** The words to show over a cover of average colour `cover`, behind them `backdrop` (as it is, whatever the
 * words): the cab's look's (`appearance`) while they read there, else the other look's if they read better (a
 * light cover in the dark look: dark words). */
export function wordsOver(cover: Rgb, appearance: Look, backdrop: Backdrop): Look {
  const seen = cover.map((c, i) => c * backdrop.cover + backdrop.rest[i] * (1 - backdrop.cover))
  const under = channelsLuminance(seen[0], seen[1], seen[2])
  const reads = (look: Look) => contrast(look === 'light' ? 0 : 1, under)
  const other: Look = appearance === 'light' ? 'dark' : 'light'
  return reads(appearance) >= READS || reads(other) <= reads(appearance) ? appearance : other
}

/** The words to show over the cover `src` (`wordsOver`, behind them the look's own backdrop): the last cover's
 * while a new one's pixels are read (the covers crossfade); the look's own with no cover, or one that can't
 * be read. */
export function useWordsOver(src: string | null, appearance: Look, backdrops: Record<Look, Backdrop>): Look {
  const [colour, setColour] = useState<Rgb | null>(null)
  useEffect(() => {
    if (!src) return
    let current = true
    void averageOf(src).then((read) => {
      if (current) setColour(read)
    })
    return () => {
      current = false
    }
  }, [src])
  return src && colour ? wordsOver(colour, appearance, backdrops[appearance]) : appearance
}

/** The luminance of a picture where `x, y` (the page's) falls on it, drawn to cover its box (`null` while its
 * pixels load, or if they can't be read). */
export function pictureAt(picture: HTMLImageElement, x: number, y: number): number | null {
  const data = pixelsOf(picture.currentSrc || picture.src)
  if (!data) return null
  const box = picture.getBoundingClientRect()
  const ratio = (picture.naturalWidth || 1) / (picture.naturalHeight || 1)
  // object-fit: cover — the picture scaled to cover the box, centred.
  const scale = Math.max(box.width / ratio, box.height) / box.height
  const width = box.height * ratio * scale
  const height = box.height * scale
  const u = (x - box.left + (width - box.width) / 2) / width
  const v = (y - box.top + (height - box.height) / 2) / height
  const column = Math.min(SMALL - 1, Math.max(0, Math.floor(u * SMALL)))
  const row = Math.min(SMALL - 1, Math.max(0, Math.floor(v * SMALL)))
  const at = (row * SMALL + column) * 4
  return channelsLuminance(data.data[at], data.data[at + 1], data.data[at + 2])
}

/** How light the page is at `x, y`, under `bar`. */
function lightAt(bar: HTMLElement, x: number, y: number, page: number): number {
  const under = document.elementsFromPoint(x, y).find((each) => !bar.contains(each) && !each.closest('.am-toast'))
  if (!under) return page
  const covered = under.closest<HTMLElement>(COVERED)
  const picture =
    under instanceof HTMLImageElement ? under : (covered?.querySelector<HTMLImageElement>(':scope > img') ?? null)
  if (picture) {
    const seen = pictureAt(picture, x, y)
    if (seen !== null) return seen
  }
  const color = covered ? getComputedStyle(covered).getPropertyValue('--cover') : ''
  return luminance(color) ?? page
}

/** How often the bar looks again while shown (and as the page scrolls). */
const LOOK_EVERY = 200

/** Whether what's under `bar` is light, looked at while `shown`: changed only when two looks in a row say
 * so. */
export function useBackdropLight(bar: RefObject<HTMLElement | null>, shown: boolean): boolean {
  const [light, setLight] = useState(false)
  useEffect(() => {
    if (!shown) return
    let showing = light
    let pending = 0
    const look = () => {
      const element = bar.current
      if (!element) return
      const box = element.getBoundingClientRect()
      if (box.width === 0) return
      const page = luminance(getComputedStyle(element).getPropertyValue('--app-background')) ?? 0
      const lights: number[] = []
      for (const row of [0.3, 0.7]) {
        for (let i = 1; i <= 7; i++) {
          lights.push(lightAt(element, box.left + (box.width * i) / 8, box.top + box.height * row, page))
        }
      }
      const wanted = goesLight(lightness(lights), showing)
      pending = wanted === showing ? 0 : pending + 1
      if (pending >= 2) {
        showing = wanted
        pending = 0
        setLight(wanted)
      }
    }
    look()
    const timer = setInterval(look, LOOK_EVERY)
    document.addEventListener('scroll', look, { capture: true, passive: true })
    return () => {
      clearInterval(timer)
      document.removeEventListener('scroll', look, { capture: true })
    }
    // Started again only as it's shown or hidden; what shows carries on from `showing`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bar, shown])
  return light
}

/** The class for a look's words over a cover (shell.css): `words-dark` for the light look's, `words-light` for
 * the dark's. */
export function wordsClass(look: Look): string {
  return look === 'light' ? 'words-dark' : 'words-light'
}
