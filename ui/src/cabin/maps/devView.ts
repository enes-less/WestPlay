import maplibregl from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import { buildingMeshes } from './buildingMeshes'
import { loadMapData } from './mapData'
import { NO_ENTRY_ICON, drawNoEntry } from './placeIcons'
import { METRES_PER_DEGREE } from './projection'
import { raisedRoads } from './raisedRoads'
import { mapStyle } from './style'

/** The development map alone (dev-map.html), to look at a place: `?at=<x>,<z>&zoom=17&look=dark&pitch=0&bearing=0`,
 * x and z in game metres. The map's `window.westplayMap` for looking into it. */
const query = new URLSearchParams(location.search)
const [x, z] = (query.get('at') ?? '0,0').split(',').map(Number)

void loadMapData().then((data) => {
  if (!data) return
  const map = new maplibregl.Map({
    container: 'map',
    style: mapStyle(query.get('look') === 'light' ? 'light' : 'dark', data),
    center: [x / METRES_PER_DEGREE, -z / METRES_PER_DEGREE],
    zoom: Number(query.get('zoom') ?? 17),
    pitch: Number(query.get('pitch') ?? 0),
    bearing: Number(query.get('bearing') ?? 0),
    maxZoom: 21,
    attributionControl: false,
    fadeDuration: 0,
    canvasContextAttributes: { antialias: true, preserveDrawingBuffer: true },
  })
  if (data.raised) {
    const look = query.get('look') === 'light' ? 'light' : 'dark'
    const raised = raisedRoads(map, data.raised, () => look)
    map.on('render', () => raised.update(performance.now()))
    Object.assign(window, { westplayRaised: raised })
  }
  if (data.meshes) {
    const look = query.get('look') === 'light' ? 'light' : 'dark'
    const meshes = buildingMeshes(map, data.meshes, () => look)
    map.on('render', () => meshes.update(performance.now()))
    Object.assign(window, { westplayMeshes: meshes })
  }
  map.on('styleimagemissing', ({ id }: { id: string }) => {
    if (id === NO_ENTRY_ICON && !map.hasImage(id)) {
      map.addImage(id, drawNoEntry(window.devicePixelRatio || 1), { pixelRatio: window.devicePixelRatio || 1 })
    }
  })
  // A page in a hidden pane gets no animation frames: going to a place draws it by hand until its tiles are in.
  const go = async (gameX: number, gameZ: number, zoom: number) => {
    map.jumpTo({ center: [gameX / METRES_PER_DEGREE, -gameZ / METRES_PER_DEGREE], zoom })
    for (let round = 0; round < 10; round++) {
      map.redraw()
      await new Promise((done) => setTimeout(done, 200))
    }
    map.redraw()
  }
  Object.assign(window, { westplayMap: map, westplayGo: go })
})
