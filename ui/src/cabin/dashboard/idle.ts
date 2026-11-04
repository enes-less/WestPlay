import { useEffect, useState } from 'react'
import { useLive } from '../state/store'
import { GROWS_FROM_KMH, mapGrows, UNTOUCHED_MS } from './mapGrows'

/** What counts as using the screen: the pointer moved (the interaction key held), pressed, the wheel, a key. */
const USES = ['pointermove', 'pointerdown', 'wheel', 'keydown'] as const

/** When the screen was last used (the page's clock, ms): the page coming up counts as a use. */
let lastUse = performance.now()
const usedListeners = new Set<() => void>()
if (typeof window !== 'undefined') {
  for (const use of USES) {
    window.addEventListener(
      use,
      () => {
        lastUse = performance.now()
        for (const listener of usedListeners) listener()
      },
      { passive: true, capture: true },
    )
  }
}

/**
 * Whether the Dashboard's map has grown to fill it (Dashboard.tsx), while `on`: the truck driving at 30 km/h or
 * more and the screen untouched 30 s (owner 2026-11-03). Any use of the screen, or the truck slowing below it,
 * brings the widgets back.
 */
export function useMapGrown(on: boolean): boolean {
  const fast = useLive((state) => Math.abs(state.telemetry?.speed ?? 0) * 3.6 >= GROWS_FROM_KMH)
  const [grown, setGrown] = useState(false)
  useEffect(() => {
    if (!on || !fast) {
      setGrown(false)
      return
    }
    let timer: ReturnType<typeof setTimeout> | undefined
    const check = () => {
      clearTimeout(timer)
      const untouched = performance.now() - lastUse
      setGrown(mapGrows(true, GROWS_FROM_KMH, untouched))
      if (untouched < UNTOUCHED_MS) timer = setTimeout(check, UNTOUCHED_MS - untouched)
    }
    check()
    usedListeners.add(check)
    return () => {
      clearTimeout(timer)
      usedListeners.delete(check)
    }
  }, [on, fast])
  return on && fast && grown
}
