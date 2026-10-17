import { useEffect, useState } from 'react'

/**
 * Now Playing's colour, from the song's cover (as YouTube Music's own player takes it): the cover's own hue,
 * deep and muted, so white words read over it. Google's image hosts let a page read their pictures' pixels; a
 * picture that can't be read gives none (Now Playing's own dark then).
 */

const known = new Map<string, string | null>()
const SMALL = 16

/** A colour (`hsl(…)`) from a picture's pixels: their average, its hue kept, made deep and muted. */
export function colorOf(data: Uint8ClampedArray): string | null {
  let red = 0
  let green = 0
  let blue = 0
  let weight = 0
  for (let i = 0; i < data.length; i += 4) {
    const [r, g, b, a] = [data[i], data[i + 1], data[i + 2], data[i + 3]]
    if (a < 128) continue
    // The more coloured a pixel, the more it counts (a grey background doesn't wash the colour out).
    const w = 0.2 + (Math.max(r, g, b) - Math.min(r, g, b)) / 255
    red += r * w
    green += g * w
    blue += b * w
    weight += w
  }
  if (weight === 0) return null
  const [r, g, b] = [red / weight / 255, green / weight / 255, blue / weight / 255]
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const lightness = (max + min) / 2
  const chroma = max - min
  let hue = 0
  if (chroma > 0) {
    if (max === r) hue = ((g - b) / chroma) % 6
    else if (max === g) hue = (b - r) / chroma + 2
    else hue = (r - g) / chroma + 4
    hue = (hue * 60 + 360) % 360
  }
  const saturation = chroma === 0 ? 0 : chroma / (1 - Math.abs(2 * lightness - 1))
  const deep = Math.min(0.24, Math.max(0.13, lightness * 0.45))
  const muted = Math.min(0.42, saturation * 0.7)
  return `hsl(${Math.round(hue)} ${Math.round(muted * 100)}% ${Math.round(deep * 100)}%)`
}

/** The colour of the picture at `src` once it's read (`null` until then, or if it can't be). */
export function useCoverColor(src: string | undefined): string | null {
  const [color, setColor] = useState<{ src: string | undefined; color: string | null }>(() => ({
    src,
    color: src ? (known.get(src) ?? null) : null,
  }))
  useEffect(() => {
    if (!src || known.has(src)) return
    let current = true
    const image = new Image()
    image.crossOrigin = 'anonymous'
    image.onload = () => {
      let found: string | null = null
      try {
        const canvas = document.createElement('canvas')
        canvas.width = SMALL
        canvas.height = SMALL
        const context = canvas.getContext('2d', { willReadFrequently: true })!
        context.drawImage(image, 0, 0, SMALL, SMALL)
        found = colorOf(context.getImageData(0, 0, SMALL, SMALL).data)
      } catch {
        // Not readable: none.
      }
      known.set(src, found)
      if (current) setColor({ src, color: found })
    }
    image.onerror = () => known.set(src, null)
    image.src = src
    return () => {
      current = false
    }
  }, [src])
  return color.src === src ? color.color : src ? (known.get(src) ?? null) : null
}
