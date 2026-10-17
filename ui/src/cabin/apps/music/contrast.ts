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

/** Pictures' pixels, small (an average of each part is all that's needed), by address. */
const pixels = new Map<string, ImageData | 'loading' | 'unreadable'>()
const SMALL = 24

function pixelsOf(src: string): ImageData | null {
  const known = pixels.get(src)
  if (known instanceof ImageData) return known
  if (known) return null
  pixels.set(src, 'loading')
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
  }
  image.onerror = () => pixels.set(src, 'unreadable')
  image.src = src
  return null
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
