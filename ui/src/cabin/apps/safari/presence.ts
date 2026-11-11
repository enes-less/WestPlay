import { useEffect, useRef, useState } from 'react'

/**
 * Something that animates in and out (a menu, a sheet, Safari's tab overview): `'in'` while it's open, `'out'` for
 * `ms` after it closes (its closing animation, still on screen), then `null` (gone).
 */
export function usePresence(open: boolean, ms: number): 'in' | 'out' | null {
  const [leaving, setLeaving] = useState(false)
  const wasOpen = useRef(open)
  useEffect(() => {
    if (open) {
      wasOpen.current = true
      setLeaving(false)
      return
    }
    if (!wasOpen.current) return
    wasOpen.current = false
    setLeaving(true)
    const timer = setTimeout(() => setLeaving(false), ms)
    return () => clearTimeout(timer)
  }, [open, ms])
  return open ? 'in' : leaving ? 'out' : null
}

/** The last value something showed while it had one, kept for its closing animation (a dialog's text as it goes). */
export function useLast<T>(value: T | null): T | null {
  const last = useRef(value)
  if (value !== null) last.current = value
  return value ?? last.current
}
