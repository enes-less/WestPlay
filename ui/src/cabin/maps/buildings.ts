import type { FilterSpecification } from 'maplibre-gl'

/**
 * The buildings raised on the map (the tiles' `building` layer: crates/mapdata buildings.rs). A border
 * crossing's or a filling station's canopy is raised too, but not while the truck is under it (owner
 * 2026-10-22): it would cover the truck.
 */

/** A canopy as the tiles give it: its id and its outline (rings of [lng, lat]). */
export interface Canopy {
  id: number
  rings: [number, number][][]
}

/** Whether `point` is inside `ring` (even-odd). */
function inside(ring: readonly [number, number][], [x, y]: [number, number]): boolean {
  let within = false
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [xi, yi] = ring[i]
    const [xj, yj] = ring[j]
    if (yi > y !== yj > y && x < xi + ((y - yi) * (xj - xi)) / (yj - yi)) within = !within
  }
  return within
}

/** The ids of the canopies over `point` ([lng, lat]), each once, in order. */
export function canopiesOver(canopies: readonly Canopy[], point: [number, number]): number[] {
  const ids = canopies.filter((canopy) => canopy.rings.some((ring) => inside(ring, point))).map((canopy) => canopy.id)
  return [...new Set(ids)].sort((a, b) => a - b)
}

/** The building layer's filter: every building but the canopies `hidden` (`null`: none hidden). */
export function buildingFilter(hidden: readonly number[]): FilterSpecification | null {
  return hidden.length === 0 ? null : ['!', ['in', ['get', 'id'], ['literal', [...hidden]]]]
}
