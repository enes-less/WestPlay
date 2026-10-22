import { isInHost, type PlannedRoute, postToHost, type RouteAction, type RouteAnswer } from '../../shared/host'

/**
 * Changes the game's own GPS route (Maps' search): the host passes the request to the game plugin, which
 * gives the game's navigator its new waypoints as the game's map does (crates/plugin/src/navigation.rs), and
 * the game's answer comes back. Outside the host (a browser preview) there is no game: `noAnswer`.
 */

let nextId = 1
const waiting = new Map<number, (answer: RouteAnswer) => void>()

/** Asks the game to change its route: at `node` (a map node's uid, hex) to go there or add a stop. */
export function changeRoute(action: RouteAction, node?: string): Promise<RouteAnswer> {
  if (!isInHost()) return Promise.resolve('noAnswer')
  const id = nextId++
  postToHost({ type: 'route', id, action, node })
  return new Promise((resolve) => waiting.set(id, resolve))
}

/** The host's `routeAnswer`: settles the request it answers. */
export function receiveRouteAnswer(id: number, answer: RouteAnswer): void {
  waiting.get(id)?.(answer)
  waiting.delete(id)
}

const planning = new Map<number, (routes: PlannedRoute[]) => void>()
const following = new Map<number, (followed: { follows: boolean; answer: string }) => void>()

/**
 * WestPlay's own routes to `node` (a map node's uid, hex) from where the truck is (crates/host planner.rs),
 * to offer before Go, with the request's `id` for following one. None outside the host, or where the map
 * has no road graph (built by an older WestPlay): then Go goes straight to the game.
 */
export function planRoutes(node: string): Promise<{ id: number; routes: PlannedRoute[] }> {
  if (!isInHost()) return Promise.resolve({ id: 0, routes: [] })
  const id = nextId++
  postToHost({ type: 'routes', id, node })
  return new Promise((resolve) => {
    planning.set(id, (routes) => resolve({ id, routes }))
    // No answer in time (no map, the host busy): none offered.
    setTimeout(() => planning.get(id)?.([]), 8000)
  })
}

/** Hands route `index` of request `id` to the game's GPS: whether the game follows it. */
export function followRoute(id: number, index: number): Promise<{ follows: boolean; answer: string }> {
  if (!isInHost()) return Promise.resolve({ follows: false, answer: 'noAnswer' })
  postToHost({ type: 'follow', id, index })
  return new Promise((resolve) => {
    following.set(id, resolve)
    setTimeout(() => following.get(id)?.({ follows: false, answer: 'noAnswer' }), 30_000)
  })
}

/** The host's `routes`: settles the request it answers. */
export function receiveRoutes(id: number, routes: PlannedRoute[]): void {
  planning.get(id)?.(routes)
  planning.delete(id)
}

/** The host's `followed`: settles the hand-over it answers. */
export function receiveFollowed(id: number, follows: boolean, answer: string): void {
  following.get(id)?.({ follows, answer })
  following.delete(id)
}

const detouring = new Map<number, (seconds: (number | null)[]) => void>()

/** What stopping at each of `nodes` (map node uids, hex) adds to the trip the game's route makes (seconds on
 * the map's ground; `null`: not known). None outside the host. */
export function askDetours(nodes: string[]): Promise<(number | null)[]> {
  if (!isInHost() || nodes.length === 0) return Promise.resolve(nodes.map(() => null))
  const id = nextId++
  postToHost({ type: 'detours', id, nodes })
  return new Promise((resolve) => {
    detouring.set(id, resolve)
    setTimeout(() => detouring.get(id)?.(nodes.map(() => null)), 6000)
  })
}

/** The host's `detours`: settles the request it answers. */
export function receiveDetours(id: number, seconds: (number | null)[]): void {
  detouring.get(id)?.(seconds)
  detouring.delete(id)
}
