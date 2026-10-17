import { isInHost, postToHost, type RouteAction, type RouteAnswer } from '../../shared/host'

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
