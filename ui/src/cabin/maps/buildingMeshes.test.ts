import { describe, expect, it } from 'vitest'
import { lightAt, readModel, readTile } from './buildingMeshes'

/** A model's file as crates/mapdata meshes.rs writes it: one level, a triangle. */
function modelBytes(): ArrayBuffer {
  const bytes = new ArrayBuffer(16 + 12 + 20 + 8)
  const view = new DataView(bytes)
  view.setUint32(0, 0x314d5057, true)
  view.setFloat32(4, 0.5, true)
  view.setFloat32(8, 4, true)
  view.setUint32(12, 1, true)
  view.setUint32(16, 3, true)
  view.setUint32(20, 3, true)
  view.setUint32(24, 2, true)
  const vertices = [0, 0, 0, 8, 0, 0, 8, 8, 0]
  vertices.forEach((value, i) => view.setInt16(28 + i * 2, value, true))
  ;[0, 1, 2].forEach((value, i) => view.setUint16(48 + i * 2, value, true))
  return bytes
}

/** A tile's file: two placings of model 3, one of model 5, and a bridge's triangle. */
function tileBytes(): ArrayBuffer {
  const bytes = new ArrayBuffer(12 + 3 * 28 + 3 * 6 + 2)
  const view = new DataView(bytes)
  view.setUint32(0, 0x31545057, true)
  view.setUint32(4, 3, true)
  view.setUint32(8, 3, true)
  ;[3, 3, 5].forEach((model, i) => view.setUint32(12 + i * 28, model, true))
  return bytes
}

describe('buildingMeshes', () => {
  it('reads a model file as written', () => {
    const model = readModel(modelBytes())
    expect(model).not.toBeNull()
    expect(model!.unit).toBe(0.5)
    expect(model!.top).toBe(4)
    expect(model!.levels).toHaveLength(1)
    expect([...model!.levels[0].vertices]).toEqual([0, 0, 0, 8, 0, 0, 8, 8, 0])
    expect([...model!.levels[0].indices]).toEqual([0, 1, 2])
  })

  it('takes no model from a file that isn’t whole', () => {
    expect(readModel(modelBytes().slice(0, 40))).toBeNull()
    expect(readModel(new ArrayBuffer(4))).toBeNull()
  })

  it('reads a tile’s placings in runs of a model, and its bridges', () => {
    const tile = readTile(1, 2, tileBytes())
    expect(tile).not.toBeNull()
    expect(tile!.runs).toEqual([
      { model: 3, first: 0, count: 2 },
      { model: 5, first: 2, count: 1 },
    ])
    expect(tile!.bridge.length).toBe(9)
    expect(readTile(1, 2, tileBytes().slice(0, 50))).toBeNull()
  })

  it('lights as MapLibre does, the light turned with the map', () => {
    // MapLibre's position [1.15, 210, 30]: azimuth 300° from x, 30° from straight up.
    const [x, y, z] = lightAt(0)
    expect(x).toBeCloseTo(0.2875, 3)
    expect(y).toBeCloseTo(-0.498, 3)
    expect(z).toBeCloseTo(0.996, 3)
    const [tx, ty] = lightAt(90)
    expect(tx).toBeCloseTo(0.498, 3)
    expect(ty).toBeCloseTo(0.2875, 3)
  })
})
