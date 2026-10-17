import { useShallow } from 'zustand/react/shallow'
import { guidanceOf } from '../maps/guidance'
import { useMapData } from '../maps/mapData'
import { useCabin, useLive } from '../state/store'
import { useText } from '../state/text'
import { destinationView } from './destination'

/* What the Dashboard's widget column shows, for the widgets (widgets.tsx) and the Dashboard itself. */

/** The destination widget's view (`EMPTY` without one). */
export function useDestinationView() {
  const text = useText()
  const labels = useMapData()?.labels
  return useLive(
    useShallow((state) => {
      const end = state.route?.nodes.at(-1)?.position ?? null
      return destinationView(state.job, end, state.telemetry, labels ?? [], text) ?? EMPTY
    }),
  )
}

/** No destination (one object, so the store's shallow comparison sees "still nothing"). */
export const EMPTY = { kicker: '', title: '', subtitle: null, detail: '', late: false }

/** Whether the column has a widget to show: turn-by-turn, or one turned on in Settings with something to
 * show (Now Playing always has). */
export function useAnyWidget(): boolean {
  const widgets = useCabin((state) => state.dashboardWidgets)
  const guided = useGuided()
  const destination = useDestinationView() !== EMPTY
  const weather = useLive((state) => state.weather !== null)
  return guided || widgets.nowPlaying || (widgets.destination && destination) || (widgets.weather && weather)
}

/** Whether turn-by-turn shows: while the route has a step to show (as GuidanceCard decides). */
export function useGuided(): boolean {
  return useLive((state) => {
    const { route, routeNext, telemetry } = state
    return (
      route !== null &&
      guidanceOf(route.distances, route.maneuvers, routeNext, telemetry?.navigationDistance ?? 0) !== null
    )
  })
}
