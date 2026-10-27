import { useEffect, useRef, useState } from 'react'
import { postToHost } from '../../shared/host'
import { useLive } from '../state/store'
import { lightUnder } from './cursorLight'

/** How long the cursor stays without the mouse moving, clicking or scrolling, ms. */
const IDLE_MS = 5000
/** How often the cursor looks at what's under it: as it moves (at most), and while it stays (the page can move
 * under it), ms. */
const LOOK_MOVING = 80
const LOOK_STILL = 250

interface CursorState {
  x: number
  y: number
  pressed: boolean
}

/**
 * Where the player points on the screen while holding the interaction key (the host sends the mouse as
 * ordinary mouse events, crates/host/src/pointer.rs). WebView2 draws no system cursor offscreen, so the page
 * draws its own: a soft touch point that follows the mouse and shrinks while pressed, gone when the player
 * lets go of the key (the host then sends the mouse out of the page). Left still for `IDLE_MS` (not
 * pressed), it fades away, off the map and the art, until the mouse moves again. Over something light (a light
 * page, a light cover) it goes dark, fading between (cursorLight.ts), as Apple Music's mini player does.
 *
 * Where the host draws it (crates/host cursor.rs: into the frames the game takes, the moment the mouse moves;
 * the page's showed three or four frames late), the page draws none, and only says what's under it: light or not.
 */
export function Cursor() {
  const [cursor, setCursor] = useState<CursorState | null>(null)
  const [idle, setIdle] = useState(false)
  const [light, setLight] = useState(false)
  const where = useRef<{ x: number; y: number } | null>(null)
  const byHost = useLive((state) => state.cursorByHost)

  // What's under it, for the host drawing it: said as it changes, and again as the host starts drawing it.
  useEffect(() => {
    if (byHost) postToHost({ type: 'cursorLight', light })
  }, [byHost, light])

  // What's under it, looked at as it moves and while it stays: changed only when two looks in a row say so.
  useEffect(() => {
    let showing = false
    let pending = 0
    let lookedAt = 0
    const look = () => {
      const at = where.current
      if (!at) return
      lookedAt = performance.now()
      const wanted = lightUnder(at.x, at.y, showing)
      if (wanted === null) return
      pending = wanted === showing ? 0 : pending + 1
      if (pending >= 2) {
        showing = wanted
        pending = 0
        setLight(wanted)
      }
    }
    const moved = () => {
      if (performance.now() - lookedAt >= LOOK_MOVING) look()
    }
    window.addEventListener('pointermove', moved)
    const timer = setInterval(look, LOOK_STILL)
    return () => {
      window.removeEventListener('pointermove', moved)
      clearInterval(timer)
    }
  }, [])

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined
    // Any use of the mouse shows the cursor and starts the wait again (none while a button is held:
    // a drag can pause without the cursor leaving).
    const awake = (pressed: boolean) => {
      setIdle(false)
      clearTimeout(timer)
      if (!pressed) timer = setTimeout(() => setIdle(true), IDLE_MS)
    }
    const move = (event: PointerEvent) => {
      const pressed = (event.buttons & 1) !== 0
      setCursor({ x: event.clientX, y: event.clientY, pressed })
      where.current = { x: event.clientX, y: event.clientY }
      awake(pressed)
    }
    const wheel = (event: WheelEvent) => awake((event.buttons & 1) !== 0)
    const leave = () => {
      clearTimeout(timer)
      setCursor(null)
      where.current = null
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerdown', move)
    window.addEventListener('pointerup', move)
    window.addEventListener('wheel', wheel, { passive: true })
    document.documentElement.addEventListener('pointerleave', leave)
    return () => {
      clearTimeout(timer)
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerdown', move)
      window.removeEventListener('pointerup', move)
      window.removeEventListener('wheel', wheel)
      document.documentElement.removeEventListener('pointerleave', leave)
    }
  }, [])

  if (!cursor || byHost) return null
  return (
    // The outer element only positions; the dot inside shrinks while pressed. (Scaling the positioned
    // element itself would scale its position too, pulling the dot towards the top-left corner.)
    // Its own transition name keeps it live and on top while screens animate (shell/transitions.css).
    <div
      className={`cursor${idle ? ' idle' : ''}${light ? ' on-light' : ''}`}
      style={{ transform: `translate(${cursor.x}px, ${cursor.y}px)`, viewTransitionName: 'cursor' }}
      aria-hidden
    >
      <div className={cursor.pressed ? 'cursor-dot pressed' : 'cursor-dot'} />
    </div>
  )
}
