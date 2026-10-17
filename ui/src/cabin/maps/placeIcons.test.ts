import { describe, expect, it } from 'vitest'
import { parsePlaceIcon, placeIconName, towards, wrapName } from './placeIcons'

describe('place icons', () => {
  it('names images for a kind, and a name, and reads them back', () => {
    expect(placeIconName('fuel')).toBe('place-fuel')
    expect(placeIconName('dealer', 'Mercedes-Benz')).toBe('place-dealer|Mercedes-Benz')
    expect(parsePlaceIcon('place-dealer|Mercedes-Benz')).toEqual({ kind: 'dealer', name: 'Mercedes-Benz' })
    expect(parsePlaceIcon('place-garage')).toEqual({ kind: 'garage', name: null })
    expect(parsePlaceIcon('place-ferry|Calais')).toEqual({ kind: 'ferry', name: 'Calais' })
    expect(parsePlaceIcon('place-hotel')).toEqual({ kind: 'hotel', name: null })
    expect(parsePlaceIcon('place-casino')).toBeNull()
    expect(parsePlaceIcon('road-arrow')).toBeNull()
  })

  it('wraps names at whole words, within the lines allowed', () => {
    const measure = (text: string) => text.length * 10
    expect(wrapName('Vilniaus Popieriaus Fabrikas', measure, 200, 3)).toEqual(['Vilniaus Popieriaus', 'Fabrikas'])
    expect(wrapName('ACC', measure, 200, 3)).toEqual(['ACC'])
    expect(wrapName('a b c d', measure, 10, 2)).toEqual(['a', 'b c d'])
  })

  it('moves a colour towards white or black', () => {
    expect(towards('#000000', 255, 0.5)).toBe('rgb(128 128 128)')
    expect(towards('#ffffff', 0, 0.5)).toBe('rgb(128 128 128)')
    expect(towards('#3478f6', 255, 0)).toBe('rgb(52 120 246)')
  })
})
