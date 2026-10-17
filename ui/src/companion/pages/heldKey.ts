import { useEffect, useState } from 'react'
import type { InteractionKey } from '../useSettings'

/** The player's key, held or not, while `active`: a key of the keyboard or a side button of the mouse. */
export function useHeldKey(key: InteractionKey, active: boolean): boolean {
  const [held, setHeld] = useState(false)
  useEffect(() => {
    if (!active) return
    const code = key === 'leftAlt' ? 'AltLeft' : key === 'rightCtrl' ? 'ControlRight' : null
    const button = key === 'mouseBack' ? 3 : key === 'mouseForward' ? 4 : null
    const onKey = (pressed: boolean) => (event: KeyboardEvent) => {
      if (event.code !== code) return
      event.preventDefault()
      setHeld(pressed)
    }
    // The mouse's side buttons go back and forward in a browser: not here.
    const onButton = (pressed: boolean | null) => (event: MouseEvent) => {
      if (event.button !== button) return
      event.preventDefault()
      if (pressed !== null) setHeld(pressed)
    }
    const handlers: [string, EventListener][] = [
      ['keydown', onKey(true) as EventListener],
      ['keyup', onKey(false) as EventListener],
      ['mousedown', onButton(true) as EventListener],
      ['mouseup', onButton(false) as EventListener],
      ['auxclick', onButton(null) as EventListener],
      ['blur', () => setHeld(false)],
    ]
    handlers.forEach(([name, handler]) => window.addEventListener(name, handler))
    return () => {
      handlers.forEach(([name, handler]) => window.removeEventListener(name, handler))
      setHeld(false)
    }
  }, [key, active])
  return held
}
