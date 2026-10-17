import { describe, expect, it } from 'vitest'
import { DOUBLE_TAP_MS, initialKeyboard, type KeyboardState, press, withoutLast, withoutLastWord } from './editing'
import { type Key, keyRows } from './layouts'

const char = (c: string): Key => ({ kind: 'char', char: c })

/** Presses each key in turn, a second apart unless given times. */
function typed(keys: Key[], state: KeyboardState = initialKeyboard('en'), times?: number[]) {
  let text = ''
  keys.forEach((key, i) => ({ state, text } = press(state, text, key, times?.[i] ?? i * 1000)))
  return { state, text }
}

describe('keyRows', () => {
  it('lays out English QWERTY and Turkish Q as on the iPhone', () => {
    const letters = (rows: Key[][]) =>
      rows.slice(0, 3).map((row) => row.map((key) => (key.kind === 'char' ? key.char : key.kind)).join(' '))
    expect(letters(keyRows('en', 'letters'))).toEqual([
      'q w e r t y u i o p',
      'a s d f g h j k l',
      'shift z x c v b n m delete',
    ])
    expect(letters(keyRows('tr', 'letters'))).toEqual([
      'q w e r t y u ı o p ğ ü',
      'a s d f g h j k l ş i',
      'shift z x c v b n m ö ç delete',
    ])
  })

  it('switches between layers from the bottom row and the third', () => {
    expect(keyRows('en', 'letters')[3].map((key) => key.kind)).toEqual(['layer', 'globe', 'space', 'hide', 'return'])
    expect(keyRows('en', 'letters')[3][0]).toEqual({ kind: 'layer', to: 'numbers' })
    expect(keyRows('en', 'numbers')[3][0]).toEqual({ kind: 'layer', to: 'letters' })
    expect(keyRows('en', 'numbers')[2][0]).toEqual({ kind: 'layer', to: 'symbols' })
    expect(keyRows('en', 'symbols')[2][0]).toEqual({ kind: 'layer', to: 'numbers' })
  })
})

describe('press', () => {
  it('types letters, spaces and deletes', () => {
    expect(typed([char('t'), char('a'), { kind: 'space' }, char('x'), { kind: 'delete' }]).text).toBe('ta ')
  })

  it('makes one capital with shift, and keeps capitals while it is locked', () => {
    expect(typed([{ kind: 'shift' }, char('t'), char('a')]).text).toBe('Ta')
    const tap = { kind: 'shift' } as const
    const locked = typed([tap, tap, char('a'), char('b')], undefined, [0, DOUBLE_TAP_MS - 50, 1000, 2000])
    expect(locked).toMatchObject({ text: 'AB', state: { shift: 'locked' } })
    // Two taps too far apart: the second turns shift off again.
    expect(typed([tap, tap, char('a')], undefined, [0, DOUBLE_TAP_MS + 50, 1000]).text).toBe('a')
    // A tap on a locked shift unlocks it.
    expect(typed([tap, tap, tap, char('a')], undefined, [0, 100, 1000, 2000]).text).toBe('a')
  })

  it('writes Turkish capitals by Turkish rules', () => {
    const turkish = initialKeyboard('tr')
    const shift = { kind: 'shift' } as const
    expect(typed([shift, char('i'), shift, char('ı')], turkish).text).toBe('İI')
    // The globe brings the other language's letters.
    expect(typed([{ kind: 'globe' }, shift, char('i')], turkish)).toMatchObject({
      text: 'I',
      state: { language: 'en' },
    })
  })

  it('brings the letters back after a space typed on the numbers', () => {
    const result = typed([{ kind: 'layer', to: 'numbers' }, char('4'), char('2'), { kind: 'space' }])
    expect(result).toMatchObject({ text: '42 ', state: { layer: 'letters' } })
  })

  it('says when return submits and when the keyboard is put away', () => {
    expect(press(initialKeyboard('en'), 'x', { kind: 'return' }, 0)).toMatchObject({ submit: true, text: 'x' })
    expect(press(initialKeyboard('en'), 'x', { kind: 'hide' }, 0)).toMatchObject({ hide: true, text: 'x' })
  })
})

describe('withoutLast', () => {
  it('deletes what reads as one character', () => {
    expect(withoutLast('Köln')).toBe('Köl')
    // A decomposed accent (e + combining acute) and a flag go whole.
    expect(withoutLast('Café')).toBe('Caf')
    expect(withoutLast('Go 🇹🇷')).toBe('Go ')
    expect(withoutLast('')).toBe('')
  })
})

describe('withoutLastWord', () => {
  it('deletes the last word and the spaces after it', () => {
    expect(withoutLastWord('Kokkola service')).toBe('Kokkola ')
    expect(withoutLastWord('Kokkola service  ')).toBe('Kokkola ')
    expect(withoutLastWord('Kokkola')).toBe('')
    expect(withoutLastWord('')).toBe('')
  })
})
