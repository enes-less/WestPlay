import type { Language } from '../state/strings'

/**
 * The keyboard's keys, as on the iPhone: English QWERTY and Turkish Q, each with the same number and symbol
 * layers. Letters are written lowercase; shift makes them capitals by their language's rules (Turkish i and
 * ı are İ and I).
 */

/** Which set of keys shows. */
export type Layer = 'letters' | 'numbers' | 'symbols'

/** What the return key says it does: search for what was typed, go there, or simply finish. */
export type ReturnKind = 'search' | 'go' | 'done'

export type Key =
  | { kind: 'char'; char: string }
  | { kind: 'shift' }
  | { kind: 'delete' }
  /** 123, #+= or ABC: another layer. */
  | { kind: 'layer'; to: Layer }
  /** The other language's letters. */
  | { kind: 'globe' }
  | { kind: 'space' }
  | { kind: 'return' }
  /** Puts the keyboard away. */
  | { kind: 'hide' }

const LETTERS: Record<Language, readonly string[]> = {
  en: ['qwertyuiop', 'asdfghjkl', 'zxcvbnm'],
  tr: ['qwertyuıopğü', 'asdfghjklşi', 'zxcvbnmöç'],
}

const NUMBERS = ['1234567890', '-/:;()€&@"', ".,?!'"]
const SYMBOLS = ['[]{}#%^*+=', '_\\|~<>$£¥•', ".,?!'"]

const chars = (row: string): Key[] => Array.from(row, (char) => ({ kind: 'char', char }))

/** The keys of a layer, row by row: three rows of characters, then the bottom row. */
export function keyRows(language: Language, layer: Layer): Key[][] {
  const rows = layer === 'letters' ? LETTERS[language] : layer === 'numbers' ? NUMBERS : SYMBOLS
  const [top, middle, bottom] = rows.map(chars)
  // Left of the third row: shift on letters, the other symbols layer on the others.
  const left: Key =
    layer === 'letters' ? { kind: 'shift' } : { kind: 'layer', to: layer === 'numbers' ? 'symbols' : 'numbers' }
  const switcher: Key = layer === 'letters' ? { kind: 'layer', to: 'numbers' } : { kind: 'layer', to: 'letters' }
  return [
    top,
    middle,
    [left, ...bottom, { kind: 'delete' }],
    [switcher, { kind: 'globe' }, { kind: 'space' }, { kind: 'hide' }, { kind: 'return' }],
  ]
}

/** A letter in capitals, by its language's rules. */
export function capital(char: string, language: Language): string {
  return char.toLocaleUpperCase(language === 'tr' ? 'tr-TR' : 'en-GB')
}

/** The other language, for the globe key. */
export function otherLanguage(language: Language): Language {
  return language === 'tr' ? 'en' : 'tr'
}
