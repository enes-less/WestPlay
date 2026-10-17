import { describe, expect, it } from 'vitest'
import { type Destination, type DestinationKind, fold, nearestOfKind, ownGarages, searchPlaces } from './places'
import { toLngLat } from './projection'

const KINDS: Record<DestinationKind, string> = {
  fuel: 'Fuel',
  service: 'Service',
  garage: 'Garage',
  dealer: 'Dealer',
  company: 'Company',
  parking: 'Rest area',
  hotel: 'Hotel',
  recruitment: 'Recruitment agency',
  weigh: 'Weigh station',
  ferry: 'Ferry',
  train: 'Train',
  city: 'City',
}

/** A place `km` east of the truck (at 0, 0). */
function place(kind: DestinationKind, name: string | undefined, city: string, km: number, extra = {}): Destination {
  return { kind, name, city, at: toLngLat(km * 1000, 0), node: `${name}-${km}`, ...extra }
}

const truck = toLngLat(0, 0)

describe('fold', () => {
  it('matches whatever the accents and the Turkish i', () => {
    expect(fold('İstanbul')).toBe(fold('istanbul'))
    expect(fold('ISPARTA')).toBe('isparta')
    expect(fold('Jönköping')).toBe('jonkoping')
    expect(fold('Kırıkkale')).toBe('kirikkale')
  })
})

describe('searchPlaces', () => {
  const places = [
    place('company', 'Tradeaux', 'Taranto', 9),
    place('fuel', 'Filling station', 'Taranto', 4),
    place('city', 'Taranto', 'Taranto', 12),
    place('company', 'Tarandi', 'Bari', 80),
    place('service', 'Service', 'Kokkola', 300),
  ]
  const names = (found: Destination[]) => found.map((p) => `${p.name} (${p.city})`)

  it('puts the place named so first (a city before the rest), then those in a city of that name', () => {
    expect(names(searchPlaces(places, 'Tara', truck, KINDS))).toEqual([
      'Taranto (Taranto)',
      'Tarandi (Bari)',
      // In Taranto: nearest first.
      'Filling station (Taranto)',
      'Tradeaux (Taranto)',
    ])
  })

  it('finds by every word, a city, and what a place is', () => {
    expect(names(searchPlaces(places, 'service kokkola', truck, KINDS))).toEqual(['Service (Kokkola)'])
    expect(names(searchPlaces(places, 'fuel', truck, KINDS))).toEqual(['Filling station (Taranto)'])
    expect(searchPlaces(places, '  ', truck, KINDS)).toEqual([])
    expect(searchPlaces(places, 'zzz', truck, KINDS)).toEqual([])
  })
})

describe('nearestOfKind', () => {
  it('lists a kind nearest first', () => {
    const places = [place('fuel', 'A', 'X', 30), place('fuel', 'B', 'X', 2), place('hotel', 'C', 'X', 1)]
    expect(nearestOfKind(places, 'fuel', truck).map((p) => p.name)).toEqual(['B', 'A'])
    expect(nearestOfKind(places, 'fuel', truck, 1).map((p) => p.name)).toEqual(['B'])
  })
})

describe('ownGarages', () => {
  it("lists the player's garages, their headquarters' first", () => {
    const places = [
      place('garage', 'Garage', 'Bari', 1, { cityId: 'bari' }),
      place('garage', 'Garage', 'Kokkola', 2, { cityId: 'kokkola' }),
      place('garage', 'Garage', 'Turku', 3, { cityId: 'turku' }),
    ]
    expect(ownGarages(places, ['bari', 'kokkola'], 'kokkola').map((p) => p.cityId)).toEqual(['kokkola', 'bari'])
    expect(ownGarages(places, [], null)).toEqual([])
  })
})
