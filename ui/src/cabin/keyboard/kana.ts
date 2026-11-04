/**
 * Japanese typed in romaji, turned into hiragana as it goes, as iOS's Romaji keyboard does (without its kanji:
 * those need a dictionary). The romaji not yet a kana stays as it's typed (`k`, then `ka`: か); a doubled
 * consonant is a small tsu (`kk`: っk), `n` before a consonant or `nn` is ん, `-` the long-vowel mark. Map
 * search finds katakana names by hiragana (maps/places.ts folds them alike), so `pari` finds パリ.
 */

const VOWELS = 'aiueo'
/** The kana of each romaji, rows by their consonant. */
const ROWS: Record<string, string> = {
  '': 'あいうえお',
  k: 'かきくけこ',
  g: 'がぎぐげご',
  s: 'さしすせそ',
  z: 'ざじずぜぞ',
  t: 'たちつてと',
  d: 'だぢづでど',
  n: 'なにぬねの',
  h: 'はひふへほ',
  b: 'ばびぶべぼ',
  p: 'ぱぴぷぺぽ',
  m: 'まみむめも',
  r: 'らりるれろ',
  x: 'ぁぃぅぇぉ',
  l: 'ぁぃぅぇぉ',
}
const TABLE: Record<string, string> = {
  ya: 'や',
  yu: 'ゆ',
  yo: 'よ',
  wa: 'わ',
  wo: 'を',
  shi: 'し',
  chi: 'ち',
  tsu: 'つ',
  fu: 'ふ',
  ji: 'じ',
  xtsu: 'っ',
  ltu: 'っ',
  xtu: 'っ',
  xya: 'ゃ',
  xyu: 'ゅ',
  xyo: 'ょ',
  nn: 'ん',
  "n'": 'ん',
  '-': 'ー',
  fa: 'ふぁ',
  fi: 'ふぃ',
  fe: 'ふぇ',
  fo: 'ふぉ',
  va: 'ゔぁ',
  vi: 'ゔぃ',
  vu: 'ゔ',
  ve: 'ゔぇ',
  vo: 'ゔぉ',
  ti: 'てぃ',
  di: 'でぃ',
  je: 'じぇ',
  she: 'しぇ',
  che: 'ちぇ',
}
for (const [consonant, kana] of Object.entries(ROWS)) {
  for (let i = 0; i < 5; i++) TABLE[consonant + VOWELS[i]] ??= kana[i]
}
// The contracted sounds: きゃ, しゃ, ちゃ, じゃ…
const SMALL = { a: 'ゃ', u: 'ゅ', o: 'ょ' } as const
for (const [romaji, kana] of Object.entries({
  ky: 'き',
  gy: 'ぎ',
  sy: 'し',
  sh: 'し',
  zy: 'じ',
  j: 'じ',
  jy: 'じ',
  ty: 'ち',
  ch: 'ち',
  cy: 'ち',
  dy: 'ぢ',
  ny: 'に',
  hy: 'ひ',
  by: 'び',
  py: 'ぴ',
  my: 'み',
  ry: 'り',
})) {
  for (const [vowel, small] of Object.entries(SMALL)) TABLE[romaji + vowel] ??= kana + small
}
const LONGEST = Math.max(...Object.keys(TABLE).map((romaji) => romaji.length))

/** Romaji turned to kana as far as it goes; what's left (a consonant waiting for its vowel) stays. */
function kana(romaji: string): string {
  let out = ''
  let rest = romaji
  while (rest) {
    // A doubled consonant (not n): a small tsu, the second starting the next kana.
    if (rest.length > 1 && rest[0] === rest[1] && /[bcdfghjklmpqrstvwxyz]/.test(rest[0])) {
      out += 'っ'
      rest = rest.slice(1)
      continue
    }
    // n before a consonant (not y, nor another n waiting to be nn): ん.
    if (rest.length > 1 && rest[0] === 'n' && /[bcdfghjklmpqrstvwxz]/.test(rest[1])) {
      out += 'ん'
      rest = rest.slice(1)
      continue
    }
    let matched = false
    for (let length = Math.min(LONGEST, rest.length); length > 0; length--) {
      const kana = TABLE[rest.slice(0, length)]
      if (kana) {
        out += kana
        rest = rest.slice(length)
        matched = true
        break
      }
    }
    if (!matched) {
      // Waiting for more (a consonant, `sh`, `ts`), or a letter no kana starts with: left as typed.
      return out + rest
    }
  }
  return out
}

/** The text after typing `char`: the romaji at its end (lower-case letters, `'` and `-`) turned to kana. */
export function kanaTyped(text: string, char: string): string {
  const typed = text + char
  const romaji = /[a-z'-]+$/.exec(typed)?.[0] ?? ''
  return romaji ? typed.slice(0, typed.length - romaji.length) + kana(romaji) : typed
}
