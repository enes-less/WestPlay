import maplibregl, { type GeoJSONSource } from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import workerUrl from 'maplibre-gl/dist/maplibre-gl-csp-worker.js?url'
import { create as createStore } from 'zustand'
import type { Telemetry } from '../../shared/host'
import type { Appearance } from '../state/appearance'
import { type LiveState, useLive } from '../state/store'
import {
  AUTO_RECENTRE_MS,
  bearingOf,
  blendPose,
  type CameraPose,
  clampZoomOffset,
  damp,
  dampBearing,
  easeInOut,
  FOLLOW_DELAY_MS,
  GLIDE_MS,
  samePose,
  type TruckSample,
  truckAt,
  zoomForSpeed,
  zoomOffsetFor,
} from './camera'
import { labelLines, placeLabels } from './labels'
import { drawPlaceIcon, parsePlaceIcon } from './placeIcons'
import type { MapData } from './mapData'
import { loadView, type SavedView, saveView } from './mapView'
import { fromLngLat, labelRange, metresBetween, toLngLat } from './projection'
import {
  approachLine,
  headLine,
  type Point,
  type Progress,
  progressOf,
  type RaisedRoad,
  restLine,
  routeLevels,
  routeSpans,
  snapToRoute,
  type Spans,
} from './route'
import { MAP_SOURCE, mapStyle, paintFor, placeImagesFor, ROUTE_HEAD_SOURCE, ROUTE_SOURCE } from './style'

/**
 * The one live map of the page. It's made once and moved between the screens that show it (the
 * Dashboard's map tile, the Maps app), so opening either is instant: no new WebGL context, no parsing
 * the map again, and the camera carries on where it was.
 *
 * It follows the truck. In the Maps app the player can also move it (holding the interaction key): drag
 * to pan, right-drag to turn, the wheel or the zoom buttons to zoom. Moving it leaves the truck behind
 * (browsing) until the recentre button, or a while without touching it, glides it back; zooming while
 * following only brings the following camera closer or further.
 */

// MapLibre's worker as its own file: the copy inlined in its main bundle never starts once Vite has
// pre-bundled it.
maplibregl.setWorkerUrl(workerUrl)

/** The driving view's tilt, degrees. */
const PITCH = 52
/** The truck sits this far down the map (share of its height above it), leaving the road ahead in view. */
const PUCK_FROM_TOP = 0.66
/** How quickly the map turns after the truck (ms): slow enough to feel calm, quick enough to keep up. */
const BEARING_TAU_MS = 450
const ZOOM_TAU_MS = 1500
const SAMPLES_KEPT = 8
/** Place names' type size (a second line smaller: labelLines), and halo (maps.css), and the space kept
 * between two names, pixels. */
const LABEL_SIZE = 26
const LABEL_SECOND_SIZE = 20
const LABEL_HALO = 4
const LABEL_GAP = 16
/** Zoom levels per wheel notch (120 units). */
const WHEEL_ZOOM = 0.5
const MIN_ZOOM = 9
const MAX_ZOOM = 19.5
/** How far the truck moves along the route before the route's head is drawn again from it, metres. */
const ROUTE_HEAD_STEP = 0.5
/** How often, at most, the raised roads are read again from the tiles as they load, ms. */
const RAISED_EVERY_MS = 500

type Mode = 'follow' | 'browse' | 'return'

interface LiveMap {
  data: MapData
  map: maplibregl.Map
  /** The map's own container, moved into whichever screen shows it. */
  element: HTMLDivElement
  puck: maplibregl.Marker
  /** The end of the route. */
  destination: maplibregl.Marker
  /** The route drawn (`route`, as the store has it), as spans, with the span the rest of it is drawn after
   * and where its head was last drawn from (`null`: not drawn yet). */
  routeShown: { route: LiveState['route']; spans: Spans; restAfter: number | null; headFrom: Point | null } | null
  /** Where the truck is along the route this frame (`null`: no route). */
  progress: Progress | null
  /** The raised road stretches in the tiles loaded (bridges: the route on one is drawn over the road
   * beneath), what they were read as (to tell when they change), whether tiles have loaded since, and when
   * they were last read. */
  raised: { roads: RaisedRoad[]; read: string; stale: boolean; at: number }
  /** Place names, most important first, with their size on screen. */
  labels: { marker: maplibregl.Marker; at: [number, number]; width: number; height: number }[]
  /** The appearance the page wants, and the one the map is painted in (they differ until it's restyled). */
  wanted: Appearance
  painted: Appearance
  /** The owned garages' cities the place icons show as the player's (`null`: not applied yet). */
  garagesShown: string | null
  /** The following camera's damped bearing and zoom. */
  camera: { bearing: number; zoom: number } | null
  mode: Mode
  /** Whether the screen showing it lets the player move it (the Maps app). */
  interactive: boolean
  /** A mouse button is held on the map: a drag may be starting, which moving the camera would cancel. */
  pressed: boolean
  /** When the player last moved the map, ms. */
  touched: number
  /** How far the player zoomed the following camera from its speed zoom. */
  zoomOffset: number
  /** The zoom when browsing began: a different zoom when it ends is one the player chose, and kept. */
  browsedFrom: number
  /** The glide back onto the truck: where it set off from, and when. */
  glide: { from: CameraPose; started: number } | null
  /** The view the place names were last placed for (`null`: place them again). */
  labelsPlacedFor: string | null
  /** The view as last kept (mapView.ts), and when it was last looked at. */
  kept: string
  keptAt: number
}

/** Whether the map is browsing (moved away from the truck): the Maps app shows its recentre button. */
export const useMapBrowsing = createStore<{ browsing: boolean }>()(() => ({ browsing: false }))

let live: LiveMap | null = null

/** The truck's recent positions, kept from the first map onwards so the camera never starts cold. */
const samples: TruckSample[] = []
let collecting = false

function collectSamples() {
  if (collecting) return
  collecting = true
  const add = (telemetry: Telemetry) => {
    samples.push({
      time: performance.now(),
      x: telemetry.x,
      z: telemetry.z,
      heading: telemetry.heading,
      speed: telemetry.speed,
    })
    if (samples.length > SAMPLES_KEPT) samples.shift()
  }
  // Where the truck is now: the host only sends telemetry when it changes, so a parked truck may send
  // nothing more for a long while.
  const current = useLive.getState().telemetry
  if (current) add(current)
  useLive.subscribe((state, previous) => {
    if (state.telemetry && state.telemetry !== previous.telemetry) add(state.telemetry)
  })
}

function create(data: MapData, appearance: Appearance): LiveMap {
  const element = document.createElement('div')
  element.className = 'road-map'
  const map = new maplibregl.Map({
    container: element,
    style: mapStyle(appearance, data),
    center: data.centre,
    zoom: 15,
    pitch: PITCH,
    maxPitch: 70,
    minZoom: MIN_ZOOM,
    maxZoom: MAX_ZOOM,
    // Gestures are switched on only where the map can be moved (setInteractive).
    interactive: false,
    // Turning (right-drag) keeps the tilt.
    pitchWithRotate: false,
    attributionControl: false,
    fadeDuration: 0,
    canvasContextAttributes: { antialias: true },
  })

  // Added to the map when in range and clear of more important names (showLabels). Each says what
  // labelLines gives: the name in the player's language, under the place's own where that's in another
  // script.
  const measure = document.createElement('canvas').getContext('2d')
  const fontFamily = getComputedStyle(document.body).fontFamily
  const font = (size: number) => `600 ${size}px ${fontFamily}`
  const textWidth = (text: string, size: number) => {
    if (!measure) return text.length * size * 0.6
    measure.font = font(size)
    return measure.measureText(text).width
  }
  const lines = [...data.labels]
    .sort((a, b) => b.population - a.population)
    .map((label) => ({ label, ...labelLines(label.name, label.localized) }))
  const measured = ({ first, second }: { first: string; second: string | null }) => ({
    width: Math.max(textWidth(first, LABEL_SIZE), second ? textWidth(second, LABEL_SECOND_SIZE) : 0) + LABEL_HALO * 2,
    height: (LABEL_SIZE + (second ? LABEL_SECOND_SIZE : 0)) * 1.2 + LABEL_HALO * 2,
  })
  const labels = lines.map(({ label, first, second }) => {
    const element = document.createElement('div')
    element.className = `map-label map-label-${label.kind}`
    const line = (text: string, className: string) => {
      const span = document.createElement('span')
      span.className = className
      span.textContent = text
      element.append(span)
    }
    line(first, 'map-label-first')
    if (second) line(second, 'map-label-second')
    const marker = new maplibregl.Marker({ element, pitchAlignment: 'viewport', rotationAlignment: 'viewport' })
    return { marker: marker.setLngLat(label.at), at: label.at, ...measured({ first, second }) }
  })
  // Measured again once the type for every name is in: the fonts for other scripts load only when a page
  // first needs them, and until then the sizes are a fallback font's.
  const allText = lines.map(({ first, second }) => `${first} ${second ?? ''}`).join(' ')
  Promise.all([document.fonts.load(font(LABEL_SIZE), allText), document.fonts.load(font(LABEL_SECOND_SIZE), allText)])
    .then(() => {
      lines.forEach((line, index) => Object.assign(labels[index], measured(line)))
      created.labelsPlacedFor = null
    })
    .catch(() => {})

  const puckElement = document.createElement('div')
  puckElement.className = 'map-puck'
  puckElement.innerHTML =
    '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4.5 18 19l-6-3.2L6 19Z" fill="currentColor"/></svg>'
  const puck = new maplibregl.Marker({ element: puckElement, pitchAlignment: 'viewport', rotationAlignment: 'map' })
  const destinationElement = document.createElement('div')
  destinationElement.className = 'map-destination'
  destinationElement.innerHTML =
    '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6.5 21V4.2c3.6-1.9 6.6 1.6 11 0v8.6c-4.4 1.6-7.4-1.9-11 0" fill="currentColor" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"/></svg>'
  const destination = new maplibregl.Marker({
    element: destinationElement,
    pitchAlignment: 'viewport',
    rotationAlignment: 'viewport',
  })

  const created: LiveMap = {
    data,
    map,
    element,
    puck,
    destination,
    routeShown: null,
    progress: null,
    raised: { roads: [], read: '', stale: true, at: -Infinity },
    labels,
    wanted: appearance,
    painted: appearance,
    garagesShown: null,
    camera: null,
    mode: 'follow',
    interactive: false,
    pressed: false,
    touched: 0,
    zoomOffset: 0,
    browsedFrom: 0,
    glide: null,
    labelsPlacedFor: null,
    kept: '',
    keptAt: 0,
  }
  // Place icons are drawn when the map first needs each (placeIcons.ts), and again once the type for their
  // names is in.
  map.on('styleimagemissing', ({ id }: { id: string }) => {
    const icon = parsePlaceIcon(id)
    if (!icon || map.hasImage(id)) return
    const pixelRatio = window.devicePixelRatio || 1
    map.addImage(id, drawPlaceIcon(icon.kind, icon.name, created.painted, pixelRatio, fontFamily), { pixelRatio })
  })
  document.fonts.ready.then(() => redrawPlaceIcons(created)).catch(() => {})
  listen(created)
  restoreView(created)
  return created
}

/** How often the view is looked at to be kept, ms. */
const KEEP_EVERY_MS = 500

/** The view the player left the map in, kept from before (a hot reload, a reload, a restart): their
 * follow zoom, and where they'd browsed to (with the full while to look again before it glides back). */
function restoreView(target: LiveMap) {
  const saved = loadView()
  if (!saved) return
  target.zoomOffset = clampZoomOffset(saved.zoomOffset)
  if (saved.browse) {
    const { center, zoom, bearing, pitch, from } = saved.browse
    target.map.jumpTo({ center, zoom, bearing, pitch })
    browse(target)
    target.browsedFrom = from
  }
  target.kept = JSON.stringify(viewOf(target))
}

/** Keeps the view, now and then, when it has changed. */
function keepView(target: LiveMap, now: number) {
  if (now - target.keptAt < KEEP_EVERY_MS) return
  target.keptAt = now
  const view = JSON.stringify(viewOf(target))
  if (view === target.kept) return
  target.kept = view
  saveView(JSON.parse(view) as SavedView)
}

function viewOf({ map, mode, zoomOffset, browsedFrom }: LiveMap): SavedView {
  const browse =
    mode === 'browse'
      ? {
          center: map.getCenter().toArray() as [number, number],
          zoom: map.getZoom(),
          bearing: map.getBearing(),
          pitch: map.getPitch(),
          from: browsedFrom,
        }
      : null
  return { zoomOffset, browse }
}

/** The player's hands on the map. */
function listen(target: LiveMap) {
  const { map } = target
  const canvas = map.getCanvasContainer()
  canvas.addEventListener('mousedown', () => {
    if (!target.interactive) return
    target.pressed = true
    target.touched = performance.now()
  })
  window.addEventListener('mouseup', () => {
    target.pressed = false
  })
  // Dragging or turning it leaves the truck behind.
  const leave = (event: { originalEvent?: unknown }) => {
    if (event.originalEvent && target.interactive) browse(target)
  }
  map.on('dragstart', leave)
  map.on('rotatestart', leave)
  map.on('move', (event) => {
    if (event.originalEvent) target.touched = performance.now()
  })
  // A tile in: the raised roads may be more (syncRoute reads them again).
  map.on('sourcedata', (event) => {
    if (event.sourceId === MAP_SOURCE && event.tile) target.raised.stale = true
  })
  // The wheel, while following: closer or further, still following. (Browsing, MapLibre zooms.)
  canvas.addEventListener(
    'wheel',
    (event) => {
      if (!target.interactive) return
      target.touched = performance.now()
      if (target.mode !== 'browse') {
        target.zoomOffset = clampZoomOffset(target.zoomOffset - (event.deltaY / 120) * WHEEL_ZOOM)
      }
    },
    { passive: true },
  )
}

function browse(target: LiveMap) {
  // Measured from here (a glide back cut short may have moved the zoom already: that's no choice).
  target.browsedFrom = target.map.getZoom()
  target.mode = 'browse'
  target.glide = null
  target.touched = performance.now()
  // Browsing, the wheel zooms around the cursor, like any desktop map.
  target.map.scrollZoom.enable()
  useMapBrowsing.setState({ browsing: true })
}

function follow(target: LiveMap) {
  target.mode = 'follow'
  target.glide = null
  target.map.scrollZoom.disable()
  useMapBrowsing.setState({ browsing: false })
}

/** Switches the gestures for the screen showing the map: the Maps app moves it, the Dashboard doesn't. */
function setInteractive(target: LiveMap, interactive: boolean) {
  target.interactive = interactive
  const { map } = target
  for (const handler of [map.dragPan, map.dragRotate]) {
    if (interactive) handler.enable()
    else handler.disable()
  }
  if (!interactive) {
    target.pressed = false
    follow(target)
    // Straight onto the truck.
    target.camera = null
  }
}

/**
 * Restyles the map to the wanted appearance, if it isn't already. Called every frame: until the style
 * itself has loaded, MapLibre refuses paint changes, and the next frame tries again. (Its "is the style
 * loaded" check also waits for every tile, which never settles while driving: a switch made then was lost.)
 */
function syncAppearance(target: LiveMap) {
  if (target.painted === target.wanted) return
  try {
    for (const { layer, property, value } of paintFor(target.wanted)) {
      target.map.setPaintProperty(layer, property, value)
    }
    target.painted = target.wanted
    // Names' colours follow the appearance.
    redrawPlaceIcons(target)
  } catch {
    // The style is still loading.
  }
}

/** Shows the player's own garages as theirs (the host reads them from their save), once they change.
 * Like the appearance, retried each frame until the style has loaded. */
function syncGarages(target: LiveMap) {
  const owned = useLive.getState().ownedGarages
  const key = owned.join(',')
  if (target.garagesShown === key) return
  try {
    for (const { layer, value } of placeImagesFor(owned)) target.map.setLayoutProperty(layer, 'icon-image', value)
    target.garagesShown = key
  } catch {
    // The style is still loading.
  }
}

/** Draws the place icons the map has again, for its appearance (names' colours) and type. */
function redrawPlaceIcons({ map, painted }: LiveMap) {
  const pixelRatio = window.devicePixelRatio || 1
  const fontFamily = getComputedStyle(document.body).fontFamily
  for (const id of map.listImages()) {
    const icon = parsePlaceIcon(id)
    if (icon?.name) map.updateImage(id, drawPlaceIcon(icon.kind, icon.name, painted, pixelRatio, fontFamily))
  }
}

/** The screens showing the map, latest last: it's in the latest, and goes back when that one closes. */
const hosts: { element: HTMLElement; interactive: boolean }[] = []
let frame: number | null = null

/**
 * Shows the live map in `host` and follows the truck there (letting the player move it if `interactive`),
 * until the returned function is called (the map then stays alive, ready for the next screen).
 */
export function showLiveMap(
  host: HTMLElement,
  data: MapData,
  appearance: Appearance,
  interactive: boolean,
): () => void {
  collectSamples()
  if (live?.data !== data) {
    if (frame !== null) cancelAnimationFrame(frame)
    frame = null
    live?.map.remove()
    live = create(data, appearance)
    // Development: the map, for inspecting it from the browser's console.
    if (import.meta.env.DEV) Object.assign(window, { westplayMap: live.map })
  }
  live.wanted = appearance
  syncAppearance(live)
  const entry = { element: host, interactive }
  hosts.push(entry)
  place()
  return () => {
    hosts.splice(hosts.lastIndexOf(entry), 1)
    place()
  }
}

/** Puts the map in the latest screen showing it, following the truck while any does. */
function place() {
  if (!live) return
  const current = live
  const host = hosts.at(-1)
  if (!host) {
    current.element.remove()
    if (frame !== null) cancelAnimationFrame(frame)
    frame = null
    return
  }
  if (current.element.parentElement !== host.element) {
    host.element.appendChild(current.element)
    current.map.resize()
    // Straight onto the truck, without easing in from wherever the camera was.
    current.camera = null
  }
  if (current.interactive !== host.interactive) setInteractive(current, host.interactive)
  if (frame === null) {
    let lastFrame = performance.now()
    frame = requestAnimationFrame(function step(now) {
      frame = requestAnimationFrame(step)
      const dt = Math.min(now - lastFrame, 100)
      lastFrame = now
      syncAppearance(current)
      syncGarages(current)
      syncRoute(current, now)
      updateCamera(current, now, dt)
      showLabels(current)
      keepView(current, now)
    })
  }
}

export function setLiveMapAppearance(appearance: Appearance) {
  if (!live) return
  live.wanted = appearance
  syncAppearance(live)
}

/** Glides the map back onto the truck (the recentre button). */
export function recentreLiveMap() {
  if (!live || live.mode !== 'browse') return
  const { map } = live
  // A zoom the player chose while browsing (the wheel, the buttons) stays: the camera follows at it.
  if (Math.abs(map.getZoom() - live.browsedFrom) > 0.01) {
    const speed = truckAt(samples, performance.now() - FOLLOW_DELAY_MS)?.speed ?? 0
    live.zoomOffset = zoomOffsetFor(map.getZoom(), speed)
  }
  live.glide = {
    from: {
      center: map.getCenter().toArray() as [number, number],
      zoom: map.getZoom(),
      bearing: map.getBearing(),
      pitch: map.getPitch(),
    },
    started: performance.now(),
  }
  live.mode = 'return'
  map.scrollZoom.disable()
  useMapBrowsing.setState({ browsing: false })
}

/** Zooms in (1) or out (-1) a step: the following camera closer or further, or the browsed map. */
export function zoomLiveMap(direction: 1 | -1) {
  if (!live) return
  live.touched = performance.now()
  // Browsing, or with no truck to follow (none yet: the camera only follows one), the map itself zooms.
  const following = truckAt(samples, performance.now() - FOLLOW_DELAY_MS) !== null
  if (live.mode === 'browse' || !following) {
    live.map.easeTo({ zoom: live.map.getZoom() + direction, duration: 300 })
  } else {
    live.zoomOffset = clampZoomOffset(live.zoomOffset + direction)
  }
}

/**
 * Shows the place names that fit: only places near enough to matter at this zoom (the tilted view reaches
 * to the horizon, where everything far away would pile up, and the game's distances between cities are
 * short), and of those, none overlapping a more important one.
 */
function showLabels(target: LiveMap) {
  const { map, labels } = target
  const centre = map.getCenter().toArray() as [number, number]
  const { clientWidth: width, clientHeight: height } = map.getContainer()
  // Only when the view has changed: placing them projects every name.
  const view = `${centre} ${map.getZoom()} ${map.getBearing()} ${map.getPitch()} ${width}x${height}`
  if (view === target.labelsPlacedFor) return
  target.labelsPlacedFor = view
  const range = labelRange(map.getZoom(), width)
  const boxes = labels.map((label) => {
    if (metresBetween(centre, label.at) > range) return null
    const { x, y } = map.project(label.at)
    if (x < -label.width || x > width + label.width || y < -label.height || y > height + label.height) return null
    return { x, y, width: label.width, height: label.height }
  })
  const shown = placeLabels(boxes, LABEL_GAP)
  labels.forEach(({ marker }, index) => {
    const visible = marker.getElement().isConnected
    if (shown.has(index) && !visible) marker.addTo(map)
    else if (!shown.has(index) && visible) marker.remove()
  })
}

/** A unit direction on the ground (x east, z south) for a compass bearing (degrees). */
function along(bearing: number): Point {
  const radians = (bearing * Math.PI) / 180
  return [Math.sin(radians), -Math.cos(radians)]
}

/** A route source's data. */
type LineData = Parameters<GeoJSONSource['setData']>[0]

const NO_LINE: LineData = { type: 'FeatureCollection', features: [] }

/** The route line `points`, in parts by the level each is drawn on (route.ts routeLevels: over `raised`). */
function lineOf(points: Point[], raised: RaisedRoad[]): LineData {
  if (points.length < 2) return NO_LINE
  return {
    type: 'FeatureCollection',
    features: routeLevels(points, raised).map(({ points, level }) => ({
      type: 'Feature',
      properties: { level },
      geometry: { type: 'LineString', coordinates: points.map(([x, z]) => toLngLat(x, z)) },
    })),
  }
}

/** The raised road stretches in the tiles loaded (style.ts ROAD_LEVELS), on the game's ground. */
function raisedRoads(map: maplibregl.Map): RaisedRoad[] {
  const raised = map.querySourceFeatures(MAP_SOURCE, {
    sourceLayer: 'road',
    filter: ['>', ['coalesce', ['get', 'level'], 0], 0],
  })
  return raised.flatMap(({ geometry, properties }) => {
    const lines: number[][][] =
      geometry.type === 'LineString'
        ? [geometry.coordinates]
        : geometry.type === 'MultiLineString'
          ? geometry.coordinates
          : []
    const [width, level] = [Number(properties.width), Number(properties.level)]
    return lines.map((line) => ({ points: line.map(([lng, lat]) => fromLngLat(lng, lat)), width, level }))
  })
}

/**
 * Reads the raised roads again once tiles have loaded (at most every RAISED_EVERY_MS), and has the route
 * drawn again where they've changed.
 */
function syncRaised(target: LiveMap, now: number) {
  const { raised } = target
  if (!raised.stale || now - raised.at < RAISED_EVERY_MS) return
  const roads = raisedRoads(target.map)
  // What they are, cheaply: how many, and where they start.
  const read = `${roads.length}:${roads.reduce((sum, road) => sum + road.points[0][0] + road.points[0][1], 0)}`
  Object.assign(raised, { stale: false, at: now })
  if (read === raised.read) return
  Object.assign(raised, { roads, read })
  if (target.routeShown) Object.assign(target.routeShown, { restAfter: null, headFrom: null })
}

/**
 * Draws the game's GPS route (route.ts) as it changes, and the part ahead of the truck as it moves: the
 * rest of the route only when the truck reaches its next span, the head (to the end of the truck's span)
 * every half metre, so a parked truck leaves the map idle. Until the style has loaded, its sources aren't
 * there, and the next frame tries again.
 */
function syncRoute(target: LiveMap, now: number) {
  const { map, destination } = target
  const rest = map.getSource<GeoJSONSource>(ROUTE_SOURCE)
  const head = map.getSource<GeoJSONSource>(ROUTE_HEAD_SOURCE)
  target.progress = null
  if (!rest || !head) return
  const { route, routeNext } = useLive.getState()
  if (target.routeShown?.route !== route) {
    target.routeShown = {
      route,
      spans: route ? routeSpans(route.nodes, route.paths, route.shifts) : [],
      restAfter: null,
      headFrom: null,
    }
    const end = route?.nodes.at(-1)
    if (end) {
      destination.setLngLat(toLngLat(...end.position))
      if (!destination.getElement().isConnected) destination.addTo(map)
    } else {
      rest.setData(NO_LINE)
      head.setData(NO_LINE)
      destination.remove()
    }
  }
  syncRaised(target, now)
  const shown = target.routeShown
  const truck = truckAt(samples, now - FOLLOW_DELAY_MS)
  // The way the truck faces, on the ground (x east, z south), from its compass bearing.
  const facing = truck && along(bearingOf(truck.heading))
  const found = route && truck ? progressOf(shown.spans, routeNext, [truck.x, truck.z], facing ?? undefined) : null
  // Short of the route's start, the line runs from the truck to it, and the truck is shown where it is.
  const approach = found && truck && facing ? approachLine(shown.spans, found, [truck.x, truck.z], facing) : null
  target.progress = found && truck && approach ? { ...found, at: [truck.x, truck.z], distance: 0 } : found
  const progress = target.progress
  if (!progress) return
  if (progress.span !== shown.restAfter) {
    rest.setData(lineOf(restLine(shown.spans, progress.span), target.raised.roads))
    shown.restAfter = progress.span
    shown.headFrom = null
  }
  const moved = shown.headFrom
    ? Math.hypot(progress.at[0] - shown.headFrom[0], progress.at[1] - shown.headFrom[1])
    : Infinity
  if (moved >= ROUTE_HEAD_STEP) {
    const line = approach ? [...approach, ...shown.spans[0].slice(1)] : headLine(shown.spans, progress)
    head.setData(lineOf(line, target.raised.roads))
    shown.headFrom = progress.at
  }
}

function updateCamera(target: LiveMap, now: number, dt: number) {
  const truck = truckAt(samples, now - FOLLOW_DELAY_MS)
  const { map, puck } = target
  if (!truck) return

  // The puck goes with the truck whatever the camera does: on the route, in the middle of its road.
  const shown: Point = target.progress ? snapToRoute(target.progress, [truck.x, truck.z]) : [truck.x, truck.z]
  const position = toLngLat(...shown)
  const bearing = bearingOf(truck.heading)
  puck.setLngLat(position).setRotation(bearing)
  if (!puck.getElement().isConnected) puck.addTo(map)

  if (target.mode === 'browse') {
    if (!target.pressed && now - target.touched > AUTO_RECENTRE_MS) recentreLiveMap()
    return
  }

  const zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoomForSpeed(truck.speed) + target.zoomOffset))
  const height = map.getContainer().clientHeight
  const padding = { top: height * (PUCK_FROM_TOP - 0.5) * 2, bottom: 0, left: 0, right: 0 }

  if (target.mode === 'return' && target.glide) {
    // Towards where the following camera is now (the truck moves on while it glides).
    const t = (now - target.glide.started) / GLIDE_MS
    const pose = blendPose(target.glide.from, { center: position, zoom, bearing, pitch: PITCH }, easeInOut(t))
    moveCamera(map, pose, padding)
    if (t >= 1) {
      follow(target)
      target.camera = { bearing, zoom }
    }
    return
  }

  target.camera = target.camera
    ? {
        bearing: dampBearing(target.camera.bearing, bearing, dt, BEARING_TAU_MS),
        zoom: damp(target.camera.zoom, zoom, dt, ZOOM_TAU_MS),
      }
    : { bearing, zoom }
  // Held still while a button is down, so a drag can start (moving the camera cancels it).
  if (target.pressed) return
  moveCamera(map, { center: position, bearing: target.camera.bearing, zoom: target.camera.zoom, pitch: PITCH }, padding)
}

/**
 * Moves the map to `pose`, unless it's there already: every move draws the whole map again, so a parked
 * truck (and a camera that has settled) leaves the map, and the GPU the game shares, idle.
 */
function moveCamera(
  map: maplibregl.Map,
  pose: CameraPose,
  padding: { top: number; bottom: number; left: number; right: number },
) {
  const current = {
    center: map.getCenter().toArray() as [number, number],
    zoom: map.getZoom(),
    bearing: map.getBearing(),
    pitch: map.getPitch(),
  }
  if (samePose(current, pose) && Math.abs((map.getPadding().top ?? 0) - padding.top) < 0.01) return
  map.jumpTo({ ...pose, padding })
}

// Development: when this module is replaced (Vite hot reload), the old map goes with it, so a new one
// doesn't land on top.
import.meta.hot?.dispose(() => {
  if (frame !== null) cancelAnimationFrame(frame)
  live?.element.remove()
  live?.map.remove()
})
