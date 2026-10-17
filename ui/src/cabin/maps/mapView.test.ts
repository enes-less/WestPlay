import { describe, expect, it } from 'vitest'
import { loadView, saveView } from './mapView'

function memory() {
  const items = new Map<string, string>()
  return {
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
  }
}

describe('the kept map view', () => {
  it('keeps the follow zoom and a browsed view, and reads them back', () => {
    const store = memory()
    expect(loadView(store)).toBeNull()
    saveView({ zoomOffset: -1.5, browse: null }, store)
    expect(loadView(store)).toEqual({ zoomOffset: -1.5, browse: null })
    const browse = { center: [-0.27, 0.045] as [number, number], zoom: 16, bearing: 30, pitch: 0, from: 15 }
    saveView({ zoomOffset: 0.5, browse }, store)
    expect(loadView(store)).toEqual({ zoomOffset: 0.5, browse })
  })

  it('takes a view it cannot read as none', () => {
    const store = memory()
    store.setItem('westplay.map.view', '{not json')
    expect(loadView(store)).toBeNull()
    store.setItem('westplay.map.view', JSON.stringify({ zoomOffset: 'far' }))
    expect(loadView(store)).toBeNull()
    store.setItem('westplay.map.view', JSON.stringify({ zoomOffset: 0, browse: { center: [1], zoom: 3 } }))
    expect(loadView(store)).toBeNull()
    expect(loadView(null)).toBeNull()
  })
})
