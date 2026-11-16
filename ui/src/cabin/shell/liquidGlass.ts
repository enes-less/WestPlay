/**
 * Liquid Glass's lens, its arrival and its touch (liquidGlass.css has the rest of the material): Apple's glass
 * "bends, shapes and concentrates light" rather than only blurring what's behind it, appears by "gradually modulating
 * the light bending and lensing" rather than fading, and lights up from under the finger (WWDC25 "Meet Liquid Glass").
 *
 * A glass comes and goes by how present it is (`--lg-arrival`, 0 to 1: liquidGlass.css), never by its opacity: its
 * frost, tint, shadow and the words on it follow that in the stylesheet, and its lens, its rim's light and its
 * adapting here (`follow`), whoever moves it (this file as a glass arrives, a component's own closing or hiding).
 *
 * The lens is worked out as light through glass, not drawn by eye (the model of kube.io's "Liquid Glass in the
 * Browser"): the pane is flat in its middle and rounds over at its rim as a convex squircle, `surface(t)`, `t` 0 at
 * the edge and 1 where the rim (`bezel` wide) meets the middle, the glass `thick` high there. A ray straight from the
 * eye meets that surface at the angle its slope gives, is bent once on the way in (Snell's law, glass's index 1.5),
 * and crosses the glass under it to the picture behind: it lands further in than it entered, by the height of the
 * glass there times the tangent of the angle it was turned. So each point of the rim shows what is that far inside
 * (`shiftAt`): almost nothing at the very edge (no glass under it), the most just inside it, falling away to none at
 * the flat middle; where it falls faster than a pixel a pixel the picture shows mirrored, as at a real pane's edge.
 *
 * That shift, inward along the rounded rectangle's normal, is each element's displacement map (red the x of it, green
 * the y, 128 none), drawn as part of its backdrop filter (`--lg-lens`) after the blur. It only ever reaches inward, so
 * it stays inside the backdrop its own box has (Chromium gives a backdrop filter nothing from outside the box). Larger
 * glass is thicker (Apple: "more pronounced lensing"): its rim wider, the glass higher.
 *
 * CarPlay's own glass (`lg-system`: the sidebar, the Dashboard's widgets) bends the other way, as Apple's does in its
 * CarPlay pictures: the band along its rim shows what is beyond its edge, drawn in (measured across Apple's sidebar:
 * a band at each side in the colour of what's outside it there; Apple: glass samples "content from an area larger
 * than itself"). Across the rim each point shows what's `OUT` further out at the very edge, easing to itself where
 * the rim meets the middle (`drawnIn`), so the band blends into the body with no line (owner 2026-11-16: "why is the
 * refraction CLEARLY visible and not smoothly blending?"). Chromium gives a backdrop filter nothing from outside its
 * element's box, and a layer larger than the glass inside it sees nothing at all once the glass is a group of its
 * own (the sidebar is, by its view-transition name; any glass fading is): so what is beyond the edge is taken as the
 * strip just inside it, mirrored about the edge. What lies there is the wallpaper running on under the glass (the
 * screen's margin, the gap between widgets), so the two are all but the same picture.
 *
 * The rim's light is in the same filter, because it is the light of what's behind, never light of its own (owner
 * 2026-11-16, of a white line round glass over a black wallpaper: "That much light rim on dark background doesnt
 * seem right"; then, with iOS 26's Home screen beside ours: "it's not always white, it just lightens whatever color
 * is behind": its dock's edge over a blue wallpaper is a lighter blue). Along the very edge what's behind is
 * brightened by a share of itself, so it keeps its colour and over black there is nothing to light; most where the
 * edge faces the light (from above left) and opposite it, where it comes back through the glass; on a control's arcs
 * between, it is darkened ("You can actually see dark parts on the rim"). Soft both, fading in from the edge ("apples
 * rim blends softly, not a hard stroke"). The map's blue channel carries it: above 128 light, below it shade.
 *
 * CarPlay's own glass lets most of what's behind through, so it also adapts to it, as Apple's does ("continuously
 * adapts to what's behind it"), a player's wallpaper being any picture in either look: where what's behind is too
 * light for the dark look's white words it is darkened, where too dark for the light look's dark words lightened, by
 * how light it is there (`--lg-adapt`, liquidGlass.css); in between, untouched. First in the filter, on the blurred
 * backdrop, so it follows the picture region by region.
 *
 * What it costs is kept down: a map is worked out once a size (shared by every glass of that size), as a few small
 * pieces, never one picture the size of the glass (`LensMap`: the compositor is sent a filter's pictures every
 * frame); while a glass is changing size its lens is put aside (the blur stays) and made once when the size has
 * settled, never once a frame.
 */

/** Glass's index of refraction. */
const INDEX = 1.5

/** How a glass surface bends: its rim's width and the glass's height where the rim meets the flat middle (px). */
export interface Lens {
  bezel: number
  thick: number
}

/** Glass whose shorter side is at least this (px, as laid out) is the thicker material (`lg-large`): the sidebar (112
 * to 139 across by the screen), the Dashboard's widgets, menus, the keyboard; bars and buttons (to 92) are not. */
const LARGE = 100

/** The glass's height over its rim's width: measured against Apple's bars (a 104 px bar: a 28 px rim, 40 high). */
const HEIGHT = 1.43

/** The rim's share of the shorter side, and the widest it gets (px): small glass, large glass. */
const RIM = 0.27
const WIDEST = 28
const WIDEST_LARGE = 36

/** The lens of a glass `width` x `height` with corners `radius`: its rim a share of its shorter side, no wider than
 * its corners are round (past them the two sides' bends would meet in a crease), larger glass thicker. */
export function lensFor(width: number, height: number, radius: number): Lens {
  const short = Math.min(width, height)
  const widest = short >= LARGE ? WIDEST_LARGE : WIDEST
  const bezel = Math.max(2, Math.min(short * RIM, widest, Math.max(radius, 8), short / 2))
  return { bezel, thick: bezel * HEIGHT }
}

/** The rim's surface, a convex squircle: its height at `t` across the rim (0 the edge, 1 the flat middle), as a share
 * of the glass's. Steep at the edge, level where it meets the middle. */
export function surface(t: number): number {
  const s = Math.min(1, Math.max(0, t))
  return (1 - (1 - s) ** 4) ** 0.25
}

/** How far inside (px) the point at `t` across the rim shows what's behind it: a ray from straight above, bent once
 * entering the surface there, across the glass under it. */
export function shiftAt(t: number, lens: Lens): number {
  const step = 1e-3
  const from = Math.max(0, t - step)
  const to = Math.min(1, t + step)
  const slope = ((surface(to) - surface(from)) / (to - from)) * (lens.thick / lens.bezel)
  const incidence = Math.atan(slope)
  const refracted = Math.asin(Math.sin(incidence) / INDEX)
  return lens.thick * surface(t) * Math.tan(incidence - refracted)
}

/** A lens's shifts across its rim, `samples` of them from the edge in, and the largest. */
export function profile(lens: Lens, samples = 128): { shifts: Float32Array; most: number } {
  const shifts = new Float32Array(samples)
  let most = 0
  for (let i = 0; i < samples; i++) {
    shifts[i] = shiftAt((i + 0.5) / samples, lens)
    if (shifts[i] > most) most = shifts[i]
  }
  return { shifts, most }
}

/** How far past its edge (px) CarPlay's own glass draws from: within the screen's margin round the sidebar and the
 * gap between widgets (16), where the wallpaper runs on as it does under the glass. */
export const OUT = 14

/** How far out from itself (px) a point `t` across the rim of CarPlay's own glass shows: `OUT` at the very edge,
 * easing to none (and level) where the rim meets the middle. Never more than a pixel a pixel less: no fold. */
export function drawnIn(t: number): number {
  const s = Math.min(1, Math.max(0, t))
  return OUT * (1 - s) ** 2
}

/** A rounded rectangle `width` x `height` with corners `radius`: how far a point is in from the edge, and the way
 * out from it there (its outward normal). */
export function edgeOf(x: number, y: number, width: number, height: number, radius: number) {
  const r = Math.min(radius, width / 2, height / 2)
  const cx = Math.abs(x - width / 2) - (width / 2 - r)
  const cy = Math.abs(y - height / 2) - (height / 2 - r)
  const sx = x < width / 2 ? -1 : 1
  const sy = y < height / 2 ? -1 : 1
  if (cx > 0 && cy > 0) {
    const length = Math.hypot(cx, cy) || 1
    return { distance: r - length, nx: (sx * cx) / length, ny: (sy * cy) / length }
  }
  const toSide = width / 2 - Math.abs(x - width / 2)
  const toEnd = height / 2 - Math.abs(y - height / 2)
  return toSide < toEnd ? { distance: toSide, nx: sx, ny: 0 } : { distance: toEnd, nx: 0, ny: sy }
}

/** The way to the light: from above left. */
const LIGHT = { x: -Math.SQRT1_2, y: -Math.SQRT1_2 }

/** The rim's light: how far in from the edge it fades (px), the share of it every edge has whichever way it faces,
 * and how much less comes back through the glass on the far side than falls on the near. */
const LIGHT_REACH = 1.3
const LIGHT_ALL_ROUND = 0.3
const LIGHT_FAR = 0.8

/** The rim's shade, on the arcs between: how far in it fades (px). */
const SHADE_REACH = 2.6

/** The rim's light at a point `distance` in from the edge whose way out is `nx, ny`: 1 the most light, -1 the most
 * shade (`shaded`: a control's; CarPlay's own glass has none), 0 neither. */
export function rimLight(distance: number, nx: number, ny: number, shaded: boolean): number {
  const facing = nx * LIGHT.x + ny * LIGHT.y
  const lobe = Math.abs(facing) ** 1.5 * (facing > 0 ? 1 : LIGHT_FAR)
  const light = (LIGHT_ALL_ROUND + (1 - LIGHT_ALL_ROUND) * lobe) * Math.exp(-((distance / LIGHT_REACH) ** 2))
  if (!shaded) return light
  // Across the light: the arcs at the top right and bottom left.
  const across = Math.abs(nx * LIGHT.y - ny * LIGHT.x)
  const shade = across ** 3 * Math.exp(-((distance / SHADE_REACH) ** 2))
  return Math.max(-1, Math.min(1, light - shade))
}

/** A piece of a glass's map: a PNG's address (red and green the bend, blue the rim's light), and where on the glass
 * it goes (px, as laid out; stretched to fit). */
interface Tile {
  href: string
  x: number
  y: number
  width: number
  height: number
}

/** A glass's map, in pieces (its four corners, and a strip a pixel thick for each straight side, stretched along it;
 * its middle is flat), and the scale that turns its bend back into pixels. In pieces because of what it costs:
 * Chromium sends a filter's picture to its compositor anew every frame the backdrop changes, so one picture the
 * size of the glass cost by the glass's area (measured: 4 ms a frame for one Dashboard widget's, on a fast card;
 * the same filter with no picture, 0.2 ms). The pieces are a few thousand pixels whatever the glass's size. */
interface LensMap {
  tiles: Tile[]
  scale: number
}

/** The maps made, by size and corners: every glass of a size shares one (the round buttons, a row of keys). */
const maps = new Map<string, LensMap>()
const MAPS_KEPT = 64

/** The map of a glass `width` x `height` with corners `radius` (px, as laid out), its rim `shaded` or not, with
 * `detail` pixels to each of those (the zoom the glass is drawn at, so its rim's fine light is as sharp as the
 * screen's own pixels on a screen whose UI is scaled up). With `out` (CarPlay's own glass), its rim draws in what's
 * that far beyond the edge. */
export function lensMap(
  width: number,
  height: number,
  radius: number,
  shaded: boolean,
  detail = 1,
  out = 0,
): LensMap {
  const key = `${width}x${height}r${radius.toFixed(1)}${shaded ? 's' : ''}o${out}@${detail.toFixed(2)}`
  const had = maps.get(key)
  if (had) return had
  const lens = lensFor(width, height, radius)
  const { shifts, most: mostIn } = profile(lens)
  const most = out > 0 ? out : mostIn
  const reach = Math.max(lens.bezel, SHADE_REACH * 2.5)
  /** The map at a point of the glass: red, green, blue. */
  const at = (x: number, y: number): [number, number, number] => {
    const edge = edgeOf(x, y, width, height, radius)
    if (edge.distance >= reach || edge.distance < 0) return [128, 128, 128]
    let along = 0
    if (edge.distance < lens.bezel && out > 0) {
      // Drawn from further out, and what would be beyond the edge from as far inside it: the way to there, along
      // the normal (out) or against it (in).
      along = edge.distance - Math.abs(edge.distance - drawnIn(edge.distance / lens.bezel))
    } else if (edge.distance < lens.bezel) {
      // Drawn from further in: the shift against the normal.
      along = -shifts[Math.min(shifts.length - 1, Math.floor((edge.distance / lens.bezel) * shifts.length))]
    }
    return [
      Math.round(128 + (127 * along * edge.nx) / most),
      Math.round(128 + (127 * along * edge.ny) / most),
      Math.round(128 + 127 * rimLight(edge.distance, edge.nx, edge.ny, shaded)),
    ]
  }
  /** A piece: the part of the glass from `x, y`, `w` x `h`, as `across` x `down` pixels. */
  const piece = (x: number, y: number, w: number, h: number, across: number, down: number): Tile => {
    const canvas = document.createElement('canvas')
    canvas.width = across
    canvas.height = down
    const context = canvas.getContext('2d')!
    const image = context.createImageData(across, down)
    for (let j = 0; j < down; j++) {
      for (let i = 0; i < across; i++) {
        const [red, green, blue] = at(x + ((i + 0.5) * w) / across, y + ((j + 0.5) * h) / down)
        image.data.set([red, green, blue, 255], (j * across + i) * 4)
      }
    }
    context.putImageData(image, 0, 0)
    return { href: canvas.toDataURL('image/png'), x, y, width: w, height: h }
  }
  // Each corner a square that holds its arc and the rim's reach; between them, the straight sides: the same all
  // along, so a strip a pixel thick, stretched.
  const round = Math.min(radius, width / 2, height / 2)
  const corner = Math.max(1, Math.min(Math.ceil(Math.max(round, reach)) + 1, Math.floor(Math.min(width, height) / 2)))
  const fine = Math.max(1, Math.round(corner * detail))
  const tiles = [
    piece(0, 0, corner, corner, fine, fine),
    piece(width - corner, 0, corner, corner, fine, fine),
    piece(0, height - corner, corner, corner, fine, fine),
    piece(width - corner, height - corner, corner, corner, fine, fine),
  ]
  const between = { x: width - 2 * corner, y: height - 2 * corner }
  if (between.x > 0) {
    tiles.push(piece(corner, 0, between.x, corner, 1, fine))
    tiles.push(piece(corner, height - corner, between.x, corner, 1, fine))
  }
  if (between.y > 0) {
    tiles.push(piece(0, corner, corner, between.y, fine, 1))
    tiles.push(piece(width - corner, corner, corner, between.y, fine, 1))
  }
  const made = { tiles, scale: most * 2 }
  if (maps.size >= MAPS_KEPT) maps.delete(maps.keys().next().value!)
  maps.set(key, made)
  return made
}

const SVG_NS = 'http://www.w3.org/2000/svg'
let holder: SVGSVGElement | null = null
let counter = 0

/** A glass's own filter: its size and corners as last made, the bend's full scale, and the frame of its lens coming
 * in (if it is). */
interface Made {
  id: string
  key: string
  filter: SVGFilterElement
  bend: SVGFEDisplacementMapElement | null
  scale: number
  frame: number
  settling: number
  /** How strongly its rim is lit and shaded, and how it adapts, with the glass all there. */
  rim: { gain: number; shade: number }
  adapt: number[] | null
  /** How present the glass is (`--lg-arrival`) as last drawn, the frame it's being followed in (0: not), and how far
   * its lens has come back after a change of size (0 to 1). */
  present: number
  following: number
  back: number
  /** The sidebar's: its box (px, as laid out) and the zoom it's drawn at, for its group's copy (`forGroup`); none
   * for any other glass. */
  group: { width: number; height: number; zoom: number } | null
}

const made = new WeakMap<HTMLElement, Made>()

/** A number a glass's style gives (liquidGlass.css: `--lg-rim-gain` and the like). */
function styled(style: CSSStyleDeclaration, name: string): number {
  return parseFloat(style.getPropertyValue(name)) || 0
}

/** A colour as the page computes it (`rgb(52, 50, 58)`, `rgba(52, 50, 58, 0.6)`): its lightness (0 to 1, its
 * channels' mean) and how solid it is. Anything else: nothing there. */
export function tintOf(colour: string): { light: number; solid: number } {
  const parts = /^rgba?\(([^)]*)\)$/.exec(colour.trim())?.[1].split(/[\s,/]+/).map(Number)
  if (!parts || parts.length < 3 || parts.some((part) => Number.isNaN(part))) return { light: 0, solid: 0 }
  return { light: (parts[0] + parts[1] + parts[2]) / 3 / 255, solid: parts[3] ?? 1 }
}

/** The most a rim's light multiplies what's behind by, beyond itself: past this every colour is white. */
const MOST_GAIN = 3

/** The filter's numbers for a rim lit by `gain` of what's behind it and shaded by `shade` of it, on glass tinted
 * `tint`. The filter works on what's behind, under the tint (the tint is the element's own colour, over its
 * backdrop), so both are made up for the share the tint covers. Only what's behind is lit, never the tint: the
 * light keeps the backdrop's colour. */
export function rimNumbers(gain: number, shade: number, tint: { light: number; solid: number }) {
  const through = Math.max(0.05, 1 - tint.solid)
  return { gain: Math.min(MOST_GAIN, gain / through), shade: Math.min(1, shade / through) }
}

/** Sets how strongly a glass's rim is lit and shaded, from its look's style (liquidGlass.css): the light a share of
 * what's behind (`--lg-rim-gain`), the shade a share taken away (`--lg-rim-shade`), under its tint (`--glass`). */
function relight(element: HTMLElement, lens: Made) {
  const style = getComputedStyle(element)
  lens.rim = rimNumbers(
    styled(style, '--lg-rim-gain'),
    styled(style, '--lg-rim-shade'),
    tintOf(style.getPropertyValue('--glass')),
  )
  const adapt = adaptOf(style)
  lens.adapt = adapt ? adapt.table.split(/\s+/).map(Number) : null
  if (adapt) {
    const colour = adapt.white ? '1' : '0'
    lens.filter.querySelectorAll('feFuncR, feFuncG, feFuncB').forEach((each) => each.setAttribute('intercept', colour))
  }
  lens.present = presenceOf(style)
  draw(lens)
}

/** How present a glass is (`--lg-arrival`, liquidGlass.css): 0 not at all, 1 all there. */
function presenceOf(style: CSSStyleDeclaration): number {
  const value = parseFloat(style.getPropertyValue('--lg-arrival'))
  return Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 1
}

/** Draws a glass's filter as present as the glass is: its bend, its rim's light and shade, its adapting. */
function draw(lens: Made) {
  const share = lens.present * lens.back
  lens.bend?.setAttribute('scale', (lens.scale * share).toFixed(2))
  const light = lens.filter.querySelector('[result="lit"]')
  light?.setAttribute('k1', (lens.rim.gain * share).toFixed(3))
  lens.filter.querySelector('[result="shade"]')?.setAttribute('values', shadeOf(lens.rim.shade * share))
  if (lens.adapt) {
    const table = lens.adapt.map((solid) => (solid * lens.present).toFixed(3)).join(' ')
    lens.filter.querySelector('feFuncA')?.setAttribute('tableValues', table)
  }
  if (lens.group) forGroup(lens, lens.group)
}

/** The sidebar's filter again for its group while the screen changes (transitions.css `::view-transition-group`),
 * which draws the sidebar's backdrop then (a snapshot has nothing behind it). The group is the page's, not the
 * CarPlay area's: it is as large as the sidebar is drawn, where the sidebar's own filter is in its laid-out pixels
 * (the area's `zoom`: 1.7 on the Scania's 7", 0.78 on the Smart Dash). So the group has a copy with every measure
 * times that zoom; with the sidebar's own, on a scaled screen the lens and the rim's light sat in the wrong place
 * until the change was over (owner 2026-11-17: "change of color on the rim only happens when animations settle"). */
let groupFilter: SVGFilterElement | null = null

function forGroup(lens: Made, group: { width: number; height: number; zoom: number }) {
  if (!holder) return
  if (!groupFilter) {
    groupFilter = document.createElementNS(SVG_NS, 'filter') as SVGFilterElement
    groupFilter.setAttribute('id', 'lg-sidebar-group')
    holder.appendChild(groupFilter)
  }
  const copy = groupFilter
  for (const name of ['x', 'y', 'filterUnits', 'primitiveUnits', 'color-interpolation-filters']) {
    copy.setAttribute(name, lens.filter.getAttribute(name) ?? '')
  }
  copy.setAttribute('width', (group.width * group.zoom).toFixed(2))
  copy.setAttribute('height', (group.height * group.zoom).toFixed(2))
  copy.innerHTML = lens.filter.innerHTML
  const scaled = (element: Element, name: string) => {
    element.setAttribute(name, (Number(element.getAttribute(name)) * group.zoom).toFixed(2))
  }
  copy.querySelectorAll('feImage').forEach((piece) => {
    for (const name of ['x', 'y', 'width', 'height']) scaled(piece, name)
  })
  copy.querySelectorAll('feDisplacementMap').forEach((bend) => scaled(bend, 'scale'))
  const page = document.documentElement.style
  page.setProperty('--lg-sidebar-lens', 'url(#lg-sidebar-group)')
  page.setProperty('--lg-sidebar-zoom', String(group.zoom))
}

/** Keeps a glass's filter as present as the glass is while that changes (its arrival, a component's closing or
 * hiding of it): each frame while anything of its is moving, and a moment after. */
function follow(element: HTMLElement) {
  const lens = made.get(element)
  if (!lens || lens.following) return
  let settled = 0
  const step = () => {
    lens.following = 0
    if (!element.isConnected || made.get(element) !== lens) return
    const present = presenceOf(getComputedStyle(element))
    if (present !== lens.present) {
      lens.present = present
      draw(lens)
      settled = 0
    } else settled++
    const moving = element.getAnimations().some((each) => each.playState === 'running')
    if (moving || settled < 12) lens.following = requestAnimationFrame(step)
  }
  step()
}

/** How a glass adapts to what's behind it (`--lg-adapt`: how solid a veil over it is, from the darkest behind to the
 * lightest, in even steps; `--lg-adapt-white`: the veil white, to lighten, or black, to darken), `null` if it
 * doesn't. */
function adaptOf(style: CSSStyleDeclaration): { table: string; white: boolean } | null {
  const table = style.getPropertyValue('--lg-adapt').trim()
  if (!table || table === 'none') return null
  return { table, white: styled(style, '--lg-adapt-white') > 0 }
}

/** How light what's behind is, as the filter takes it for the veil: its luminance, as how solid a (colourless) layer
 * is; the table then turns that into how solid the veil is, and gives the veil its colour. The colour is given there
 * and not here: a layer not solid at all (what's behind black, or nothing behind at all) has no colour to keep, and
 * a white veil made here came out of the table black over those (the light look's sidebar went dark while an app
 * opened or closed: owner 2026-11-17; its snapshot for the screen change is taken with nothing behind it). */
const LIGHTNESS_BEHIND = '0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0.2126 0.7152 0.0722 0 0'

/** The rim's light as the filter takes it from the map: white, as solid as the map's blue is above its middle
 * (twice it less one). */
const LIGHT_OF_MAP = '0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0 0 2 0 -1'

/** The rim's shade as the filter takes it from the map: black, as solid as the map's blue is below its middle, times
 * `strength`. */
function shadeOf(strength: number): string {
  return `0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 ${(-2 * strength).toFixed(3)} 0 ${strength.toFixed(3)}`
}

/** How long a lens takes to come in (the glass arriving, or settled at a new size), and how long a size must hold
 * before its lens is made (ms). */
const ARRIVAL_MS = 420
const SETTLED_MS = 140

function radiusOf(element: HTMLElement, width: number, height: number): number {
  const value = getComputedStyle(element).borderTopLeftRadius
  const px = value.endsWith('%') ? (parseFloat(value) / 100) * Math.min(width, height) : parseFloat(value)
  return Math.min(Number.isFinite(px) ? px : 0, width / 2, height / 2)
}

function still(): boolean {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

/** The size a glass's lens is made at, and its key: its laid-out size. A backdrop filter's own space is the
 * element's, in its laid-out pixels, whatever zoom it's drawn at (the screen's UI scale, `zoom` on the CarPlay area:
 * 1.7 on the Scania's 7", 0.78 on the Smart Dash) or scale (a flex, a HUD growing in): the filter is scaled with the
 * element. Made at the zoomed size instead (as it was), only the top left of the lens fell on a 7" screen's glass,
 * and on the Smart Dash the lens stopped at 78% of it, the rest of its backdrop left unfiltered. */
function sizeOf(element: HTMLElement) {
  const boxWidth = element.offsetWidth
  const height = element.offsetHeight
  if (boxWidth < 2 || height < 2) return null
  // Shown cut short at its right (liquidGlass.css `--lg-cut`): the lens is the shown shape's, the box the whole one's.
  const cut = Math.max(0, parseFloat(element.style.getPropertyValue('--lg-cut')) || 0)
  const width = Math.max(2, Math.round(boxWidth - cut))
  const radius = radiusOf(element, width, height)
  const large = Math.min(width, height) >= LARGE
  // Its rim shaded on the arcs between the light (a control's) or lit alone (CarPlay's own glass): its style says.
  const style = getComputedStyle(element)
  const shaded = styled(style, '--lg-rim-shade') > 0
  const adapted = adaptOf(style) !== null
  // Its map as fine as the screen's pixels where the UI is drawn larger (`zoom` on the CarPlay area); never coarser.
  const zoom = (element as HTMLElement & { currentCSSZoom?: number }).currentCSSZoom || 1
  const detail = Math.min(2, Math.max(1, zoom))
  // CarPlay's own glass: its rim draws in what's beyond its edge.
  const out = element.classList.contains('lg-system') ? OUT : 0
  const marks = `${shaded ? 's' : ''}${adapted ? 'a' : ''}o${out}`
  const key = `${width}x${height}r${radius.toFixed(1)}${marks}@${detail.toFixed(2)}`
  return { box: { width: boxWidth, height }, width, height, radius, large, shaded, adapted, detail, out, zoom, key }
}

/** Brings a glass's lens back in over `ms`, after it was put aside for a change of size. */
function comeBack(lens: Made, ms: number) {
  cancelAnimationFrame(lens.frame)
  if (still() || ms <= 0) {
    lens.back = 1
    draw(lens)
    return
  }
  const began = performance.now()
  const step = (now: number) => {
    const share = Math.min(1, Math.max(0, (now - began) / ms))
    lens.back = 1 - (1 - share) ** 3
    draw(lens)
    if (share < 1) lens.frame = requestAnimationFrame(step)
  }
  step(began)
}

/** Makes (or makes again, at a new size or corners) a glass's lens. */
function shape(element: HTMLElement) {
  const size = sizeOf(element)
  if (!size) return
  element.classList.toggle('lg-large', size.large)
  const had = made.get(element)
  if (had?.key === size.key) {
    relight(element, had)
    // Put aside while it was changing size, and back at the size it had.
    if (!element.style.getPropertyValue('--lg-lens')) {
      element.style.setProperty('--lg-lens', `url(#${had.id})`)
      comeBack(had, ARRIVAL_MS / 2)
    }
    return
  }
  const { tiles, scale } = lensMap(size.width, size.height, size.radius, size.shaded, size.detail, size.out)
  const lens: Made = had ?? {
    id: `lg-${++counter}`,
    key: '',
    filter: document.createElementNS(SVG_NS, 'filter') as SVGFilterElement,
    bend: null,
    scale: 0,
    frame: 0,
    settling: 0,
    rim: { gain: 0, shade: 0 },
    adapt: null,
    present: 1,
    following: 0,
    back: 1,
    group: null,
  }
  cancelAnimationFrame(lens.frame)
  lens.key = size.key
  lens.scale = scale
  // The sidebar's lens is also its group's while the screen changes (`forGroup`).
  lens.group = element.classList.contains('sidebar') ? { ...size.box, zoom: size.zoom } : null
  const { filter } = lens
  filter.setAttribute('id', lens.id)
  filter.setAttribute('x', '0')
  filter.setAttribute('y', '0')
  filter.setAttribute('width', String(size.box.width))
  filter.setAttribute('height', String(size.box.height))
  filter.setAttribute('filterUnits', 'userSpaceOnUse')
  filter.setAttribute('primitiveUnits', 'userSpaceOnUse')
  filter.setAttribute('color-interpolation-filters', 'sRGB')
  // Adapted to what's behind, where it is (a veil over it, as solid as its lightness says); the bend; then the rim's
  // light, what's behind times it; then, on a shaded rim, the shade over that. `relight` sets how much of each.
  // The veil and the shade are laid only on what's there (`atop`): with nothing behind (the glass's snapshot for a
  // screen change) the filter leaves nothing, the glass's own colour and words alone going into the snapshot.
  const colour = ['R', 'G', 'B'].map((channel) => `<feFunc${channel} type="linear" slope="0" intercept="0" />`).join('')
  filter.innerHTML =
    (size.adapted
      ? `<feColorMatrix in="SourceGraphic" type="matrix" values="${LIGHTNESS_BEHIND}" result="veil" />` +
        `<feComponentTransfer in="veil" result="veiled">${colour}` +
        `<feFuncA type="table" tableValues="0 0" /></feComponentTransfer>` +
        `<feComposite in="veiled" in2="SourceGraphic" operator="atop" result="behind" />`
      : '') +
    `<feFlood flood-color="rgb(128 128 128)" result="flat" />` +
    tiles
      .map(
        (tile, i) =>
          `<feImage href="${tile.href}" x="${tile.x}" y="${tile.y}" width="${tile.width}" ` +
          `height="${tile.height}" preserveAspectRatio="none" result="piece${i}" />`,
      )
      .join('') +
    `<feMerge result="map"><feMergeNode in="flat" />` +
    tiles.map((_, i) => `<feMergeNode in="piece${i}" />`).join('') +
    `</feMerge>` +
    `<feDisplacementMap in="${size.adapted ? 'behind' : 'SourceGraphic'}" in2="map" ` +
    `scale="0" xChannelSelector="R" yChannelSelector="G" result="bent" />` +
    `<feColorMatrix in="map" type="matrix" values="${LIGHT_OF_MAP}" result="light" />` +
    `<feComposite in="bent" in2="light" operator="arithmetic" k1="0" k2="1" k3="0" k4="0" result="lit" />` +
    (size.shaded
      ? `<feColorMatrix in="map" type="matrix" values="${shadeOf(0)}" result="shade" />` +
        `<feComposite in="shade" in2="lit" operator="atop" />`
      : '')
  lens.bend = filter.querySelector('feDisplacementMap')
  if (!had) holder?.appendChild(filter)
  made.set(element, lens)
  relight(element, lens)
  element.style.setProperty('--lg-lens', `url(#${lens.id})`)
  // Back after a change of size: the lens comes in again.
  if (had) comeBack(lens, ARRIVAL_MS / 2)
}

/** A glass changed size: its lens is put aside (a map made for another size would cut its backdrop off), and made
 * again once the size has held. */
function resized(element: HTMLElement) {
  if (element.classList.contains('no-lens')) return
  const lens = made.get(element)
  if (!lens) {
    shape(element)
    return
  }
  const size = sizeOf(element)
  if (!size) return
  if (size.key === lens.key) {
    // Its state changed, not its size: lit as it is now, and followed if it is coming or going.
    relight(element, lens)
    follow(element)
    return
  }
  element.classList.toggle('lg-large', size.large)
  element.style.removeProperty('--lg-lens')
  window.clearTimeout(lens.settling)
  lens.settling = window.setTimeout(() => {
    if (element.isConnected) shape(element)
  }, SETTLED_MS)
}

function forget(element: HTMLElement) {
  const lens = made.get(element)
  if (!lens) return
  cancelAnimationFrame(lens.frame)
  cancelAnimationFrame(lens.following)
  window.clearTimeout(lens.settling)
  lens.filter.remove()
  made.delete(element)
}

/** A glass arriving on the screen materialises: it goes from not there to all there (`--lg-arrival`), over whatever
 * way the element itself comes (a slide, a spring). */
function materialise(element: HTMLElement) {
  if (still() || typeof element.animate !== 'function') return
  element.animate([{ '--lg-arrival': 0 } as Keyframe, { '--lg-arrival': 1 } as Keyframe], {
    duration: ARRIVAL_MS,
    easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
  })
}

/** Lights a pressed glass from under the finger (liquidGlass.css `lg-lit`), until it's let go. */
function onPress(event: PointerEvent) {
  const target = event.target instanceof Element ? event.target : null
  const glass = target?.closest<HTMLElement>('.glass')
  if (!glass || !target?.closest('button, [role="button"], a, input')) return
  const box = glass.getBoundingClientRect()
  glass.style.setProperty('--lg-x', `${((event.clientX - box.left) / Math.max(1, box.width)) * 100}%`)
  glass.style.setProperty('--lg-y', `${((event.clientY - box.top) / Math.max(1, box.height)) * 100}%`)
  glass.classList.add('lg-lit')
  const release = () => {
    glass.classList.remove('lg-lit')
    window.removeEventListener('pointerup', release, true)
    window.removeEventListener('pointercancel', release, true)
  }
  window.addEventListener('pointerup', release, true)
  window.addEventListener('pointercancel', release, true)
}

/** Watches the page for glass: each one's lens made as it comes (those arriving later materialise), put aside while
 * it changes size and made again when it settles, dropped as it goes; and pressed glass lit. Returns how to stop. */
export function startLiquidGlass(root: HTMLElement = document.body): () => void {
  holder = document.createElementNS(SVG_NS, 'svg')
  holder.setAttribute('aria-hidden', 'true')
  holder.style.cssText = 'position:absolute;width:0;height:0;overflow:hidden;pointer-events:none'
  document.body.appendChild(holder)
  const watched = new Set<HTMLElement>()
  // The first report of each is its size as it is (its lens made by `take`): only later ones are changes.
  const sizes = new ResizeObserver((entries) => {
    for (const entry of entries) resized(entry.target as HTMLElement)
  })
  let started = false
  const take = (element: HTMLElement) => {
    if (watched.has(element)) return
    watched.add(element)
    // Over what the page can't see (a video the host draws under it), nothing to bend: its frost alone arrives.
    if (!element.classList.contains('no-lens')) {
      shape(element)
      sizes.observe(element)
    }
    if (started) materialise(element)
    follow(element)
  }
  const drop = (element: HTMLElement) => {
    if (!watched.delete(element)) return
    sizes.unobserve(element)
    element.style.removeProperty('--lg-lens')
    forget(element)
  }
  const scan = (node: Node) => {
    if (!(node instanceof HTMLElement)) return
    if (node.classList.contains('glass')) take(node)
    node.querySelectorAll<HTMLElement>('.glass').forEach(take)
  }
  const unscan = (node: Node) => {
    if (!(node instanceof HTMLElement)) return
    if (watched.has(node)) drop(node)
    node.querySelectorAll<HTMLElement>('.glass').forEach(drop)
  }
  scan(root)
  started = true
  const changes = new MutationObserver((records) => {
    for (const record of records) {
      if (record.type === 'attributes') {
        const element = record.target as HTMLElement
        // A glass whose state changed may be cut differently (`--lg-cut`) or lit differently (`lg-light`): its lens
        // follows. The cab's look changed (the shell's class): every glass is lit anew.
        if (element.classList.contains('shell')) watched.forEach(resized)
        else if (!element.classList.contains('glass')) drop(element)
        else if (watched.has(element)) resized(element)
        else take(element)
        continue
      }
      record.removedNodes.forEach(unscan)
      record.addedNodes.forEach(scan)
    }
  })
  changes.observe(root, { childList: true, subtree: true, attributes: true, attributeFilter: ['class'] })
  // A glass coming or going by a transition or an animation of its own, or of what holds it (the volume pill's
  // holder hides it): followed.
  const onMoving = (event: Event) => {
    if (!(event.target instanceof HTMLElement)) return
    if (watched.has(event.target)) follow(event.target)
    else if ((event as TransitionEvent).propertyName === '--lg-arrival') {
      event.target.querySelectorAll<HTMLElement>('.glass').forEach(follow)
    }
  }
  document.addEventListener('transitionrun', onMoving, true)
  document.addEventListener('animationstart', onMoving, true)
  window.addEventListener('pointerdown', onPress, true)
  return () => {
    changes.disconnect()
    sizes.disconnect()
    document.removeEventListener('transitionrun', onMoving, true)
    document.removeEventListener('animationstart', onMoving, true)
    window.removeEventListener('pointerdown', onPress, true)
    watched.forEach(drop)
    holder?.remove()
    holder = null
    groupFilter = null
  }
}
