import { type RefObject, useEffect, useState } from 'react'

/** A box on the page, in page pixels (a `DOMRect`'s fields). */
export interface Box {
  left: number
  top: number
  width: number
  height: number
}

/** How far in from the CarPlay area's left edge the pointer brings the sidebar out, as a share of its width. */
export const EDGE_ZONE = 0.06
/** The band of the edge that does it: the middle of its height (clear of the player's own corner buttons). */
export const MIDDLE_FROM = 0.3
export const MIDDLE_TO = 0.7
/** Once out, it stays while the pointer is over it or this close beside it, page pixels. */
export const STAY_MARGIN = 48

/**
 * Whether the sidebar is out over the full-screen player, as the pointer moves: brought out by pointing at
 * the left edge, near the middle; kept while the pointer is on it or just beside it; gone once the pointer
 * moves off to the right, or leaves the screen (`pointer` null: the player let go of the interaction key).
 * `sidebarRight` is where the sidebar ends while out.
 */
export function sidebarPeeks(
  peeking: boolean,
  pointer: { x: number; y: number } | null,
  area: Box,
  sidebarRight: number,
): boolean {
  if (!pointer) return false
  const x = pointer.x - area.left
  const y = (pointer.y - area.top) / area.height
  if (x < 0 || y < 0 || y > 1) return false
  if (x <= area.width * EDGE_ZONE && y >= MIDDLE_FROM && y <= MIDDLE_TO) return true
  return peeking && pointer.x <= Math.max(sidebarRight, area.left + area.width * EDGE_ZONE) + STAY_MARGIN
}

/**
 * The sidebar peeking out over the full-screen player (`watching`), from the pointer over `area` (the CarPlay
 * area, holding the `.sidebar`). Always `false` while not watching.
 */
export function useSidebarPeek(watching: boolean, area: RefObject<HTMLElement | null>): boolean {
  const [peeking, setPeeking] = useState(false)

  useEffect(() => {
    if (!watching) return
    let current = false
    const update = (pointer: { x: number; y: number } | null) => {
      const element = area.current
      if (!element) return
      const sidebar = element.querySelector('.sidebar')
      const next = sidebarPeeks(
        current,
        pointer,
        element.getBoundingClientRect(),
        sidebar?.getBoundingClientRect().right ?? 0,
      )
      if (next !== current) {
        current = next
        setPeeking(next)
      }
    }
    const move = (event: PointerEvent) => update({ x: event.clientX, y: event.clientY })
    const leave = () => update(null)
    window.addEventListener('pointermove', move)
    document.documentElement.addEventListener('pointerleave', leave)
    return () => {
      window.removeEventListener('pointermove', move)
      document.documentElement.removeEventListener('pointerleave', leave)
      // Back in the sidebar's place when the player closes: watching again starts with it aside.
      setPeeking(false)
    }
  }, [watching, area])

  return watching && peeking
}
