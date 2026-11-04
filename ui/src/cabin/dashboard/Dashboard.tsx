import { GuidanceCard } from '../maps/GuidanceCard'
import { useCabin } from '../state/store'
import { useMapGrown } from './idle'
import { MapView } from './MapView'
import { useAnyWidget } from './widgetState'
import { DestinationWidget, NowPlayingWidget, WeatherWidget } from './widgets'
import './dashboard.css'

/**
 * CarPlay-style Dashboard: the map takes most of the space (with one ETA pill), and a column of one-job
 * widgets sits on the right: turn-by-turn on top while there's a route, the weather where
 * there's room. Driving at 30 km/h or more with the screen left alone 30 s, the widgets slide away to the right
 * and the map grows into their room; any use of the screen, or slowing below it, brings them back (owner
 * 2026-11-03; a setting, Settings › Dashboard).
 */
export function Dashboard() {
  // No widget to show (those turned on in Settings have nothing, and no route): the map has the whole width.
  const anyWidget = useAnyWidget()
  const growing = useCabin((state) => state.mapGrows)
  const grown = useMapGrown(growing && anyWidget)
  return (
    <section className={grown ? 'screen dashboard map-grown' : 'screen dashboard'} aria-label="Dashboard">
      <MapView />
      <div className="widgets" hidden={!anyWidget}>
        <GuidanceCard variant="widget" />
        <NowPlayingWidget />
        <DestinationWidget />
        <WeatherWidget />
      </div>
    </section>
  )
}
