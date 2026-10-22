import { validateStyleMin } from '@maplibre/maplibre-gl-style-spec'
import { describe, expect, it } from 'vitest'
import type { MapData } from './mapData'
import { mapStyle } from './style'

const data: MapData = {
  tiles: 'http://localhost/map/tiles-1/{z}/{x}/{y}.pbf',
  signs: 'http://localhost/map/signs',
  minzoom: 6,
  maxzoom: 14,
  bounds: [-1, -1, 1, 1],
  centre: [0, 0],
  labels: [],
  places: null,
  countries: null,
}

describe('the map style', () => {
  // A style MapLibre refuses draws no map at all: the tunnels' colour wrapped round the dark map's zoom curve
  // did (a zoom curve may only be an expression's outermost), and the cab showed none (owner 2026-10-22).
  it.each(['light', 'dark'] as const)('is one MapLibre takes, in %s', (appearance) => {
    expect(validateStyleMin(mapStyle(appearance, data)).map((error) => error.message)).toEqual([])
  })
})
