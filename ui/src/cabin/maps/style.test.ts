import { validateStyleMin } from '@maplibre/maplibre-gl-style-spec'
import { describe, expect, it } from 'vitest'
import { cameraBounds, type MapData } from './mapData'
import { mapStyle } from './style'

const data: MapData = {
  tiles: 'http://localhost/map/tiles-1/{z}/{x}/{y}.pbf',
  surfaces: 'http://localhost/map/tiles-1/surface/{z}/{x}/{y}.pbf',
  raised: 'http://localhost/map/tiles-1/surface/raised',
  meshes: null,
  signs: 'http://localhost/map/signs',
  minzoom: 6,
  maxzoom: 14,
  bounds: [-1, -1, 1, 1],
  world: null,
  centre: [0, 0],
  labels: [],
  places: null,
  countries: null,
  scale: 19,
}

describe('the map style', () => {
  // A style MapLibre refuses draws no map at all: the tunnels' colour wrapped round the dark map's zoom curve
  // did (a zoom curve may only be an expression's outermost), and the cab showed none (owner 2026-10-22).
  it.each(['light', 'dark'] as const)('is one MapLibre takes, in %s', (appearance) => {
    expect(validateStyleMin(mapStyle(appearance, data)).map((error) => error.message)).toEqual([])
  })

  // ATS's: the game's picture of North America, its sea over the page's ground and the sea beyond the picture; the
  // tiles over all of it. What hasn't loaded shows as ground, not sea.
  it('draws the game world beyond the map when it has one, else the sea all round', () => {
    const world = {
      url: 'http://localhost/map/tiles-1/world.json',
      bounds: [-9, -9, 9, 9] as [number, number, number, number],
      cut: [null, -8, 8, 8] as [number | null, number | null, number | null, number | null],
    }
    const ats = { ...data, world }
    const style = mapStyle('light', ats)
    expect(validateStyleMin(style).map((error) => error.message)).toEqual([])
    const ids = style.layers.map((layer) => layer.id)
    expect(ids.slice(ids.indexOf('beyond'), ids.indexOf('beyond') + 2)).toEqual(['beyond', 'world-sea'])
    expect(ids.indexOf('world-sea')).toBeLessThan(ids.indexOf('sea'))
    const beyond = (map: MapData) => (mapStyle('light', map).sources['westplay-beyond'] as { data: unknown }).data
    // The frame's hole: the picture's bounds, else the map's.
    expect(beyond(ats)).toMatchObject({ geometry: { coordinates: { 1: { 0: [-9, -9] } } } })
    expect(beyond(data)).toMatchObject({ geometry: { coordinates: { 1: { 0: [-1, -1] } } } })
    expect(mapStyle('light', data).layers.some((layer) => layer.id.startsWith('world'))).toBe(false)
  })

  // The camera goes a degree past the map's edges, but never past where the game's picture cuts its land off.
  it('keeps the camera within the game picture where its land is cut off', () => {
    expect(cameraBounds(data)).toEqual([
      [-2, -2],
      [2, 2],
    ])
    const world = { url: 'world.json', bounds: [-9, -9, 9, 9] as [number, number, number, number] }
    const cut = { ...data, world: { ...world, cut: [null, -1.5, 1.2, 1.7] as [number | null, number | null, number | null, number | null] } }
    expect(cameraBounds(cut)).toEqual([
      [-2, -1.5],
      [1.2, 1.7],
    ])
  })
})
