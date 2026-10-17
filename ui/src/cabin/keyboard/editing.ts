import type { Language } from '../state/strings'
import { capital, type Key, type Layer, otherLanguage } from './layouts'

/** Shift: off, for the next letter only, or locked on (caps lock, by tapping it twice). */
export type Shift = 'off' | 'once' | 'locked'

export interface KeyboardState {
  layer: Layer
  shift: Shift
  language: Language
  /** When shift was last tapped (ms), to tell a double tap. */
  shiftTappedAt: number | null
}

/** How soon after the first a second tap on shift locks it, ms (iOS's feel). */
export const DOUBLE_TAP_MS = 350

export function initialKeyboard(language: Language): KeyboardState {
  return { layer: 'letters', shift: 'off', language, shiftTappedAt: null }
}

/** What a key press does: the keyboard and text after it, and whether it submits or puts the keyboard away. */
export interface Outcome {
  state: KeyboardState
  text: string
  submit?: boolean
  hide?: boolean
}

/** Presses `key` (at `at`, ms) with `text` typed so far. */
export function press(state: KeyboardState, text: string, key: Key, at: number): Outcome {
  switch (key.kind) {
    case 'char': {
      const letter = state.layer === 'letters' && state.shift !== 'off'
      return {
        state: state.shift === 'once' ? { ...state, shift: 'off' } : state,
        text: text + (letter ? capital(key.char, state.language) : key.char),
      }
    }
    case 'space':
      // As on the iPhone, a space after numbers or symbols brings the letters back.
      return { state: { ...state, layer: 'letters' }, text: `${text} ` }
    case 'delete':
      return { state, text: withoutLast(text) }
    case 'shift': {
      const doubleTap =
        state.shift === 'once' && state.shiftTappedAt !== null && at - state.shiftTappedAt <= DOUBLE_TAP_MS
      const shift: Shift = doubleTap ? 'locked' : state.shift === 'off' ? 'once' : 'off'
      return { state: { ...state, shift, shiftTappedAt: at }, text }
    }
    case 'layer':
      return { state: { ...state, layer: key.to, shift: 'off' }, text }
    case 'globe':
      return { state: { ...state, language: otherLanguage(state.language), layer: 'letters', shift: 'off' }, text }
    case 'return':
      return { state, text, submit: true }
    case 'hide':
      return { state, text, hide: true }
  }
}

const graphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' })

/** The text without its last character as the reader sees it (a flag or an accented letter is one). */
export function withoutLast(text: string): string {
  let last = 0
  for (const { index } of graphemes.segment(text)) last = index
  return text.slice(0, last)
}

/** The text without its last word and the spaces after it (Ctrl+Backspace on the player's own keyboard). */
export function withoutLastWord(text: string): string {
  return text.replace(/\S*\s*$/u, '')
}
