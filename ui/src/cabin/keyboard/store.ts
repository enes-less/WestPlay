import { create } from 'zustand'
import type { TypedEvent, TypingState } from '../../shared/host'
import type { Language } from '../state/strings'
import { initialKeyboard, type KeyboardState, press, withoutLastWord } from './editing'
import type { Key, ReturnKind } from './layouts'

/** The text field being typed into: how to read and change its text, and what its return key does. */
export interface Typing {
  id: string
  returnKind: ReturnKind
  read(): string
  write(text: string): void
  submit(): void
}

interface KeyboardStore {
  /** The field the keyboard types into, `null` when the keyboard is away. */
  field: Typing | null
  /** That field's selection: a number that changes with every selection (0: none), so the player's own
   * keyboard types only into a field selected while the truck stood still (the game plugin decides). */
  selection: number
  keys: KeyboardState
  /** The language the player switched the keyboard to (the globe key), for the rest of the session. */
  chosen: Language | null
  /** Whether the player's own keyboard types into the field (the host says). */
  physical: TypingState
  /** The last character typed on the player's own keyboard, for its key to light up (a new `id` each). */
  flash: { char: string; id: number } | null
  /** Starts typing into `field` (selecting it): the keyboard comes up, in `language` (the game's) unless
   * the player switched it. */
  attach(field: Typing, language: Language): void
  /** Stops typing into the field `id`, if the keyboard types into it: the keyboard goes away. */
  detach(id: string): void
  /** A key of the keyboard, pressed at `at` (ms). */
  press(key: Key, at?: number): void
  /** What the player typed on their own keyboard. */
  typed(event: TypedEvent): void
}

let selections = 0

export const useKeyboard = create<KeyboardStore>()((set, get) => ({
  field: null,
  selection: 0,
  keys: initialKeyboard('en'),
  chosen: null,
  physical: 'off',
  flash: null,
  attach: (field, language) => {
    const { field: current, chosen } = get()
    // A newly selected field starts on the letters, in the player's language; one selected again keeps
    // the keyboard as it was. Either way it's a new selection, which the player's own keyboard may type into.
    const selection = ++selections
    if (current?.id === field.id) set({ field, selection })
    else set({ field, selection, keys: initialKeyboard(chosen ?? language) })
  },
  detach: (id) => {
    if (get().field?.id === id) set({ field: null, selection: 0 })
  },
  press: (key, at = performance.now()) => {
    const { field, keys } = get()
    if (!field) return
    const before = field.read()
    const outcome = press(keys, before, key, at)
    const away = outcome.hide || outcome.submit
    set({
      keys: outcome.state,
      chosen: key.kind === 'globe' ? outcome.state.language : get().chosen,
      field: away ? null : field,
      selection: away ? 0 : get().selection,
    })
    if (outcome.text !== before) field.write(outcome.text)
    if (outcome.submit) field.submit()
  },
  typed: (event) => {
    const { field, flash } = get()
    if (!field) return
    if ('text' in event) {
      field.write(field.read() + event.text)
      set({ flash: { char: event.text.slice(-1), id: (flash?.id ?? 0) + 1 } })
      return
    }
    switch (event.key) {
      case 'backspace':
        get().press({ kind: 'delete' })
        break
      case 'wordBackspace':
        field.write(withoutLastWord(field.read()))
        break
      case 'enter':
        get().press({ kind: 'return' })
        break
      case 'escape':
        get().press({ kind: 'hide' })
        break
    }
  },
}))
