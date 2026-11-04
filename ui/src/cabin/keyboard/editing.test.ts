import { describe, expect, it } from 'vitest'
import { DOUBLE_TAP_MS, initialKeyboard, type KeyboardState, press, withoutLast, withoutLastWord } from './editing'
import { hangulDeleted, hangulTyped } from './hangul'
import { kanaTyped } from './kana'
import { capital, type Key, keyboardLanguage, keyRows } from './layouts'

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

  it("lays out each language's own letters, and gives English to the languages that type it", () => {
    const top = (language: Parameters<typeof keyRows>[0]) =>
      keyRows(language, 'letters')[0].map((key) => (key.kind === 'char' ? key.char : '')).join('')
    expect(top(keyboardLanguage('de_de'))).toBe('qwertzuiopü')
    expect(top(keyboardLanguage('fr_fr'))).toBe('azertyuiop')
    expect(top(keyboardLanguage('ru_ru'))).toBe('йцукенгшщзхъ')
    expect(top(keyboardLanguage('sr_sp'))).toBe('qwertzuiopšđ')
    expect(top(keyboardLanguage('fi_fi'))).toBe('qwertyuiopå')
    // Canada's French, Italian, Chinese: QWERTY.
    for (const language of ['fr_ca', 'it_it', 'zh_cn', 'en_us']) expect(keyboardLanguage(language)).toBe('en')
    // No row wider than the top one: the keys take its width.
    for (const language of ['tr', 'de', 'fr', 'hu', 'et', 'hr', 'ru', 'uk', 'bg', 'mk', 'sr', 'ka', 'ko'] as const) {
      const rows = keyRows(language, 'letters')
      for (const row of rows.slice(1, 3)) expect(row.length, language).toBeLessThanOrEqual(rows[0].length)
    }
  })

  it('switches between layers from the bottom row and the third', () => {
    // The globe only where there's another keyboard: English alone has none.
    expect(keyRows('en', 'letters')[3].map((key) => key.kind)).toEqual(['layer', 'space', 'hide', 'return'])
    expect(keyRows('en', 'letters', true)[3].map((key) => key.kind)).toEqual(['layer', 'globe', 'space', 'hide', 'return'])
    expect(keyRows('de', 'letters')[3][1]).toEqual({ kind: 'globe' })
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

  it("shifts to Georgian's and Korean's other letters, which have no capitals", () => {
    expect(capital('წ', 'ka')).toBe('ჭ')
    expect(capital('ა', 'ka')).toBe('ა')
    expect(capital('ㄱ', 'ko')).toBe('ㄲ')
    expect(capital('ö', 'de')).toBe('Ö')
    // The globe goes between the language's own keyboard and English, both ways.
    const german = initialKeyboard('de')
    const there = typed([{ kind: 'globe' }], german).state
    expect(there).toMatchObject({ language: 'en', own: 'de' })
    expect(typed([{ kind: 'globe' }], there).state.language).toBe('de')
  })

  it('joins Korean letters into syllables, and takes them apart a letter at a time', () => {
    const typeAll = (letters: string) => Array.from(letters).reduce(hangulTyped, '')
    expect(typeAll('ㅎㅏㄴㄱㅡㄹ')).toBe('한글')
    expect(typeAll('ㅍㅏㄹㅣ')).toBe('파리')
    expect(typeAll('ㄱㅏㅂㅏ')).toBe('가바')
    expect(typeAll('ㄷㅏㄹㄱ')).toBe('닭')
    expect(typeAll('ㄷㅏㄹㄱㅏ')).toBe('달가')
    expect(typeAll('ㄱㅗㅏ')).toBe('과')
    expect(typeAll('ㅂㅔㄹㅡㄹㄹㅣㄴ')).toBe('베를린')
    expect(hangulDeleted('닭')).toBe('달')
    expect(hangulDeleted('달')).toBe('다')
    expect(hangulDeleted('다')).toBe('ㄷ')
    expect(hangulDeleted('과')).toBe('고')
    expect(hangulDeleted('')).toBeNull()
    // Typed on the Korean keyboard, shift for the doubled consonant (closing the syllable: 하 + ㄲ is 핚).
    const korean = initialKeyboard('ko')
    expect(typed([char('ㅎ'), char('ㅏ'), char('ㄴ'), { kind: 'delete' }, { kind: 'shift' }, char('ㄱ')], korean).text).toBe('핚')
  })

  it('turns romaji into hiragana as it is typed', () => {
    const typeAll = (romaji: string) => Array.from(romaji).reduce(kanaTyped, '')
    expect(typeAll('pari')).toBe('ぱり')
    expect(typeAll('tokyo')).toBe('ときょ')
    expect(typeAll('rondon')).toBe('ろんどn')
    expect(typeAll('rondonn')).toBe('ろんどん')
    expect(typeAll('kitte')).toBe('きって')
    expect(typeAll('shinkansen')).toBe('しんかんせn')
    expect(typeAll('ra-men')).toBe('らーめn')
    expect(typeAll('k')).toBe('k')
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
