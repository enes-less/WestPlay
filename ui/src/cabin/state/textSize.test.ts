import { describe, expect, it } from 'vitest'
import { DEFAULT_TEXT_SIZE, TEXT_SIZES, textScale, toTextSize } from './textSize'

describe('text size', () => {
  it('goes from a little smaller than the apps’ text as designed to twice it, starting a quarter larger', () => {
    expect(TEXT_SIZES[0]).toBe(0.8)
    expect(TEXT_SIZES[TEXT_SIZES.length - 1]).toBe(2)
    expect(textScale(DEFAULT_TEXT_SIZE)).toBe(1.24)
    expect(TEXT_SIZES.every((scale, index) => index === 0 || scale > TEXT_SIZES[index - 1])).toBe(true)
  })

  it('keeps only a step there is', () => {
    expect(toTextSize(8)).toBe(8)
    expect(toTextSize(0)).toBe(0)
    for (const wrong of [-1, 9, 2.5, '4', null, undefined]) expect(toTextSize(wrong)).toBe(DEFAULT_TEXT_SIZE)
  })
})
