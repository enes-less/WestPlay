/**
 * Safari's bars in the page's own colour, as iPadOS 26's take it (the page's `theme-color`, else its background,
 * as the host says it): the colour, and whether the symbols on it are dark (a light page) or light.
 */
export interface PageColours {
  bar: string
  look: 'light' | 'dark'
}

/** A CSS colour's red, green and blue (0 to 255) and its opacity (0 to 1): `#rgb`, `#rrggbb`, `#rrggbbaa`,
 * `rgb()` and `rgba()` (with commas or spaces); `null` for any other. */
export function rgbOf(colour: string): [number, number, number, number] | null {
  const text = colour.trim().toLowerCase()
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/.exec(text)
  if (hex) {
    const digits = hex[1].length === 3 ? [...hex[1]].map((d) => d + d).join('') : hex[1]
    const parts = digits.match(/../g)?.map((pair) => parseInt(pair, 16)) ?? []
    return [parts[0], parts[1], parts[2], parts.length > 3 ? parts[3] / 255 : 1]
  }
  const rgb = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:\s*[,/]\s*([\d.]+%?))?\s*\)$/.exec(text)
  if (rgb) {
    const alpha = rgb[4] === undefined ? 1 : rgb[4].endsWith('%') ? parseFloat(rgb[4]) / 100 : parseFloat(rgb[4])
    return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3]), alpha]
  }
  return null
}

/** The bars' colours for a page's colour; `null` (the bars' own) for none, one not read, or one see-through. */
export function pageColours(colour: string | undefined): PageColours | null {
  const rgb = colour ? rgbOf(colour) : null
  if (!rgb || rgb[3] < 0.9) return null
  const [r, g, b] = rgb.map((value) => Math.min(255, Math.max(0, value)))
  // Relative luminance (WCAG): dark symbols on what's lighter than the middle.
  const linear = (value: number) => {
    const c = value / 255
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
  }
  const luminance = 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
  return { bar: `rgb(${r} ${g} ${b})`, look: luminance > 0.4 ? 'light' : 'dark' }
}
