import { describe, expect, it } from 'vitest'
import { logoBrands } from './brand'

describe('logoBrands', () => {
  it("is the game's own brand id for its trucks", () => {
    expect(logoBrands('daf', 'DAF')).toEqual(['daf'])
    expect(logoBrands('scania', 'Scania')).toEqual(['scania'])
  })

  it("falls back to the game's brand a modded truck is a version of", () => {
    // A modded DAF XG+ (seen in the cab: "DAF 2021 Jasper").
    expect(logoBrands('new_daf', 'DAF')).toEqual(['new_daf', 'daf'])
    expect(logoBrands('mb_actros_mp5', 'Mercedes-Benz')).toEqual(['mb_actros_mp5', 'mercedes'])
    expect(logoBrands('volvo.fh16_classic', 'Volvo')).toEqual(['volvo.fh16_classic', 'volvo'])
  })

  it("names the brand of a mod vehicle that doesn't name it", () => {
    // Oyuncuyusbis' Travego 16 SHD, as the SDK reports it: a Mercedes-Benz.
    expect(logoBrands('t16shd', 'Travego 16SHD (OB)')).toEqual(['t16shd', 'mercedes'])
    // Their Lion's Coach names its brand ("Man"); their Setras are Setras.
    expect(logoBrands('manlions', 'Man')).toEqual(['manlions', 'man'])
    expect(logoBrands('setra516hd', 'Setra')).toEqual(['setra516hd', 'setra'])
    expect(logoBrands('setradt531', 'Daimler Setra S531DT')).toEqual(['setradt531', 'setra'])
  })

  it('finds only whole words, so a brand inside another word is no match', () => {
    expect(logoBrands('german_trucks', 'Germanic')).toEqual(['german_trucks'])
  })

  it('has its own id alone for a brand the game has no version of', () => {
    expect(logoBrands('tatra', 'Tatra')).toEqual(['tatra'])
    expect(logoBrands('', '')).toEqual([])
  })
})
