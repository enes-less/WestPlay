/**
 * Korean typed letter by letter (the two-set keyboard's jamo), joined into syllables as it goes, as iOS's
 * Korean keyboard does: a consonant then a vowel make a syllable, a consonant after it closes it (or joins its
 * closing consonant: ㄹ+ㄱ is ㄺ), and a vowel after a closed syllable takes its closing consonant to open the
 * next (갑 + ㅏ is 가바). Delete takes the last letter away, not the whole syllable.
 */

const INITIALS = 'ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ'
const MEDIALS = 'ㅏㅐㅑㅒㅓㅔㅕㅖㅗㅘㅙㅚㅛㅜㅝㅞㅟㅠㅡㅢㅣ'
/** The closing consonants, in Unicode's order (none first). */
const FINALS = ['', ...'ㄱㄲㄳㄴㄵㄶㄷㄹㄺㄻㄼㄽㄾㄿㅀㅁㅂㅄㅅㅆㅇㅈㅊㅋㅌㅍㅎ']

/** Two vowels that make one (ㅗ+ㅏ: ㅘ). */
const VOWEL_PAIRS: Record<string, string> = {
  ㅗㅏ: 'ㅘ',
  ㅗㅐ: 'ㅙ',
  ㅗㅣ: 'ㅚ',
  ㅜㅓ: 'ㅝ',
  ㅜㅔ: 'ㅞ',
  ㅜㅣ: 'ㅟ',
  ㅡㅣ: 'ㅢ',
}
/** Two closing consonants that make one (ㄹ+ㄱ: ㄺ). */
const FINAL_PAIRS: Record<string, string> = {
  ㄱㅅ: 'ㄳ',
  ㄴㅈ: 'ㄵ',
  ㄴㅎ: 'ㄶ',
  ㄹㄱ: 'ㄺ',
  ㄹㅁ: 'ㄻ',
  ㄹㅂ: 'ㄼ',
  ㄹㅅ: 'ㄽ',
  ㄹㅌ: 'ㄾ',
  ㄹㅍ: 'ㄿ',
  ㄹㅎ: 'ㅀ',
  ㅂㅅ: 'ㅄ',
}
const split = (pairs: Record<string, string>) =>
  Object.fromEntries(Object.entries(pairs).map(([two, one]) => [one, [...two] as [string, string]]))
const VOWEL_PARTS = split(VOWEL_PAIRS)
const FINAL_PARTS = split(FINAL_PAIRS)

const FIRST = 0xac00
const LAST = 0xd7a3

interface Syllable {
  initial: number
  medial: number
  final: number
}

function syllableOf(char: string): Syllable | null {
  const code = char.codePointAt(0) ?? 0
  if (code < FIRST || code > LAST) return null
  const at = code - FIRST
  return { initial: Math.floor(at / 588), medial: Math.floor((at % 588) / 28), final: at % 28 }
}

function syllable({ initial, medial, final }: Syllable): string {
  return String.fromCodePoint(FIRST + initial * 588 + medial * 28 + final)
}

/** The text after typing the letter `char` (a jamo; anything else is added as it is). */
export function hangulTyped(text: string, char: string): string {
  const before = text.slice(0, -1)
  const last = text.slice(-1)
  const open = syllableOf(last)
  const vowel = MEDIALS.indexOf(char)
  if (vowel < 0) {
    const final = FINALS.indexOf(char)
    if (open && final > 0) {
      // Closing an open syllable, or joining its closing consonant.
      if (open.final === 0) return before + syllable({ ...open, final })
      const pair = FINAL_PAIRS[FINALS[open.final] + char]
      if (pair) return before + syllable({ ...open, final: FINALS.indexOf(pair) })
    }
    return text + char
  }
  // A vowel: after a lone consonant, a syllable; after a vowel it pairs with, that vowel.
  const initial = INITIALS.indexOf(last)
  if (last && initial >= 0) return before + syllable({ initial, medial: vowel, final: 0 })
  if (open && open.final === 0) {
    const pair = VOWEL_PAIRS[MEDIALS[open.medial] + char]
    if (pair) return before + syllable({ ...open, medial: MEDIALS.indexOf(pair) })
  }
  if (open && open.final > 0) {
    // The closing consonant (or a pair's second) opens the next syllable.
    const parts = FINAL_PARTS[FINALS[open.final]]
    const kept = parts ? FINALS.indexOf(parts[0]) : 0
    const moved = parts ? parts[1] : FINALS[open.final]
    const next = INITIALS.indexOf(moved)
    if (next >= 0) return before + syllable({ ...open, final: kept }) + syllable({ initial: next, medial: vowel, final: 0 })
  }
  const pair = VOWEL_PAIRS[last + char]
  if (pair) return before + pair
  return text + char
}

/** The text after delete: a syllable loses its last letter (its closing consonant, a pair's second, its
 * vowel), anything else its last character; `null` for an empty text. */
export function hangulDeleted(text: string): string | null {
  if (!text) return null
  const before = text.slice(0, -1)
  const last = text.slice(-1)
  const open = syllableOf(last)
  if (!open) {
    const parts = VOWEL_PARTS[last]
    return parts ? before + parts[0] : before
  }
  if (open.final > 0) {
    const parts = FINAL_PARTS[FINALS[open.final]]
    return before + syllable({ ...open, final: parts ? FINALS.indexOf(parts[0]) : 0 })
  }
  const parts = VOWEL_PARTS[MEDIALS[open.medial]]
  if (parts) return before + syllable({ ...open, medial: MEDIALS.indexOf(parts[0]) })
  return before + INITIALS[open.initial]
}
