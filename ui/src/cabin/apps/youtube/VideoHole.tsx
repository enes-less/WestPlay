import { type ReactNode, useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { whenTransitionEnds } from '../../state/launch'
import { useCabin } from '../../state/store'
import { command, followArea, useYouTube } from './youtube'

/** How long the CarPlay area takes at most to grow back from the truck's screen, ms (its spring). */
const GROW_MS = 900

/**
 * Where the video playing shows outside the full-screen player (YouTube's Now playing, the Dashboard's): its
 * picture is YouTube's own, placed under the page by the host, and the CarPlay area is cut open over this
 * element (Shell.tsx), its rounding included. `crop`: the picture fills it, cropped (otherwise all of it
 * shows, with bars). Placed again whenever it may have moved.
 *
 * The picture isn't part of the page, so it can't slide or fade with it: while its screen comes or goes
 * (Home and the Dashboard trading places), the video's thumbnail stands in, moving with the screen, and the
 * picture takes over where it stops. Outside WestPlay (a plain browser) the thumbnail is all there is.
 */
export function VideoHole({ className, crop = false }: { className: string; crop?: boolean }) {
  const thumbnail = useYouTube((store) => store.current?.thumbnail ?? null)
  // The screen handed to the truck (Alt+W): the CarPlay area shrinks away, and the picture goes at once.
  const handedOver = useCabin((state) => state.truckScreen)
  const frame = useRef<HTMLSpanElement>(null)
  // Handed over before this placing: the CarPlay area is growing back.
  const returning = useRef(false)
  useEffect(() => {
    const element = frame.current
    if (!element) return
    if (handedOver) {
      returning.current = true
      return
    }
    // The screen it's on, while it comes or goes (CabinApp.tsx's `Screens`).
    const slot = element.closest('.screen-slot')
    const moving = () => !!slot && (slot.classList.contains('entering') || slot.classList.contains('leaving'))
    const place = () => {
      const area = document.querySelector('.carplay')?.getBoundingClientRect()
      const box = element.getBoundingClientRect()
      if (!area || box.width === 0) return
      // Its rounding in screen pixels (the CarPlay area's content is zoomed to the screen's scale).
      const zoom = box.width / element.offsetWidth
      const radius = parseFloat(getComputedStyle(element).borderTopLeftRadius) * zoom
      useYouTube.setState({
        hole: { x: box.left - area.left, y: box.top - area.top, width: box.width, height: box.height, radius },
      })
      const scale = window.devicePixelRatio
      command({
        action: 'show',
        x: Math.round(box.left * scale),
        y: Math.round(box.top * scale),
        width: Math.round(box.width * scale),
        height: Math.round(box.height * scale),
        crop,
      })
    }
    let following: (() => void) | null = null
    let settling: ReturnType<typeof setTimeout>[] = []
    let gone = false
    const follow = () => {
      if (gone || following || moving()) return
      following = followArea(place, element)
      // An app's window grows in from its icon (a transform, which no size change tells of): placed again
      // as it settles.
      settling = [120, 350, 700].map((ms) => setTimeout(place, ms))
    }
    // On a screen still coming in, the picture waits for it to stop (a screen going never shows it).
    const stopped = slot ? new MutationObserver(follow) : null
    if (slot) stopped?.observe(slot, { attributes: true, attributeFilter: ['class'] })
    // The CarPlay area growing back from the truck's screen (a transform, which nothing tells of): the
    // picture once it's grown (at the latest a moment after).
    const area = document.querySelector('.carplay')
    let grown: ReturnType<typeof setTimeout> | undefined
    const grew = (event: Event) => {
      if (event.target !== area || (event as TransitionEvent).propertyName !== 'transform') return
      area?.removeEventListener('transitionend', grew)
      clearTimeout(grown)
      follow()
    }
    if (returning.current && area) {
      returning.current = false
      area.addEventListener('transitionend', grew)
      grown = setTimeout(() => {
        area.removeEventListener('transitionend', grew)
        follow()
      }, GROW_MS)
    } else {
      returning.current = false
      // A screen change animating (the full-screen player closing onto the Dashboard): the picture where it
      // was, under the page fading over it, until the change ends (youtube.ts `leavePlayer`).
      whenTransitionEnds(follow)
    }
    return () => {
      gone = true
      stopped?.disconnect()
      area?.removeEventListener('transitionend', grew)
      clearTimeout(grown)
      settling.forEach(clearTimeout)
      if (!following) return
      following()
      useYouTube.setState({ hole: null })
      // Gone (whatever shows it next places it again).
      command({ action: 'hide' })
    }
  }, [crop, handedOver])
  // The thumbnail, under the picture (cut out with the page while the picture shows).
  return (
    <span ref={frame} className={className}>
      {thumbnail && <img src={thumbnail} alt="" />}
    </span>
  )
}

/**
 * Content over the video where it shows through the page (`VideoHole`): drawn in its own layer above the
 * CarPlay area (anything in the CarPlay area there is cut out with it), the same size, at the same scale.
 */
export function OverVideo({ children }: { children: ReactNode }) {
  const hole = useYouTube((store) => store.hole)
  // The layer is the shell's (Shell.tsx): there by the time a hole is (a VideoHole placed after it).
  const layer = hole ? document.getElementById('video-over') : null
  if (!layer || !hole) return null
  return createPortal(
    <div
      className="over-video"
      style={{ left: hole.x, top: hole.y, width: hole.width, height: hole.height, borderRadius: hole.radius }}
    >
      <div
        className="over-video-content"
        style={{
          width: `calc(${hole.width}px / var(--ui-scale, 1))`,
          height: `calc(${hole.height}px / var(--ui-scale, 1))`,
        }}
      >
        {children}
      </div>
    </div>,
    layer,
  )
}
