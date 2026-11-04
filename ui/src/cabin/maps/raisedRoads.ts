import type maplibregl from 'maplibre-gl'
import type { CustomLayerInterface, CustomRenderMethodInput } from 'maplibre-gl'
import type { Appearance } from '../state/appearance'
import { METRES_PER_DEGREE } from './projection'
import type { RaisedLine, SurfaceAt } from './route'
import {
  OVER_ROUTE_LAYER,
  ROUTE_LOWEST_LAYER,
  SURFACE_SOURCE,
  raisedColours,
  routeColours,
  routeWidth,
} from './style'

/**
 * The roads at their real heights, where one passes over another: MapLibre can't draw these (a line has no height, a
 * fill-extrusion one height a feature, terrain one height a point, so nothing can pass under anything), so this layer
 * draws them itself from the map's own files (crates/mapdata surface/raised.rs), a tile at a time. Once it has a file, the
 * flat map's copy of what's in it goes (`flatCopy`), so a road isn't drawn twice, and till then the road is there. Over
 * water it paints the water back under each raised road. Then the shadows, then the roads and their sides, lowest first:
 * nothing writes depth, so what the map draws after this (the route, traffic) isn't cut by it. A second layer,
 * over the route's own, draws the stretches of the route that are on raised roads (route.ts cuts them out of its line).
 */

export const RAISED_LAYER = 'raised-roads'
export const RAISED_ROUTE_LAYER = 'raised-route'
/** From here in the surfaces are the map's roads, and raised ones start to rise, up to their whole height at the second. */
const RISE_ZOOMS = [15, 15.5] as const
const FILE_ZOOM = 14
const TILES = 2 ** FILE_ZOOM
/** How far past their tile a file's vertices may lie, of the tile, and how a vertex is laid out (raised.rs). */
const PAST = 0.0625
const VERTEX_BYTES = 10
const VERTEX_WORDS = VERTEX_BYTES / 2
const BUCKETS = 32
const HEADER_BYTES = 8 + 4 * BUCKETS
/** Tiles further from the middle of the view than this, in tiles, aren't loaded: past that the map is too far to see them. */
const TILES_REACH = 2
const TILES_KEPT = 40
const LOADING_AT_ONCE = 4
const LOOK_EVERY_MS = 300
/** A tile's surfaces are found by cells: how many a side, and how many of a vertex's units one is. */
const CELLS = 64
const CELL_UNITS = 1024
/** The route's edge, pixels, and how far above the road it lies, m (the line isn't cut by anything). */
const ROUTE_CASING = 4
const ROUTE_ABOVE = 0.3
/** One metre in MapLibre's world (x and y 0 to 1), and how the ground's shadows fall from what casts them, m/m. */
const METRE = 1 / 40075016.686
const SHADOW = [0.7, 0.45] as const
const SHADOW_ALPHA = 0.27

/** How much lighter a raised road is at this height, m, so that one over another shows. */
const LIGHTER = [0.22, 30] as const

/** How high a road's side is showing in full, from nothing at the first, m: a road barely off the map has no side to see. */
const SIDE_FROM = [0.5, 3] as const

const WATER = 0
const SHADOWS = 1
const SURFACE = 2

const VERTEX_SHADER = `#version 300 es
uniform mat4 u_matrix;
uniform float u_rise;
uniform float u_pass;
uniform vec2 u_shadow;
uniform vec3 u_kinds[5];
uniform vec3 u_side;
uniform vec3 u_water;
uniform float u_past;
uniform vec2 u_towards;
in vec2 a_at;
in float a_high;
in vec2 a_out;
in float a_kind;
in float a_flags;
out vec3 v_colour;
out vec3 v_place;
out float v_lit;
const float PAST = ${PAST};
const float METRE = ${METRE};
const float TILES = ${TILES}.0;
const vec4 NOWHERE = vec4(2.0, 2.0, 2.0, 1.0);
void main() {
  int flags = int(a_flags);
  bool side = (flags & 1) != 0;
  bool bottom = (flags & 2) != 0;
  bool water = (flags & 8) != 0;
  float high = a_high * 0.01 * u_rise;
  vec2 at = a_at * (1.0 + 2.0 * PAST) - PAST;
  v_place = vec3(at / (METRE * TILES), high);
  v_colour = u_kinds[int(a_kind)];
  if (side) v_colour = mix(v_colour, u_side, smoothstep(${SIDE_FROM[0]}, ${SIDE_FROM[1]}.0, high));
  v_lit = side ? 0.0 : 1.0;
  gl_Position = NOWHERE;
  if (u_pass < 0.5) {
    if (!water) return;
    if (bottom) at += a_out * u_past;
    high = 0.0;
    v_colour = u_water;
  } else if (u_pass < 1.5) {
    if (side) return;
    at += u_shadow * high * METRE * TILES;
    high = 0.0;
  } else if (side && dot(a_out, u_towards) < 0.0) {
    // A side turned from the camera is behind its road: on a slope, its tip would show through at each join.
    return;
  }
  gl_Position = u_matrix * vec4(at, high * METRE, 1.0);
}`

const FRAGMENT_SHADER = `#version 300 es
precision highp float;
uniform float u_pass;
in vec3 v_colour;
in vec3 v_place;
in float v_lit;
out vec4 colour;
void main() {
  if (u_pass < 0.5) {
    colour = vec4(v_colour, 1.0);
  } else if (u_pass < 1.5) {
    colour = vec4(0.0, 0.0, 0.0, ${SHADOW_ALPHA});
  } else {
    vec3 up = normalize(cross(dFdx(v_place), dFdy(v_place)));
    float tilted = 1.0 + min((1.0 - abs(up.z)) * 3.0, 0.12);
    float higher = 1.0 + ${LIGHTER[0]} * smoothstep(0.0, ${LIGHTER[1]}.0, v_place.z);
    colour = vec4(v_colour * mix(1.0, tilted * higher, v_lit), 1.0);
  }
}`

const ROUTE_VERTEX_SHADER = `#version 300 es
uniform mat4 u_matrix;
uniform float u_rise;
uniform float u_half;
in vec2 a_at;
in float a_high;
in vec2 a_along;
in float a_side;
void main() {
  vec2 at = a_at + vec2(-a_along.y, a_along.x) * a_side * u_half;
  gl_Position = u_matrix * vec4(at, (a_high * u_rise + ${ROUTE_ABOVE}) * ${METRE}, 1.0);
}`

const ROUTE_FRAGMENT_SHADER = `#version 300 es
precision highp float;
uniform vec3 u_colour;
out vec4 colour;
void main() {
  colour = vec4(u_colour, 1.0);
}`

type Key = `${number}/${number}`

/** A tile's file, as far as it's got. */
interface Tile {
  x: number
  y: number
  /** The height the vertices' heights in the game are from. */
  base: number
  /** The vertex each bucket ends at. */
  ends: Uint32Array
  vertices: Uint16Array
  /** Which of its road surfaces are in which cell, once asked. */
  cells: { starts: Uint32Array; surfaces: Uint32Array } | null
  array: WebGLVertexArrayObject | null
  buffer: WebGLBuffer | null
}

export interface RaisedRoads {
  /** Keeps the tiles round the view loaded. Called every frame; looks now and then. */
  update(now: number): void
  surfaceAt: SurfaceAt
  /** Changes as tiles come and go: what `surfaceAt` says then may be different. */
  tilesChanged(): number
  /** Shows the roads at their real heights, or the flat map's alone. */
  setShown(shown: boolean): void
  /** The stretches of the route on raised roads, to draw. */
  showRoute(lines: readonly RaisedLine[]): void
  /** How far on the screen something `height` over the ground at (x, z) is from the ground there, CSS pixels. */
  screenLift(x: number, z: number, height: number): [number, number]
}

/** Where (x, z) on the game's ground is in MapLibre's world (0 to 1 each way). */
function worldOf(x: number, z: number): [number, number] {
  const latitude = (-z / METRES_PER_DEGREE) * (Math.PI / 180)
  return [(x / METRES_PER_DEGREE + 180) / 360, 0.5 - Math.log(Math.tan(Math.PI / 4 + latitude / 2)) / (2 * Math.PI)]
}

/** `main` for what's given in tile widths from (x, y), which is in tile widths too. */
function matrixFrom(main: ArrayLike<number>, x: number, y: number): Float32Array {
  const matrix = new Float32Array(16)
  for (let i = 0; i < 4; i++) {
    matrix[i] = main[i] / TILES
    matrix[4 + i] = main[4 + i] / TILES
    matrix[8 + i] = main[8 + i]
    matrix[12 + i] = (main[i] * x + main[4 + i] * y) / TILES + main[12 + i]
  }
  return matrix
}

function linked(gl: WebGLRenderingContext | WebGL2RenderingContext, vertex: string, fragment: string): WebGLProgram {
  const program = gl.createProgram()
  for (const [kind, text] of [
    [gl.VERTEX_SHADER, vertex],
    [gl.FRAGMENT_SHADER, fragment],
  ] as const) {
    const shader = gl.createShader(kind)!
    gl.shaderSource(shader, text)
    gl.compileShader(shader)
    gl.attachShader(program, shader)
  }
  gl.linkProgram(program)
  return program
}

function locations(gl: WebGLRenderingContext | WebGL2RenderingContext, program: WebGLProgram, names: string[]) {
  return Object.fromEntries(names.map((name) => [name, gl.getUniformLocation(program, name)]))
}

/** `gl` set to draw over the map: reading the depth the buildings left, leaving it as it is. */
function overTheMap(gl: WebGL2RenderingContext) {
  gl.disable(gl.CULL_FACE)
  gl.disable(gl.STENCIL_TEST)
  gl.enable(gl.BLEND)
  gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA)
  gl.enable(gl.DEPTH_TEST)
  gl.depthFunc(gl.LEQUAL)
  gl.depthMask(false)
}

/** The surfaces of a tile by cell. */
function cellsOf(tile: Tile): NonNullable<Tile['cells']> {
  const { vertices } = tile
  const triangles = tile.ends[BUCKETS - 1] / 3
  const starts = new Uint32Array(CELLS * CELLS + 1)
  const inCells = (visit: (cell: number, triangle: number) => void) => {
    for (let triangle = 0; triangle < triangles; triangle++) {
      const at = triangle * 3 * VERTEX_WORDS
      // Their sides, and what's under them: the height to find is the one of the road on top.
      if ((vertices[at + 4] >> 8) & 7) continue
      const [a, b, c] = [at, at + VERTEX_WORDS, at + 2 * VERTEX_WORDS]
      const low = (i: number) => Math.floor(Math.min(vertices[a + i], vertices[b + i], vertices[c + i]) / CELL_UNITS)
      const high = (i: number) => Math.floor(Math.max(vertices[a + i], vertices[b + i], vertices[c + i]) / CELL_UNITS)
      for (let y = low(1); y <= high(1); y++) {
        for (let x = low(0); x <= high(0); x++) visit(x + y * CELLS, triangle)
      }
    }
  }
  inCells((cell) => starts[cell + 1]++)
  for (let cell = 0; cell < CELLS * CELLS; cell++) starts[cell + 1] += starts[cell]
  const filled = starts.slice()
  const surfaces = new Uint32Array(starts[CELLS * CELLS])
  inCells((cell, triangle) => {
    surfaces[filled[cell]++] = triangle
  })
  return { starts, surfaces }
}

/** The raised roads of the map whose files are at `address`, painted in `appearance()`. */
export function raisedRoads(map: maplibregl.Map, address: string, appearance: () => Appearance): RaisedRoads {
  const tiles = new Map<Key, Tile>()
  const loading = new Set<Key>()
  let known: Set<Key> | null = null
  let looked = -Infinity
  let context: WebGL2RenderingContext | null = null
  let program: WebGLProgram | null = null
  let uniforms: Record<string, WebGLUniformLocation | null> = {}
  let attributes: Record<string, number> = {}
  let changed = 0
  let showing = true
  let main: ArrayLike<number> | null = null
  let route: {
    program: WebGLProgram
    uniforms: Record<string, WebGLUniformLocation | null>
    array: WebGLVertexArrayObject
    buffer: WebGLBuffer
  } | null = null
  let routeLines: readonly RaisedLine[] = []
  let routeStale = false
  /** Where the stretches of the route are in its buffer, and the place its points are from, in tile widths. */
  let routeRuns: { first: number; count: number }[] = []
  let routeOrigin: [number, number] = [0, 0]

  void fetch(`${address}/index.json`)
    .then((response) => (response.ok ? response.json() : []))
    .then((list: [number, number][]) => {
      known = new Set(list.map(([x, y]) => `${x}/${y}` as Key))
      looked = -Infinity
    })
    .catch(() => {
      known = new Set()
    })

  function load(key: Key, x: number, y: number) {
    loading.add(key)
    void fetch(`${address}/${x}/${y}.bin`)
      .then((response) =>
        response.ok && response.body
          ? new Response(response.body.pipeThrough(new DecompressionStream('gzip'))).arrayBuffer()
          : null,
      )
      .then((bytes) => {
        // Anything but what the file says it is, and nothing's drawn from it: wrong counts could lose the GPU's context.
        const whole =
          bytes &&
          bytes.byteLength >= HEADER_BYTES &&
          new DataView(bytes).getUint32(0, true) * VERTEX_BYTES === bytes.byteLength - HEADER_BYTES
        if (!whole) {
          known?.delete(key)
          return
        }
        const ends = new Uint32Array(bytes.slice(8, HEADER_BYTES))
        const base = new DataView(bytes).getFloat32(4, true)
        const vertices = new Uint16Array(bytes, HEADER_BYTES)
        tiles.set(key, { x, y, base, ends, vertices, cells: null, array: null, buffer: null })
        flatCopy(x, y, showing)
        changed++
        map.triggerRepaint()
      })
      .catch(() => known?.delete(key))
      .finally(() => loading.delete(key))
  }

  /** Takes off the flat map what a file draws at its height, or gives it back. What passes under it isn't the file's. */
  function flatCopy(x: number, y: number, raised: boolean) {
    if (!map.getSource(SURFACE_SOURCE)) return
    const id = x * TILES + y + 1
    for (const sourceLayer of ['surface', 'surface_edge']) {
      map.setFeatureState({ source: SURFACE_SOURCE, sourceLayer, id }, { raised })
    }
  }

  function drop(key: Key, tile: Tile) {
    if (context && tile.buffer) context.deleteBuffer(tile.buffer)
    if (context && tile.array) context.deleteVertexArray(tile.array)
    tiles.delete(key)
    flatCopy(tile.x, tile.y, false)
    changed++
  }

  const surfaceAt: SurfaceAt = (x, z, near, reach) => {
    if (!showing) return null
    const world = worldOf(x, z)
    let found: { drawn: number; game: number } | null = null
    let off = reach
    for (const tile of tiles.values()) {
      const u = ((world[0] * TILES - tile.x + PAST) / (1 + 2 * PAST)) * 65535
      const v = ((world[1] * TILES - tile.y + PAST) / (1 + 2 * PAST)) * 65535
      if (u < 0 || v < 0 || u >= 65535 || v >= 65535) continue
      tile.cells ??= cellsOf(tile)
      const { starts, surfaces } = tile.cells
      const { vertices } = tile
      const cell = Math.floor(u / CELL_UNITS) + Math.floor(v / CELL_UNITS) * CELLS
      for (let i = starts[cell]; i < starts[cell + 1]; i++) {
        const a = surfaces[i] * 3 * VERTEX_WORDS
        const [b, c] = [a + VERTEX_WORDS, a + 2 * VERTEX_WORDS]
        const [ax, ay, bx, by, cx, cy] = [
          vertices[a],
          vertices[a + 1],
          vertices[b],
          vertices[b + 1],
          vertices[c],
          vertices[c + 1],
        ]
        const across = (bx - ax) * (cy - ay) - (cx - ax) * (by - ay)
        if (across === 0) continue
        const onB = ((u - ax) * (cy - ay) - (cx - ax) * (v - ay)) / across
        const onC = ((bx - ax) * (v - ay) - (u - ax) * (by - ay)) / across
        const onA = 1 - onB - onC
        if (onA < 0 || onB < 0 || onC < 0) continue
        const signed = (word: number) => (word << 16) >> 16
        const game =
          tile.base +
          (signed(vertices[a + 3]) * onA + signed(vertices[b + 3]) * onB + signed(vertices[c + 3]) * onC) / 10
        if (Math.abs(game - near) > off) continue
        off = Math.abs(game - near)
        found = { drawn: (vertices[a + 2] * onA + vertices[b + 2] * onB + vertices[c + 2] * onC) / 100, game }
      }
    }
    return found
  }

  /** The tiles loaded that the view can see: past its corners there's no need to draw them. */
  function inView(): Tile[] {
    const bounds = map.getBounds()
    const [west, north] = worldOf(bounds.getWest() * METRES_PER_DEGREE, -bounds.getNorth() * METRES_PER_DEGREE)
    const [east, south] = worldOf(bounds.getEast() * METRES_PER_DEGREE, -bounds.getSouth() * METRES_PER_DEGREE)
    const shown = []
    for (const tile of tiles.values()) {
      const [x, y] = [tile.x / TILES, tile.y / TILES]
      const side = (1 + PAST) / TILES
      if (x + side >= west && x - PAST / TILES <= east && y + side >= north && y - PAST / TILES <= south)
        shown.push(tile)
    }
    return shown
  }

  /** How much of its height a raised road has at this zoom. */
  function riseAt(zoom: number): number {
    return Math.min(Math.max((zoom - RISE_ZOOMS[0]) / (RISE_ZOOMS[1] - RISE_ZOOMS[0]), 0), 1)
  }

  function screenLift(x: number, z: number, height: number): [number, number] {
    if (!main || height <= 0) return [0, 0]
    const m = main
    const [wx, wy] = worldOf(x, z)
    const screen = (up: number) => {
      const w = m[3] * wx + m[7] * wy + m[11] * up + m[15]
      return [(m[0] * wx + m[4] * wy + m[8] * up + m[12]) / w, (m[1] * wx + m[5] * wy + m[9] * up + m[13]) / w]
    }
    const [ground, raised] = [screen(0), screen(height * riseAt(map.getZoom()) * METRE)]
    const { clientWidth, clientHeight } = map.getCanvas()
    return [((raised[0] - ground[0]) / 2) * clientWidth, (-(raised[1] - ground[1]) / 2) * clientHeight]
  }

  function showRoute(lines: readonly RaisedLine[]) {
    if (lines.length === 0 && routeLines.length === 0) return
    routeLines = lines
    routeStale = true
    map.triggerRepaint()
  }

  /** The route's stretches into its buffer: two vertices a point, x, y, height, the way it goes (x, y) and which side. */
  function uploadRoute(gl: WebGL2RenderingContext, buffer: WebGLBuffer) {
    routeStale = false
    routeRuns = []
    const lines = routeLines.filter((line) => line.length > 1)
    if (lines.length === 0) return
    const origin = worldOf(lines[0][0][0], lines[0][0][1]).map((part) => part * TILES)
    routeOrigin = [origin[0], origin[1]]
    const data = new Float32Array(lines.reduce((sum, line) => sum + line.length, 0) * 2 * 6)
    let vertex = 0
    for (const line of lines) {
      routeRuns.push({ first: vertex, count: line.length * 2 })
      const placed = line.map(([x, z]) => worldOf(x, z).map((part, i) => part * TILES - origin[i]))
      line.forEach(([, , height], index) => {
        const [before, after] = [placed[Math.max(index - 1, 0)], placed[Math.min(index + 1, line.length - 1)]]
        const [wayX, wayY] = [after[0] - before[0], after[1] - before[1]]
        const length = Math.hypot(wayX, wayY) || 1
        for (const side of [-1, 1]) {
          data.set([placed[index][0], placed[index][1], height, wayX / length, wayY / length, side], vertex * 6)
          vertex++
        }
      })
    }
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer)
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW)
  }

  function update(now: number) {
    if (now - looked < LOOK_EVERY_MS || !known || !showing || map.getZoom() < RISE_ZOOMS[0]) return
    looked = now
    const { lng, lat } = map.getCenter()
    const middle = [
      ((lng + 180) / 360) * TILES,
      (0.5 - Math.log(Math.tan(Math.PI / 4 + (lat * Math.PI) / 360)) / (2 * Math.PI)) * TILES,
    ]
    const away = (x: number, y: number) =>
      Math.max(x - middle[0], middle[0] - x - 1, 0) ** 2 + Math.max(y - middle[1], middle[1] - y - 1, 0) ** 2
    const missing: [number, Key, number, number][] = []
    for (let x = Math.floor(middle[0] - TILES_REACH); x <= middle[0] + TILES_REACH; x++) {
      for (let y = Math.floor(middle[1] - TILES_REACH); y <= middle[1] + TILES_REACH; y++) {
        const key: Key = `${x}/${y}`
        if (known.has(key) && !tiles.has(key) && !loading.has(key) && away(x, y) <= TILES_REACH ** 2) {
          missing.push([away(x, y), key, x, y])
        }
      }
    }
    // The nearest first.
    missing.sort((a, b) => a[0] - b[0])
    for (const [, key, x, y] of missing.slice(0, Math.max(LOADING_AT_ONCE - loading.size, 0))) load(key, x, y)
    if (tiles.size > TILES_KEPT) {
      for (const [key, tile] of tiles) {
        if (away(tile.x, tile.y) > TILES_REACH ** 2) drop(key, tile)
      }
    }
  }

  function upload(gl: WebGL2RenderingContext, tile: Tile) {
    tile.array = gl.createVertexArray()
    tile.buffer = gl.createBuffer()
    gl.bindVertexArray(tile.array)
    gl.bindBuffer(gl.ARRAY_BUFFER, tile.buffer)
    gl.bufferData(gl.ARRAY_BUFFER, tile.vertices, gl.STATIC_DRAW)
    const attribute = (name: string, size: number, type: number, normalized: boolean, offset: number) => {
      gl.enableVertexAttribArray(attributes[name])
      gl.vertexAttribPointer(attributes[name], size, type, normalized, VERTEX_BYTES, offset)
    }
    attribute('a_at', 2, gl.UNSIGNED_SHORT, true, 0)
    attribute('a_high', 1, gl.UNSIGNED_SHORT, false, 4)
    attribute('a_out', 2, gl.BYTE, true, 6)
    attribute('a_kind', 1, gl.UNSIGNED_BYTE, false, 8)
    attribute('a_flags', 1, gl.UNSIGNED_BYTE, false, 9)
  }

  const layer: CustomLayerInterface = {
    id: RAISED_LAYER,
    type: 'custom',
    renderingMode: '3d',
    onAdd(_, gl) {
      context = gl as WebGL2RenderingContext
      // A new context, if the last was lost: nothing made in it is there any more.
      for (const tile of tiles.values()) {
        tile.array = null
        tile.buffer = null
      }
      program = linked(gl, VERTEX_SHADER, FRAGMENT_SHADER)
      uniforms = locations(gl, program, [
        'u_matrix',
        'u_rise',
        'u_pass',
        'u_shadow',
        'u_kinds',
        'u_side',
        'u_water',
        'u_past',
        'u_towards',
      ])
      for (const name of ['a_at', 'a_high', 'a_out', 'a_kind', 'a_flags'])
        attributes[name] = gl.getAttribLocation(program, name)
    },
    render(raw, args: CustomRenderMethodInput) {
      const gl = raw as WebGL2RenderingContext
      const zoom = map.getZoom()
      main = args.defaultProjectionData.mainMatrix
      if (zoom < RISE_ZOOMS[0] || !program || tiles.size === 0) return
      const colours = raisedColours(appearance(), zoom)
      const shown = inView()
      gl.useProgram(program)
      gl.uniform1f(uniforms.u_rise, riseAt(zoom))
      gl.uniform2f(uniforms.u_shadow, SHADOW[0], SHADOW[1])
      gl.uniform3fv(uniforms.u_kinds, colours.kinds.flat())
      gl.uniform3fv(uniforms.u_side, colours.side)
      gl.uniform3fv(uniforms.u_water, colours.water)
      const bearing = (map.getBearing() * Math.PI) / 180
      gl.uniform2f(uniforms.u_towards, -Math.sin(bearing), Math.cos(bearing))
      gl.uniform1f(uniforms.u_past, (colours.past * TILES) / (512 * 2 ** zoom))
      overTheMap(gl)
      for (const tile of shown) {
        if (!tile.array) upload(gl, tile)
      }
      const within = (tile: Tile) => {
        gl.uniformMatrix4fv(uniforms.u_matrix, false, matrixFrom(args.defaultProjectionData.mainMatrix, tile.x, tile.y))
        gl.bindVertexArray(tile.array)
      }
      // The water back under them, and the shadows.
      for (const pass of [WATER, SHADOWS]) {
        gl.uniform1f(uniforms.u_pass, pass)
        for (const tile of shown) {
          within(tile)
          gl.drawArrays(gl.TRIANGLES, 0, tile.ends[BUCKETS - 1])
        }
      }
      // The roads, level by level across the tiles, so that a road over the edge of a tile is on top there too.
      gl.uniform1f(uniforms.u_pass, SURFACE)
      for (let bucket = 0; bucket < BUCKETS; bucket++) {
        for (const tile of shown) {
          const from = bucket === 0 ? 0 : tile.ends[bucket - 1]
          const count = tile.ends[bucket] - from
          if (count === 0) continue
          within(tile)
          gl.drawArrays(gl.TRIANGLES, from, count)
        }
      }
      gl.bindVertexArray(null)
    },
  }
  const routeLayer: CustomLayerInterface = {
    id: RAISED_ROUTE_LAYER,
    type: 'custom',
    renderingMode: '3d',
    onAdd(_, raw) {
      const gl = raw as WebGL2RenderingContext
      const made = linked(gl, ROUTE_VERTEX_SHADER, ROUTE_FRAGMENT_SHADER)
      const array = gl.createVertexArray()
      const buffer = gl.createBuffer()
      gl.bindVertexArray(array)
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer)
      const attribute = (name: string, size: number, offset: number) => {
        const at = gl.getAttribLocation(made, name)
        gl.enableVertexAttribArray(at)
        gl.vertexAttribPointer(at, size, gl.FLOAT, false, 24, offset)
      }
      attribute('a_at', 2, 0)
      attribute('a_high', 1, 8)
      attribute('a_along', 2, 12)
      attribute('a_side', 1, 20)
      gl.bindVertexArray(null)
      route = {
        program: made,
        uniforms: locations(gl, made, ['u_matrix', 'u_rise', 'u_half', 'u_colour']),
        array,
        buffer,
      }
      routeStale = true
    },
    render(raw, args: CustomRenderMethodInput) {
      const gl = raw as WebGL2RenderingContext
      if (!route) return
      if (routeStale) uploadRoute(gl, route.buffer)
      if (routeRuns.length === 0) return
      const zoom = map.getZoom()
      const colours = routeColours(appearance())
      // As MapLibre draws a line: its width lies on the ground, so it narrows into the distance with the flat map's.
      const tilesPerPixel = TILES / (512 * 2 ** zoom)
      gl.useProgram(route.program)
      gl.uniformMatrix4fv(
        route.uniforms.u_matrix,
        false,
        matrixFrom(args.defaultProjectionData.mainMatrix, routeOrigin[0], routeOrigin[1]),
      )
      gl.uniform1f(route.uniforms.u_rise, riseAt(zoom))
      gl.bindVertexArray(route.array)
      overTheMap(gl)
      for (const [colour, width] of [
        [colours.casing, routeWidth(zoom) + ROUTE_CASING],
        [colours.fill, routeWidth(zoom)],
      ] as const) {
        gl.uniform3fv(route.uniforms.u_colour, colour)
        gl.uniform1f(route.uniforms.u_half, (width / 2) * tilesPerPixel)
        for (const { first, count } of routeRuns) gl.drawArrays(gl.TRIANGLE_STRIP, first, count)
      }
      gl.bindVertexArray(null)
    },
  }

  function applyShown() {
    if (map.getLayer(RAISED_LAYER)) map.setLayoutProperty(RAISED_LAYER, 'visibility', showing ? 'visible' : 'none')
    for (const tile of tiles.values()) flatCopy(tile.x, tile.y, showing)
  }

  function setShown(shown: boolean) {
    if (shown === showing) return
    showing = shown
    changed++
    looked = -Infinity
    applyShown()
  }

  // Again after a lost context: MapLibre brings the style back without layers like these.
  map.on('style.load', () => {
    map.addLayer(layer, ROUTE_LOWEST_LAYER)
    map.addLayer(routeLayer, OVER_ROUTE_LAYER)
    applyShown()
  })
  return { update, surfaceAt, tilesChanged: () => changed, setShown, showRoute, screenLift }
}
