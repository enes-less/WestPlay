import { type RefObject, useCallback, useEffect, useRef, useState } from 'react'
import { DEMO_CHANNEL, type DemoCommand, type DemoReport } from '../../cabin/demo'
import { call, windowInFront } from '../bridge'

/** A box on the screen, in its own pixels (1280x720). */
export interface Rect {
  x: number
  y: number
  width: number
  height: number
}

/** Whether the companion's window is in front now: not minimised, and the one Windows has in front. */
async function inFront(): Promise<boolean> {
  return document.visibilityState === 'visible' && windowInFront()
}

/** The live WestPlay in the screen (cabin.html in demo mode, cabin/demo.ts): commands to it, its icons'
 * places from it. While the window isn't in front, the room and the demo in it hold still (`away`: what never
 * ends paused, nothing drawn), and go on as it comes back. */
export function useDemo(): {
  frame: RefObject<HTMLIFrameElement | null>
  ready: boolean
  /** Where it is now (`home`, `dashboard`, `app:music`), as it last said. */
  route: string
  send(command: DemoCommand): void
  locate(target: string): Promise<Rect | null>
} {
  const frame = useRef<HTMLIFrameElement>(null)
  const [ready, setReady] = useState(false)
  const [route, setRoute] = useState('home')
  const waiting = useRef(new Map<number, (rect: Rect | null) => void>())
  const nextId = useRef(1)

  useEffect(() => {
    const receive = (event: MessageEvent) => {
      if (event.origin !== location.origin || event.source !== frame.current?.contentWindow) return
      const data = event.data as { channel?: string } & DemoReport
      if (data?.channel !== DEMO_CHANNEL) return
      if (data.type === 'ready') setReady(true)
      if (data.type === 'route') setRoute(data.route)
      if (data.type === 'located') {
        waiting.current.get(data.id)?.(data.rect)
        waiting.current.delete(data.id)
      }
    }
    window.addEventListener('message', receive)
    return () => window.removeEventListener('message', receive)
  }, [])

  const send = useCallback((command: DemoCommand) => {
    frame.current?.contentWindow?.postMessage({ channel: DEMO_CHANNEL, ...command }, location.origin)
  }, [])

  // In front or not: the page marked, and the demo told (again as it loads). Asked each second too: no event
  // says so when the window comes up behind another.
  useEffect(() => {
    let active = true
    let held: boolean | null = null
    const follow = () => {
      inFront().then((front) => {
        const away = !front
        if (!active || away === held) return
        held = away
        document.documentElement.toggleAttribute('data-away', away)
        send({ type: 'away', away })
        call('room_held', { held: away }).catch((error: unknown) => console.error('Not logged', error))
      }, console.error)
    }
    follow()
    const asking = window.setInterval(follow, 1000)
    window.addEventListener('focus', follow)
    window.addEventListener('blur', follow)
    document.addEventListener('visibilitychange', follow)
    return () => {
      active = false
      window.clearInterval(asking)
      window.removeEventListener('focus', follow)
      window.removeEventListener('blur', follow)
      document.removeEventListener('visibilitychange', follow)
      document.documentElement.removeAttribute('data-away')
    }
  }, [send, ready])
  const locate = useCallback(
    (target: string) =>
      new Promise<Rect | null>((resolve) => {
        const id = nextId.current++
        waiting.current.set(id, resolve)
        send({ type: 'locate', target, id })
        window.setTimeout(() => {
          if (waiting.current.delete(id)) resolve(null)
        }, 1000)
      }),
    [send],
  )
  return { frame, ready, route, send, locate }
}
