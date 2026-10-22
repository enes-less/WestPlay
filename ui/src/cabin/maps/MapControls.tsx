import { useEffect, useRef, useState } from 'react'
import { useText } from '../state/text'
import { followBearing, northUpLiveMap, recentreLiveMap, useMapBrowsing, zoomLiveMap } from './liveMap'

/**
 * The Maps app's buttons over the map, like CarPlay's: zoom in and out; the compass right over the zoom pair,
 * which turns the map north up; and the arrow, once the map has been moved away from the truck or turned
 * north up, which brings the chase view back. (The wheel, dragging and right-dragging do the same with the
 * mouse.)
 */
export function MapControls() {
  const browsing = useMapBrowsing((state) => state.browsing)
  const text = useText()
  return (
    <div className="map-controls">
      <button
        type="button"
        className={browsing ? 'map-control map-recentre glass shown' : 'map-control map-recentre glass'}
        aria-label={text.recentre}
        aria-hidden={!browsing}
        tabIndex={browsing ? 0 : -1}
        onClick={recentreLiveMap}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M20.5 3.5 3.8 10.6c-.8.4-.7 1.6.2 1.8l6.6 1.1 1.1 6.6c.2.9 1.4 1 1.8.2Z" fill="currentColor" />
        </svg>
      </button>
      <MapCompass label={text.northUp} />
      <MapZoom />
    </div>
  )
}

/** How long the Dashboard's zoom buttons stay after the player last used the map, ms (owner 2026-10-21). */
const DASHBOARD_IDLE_MS = 15_000

/**
 * The Dashboard map's zoom buttons (owner 2026-10-21), the Maps app's pair in the same place: the map there
 * only follows the truck, closer or further. They fade out after 15 s without the player using the map, and
 * come back as the pointer moves over it or presses it.
 */
export function DashboardMapControls() {
  const controls = useRef<HTMLDivElement>(null)
  const [shown, setShown] = useState(true)
  useEffect(() => {
    // The map tile around them: using any of it brings them back.
    const tile = controls.current?.parentElement
    if (!tile) return
    let timer = setTimeout(() => setShown(false), DASHBOARD_IDLE_MS)
    const used = () => {
      clearTimeout(timer)
      setShown(true)
      timer = setTimeout(() => setShown(false), DASHBOARD_IDLE_MS)
    }
    const events = ['pointermove', 'pointerdown', 'wheel'] as const
    for (const event of events) tile.addEventListener(event, used, { passive: true })
    return () => {
      clearTimeout(timer)
      for (const event of events) tile.removeEventListener(event, used)
    }
  }, [])
  return (
    <div ref={controls} className={shown ? 'map-controls dashboard-zoom' : 'map-controls dashboard-zoom faded'}>
      <MapZoom />
    </div>
  )
}

/** Zoom in and out a step, a pair of buttons in one glass. */
function MapZoom() {
  const text = useText()
  return (
    <div className="map-zoom glass">
      <button
        type="button"
        className="map-control"
        data-control="zoom-in"
        aria-label={text.zoomIn}
        onClick={() => zoomLiveMap(1)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M12 5v14M5 12h14" />
        </svg>
      </button>
      <span className="map-zoom-divider" aria-hidden />
      <button
        type="button"
        className="map-control"
        data-control="zoom-out"
        aria-label={text.zoomOut}
        onClick={() => zoomLiveMap(-1)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M5 12h14" />
        </svg>
      </button>
    </div>
  )
}

/**
 * The map's north, as Apple Maps shows it: a compass whose red mark and "N" point north, turned against the
 * map as it turns with the truck (owner 2026-10-21: the north was nowhere to be seen). Turned in place, not
 * re-rendered, every time the map turns. Tapped, the map turns north up, seen from above.
 */
function MapCompass({ label }: { label: string }) {
  const dial = useRef<HTMLDivElement>(null)
  useEffect(
    () =>
      followBearing((bearing) => {
        if (dial.current) dial.current.style.transform = `rotate(${-bearing}deg)`
      }),
    [],
  )
  return (
    <button type="button" className="map-compass glass" aria-label={label} onClick={northUpLiveMap}>
      <div ref={dial} className="map-compass-dial">
        <svg viewBox="0 0 44 44" aria-hidden="true">
          <path className="map-compass-north" d="M22 3.5 25.5 10h-7Z" />
          <text x="22" y="27.5" textAnchor="middle">
            N
          </text>
        </svg>
      </div>
    </button>
  )
}
