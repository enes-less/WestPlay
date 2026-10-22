import { useEffect, useRef } from 'react'
import { bindable, buttonOf, isModifier, keyOf, useKeyName } from '../keys'
import type { Words } from '../onboarding/words'
import type { Binding, KeyCode } from '../useSettings'

/**
 * A keybind's key, as a button: clicked, it listens, and the next key pressed (or the mouse's back or forward
 * button) becomes it; pressed while holding the screen's key (`with`), it's bound with it. Esc, or clicking
 * elsewhere, leaves it as it was. Shift, Ctrl and Alt are taken as they're let go, alone (AltGr presses Ctrl
 * and Right Alt together: Right Alt).
 */
export function KeyButton({
  binding,
  with: interactionKey,
  label,
  listening,
  listen,
  choose,
  words,
  pressKey,
}: {
  binding: Binding
  /** The screen's key, for a binding that can be pressed with it; `undefined` for the screen's key itself. */
  with?: KeyCode
  label: string
  listening: boolean
  /** Starts or stops listening (one keybind at a time). */
  listen(on: boolean): void
  choose(binding: Binding): void
  words: Words
  /** "Press a key…" */
  pressKey: string
}) {
  const button = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    if (!listening) return
    let held = false
    // The Shift, Ctrl or Alt last pressed, taken as it's let go if no other key came with it.
    let modifier: KeyCode | null = null
    const take = (key: KeyCode, event?: KeyboardEvent) => {
      if (key === interactionKey) return
      if (!bindable(key)) return
      const withKey = held || (event !== undefined && interactionKey !== undefined && flagged(event, interactionKey))
      choose({ key, withInteractionKey: interactionKey !== undefined && withKey })
      listen(false)
    }
    const keydown = (event: KeyboardEvent) => {
      event.preventDefault()
      event.stopPropagation()
      if (event.repeat) return
      if (event.key === 'Escape') {
        listen(false)
        return
      }
      const key = keyOf(event)
      if (key === null) return
      if (key === interactionKey) {
        held = true
        return
      }
      if (isModifier(key)) {
        modifier = key
        return
      }
      take(key, event)
    }
    const keyup = (event: KeyboardEvent) => {
      event.preventDefault()
      const key = keyOf(event)
      if (key === interactionKey) held = false
      else if (key !== null && key === modifier) take(key)
    }
    const mousedown = (event: MouseEvent) => {
      const key = buttonOf(event.button)
      if (key === null) return
      if (key === interactionKey) {
        event.preventDefault()
        held = true
        return
      }
      // The back and forward buttons are bound; a click elsewhere stops listening (on this button, its click
      // does).
      if (key === 0x05 || key === 0x06) {
        event.preventDefault()
        take(key)
      } else if (!button.current?.contains(event.target as Node)) {
        listen(false)
      }
    }
    const mouseup = (event: MouseEvent) => {
      const key = buttonOf(event.button)
      if (key === interactionKey) held = false
      // The side buttons go back and forward in a browser: not here.
      if (key === 0x05 || key === 0x06) event.preventDefault()
    }
    const stop = () => listen(false)
    const handlers: [string, EventListener, boolean][] = [
      ['keydown', keydown as EventListener, true],
      ['keyup', keyup as EventListener, true],
      ['mousedown', mousedown as EventListener, true],
      ['mouseup', mouseup as EventListener, true],
      ['auxclick', ((event: MouseEvent) => event.preventDefault()) as EventListener, true],
      ['blur', stop, false],
    ]
    handlers.forEach(([name, handler, capture]) => window.addEventListener(name, handler, capture))
    return () => handlers.forEach(([name, handler, capture]) => window.removeEventListener(name, handler, capture))
  }, [listening, interactionKey, choose, listen])

  return (
    <button
      ref={button}
      type="button"
      className="ob-button cp-keybind"
      data-glass
      data-small
      data-listening={listening || undefined}
      aria-label={label}
      onClick={() => listen(!listening)}
    >
      {listening ? pressKey : <BindingName binding={binding} with={interactionKey} words={words} />}
    </button>
  )
}

/** Whether `event` says the screen's key `key` is down, if it's Shift, Ctrl or Alt (either side): for one
 * pressed before listening began. */
function flagged(event: KeyboardEvent, key: KeyCode): boolean {
  if (key === 0xa0 || key === 0xa1) return event.shiftKey
  if (key === 0xa2 || key === 0xa3) return event.ctrlKey
  // AltGr is Ctrl and Alt together: Alt alone.
  if (key === 0xa4) return event.altKey && !event.ctrlKey
  if (key === 0xa5) return event.altKey
  return false
}

/** A binding's keys: "Left Alt + 1", or just "F9". */
export function BindingName({
  binding,
  with: interactionKey,
  words,
}: {
  binding: Binding
  with?: KeyCode
  words: Words
}) {
  const key = useKeyName(binding.key, words)
  const screenKey = useKeyName(interactionKey ?? binding.key, words)
  return <>{interactionKey !== undefined && binding.withInteractionKey ? `${screenKey} + ${key}` : key}</>
}
