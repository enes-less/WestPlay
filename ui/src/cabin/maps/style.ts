import type {
  DataDrivenPropertyValueSpecification,
  ExpressionSpecification,
  GeoJSONSourceSpecification,
  LayerSpecification,
  StyleSpecification,
} from 'maplibre-gl'
import type { Appearance } from '../state/appearance'
import type { MapData } from './mapData'
import { PLACE_ICON_PREFIX, PLACE_ICON_SIZE, SHADOW_ROOM } from './placeIcons'
import { pixelsPerMetre } from './projection'

/**
 * The map's look, in the spirit of Apple Maps: calm ground, white streets with faint casings, fast roads in
 * a heavier grey (Apple's own, sampled from its light and dark maps: neutral grey by day, lavender-grey by
 * night), green only where the game says green. No yellow: ETS2 players read yellow roads as explored ones.
 * Blue is kept for the route and the puck: the route (the game's GPS route, route.ts) over the roads.
 *
 * Data (vector tiles from crates/mapdata, area.rs), in five layers: `sea`, the whole area under
 * everything (the page draws the sea on beyond it, as the game's world lies in the sea: `beyond`); `land`
 * polygons over it (everything the sea doesn't cover); `water` polygons, painted by the game; `area` polygons with `color` (0 paved, 1 light, 2 dark, 3 green)
 * and `over`; `road` lines with `class` and `width` in metres. Labels are drawn by the page (MapLabels), in
 * WestPlay's own type.
 */

export const MAP_SOURCE = 'westplay-map'
/** The sea beyond the exported area, drawn by the page (no tiles needed for open sea all round). */
const BEYOND_SOURCE = 'westplay-beyond'
/** How far the sea beyond reaches, degrees: well past what the map shows from its widest zoom. */
const BEYOND_REACH = 20

/**
 * The sea beyond the area: a frame from `BEYOND_REACH` out to the area's `bounds`, overlapping the tiles'
 * own sea (which reaches half a grid cell further), so the two meet without a seam.
 */
function beyondArea([west, south, east, north]: [number, number, number, number]): GeoJSONSourceSpecification['data'] {
  const [w, s, e, n] = [west - BEYOND_REACH, south - BEYOND_REACH, east + BEYOND_REACH, north + BEYOND_REACH]
  return {
    type: 'Feature',
    properties: {},
    geometry: {
      type: 'Polygon',
      // The outer ring counter-clockwise and the hole clockwise (GeoJSON's rule, which MapLibre follows).
      coordinates: [
        [
          [w, s],
          [e, s],
          [e, n],
          [w, n],
          [w, s],
        ],
        [
          [west, south],
          [west, north],
          [east, north],
          [east, south],
          [west, south],
        ],
      ],
    },
  }
}

/** Road classes, quietest first: later ones draw on top. */
export const ROAD_CLASSES = ['rail', 'minor', 'local', 'trunk', 'motorway'] as const
export type RoadClass = (typeof ROAD_CLASSES)[number]

interface Palette {
  ground: string
  /** Sea, lakes and rivers: ground the game paints with water. */
  water: string
  /** Map area colours by the game's index. */
  areas: [string, string, string, string]
  roads: Record<RoadClass, { fill: string; casing: string }>
  /** A soft shadow under the roads, lifting them off the ground (white roads on pale ground vanish on the
   * truck's screen without it). */
  roadShadow: string
  /** The GPS route over the roads, and its edge. */
  route: { fill: string; casing: string }
}

const PALETTES: Record<Appearance, Palette> = {
  light: {
    // Darker than Apple's (#f3f1ec): the truck's screen, at arm's length and washed out by the cab's
    // light, needs the white roads to stand clear of the ground.
    ground: '#e6e2d9',
    // Apple's own close-up water (sampled from its map images).
    water: '#94cdf2',
    areas: ['#d6d1c6', '#dedad1', '#cec8bc', '#c6e0b3'],
    roads: {
      rail: { fill: '#aaa599', casing: '#aaa599' },
      minor: { fill: '#ffffff', casing: '#b3ada2' },
      local: { fill: '#ffffff', casing: '#a29c90' },
      trunk: { fill: '#bdbec2', casing: '#fbfbfc' },
      motorway: { fill: '#a8a9ad', casing: '#fbfbfc' },
    },
    roadShadow: 'rgb(50 42 25 / 0.42)',
    route: { fill: '#2f7cf6', casing: '#1559c9' },
  },
  dark: {
    ground: '#1f2226',
    water: '#1b2f55',
    areas: ['#2b2e33', '#272a2f', '#2f3338', '#1f3027'],
    roads: {
      rail: { fill: '#3b3f45', casing: '#3b3f45' },
      minor: { fill: '#3c4046', casing: '#2a2d32' },
      local: { fill: '#474b52', casing: '#2c2f34' },
      trunk: { fill: '#646a7e', casing: '#3a3e4a' },
      motorway: { fill: '#7a8098', casing: '#3f4352' },
    },
    roadShadow: 'rgb(0 0 0 / 0.35)',
    route: { fill: '#3d8bff', casing: '#1b4a96' },
  },
}

export function groundColor(appearance: Appearance): string {
  return PALETTES[appearance].ground
}

type DrawnClass = Exclude<RoadClass, 'rail'>

/**
 * Road widths on screen, in pixels at each zoom, for a typical road of each class: like Apple's, by what
 * the road is rather than its true size, so the network reads at a glance (MapLibre interpolates between
 * zooms, exponentially like the scale).
 */
const CLASS_WIDTHS: Record<DrawnClass, [zoom: number, pixels: number][]> = {
  motorway: [
    [10, 2],
    [13, 3.5],
    [15, 7],
    [16, 10],
    [17, 14],
    [18, 20],
    [20, 48],
  ],
  trunk: [
    [10, 1.5],
    [13, 3],
    [15, 6],
    [16, 8.5],
    [17, 12],
    [18, 17],
    [20, 40],
  ],
  local: [
    [10, 0.5],
    [13, 1.5],
    [15, 4.5],
    [16, 7],
    [17, 10],
    [18, 14],
    [20, 34],
  ],
  minor: [
    [10, 0.25],
    [13, 0.75],
    [15, 3],
    [16, 5],
    [17, 7.5],
    [18, 11],
    [20, 26],
  ],
}

/** The true width (m) of the typical road each class's widths are for (crates/mapdata: RoadClass::typical_width,
 * the width roads drawn far out are given). */
const TYPICAL_WIDTH: Record<DrawnClass, number> = { motorway: 16, trunk: 12, local: 8, minor: 6 }

/**
 * A road's width on screen: its class's width, made wider or narrower for its lanes (a one-lane slip
 * road is slimmer than the motorway it leaves, gently: by the square root, and junctions' many-lane
 * middles only a little wider), but never thinner than its
 * true width once zoomed right in. Plus `extra` pixels (the casing's edge).
 */
function roadWidth(
  roadClass: DrawnClass,
  extra: number | ((zoom: number) => number),
): DataDrivenPropertyValueSpecification<number> {
  const extraAt = typeof extra === 'number' ? () => extra : extra
  const lanes: ExpressionSpecification = [
    'min',
    1.2,
    ['max', 0.55, ['sqrt', ['/', ['get', 'width'], TYPICAL_WIDTH[roadClass]]]],
  ]
  const stops = CLASS_WIDTHS[roadClass].flatMap(([zoom, pixels]) => [
    zoom,
    [
      '+',
      ['max', ['*', pixels, lanes], ['*', ['get', 'width'], pixelsPerMetre(zoom)]],
      extraAt(zoom),
    ] as ExpressionSpecification,
  ])
  return ['interpolate', ['exponential', 2], ['zoom'], ...stops] as ExpressionSpecification
}

function areaLayers(palette: Palette, over: boolean): LayerSpecification[] {
  return palette.areas.map((color, index) => ({
    id: `area-${index}${over ? '-over' : ''}`,
    type: 'fill',
    source: MAP_SOURCE,
    'source-layer': 'area',
    filter: ['all', ['==', ['get', 'color'], index], ['==', ['get', 'over'], over]],
    paint: { 'fill-color': color, 'fill-antialias': true },
  }))
}

/**
 * The levels roads are drawn on, the ground first: a stretch of road over another (a bridge) is drawn a
 * level above it, shadow, edge and all (crates/mapdata area.rs road_levels: the tiles' `level`, none on the
 * ground).
 */
export const ROAD_LEVELS = 4

/** The roads: rail lines (under every road), and each level's roads (levelLayers), the ground's first. */
function roadLayers(palette: Palette): { rail: LayerSpecification; levels: LayerSpecification[][] } {
  const roads = ROAD_CLASSES.filter((c): c is DrawnClass => c !== 'rail')
  const rail: LayerSpecification = {
    id: 'road-rail',
    type: 'line',
    source: MAP_SOURCE,
    'source-layer': 'road',
    filter: ['==', ['get', 'class'], 'rail'],
    paint: { 'line-color': palette.roads.rail.fill, 'line-width': 1.5, 'line-dasharray': [3, 2] },
  }
  const levels = Array.from({ length: ROAD_LEVELS }, (_, level) => levelLayers(palette, roads, level))
  return { rail, levels }
}

/** One level's roads: shadows, then edges (casings), then fills, each by class, quietest first. */
function levelLayers(palette: Palette, roads: DrawnClass[], level: number): LayerSpecification[] {
  const filter = (roadClass: RoadClass): ExpressionSpecification => [
    'all',
    ['==', ['get', 'class'], roadClass],
    ['==', ['coalesce', ['get', 'level'], 0], level],
  ]
  const suffix = level === 0 ? '' : `-${level}`
  // A raised stretch goes on as the same road on the ground below (or the level below): its edges and
  // shadow end square where it does, under its round end, rather than ringing round across that road.
  const cap = level === 0 ? 'round' : 'butt'
  // A soft shadow under each road, a touch below it (screen down), like the lift on Apple's roads.
  const shadows: LayerSpecification[] = roads.map((roadClass) => ({
    id: `road-${roadClass}-shadow${suffix}`,
    type: 'line',
    source: MAP_SOURCE,
    'source-layer': 'road',
    filter: filter(roadClass),
    layout: { 'line-cap': cap, 'line-join': 'round' },
    paint: {
      'line-color': palette.roadShadow,
      // Barely there zoomed out (a whole network of shadows turns to smudge), full from street level.
      'line-width': roadWidth(roadClass, (zoom) => Math.min(7, Math.max(0, (zoom - 11) * 1.75))),
      'line-blur': ['interpolate', ['linear'], ['zoom'], 11, 0.5, 15, 5],
      'line-opacity': ['interpolate', ['linear'], ['zoom'], 9, 0, 12, 1],
      'line-translate': [0, 2],
      'line-translate-anchor': 'viewport',
    },
  }))
  // All of a level's casings below all its fills, so road pieces (and junction segments) join without seams.
  const casings: LayerSpecification[] = roads.map((roadClass) => ({
    id: `road-${roadClass}-casing${suffix}`,
    type: 'line',
    source: MAP_SOURCE,
    'source-layer': 'road',
    filter: filter(roadClass),
    layout: { 'line-cap': cap, 'line-join': 'round' },
    paint: { 'line-color': palette.roads[roadClass].casing, 'line-width': roadWidth(roadClass, 2) },
  }))
  const fills: LayerSpecification[] = roads.map((roadClass) => ({
    id: `road-${roadClass}${suffix}`,
    type: 'line',
    source: MAP_SOURCE,
    'source-layer': 'road',
    filter: filter(roadClass),
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: { 'line-color': palette.roads[roadClass].fill, 'line-width': roadWidth(roadClass, 0) },
  }))
  return [...shadows, ...casings, ...fills]
}

/**
 * Places (placeIcons.ts), from street level, companies a step closer (there are many), ferry ports, train
 * terminals and border crossings from much further out (few, and routes run through them): upright to the screen whatever
 * the tilt, the badge on the place and its name beside it, none over another (ports and terminals win, then
 * garages, dealers, services, recruitment agencies, hotels, border crossings and toll gates, weigh
 * stations, companies, fuel, resting places), their size
 * easing in as the map comes closer. Where a name has no room, the place keeps its badge, as on Apple's
 * map: the named places are placed first (the upper layer), then the bare badges of the rest where they
 * fit (a place already shown named has its own badge in the way).
 */
function placeLayers(): LayerSpecification[] {
  return [placeLayer('places-badges', placeImage([], false)), placeLayer('places', placeImage([], true))]
}

/**
 * A place's image name (placeIcons.ts placeIconName), named or bare: a garage in one of `ownedGarages`'s
 * cities is the player's, any other is for sale (its tiles' `sale_name`: what one for sale is called).
 */
function placeImage(ownedGarages: string[], named: boolean): ExpressionSpecification {
  const owned: ExpressionSpecification = [
    'all',
    ['==', ['get', 'kind'], 'garage'],
    ['in', ['coalesce', ['get', 'city'], ''], ['literal', ownedGarages]],
  ]
  const forSale: ExpressionSpecification = ['all', ['==', ['get', 'kind'], 'garage'], ['!', owned]]
  const kind: ExpressionSpecification = ['case', forSale, 'garage-sale', ['get', 'kind']]
  const name: ExpressionSpecification = ['case', forSale, ['get', 'sale_name'], ['get', 'name']]
  return ['concat', PLACE_ICON_PREFIX, kind, named ? ['case', ['to-boolean', name], ['concat', '|', name], ''] : '']
}

/** The place layers' images for the garages the player owns, to apply to a live map as they change. */
export function placeImagesFor(ownedGarages: string[]): { layer: string; value: ExpressionSpecification }[] {
  return [
    { layer: 'places-badges', value: placeImage(ownedGarages, false) },
    { layer: 'places', value: placeImage(ownedGarages, true) },
  ]
}

/** A layer of place icons: `image`, their images' names (placeIcons.ts placeIconName). */
function placeLayer(id: string, image: ExpressionSpecification): LayerSpecification {
  return {
    id,
    type: 'symbol',
    source: MAP_SOURCE,
    'source-layer': 'place',
    minzoom: TRANSIT_MIN_ZOOM,
    filter: [
      'match',
      ['get', 'kind'],
      ['ferry', 'train', 'border'],
      true,
      'company',
      ['>=', ['zoom'], COMPANY_MIN_ZOOM],
      ['>=', ['zoom'], PLACE_MIN_ZOOM],
    ],
    layout: {
      'icon-image': image,
      // The badge on the place, its name to the right.
      'icon-anchor': 'left',
      'icon-offset': [-(SHADOW_ROOM + PLACE_ICON_SIZE / 2), 0],
      'icon-size': ['interpolate', ['linear'], ['zoom'], TRANSIT_MIN_ZOOM, 0.7, PLACE_MIN_ZOOM, 0.8, 15, 1],
      'icon-padding': 6,
      'icon-pitch-alignment': 'viewport',
      'icon-rotation-alignment': 'viewport',
      'symbol-sort-key': [
        'match',
        ['get', 'kind'],
        ['ferry', 'train'],
        0,
        'garage',
        1,
        'dealer',
        2,
        'service',
        3,
        ['recruitment', 'hotel', 'border', 'toll'],
        4,
        'weigh',
        5,
        'company',
        6,
        'parking',
        8,
        7,
      ],
    },
  }
}

/** The GPS route (route.ts): the part of it on the span the truck is on (`head`, redrawn as the truck
 * moves) and the rest, as GeoJSON lines the live map fills in. */
export const ROUTE_SOURCE = 'westplay-route'
export const ROUTE_HEAD_SOURCE = 'westplay-route-head'

/** The route's width on screen, pixels at each zoom: a little wider than a motorway, so it reads at a
 * glance over whatever road it follows. */
const ROUTE_WIDTHS: [zoom: number, pixels: number][] = [
  [9, 4],
  [13, 6],
  [15, 9],
  [16, 12],
  [17, 16],
  [18, 22],
  [20, 50],
]

/**
 * The route over the roads in Apple's blue, with a darker edge, under the road numbers and the places: the
 * part of it on each level (route.ts routeLevels: its sources' `level`, none on the ground) over that
 * level's roads, so it passes under a bridge over the road it's on, and over the road beneath when it's on
 * the bridge. Both edges below both fills, so the head and the rest join without a seam.
 */
function routeLayers(palette: Palette, level: number): LayerSpecification[] {
  const width = (extra: number): ExpressionSpecification =>
    [
      'interpolate',
      ['exponential', 2],
      ['zoom'],
      ...ROUTE_WIDTHS.flatMap(([zoom, pixels]) => [zoom, pixels + extra]),
    ] as ExpressionSpecification
  // On a raised level, its edges end square where it comes down to the level below, under its round end
  // (as the roads': levelLayers).
  const edgeCap = level === 0 ? 'round' : 'butt'
  const layer = (id: string, source: string, color: string, extra: number): LayerSpecification => ({
    id: level === 0 ? id : `${id}-${level}`,
    type: 'line',
    source,
    filter: ['==', ['coalesce', ['get', 'level'], 0], level],
    layout: { 'line-cap': extra > 0 ? edgeCap : 'round', 'line-join': 'round' },
    paint: { 'line-color': color, 'line-width': width(extra) },
  })
  return [
    layer('route-casing', ROUTE_SOURCE, palette.route.casing, 4),
    layer('route-head-casing', ROUTE_HEAD_SOURCE, palette.route.casing, 4),
    layer('route', ROUTE_SOURCE, palette.route.fill, 0),
    layer('route-head', ROUTE_HEAD_SOURCE, palette.route.fill, 0),
  ]
}

/** No route yet. */
const NO_LINE: GeoJSONSourceSpecification['data'] = { type: 'FeatureCollection', features: [] }

/** The sprite of road number signs, and the zoom they show from (the tiles have them a step earlier,
 * crates/mapdata area.rs SIGN_ZOOM). */
const SIGN_SPRITE = 'signs'
const SIGN_MIN_ZOOM = 10

/**
 * Road numbers where the game shows them, as the game's own signs (each country's own: Germany's blue A,
 * France's red, the green E roads): upright to the screen whatever the tilt, growing a little as the map
 * comes closer, under the places (which win where they'd meet) and never over one another.
 */
function signLayer(): LayerSpecification {
  return {
    id: 'road-signs',
    type: 'symbol',
    source: MAP_SOURCE,
    'source-layer': 'sign',
    minzoom: SIGN_MIN_ZOOM,
    layout: {
      'icon-image': ['concat', `${SIGN_SPRITE}:`, ['get', 'sign']],
      'icon-size': ['interpolate', ['linear'], ['zoom'], SIGN_MIN_ZOOM, 0.9, 15, 1.25],
      'icon-padding': 3,
      'icon-pitch-alignment': 'viewport',
      'icon-rotation-alignment': 'viewport',
    },
  }
}

/** The zoom places show from (the tiles have them a little earlier, crates/mapdata area.rs PLACE_ZOOM and
 * TRANSIT_ZOOM). */
const PLACE_MIN_ZOOM = 12
const COMPANY_MIN_ZOOM = 13
const TRANSIT_MIN_ZOOM = 10

/** The map's style, drawing `data`'s tiles (`null`: no source, for reading paint properties alone). */
export function mapStyle(appearance: Appearance, data: MapData | null): StyleSpecification {
  const palette = PALETTES[appearance]
  const roads = roadLayers(palette)
  const [ground, ...raised] = roads.levels
  return {
    version: 8,
    transition: TRANSITION,
    // The game's own road number signs (crates/mapdata signs.rs), as `signs:<name>` images.
    ...(data?.signs ? { sprite: [{ id: SIGN_SPRITE, url: data.signs }] } : {}),
    sources: data
      ? {
          [MAP_SOURCE]: {
            type: 'vector',
            tiles: [data.tiles],
            minzoom: data.minzoom,
            maxzoom: data.maxzoom,
            bounds: data.bounds,
          },
          [BEYOND_SOURCE]: { type: 'geojson', data: beyondArea(data.bounds) },
          [ROUTE_SOURCE]: { type: 'geojson', data: NO_LINE },
          [ROUTE_HEAD_SOURCE]: { type: 'geojson', data: NO_LINE },
        }
      : {},
    layers: [
      // Plain ground where nothing has loaded yet (not sea: a tile still on its way isn't open water).
      { id: 'ground', type: 'background', paint: { 'background-color': palette.ground } },
      { id: 'beyond', type: 'fill', source: BEYOND_SOURCE, paint: { 'fill-color': palette.water } },
      {
        id: 'sea',
        type: 'fill',
        source: MAP_SOURCE,
        'source-layer': 'sea',
        paint: { 'fill-color': palette.water, 'fill-antialias': false },
      },
      {
        id: 'land',
        type: 'fill',
        source: MAP_SOURCE,
        'source-layer': 'land',
        paint: { 'fill-color': palette.ground, 'fill-antialias': true },
      },
      {
        id: 'water',
        type: 'fill',
        source: MAP_SOURCE,
        'source-layer': 'water',
        paint: { 'fill-color': palette.water, 'fill-antialias': true },
      },
      ...areaLayers(palette, false),
      roads.rail,
      ...ground,
      ...areaLayers(palette, true),
      ...routeLayers(palette, 0),
      // Each raised level's roads over everything on the levels below, and the route on them over those.
      ...raised.flatMap((level, index) => [...level, ...routeLayers(palette, index + 1)]),
      signLayer(),
      ...placeLayers(),
    ],
  }
}

/** Every paint property of the style for `appearance`, to restyle a live map (at once, see TRANSITION). */
export function paintFor(appearance: Appearance): { layer: string; property: string; value: unknown }[] {
  return mapStyle(appearance, null).layers.flatMap((layer) =>
    Object.entries(layer.paint ?? {}).map(([property, value]) => ({ layer: layer.id, property, value })),
  )
}

/** Paint changes (light ↔ dark) are instant: the whole screen crossfades as the appearance changes, the map
 * with it (state/launch.ts). */
export const TRANSITION = { duration: 0, delay: 0 }
