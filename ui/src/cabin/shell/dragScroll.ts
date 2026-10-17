/**
 * Drag to scroll, as on a touch screen: the cab has a mouse, not a finger, so a list follows the cursor while
 * the button is held, and glides on when let go (or settles on its nearest snap point, as Home's pages do).
 * One handler for the whole page: whatever scrolls, scrolls this way.
 *
 * A drag goes to the nearest list that scrolls the way it goes (sideways on YouTube's chips moves the chips;
 * up or down there moves the list they're in). What it's let go over isn't clicked. Controls that handle a
 * drag of their own (sliders, the player's bar, the keyboard, the map) are left alone.
 */

/** How far (page pixels) the cursor moves with the button held before it's a drag, not a click. */
export const DRAG_FROM = 8

/** Glide: the speed the list carries on with (pixels per millisecond) falls by this share each millisecond. */
export const GLIDE_FALL = 0.004
/** Below this speed (pixels per millisecond) the glide stops. */
export const GLIDE_STOP = 0.02
/** The speed let go with: from how the cursor moved in the last this many milliseconds. */
export const SPEED_WINDOW = 80

/** Not dragged to scroll: controls with a drag of their own. */
const OWN_DRAG = '[role="slider"], input, textarea, select, .yt-bar, .keyboard, .maplibregl-canvas, [data-own-drag]'

export type Axis = 'x' | 'y'

/** The way a drag goes, once it's one: `null` while it's still within `DRAG_FROM` of where it began. */
export function dragAxis(dx: number, dy: number): Axis | null {
  if (Math.hypot(dx, dy) < DRAG_FROM) return null
  return Math.abs(dx) > Math.abs(dy) ? 'x' : 'y'
}

/** The speed (pixels per millisecond) of the last `SPEED_WINDOW` ms of `samples` (time ms, position px). */
export function releaseSpeed(samples: readonly [number, number][]): number {
  if (samples.length < 2) return 0
  const [lastTime, lastAt] = samples[samples.length - 1]
  const first = samples.find(([time]) => lastTime - time <= SPEED_WINDOW) ?? samples[0]
  const elapsed = lastTime - first[0]
  return elapsed > 0 ? (lastAt - first[1]) / elapsed : 0
}

/** How far a glide at `speed` (pixels per millisecond) carries on, in all: the sum of its falling speed. */
export function glideDistance(speed: number): number {
  return Math.abs(speed) < GLIDE_STOP ? 0 : speed / GLIDE_FALL
}

function scrollsAlong(element: Element, axis: Axis): boolean {
  const style = getComputedStyle(element)
  const overflow = axis === 'x' ? style.overflowX : style.overflowY
  if (overflow !== 'auto' && overflow !== 'scroll') return false
  return axis === 'x' ? element.scrollWidth > element.clientWidth + 1 : element.scrollHeight > element.clientHeight + 1
}

/** The nearest element from `start` up that scrolls along `axis`. */
function scrollerFor(start: Element, axis: Axis): HTMLElement | null {
  for (let element: Element | null = start; element; element = element.parentElement) {
    if (element instanceof HTMLElement && scrollsAlong(element, axis)) return element
  }
  return null
}

function scrollOf(element: HTMLElement, axis: Axis): number {
  return axis === 'x' ? element.scrollLeft : element.scrollTop
}

function setScroll(element: HTMLElement, axis: Axis, value: number): void {
  if (axis === 'x') element.scrollLeft = value
  else element.scrollTop = value
}

/** Starts drag-to-scroll on the page; returns the function that stops it. */
export function installDragScroll(): () => void {
  let press: { target: Element; x: number; y: number; pointer: number } | null = null
  let drag: {
    scroller: HTMLElement
    axis: Axis
    from: number
    start: number
    samples: [number, number][]
    snap: string
  } | null = null
  let gliding = 0

  const down = (event: PointerEvent) => {
    cancelAnimationFrame(gliding)
    if (event.button !== 0 || !(event.target instanceof Element) || event.target.closest(OWN_DRAG)) return
    press = { target: event.target, x: event.clientX, y: event.clientY, pointer: event.pointerId }
  }

  const move = (event: PointerEvent) => {
    if (!press || event.pointerId !== press.pointer) return
    if (!drag) {
      const axis = dragAxis(event.clientX - press.x, event.clientY - press.y)
      if (!axis) return
      const scroller = scrollerFor(press.target, axis)
      if (!scroller) {
        press = null
        return
      }
      // Snapping would pull the list back each move: off while it's dragged.
      const snap = scroller.style.scrollSnapType
      scroller.style.scrollSnapType = 'none'
      const from = axis === 'x' ? press.x : press.y
      drag = { scroller, axis, from, start: scrollOf(scroller, axis), samples: [], snap }
    }
    const at = drag.axis === 'x' ? event.clientX : event.clientY
    setScroll(drag.scroller, drag.axis, drag.start - (at - drag.from))
    drag.samples.push([event.timeStamp, at])
    if (drag.samples.length > 32) drag.samples.shift()
  }

  const up = (event: PointerEvent) => {
    if (!press || event.pointerId !== press.pointer) return
    press = null
    if (!drag) return
    const { scroller, axis, samples, snap } = drag
    drag = null
    // What it's let go over isn't clicked: the drag was the gesture.
    const swallow = (click: Event) => {
      click.stopPropagation()
      click.preventDefault()
    }
    window.addEventListener('click', swallow, { capture: true, once: true })
    setTimeout(() => window.removeEventListener('click', swallow, { capture: true }), 0)

    const speed = -releaseSpeed(samples)
    scroller.style.scrollSnapType = snap
    if (getComputedStyle(scroller).scrollSnapType !== 'none') {
      // A list that snaps settles on a snap point, carried on the way it was flung.
      const by = glideDistance(speed) * 0.35
      scroller.scrollBy(axis === 'x' ? { left: by, behavior: 'smooth' } : { top: by, behavior: 'smooth' })
      return
    }
    let velocity = speed
    let last = performance.now()
    const glide = (now: number) => {
      const elapsed = Math.min(now - last, 50)
      last = now
      setScroll(scroller, axis, scrollOf(scroller, axis) + velocity * elapsed)
      velocity *= (1 - GLIDE_FALL) ** elapsed
      if (Math.abs(velocity) >= GLIDE_STOP) gliding = requestAnimationFrame(glide)
    }
    if (Math.abs(velocity) >= GLIDE_STOP) gliding = requestAnimationFrame(glide)
  }

  window.addEventListener('pointerdown', down, true)
  window.addEventListener('pointermove', move, true)
  window.addEventListener('pointerup', up, true)
  window.addEventListener('pointercancel', up, true)
  return () => {
    cancelAnimationFrame(gliding)
    window.removeEventListener('pointerdown', down, true)
    window.removeEventListener('pointermove', move, true)
    window.removeEventListener('pointerup', up, true)
    window.removeEventListener('pointercancel', up, true)
  }
}
