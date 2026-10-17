/**
 * The game's ground plane (metres, x east, z south) on MapLibre's globe: scaled as if the world lay on the
 * equator, where a degree is the same distance both ways and Web Mercator barely bends. Must match
 * `westplay_mapdata::area::METRES_PER_DEGREE`, which exported the map data.
 */
export const METRES_PER_DEGREE = 111_319.490_793_273_57

export function toLngLat(x: number, z: number): [number, number] {
  return [x / METRES_PER_DEGREE, -z / METRES_PER_DEGREE]
}

/** Where `lng`, `lat` is on the game's ground (toLngLat the other way). */
export function fromLngLat(lng: number, lat: number): [x: number, z: number] {
  return [lng * METRES_PER_DEGREE, -lat * METRES_PER_DEGREE]
}

/** Screen pixels per metre of ground at a MapLibre zoom (512-pixel tiles), on the equator. */
export function pixelsPerMetre(zoom: number): number {
  return (512 * 2 ** zoom) / (360 * METRES_PER_DEGREE)
}

/** Ground distance between two points (lng, lat) of the game's world, metres. */
export function metresBetween(a: [number, number], b: [number, number]): number {
  return Math.hypot(a[0] - b[0], a[1] - b[1]) * METRES_PER_DEGREE
}

/**
 * How far from the map's centre place names show: a little beyond the ground the map spans across at
 * `zoom` (`width` pixels wide), so names come in just before their place does.
 */
export function labelRange(zoom: number, width: number): number {
  return (1.5 * width) / pixelsPerMetre(zoom)
}
