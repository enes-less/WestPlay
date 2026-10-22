import { useEffect, useState } from 'react'
import { buttonOf, keyOf } from '../keys'
import type { InteractionMode, KeyCode } from '../useSettings'

/** The player's key, held or not, while `active`: a key of the keyboard or a button of the mouse. In toggle
 * mode, each press turns it on or off, as in the game. */
export function useHeldKey(key: KeyCode, mode: InteractionMode, active: boolean): boolean {
  const [held, setHeld] = useState(false)
  useEffect(() => {
    if (!active) return
    const press = (pressed: boolean) => {
      if (mode === 'hold') setHeld(pressed)
      else if (pressed) setHeld((on) => !on)
    }
    const onKey = (pressed: boolean) => (event: KeyboardEvent) => {
      if (keyOf(event) !== key) return
      event.preventDefault()
      // Held down, a key repeats: one press.
      if (!event.repeat) press(pressed)
    }
    // The mouse's side buttons go back and forward in a browser: not here.
    const onButton = (pressed: boolean | null) => (event: MouseEvent) => {
      if (buttonOf(event.button) !== key) return
      event.preventDefault()
      if (pressed !== null) press(pressed)
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
  }, [key, mode, active])
  return held
}
