import type maplibregl from 'maplibre-gl'
import type { CustomLayerInterface, CustomRenderMethodInput } from 'maplibre-gl'
import type { Appearance } from '../state/appearance'
import { RAISED_LAYER } from './raisedRoads'
import {
  BUILDING_LIGHT_SPHERE,
  BUILDING_LIGHT_STRENGTH,
  BUILDING_RISE_ZOOMS,
  ROUTE_LOWEST_LAYER,
  buildingColour,
  flatBuildingColour,
} from './style'

/**
 * The buildings in their real shapes (owner 2026-11-02: "instead of picking the top, we will take its shape"): the
 * game's own triangles of each, from the map's files (crates/mapdata meshes.rs). Each model's triangles are in a file
 * of their own, read once, and drawn wherever a tile's file places it; a bridge's are in its tile's file as its road
 * is drawn. Near, a model's triangles merged on a grid of a metre (a landmark's as they are); further off, on one of
 * four. Lit as MapLibre lights the raised outlines it draws (style.ts BUILDING_LIGHT), which stay for what's lifted off
 * the ground (a filling station's roof, left out while the truck is under it) and a structure set's hall.
 *
 * With 3D buildings off (the companion's Map page), the same shapes lie flat on the map, in the flat buildings' own
 * colour and unlit, as the map before 3D buildings had its buildings (owner 2026-11-04: "when 3d buildings are turned
 * off, i still want to see building shapes as flat"); the bridges' models are left out then, their roads drawn anyway.
 */

export const MESH_LAYER = 'building-shapes'
const FILE_ZOOM = 14
const TILES = 2 ** FILE_ZOOM
/** How far past their tile a file's points may lie, of the tile (meshes.rs). */
const PAST = 0.5
const MODEL_MAGIC = 0x314d5057
const TILE_MAGIC = 0x31545057
const PLACING_BYTES = 28
const NO_CAP = 65535
/** Tiles further past the view than this, in tiles, aren't loaded; at most this many kept, this many loaded at once. */
const TILES_PAST_VIEW = 0.5
const TILES_KEPT = 48
const LOADING_AT_ONCE = 6
const MODELS_KEPT = 1500
/** Model files fetched at once: the next starts as one is in, nearest first. */
const MODEL_FETCHES = 16
const LOOK_EVERY_MS = 300
/** Near (a model's finer triangles): from this zoom, the tiles this near the middle of the view, in tiles. */
const NEAR_ZOOM = 16
const NEAR_TILES = 1
/** One metre in MapLibre's world (x and y 0 to 1). */
const METRE = 1 / 40075016.686
/** The side of a wall darker than its top, at most (MapLibre's vertical gradient at BUILDING_LIGHT's strength). */
const WALL_SHADE = 0.7 + (0.98 - 0.7) * (1 - BUILDING_LIGHT_STRENGTH)

const VERTEX_SHADER = `#version 300 es
uniform mat4 u_matrix;
uniform float u_rise;
uniform float u_unit;
uniform float u_top;
uniform float u_bridge;
in vec3 a_pos;
in vec2 a_at;
in float a_yaw;
in vec3 a_scale;
in float a_cap;
out vec3 v_place;
out float v_gradient;
const float PAST = ${PAST};
const float METRE = ${METRE};
const float TILES = ${TILES}.0;
void main() {
  vec2 at;
  float high;
  float top;
  if (u_bridge > 0.5) {
    at = a_pos.xy / 65535.0 * (1.0 + 2.0 * PAST) - PAST;
    high = a_pos.z * 0.01;
    top = high;
  } else {
    // Scaled, then turned about the up axis as the game turns its nodes (buildings.rs place).
    vec3 p = a_pos * u_unit * vec3(a_scale.x, abs(a_scale.y), a_scale.z);
    float c = cos(a_yaw);
    float s = sin(a_yaw);
    vec2 off = vec2(p.x * c + p.z * s, -p.x * s + p.z * c);
    at = a_at * (1.0 + 2.0 * PAST) - PAST + off * METRE * TILES;
    // Under a road over it, it stops under the road (raised.rs Decks::over).
    high = a_cap < ${NO_CAP}.0 ? min(p.y, a_cap * 0.01) : p.y;
    top = u_top * abs(a_scale.y);
  }
  high *= u_rise;
  // MapLibre's vertical gradient: a wall darker towards its foot, by how tall the building is.
  v_gradient = clamp(high / max(top * u_rise, 0.01) * pow(max(top, 0.0) / 150.0, 0.5), ${WALL_SHADE}, 1.0);
  v_place = vec3(at / (METRE * TILES), high);
  gl_Position = u_matrix * vec4(at, high * METRE, 1.0);
}`

const FRAGMENT_SHADER = `#version 300 es
precision highp float;
uniform vec3 u_colour;
uniform vec3 u_light;
uniform float u_flat;
in vec3 v_place;
in float v_gradient;
out vec4 colour;
const float STRENGTH = ${BUILDING_LIGHT_STRENGTH};
void main() {
  // Flat (3D buildings off): the shape alone, in its colour.
  if (u_flat > 0.5) {
    colour = vec4(u_colour, 1.0);
    return;
  }
  // The face's own way out (x east, y south, z up), as the map's outlines have theirs: lit as MapLibre lights them.
  vec3 normal = -normalize(cross(dFdx(v_place), dFdy(v_place)));
  float value = dot(u_colour, vec3(0.2126, 0.7152, 0.0722));
  float lit = clamp(dot(normal, u_light), 0.0, 1.0);
  lit = mix(1.0 - STRENGTH, max(1.0 - value + STRENGTH, 1.0), lit);
  if (abs(normal.z) < 0.5) lit *= v_gradient;
  colour = vec4(clamp((u_colour + 0.03) * lit, 0.0, 1.0), 1.0);
}`

type Key = `${number}/${number}`

/** A run of a tile's placings of one model. */
interface Run {
  model: number
  first: number
  count: number
}

interface Tile {
  x: number
  y: number
  placings: ArrayBuffer
  runs: Run[]
  bridge: Uint16Array
  buffer: WebGLBuffer | null
  bridgeArray: WebGLVertexArrayObject | null
  bridgeBuffer: WebGLBuffer | null
}

interface Level {
  vertices: Int16Array
  indices: Uint16Array | Uint32Array
  array: WebGLVertexArrayObject | null
  vertexBuffer: WebGLBuffer | null
  indexBuffer: WebGLBuffer | null
}

interface Model {
  unit: number
  top: number
  levels: Level[]
  used: number
}

export interface BuildingMeshes {
  /** Keeps the tiles round the view loaded, and their models. Called every frame; looks now and then. */
  update(now: number): void
  setShown(shown: boolean): void
  /** What it has: for looking into it. */
  stats(): {
    known: number
    tiles: number
    loading: number
    models: number
    loadingModels: number
    unreadable: number
    drawn: number
  }
}

/** `main` for what's given in tile widths from (x, y), which is in tile widths too (as raisedRoads.ts). */
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

function linked(gl: WebGL2RenderingContext, vertex: string, fragment: string): WebGLProgram {
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

async function gunzipped(response: Response): Promise<ArrayBuffer | null> {
  if (!response.ok || !response.body) return null
  return new Response(response.body.pipeThrough(new DecompressionStream('gzip'))).arrayBuffer()
}

/** A model's file (meshes.rs), or `null` if it isn't whole. */
export function readModel(bytes: ArrayBuffer): Model | null {
  const view = new DataView(bytes)
  if (bytes.byteLength < 16 || view.getUint32(0, true) !== MODEL_MAGIC) return null
  const [unit, top, count] = [view.getFloat32(4, true), view.getFloat32(8, true), view.getUint32(12, true)]
  const heads = Array.from({ length: count }, (_, i) => [0, 1, 2].map((k) => view.getUint32(16 + i * 12 + k * 4, true)))
  let at = 16 + count * 12
  const pad = (n: number) => (n + 3) & ~3
  const levels: Level[] = []
  for (const [vertexCount, indexCount, indexBytes] of heads) {
    const vertexEnd = at + pad(vertexCount * 6)
    const indexEnd = vertexEnd + pad(indexCount * indexBytes)
    if (indexEnd > bytes.byteLength || (indexBytes !== 2 && indexBytes !== 4)) return null
    const vertices = new Int16Array(bytes, at, vertexCount * 3)
    const indices =
      indexBytes === 2 ? new Uint16Array(bytes, vertexEnd, indexCount) : new Uint32Array(bytes, vertexEnd, indexCount)
    levels.push({ vertices, indices, array: null, vertexBuffer: null, indexBuffer: null })
    at = indexEnd
  }
  return { unit, top, levels, used: 0 }
}

/** A tile's file (meshes.rs), or `null` if it isn't whole. */
export function readTile(x: number, y: number, bytes: ArrayBuffer): Tile | null {
  const view = new DataView(bytes)
  if (bytes.byteLength < 12 || view.getUint32(0, true) !== TILE_MAGIC) return null
  const [placings, vertices] = [view.getUint32(4, true), view.getUint32(8, true)]
  const bridgeAt = 12 + placings * PLACING_BYTES
  if (bridgeAt + vertices * 6 > bytes.byteLength) return null
  const runs: Run[] = []
  for (let i = 0; i < placings; i++) {
    const model = view.getUint32(12 + i * PLACING_BYTES, true)
    const last = runs.at(-1)
    if (last && last.model === model) last.count++
    else runs.push({ model, first: i, count: 1 })
  }
  return {
    x,
    y,
    placings: bytes.slice(12, bridgeAt),
    runs,
    bridge: new Uint16Array(bytes.slice(bridgeAt, bridgeAt + vertices * 6)),
    buffer: null,
    bridgeArray: null,
    bridgeBuffer: null,
  }
}

/** The light's way to the map (x east, y south, z up) as MapLibre has it at `bearing` degrees (fill-extrusion). */
export function lightAt(bearing: number): [number, number, number] {
  const [r, azimuth, polar] = BUILDING_LIGHT_SPHERE
  const a = ((azimuth + 90) * Math.PI) / 180
  const p = (polar * Math.PI) / 180
  const [x, y, z] = [r * Math.cos(a) * Math.sin(p), r * Math.sin(a) * Math.sin(p), r * Math.cos(p)]
  const turn = (bearing * Math.PI) / 180
  const [c, s] = [Math.cos(turn), Math.sin(turn)]
  return [c * x - s * y, s * x + c * y, z]
}

/** The buildings in their real shapes of the map whose files are at `address`, painted in `appearance()`. */
export function buildingMeshes(map: maplibregl.Map, address: string, appearance: () => Appearance): BuildingMeshes {
  const tiles = new Map<Key, Tile>()
  const loading = new Set<Key>()
  const models = new Map<number, Model>()
  const loadingModels = new Set<number>()
  const unreadable = new Set<number>()
  /** The models wanted and not yet in, nearest first. */
  let queue: number[] = []
  let known: Set<Key> | null = null
  let looked = -Infinity
  let context: WebGL2RenderingContext | null = null
  let program: WebGLProgram | null = null
  let uniforms: Record<string, WebGLUniformLocation | null> = {}
  const attributes: Record<string, number> = {}
  let showing = true
  let frame = 0
  /** A look asked for, when one came too soon after the last: the map may be still by then, drawing nothing. */
  let again: ReturnType<typeof setTimeout> | null = null
  let drawn = 0

  void fetch(`${address}/index.json`)
    .then((response) => (response.ok ? response.json() : []))
    .then((list: [number, number][]) => {
      known = new Set(list.map(([x, y]) => `${x}/${y}` as Key))
      looked = -Infinity
    })
    .catch(() => {
      known = new Set()
    })

  function loadTile(key: Key, x: number, y: number) {
    loading.add(key)
    void fetch(`${address}/${x}/${y}.bin`)
      .then(gunzipped)
      .then((bytes) => {
        const tile = bytes && readTile(x, y, bytes)
        if (!tile) {
          known?.delete(key)
          return
        }
        tiles.set(key, tile)
        map.triggerRepaint()
      })
      .catch(() => known?.delete(key))
      .finally(() => loading.delete(key))
  }

  function loadModel(id: number) {
    loadingModels.add(id)
    void fetch(`${address}/models/${id}.bin`)
      .then(gunzipped)
      .then((bytes) => {
        const model = bytes && readModel(bytes)
        if (!model) {
          unreadable.add(id)
          return
        }
        models.set(id, model)
        map.triggerRepaint()
      })
      .catch(() => unreadable.add(id))
      .finally(() => {
        loadingModels.delete(id)
        pump()
      })
  }

  function pump() {
    while (loadingModels.size < MODEL_FETCHES && queue.length > 0) {
      const id = queue.shift()!
      if (!models.has(id) && !loadingModels.has(id) && !unreadable.has(id)) loadModel(id)
    }
  }

  function dropTile(key: Key, tile: Tile) {
    if (context) {
      if (tile.buffer) context.deleteBuffer(tile.buffer)
      if (tile.bridgeBuffer) context.deleteBuffer(tile.bridgeBuffer)
      if (tile.bridgeArray) context.deleteVertexArray(tile.bridgeArray)
    }
    tiles.delete(key)
  }

  function dropModel(id: number, model: Model) {
    if (context) {
      for (const level of model.levels) {
        if (level.vertexBuffer) context.deleteBuffer(level.vertexBuffer)
        if (level.indexBuffer) context.deleteBuffer(level.indexBuffer)
        if (level.array) context.deleteVertexArray(level.array)
      }
    }
    models.delete(id)
  }

  /** The tiles the view takes in, with a little past it: x0, y0, x1, y1 in tile widths. */
  function viewTiles(): [number, number, number, number] {
    const bounds = map.getBounds()
    const tileX = (lng: number) => ((lng + 180) / 360) * TILES
    const tileY = (lat: number) =>
      (0.5 - Math.log(Math.tan(Math.PI / 4 + (lat * Math.PI) / 360)) / (2 * Math.PI)) * TILES
    return [
      tileX(bounds.getWest()) - TILES_PAST_VIEW,
      tileY(bounds.getNorth()) - TILES_PAST_VIEW,
      tileX(bounds.getEast()) + TILES_PAST_VIEW,
      tileY(bounds.getSouth()) + TILES_PAST_VIEW,
    ]
  }

  function middle(): [number, number] {
    const { lng, lat } = map.getCenter()
    return [
      ((lng + 180) / 360) * TILES,
      (0.5 - Math.log(Math.tan(Math.PI / 4 + (lat * Math.PI) / 360)) / (2 * Math.PI)) * TILES,
    ]
  }

  function update(now: number) {
    if (!known || map.getZoom() < BUILDING_RISE_ZOOMS[0]) return
    if (now - looked < LOOK_EVERY_MS) {
      again ??= setTimeout(() => {
        again = null
        map.triggerRepaint()
      }, LOOK_EVERY_MS)
      return
    }
    looked = now
    const [x0, y0, x1, y1] = viewTiles()
    const [mx, my] = middle()
    const away = (x: number, y: number) => (x + 0.5 - mx) ** 2 + (y + 0.5 - my) ** 2
    const inView = (x: number, y: number) => x + 1 >= x0 && x <= x1 && y + 1 >= y0 && y <= y1
    const missing: [number, Key, number, number][] = []
    for (let x = Math.floor(x0); x <= x1; x++) {
      for (let y = Math.floor(y0); y <= y1; y++) {
        const key: Key = `${x}/${y}`
        if (known.has(key) && !tiles.has(key) && !loading.has(key)) missing.push([away(x, y), key, x, y])
      }
    }
    // The nearest first, and no more than are kept.
    missing.sort((a, b) => a[0] - b[0])
    const room = Math.max(TILES_KEPT - tiles.size, 0)
    for (const [, key, x, y] of missing.slice(0, Math.min(Math.max(LOADING_AT_ONCE - loading.size, 0), room + 1))) {
      loadTile(key, x, y)
    }
    if (tiles.size > TILES_KEPT || missing.length > 0) {
      for (const [key, tile] of tiles) {
        if (!inView(tile.x, tile.y)) dropTile(key, tile)
      }
    }
    // The models the tiles in view place, the nearest tiles' first.
    const wanted: number[] = []
    const shown = [...tiles.values()].filter((tile) => inView(tile.x, tile.y))
    shown.sort((a, b) => away(a.x, a.y) - away(b.x, b.y))
    for (const tile of shown) {
      for (const run of tile.runs) {
        if (!models.has(run.model) && !loadingModels.has(run.model) && !unreadable.has(run.model))
          wanted.push(run.model)
      }
    }
    queue = [...new Set(wanted)]
    pump()
    if (models.size > MODELS_KEPT) {
      const byUse = [...models].sort((a, b) => a[1].used - b[1].used)
      for (const [id, model] of byUse.slice(0, models.size - MODELS_KEPT)) dropModel(id, model)
    }
  }

  function uploadLevel(gl: WebGL2RenderingContext, level: Level) {
    level.array = gl.createVertexArray()
    level.vertexBuffer = gl.createBuffer()
    level.indexBuffer = gl.createBuffer()
    gl.bindVertexArray(level.array)
    gl.bindBuffer(gl.ARRAY_BUFFER, level.vertexBuffer)
    gl.bufferData(gl.ARRAY_BUFFER, level.vertices, gl.STATIC_DRAW)
    gl.enableVertexAttribArray(attributes.a_pos)
    gl.vertexAttribPointer(attributes.a_pos, 3, gl.SHORT, false, 6, 0)
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, level.indexBuffer)
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, level.indices, gl.STATIC_DRAW)
    for (const name of ['a_at', 'a_yaw', 'a_scale', 'a_cap']) {
      gl.enableVertexAttribArray(attributes[name])
      gl.vertexAttribDivisor(attributes[name], 1)
    }
    gl.bindVertexArray(null)
  }

  function uploadTile(gl: WebGL2RenderingContext, tile: Tile) {
    tile.buffer = gl.createBuffer()
    gl.bindBuffer(gl.ARRAY_BUFFER, tile.buffer)
    gl.bufferData(gl.ARRAY_BUFFER, tile.placings, gl.STATIC_DRAW)
    if (tile.bridge.length > 0) {
      tile.bridgeArray = gl.createVertexArray()
      tile.bridgeBuffer = gl.createBuffer()
      gl.bindVertexArray(tile.bridgeArray)
      gl.bindBuffer(gl.ARRAY_BUFFER, tile.bridgeBuffer)
      gl.bufferData(gl.ARRAY_BUFFER, tile.bridge, gl.STATIC_DRAW)
      gl.enableVertexAttribArray(attributes.a_pos)
      gl.vertexAttribPointer(attributes.a_pos, 3, gl.UNSIGNED_SHORT, false, 6, 0)
      gl.bindVertexArray(null)
    }
  }

  const layer: CustomLayerInterface = {
    id: MESH_LAYER,
    type: 'custom',
    renderingMode: '3d',
    onAdd(_, raw) {
      const gl = raw as WebGL2RenderingContext
      context = gl
      // A new context, if the last was lost: nothing made in it is there any more.
      for (const tile of tiles.values()) Object.assign(tile, { buffer: null, bridgeArray: null, bridgeBuffer: null })
      for (const model of models.values()) {
        for (const level of model.levels) Object.assign(level, { array: null, vertexBuffer: null, indexBuffer: null })
      }
      program = linked(gl, VERTEX_SHADER, FRAGMENT_SHADER)
      uniforms = Object.fromEntries(
        ['u_matrix', 'u_rise', 'u_unit', 'u_top', 'u_bridge', 'u_colour', 'u_light', 'u_flat'].map((name) => [
          name,
          gl.getUniformLocation(program!, name),
        ]),
      )
      for (const name of ['a_pos', 'a_at', 'a_yaw', 'a_scale', 'a_cap'])
        attributes[name] = gl.getAttribLocation(program, name)
    },
    render(raw, args: CustomRenderMethodInput) {
      const gl = raw as WebGL2RenderingContext
      const zoom = map.getZoom()
      if (zoom < BUILDING_RISE_ZOOMS[0] || !program || tiles.size === 0) return
      frame++
      // 3D buildings off: flat.
      const flat = !showing
      const rise = flat
        ? 0
        : Math.min(Math.max((zoom - BUILDING_RISE_ZOOMS[0]) / (BUILDING_RISE_ZOOMS[1] - BUILDING_RISE_ZOOMS[0]), 0), 1)
      const main = args.defaultProjectionData.mainMatrix
      const [x0, y0, x1, y1] = viewTiles()
      const [mx, my] = middle()
      gl.useProgram(program)
      gl.uniform1f(uniforms.u_rise, rise)
      gl.uniform3fv(uniforms.u_colour, flat ? flatBuildingColour(appearance()) : buildingColour(appearance()))
      gl.uniform3fv(uniforms.u_light, lightAt(map.getBearing()))
      gl.uniform1f(uniforms.u_flat, flat ? 1 : 0)
      gl.disable(gl.CULL_FACE)
      gl.disable(gl.STENCIL_TEST)
      gl.disable(gl.BLEND)
      gl.enable(gl.DEPTH_TEST)
      gl.depthFunc(gl.LEQUAL)
      gl.depthMask(true)
      drawn = 0
      for (const tile of tiles.values()) {
        if (tile.x + 1 < x0 || tile.x > x1 || tile.y + 1 < y0 || tile.y > y1) continue
        if (!tile.buffer) uploadTile(gl, tile)
        gl.uniformMatrix4fv(uniforms.u_matrix, false, matrixFrom(main, tile.x, tile.y))
        const near = zoom >= NEAR_ZOOM && Math.hypot(tile.x + 0.5 - mx, tile.y + 0.5 - my) <= NEAR_TILES + 0.75
        gl.uniform1f(uniforms.u_bridge, 0)
        for (const run of tile.runs) {
          const model = models.get(run.model)
          if (!model) continue
          model.used = frame
          const level = model.levels[near ? 0 : Math.min(1, model.levels.length - 1)]
          if (!level || level.indices.length === 0) continue
          if (!level.array) uploadLevel(gl, level)
          gl.bindVertexArray(level.array)
          gl.bindBuffer(gl.ARRAY_BUFFER, tile.buffer)
          const base = run.first * PLACING_BYTES
          gl.vertexAttribPointer(attributes.a_at, 2, gl.UNSIGNED_SHORT, true, PLACING_BYTES, base + 4)
          gl.vertexAttribPointer(attributes.a_yaw, 1, gl.FLOAT, false, PLACING_BYTES, base + 8)
          gl.vertexAttribPointer(attributes.a_scale, 3, gl.FLOAT, false, PLACING_BYTES, base + 12)
          gl.vertexAttribPointer(attributes.a_cap, 1, gl.UNSIGNED_SHORT, false, PLACING_BYTES, base + 24)
          gl.uniform1f(uniforms.u_unit, model.unit)
          gl.uniform1f(uniforms.u_top, model.top)
          const type = level.indices instanceof Uint32Array ? gl.UNSIGNED_INT : gl.UNSIGNED_SHORT
          gl.drawElementsInstanced(gl.TRIANGLES, level.indices.length, type, 0, run.count)
          drawn += run.count
        }
        if (tile.bridgeArray && !flat) {
          gl.bindVertexArray(tile.bridgeArray)
          gl.uniform1f(uniforms.u_bridge, 1)
          gl.drawArrays(gl.TRIANGLES, 0, tile.bridge.length / 3)
        }
      }
      gl.bindVertexArray(null)
    },
  }

  /** Raised (3D buildings on) or flat. */
  function setShown(shown: boolean) {
    if (shown === showing) return
    showing = shown
    looked = -Infinity
    map.triggerRepaint()
  }

  // Again after a lost context: MapLibre brings the style back without layers like these. Under the raised roads,
  // which read what it leaves in the depth buffer.
  map.on('style.load', () => {
    map.addLayer(layer, map.getLayer(RAISED_LAYER) ? RAISED_LAYER : ROUTE_LOWEST_LAYER)
  })
  const stats = () => ({
    known: known?.size ?? -1,
    tiles: tiles.size,
    loading: loading.size,
    models: models.size,
    loadingModels: loadingModels.size,
    unreadable: unreadable.size,
    drawn,
  })
  return { update, setShown, stats }
}
