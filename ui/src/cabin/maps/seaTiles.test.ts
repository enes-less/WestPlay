import type maplibregl from 'maplibre-gl'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { addSeaProtocol, addSeaTiles, seaTile, seaTilesUrl } from './seaTiles'

const FOLDER = 'https://westplay.maps/europe-1/tiles-100'
const SEA = new Uint8Array([0x1a, 0x1f, 0x78, 0x02])

/** The files there: the sea tile, a tile of a shore; any other not there. */
function serve() {
  const asked: string[] = []
  vi.stubGlobal(
    'fetch',
    vi.fn(async (address: string) => {
      asked.push(address)
      if (address === `${FOLDER}/sea/14.pbf`) return new Response(SEA.slice())
      if (address === `${FOLDER}/14/9/9.pbf`) return new Response(new Uint8Array([7, 7]))
      if (address === `${FOLDER}/14/9/10.pbf`) return new Response('broken', { status: 500 })
      return new Response('', { status: 404 })
    }),
  )
  return asked
}

afterEach(() => vi.unstubAllGlobals())

describe("a map's tiles of open sea alone, listed and not written", () => {
  addSeaTiles(FOLDER, {
    zooms: {
      '14': {
        tile: 'sea/14.pbf',
        rows: [
          [5, 10, 12],
          [6, 3, 3],
          [5, 20, 20],
        ],
      },
    },
  })
  const url = (z: number, x: number, y: number) =>
    seaTilesUrl(FOLDER).replace('{z}', `${z}`).replace('{x}', `${x}`).replace('{y}', `${y}`)

  it("gives a listed tile its zoom's sea tile, fetched once, a copy each time", async () => {
    const asked = serve()
    for (const [x, y] of [
      [10, 5],
      [12, 5],
      [20, 5],
      [3, 6],
    ]) {
      const bytes = await seaTile(url(14, x, y))
      expect(new Uint8Array(bytes), `${x}, ${y}`).toEqual(SEA)
    }
    expect(asked).toEqual([`${FOLDER}/sea/14.pbf`])
  })

  it('fetches any other as it is, a tile not there nothing', async () => {
    const asked = serve()
    expect(new Uint8Array(await seaTile(url(14, 9, 9)))).toEqual(new Uint8Array([7, 7]))
    // Past a run's ends, another row, another zoom: not listed.
    expect((await seaTile(url(14, 13, 5))).byteLength).toBe(0)
    expect((await seaTile(url(14, 10, 6))).byteLength).toBe(0)
    expect((await seaTile(url(13, 10, 5))).byteLength).toBe(0)
    expect(asked).toEqual([
      `${FOLDER}/14/9/9.pbf`,
      `${FOLDER}/14/13/5.pbf`,
      `${FOLDER}/14/10/6.pbf`,
      `${FOLDER}/13/10/5.pbf`,
    ])
    await expect(seaTile(url(14, 9, 10))).rejects.toThrow('500')
  })

  it("leaves another map's tiles as they are", async () => {
    const asked = serve()
    const other = 'https://westplay.maps/europe-2/tiles-200'
    expect((await seaTile(`westplay-sea://${other}/14/10/5.pbf`)).byteLength).toBe(0)
    expect(asked).toEqual([`${other}/14/10/5.pbf`])
  })

  it('is added to MapLibre once, however many maps are made', () => {
    const addProtocol = vi.fn()
    const library = { addProtocol } as unknown as typeof maplibregl
    addSeaProtocol(library)
    addSeaProtocol(library)
    expect(addProtocol).toHaveBeenCalledTimes(1)
    expect(addProtocol.mock.calls[0][0]).toBe('westplay-sea')
  })
})
