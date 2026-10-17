import { describe, expect, it } from 'vitest'
import { languages } from '../../onboarding/words'
import { companionLanguages, companionWords } from '.'

/** Every text in `words`, by its path. */
function texts(value: unknown, path = ''): [string, string][] {
  if (typeof value === 'string') return [[path, value]]
  if (value && typeof value === 'object') {
    return Object.entries(value).flatMap(([key, inner]) => texts(inner, path ? `${path}.${key}` : key))
  }
  return []
}

/** The `{names}` a text fills in. */
const slots = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort()

describe('the companion’s words', () => {
  it('are there in each of the game’s languages, as the onboarding’s', () => {
    expect(companionLanguages()).toEqual(languages())
  })

  const english = new Map(texts(companionWords('en_us')))

  it.each(companionLanguages())('%s has every text, none empty, each filling in what English does', (code) => {
    const words = texts(companionWords(code))
    expect(words.map(([path]) => path).sort()).toEqual([...english.keys()].sort())
    for (const [path, text] of words) {
      expect(text.trim(), path).not.toBe('')
      expect(slots(text), `${code} ${path}`).toEqual(slots(english.get(path)!))
    }
  })

  it('falls back to English for a language the game doesn’t have', () => {
    expect(companionWords('xx_xx')).toBe(companionWords('en_us'))
  })
})
