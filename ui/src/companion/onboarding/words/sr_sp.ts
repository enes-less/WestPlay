import type { Words } from '.'
import { words as cyrillic } from './sr_sr'

/** Serbian Cyrillic's letters in Latin, one for one (as the game's own `sr_sp` is its `sr_sr`). */
const LATIN: Record<string, string> = {
  А: 'A',
  Б: 'B',
  В: 'V',
  Г: 'G',
  Д: 'D',
  Ђ: 'Đ',
  Е: 'E',
  Ж: 'Ž',
  З: 'Z',
  И: 'I',
  Ј: 'J',
  К: 'K',
  Л: 'L',
  Љ: 'Lj',
  М: 'M',
  Н: 'N',
  Њ: 'Nj',
  О: 'O',
  П: 'P',
  Р: 'R',
  С: 'S',
  Т: 'T',
  Ћ: 'Ć',
  У: 'U',
  Ф: 'F',
  Х: 'H',
  Ц: 'C',
  Ч: 'Č',
  Џ: 'Dž',
  Ш: 'Š',
  а: 'a',
  б: 'b',
  в: 'v',
  г: 'g',
  д: 'd',
  ђ: 'đ',
  е: 'e',
  ж: 'ž',
  з: 'z',
  и: 'i',
  ј: 'j',
  к: 'k',
  л: 'l',
  љ: 'lj',
  м: 'm',
  н: 'n',
  њ: 'nj',
  о: 'o',
  п: 'p',
  р: 'r',
  с: 's',
  т: 't',
  ћ: 'ć',
  у: 'u',
  ф: 'f',
  х: 'h',
  ц: 'c',
  ч: 'č',
  џ: 'dž',
  ш: 'š',
}

/** {@link LATIN} through a value: its strings, and the strings in its arrays and objects. */
export function latin<T>(value: T): T {
  if (typeof value === 'string') return [...value].map((letter) => LATIN[letter] ?? letter).join('') as T
  if (Array.isArray(value)) return value.map(latin) as T
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, inner]) => [key, latin(inner)])) as T
  }
  return value
}

/** Serbian, in Latin letters (the game's `sr_sp`). */
export const words: Words = { ...latin(cyrillic), locale: 'sr-Latn-RS' }
