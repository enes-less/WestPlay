import { describe, expect, it } from 'vitest'
import { DEFAULT_TEXT_SIZE, TEXT_SIZES, textScale, toTextSize } from './textSize'

describe('text size', () => {
  it('starts on the apps’ text as designed, with smaller and larger steps either side', () => {
    expect(textScale(DEFAULT_TEXT_SIZE)).toBe(1)
    expect(TEXT_SIZES.slice(0, DEFAULT_TEXT_SIZE).every((scale) => scale < 1)).toBe(true)
    expect(TEXT_SIZES.slice(DEFAULT_TEXT_SIZE + 1).every((scale) => scale > 1)).toBe(true)
  })

  it('keeps only a step there is', () => {
    expect(toTextSize(6)).toBe(6)
    expect(toTextSize(0)).toBe(0)
    for (const wrong of [-1, 7, 2.5, '4', null, undefined]) expect(toTextSize(wrong)).toBe(DEFAULT_TEXT_SIZE)
  })
})
