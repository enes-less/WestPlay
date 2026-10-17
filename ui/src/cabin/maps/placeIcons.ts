import type { Appearance } from '../state/appearance'

/**
 * The icons of places on the map (the `place` tile layer: crates/mapdata, area.rs), in the manner of Apple
 * Maps: a round badge in the category's colour with a white glyph (dark on a light badge), lifted off the
 * map by a soft shadow, and its name beside it in the same colour: its own (a dealer's brand, a company, a
 * port), or else what it is (the game's map legend, in the player's language). Transit and parking are rounded squares, as there: ferry ports, train terminals, truck
 * parks. Drawn here, in WestPlay's own shapes and type, at the screen's pixel ratio, when the map first asks
 * for them; drawn again when the appearance changes (the names' colours follow it).
 */

export const PLACE_KINDS = [
  'fuel',
  'service',
  'garage',
  'garage-sale',
  'dealer',
  'company',
  'parking',
  'hotel',
  'recruitment',
  'weigh',
  'ferry',
  'train',
  'toll',
  'border',
] as const
export type PlaceKind = (typeof PLACE_KINDS)[number]

/** The start of a place icon's image name (`icon-image` in style.ts): `place-<kind>`, or for a named
 * place `place-<kind>|<name>`. */
export const PLACE_ICON_PREFIX = 'place-'
const NAME_SEPARATOR = '|'

/** The badge's diameter, and the room around it for its shadow, CSS pixels. */
export const PLACE_ICON_SIZE = 50
export const SHADOW_ROOM = 3
/** Names: their type size, the gap from the badge, their halo, and how wide a line may grow before it
 * wraps (pixels). */
const NAME_SIZE = 24
const NAME_GAP = 6
const NAME_HALO = 4
const NAME_WIDTH = 215
const NAME_LINES = 3

const COLOURS: Record<PlaceKind, string> = {
  // Apple's own fuel blue.
  fuel: '#3478f6',
  // Apple draws car services grey.
  service: '#8e8e93',
  // A colour of its own: Apple has nothing like a player's garage, and its purple means hotels. One for
  // sale is the same garage, drawn as an outline (OUTLINED).
  garage: '#5e5ce6',
  'garage-sale': '#5e5ce6',
  // Apple's car dealers: gold, with a dark glyph (white on a light badge doesn't read).
  dealer: '#e2b12f',
  // Apple's brown.
  company: '#a2845e',
  // Apple's parking blue, on a square: told from fuel by its shape and glyph, as there.
  parking: '#3478f6',
  // Apple's hotels: purple.
  hotel: '#af52de',
  // Apple's teal, a shade deeper so its names read on the light map.
  recruitment: '#1f9fb4',
  // Apple's orange: a stop the road calls for.
  weigh: '#ff9500',
  // Sea blue, deeper than fuel's, and slate for rail: transit, each its own.
  ferry: '#1b84b5',
  train: '#55606e',
  // Apple's green, and its red: paying, and stopping at a border.
  toll: '#2fa84f',
  border: '#e5483e',
}

/** Kinds drawn as an outline in their colour on a plain badge (white, or the dark map's own grey), rather
 * than filled: a garage for sale, quieter than the player's own. */
const OUTLINED: ReadonlySet<PlaceKind> = new Set(['garage-sale'])
const OUTLINE_FILL: Record<Appearance, string> = { light: '#ffffff', dark: '#2c2c2e' }
const OUTLINE_WIDTH = 2.5

/** Kinds drawn as rounded squares rather than round badges. */
const SQUARE_KINDS: ReadonlySet<PlaceKind> = new Set(['parking', 'ferry', 'train'])
/** A square badge's corner radius, as a share of its side (Apple's transit squares). */
const SQUARE_CORNER = 0.26

/** The glyph's colour: white, or near black on a light badge. */
const GLYPH_COLOURS: Partial<Record<PlaceKind, string>> = { dealer: '#2a2208' }

/** Behind names, so they read over roads: the map's ground (style.ts). */
const HALOS: Record<Appearance, string> = { light: 'rgb(230 226 217 / 0.92)', dark: 'rgb(31 34 38 / 0.92)' }

/** The image name for a place of `kind`, named `name` or not. */
export function placeIconName(kind: PlaceKind, name?: string): string {
  return `${PLACE_ICON_PREFIX}${kind}${name ? `${NAME_SEPARATOR}${name}` : ''}`
}

/** What a map image name is, if it's a place icon. */
export function parsePlaceIcon(imageId: string): { kind: PlaceKind; name: string | null } | null {
  if (!imageId.startsWith(PLACE_ICON_PREFIX)) return null
  const rest = imageId.slice(PLACE_ICON_PREFIX.length)
  const at = rest.indexOf(NAME_SEPARATOR)
  const [kind, name] = at < 0 ? [rest, null] : [rest.slice(0, at), rest.slice(at + 1)]
  return (PLACE_KINDS as readonly string[]).includes(kind) ? { kind: kind as PlaceKind, name } : null
}

/** A name in lines no wider than `width` (by `measure`), at most `lines` of them, words kept whole (one too
 * long for a line has it to itself). */
export function wrapName(name: string, measure: (text: string) => number, width: number, lines: number): string[] {
  const wrapped: string[] = []
  for (const word of name.split(/\s+/).filter(Boolean)) {
    const last = wrapped.at(-1)
    if (last !== undefined && measure(`${last} ${word}`) <= width) wrapped[wrapped.length - 1] = `${last} ${word}`
    else wrapped.push(word)
  }
  if (wrapped.length <= lines) return wrapped
  return [...wrapped.slice(0, lines - 1), wrapped.slice(lines - 1).join(' ')]
}

/** A place's icon as pixels, for `map.addImage(…, { pixelRatio })`. The badge's centre is
 * `SHADOW_ROOM + PLACE_ICON_SIZE / 2` from the image's left edge, halfway down. */
export function drawPlaceIcon(
  kind: PlaceKind,
  name: string | null,
  appearance: Appearance,
  pixelRatio: number,
  fontFamily: string,
): { width: number; height: number; data: Uint8Array } {
  const badge = PLACE_ICON_SIZE + SHADOW_ROOM * 2
  const font = `600 ${NAME_SIZE}px ${fontFamily}`
  const measuring = document.createElement('canvas').getContext('2d')
  if (measuring) measuring.font = font
  const measure = (text: string) => measuring?.measureText(text).width ?? text.length * NAME_SIZE * 0.55
  const lines = name ? wrapName(name, measure, NAME_WIDTH, NAME_LINES) : []
  const lineHeight = NAME_SIZE * 1.15
  const textWidth = Math.max(0, ...lines.map(measure))
  const width = Math.ceil(lines.length ? badge + NAME_GAP + textWidth + NAME_HALO : badge)
  const height = Math.ceil(Math.max(badge, lines.length * lineHeight + NAME_HALO * 2))

  const canvas = document.createElement('canvas')
  canvas.width = Math.ceil(width * pixelRatio)
  canvas.height = Math.ceil(height * pixelRatio)
  const context = canvas.getContext('2d')
  const empty = { width: canvas.width, height: canvas.height, data: new Uint8Array(canvas.width * canvas.height * 4) }
  if (!context) return empty
  context.scale(pixelRatio, pixelRatio)
  const centre = { x: SHADOW_ROOM + PLACE_ICON_SIZE / 2, y: height / 2 }
  const radius = PLACE_ICON_SIZE / 2 - 1

  // The badge: its colour, a touch lighter at the top, on a soft shadow, with a hairline edge; round, or a
  // rounded square of the same size.
  context.save()
  context.shadowColor = 'rgb(0 0 0 / 0.35)'
  context.shadowBlur = 3
  context.shadowOffsetY = 1
  const outlined = OUTLINED.has(kind)
  const fill = context.createLinearGradient(0, centre.y - radius, 0, centre.y + radius)
  fill.addColorStop(0, outlined ? OUTLINE_FILL[appearance] : towards(COLOURS[kind], 255, 0.18))
  fill.addColorStop(1, outlined ? OUTLINE_FILL[appearance] : COLOURS[kind])
  context.fillStyle = fill
  context.beginPath()
  if (SQUARE_KINDS.has(kind)) {
    const side = radius * 2
    context.roundRect(centre.x - radius, centre.y - radius, side, side, side * SQUARE_CORNER)
  } else {
    context.arc(centre.x, centre.y, radius, 0, Math.PI * 2)
  }
  context.fill()
  context.restore()
  if (outlined) {
    context.beginPath()
    context.arc(centre.x, centre.y, radius - OUTLINE_WIDTH / 2, 0, Math.PI * 2)
    context.strokeStyle = COLOURS[kind]
    context.lineWidth = OUTLINE_WIDTH
  } else {
    context.strokeStyle = 'rgb(0 0 0 / 0.18)'
    context.lineWidth = 1
  }
  context.stroke()

  // The glyph, on a 24-unit grid centred in the badge: drawn apart and laid on, so what it cuts out
  // of itself (the wrench's jaw, the truck's wheel arches) leaves the badge showing.
  const glyph = document.createElement('canvas')
  glyph.width = canvas.width
  glyph.height = canvas.height
  const pen = glyph.getContext('2d')
  if (pen) {
    const scale = (PLACE_ICON_SIZE * 0.62) / 24
    pen.scale(pixelRatio, pixelRatio)
    pen.translate(centre.x - 12 * scale, centre.y - 12 * scale)
    pen.scale(scale, scale)
    pen.fillStyle = pen.strokeStyle = outlined ? COLOURS[kind] : (GLYPH_COLOURS[kind] ?? '#fff')
    GLYPHS[kind](pen)
    context.save()
    context.setTransform(1, 0, 0, 1, 0, 0)
    context.drawImage(glyph, 0, 0)
    context.restore()
  }

  // The name: in the badge's colour, lighter on the dark map and deeper on the light one, on a halo.
  if (lines.length) {
    context.font = font
    context.textBaseline = 'middle'
    context.lineJoin = 'round'
    context.lineWidth = NAME_HALO * 2
    context.strokeStyle = HALOS[appearance]
    context.fillStyle = appearance === 'dark' ? towards(COLOURS[kind], 255, 0.3) : towards(COLOURS[kind], 0, 0.45)
    const x = badge + NAME_GAP
    lines.forEach((line, index) => {
      const y = centre.y + (index - (lines.length - 1) / 2) * lineHeight
      context.strokeText(line, x, y)
      context.fillText(line, x, y)
    })
  }

  const pixels = context.getImageData(0, 0, canvas.width, canvas.height)
  return { width: canvas.width, height: canvas.height, data: new Uint8Array(pixels.data.buffer) }
}

function garageGlyph(context: CanvasRenderingContext2D) {
  context.fill(new Path2D('M3 11 12 4l9 7v9.5H3ZM7.5 12.5v8h9v-8Z'), 'evenodd')
  for (const y of [13.6, 15.8, 18]) context.fillRect(7.5, y, 9, 1.2)
}

/** `hex` (#rrggbb) moved `share` of the way to grey level `level` (255 white, 0 black). */
export function towards(hex: string, level: number, share: number): string {
  const channel = (at: number) => {
    const value = parseInt(hex.slice(at, at + 2), 16)
    return Math.round(value + (level - value) * share)
  }
  return `rgb(${channel(1)} ${channel(3)} ${channel(5)})`
}

const GLYPHS: Record<PlaceKind, (context: CanvasRenderingContext2D) => void> = {
  // A fuel pump: its body with a window, standing on a base, the nozzle's hose curving down its side.
  fuel(context) {
    const body = new Path2D('M5.5 3h7A1.5 1.5 0 0 1 14 4.5V20H4V4.5A1.5 1.5 0 0 1 5.5 3ZM6.5 5.5v4h5v-4Z')
    context.fill(body, 'evenodd')
    context.fillRect(3, 19.5, 12, 1.8)
    context.lineWidth = 1.7
    context.lineCap = context.lineJoin = 'round'
    context.stroke(new Path2D('M14 9h1.6a1.4 1.4 0 0 1 1.4 1.4v6.1a1.5 1.5 0 0 0 3 0V8.2L17.6 5.6'))
  },
  // A wrench across the badge: its handle, and the open jaw of its head.
  service(context) {
    context.lineWidth = 3.4
    context.lineCap = 'round'
    context.beginPath()
    context.moveTo(5.2, 18.8)
    context.lineTo(13.4, 10.6)
    context.stroke()
    context.beginPath()
    context.arc(15.6, 8.4, 4.6, 0, Math.PI * 2)
    context.fill()
    // The jaw: cut out of the head, opening to the top right.
    context.save()
    context.globalCompositeOperation = 'destination-out'
    context.translate(15.6, 8.4)
    context.rotate(-Math.PI / 4)
    context.fillRect(-1.8, -6, 3.6, 5.6)
    context.restore()
  },
  // A garage: the house's shape with a roller door, its slats.
  garage: garageGlyph,
  'garage-sale': garageGlyph,
  // A truck from the side, heading right: its box, its cab with a window, two wheels in their arches.
  dealer(context) {
    context.fill(new Path2D('M2 6.5h12v10H2ZM15 9.5h3.6a1.5 1.5 0 0 1 1.3.8l1.8 3.3a1.5 1.5 0 0 1 .3.9v2H15Z'))
    context.save()
    context.globalCompositeOperation = 'destination-out'
    context.fillRect(16.3, 10.8, 2.4, 2.4) // the window
    for (const x of [6, 18]) {
      context.beginPath()
      context.arc(x, 17.2, 3.1, 0, Math.PI * 2)
      context.fill()
    }
    context.restore()
    for (const x of [6, 18]) {
      context.beginPath()
      context.arc(x, 17.2, 2, 0, Math.PI * 2)
      context.fill()
    }
  },
  // A depot: a works with a sawtooth roof and a chimney, a row of windows along it.
  company(context) {
    context.fill(new Path2D('M3 20.5V10.5l5 3v-3l5 3v-3l5 3V5h3v15.5Z'))
    context.save()
    context.globalCompositeOperation = 'destination-out'
    for (const x of [5.5, 10.5, 15.5]) context.fillRect(x, 15.8, 2.6, 2.2)
    context.restore()
  },
  // A bold P, its bowl open.
  parking(context) {
    context.fill(
      new Path2D('M7 3.5h6.3a5.6 5.6 0 0 1 0 11.2H10.6v5.8H7ZM10.6 6.7v4.8h2.5a2.4 2.4 0 0 0 0-4.8Z'),
      'evenodd',
    )
  },
  // A bed from the side: its headboard, the mattress with a pillow on it, its legs.
  hotel(context) {
    context.fill(new Path2D('M3 5.5h2.2v8.3H21v5.2h-2.2v-2.4H5.2v2.4H3Z'))
    context.fill(new Path2D('M10.5 9.2h7.8a2.7 2.7 0 0 1 2.7 2.7v.9H10.5Z'))
    context.beginPath()
    context.arc(7.6, 10.8, 1.9, 0, Math.PI * 2)
    context.fill()
  },
  // A person, head and shoulders.
  recruitment(context) {
    context.beginPath()
    context.arc(12, 7.8, 3.9, 0, Math.PI * 2)
    context.fill()
    context.fill(new Path2D('M4.2 20.5a7.8 7 0 0 1 15.6 0Z'))
  },
  // A weight: its body, widening down, under a ring to lift it by.
  weigh(context) {
    context.fill(
      new Path2D('M7.2 10h9.6a1 1 0 0 1 1 .8l2 8.6a1 1 0 0 1-1 1.2H5.2a1 1 0 0 1-1-1.2l2-8.6a1 1 0 0 1 1-.8Z'),
    )
    context.lineWidth = 2
    context.beginPath()
    context.arc(12, 6.4, 2.6, 0, Math.PI * 2)
    context.stroke()
  },
  // A ferry from the side: its hull, a deck house with windows, a funnel.
  ferry(context) {
    context.fill(new Path2D('M2.5 13.5h19l-2.4 5a1.5 1.5 0 0 1-1.4.9H6.3a1.5 1.5 0 0 1-1.4-.9Z'))
    context.fill(new Path2D('M6.5 9h11v3.5h-11ZM12.5 4.5h3v3.5h-3Z'))
    context.save()
    context.globalCompositeOperation = 'destination-out'
    for (const x of [8, 11, 14]) context.fillRect(x, 10.1, 1.8, 1.4)
    context.restore()
  },
  // A toll gate: its booth, and the barrier arm across the lane, striped.
  toll(context) {
    context.fill(new Path2D('M3.5 6h5v14h-5Z'))
    context.fill(new Path2D('M8.5 9.6h12.2a1.2 1.2 0 0 1 0 2.4H8.5Z'))
    context.save()
    context.globalCompositeOperation = 'destination-out'
    context.fillRect(5, 8, 2, 2.6) // the booth's window
    for (const x of [11.5, 16]) context.fillRect(x, 9.6, 2, 2.4)
    context.restore()
  },
  // A flag on its pole.
  border(context) {
    context.fillRect(5, 3.5, 2, 17)
    context.fill(new Path2D('M7 4.5h11.5l-2.6 4 2.6 4H7Z'))
  },
  // A train head on: its body with a windscreen and two lamps, on its rails.
  train(context) {
    context.fill(new Path2D('M7 3h10a3 3 0 0 1 3 3v9.5a3 3 0 0 1-3 3H7a3 3 0 0 1-3-3V6a3 3 0 0 1 3-3Z'))
    context.save()
    context.globalCompositeOperation = 'destination-out'
    context.fill(new Path2D('M7 5.8h10a.8.8 0 0 1 .8.8v3.6a.8.8 0 0 1-.8.8H7a.8.8 0 0 1-.8-.8V6.6a.8.8 0 0 1 .8-.8Z'))
    for (const x of [8.2, 15.8]) {
      context.beginPath()
      context.arc(x, 14.6, 1.3, 0, Math.PI * 2)
      context.fill()
    }
    context.restore()
    context.lineWidth = 1.8
    context.lineCap = 'round'
    context.beginPath()
    context.moveTo(8, 19)
    context.lineTo(6, 21.5)
    context.moveTo(16, 19)
    context.lineTo(18, 21.5)
    context.stroke()
  },
}
