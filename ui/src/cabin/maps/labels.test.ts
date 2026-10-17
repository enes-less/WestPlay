import { describe, expect, it } from 'vitest'
import { labelLines, placeLabels, scriptOf } from './labels'

const box = (x: number, y: number, width = 100, height = 30) => ({ x, y, width, height })

describe('placeLabels', () => {
  it('keeps the more important of two names that would overlap', () => {
    expect(placeLabels([box(0, 0), box(50, 10)], 0)).toEqual(new Set([0]))
  })

  it('keeps names clear of each other, padding included', () => {
    expect(placeLabels([box(0, 0), box(110, 0)], 0)).toEqual(new Set([0, 1]))
    expect(placeLabels([box(0, 0), box(110, 0)], 12)).toEqual(new Set([0]))
  })

  it('skips names out of view, and lets later ones fill the space', () => {
    expect(placeLabels([null, box(0, 0), box(40, 0), box(0, 100)], 0)).toEqual(new Set([1, 3]))
  })
})

describe('labelLines', () => {
  it("shows the player's name for a place in the same script as its own", () => {
    expect(labelLines('Wien', 'Viyana')).toEqual({ first: 'Viyana', second: null })
    expect(labelLines('Calais')).toEqual({ first: 'Calais', second: null })
  })

  it("keeps a name in another script on top, with the player's under it", () => {
    expect(labelLines('София', 'Sofya')).toEqual({ first: 'София', second: 'Sofya' })
    expect(labelLines('Αθήνα', 'Atina')).toEqual({ first: 'Αθήνα', second: 'Atina' })
    // The other way round too: a Latin name for a player reading Cyrillic.
    expect(labelLines('Berlin', 'Берлин')).toEqual({ first: 'Berlin', second: 'Берлин' })
  })
})

describe('scriptOf', () => {
  it('goes by the first letter', () => {
    expect(scriptOf('İstanbul')).toBe('latin')
    expect(scriptOf('Велико Търново')).toBe('cyrillic')
    expect(scriptOf('Ηράκλειο')).toBe('greek')
    expect(scriptOf('ベルリン')).toBe('cjk')
    expect(scriptOf('123')).toBe('none')
  })
})
