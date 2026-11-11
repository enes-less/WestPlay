import { type RefObject, useLayoutEffect } from 'react'
import { create } from 'zustand'
import type { Hole } from '../youtube/youtube'

/**
 * Where Safari's page shows through the cab: the CarPlay area is cut open there (Shell.tsx), the page being under
 * it (WebPage.tsx); and what of the cab is drawn over the page all the same (`covers`: the address field's
 * suggestions, a page's dialog, the page menu), the cut closed again there and the mouse the cab's there. All in
 * the CarPlay area's own pixels, from its top left (as YouTube's `Hole`).
 */
interface PageHole {
  hole: Hole | null
  covers: Record<string, Cover>
}

/**
 * What of the cab is drawn over the page: its rectangle, and its shadow (CSS `box-shadow`, its outer ones only, in
 * the CarPlay area's pixels; none: it has none). The shadow falls on the page through the cut (Shell.tsx), not cut
 * at the cover's edge with the rest of the cab there (owner 2026-11-08: "shadow clip issue").
 */
export interface Cover extends Hole {
  shadow?: string
}

/** `element`'s outer shadows (its `box-shadow` less the inset ones), their lengths times `zoom`; none: `undefined`. */
export function outerShadow(boxShadow: string, zoom: number): string | undefined {
  if (!boxShadow || boxShadow === 'none') return undefined
  const shadows: string[] = []
  let depth = 0
  let start = 0
  for (let i = 0; i <= boxShadow.length; i++) {
    const c = boxShadow[i]
    if (c === '(') depth++
    else if (c === ')') depth--
    else if ((c === ',' && depth === 0) || i === boxShadow.length) {
      shadows.push(boxShadow.slice(start, i).trim())
      start = i + 1
    }
  }
  const outer = shadows
    .filter((shadow) => shadow && !/\binset\b/.test(shadow))
    .map((shadow) =>
      shadow.replace(/(-?[\d.]+)px/g, (_, n: string) => `${Math.round(parseFloat(n) * zoom * 100) / 100}px`),
    )
  return outer.length > 0 ? outer.join(', ') : undefined
}

export const usePageHole = create<PageHole>(() => ({ hole: null, covers: {} }))

/** The rectangle `element` takes of the CarPlay area (its pixels), its rounding included; none when not laid out. */
export function rectInArea(element: Element): Hole | null {
  const area = document.querySelector('.carplay')?.getBoundingClientRect()
  const box = element.getBoundingClientRect()
  if (!area || box.width === 0 || box.height === 0) return null
  const own = (element as HTMLElement).offsetWidth || box.width
  const style = getComputedStyle(element)
  const zoom = box.width / own
  const radius = parseFloat(style.borderBottomLeftRadius) * zoom
  const top = parseFloat(style.borderTopLeftRadius) * zoom
  return {
    x: box.left - area.left,
    y: box.top - area.top,
    width: box.width,
    height: box.height,
    radius,
    ...(top !== radius ? { top } : {}),
  }
}

let covers = 0

/** Whether `element` is moving or changing by a transition or an animation of its own (a transform, which no size
 * change tells of: the keyboard rising, a menu growing in). */
function moving(element: HTMLElement): boolean {
  return element.isConnected && element.getAnimations().some((animation) => animation.playState === 'running')
}

function sameCover(a: Cover | undefined, b: Cover | null): boolean {
  return (
    !!a &&
    !!b &&
    a.x === b.x &&
    a.y === b.y &&
    a.width === b.width &&
    a.height === b.height &&
    a.radius === b.radius &&
    a.top === b.top &&
    a.shadow === b.shadow
  )
}

/**
 * `element` drawn over Safari's page while `on`: the cut closed there (it shows, its shadow on the page), the mouse
 * the cab's there. Its rectangle followed every frame as it moves (rising, sliding, growing in), and as it changes
 * size; let go once it's still after `on` ends, as it goes away (the keyboard sliding down: seen all the way).
 */
export function useCover(element: RefObject<HTMLElement | null>, on: boolean): void {
  useLayoutEffect(() => {
    const node = element.current
    if (!on || !node) return
    const key = `cover-${++covers}`
    const place = () => {
      const rect = rectInArea(node)
      const box = node.getBoundingClientRect()
      const zoom = box.width / (node.offsetWidth || box.width || 1)
      const cover: Cover | null = rect ? { ...rect, shadow: outerShadow(getComputedStyle(node).boxShadow, zoom) } : null
      if (!cover?.shadow) delete cover?.shadow
      usePageHole.setState((state) => {
        if (sameCover(state.covers[key], cover)) return state
        const { [key]: _, ...others } = state.covers
        return { covers: cover ? { ...others, [key]: cover } : others }
      })
    }
    const remove = () =>
      usePageHole.setState((state) => {
        const { [key]: _, ...others } = state.covers
        return { covers: others }
      })
    // Every frame while it moves.
    let frame = 0
    let letGo = false
    const follow = () => {
      frame = 0
      place()
      if (moving(node)) frame = requestAnimationFrame(follow)
      else if (letGo) remove()
    }
    const started = (event: Event) => {
      if (event.target === node && !frame) frame = requestAnimationFrame(follow)
    }
    follow()
    const resized = new ResizeObserver(place)
    resized.observe(node)
    node.addEventListener('transitionrun', started)
    node.addEventListener('animationstart', started)
    return () => {
      resized.disconnect()
      node.removeEventListener('transitionrun', started)
      node.removeEventListener('animationstart', started)
      cancelAnimationFrame(frame)
      frame = 0
      // Going away by a transition (the class that moves it out changed with `on`, its transition starting this
      // frame): followed until it's still.
      letGo = true
      requestAnimationFrame(() => {
        if (moving(node)) follow()
        else remove()
      })
    }
  }, [element, on])
}

/** A rounded rectangle as an image, for a CSS mask (its top corners rounded by `top` where that's given). */
function shapeOf({ width, height, radius, top = radius }: Hole): string {
  const [w, h] = [width, height]
  const [t, b] = [Math.min(top, w / 2, h / 2), Math.min(radius, w / 2, h / 2)]
  const path = `M${t} 0H${w - t}A${t} ${t} 0 0 1 ${w} ${t}V${h - b}A${b} ${b} 0 0 1 ${w - b} ${h}H${b}A${b} ${b} 0 0 1 0 ${h - b}V${t}A${t} ${t} 0 0 1 ${t} 0Z`
  const shape = `<svg xmlns='http://www.w3.org/2000/svg' width='${w}' height='${h}'><path d='${path}'/></svg>`
  return `url("data:image/svg+xml,${encodeURIComponent(shape)}")`
}

/** A mask showing only `hole`: what's drawn over the page's shadows falls on it there alone (Shell.tsx). */
export function onlyHole(hole: Hole): Record<string, string> {
  return {
    maskImage: shapeOf(hole),
    maskPosition: `${hole.x}px ${hole.y}px`,
    maskSize: `${hole.width}px ${hole.height}px`,
    maskRepeat: 'no-repeat',
  }
}

/**
 * A mask showing everything but `hole`, through which what's under the page shows, less `covers` (shown all the
 * same). The layers top first, each put with what's under it: the covers added, the whole page less the hole, the
 * hole itself last.
 */
export function holeMask(hole: Hole, covers: readonly Hole[] = []): Record<string, string> {
  const layers = [
    ...covers.map((cover) => ({ image: shapeOf(cover), at: cover, op: 'add' })),
    { image: 'linear-gradient(#000 0 0)', at: null, op: 'exclude' },
    { image: shapeOf(hole), at: hole, op: 'add' },
  ]
  return {
    maskImage: layers.map((layer) => layer.image).join(', '),
    maskPosition: layers.map((layer) => (layer.at ? `${layer.at.x}px ${layer.at.y}px` : '0 0')).join(', '),
    maskSize: layers.map((layer) => (layer.at ? `${layer.at.width}px ${layer.at.height}px` : '100% 100%')).join(', '),
    maskRepeat: layers.map(() => 'no-repeat').join(', '),
    maskComposite: layers.map((layer) => layer.op).join(', '),
  }
}
