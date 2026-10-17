import { type RefObject, useCallback, useEffect, useRef, useState } from 'react'
import { DEMO_CHANNEL, type DemoCommand, type DemoReport } from '../../cabin/demo'

/** A box on the screen, in its own pixels (1280x720). */
export interface Rect {
  x: number
  y: number
  width: number
  height: number
}

/** The live WestPlay in the screen (cabin.html in demo mode, cabin/demo.ts): commands to it, its icons'
 * places from it. */
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
