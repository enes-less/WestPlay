import { useEffect, useState } from 'react'
import { GuidanceCard } from '../maps/GuidanceCard'
import { useCabin } from '../state/store'
import { useMapGrown } from './idle'
import { MapView } from './MapView'
import { useAnyWidget } from './widgetState'
import { DestinationWidget, NowPlayingWidget, WeatherWidget } from './widgets'
import './dashboard.css'

/** Must match --motion-duration in shell.css: how long the map takes to grow or shrink. */
const MOTION_MS = 650

/**
 * Where the map's growing is (dashboard.css): resting beside the widgets; `wide`, its content made the whole
 * width a frame before it grows; grown; shrinking back, still the whole width until it has.
 */
type Growth = 'rest' | 'wide' | 'grown' | 'shrinking'

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
  const growth = useGrowth(grown)
  const classes = ['screen', 'dashboard']
  if (anyWidget) classes.push('has-widgets')
  if (growth !== 'rest') classes.push('map-wide')
  if (growth === 'grown') classes.push('map-grown')
  if (growth === 'grown' || growth === 'shrinking') classes.push('map-moving')
  return (
    <section className={classes.join(' ')} aria-label="Dashboard">
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

/**
 * The map's growing as `grown` asks for it, by transforms alone (dashboard.css): growing, the map's content is
 * made the whole width first (the map drawn at that size once, before anything moves), and grows only once that
 * has been on the screen; shrunk back, it's made just what shows again.
 */
function useGrowth(grown: boolean): Growth {
  const [growth, setGrowth] = useState<Growth>('rest')
  useEffect(() => {
    setGrowth((now) => {
      if (grown) return now === 'rest' ? 'wide' : now === 'shrinking' ? 'grown' : now
      return now === 'grown' ? 'shrinking' : now === 'wide' ? 'rest' : now
    })
  }, [grown])
  useEffect(() => {
    if (growth === 'wide') {
      // Two frames: the whole width laid out, the map drawn at it and shown, before the move starts from there.
      let second = 0
      const first = requestAnimationFrame(() => {
        second = requestAnimationFrame(() => setGrowth('grown'))
      })
      return () => {
        cancelAnimationFrame(first)
        cancelAnimationFrame(second)
      }
    }
    if (growth === 'shrinking') {
      // Once the move has surely ended (it starts a little after this, as the page draws the change).
      const done = setTimeout(() => setGrowth('rest'), MOTION_MS + 100)
      return () => clearTimeout(done)
    }
  }, [growth])
  return growth
}
