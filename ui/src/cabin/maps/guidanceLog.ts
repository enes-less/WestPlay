import { postToHost } from '../../shared/host'
import { useLive } from '../state/store'
import { guidanceLine } from './guidance'

/** How often the cab's guidance is written to the host's log while there's a route (as the host's own route line). */
const SAY_EVERY_MS = 30_000

/**
 * Every 30 s while there's a route, the cab's guidance ([`guidanceLine`]) for the host's log (app.log). A player's
 * every turn showed "now" in their Convoy (2026-11-10): the log had the distances but not the turn they were taken
 * from. Returns its stop.
 */
export function logGuidance(): () => void {
  const timer = window.setInterval(() => {
    const { route, routeNext, telemetry } = useLive.getState()
    const line = route && guidanceLine(route, routeNext, telemetry?.navigationDistance ?? 0)
    if (line) postToHost({ type: 'guidance', line })
  }, SAY_EVERY_MS)
  return () => window.clearInterval(timer)
}
