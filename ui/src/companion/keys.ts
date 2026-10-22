import { useEffect, useState } from 'react'
import { call } from './bridge'
import type { Words } from './onboarding/words'
import { KEY_PRESETS, type KeyCode, type KeyPreset } from './useSettings'

/**
 * Keys as Windows numbers them (virtual-key codes, which the plugin checks in the game), from the companion's
 * own key and mouse events, and their names for the player.
 */

/** A side's own code for Shift, Ctrl, Alt and the Windows keys (the event's `keyCode` is the same for both). */
const SIDED: Record<string, KeyCode> = {
  ShiftLeft: 0xa0,
  ShiftRight: 0xa1,
  ControlLeft: 0xa2,
  ControlRight: 0xa3,
  AltLeft: 0xa4,
  AltRight: 0xa5,
  MetaLeft: 0x5b,
  MetaRight: 0x5c,
}

/** The key pressed: its virtual-key code (in Windows' browser, a key event's `keyCode` is one); `null`: none. */
export function keyOf(event: KeyboardEvent): KeyCode | null {
  const sided = SIDED[event.code]
  if (sided !== undefined) return sided
  return event.keyCode > 0 && event.keyCode < 0xff ? event.keyCode : null
}

/** A mouse button's code (`event.button`: 0 left, 1 middle, 2 right, 3 back, 4 forward). */
export function buttonOf(button: number): KeyCode | null {
  return [0x01, 0x04, 0x02, 0x05, 0x06][button] ?? null
}

/** Shift, Ctrl and Alt: pressed with another key (AltGr is Ctrl and Right Alt), so taken when let go alone. */
export function isModifier(key: KeyCode): boolean {
  return key >= 0xa0 && key <= 0xa5
}

/** Keys that can't be bound: the mouse's left, right and middle buttons (the screen's), the Windows keys
 * (Windows' own), and Esc (it cancels choosing). */
export function bindable(key: KeyCode): boolean {
  return ![0x01, 0x02, 0x04, 0x5b, 0x5c, 0x1b].includes(key)
}

export function presetOf(key: KeyCode): KeyPreset | null {
  const found = Object.entries(KEY_PRESETS).find(([, code]) => code === key)
  return found ? (found[0] as KeyPreset) : null
}

/** Windows' names for keys, asked once each. */
const names = new Map<KeyCode, Promise<string | null>>()

function nameOf(key: KeyCode): Promise<string | null> {
  let name = names.get(key)
  if (!name) {
    name = call<string | null>('key_name', { key }).catch(() => null)
    names.set(key, name)
  }
  return name
}

/** A key's name for the player: the onboarding's for its keys (in their language), else Windows' for the
 * player's keyboard ("Num 4", "F9", "Q"). */
export function useKeyName(key: KeyCode, words: Words): string {
  const preset = presetOf(key)
  const [asked, setAsked] = useState<{ key: KeyCode; name: string | null } | null>(null)
  useEffect(() => {
    if (preset) return
    let active = true
    nameOf(key).then((name) => active && setAsked({ key, name }))
    return () => {
      active = false
    }
  }, [key, preset])
  if (preset) return words.controls.keys[preset]
  if (asked?.key !== key) return '…'
  // A key Windows has no name for: its number, as Windows has it.
  return asked.name ?? `0x${key.toString(16).toUpperCase().padStart(2, '0')}`
}
